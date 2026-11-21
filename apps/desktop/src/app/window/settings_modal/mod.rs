//! The native Settings modal (Settings, Hotkeys, Configure Agents, Configure Actions and Open
//! Targets all open it on their own page).
//!
//! CDXC:Settings 2026-09-30 DECISION:
//! User: migrate every React modal to GPUI with GPUI-Kit components, looking and working exactly as now, so the app runs without CEF; now that every page is native, "i want to show the gpui one for everything please no more react settings" (supersedes the 2026-09-28 `GHOSTEX_NATIVE_SETTINGS=1` opt-in). Settings, Hotkeys, Configure Agents, Configure Actions and Open Targets always open this modal.
//! SEE-ALSO: docs/2026-09-28/gpui-modals-migration/SETTINGS-ARCH.md (how a page plugs in), packages/core-ui/settings-modal.tsx (deleted 2026-10-01) and packages/core-ui/settings-modal/ (deleted 2026-10-01) (the React twin), apps/desktop/src/app/settings_modal_lifecycle.rs (open, save, bridge).
//!
//! Like every native modal view, this module depends only on gpui, gpui-component, gpui-base,
//! serde_json and the shared modal kit, so the `native-modal-demo` binary can include it.
// The field library and the store API also serve the pages still being ported on this shell.
#![allow(dead_code)]
pub(crate) mod catalog;
pub(crate) mod fields;
pub(crate) mod model;
pub(crate) mod page;
pub(crate) mod palette;
pub(crate) mod rail;
pub(crate) mod restore;
pub(crate) mod search;
pub(crate) mod shell;
pub(crate) mod store;
pub(crate) mod tabs;
pub(crate) mod theme_editor;

pub(crate) use model::{SettingsModalCommand, SettingsOpenRequest};
pub(crate) use shell::{
    GpuiSettingsModalWindow, SETTINGS_MODAL_HEIGHT, SETTINGS_MODAL_WIDTH, SettingsModalConfig,
};
pub(crate) use theme_editor::{GpuiThemeEditorWindow, THEME_EDITOR_HEIGHT, THEME_EDITOR_WIDTH};
