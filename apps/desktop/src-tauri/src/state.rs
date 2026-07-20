use crate::config::DesktopConfigStore;
use ora_backend::Backend;

/// Holds the shared Backend and Desktop configuration store managed by Tauri.
#[derive(Clone)]
pub struct DesktopState {
    pub backend: Backend,
    pub config: DesktopConfigStore,
}

/// Retains process-scoped writer guards for the full Tauri application lifetime.
pub struct DesktopRuntimeGuard {
    pub _logging: ora_logging::LoggingGuard,
}
