//! The `AcpConnection`: a connection-level driver for the stable ACP client surface.
//!
//! One `AcpConnection` corresponds to one connection to an agent over a transport.
//! It drives both connection-level methods (`initialize`, `authenticate`, `logout`)
//! and the session-scoped methods that carry a `SessionId` in their payload
//! (`session/new`, `session/prompt`, `session/cancel`, ...) — a single connection
//! can host multiple sessions. It owns ACP-layer id correlation, streaming
//! `session/update` aggregation, capability gating, and inbound reverse-request
//! routing, and speaks only standard ACP methods — no invented wire semantics.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use agent_client_protocol_schema::v1::{
    AgentCapabilities, AgentRequest, CancelNotification, Error as ProtocolError, InitializeRequest,
    InitializeResponse, LoadSessionRequest, LoadSessionResponse, NewSessionRequest,
    NewSessionResponse, PromptRequest, PromptResponse, RequestId, SessionNotification,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::dispatch::decode_agent_request;
use crate::error::{AcpError, SessionError};
use crate::frame::AcpFrame;
use crate::transport::AcpTransport;

/// Method name constants for the outbound (client -> agent) calls this crate drives.
const METHOD_INITIALIZE: &str = "initialize";
const METHOD_SESSION_NEW: &str = "session/new";
const METHOD_SESSION_LOAD: &str = "session/load";
const METHOD_SESSION_PROMPT: &str = "session/prompt";
const METHOD_SESSION_CANCEL: &str = "session/cancel";
const METHOD_SESSION_UPDATE: &str = "session/update";

/// The result of one prompt turn: the ordered interim updates plus the final response.
#[derive(Debug)]
pub struct PromptTurn {
    /// `session/update` notifications observed during the turn, in arrival order.
    pub updates: Vec<SessionNotification>,
    /// The final `session/prompt` response, carrying the `stop_reason`.
    pub response: PromptResponse,
}

/// A handler for inbound agent -> client requests (`fs/*`, `session/request_permission`, `terminal/*`).
///
/// Returns the JSON-RPC `result` payload for the reverse request (built by the app,
/// e.g. `serde_json::to_value(ReadTextFileResponse::new(..))`), or a protocol error to
/// reject it. A `Value` is used rather than the official `ClientResponse` enum because
/// that enum is `#[non_exhaustive]` and cannot be constructed outside the schema crate.
type RequestHandler = Box<dyn Fn(AgentRequest) -> Result<Value, ProtocolError> + Send + Sync>;

/// Drives the client side of one ACP connection over a transport.
pub struct AcpConnection<T> {
    transport: T,
    next_id: AtomicI64,
    capabilities: Mutex<Option<AgentCapabilities>>,
    request_handler: Option<RequestHandler>,
}

