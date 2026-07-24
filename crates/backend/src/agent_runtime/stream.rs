use crate::BackendError;
use ora_logging::ora_debug;
use tokio::sync::mpsc;

use super::RuntimeCommand;

/// Owns one finite business-event stream and cancels its operation when consumption stops early.
pub struct SessionEventStream<Event> {
    receiver: mpsc::Receiver<Result<Event, BackendError>>,
    commands: mpsc::UnboundedSender<RuntimeCommand>,
    operation_id: u64,
    completed: bool,
}

impl<Event> SessionEventStream<Event> {
    /// Builds a stream tied to one actor operation generation.
    pub(super) fn new(
        receiver: mpsc::Receiver<Result<Event, BackendError>>,
        commands: mpsc::UnboundedSender<RuntimeCommand>,
        operation_id: u64,
    ) -> Self {
        Self {
            receiver,
            commands,
            operation_id,
            completed: false,
        }
    }

    /// Receives the next ordered event or terminal error from the backend actor.
    pub async fn recv(&mut self) -> Option<Result<Event, BackendError>> {
        let event = self.receiver.recv().await;
        if matches!(&event, Some(Err(_)) | None) {
            self.completed = true;
        }
        event
    }
}

impl<Event> Drop for SessionEventStream<Event> {
    fn drop(&mut self) {
        if !self.completed {
            ora_debug!(
                operation_id = self.operation_id,
                "stream dropped, sending cancel"
            );
            let _ = self.commands.send(RuntimeCommand::Cancel {
                operation_id: self.operation_id,
            });
        }
    }
}
