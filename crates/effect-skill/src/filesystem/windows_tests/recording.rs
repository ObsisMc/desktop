use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing_subscriber::{Layer, Registry, layer::Context};

/// Records real adapter events under a test-local subscriber.
#[derive(Clone, Default)]
pub(super) struct Recording(Arc<Mutex<Vec<BTreeMap<String, String>>>>);

impl Recording {
    /// Returns an isolated copy after the tested synchronous operation has settled.
    pub(super) fn events(&self) -> Vec<BTreeMap<String, String>> {
        self.0.lock().unwrap().clone()
    }
}

impl Layer<Registry> for Recording {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, Registry>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
}

#[derive(Default)]
struct Fields(BTreeMap<String, String>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}
