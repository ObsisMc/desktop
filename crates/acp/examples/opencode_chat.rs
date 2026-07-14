//! Have a real conversation with a live `opencode acp` agent.
//!
//! Spawns `opencode acp`, runs the ACP handshake, opens a session, sends one
//! prompt, and prints the agent's streamed reply — a full round trip through the
//! `AcpConnection` against a third-party ACP agent backed by a real LLM.
//!
//! A throwaway `opencode.json` pins a free model in a temp working directory, so
//! this neither touches your global opencode config nor costs anything.
//!
//! Run with:
//!   cargo run -p ora-acp --example opencode_chat -- "your message here"
//!
//! Requires `opencode` on PATH with a configured provider (e.g. OpenRouter).

use std::process::Stdio;
use std::sync::Mutex;

use ora_acp::schema::{InitializeRequest, ProtocolError, ProtocolVersion, RequestId};
use ora_acp::{AcpConnection, AcpError, AcpFrame, AcpTransport};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex as AsyncMutex;

const FREE_MODEL: &str = "opencode/deepseek-v4-flash-free";

struct StdioTransport {
    stdin: AsyncMutex<ChildStdin>,
    stdout: AsyncMutex<Lines<BufReader<ChildStdout>>>,
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
    fn spawn(cwd: &std::path::Path) -> Result<Self, AcpError> {
        let mut child = opencode_command()
            .arg("acp")
            .arg("--cwd")
            .arg(cwd)
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
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

fn frame_to_wire(frame: &AcpFrame) -> Value {
    match frame {
        AcpFrame::Request { id, method, params } => {
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
        }
        AcpFrame::Notification { method, params } => {
            json!({ "jsonrpc": "2.0", "method": method, "params": params })
        }
        AcpFrame::Success { id, result } => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        AcpFrame::Failure { id, error } => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    }
}

fn wire_to_frame(line: &str) -> Result<AcpFrame, AcpError> {
    let value: Value = serde_json::from_str(line)
        .map_err(|error| AcpError::Transport(format!("bad json from agent: {error}")))?;
    let id = value.get("id").cloned();
    let method = value.get("method").and_then(Value::as_str);

    match (id, method) {
        (Some(id), None) => {
            let id: RequestId = serde_json::from_value(id)
                .map_err(|error| AcpError::Transport(format!("bad id: {error}")))?;
            if let Some(error) = value.get("error") {
                let error: ProtocolError = serde_json::from_value(error.clone())
                    .map_err(|error| AcpError::Transport(format!("bad error: {error}")))?;
                Ok(AcpFrame::Failure { id, error })
            } else {
                Ok(AcpFrame::Success {
                    id,
                    result: value.get("result").cloned().unwrap_or(Value::Null),
                })
            }
        }
        (Some(id), Some(method)) => {
            let id: RequestId = serde_json::from_value(id)
                .map_err(|error| AcpError::Transport(format!("bad id: {error}")))?;
            Ok(AcpFrame::Request {
                id,
                method: method.to_string(),
                params: value.get("params").cloned().unwrap_or(Value::Null),
            })
        }
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
            .and(stdin.write_all(b"\n").await)
            .map_err(|error| AcpError::Transport(format!("write failed: {error}")))?;
        stdin
            .flush()
            .await
            .map_err(|error| AcpError::Transport(format!("flush failed: {error}")))
    }

    async fn recv(&self) -> Result<Option<AcpFrame>, AcpError> {
        let mut stdout = self.stdout.lock().await;
        loop {
            match stdout
                .next_line()
                .await
                .map_err(|error| AcpError::Transport(format!("read failed: {error}")))?
            {
                None => return Ok(None),
                Some(line) if line.trim().is_empty() => continue,
                Some(line) => return Ok(Some(wire_to_frame(&line)?)),
            }
        }
    }
}

/// Pulls the text out of an `agent_message_chunk` update; empty for other kinds.
fn chunk_text(update: &Value) -> String {
    if update.get("sessionUpdate").and_then(Value::as_str) != Some("agent_message_chunk") {
        return String::new();
    }
    update
        .get("content")
        .and_then(|content| content.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let message = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Reply with a short one-sentence greeting.".to_string());

    // Temp working dir pinning a free model, so we neither touch global config nor pay.
    let workdir = std::env::temp_dir().join(format!("ora-acp-chat-{}", std::process::id()));
    std::fs::create_dir_all(&workdir)?;
    std::fs::write(
        workdir.join("opencode.json"),
        json!({ "$schema": "https://opencode.ai/config.json", "model": FREE_MODEL }).to_string(),
    )?;

    let transport = StdioTransport::spawn(&workdir)?;
    // Auto-reject reverse calls; a plain chat prompt shouldn't need tools.
    let connection = AcpConnection::new(transport)
        .with_request_handler(|_request| Err(ProtocolError::method_not_found()));

    let reply = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        connection
            .initialize(InitializeRequest::new(ProtocolVersion::LATEST))
            .await?;
        let session = connection
            .new_session(serde_json::from_value(json!({
                "cwd": workdir.to_string_lossy(),
                "mcpServers": []
            }))?)
            .await?;

        println!("you    > {message}");
        let prompt = serde_json::from_value(json!({
            "sessionId": session.session_id.to_string(),
            "prompt": [{ "type": "text", "text": message }]
        }))?;
        let turn = connection.prompt(prompt).await?;

        let text: String = turn
            .updates
            .iter()
            .map(|notification| {
                serde_json::to_value(&notification.update)
                    .map(|value| chunk_text(&value))
                    .unwrap_or_default()
            })
            .collect();
        Ok::<_, Box<dyn std::error::Error>>((text, turn.response.stop_reason))
    })
    .await;

    // Best-effort cleanup of the temp workdir.
    let _ = std::fs::remove_dir_all(&workdir);

    match reply {
        Ok(Ok((text, stop_reason))) => {
            println!("opencode> {}", text.trim());
            println!("\n[stop_reason: {stop_reason:?}]");
            Ok(())
        }
        Ok(Err(error)) => Err(error),
        Err(_) => Err("timed out waiting for opencode".into()),
    }
}
