//! Under window glass a dialog nested in Settings is a see-through frosted tint over the page, so
//! its backdrop is a veil of the theme's own colour instead of the black dim: the page stays in
//! view, but faint enough that its text never reads through the sheet (CDXC:Settings 2026-11-08 on
//! `SettingsPalette::dialog_surface`).
use super::super::super::native_modal_kit::*;
use gpui::{Rgba, Window, WindowId};
use std::cell::RefCell;
use std::collections::HashMap;

/// How much of the page the veil covers under glass. With the sheet's own tint over it, the page's
/// text behind a dialog keeps well under a tenth of its contrast.
const GLASS_VEIL_ALPHA: f32 = 0.92;

thread_local! {
    /// The veil colour of each Settings window drawn under glass, as its shell last rendered.
    static GLASS_VEILS: RefCell<HashMap<WindowId, Rgba>> = RefCell::new(HashMap::new());
}

/// Records the veil colour of the Settings shell in `window` (`None` without glass); the shell
/// calls it each render, before its pages draw their dialogs.
pub(crate) fn set_settings_glass(window: &Window, veil: Option<Rgba>) {
    let window_id = window.window_handle().window_id();
    GLASS_VEILS.with(|veils| {
        let mut veils = veils.borrow_mut();
        match veil {
            Some(veil) => {
                veils.insert(window_id, veil);
            }
            None => {
                veils.remove(&window_id);
            }
        }
    });
}

/// A nested dialog's backdrop: black at `alpha`, or under glass the theme-coloured veil. A
/// backdrop that dims nothing (`alpha` 0) stays clear.
pub(crate) fn nested_backdrop(window: &Window, alpha: f32) -> Rgba {
    let window_id = window.window_handle().window_id();
    let veil = GLASS_VEILS.with(|veils| veils.borrow().get(&window_id).copied());
    match veil {
        Some(veil) if alpha > 0.0 => Rgba {
            a: GLASS_VEIL_ALPHA,
            ..veil
        },
        _ => modal_rgba(0x000000, alpha),
    }
}
