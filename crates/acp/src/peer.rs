use futures_util::StreamExt;
use ora_contracts::acp::error::Error as RpcError;
use ora_contracts::acp::literals::CLIENT_METHOD_NAMES;
use ora_contracts::acp::notification::SessionNotification;
use ora_contracts::acp::permission::RequestPermissionRequest;
use ora_contracts::acp::rpc::RequestId;
use ora_logging::ora_trace;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::codec::{FramedRead, LinesCodec, LinesCodecError};

const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const DATA_QUEUE_CAPACITY: usize = 256;

type PendingResponse = Result<Value, RpcError>;

/// Reports framing, correlation, serialization, and process-pipe failures.
#[derive(Debug, Error)]
pub enum AcpError {
    #[error("ACP stream ended before the pending operation completed")]
    StreamClosed,
    #[error("ACP transport I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("ACP frame is invalid: {0}")]
    InvalidFrame(String),
    #[error("ACP frame exceeds 8 MiB")]
    FrameTooLarge,
    #[error("ACP returned an operation error: {0}")]
    RequestFailed(String),
    #[error("ACP response payload is invalid: {0}")]
    InvalidResponse(String),
    #[error("ACP update queue overflowed")]
    DataQueueOverflow,
}

/// Carries one permission request together with its JSON-RPC correlation id.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRequest {
    pub request_id: RequestId,
    pub request: RequestPermissionRequest,
}

/// Carries low-volume control messages separately from bounded session updates.
#[derive(Debug)]
pub enum AcpControl {
    PermissionRequest(PermissionRequest),
    Fatal(AcpError),
}

/// Sends correlated ACP requests and protocol responses over one serialized writer.
pub struct AcpClient<Writer> {
    writer: Arc<Mutex<Writer>>,
    pending: Arc<Mutex<HashMap<RequestId, oneshot::Sender<PendingResponse>>>>,
    next_request_id: Arc<AtomicI64>,
}

impl<Writer> Clone for AcpClient<Writer> {
    fn clone(&self) -> Self {
        Self {
            writer: self.writer.clone(),
            pending: self.pending.clone(),
            next_request_id: self.next_request_id.clone(),
        }
    }
}

impl<Writer> AcpClient<Writer>
where
    Writer: AsyncWrite + Unpin + Send + 'static,
{
    /// Sends a typed request and waits for the independently-read correlated response.
    pub async fn request<Request, Response>(
        &self,
        method: &str,
        params: &Request,
    ) -> Result<Response, AcpError>
    where
        Request: Serialize,
        Response: DeserializeOwned,
    {
        let request_id = RequestId::Number(self.next_request_id.fetch_add(1, Ordering::Relaxed));
        let (response_sender, response_receiver) = oneshot::channel();
        self.pending
            .lock()
            .await
            .insert(request_id.clone(), response_sender);
        let frame = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        });
        if let Err(error) = self.write_frame(&frame).await {
            self.pending.lock().await.remove(&request_id);
            return Err(error);
        }
        let response = response_receiver
            .await
            .map_err(|_| AcpError::StreamClosed)?;
        match response {
            Ok(result) => serde_json::from_value(result)
                .map_err(|error| AcpError::InvalidResponse(error.to_string())),
            Err(error) => Err(AcpError::RequestFailed(error.message)),
        }
    }

    /// Sends a notification that intentionally has no JSON-RPC response.
    pub async fn notify<Params>(&self, method: &str, params: &Params) -> Result<(), AcpError>
    where
        Params: Serialize,
    {
        self.write_frame(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
        .await
    }

    /// Responds to an agent-originated permission request with a typed result payload.
    pub async fn respond<ResultBody>(
        &self,
        request_id: &RequestId,
        result: &ResultBody,
    ) -> Result<(), AcpError>
    where
        ResultBody: Serialize,
    {
        self.write_frame(&json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": result,
        }))
        .await
    }

    /// Serializes one complete NDJSON frame so concurrent control writes cannot interleave.
    async fn write_frame(&self, value: &Value) -> Result<(), AcpError> {
        write_frame(&self.writer, value).await
    }
}

