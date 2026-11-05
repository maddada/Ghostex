//! Dictate's engines and clean-up: the local models (`catalog`, `models`, `engine`, `whisper`),
//! the per-computer settings (`settings`, `dictate`), the steps after transcription (`cleanup`)
//! and their API (`http`).

mod catalog;
pub(super) mod cleanup;
pub(super) mod dictate;
mod engine;
mod http;
mod models;
pub(super) mod settings;
mod whisper;

#[cfg(test)]
mod cleanup_tests;
#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod models_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod whisper_tests;

pub(super) use engine::{spec as model_spec, transcribe as transcribe_locally, Hints};
pub(crate) use http::handle_dictate_http;
pub(super) use models::{is_ready as model_ready, model_dir, progress as model_progress};
