//! Client-side support for the stable Agent Client Protocol (ACP) v1.
//!
//! Wire types come from the official [`agent_client_protocol_schema`] crate and
//! are re-exported here; this crate adds the transport seam ([`AcpTransport`]),
//! id correlation, streaming aggregation, capability gating, and inbound-request
//! routing ([`AcpConnection`]) that the app needs, without depending on any concrete
//! pipe. Only the stable protocol surface is exposed — no `unstable_*` features.

mod connection;
mod dispatch;
mod error;
mod frame;
mod transport;

pub use connection::{AcpConnection, PromptTurn};
pub use error::{AcpError, SessionError};
pub use frame::AcpFrame;
pub use transport::AcpTransport;
#[cfg(feature = "test-util")]
pub use transport::FakeAcpTransport;

/// Re-exported stable ACP v1 wire types, sourced from `agent-client-protocol-schema`.
///
/// These are re-exports, not redefinitions: the crate never declares its own copy
/// of a protocol message type.
pub mod schema {
    pub use agent_client_protocol_schema::ProtocolVersion;
    pub use agent_client_protocol_schema::v1::{
        AgentCapabilities, AgentNotification, AgentRequest, AgentResponse, CancelNotification,
        ClientCapabilities, ClientNotification, ClientRequest, ClientResponse, ContentBlock,
        Error as ProtocolError, ErrorCode, InitializeRequest, InitializeResponse,
        LoadSessionRequest, LoadSessionResponse, NewSessionRequest, NewSessionResponse,
        PromptRequest, PromptResponse, ReadTextFileRequest, ReadTextFileResponse, RequestId,
        RequestPermissionRequest, RequestPermissionResponse, SessionId, SessionNotification,
        SessionUpdate, StopReason, WriteTextFileRequest, WriteTextFileResponse,
    };
}
