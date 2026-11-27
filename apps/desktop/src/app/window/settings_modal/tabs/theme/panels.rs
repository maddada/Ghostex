//! Size & Zoom's Panel sizes (the sidebar's width and the Commands pane's height on double-click)
//! and Window & Layout's Panels (Animations, Close side panel with its last tab, Commands pane
//! side). They were rows of Sidebar & Sessions and Terminal, with a link from Size & Zoom that
//! found nothing, until CDXC:Settings 2026-11-19 in the settings catalog's general/theme.rs.
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::{
    PageAction, SliderBinding, reset_key, segmented_field, select_field, settings_section,
    slider_number_field, toggle_field,
};
use super::{ThemeCx, ThemeTab};
use gpui::{AnyElement, Context, IntoElement, Window};
use serde_json::{Map, json};
use std::rc::Rc;

const SIDEBAR_WIDTH_KEY: &str = "sidebarDefaultWidthPx";
const COMMANDS_HEIGHT_KEY: &str = "commandsPanelDefaultHeightPx";
const ANIMATION_KEY: &str = "panelAnimationSpeed";
const FOLD_SPEED_KEY: &str = "sidebarCollapseAnimationDurationMs";
const CLOSE_SIDE_PANEL_KEY: &str = "closeSidePanelWithLastTab";
const COMMANDS_SIDE_KEY: &str = "commandsPanelSide";

fn number(name: &str) -> f64 {
    settings_catalog().number(module::SETTINGS, name)
}

impl ThemeTab {
    /// Size & Zoom's Panel sizes.
    pub(super) fn panel_sizes_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = t.p;
        let mut rows = Vec::new();
        for (key, label, description, min, max) in [
            (
                SIDEBAR_WIDTH_KEY,
                "Sidebar width on double-click",
                "The sidebar's width when you double-click its resize handle. Ghostex still opens with the width you last dragged it to.",
                number("MIN_SIDEBAR_DEFAULT_WIDTH_PX"),
                number("MAX_SIDEBAR_DEFAULT_WIDTH_PX"),
            ),
            (
                COMMANDS_HEIGHT_KEY,
                "Commands pane height on double-click",
                "The Commands pane's height when it opens (F12 or the sidebar) and when you double-click its top resize rail.",
                number("MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX"),
                number("MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX"),
            ),
        ] {
            if !t.visible(key) {
                continue;
            }
            rows.push(slider_number_field(
                self,
                &p,
                t.spec(key, label, description),
                Some(reset_key::<Self>(key)),
                SliderBinding {
                    key,
                    min,
                    max,
                    step: 1.0,
                },
                t.values.f64(key),
                window,
                cx,
            ));
        }
        settings_section(&p, "Panel sizes", None, None, rows).map(IntoElement::into_any_element)
    }

    /// Window & Layout's Panels.
    pub(super) fn panels_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = t.p;
        let values = &t.values;
        let mut rows = Vec::new();
        // CDXC:Settings 2026-10-29 DECISION (cleanup Q13): one Animations row sets the panel slide
        // speed and the sidebar fold speed together (Off 0 ms, Slow 600, Normal 400, Fast 200).
        // CDXC:Settings 2026-11-27: the fold speed on its own ("Animations: fold speed") is a Power
        // User Flag, the Fold speed row below; Animations still sets both.
        if t.visible(ANIMATION_KEY) {
            let spec = t
                .spec(
                    ANIMATION_KEY,
                    "Animations",
                    "How fast the sidebar, the side panel, the Agents Panel and the bottom or right panel slide open and closed, and how fast sidebar sections, groups and projects fold. Reduce Motion in your computer settings always turns them off.",
                )
                .modified(values.is_modified(ANIMATION_KEY));
            let options =
                settings_catalog().options(module::SETTINGS, "PANEL_ANIMATION_SPEED_OPTIONS");
            let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
            let value = values.choice(ANIMATION_KEY, &allowed);
            let reset: PageAction<Self> = Rc::new(|page: &mut Self, _window, cx| {
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    store.reset_setting(ANIMATION_KEY, cx);
                    store.reset_setting(FOLD_SPEED_KEY, cx);
                });
            });
            rows.push(segmented_field(
                &p,
                ANIMATION_KEY,
                spec,
                Some(reset),
                &options,
                Some(value.as_str()),
                None,
                |page: &mut Self, next, _window, cx| {
                    let fold_ms = match next.as_str() {
                        "off" => 0,
                        "slow" => 600,
                        "fast" => 200,
                        _ => 400,
                    };
                    let mut patch = Map::new();
                    patch.insert(ANIMATION_KEY.to_string(), json!(next));
                    patch.insert(FOLD_SPEED_KEY.to_string(), json!(fold_ms));
                    page.save_patch(patch, cx);
                },
                cx,
            ));
        }
        if t.visible(FOLD_SPEED_KEY) {
            rows.push(slider_number_field(
                self,
                &p,
                t.spec(
                    FOLD_SPEED_KEY,
                    "Fold speed",
                    "How many milliseconds sidebar sections, groups and projects take to fold open and closed, and the floating sidebar and Agents Panel to slide in. 0 turns it off. Animations sets it too.",
                ),
                Some(reset_key::<Self>(FOLD_SPEED_KEY)),
                SliderBinding {
                    key: FOLD_SPEED_KEY,
                    min: number("MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS"),
                    max: number("MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS"),
                    step: number("SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS"),
                },
                values.f64(FOLD_SPEED_KEY),
                window,
                cx,
            ));
        }
        if t.visible(CLOSE_SIDE_PANEL_KEY) {
            rows.push(toggle_field(
                self,
                &p,
                CLOSE_SIDE_PANEL_KEY,
                t.spec(
                    CLOSE_SIDE_PANEL_KEY,
                    "Close side panel with its last tab",
                    "Close the side panel when you close its last tab, instead of showing the Open a view picker.",
                ),
                values.bool(CLOSE_SIDE_PANEL_KEY),
                cx,
            ));
        }
        if t.visible(COMMANDS_SIDE_KEY) {
            let options =
                settings_catalog().options(module::SETTINGS, "COMMANDS_PANEL_SIDE_OPTIONS");
            let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
            rows.push(select_field(
                self,
                &p,
                COMMANDS_SIDE_KEY,
                t.spec(
                    COMMANDS_SIDE_KEY,
                    "Commands pane side",
                    "Where terminal Actions and F12 open the Commands pane: below the workspace or as a column to its right.",
                ),
                Some(reset_key::<Self>(COMMANDS_SIDE_KEY)),
                &options,
                &values.choice(COMMANDS_SIDE_KEY, &allowed),
                None,
                |page: &mut Self, next, _window, cx| page.save(COMMANDS_SIDE_KEY, json!(next), cx),
                window,
                cx,
            ));
        }
        settings_section(&p, t.section_title("panels", "Panels"), None, None, rows)
            .map(IntoElement::into_any_element)
    }
}
