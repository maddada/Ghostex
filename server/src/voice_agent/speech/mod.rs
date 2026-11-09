//! Dictate's engines: Hex 3 on this computer (`hex`), the per-computer settings (`settings`,
//! `dictate`, `languages`), what is left of the retired local engine (`retired`) and their API
//! (`http`).

pub(super) mod dictate;
pub(super) mod hex;
mod http;
mod languages;
pub(super) mod retired;
pub(super) mod settings;

#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod hex_tests;
#[cfg(test)]
mod settings_tests;

pub(crate) use http::handle_dictate_http;
pub(crate) use retired::spawn_startup_delete as spawn_old_speech_models_delete;
pub(crate) use settings::uses_hex as dictate_uses_hex;
