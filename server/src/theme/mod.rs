//! `ghostex theme`: agents and you restyle Ghostex through the look settings, with a history that
//! Undo and redo walk and a "Theme changed" card in the chat of the agent that changed it.

mod art;
mod endpoint;
mod gallery;
mod journal;
mod look;
mod readability;

pub(crate) use endpoint::{THEME_ENDPOINT, handle_theme_http};