/// Owns the independent data and control receivers for one ACP connection.
pub struct AcpPeer<Writer> {
    pub client: AcpClient<Writer>,
    updates: mpsc::Receiver<SessionNotification>,
    control: mpsc::UnboundedReceiver<AcpControl>,
}

impl<Writer> AcpPeer<Writer>
where
    Writer: AsyncWrite + Unpin + Send + 'static,
{
    /// Starts the reader task and returns a peer whose update queue is bounded and lossless-or-fatal.
    pub fn spawn<Reader>(reader: Reader, writer: Writer) -> Self
    where
        Reader: AsyncRead + Unpin + Send + 'static,
    {
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let writer = Arc::new(Mutex::new(writer));
        let (updates_sender, updates) = mpsc::channel(DATA_QUEUE_CAPACITY);
        let (control_sender, control) = mpsc::unbounded_channel();
        tokio::spawn(read_frames(
            reader,
            writer.clone(),
            pending.clone(),
            updates_sender,
            control_sender,
        ));
        Self {
            client: AcpClient {
                writer,
                pending,
                next_request_id: Arc::new(AtomicI64::new(1)),
            },
            updates,
            control,
        }
    }

    /// Receives the next high-volume session update from the bounded data path.
    pub async fn next_update(&mut self) -> Option<SessionNotification> {
        self.updates.recv().await
    }

    /// Receives the next permission or fatal condition from the unbounded control path.
    pub async fn next_control(&mut self) -> Option<AcpControl> {
        self.control.recv().await
    }

    /// Splits the peer so callers can select over independent data and control receivers.
    pub fn into_parts(
        self,
    ) -> (
        AcpClient<Writer>,
        mpsc::Receiver<SessionNotification>,
        mpsc::UnboundedReceiver<AcpControl>,
    ) {
        (self.client, self.updates, self.control)
    }
}

/// Parses agent frames and routes responses, updates, and requests without blocking on consumers.
async fn read_frames<Reader, Writer>(
    reader: Reader,
    writer: Arc<Mutex<Writer>>,
    pending: Arc<Mutex<HashMap<RequestId, oneshot::Sender<PendingResponse>>>>,
    updates: mpsc::Sender<SessionNotification>,
    control: mpsc::UnboundedSender<AcpControl>,
) where
    Reader: AsyncRead + Unpin,
    Writer: AsyncWrite + Unpin,
{
    let mut lines = FramedRead::new(reader, LinesCodec::new_with_max_length(MAX_FRAME_BYTES));
    while let Some(line) = lines.next().await {
        let value = match line {
            Ok(line) => match serde_json::from_str::<Value>(&line) {
                Ok(value) => value,
                Err(error) => {
                    let _ =
                        control.send(AcpControl::Fatal(AcpError::InvalidFrame(error.to_string())));
                    pending.lock().await.clear();
                    return;
                }
            },
            Err(LinesCodecError::MaxLineLengthExceeded) => {
                let _ = control.send(AcpControl::Fatal(AcpError::FrameTooLarge));
                pending.lock().await.clear();
                return;
            }
            Err(LinesCodecError::Io(error)) => {
                let _ = control.send(AcpControl::Fatal(AcpError::Io(error)));
                pending.lock().await.clear();
                return;
            }
        };
        #[cfg(debug_assertions)]
        {
            let (msg, jsonrpc_method, session_id) = trace_frame_summary(&value, "recv");
            ora_trace!(
                direction = "recv",
                jsonrpc_method = %jsonrpc_method,
                session_id = %session_id,
                frame = %value,
                "{}", msg,
            );
        }
        if let Err(error) = route_frame(value, &writer, &pending, &updates, &control).await {
            let _ = control.send(AcpControl::Fatal(error));
            pending.lock().await.clear();
            return;
        }
    }
    let _ = control.send(AcpControl::Fatal(AcpError::StreamClosed));
    // Retaining these senders would turn a known EOF into unrelated outer timeouts.
    pending.lock().await.clear();
}

