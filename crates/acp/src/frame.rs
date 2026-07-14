//! The typed ACP frame moved across the transport seam.
//!
//! A frame is one JSON-RPC message (request, response, or notification) carrying
//! already-structured payloads (`RequestId`, `serde_json::Value` params/results,
//! the official protocol `Error`) rather than raw bytes. Outer envelope framing
//! and pipe I/O belong to the transport implementation, not to this type.

use agent_client_protocol_schema::v1::{Error as ProtocolError, RequestId};
use serde_json::Value;

/// One ACP-layer JSON-RPC message.
#[derive(Debug, Clone)]
pub enum AcpFrame {
    /// A request carrying a correlation id, method name, and params.
    Request {
        /// The ACP-layer id used to correlate the matching response.
        id: RequestId,
        /// The ACP method name (e.g. `session/prompt`).
        method: String,
        /// Method-specific params.
        params: Value,
    },
    /// A successful response correlated to a prior request id.
    Success {
        /// The id of the request this answers.
        id: RequestId,
        /// Method-specific result payload.
        result: Value,
    },
    /// A failed response correlated to a prior request id.
    Failure {
        /// The id of the request this answers.
        id: RequestId,
        /// The protocol-level JSON-RPC error.
        error: ProtocolError,
    },
    /// A notification, which carries no id and expects no response.
    Notification {
        /// The ACP method name (e.g. `session/update`).
        method: String,
        /// Method-specific params.
        params: Value,
    },
}
