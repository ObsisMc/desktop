//! Runnable end-to-end example driving `AcpConnection` against an in-process
//! mock agent.
//!
//! The `ScriptedAgent` here implements [`AcpTransport`] — the same seam a future
//! duplex driver in `plugin-manager` will implement. It reacts to the frames the
//! connection sends and enqueues canned agent responses, including streaming
//! `session/update` notifications and a reverse `fs/read_text_file` request, so
//! the whole bidirectional flow runs without any real process or pipe.
//!
//! Run with:
//!   cargo run -p ora-acp --example mock_agent

use std::collections::VecDeque;
use std::sync::Mutex;

use ora_acp::schema::{
    InitializeRequest, ProtocolVersion, ReadTextFileResponse, RequestId,
};
use ora_acp::{AcpConnection, AcpError, AcpFrame, AcpTransport};
use serde_json::{Value, json};

/// A minimal in-process agent: whatever the client sends, it scripts the reply.
#[derive(Default)]
struct ScriptedAgent {
    outbox: Mutex<VecDeque<AcpFrame>>,
}

impl ScriptedAgent {
    fn queue(&self, frame: AcpFrame) {
        if let Ok(mut outbox) = self.outbox.lock() {
            outbox.push_back(frame);
        }
    }

    /// Turns one inbound client request into the agent's scripted response frames.
    fn react(&self, id: RequestId, method: &str) {
        match method {
            "initialize" => self.queue(AcpFrame::Success {
                id,
                result: json!({
                    "protocolVersion": 1,
                    "agentCapabilities": { "loadSession": true }
                }),
            }),
            "session/new" => self.queue(AcpFrame::Success {
                id,
                result: json!({ "sessionId": "sess-1" }),
            }),
            "session/prompt" => {
                // Stream two thinking chunks...
                self.queue(update_frame("Let me check that file."));
                self.queue(update_frame(" Reading it now..."));
                // ...then reverse-call the client to read a file...
                self.queue(AcpFrame::Request {
                    id: RequestId::Str("rev-1".to_string()),
                    method: "fs/read_text_file".to_string(),
                    params: json!({ "sessionId": "sess-1", "path": "/etc/hostname" }),
                });
                // ...and finish the turn.
                self.queue(AcpFrame::Success {
                    id,
                    result: json!({ "stopReason": "end_turn" }),
                });
            }
            other => {
                eprintln!("mock agent: unhandled method {other}");
                self.queue(AcpFrame::Failure {
                    id,
                    error: ora_acp::schema::ProtocolError::method_not_found(),
                });
            }
        }
    }
}

fn update_frame(text: &str) -> AcpFrame {
    AcpFrame::Notification {
        method: "session/update".to_string(),
        params: json!({
            "sessionId": "sess-1",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": text }
            }
        }),
    }
}

impl AcpTransport for ScriptedAgent {
    async fn send(&self, frame: AcpFrame) -> Result<(), AcpError> {
        match frame {
            // A client request: script the agent's reply.
            AcpFrame::Request { id, method, .. } => self.react(id, &method),
            // The client answering our reverse request: just acknowledge it.
            AcpFrame::Success { id, .. } => println!("  <- client answered reverse request {id:?}"),
            AcpFrame::Failure { id, .. } => println!("  <- client rejected reverse request {id:?}"),
            // A notification (e.g. session/cancel): nothing to reply.
            AcpFrame::Notification { .. } => {}
        }
        Ok(())
    }

    async fn recv(&self) -> Result<Option<AcpFrame>, AcpError> {
        let mut outbox = self
            .outbox
            .lock()
            .map_err(|_| AcpError::Transport("outbox poisoned".to_string()))?;
        Ok(outbox.pop_front())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The reverse-request handler: the app answers agent -> client calls here.
    let connection =
        AcpConnection::new(ScriptedAgent::default()).with_request_handler(|request| {
            use ora_acp::schema::AgentRequest;
            match request {
                AgentRequest::ReadTextFileRequest(_) => {
                    let response = ReadTextFileResponse::new("mock-host\n");
                    Ok(serde_json::to_value(response).unwrap_or(Value::Null))
                }
                _ => Err(ora_acp::schema::ProtocolError::method_not_found()),
            }
        });

    // 1. Handshake.
    let init = connection
        .initialize(InitializeRequest::new(ProtocolVersion::LATEST))
        .await?;
    println!(
        "initialized: protocol v{}, load_session={}",
        init.protocol_version.as_u16(),
        init.agent_capabilities.load_session
    );

    // 2. Open a session.
    let session = connection
        .new_session(serde_json::from_value(json!({ "cwd": "/work", "mcpServers": [] }))?)
        .await?;
    println!("session created: {}", session.session_id);

    // 3. Prompt — drives streaming updates and a reverse file read.
    let prompt = serde_json::from_value(json!({
        "sessionId": session.session_id.to_string(),
        "prompt": [{ "type": "text", "text": "What is the hostname?" }]
    }))?;
    println!("prompting...");
    let turn = connection.prompt(prompt).await?;

    for (index, update) in turn.updates.iter().enumerate() {
        let text = serde_json::to_value(&update.update)
            .ok()
            .and_then(|value| value.get("content")?.get("text")?.as_str().map(str::to_string))
            .unwrap_or_default();
        println!("  update #{index}: {text}");
    }
    println!("turn finished: stop_reason={:?}", turn.response.stop_reason);

    Ok(())
}
