//! Error model for the ACP client, layered by protocol phase.
//!
//! The three layers are kept distinct on purpose (see specs/acp-protocol):
//! transport/seam failures, session state-machine failures, and protocol-level
//! JSON-RPC errors returned by the peer.

use agent_client_protocol_schema::v1::Error as ProtocolError;

/// Top-level error returned by the ACP client, separated by the phase that failed.
#[derive(Debug, thiserror::Error)]
pub enum AcpError {
    /// A failure at the transport seam (the pipe or its implementation).
    #[error("acp transport failure: {0}")]
    Transport(String),

    /// A failure in the session state machine (correlation, encoding, gating).
    #[error("acp session error: {0}")]
    Session(#[from] SessionError),

    /// A JSON-RPC error object returned by the peer, carried verbatim.
    #[error("acp protocol error {}: {}", .0.code, .0.message)]
    Protocol(ProtocolError),
}

impl AcpError {
    /// Returns the peer's protocol error when this is a protocol-level failure.
    ///
    /// Lets callers recover the original JSON-RPC `code`/`message` without matching
    /// on the enum shape directly.
    #[must_use]
    pub fn as_protocol(&self) -> Option<&ProtocolError> {
        match self {
            Self::Protocol(error) => Some(error),
            _ => None,
        }
    }
}

/// Failures produced by the session driver itself, before or instead of a peer response.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// A method was called whose capability the peer did not advertise.
    #[error("capability unavailable for method: {0}")]
    CapabilityUnavailable(String),

    /// The transport closed before the awaited response arrived.
    #[error("transport closed before response")]
    TransportClosed,

    /// A typed request could not be encoded into JSON params.
    #[error("failed to encode params: {0}")]
    Encode(String),

    /// A peer payload could not be decoded into the expected type.
    #[error("failed to decode payload: {0}")]
    Decode(String),
}

#[cfg(test)]
mod tests {
    use super::AcpError;
    use agent_client_protocol_schema::v1::Error as ProtocolError;

    #[test]
    fn protocol_variant_preserves_peer_code_and_message() {
        let peer = ProtocolError::new(-32601, "missing method");
        let error = AcpError::Protocol(peer);

        let protocol = error
            .as_protocol()
            .unwrap_or_else(|| panic!("expected a protocol-level error"));
        assert_eq!(protocol.code, (-32601).into());
        assert_eq!(protocol.message, "missing method");
    }
}
