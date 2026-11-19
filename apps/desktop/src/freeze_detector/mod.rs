//! The dev-build freeze detector; compiled only with the `freeze-detector` feature.

mod heartbeat;
mod watchdog;

pub use heartbeat::start;
