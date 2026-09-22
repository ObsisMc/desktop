//! Generated Cloud-facing Controller contract. The `.proto` files under `proto/` are the single
//! source; regenerate with `task proto:generate` and never edit `src/gen` by hand.
#![allow(clippy::all, clippy::pedantic, clippy::nursery)]

/// `ora.controller.v1`: clone acceptance and lookup as the API Server consumes it. The prost
/// output pulls in the tonic client/server modules generated beside it.
pub mod v1 {
    include!("gen/ora/controller/v1/ora.controller.v1.rs");
}
