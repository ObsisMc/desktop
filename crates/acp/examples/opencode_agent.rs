//! Real end-to-end example: drive a live `opencode acp` agent over stdio.
//!
//! This spawns `opencode acp` (a standards-compliant ACP server) as a child
//! process and implements [`AcpTransport`] over newline-delimited JSON-RPC on its
//! stdin/stdout — a slice of what a future `plugin-manager` duplex driver does.
//! It then runs the ACP handshake and opens a session against the real agent,
//! proving the crate interoperates with a third-party ACP implementation.
//!
//! Prompting is intentionally skipped: it would require a configured model and
//! credentials. `initialize` + `session/new` exercise the wire protocol without
//! hitting an LLM.
//!
//! Run with:
//!   cargo run -p ora-acp --example opencode_agent
//!
//! Requires `opencode` on PATH (tested with 1.17.x).

use std::process::Stdio;
use std::sync::Mutex;

use ora_acp::schema::{InitializeRequest, ProtocolError, ProtocolVersion, RequestId};
use ora_acp::{AcpConnection, AcpError, AcpFrame, AcpTransport};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex as AsyncMutex;

/// A stdio JSON-RPC transport bound to a child ACP agent process.
struct StdioTransport {
    stdin: AsyncMutex<ChildStdin>,
    stdout: AsyncMutex<Lines<BufReader<ChildStdout>>>,
    // Keeps the child alive for the transport's lifetime.
    _child: Mutex<tokio::process::Child>,
}

/// Builds a command targeting the `opencode` CLI in a cross-platform way.
///
/// On Windows the CLI is a `opencode.cmd` shim that `Command::new` cannot launch
/// directly, so it is invoked through `cmd /C`.
fn opencode_command() -> tokio::process::Command {
    #[cfg(windows)]
    {
        let mut command = tokio::process::Command::new("cmd");
        command.args(["/C", "opencode"]);
        command
    }
    #[cfg(not(windows))]
    {
        tokio::process::Command::new("opencode")
    }
}

impl StdioTransport {
    fn spawn() -> Result<Self, AcpError> {
        let mut child = opencode_command()
            .arg("acp")
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| AcpError::Transport(format!("failed to spawn opencode: {error}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AcpError::Transport("no child stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AcpError::Transport("no child stdout".to_string()))?;

        Ok(Self {
            stdin: AsyncMutex::new(stdin),
            stdout: AsyncMutex::new(BufReader::new(stdout).lines()),
            _child: Mutex::new(child),
        })
    }
}

/// Serializes an outbound frame to a JSON-RPC 2.0 wire object.
fn frame_to_wire(frame: &AcpFrame) -> Value {
    match frame {
        AcpFrame::Request { id, method, params } => json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        }),
        AcpFrame::Notification { method, params } => json!({
            "jsonrpc": "2.0", "method": method, "params": params
        }),
        AcpFrame::Success { id, result } => json!({
            "jsonrpc": "2.0", "id": id, "result": result
        }),
        AcpFrame::Failure { id, error } => json!({
            "jsonrpc": "2.0", "id": id, "error": error
        }),
    }
}

/// Parses one inbound JSON-RPC wire line into a frame.
fn wire_to_frame(line: &str) -> Result<AcpFrame, AcpError> {
    let value: Value = serde_json::from_str(line)
        .map_err(|error| AcpError::Transport(format!("bad json from agent: {error}")))?;

    let id = value.get("id").cloned();
    let method = value.get("method").and_then(Value::as_str);

    match (id, method) {
        // Response: id + result / error.
        (Some(id), None) => {
            let id: RequestId = serde_json::from_value(id)
                .map_err(|error| AcpError::Transport(format!("bad id: {error}")))?;
            if let Some(error) = value.get("error") {
                let error: ProtocolError = serde_json::from_value(error.clone())
                    .map_err(|error| AcpError::Transport(format!("bad error: {error}")))?;
                Ok(AcpFrame::Failure { id, error })
            } else {
                let result = value.get("result").cloned().unwrap_or(Value::Null);
                Ok(AcpFrame::Success { id, result })
            }
        }
        // Reverse request: id + method.
        (Some(id), Some(method)) => {
            let id: RequestId = serde_json::from_value(id)
                .map_err(|error| AcpError::Transport(format!("bad id: {error}")))?;
            Ok(AcpFrame::Request {
                id,
                method: method.to_string(),
                params: value.get("params").cloned().unwrap_or(Value::Null),
            })
        }
        // Notification: method, no id.
        (None, Some(method)) => Ok(AcpFrame::Notification {
            method: method.to_string(),
            params: value.get("params").cloned().unwrap_or(Value::Null),
        }),
        (None, None) => Err(AcpError::Transport("frame has neither id nor method".to_string())),
    }
}

impl AcpTransport for StdioTransport {
    async fn send(&self, frame: AcpFrame) -> Result<(), AcpError> {
        let line = frame_to_wire(&frame).to_string();
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|error| AcpError::Transport(format!("write failed: {error}")))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|error| AcpError::Transport(format!("write newline failed: {error}")))?;
        stdin
            .flush()
            .await
            .map_err(|error| AcpError::Transport(format!("flush failed: {error}")))
    }

    async fn recv(&self) -> Result<Option<AcpFrame>, AcpError> {
        let mut stdout = self.stdout.lock().await;
        loop {
            let next = stdout
                .next_line()
                .await
                .map_err(|error| AcpError::Transport(format!("read failed: {error}")))?;
            match next {
                None => return Ok(None),
                Some(line) if line.trim().is_empty() => continue,
                Some(line) => return Ok(Some(wire_to_frame(&line)?)),
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = StdioTransport::spawn()?;
    let connection = AcpConnection::new(transport).with_request_handler(|_request| {
        // The agent shouldn't need reverse calls during handshake/session setup.
        Err(ProtocolError::method_not_found())
    });

    // Guard the whole exchange with a timeout so a stuck agent can't hang the example.
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        println!("→ initialize");
        let init = connection
            .initialize(InitializeRequest::new(ProtocolVersion::LATEST))
            .await?;
        println!(
            "← initialized by opencode: protocol v{}, load_session={}, auth_methods={}",
            init.protocol_version.as_u16(),
            init.agent_capabilities.load_session,
            init.auth_methods.len()
        );

        println!("→ session/new");
        let session = connection
            .new_session(serde_json::from_value(json!({
                "cwd": std::env::current_dir()?.to_string_lossy(),
                "mcpServers": []
            }))?)
            .await?;
        println!("← session created by opencode: {}", session.session_id);

        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await;

    match outcome {
        Ok(Ok(())) => {
            println!("\nInteroperability confirmed against a live opencode ACP agent.");
            Ok(())
        }
        Ok(Err(error)) => Err(error),
        Err(_) => Err("timed out talking to opencode".into()),
    }
}
