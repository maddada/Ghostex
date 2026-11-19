//! The terminal's background rows of Themes' Terminal colours: Terminal background and its custom
//! colour, Background opacity while the window's Transparency is on, and the background image
//! with its opacity. They were rows of the Terminal page, which this group linked to, until
//! CDXC:Settings 2026-11-19 in the settings catalog's general/theme.rs. The Terminal page's own
//! Transparency switch (a second copy of `windowGlass`) went with them: Transparency is this page's
//! Glass group.
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    PageAction, SliderBinding, color_field, reset_key, select_field, slider_number_field,
    text_field,
};
use super::{ThemeCx, ThemeTab};
use gpui::{AnyElement, Context, Window};
use serde_json::{Map, json};
use std::rc::Rc;

const MODE_KEY: &str = "terminalBackgroundMode";
const COLOR_KEY: &str = "workspaceBackgroundColor";
const OPACITY_KEY: &str = "terminalBackgroundOpacity";
const IMAGE_KEY: &str = "terminalBackgroundImage";
const IMAGE_OPACITY_KEY: &str = "terminalBackgroundImageOpacity";

/// The colour Terminal background starts from when Custom color is picked with none saved.
const STARTING_COLOR: &str = "#111111";

fn option(label: &str, value: &str) -> SettingOption {
    SettingOption {
        label: label.to_string(),
        value: value.to_string(),
    }
}

impl ThemeTab {
    pub(super) fn terminal_background_rows(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let p = t.p;
        let values = &t.values;
        let mut rows = Vec::new();
        // CDXC:Settings 2026-11-14 DECISION (AGENTS.md, Q3107): with Terminal shaders on (macOS)
        // the shaders draw the background from Ghostex's terminal config, so Terminal background
        // and its colour are hidden instead of greyed out; the saved choice returns when shaders
        // go off.
        let shaders_own_background =
            cfg!(target_os = "macos") && values.bool("terminalShadersEnabled");
        if !shaders_own_background && t.visible(MODE_KEY) {
            let options = vec![
                option("Black / white", "pure"),
                option("Follow theme", "theme"),
                option("Custom color", "custom"),
            ];
            let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
            rows.push(select_field(
                self,
                &p,
                MODE_KEY,
                t.spec(
                    MODE_KEY,
                    "Terminal background",
                    "Only changes the terminal panes. Black / white is pure black in dark mode and pure white in light mode.",
                ),
                Some(reset_key::<Self>(MODE_KEY)),
                &options,
                &values.choice(MODE_KEY, &allowed),
                None,
                |page: &mut Self, next, _window, cx| {
                    let mut patch = Map::new();
                    if next == "custom" && page.store.read(cx).values().string(COLOR_KEY).is_empty() {
                        patch.insert(COLOR_KEY.into(), json!(STARTING_COLOR));
                    }
                    patch.insert(MODE_KEY.into(), json!(next));
                    page.save_patch(patch, cx);
                },
                window,
                cx,
            ));
        }
        if !shaders_own_background && values.string(MODE_KEY) == "custom" && t.visible(COLOR_KEY) {
            rows.push(color_field(
                self,
                &p,
                COLOR_KEY,
                t.spec(
                    COLOR_KEY,
                    "Terminal background color",
                    "Painted behind terminal text in dark and light mode. Window glass keeps the theme.",
                )
                .dependent(),
                Some(reset_key::<Self>(COLOR_KEY)),
                &values.string(COLOR_KEY),
                window,
                cx,
            ));
        }
        // The opacity acts on the window's glass, so it shows only while Transparency (the Glass
        // group above) is on; the rule is on `TERMINAL_GLASS_PERCENT` in app/helpers/terminal_glass.rs.
        let glass = values.string("windowGlass");
        if glass != "opaque" && t.visible(OPACITY_KEY) {
            let description = format!(
                "How much of the work area's glass tint terminals keep while Transparency is on. 100% matches the rest of the work area; lower shows more of what is behind the window.{}",
                if glass == "auto" {
                    " Transparency is dark mode only."
                } else {
                    ""
                },
            );
            rows.push(slider_number_field(
                self,
                &p,
                t.spec(OPACITY_KEY, "Terminal background opacity (%)", &description),
                Some(reset_key::<Self>(OPACITY_KEY)),
                SliderBinding {
                    key: OPACITY_KEY,
                    min: 0.0,
                    max: 100.0,
                    step: 1.0,
                },
                values.f64(OPACITY_KEY),
                window,
                cx,
            ));
        }
        if t.visible(IMAGE_KEY) {
            let browse: PageAction<Self> = Rc::new(|page: &mut Self, _window, cx| {
                let store = page.store.clone();
                super::super::super::store::post_store_message(
                    &store,
                    json!({ "type": "pickTerminalBackgroundImageFile" }),
                    cx,
                );
            });
            rows.push(text_field(
                self,
                &p,
                IMAGE_KEY,
                t.spec(
                    IMAGE_KEY,
                    "Terminal background image",
                    "Absolute path to an image drawn behind terminal panes. Leave blank for none.",
                ),
                Some(reset_key::<Self>(IMAGE_KEY)),
                &values.string(IMAGE_KEY),
                Some("/Users/you/Pictures/background.png"),
                None,
                Some(("Choose image file".into(), browse)),
                window,
                cx,
            ));
        }
        // CDXC:Settings 2026-10-29 DECISION (cleanup Q16): the opacity sits under the picked image,
        // and the image always covers the pane (the Fit setting is gone).
        if !values.string(IMAGE_KEY).trim().is_empty() && t.visible(IMAGE_OPACITY_KEY) {
            rows.push(slider_number_field(
                self,
                &p,
                t.spec(
                    IMAGE_OPACITY_KEY,
                    "Background image opacity",
                    "Blend the background image toward the terminal background color.",
                )
                .dependent(),
                Some(reset_key::<Self>(IMAGE_OPACITY_KEY)),
                SliderBinding {
                    key: IMAGE_OPACITY_KEY,
                    min: 0.0,
                    max: 1.0,
                    step: 0.05,
                },
                values.f64(IMAGE_OPACITY_KEY),
                window,
                cx,
            ));
        }
        rows
    }
}
