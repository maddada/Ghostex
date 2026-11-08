//! Dictate's engines and clean-up: Hex 3 on this computer (`hex`), the per-computer settings
//! (`settings`, `dictate`, `languages`), what is left of the retired local engine (`retired`), the
//! steps after transcription (`cleanup`) and their API (`http`).

pub(super) mod cleanup;
pub(super) mod dictate;
pub(super) mod hex;
mod http;
mod languages;
pub(super) mod retired;
pub(super) mod settings;

#[cfg(test)]
mod cleanup_tests;
#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod hex_tests;
#[cfg(test)]
mod settings_tests;

pub(crate) use http::handle_dictate_http;
pub(crate) use settings::uses_hex as dictate_uses_hex;
