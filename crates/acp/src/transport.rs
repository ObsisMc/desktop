//! The transport seam that decouples ACP semantics from the concrete pipe.
//!
//! `AcpTransport` moves typed [`AcpFrame`]s in both directions. The real
//! implementation (a future duplex driver in `plugin-manager`, or a WebSocket /
//! gRPC bridge) lives outside this crate; here we only define the seam and a
//! test double.

use std::future::Future;

use crate::error::AcpError;
use crate::frame::AcpFrame;

/// A bidirectional channel for ACP frames.
///
/// Outbound frames flow client -> agent via [`send`](AcpTransport::send); inbound
/// frames (agent -> client responses, notifications, and reverse requests such as
/// `fs/read_text_file`) are pulled via [`recv`](AcpTransport::recv).
pub trait AcpTransport {
    /// Sends one frame toward the agent.
    fn send(&self, frame: AcpFrame) -> impl Future<Output = Result<(), AcpError>> + Send;

    /// Receives the next inbound frame, or `None` once the transport is closed.
    fn recv(&self) -> impl Future<Output = Result<Option<AcpFrame>, AcpError>> + Send;
}

/// An in-memory [`AcpTransport`] for tests: scripts inbound frames and captures outbound ones.
///
/// Only compiled when the `test-util` feature is enabled, so it stays out of
/// production builds.
#[cfg(feature = "test-util")]
#[derive(Default)]
pub struct FakeAcpTransport {
    inbound: std::sync::Mutex<std::collections::VecDeque<AcpFrame>>,
    outbound: std::sync::Mutex<Vec<AcpFrame>>,
}

#[cfg(feature = "test-util")]
impl FakeAcpTransport {
    /// Creates an empty fake transport.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueues one scripted inbound frame; `recv` returns them in this order.
    pub fn push_inbound(&self, frame: AcpFrame) -> &Self {
        if let Ok(mut inbound) = self.inbound.lock() {
            inbound.push_back(frame);
        }
        self
    }

    /// Returns a snapshot of the frames the session has sent so far.
    #[must_use]
    pub fn sent(&self) -> Vec<AcpFrame> {
        self.outbound
            .lock()
            .map(|outbound| outbound.clone())
            .unwrap_or_default()
    }
}

#[cfg(feature = "test-util")]
impl AcpTransport for FakeAcpTransport {
    async fn send(&self, frame: AcpFrame) -> Result<(), AcpError> {
        let mut outbound = self
            .outbound
            .lock()
            .map_err(|_| AcpError::Transport("outbound lock poisoned".to_string()))?;
        outbound.push(frame);
        Ok(())
    }

    async fn recv(&self) -> Result<Option<AcpFrame>, AcpError> {
        let mut inbound = self
            .inbound
            .lock()
            .map_err(|_| AcpError::Transport("inbound lock poisoned".to_string()))?;
        Ok(inbound.pop_front())
    }
}
