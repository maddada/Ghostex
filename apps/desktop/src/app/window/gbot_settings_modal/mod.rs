//! The native GBot Settings window (a GBot's Profile, Instructions, Department, Memory and
//! Routines, Computer: where its chats run, and Bot API: apps messaging it with a key).
//!
//! Like every native modal view, it depends only on gpui, gpui-component, serde_json, the shared
//! modal kit and Settings' palette, so the `native-modal-demo` binary and the web build can
//! include it.
mod bot_api;
mod computer;
mod department;
mod memory;
mod memory_model;
mod model;
mod pages;
mod parts;
mod profile;
mod rail;
mod routines;
mod voice;
mod window;

// The modal demo binary answers memory commands without naming the reply type.
#[allow(unused_imports)]
pub(crate) use bot_api::GbotApiReply;
#[allow(unused_imports)]
pub(crate) use computer::GbotComputerReply;
#[allow(unused_imports)]
pub(crate) use department::GbotDepartmentReply;
#[allow(unused_imports)]
pub(crate) use memory::GbotMemoryReply;
pub(crate) use model::{
    GBOT_SETTINGS_MODAL_INITIAL_HEIGHT, GBOT_SETTINGS_MODAL_WIDTH, GbotSettingsRecord,
    gbot_agent_choices,
};
pub(crate) use routines::GbotRoutinesReply;
pub(crate) use window::{
    GbotSettingsModalCommand, GbotSettingsModalConfig, GbotSettingsTab, GpuiGbotSettingsModalWindow,
};
