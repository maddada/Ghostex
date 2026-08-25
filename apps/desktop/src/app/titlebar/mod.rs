// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). Each submodule contributes
// its own `impl GhostexGpuiApp` block, so no glob re-export is needed here
// (inherent methods resolve on the type regardless of which module defines
// them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
pub(crate) mod action_execution;
pub(crate) mod browser_toolbar_buttons;
pub(crate) mod dropdown_panels;
pub(crate) mod extension_buttons;
pub(crate) mod icon_button_and_browser_toolbar;
pub(crate) mod menu_triggers;
pub(crate) mod open_targets_and_window_controls;
pub(crate) mod popup_lifecycle;
pub(crate) mod popup_menu_builders;
pub(crate) mod resources_snapshot;
pub(crate) mod settings_and_action_state;
pub(crate) mod titlebar_buttons;
pub(crate) mod titlebar_buttons_misc;
