//! Ghostex Capture: a floating Ghostex button over every app that shows how many sessions are
//! working, waiting for the user, or asking a question, and lets the user screenshot an area, the
//! current app or the full screen, mark it up, and send a prompt to any project or session without
//! switching to Ghostex.
//!
//! SEE-ALSO: apps/desktop/native/macos/GpuiGhostexCapture.m, packages/gx-core/src/indicators.rs
//! (the counts), docs/2026-09-30/ghostex-capture/ (the mockups the user approved).

mod capture;
mod drafts;
mod editor;
mod glass;
mod grab;
mod hotkeys;
mod icon_window;
mod lifecycle;
mod machine_sections;
mod model;
mod note;
mod overlay;
mod panel_window;
mod persistence;
mod placement;
pub(crate) mod platform;
mod project_list;
mod prompt;
mod save;
mod screen_access;
mod send;
mod targets;
mod tooltip;

pub(crate) use model::GhostexCaptureState;