impl<T: AcpTransport> AcpConnection<T> {
    /// Creates a session over the given transport.
    #[must_use]
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            next_id: AtomicI64::new(1),
            capabilities: Mutex::new(None),
            request_handler: None,
        }
    }

    /// Registers the handler invoked for inbound reverse requests.
    #[must_use]
    pub fn with_request_handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(AgentRequest) -> Result<Value, ProtocolError> + Send + Sync + 'static,
    {
        self.request_handler = Some(Box::new(handler));
        self
    }

    /// Returns a reference to the underlying transport.
    #[must_use]
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Returns the capabilities negotiated at `initialize`, if any.
    #[must_use]
    pub fn capabilities(&self) -> Option<AgentCapabilities> {
        self.capabilities
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    /// Negotiates protocol capabilities. Records the agent's advertised capabilities.
    ///
    /// # Errors
    ///
    /// Propagates transport, protocol, and decode failures.
    pub async fn initialize(
        &self,
        request: InitializeRequest,
    ) -> Result<InitializeResponse, AcpError> {
        let response: InitializeResponse = self.request(METHOD_INITIALIZE, &request).await?;
        if let Ok(mut guard) = self.capabilities.lock() {
            *guard = Some(response.agent_capabilities.clone());
        }
        Ok(response)
    }

    /// Creates a new session (`session/new`).
    ///
    /// # Errors
    ///
    /// Propagates transport, protocol, and decode failures.
    pub async fn new_session(
        &self,
        request: NewSessionRequest,
    ) -> Result<NewSessionResponse, AcpError> {
        self.request(METHOD_SESSION_NEW, &request).await
    }

    /// Loads an existing session (`session/load`). Gated on the agent's `load_session` capability.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::CapabilityUnavailable`] without sending a frame when the
    /// agent did not advertise `load_session`; otherwise propagates transport/protocol failures.
    pub async fn load_session(
        &self,
        request: LoadSessionRequest,
    ) -> Result<LoadSessionResponse, AcpError> {
        if !self.agent_supports_load_session() {
            return Err(
                SessionError::CapabilityUnavailable(METHOD_SESSION_LOAD.to_string()).into(),
            );
        }
        self.request(METHOD_SESSION_LOAD, &request).await
    }

    /// Sends a prompt and drives the turn, aggregating interim updates until the response.
    ///
    /// # Errors
    ///
    /// Propagates transport, protocol, and decode failures.
    pub async fn prompt(&self, request: PromptRequest) -> Result<PromptTurn, AcpError> {
        let id = self.next_request_id();
        let params = encode(&request)?;
        self.transport
            .send(AcpFrame::Request {
                id: id.clone(),
                method: METHOD_SESSION_PROMPT.to_string(),
                params,
            })
            .await?;

        let mut updates = Vec::new();
        let result = self.run_until_response(&id, &mut updates).await?;
        let response = decode(result)?;
        Ok(PromptTurn { updates, response })
    }

    /// Sends a `session/cancel` notification. No response is expected.
    ///
    /// # Errors
    ///
    /// Propagates transport and encoding failures.
    pub async fn cancel(&self, notification: CancelNotification) -> Result<(), AcpError> {
        let params = encode(&notification)?;
        self.transport
            .send(AcpFrame::Notification {
                method: METHOD_SESSION_CANCEL.to_string(),
                params,
            })
            .await
    }

    /// Issues a request/response call, discarding any interim notifications.
    async fn request<Req, Res>(&self, method: &str, request: &Req) -> Result<Res, AcpError>
    where
        Req: Serialize,
        Res: DeserializeOwned,
    {
        let id = self.next_request_id();
        let params = encode(request)?;
        self.transport
            .send(AcpFrame::Request {
                id: id.clone(),
                method: method.to_string(),
                params,
            })
            .await?;

        let mut updates = Vec::new();
        let result = self.run_until_response(&id, &mut updates).await?;
        decode(result)
    }

    /// Pumps inbound frames until the response with `id` arrives.
    ///
    /// Interim `session/update` notifications are aggregated into `updates`; inbound
    /// reverse requests are routed to the handler; responses with an unknown id are ignored.
    async fn run_until_response(
        &self,
        id: &RequestId,
        updates: &mut Vec<SessionNotification>,
    ) -> Result<Value, AcpError> {
        loop {
            match self.transport.recv().await? {
                None => return Err(SessionError::TransportClosed.into()),
                Some(AcpFrame::Notification { method, params }) => {
                    if method == METHOD_SESSION_UPDATE
                        && let Ok(notification) =
                            serde_json::from_value::<SessionNotification>(params)
                    {
                        updates.push(notification);
                    }
                }
                Some(AcpFrame::Request {
                    id: request_id,
                    method: _,
                    params,
                }) => {
                    self.handle_inbound_request(request_id, &params).await?;
                }
                Some(AcpFrame::Success {
                    id: response_id,
                    result,
                }) => {
                    if response_id == *id {
                        return Ok(result);
                    }
                }
                Some(AcpFrame::Failure {
                    id: response_id,
                    error,
                }) => {
                    if response_id == *id {
                        return Err(AcpError::Protocol(error));
                    }
                }
            }
        }
    }

    /// Routes one inbound reverse request to the handler and sends back the correlated response.
    async fn handle_inbound_request(&self, id: RequestId, params: &Value) -> Result<(), AcpError> {
        let response = match &self.request_handler {
            None => AcpFrame::Failure {
                id,
                error: ProtocolError::method_not_found(),
            },
            Some(handler) => match decode_agent_request(params) {
                Err(_) => AcpFrame::Failure {
                    id,
                    error: ProtocolError::invalid_params(),
                },
                Ok(request) => match handler(request) {
                    Ok(result) => AcpFrame::Success { id, result },
                    Err(error) => AcpFrame::Failure { id, error },
                },
            },
        };
        self.transport.send(response).await
    }

    fn next_request_id(&self) -> RequestId {
        RequestId::Number(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn agent_supports_load_session(&self) -> bool {
        self.capabilities
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|caps| caps.load_session))
            .unwrap_or(false)
    }
}

/// Encodes a typed request into JSON params.
fn encode<T: Serialize>(value: &T) -> Result<Value, AcpError> {
    serde_json::to_value(value).map_err(|error| SessionError::Encode(error.to_string()).into())
}

/// Decodes a JSON result into the expected typed response.
fn decode<T: DeserializeOwned>(value: Value) -> Result<T, AcpError> {
    serde_json::from_value(value).map_err(|error| SessionError::Decode(error.to_string()).into())
}
