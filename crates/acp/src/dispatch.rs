//! Decoding inbound frames onto the official routing enums.
//!
//! Reverse requests (agent -> client) are decoded into the schema crate's
//! `AgentRequest` enum; the session then routes them to a handler.

use agent_client_protocol_schema::v1::AgentRequest;
use serde_json::Value;

use crate::error::{AcpError, SessionError};

/// Decodes an inbound reverse-request `params` payload into the official [`AgentRequest`] enum.
///
/// # Errors
///
/// Returns [`AcpError::Session`] with [`SessionError::Decode`] when the payload does
/// not match any known agent -> client request variant.
pub fn decode_agent_request(params: &Value) -> Result<AgentRequest, AcpError> {
    serde_json::from_value(params.clone())
        .map_err(|error| SessionError::Decode(error.to_string()).into())
}

#[cfg(test)]
mod tests {
    use super::decode_agent_request;
    use agent_client_protocol_schema::v1::AgentRequest;
    use serde_json::json;

    #[test]
    fn decodes_request_permission_reverse_request() {
        let params = json!({
            "sessionId": "sess-1",
            "toolCall": { "toolCallId": "call-1" },
            "options": [
                { "optionId": "allow", "name": "Allow", "kind": "allow_once" }
            ]
        });

        let request = decode_agent_request(&params)
            .unwrap_or_else(|error| panic!("expected decode to succeed: {error}"));

        assert!(
            matches!(request, AgentRequest::RequestPermissionRequest(_)),
            "expected RequestPermissionRequest, got {request:?}"
        );
    }
}