/// Routes one validated JSON-RPC object and makes ambiguous shapes fatal.
async fn route_frame<Writer>(
    value: Value,
    writer: &Mutex<Writer>,
    pending: &Mutex<HashMap<RequestId, oneshot::Sender<PendingResponse>>>,
    updates: &mpsc::Sender<SessionNotification>,
    control: &mpsc::UnboundedSender<AcpControl>,
) -> Result<(), AcpError>
where
    Writer: AsyncWrite + Unpin,
{
    let object = value.as_object().ok_or_else(|| {
        AcpError::InvalidFrame("batch and non-object frames are unsupported".to_string())
    })?;
    if object.get("jsonrpc") != Some(&Value::String("2.0".to_string())) {
        return Err(AcpError::InvalidFrame("jsonrpc must equal 2.0".to_string()));
    }
    let method = object.get("method").and_then(Value::as_str);
    let id = object
        .get("id")
        .cloned()
        .map(serde_json::from_value::<RequestId>)
        .transpose()
        .map_err(|error| AcpError::InvalidFrame(error.to_string()))?;

    match (method, id) {
        (Some(method), Some(request_id))
            if method == CLIENT_METHOD_NAMES.session_request_permission =>
        {
            let request =
                serde_json::from_value(object.get("params").cloned().unwrap_or(Value::Null))
                    .map_err(|error| AcpError::InvalidFrame(error.to_string()))?;
            control
                .send(AcpControl::PermissionRequest(PermissionRequest {
                    request_id,
                    request,
                }))
                .map_err(|_| AcpError::StreamClosed)
        }
        (Some(method), Some(request_id)) => {
            // ACP can grow new client methods independently. JSON-RPC requires a correlated
            // method-not-found response, while terminating here would make extensions fatal.
            let response = json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {
                    "code": -32601,
                    "message": format!("method not found: {method}"),
                },
            });
            write_frame(writer, &response).await
        }
        (Some(method), None) if method == CLIENT_METHOD_NAMES.session_update => {
            let notification =
                serde_json::from_value(object.get("params").cloned().unwrap_or(Value::Null))
                    .map_err(|error| AcpError::InvalidFrame(error.to_string()))?;
            updates
                .try_send(notification)
                .map_err(|_| AcpError::DataQueueOverflow)
        }
        (Some(_), None) => Ok(()),
        (None, Some(request_id)) => {
            let response = if let Some(result) = object.get("result") {
                Ok(result.clone())
            } else if let Some(error) = object.get("error") {
                Err(serde_json::from_value(error.clone())
                    .map_err(|parse_error| AcpError::InvalidFrame(parse_error.to_string()))?)
            } else {
                return Err(AcpError::InvalidFrame(
                    "response has neither result nor error".to_string(),
                ));
            };
            let sender = pending.lock().await.remove(&request_id).ok_or_else(|| {
                AcpError::InvalidFrame(format!("unmatched response id {request_id}"))
            })?;
            let _ = sender.send(response);
            Ok(())
        }
        (None, None) => Err(AcpError::InvalidFrame(
            "frame has neither method nor id".to_string(),
        )),
    }
}

