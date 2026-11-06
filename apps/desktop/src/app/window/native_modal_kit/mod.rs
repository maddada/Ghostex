//! The shared look and controls of the native GPUI app modals: the
//! `.gx-app-modal` design language from packages/core-ui/styles/modals.css (deleted 2026-11-06)
//! (window surface, section panels, raised controls, hairlines, 12px sections,
//! 8px controls, one 32px control height, full-width paired footer buttons)
//! plus the shadcn switch, select and input skins the React modals use.
//!
//! CDXC:AppModal 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt in native GPUI one at a time, each one matching its React twin 1 to 1 in both appearances. Every native modal draws its chrome and controls from this one module, the same rule the React shell enforces through `AppModalShell`, so restyling the design language stays a single edit.
//! SEE-ALSO: packages/core-ui/app-modal-shell.tsx (deleted 2026-10-01) and the `.gx-app-modal` rules in packages/core-ui/styles/modals.css (deleted 2026-11-06) and modals-light.css (deleted 2026-11-06) (the tokens mirrored here), apps/desktop/src/app/native_app_modal_lifecycle.rs (window open, fit, close).
//!
//! This module depends only on gpui and gpui-component so the preview binaries can include it with `#[path]`.

pub(crate) mod controls;
pub(crate) mod legacy;
pub(crate) mod palette;
pub(crate) mod popover_host;
pub(crate) mod segmented_and_rail;
pub(crate) mod select;
pub(crate) mod shell;
pub(crate) mod skinned;

pub(crate) use controls::*;
pub(crate) use legacy::*;
pub(crate) use palette::*;
pub(crate) use popover_host::*;
pub(crate) use segmented_and_rail::*;
pub(crate) use select::*;
pub(crate) use shell::*;
pub(crate) use skinned::*;
