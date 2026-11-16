//! `ghostex theme`: agents and you restyle Ghostex through the look settings, with a history that
//! Undo and redo walk and a "Theme changed" card in the chat of the agent that changed it.

mod endpoint;
mod journal;
mod look;
mod readability;

pub(crate) use endpoint::{handle_theme_http, THEME_ENDPOINT};