/// Extracts summary fields from a JSON-RPC frame for trace-level correlation without re-parsing.
#[cfg(debug_assertions)]
fn trace_frame_summary(value: &Value, direction: &str) -> (String, String, String) {
    let jsonrpc_method = value.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let session_id = value
        .get("params")
        .and_then(|p| p.get("sessionId"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let is_response = value.get("result").is_some();
    let is_error = value.get("error").is_some();

    let message = if !jsonrpc_method.is_empty() {
        format!("{direction} {jsonrpc_method}")
    } else if is_response {
        format!("{direction} response")
    } else if is_error {
        format!("{direction} error response")
    } else {
        format!("{direction} frame")
    };

    (message, jsonrpc_method.to_string(), session_id.to_string())
}

/// Writes a reader-originated protocol response through the connection's serialized sink.
async fn write_frame<Writer>(writer: &Mutex<Writer>, value: &Value) -> Result<(), AcpError>
where
    Writer: AsyncWrite + Unpin,
{
    #[cfg(debug_assertions)]
    {
        let (msg, jsonrpc_method, session_id) = trace_frame_summary(value, "send");
        ora_trace!(
            direction = "send",
            jsonrpc_method = %jsonrpc_method,
            session_id = %session_id,
            frame = %value,
            "{}", msg,
        );
    }
    let mut bytes =
        serde_json::to_vec(value).map_err(|error| AcpError::InvalidFrame(error.to_string()))?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(AcpError::FrameTooLarge);
    }
    bytes.push(b'\n');
    let mut writer = writer.lock().await;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AcpError, AcpPeer};
    use pretty_assertions::assert_eq;
    use serde_json::{Value, json};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, duplex, split};

    /// Verifies extension requests receive method-not-found without closing request correlation.
    #[tokio::test]
    async fn rejects_unknown_agent_request_and_continues_reading() {
        let (ora_stream, agent_stream) = duplex(16 * 1024);
        let (ora_reader, ora_writer) = split(ora_stream);
        let (agent_reader, mut agent_writer) = split(agent_stream);
        let mut agent_reader = BufReader::new(agent_reader);
        let peer = AcpPeer::spawn(ora_reader, ora_writer);
        let client = peer.client.clone();
        let request = tokio::spawn(async move {
            client
                .request::<_, Value>("initialize", &json!({ "protocolVersion": 1 }))
                .await
        });

        let mut outbound = String::new();
        agent_reader
            .read_line(&mut outbound)
            .await
            .expect("read Ora request");
        let outbound: Value = serde_json::from_str(outbound.trim()).expect("parse Ora request");
        let request_id = outbound["id"].clone();
        agent_writer
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"ext/future\",\"params\":{}}\n")
            .await
            .expect("write extension request");

        let mut rejection = String::new();
        agent_reader
            .read_line(&mut rejection)
            .await
            .expect("read method-not-found response");
        assert_eq!(
            serde_json::from_str::<Value>(rejection.trim()).expect("parse rejection"),
            json!({
                "jsonrpc": "2.0",
                "id": 99,
                "error": {
                    "code": -32601,
                    "message": "method not found: ext/future",
                },
            })
        );

        let response = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": { "accepted": true },
        });
        agent_writer
            .write_all(format!("{response}\n").as_bytes())
            .await
            .expect("write correlated response");
        assert_eq!(
            request
                .await
                .expect("join request")
                .expect("complete request"),
            json!({ "accepted": true })
        );
    }

    /// Verifies EOF wakes correlated requests instead of leaving them to an outer timeout.
    #[tokio::test]
    async fn closes_pending_requests_when_agent_stdout_ends() {
        let (ora_stream, agent_stream) = duplex(16 * 1024);
        let (ora_reader, ora_writer) = split(ora_stream);
        let (agent_reader, mut agent_writer) = split(agent_stream);
        let mut agent_reader = BufReader::new(agent_reader);
        let peer = AcpPeer::spawn(ora_reader, ora_writer);
        let client = peer.client.clone();
        let request = tokio::spawn(async move {
            client
                .request::<_, Value>("initialize", &json!({ "protocolVersion": 1 }))
                .await
        });
        let mut outbound = String::new();
        agent_reader
            .read_line(&mut outbound)
            .await
            .expect("read Ora request");

        agent_writer.shutdown().await.expect("close agent writer");
        drop(agent_reader);
        drop(agent_writer);

        assert!(matches!(
            request.await.expect("join request"),
            Err(AcpError::StreamClosed)
        ));
    }
}
