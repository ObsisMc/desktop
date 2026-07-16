//! Behavioural tests for the ACP client, driven entirely through `FakeAcpTransport`.

use ora_acp::schema::{
    AgentRequest, ErrorCode, InitializeRequest, LoadSessionRequest, NewSessionRequest,
    PromptRequest, ProtocolVersion, ReadTextFileResponse, RequestId, SessionUpdate, StopReason,
};
use ora_acp::{AcpConnection, AcpError, AcpFrame, AcpTransport, FakeAcpTransport, SessionError};
use serde_json::{Value, json};

fn from_json<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value.clone())
        .unwrap_or_else(|error| panic!("failed to build value from {value}: {error}"))
}

fn prompt_request() -> PromptRequest {
    from_json(json!({
        "sessionId": "s1",
        "prompt": [{ "type": "text", "text": "hello" }]
    }))
}

fn new_session_request() -> NewSessionRequest {
    from_json(json!({ "cwd": "/work", "mcpServers": [] }))
}

fn initialize_request() -> InitializeRequest {
    InitializeRequest::new(ProtocolVersion::LATEST)
}

fn load_session_request() -> LoadSessionRequest {
    from_json(json!({ "sessionId": "s1", "cwd": "/work", "mcpServers": [] }))
}

fn ok_response(id: i64, result: Value) -> AcpFrame {
    AcpFrame::Success {
        id: RequestId::Number(id),
        result,
    }
}

fn update_frame(text: &str) -> AcpFrame {
    AcpFrame::Notification {
        method: "session/update".to_string(),
        params: json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": text }
            }
        }),
    }
}

// --- acp-transport ---

#[tokio::test]
async fn fake_transport_orders_inbound_and_captures_outbound() {
    let transport = FakeAcpTransport::new();
    transport
        .push_inbound(update_frame("first"))
        .push_inbound(update_frame("second"));

    let first = transport
        .recv()
        .await
        .unwrap_or_else(|error| panic!("recv failed: {error}"));
    assert!(matches!(first, Some(AcpFrame::Notification { .. })));

    transport
        .send(AcpFrame::Notification {
            method: "out".to_string(),
            params: json!({}),
        })
        .await
        .unwrap_or_else(|error| panic!("send failed: {error}"));

    assert_eq!(transport.sent().len(), 1);
    // One inbound frame consumed, one still queued.
    assert!(transport.recv().await.is_ok());
}

// --- acp-protocol ---

#[test]
fn prompt_request_round_trips_through_official_type() {
    let input = json!({
        "sessionId": "s1",
        "prompt": [{ "type": "text", "text": "hello" }]
    });
    let typed: PromptRequest = from_json(input);
    let reparsed: PromptRequest = from_json(
        serde_json::to_value(&typed).unwrap_or_else(|error| panic!("serialize failed: {error}")),
    );
    assert_eq!(typed, reparsed);
}

// --- acp-session: 1:1 methods + id correlation ---

#[tokio::test]
async fn initialize_sends_handshake_and_records_capabilities() {
    let transport: FakeAcpTransport = FakeAcpTransport::new();
    transport.push_inbound(ok_response(
        1,
        json!({
            "protocolVersion": 1,
            "agentCapabilities": { "loadSession": true }
        }),
    ));

    let request = initialize_request();
    let expected_params =
        serde_json::to_value(&request).unwrap_or_else(|error| panic!("serialize: {error}"));

    let session = AcpConnection::new(transport);
    // No capabilities are known before the handshake completes.
    assert!(session.capabilities().is_none());

    let response = session
        .initialize(request)
        .await
        .unwrap_or_else(|error| panic!("initialize failed: {error}"));

    // The response is decoded from the official InitializeResponse type.
    assert!(response.agent_capabilities.load_session);

    // The outbound frame is the standard `initialize` request carrying the caller's params.
    let sent = session.transport().sent();
    match &sent[0] {
        AcpFrame::Request { method, params, id } => {
            assert_eq!(method, "initialize");
            assert_eq!(params, &expected_params);
            assert_eq!(*id, RequestId::Number(1));
        }
        other => panic!("expected an initialize request frame, got {other:?}"),
    }

    // Negotiated capabilities are recorded on the connection for later gating.
    let recorded = session
        .capabilities()
        .unwrap_or_else(|| panic!("expected capabilities to be recorded"));
    assert!(recorded.load_session);
}

#[tokio::test]
async fn prompt_sends_standard_method_with_unchanged_params() {
    let transport = FakeAcpTransport::new();
    transport.push_inbound(ok_response(1, json!({ "stopReason": "end_turn" })));

    let request = prompt_request();
    let expected_params =
        serde_json::to_value(&request).unwrap_or_else(|error| panic!("serialize: {error}"));

    let session = AcpConnection::new(transport);
    let turn = session
        .prompt(request)
        .await
        .unwrap_or_else(|error| panic!("prompt failed: {error}"));

    assert!(matches!(turn.response.stop_reason, StopReason::EndTurn));

    let sent = session.transport().sent();
    match &sent[0] {
        AcpFrame::Request { method, params, .. } => {
            assert_eq!(method, "session/prompt");
            assert_eq!(params, &expected_params);
        }
        other => panic!("expected a request frame, got {other:?}"),
    }
}

