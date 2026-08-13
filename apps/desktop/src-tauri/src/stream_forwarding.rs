//! Bridges backend event streams onto Tauri IPC Channels.
//!
//! Every stream started at the command seam is deferred: the command returns before any event
//! exists, so the forwarding task owns the request's `RequestLifecycle` and is solely responsible
//! for emitting its single completion event.

use crate::workspace_files::workspace_file_backend_error;
use ora_backend::{BackendError, RequestLifecycle};
use ora_contracts::{WorkspaceFileChange, WorkspaceFileEventBatch};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;

/// Holds the cancellation token of every stream currently registered by the command seam.
type StreamRegistry = Arc<Mutex<HashMap<String, CancellationToken>>>;

/// Forwards ordered data/error/end frames and drops the backend stream on channel failure.
pub(crate) async fn forward_contract_stream<Event>(
    mut stream: ora_backend::SessionEventStream<Event>,
    cancellation: CancellationToken,
    stream_call_id: String,
    registry: StreamRegistry,
    on_event: Channel<serde_json::Value>,
    lifecycle: RequestLifecycle,
) where
    Event: Serialize + Send + 'static,
{
    loop {
        tokio::select! {
            () = cancellation.cancelled() => {
                lifecycle.complete_cancellation();
                break;
            },
            event = stream.recv() => {
                let is_terminal = matches!(&event, Some(Err(_)) | None);
                let frame = match event {
                    Some(Ok(data)) => serde_json::json!({ "type": "data", "data": data }),
                    Some(Err(error)) => {
                        lifecycle.complete_failure(&error);
                        serde_json::json!({
                            "type": "error",
                            "error": error.contract_error(lifecycle.request_id()),
                        })
                    },
                    None => {
                        lifecycle.complete_success();
                        serde_json::json!({ "type": "end" })
                    },
                };
                if on_event.send(frame).is_err() || is_terminal {
                    break;
                }
            }
        }
    }
    unregister(&registry, &stream_call_id);
}

/// Forwards debounced native workspace changes until the Desktop stream is cancelled.
pub(crate) async fn forward_workspace_watch(
    watcher: ora_fs::WorkspaceWatcher,
    cancellation: CancellationToken,
    stream_call_id: String,
    registry: StreamRegistry,
    on_event: Channel<serde_json::Value>,
    lifecycle: RequestLifecycle,
) {
    let watch_cancellation = cancellation.clone();
    let terminal_channel = on_event.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        while !watch_cancellation.is_cancelled() {
            match watcher.receive_batch(Duration::from_millis(100)) {
                Ok(Some(changes)) if !changes.is_empty() => {
                    let data = WorkspaceFileEventBatch {
                        changes: changes.into_iter().map(to_contract_change).collect(),
                    };
                    if on_event
                        .send(serde_json::json!({ "type": "data", "data": data }))
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Some(_)) | Ok(None) => {}
                Err(error) => return Err(error),
            }
        }
        Ok::<(), ora_fs::WorkspaceFileSystemError>(())
    })
    .await;

    if cancellation.is_cancelled() {
        lifecycle.complete_cancellation();
    } else {
        match result {
            Ok(Ok(())) => {
                lifecycle.complete_success();
                let _ = terminal_channel.send(serde_json::json!({ "type": "end" }));
            }
            Ok(Err(error)) => {
                let backend_error = workspace_file_backend_error(error);
                lifecycle.complete_failure(&backend_error);
                let _ = terminal_channel.send(serde_json::json!({
                    "type": "error",
                    "error": backend_error.contract_error(lifecycle.request_id()),
                }));
            }
            Err(error) => {
                let backend_error =
                    BackendError::internal("Desktop workspace watcher failed", error);
                lifecycle.complete_failure(&backend_error);
                let _ = terminal_channel.send(serde_json::json!({
                    "type": "error",
                    "error": backend_error.contract_error(lifecycle.request_id()),
                }));
            }
        }
    }
    unregister(&registry, &stream_call_id);
}

/// Releases the private stream id so a later call may reuse it.
///
/// A poisoned registry is ignored: the process is already failing, and leaving the entry behind
/// only blocks reuse of one opaque id rather than affecting the stream that just finished.
fn unregister(registry: &StreamRegistry, stream_call_id: &str) {
    if let Ok(mut registrations) = registry.lock() {
        registrations.remove(stream_call_id);
    }
}

/// Converts native watcher events to the shared file-change contract.
fn to_contract_change(change: ora_fs::WorkspaceChange) -> WorkspaceFileChange {
    match change.kind {
        ora_fs::WorkspaceChangeKind::Created => WorkspaceFileChange::Created { path: change.path },
        ora_fs::WorkspaceChangeKind::Modified => {
            WorkspaceFileChange::Modified { path: change.path }
        }
        ora_fs::WorkspaceChangeKind::Removed => WorkspaceFileChange::Removed { path: change.path },
        ora_fs::WorkspaceChangeKind::Renamed { from } => WorkspaceFileChange::Renamed {
            from,
            path: change.path,
        },
        ora_fs::WorkspaceChangeKind::RescanRequired => WorkspaceFileChange::RescanRequired,
    }
}
