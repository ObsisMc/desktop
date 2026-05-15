use tracing_subscriber::Registry;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::prelude::*;

/// Runs one test body under a TRACE-enabled subscriber so shared callsites stay observable.
pub fn with_trace_logging<R>(action: impl FnOnce() -> R) -> R {
    let subscriber = tracing_subscriber::registry().with(LevelFilter::TRACE);

    tracing::subscriber::with_default(subscriber, action)
}

/// Runs one test body under a TRACE-enabled subscriber that records structured logging events.
pub fn with_recorded_trace_logging<L, R>(layer: L, action: impl FnOnce() -> R) -> R
where
    L: Layer<Registry> + Send + Sync + 'static,
{
    let subscriber = tracing_subscriber::registry()
        .with(layer)
        .with(LevelFilter::TRACE);

    tracing::subscriber::with_default(subscriber, action)
}