#[tokio::test]
async fn response_with_unknown_id_is_ignored() {
    let transport = FakeAcpTransport::new();
    transport.push_inbound(ok_response(999, json!({ "sessionId": "other" })));
    transport.push_inbound(ok_response(1, json!({ "sessionId": "s1" })));

    let session = AcpConnection::new(transport);
    let response = session
        .new_session(new_session_request())
        .await
        .unwrap_or_else(|error| panic!("new_session failed: {error}"));

    assert_eq!(response.session_id.to_string(), "s1");
}

// --- acp-session: streaming aggregation ---

#[tokio::test]
async fn updates_are_aggregated_in_order_before_response() {
    let transport = FakeAcpTransport::new();
    transport.push_inbound(update_frame("one"));
    transport.push_inbound(update_frame("two"));
    transport.push_inbound(ok_response(1, json!({ "stopReason": "end_turn" })));

    let session = AcpConnection::new(transport);
    let turn = session
        .prompt(prompt_request())
        .await
        .unwrap_or_else(|error| panic!("prompt failed: {error}"));

    assert_eq!(turn.updates.len(), 2);
    assert_eq!(chunk_text(&turn.updates[0].update), "one");
    assert_eq!(chunk_text(&turn.updates[1].update), "two");
    assert!(matches!(turn.response.stop_reason, StopReason::EndTurn));
}

fn chunk_text(update: &SessionUpdate) -> String {
    let value = serde_json::to_value(update).unwrap_or(Value::Null);
    value
        .get("content")
        .and_then(|content| content.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

// --- acp-session: capability gating ---

#[tokio::test]
async fn load_session_is_gated_without_capability_and_sends_no_frame() {
    let transport = FakeAcpTransport::new();
    let session = AcpConnection::new(transport);

    let error = match session.load_session(load_session_request()).await {
        Ok(_) => panic!("expected load_session to be gated"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        AcpError::Session(SessionError::CapabilityUnavailable(_))
    ));
    assert!(session.transport().sent().is_empty());
}

#[tokio::test]
async fn load_session_is_allowed_after_capability_negotiated() {
    let transport = FakeAcpTransport::new();
    transport.push_inbound(ok_response(
        1,
        json!({ "protocolVersion": 1, "agentCapabilities": { "loadSession": true } }),
    ));
    transport.push_inbound(ok_response(2, json!({})));

    let session = AcpConnection::new(transport);
    session
        .initialize(initialize_request())
        .await
        .unwrap_or_else(|error| panic!("initialize failed: {error}"));
    session
        .load_session(load_session_request())
        .await
        .unwrap_or_else(|error| panic!("load_session failed: {error}"));

    assert_eq!(session.transport().sent().len(), 2);
}

// --- acp-session: inbound request routing + e2e ---

#[tokio::test]
async fn end_to_end_prompt_with_updates_and_reverse_request() {
    let transport = FakeAcpTransport::new();
    transport.push_inbound(update_frame("thinking"));
    transport.push_inbound(update_frame("still thinking"));
    transport.push_inbound(AcpFrame::Request {
        id: RequestId::Str("r1".to_string()),
        method: "fs/read_text_file".to_string(),
        params: json!({ "sessionId": "s1", "path": "/f" }),
    });
    transport.push_inbound(ok_response(1, json!({ "stopReason": "end_turn" })));

    let session = AcpConnection::new(transport).with_request_handler(|request| match request {
        AgentRequest::ReadTextFileRequest(_) => Ok(serde_json::to_value(
            ReadTextFileResponse::new("file contents"),
        )
        .unwrap_or(Value::Null)),
        _ => Err(ora_acp::schema::ProtocolError::method_not_found()),
    });

    let turn = session
        .prompt(prompt_request())
        .await
        .unwrap_or_else(|error| panic!("prompt failed: {error}"));

    assert_eq!(turn.updates.len(), 2);
    assert!(matches!(turn.response.stop_reason, StopReason::EndTurn));

    let sent = session.transport().sent();
    // sent[0] is the prompt; sent[1] is the response correlated to the reverse request.
    match &sent[1] {
        AcpFrame::Success { id, .. } => assert_eq!(*id, RequestId::Str("r1".to_string())),
        other => panic!("expected a success response to r1, got {other:?}"),
    }
}

#[tokio::test]
async fn unhandled_reverse_request_gets_protocol_error() {
    let transport = FakeAcpTransport::new();
    transport.push_inbound(AcpFrame::Request {
        id: RequestId::Str("r1".to_string()),
        method: "fs/read_text_file".to_string(),
        params: json!({ "sessionId": "s1", "path": "/f" }),
    });
    transport.push_inbound(ok_response(1, json!({ "stopReason": "end_turn" })));

    // No handler registered.
    let session = AcpConnection::new(transport);
    session
        .prompt(prompt_request())
        .await
        .unwrap_or_else(|error| panic!("prompt failed: {error}"));

    let sent = session.transport().sent();
    match &sent[1] {
        AcpFrame::Failure { id, error } => {
            assert_eq!(*id, RequestId::Str("r1".to_string()));
            assert_eq!(error.code, ErrorCode::MethodNotFound);
        }
        other => panic!("expected a failure response, got {other:?}"),
    }
}
