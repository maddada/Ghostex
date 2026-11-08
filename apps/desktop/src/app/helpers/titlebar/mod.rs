//! Titlebar and window chrome helpers: tooltips and popup geometry, project and tips settings, the
//! resource monitor, popup menu rows, progress indicators, chrome colours and tints, URL and session
//! helpers, titlebar actions, and the view mode switcher.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

pub(crate) mod chrome_colors;
pub(crate) mod chrome_tints;
pub(crate) mod extension_menu_row;
pub(crate) mod mode_switcher;
pub(crate) mod popup_rows;
pub(crate) mod progress_indicators;
pub(crate) mod project_and_tips;
pub(crate) mod resource_monitor;
pub(crate) mod titlebar_actions;
pub(crate) mod tooltips_and_popup_geometry;
pub(crate) mod urls_and_sessions;

pub(crate) use chrome_colors::*;
pub(crate) use chrome_tints::*;
pub(crate) use extension_menu_row::*;
pub(crate) use mode_switcher::*;
pub(crate) use popup_rows::*;
pub(crate) use progress_indicators::*;
pub(crate) use project_and_tips::*;
pub(crate) use resource_monitor::*;
pub(crate) use titlebar_actions::*;
pub(crate) use tooltips_and_popup_geometry::*;
pub(crate) use urls_and_sessions::*;
