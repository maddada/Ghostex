//! Panel 2, "Make it yours" (Q4387: Second, right after Welcome).
//!
//! Shares the compact theme controls with the floating Theme Editor: Appearance (System / Light /
//! Dark), Theme tiles (Default, Art themes, Agent-made looks, Presets), Background (Art / Live /
//! Glass / Solid) with thumbnail picks, Accent colour dots, and the 16 subtle-capped colour
//! squares plus uncapped Custom colour, with a live Ghostex preview on the right half of the stage.
use super::GpuiOnboardingWindow;
use super::get_started::appearance_label;
use super::interact;
use super::model::{
    DARK_PRESETS, LIGHT_PRESETS, colourfulness_display_step, matching_preset, theme_preset_label,
};
use super::primitives::*;
use super::stage::*;
use super::welcome::foot_actions;
use super::super::settings_modal::tabs::theme::gallery::poster as live_poster;
use super::super::settings_modal::tabs::theme::prompt_gallery::{
    GalleryTheme, artwork_thumbnail_bytes, gallery_themes, preview_bytes,
};
use ghostex_settings_catalog::theme_roles::{
    RoleValue, THEME_ROLES_SETTING, appearance_key, parse_role_value,
};
use gpui::StyledImage as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Image, ImageFormat, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, SharedString, Styled as _, div, img,
};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::Arc;

const ACCENT_DOTS: [&str; 7] = [
    "#86d3f8", "#d8664f", "#7aa2f7", "#7ecdc5", "#a7c080", "#d8a657", "#d69bb5",
];

const CUSTOM_SWATCHES: [&str; 6] = [
    "#1b5e3f", "#264e86", "#6b2d5c", "#7c4a1d", "#185e63", "#5c2735",
];

const ART_STRIP_IDS: [&str; 6] = [
    "moonwater",
    "glasshouse",
    "red-earth",
    "deep-current",
    "night-garden",
    "nocturne-canyon",
];

const LIVE_STRIP_IDS: [(&str, &str); 6] = [
    ("ink", "Ink"),
    ("drift", "Drift"),
    ("nebula", "Nebula"),
    ("mesh", "Mesh"),
    ("aurora", "Aurora"),
    ("silk", "Silk"),
];

#[derive(Default)]
pub(crate) struct LookState {
    pub(crate) show_more_themes: bool,
    pub(crate) show_custom_bar: bool,
    pub(crate) picked_dark: bool,
    pub(crate) picked_light: bool,
    pub(crate) active_theme_id: Option<String>,
    pub(crate) posters: HashMap<String, Arc<Image>>,
}

fn parse_rgb(hex_str: &str, fallback: u32) -> u32 {
    u32::from_str_radix(hex_str.trim().trim_start_matches('#'), 16).unwrap_or(fallback)
}

fn rgb_to_hsla(rgb: u32, alpha: f32) -> gpui::Hsla {
    rgba(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
        alpha,
    )
}

fn role_fill(roles: &Value, light: bool, id: &str, base_rgb: u32, fallback_step: f32) -> gpui::Hsla {
    let val = roles
        .get(appearance_key(light))
        .and_then(|obj| obj.get(id))
        .and_then(Value::as_str)
        .and_then(|raw| parse_role_value(raw).ok())
        .unwrap_or(RoleValue::Base {
            step: fallback_step,
            alpha: 1.0,
        });
    let [r, g, b, a] = val.rgba_on_base(light, base_rgb);
    gpui::Hsla::from(gpui::Rgba { r, g, b, a })
}

impl GpuiOnboardingWindow {
    fn look_is_dark(&self) -> bool {
        match self.settings.sidebar_theme.as_str() {
            "system" => !self.system_light,
            theme => theme.starts_with("dark"),
        }
    }

    fn look_current_bg(&self, dark: bool) -> &'static str {
        let art = if dark {
            &self.settings.window_art
        } else {
            &self.settings.window_art_light
        };
        if ghostex_settings_catalog::artwork_by_id(art).is_some() {
            return "art";
        }
        if self.settings.window_glass == "opaque" {
            return "solid";
        }
        if self.settings.window_glass_source == "live" {
            return "live";
        }
        "transparent"
    }

    fn look_cached_image(
        &mut self,
        kind: &str,
        id: &str,
        bytes: fn(&str) -> Option<&'static [u8]>,
    ) -> Option<Arc<Image>> {
        let key = format!("{kind}-{id}");
        if let Some(image) = self.look.posters.get(&key) {
            return Some(image.clone());
        }
        let image = Arc::new(Image::from_bytes(ImageFormat::Webp, bytes(id)?.to_vec()));
        self.look.posters.insert(key, image.clone());
        Some(image)
    }

    fn look_cached_live(&mut self, style: &str, light: bool) -> Option<Arc<Image>> {
        let key = format!("live-{style}-{}", if light { "light" } else { "dark" });
        if let Some(image) = self.look.posters.get(&key) {
            return Some(image.clone());
        }
        let image = live_poster(style, light)?;
        self.look.posters.insert(key, image.clone());
        Some(image)
    }

    fn apply_onboarding_gallery_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(theme) = gallery_themes().into_iter().find(|theme| theme.id == id) else {
            return;
        };
        let mut changes = theme.changes.clone();
        if let Some(background) = &theme.default_background {
            changes.extend(theme.background_patch(background, true));
        }
        if theme.kind != "preset" && !self.look_is_dark() {
            changes.insert("sidebarTheme".into(), json!("dark-2"));
        }
        let mut patch = Map::new();
        let mut roles = self
            .settings
            .theme_roles
            .as_object()
            .cloned()
            .unwrap_or_default();
        let mut touched_roles = false;
        for (key, value) in changes {
            if let Some(rest) = key.strip_prefix("role.") {
                touched_roles = true;
                let (mode, role_id) = match rest.split_once('.') {
                    Some((m @ ("dark" | "light"), r)) => (m, r),
                    _ => ("dark", rest),
                };
                let obj = roles
                    .entry(mode.to_string())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(map) = obj.as_object_mut() {
                    if value.as_str() == Some("default") {
                        map.remove(role_id);
                    } else {
                        map.insert(role_id.to_string(), value);
                    }
                }
                continue;
            }
            if let Some(text) = value.as_str() {
                if let Some(default) = ghostex_settings_catalog::default_value(&key) {
                    if default.as_f64().is_some()
                        && let Ok(num) = text.parse::<f64>()
                    {
                        patch.insert(key, json!(num));
                        continue;
                    }
                }
            }
            patch.insert(key, value);
        }
        if touched_roles {
            patch.insert(THEME_ROLES_SETTING.to_string(), Value::Object(roles));
        }
        self.look.active_theme_id = Some(id.to_string());
        self.look.show_custom_bar = false;
        self.update_settings(patch, cx);
    }

    fn pick_look_background(&mut self, bg: &str, cx: &mut Context<Self>) {
        let dark = self.look_is_dark();
        let art_key = if dark { "windowArt" } else { "windowArtLight" };
        let mut patch = Map::new();
        match bg {
            "art" => {
                let current = if dark {
                    &self.settings.window_art
                } else {
                    &self.settings.window_art_light
                };
                let id = if ghostex_settings_catalog::artwork_by_id(current).is_some() {
                    current.clone()
                } else {
                    "moonwater".to_string()
                };
                patch.insert(art_key.into(), json!(id));
                patch.insert("windowGlass".into(), json!("frosted"));
            }
            "live" => {
                patch.insert(art_key.into(), json!("none"));
                patch.insert("windowGlass".into(), json!("frosted"));
                patch.insert("windowGlassSource".into(), json!("live"));
            }
            "transparent" => {
                patch.insert(art_key.into(), json!("none"));
                patch.insert("windowGlass".into(), json!("frosted"));
                patch.insert("windowGlassSource".into(), json!("desktopAndWindows"));
            }
            _ => {
                patch.insert(art_key.into(), json!("none"));
                patch.insert("windowGlass".into(), json!("opaque"));
            }
        }
        self.update_settings(patch, cx);
    }

    fn pick_look_square(&mut self, dark: bool, preset: String, cx: &mut Context<Self>) {
        let mut patch = Map::new();
        let (key, other_key, other_picked) = if dark {
            ("darkThemePreset", "lightThemePreset", self.look.picked_light)
        } else {
            ("lightThemePreset", "darkThemePreset", self.look.picked_dark)
        };
        if !other_picked
            && preset != "custom"
            && let Some(paired) = matching_preset(dark, &preset)
        {
            patch.insert(other_key.into(), json!(paired));
        }
        patch.insert(key.into(), json!(preset));
        if dark {
            self.look.picked_dark = true;
        } else {
            self.look.picked_light = true;
        }
        self.look.active_theme_id = None;
        self.look.show_custom_bar = false;
        self.update_settings(patch, cx);
    }

    fn pick_look_custom_hex(&mut self, dark: bool, hex_color: &str, cx: &mut Context<Self>) {
        let mut patch = Map::new();
        if dark {
            patch.insert("darkThemePreset".into(), json!("custom"));
            patch.insert(
                "customSidebarTitlebarBackgroundTintColor".into(),
                json!(hex_color),
            );
            self.look.picked_dark = true;
        } else {
            patch.insert("lightThemePreset".into(), json!("custom"));
            patch.insert(
                "customSidebarTitlebarLightBackgroundTintColor".into(),
                json!(hex_color),
            );
            self.look.picked_light = true;
        }
        self.look.active_theme_id = None;
        self.update_settings(patch, cx);
    }

    fn pick_look_accent(&mut self, hex_color: &str, cx: &mut Context<Self>) {
        let light = !self.look_is_dark();
        let mut roles = self
            .settings
            .theme_roles
            .as_object()
            .cloned()
            .unwrap_or_default();
        let entry = roles
            .entry(appearance_key(light).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(map) = entry.as_object_mut() {
            map.insert("accent".into(), json!(hex_color));
            map.insert("focusRing".into(), json!(hex_color));
        }
        let mut patch = Map::new();
        patch.insert(THEME_ROLES_SETTING.into(), Value::Object(roles));
        self.update_settings(patch, cx);
    }

    pub(super) fn render_look(&mut self, s: S, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let dark = self.look_is_dark();
        let light = !dark;
        let current_bg = self.look_current_bg(dark);
        let themes: Vec<GalleryTheme> = gallery_themes();
        let left_x = 46.0;
        let left_w = 742.0;

        let mut out = Vec::new();
        out.push(eyebrow(s, left_x, 96.0, None, "Look").into_any_element());
        out.push(
            heading(
                s,
                left_x,
                126.0,
                left_w,
                54.0,
                "Make it yours.",
                None,
                false,
            )
            .into_any_element(),
        );
        out.push(
            sub(s, left_x, 194.0, left_w, 16.0, false)
                .child("Pick a theme, what shows behind the window and your colours. You can change any of it later.")
                .into_any_element(),
        );

        // ---- Appearance segmented bar ----
        let appearance = self.settings.sidebar_theme.clone();
        let sys_light = self.system_light;
        let appearance_bar = div()
            .flex()
            .gap(s.px(6.0))
            .p(s.px(4.0))
            .rounded(s.px(10.0))
            .bg(rgba(18, 24, 38, 0.72))
            .border_1()
            .border_color(rgba(160, 180, 255, 0.14))
            .children(
                [("system", "System"), ("plain-light", "Light"), ("dark-2", "Dark")]
                    .into_iter()
                    .map(|(value, name)| {
                        let selected = appearance == value;
                        let key = SharedString::from(format!("look-appearance-{value}"));
                        let label_text = appearance_label(value, name, sys_light);
                        self.control(
                            s,
                            div()
                                .id(key.clone())
                                .flex_1()
                                .h(s.px(32.0))
                                .rounded(s.px(7.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .when(selected, |this| {
                                    this.bg(rgba(44, 56, 102, 0.85))
                                        .border_1()
                                        .border_color(rgba(124, 148, 232, 0.5))
                                })
                                .child(sans(
                                    s,
                                    13.5,
                                    if selected { 600.0 } else { 400.0 },
                                    if selected { hex(0xeef1f7) } else { hex(0x9aa3b8) },
                                )
                                .child(label_text)),
                            key,
                            interact::Ring::new(7.0, 1.0),
                            interact::Keys::EnterSpace,
                            cx,
                            move |this, _, cx| {
                                let mut patch = Map::new();
                                patch.insert("sidebarTheme".into(), json!(value));
                                this.update_settings(patch, cx);
                            },
                        )
                    }),
            );

        // ---- Theme tiles (4 columns, 7 themes + All themes toggle, or 12 expanded) ----
        let active_theme_id = self.look.active_theme_id.clone();
        let tile_limit = if self.look.show_more_themes { 11 } else { 7 };
        let mut theme_tiles: Vec<AnyElement> = Vec::with_capacity(tile_limit + 1);
        for theme in themes.iter().take(tile_limit) {
            let id = theme.id.clone();
            let short = theme.label.clone().unwrap_or_else(|| {
                theme
                    .name
                    .split_whitespace()
                    .next()
                    .unwrap_or(&theme.name)
                    .to_string()
            });
            let selected = active_theme_id.as_deref() == Some(id.as_str())
                || (active_theme_id.is_none() && id == "default" && self.settings.dark_theme_preset == "gray");
            let pic = self.look_cached_image("theme-preview", &id, preview_bytes);
            let key = SharedString::from(format!("look-theme-{id}"));
            theme_tiles.push(self.control(
                s,
                div()
                    .id(key.clone())
                    .w(s.px(176.0))
                    .flex()
                    .flex_col()
                    .gap(s.px(5.0))
                    .cursor_pointer()
                    .child(
                        div()
                            .w_full()
                            .h(s.px(66.0))
                            .rounded(s.px(9.0))
                            .overflow_hidden()
                            .bg(rgba(22, 27, 40, 0.9))
                            .border_color(if selected {
                                hex(0xeef1f7)
                            } else {
                                rgba(160, 180, 255, 0.16)
                            })
                            .when(selected, |this| this.border_2())
                            .when(!selected, |this| this.border_1())
                            .when_some(pic, |this, pic| {
                                this.child(img(pic).size_full().object_fit(ObjectFit::Cover))
                            }),
                    )
                    .child(
                        sans(
                            s,
                            12.5,
                            if selected { 600.0 } else { 400.0 },
                            if selected { hex(0xeef1f7) } else { hex(0x9aa3b8) },
                        )
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(short),
                    ),
                key,
                interact::Ring::new(9.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                move |this, _, cx| this.apply_onboarding_gallery_theme(&id, cx),
            ).into_any_element());
        }
        let expanded = self.look.show_more_themes;
        let more_count = themes.len();
        theme_tiles.push(self.control(
            s,
            div()
                .id("look-theme-more")
                .w(s.px(176.0))
                .flex()
                .flex_col()
                .gap(s.px(5.0))
                .cursor_pointer()
                .child(
                    div()
                        .w_full()
                        .h(s.px(66.0))
                        .rounded(s.px(9.0))
                        .bg(rgba(27, 35, 64, 0.78))
                        .border_1()
                        .border_color(rgba(160, 180, 255, 0.22))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            sans(s, 15.0, 600.0, hex(0xdbe2f5)).child(if expanded {
                                "Fewer".to_string()
                            } else {
                                format!("+{more_count}")
                            }),
                        ),
                )
                .child(sans(s, 12.5, 400.0, hex(0x9aa3b8)).child(if expanded {
                    "Show fewer"
                } else {
                    "More themes"
                })),
            "look-theme-more",
            interact::Ring::new(9.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            move |this, _, cx| {
                this.look.show_more_themes = !expanded;
                cx.notify();
            },
        ).into_any_element());

        // ---- Background segmented + Accent dots row ----
        let bg_bar = div()
            .flex()
            .gap(s.px(4.0))
            .p(s.px(3.0))
            .rounded(s.px(9.0))
            .bg(rgba(18, 24, 38, 0.72))
            .border_1()
            .border_color(rgba(160, 180, 255, 0.14))
            .children(
                [
                    ("art", "Art"),
                    ("live", "Live"),
                    ("transparent", "Glass"),
                    ("solid", "Solid"),
                ]
                .into_iter()
                .map(|(value, label_text)| {
                    let selected = current_bg == value;
                    let key = SharedString::from(format!("look-bg-{value}"));
                    self.control(
                        s,
                        div()
                            .id(key.clone())
                            .flex_1()
                            .h(s.px(30.0))
                            .rounded(s.px(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .when(selected, |this| {
                                this.bg(rgba(44, 56, 102, 0.85))
                                    .border_1()
                                    .border_color(rgba(124, 148, 232, 0.5))
                            })
                            .child(sans(
                                s,
                                13.0,
                                if selected { 600.0 } else { 400.0 },
                                if selected { hex(0xeef1f7) } else { hex(0x9aa3b8) },
                            )
                            .child(label_text)),
                        key,
                        interact::Ring::new(6.0, 1.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| this.pick_look_background(value, cx),
                    )
                }),
            );

        let current_accent = self
            .settings
            .theme_roles
            .get(appearance_key(light))
            .and_then(|obj| obj.get("accent"))
            .and_then(Value::as_str)
            .unwrap_or("#86d3f8")
            .to_string();
        let accent_dots = div().flex().items_center().gap(s.px(10.0)).h(s.px(36.0)).children(
            ACCENT_DOTS.iter().map(|&color_hex| {
                let selected = current_accent.eq_ignore_ascii_case(color_hex);
                let key = SharedString::from(format!("look-accent-{color_hex}"));
                let rgb = parse_rgb(color_hex, 0x86d3f8);
                self.control(
                    s,
                    div()
                        .id(key.clone())
                        .size(s.px(24.0))
                        .rounded_full()
                        .cursor_pointer()
                        .bg(hex(rgb))
                        .border_color(if selected {
                            hex(0xeef1f7)
                        } else {
                            white(0.18)
                        })
                        .when(selected, |this| this.border_2())
                        .when(!selected, |this| this.border_1()),
                    key,
                    interact::Ring::new(12.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| this.pick_look_accent(color_hex, cx),
                )
            }),
        );

        // ---- Optional compact Art or Live thumbnail picker ----
        let bg_thumbs: Option<AnyElement> = if current_bg == "art" {
            let current_art = if dark {
                self.settings.window_art.clone()
            } else {
                self.settings.window_art_light.clone()
            };
            let mut thumbs = Vec::new();
            for &art_id in &ART_STRIP_IDS {
                let selected = current_art == art_id;
                let pic = self.look_cached_image("theme-art", art_id, artwork_thumbnail_bytes);
                let key = SharedString::from(format!("look-art-{art_id}"));
                thumbs.push(self.control(
                    s,
                    div()
                        .id(key.clone())
                        .flex_1()
                        .h(s.px(40.0))
                        .rounded(s.px(7.0))
                        .overflow_hidden()
                        .cursor_pointer()
                        .bg(rgba(22, 27, 40, 0.9))
                        .border_color(if selected {
                            hex(0xeef1f7)
                        } else {
                            rgba(160, 180, 255, 0.16)
                        })
                        .when(selected, |this| this.border_2())
                        .when(!selected, |this| this.border_1())
                        .when_some(pic, |this, pic| {
                            this.child(img(pic).size_full().object_fit(ObjectFit::Cover))
                        }),
                    key,
                    interact::Ring::new(7.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| {
                        let art_key = if dark { "windowArt" } else { "windowArtLight" };
                        let mut patch = Map::new();
                        patch.insert(art_key.into(), json!(art_id));
                        patch.insert("windowGlass".into(), json!("frosted"));
                        this.update_settings(patch, cx);
                    },
                ));
            }
            Some(div().flex().gap(s.px(8.0)).children(thumbs).into_any_element())
        } else if current_bg == "live" {
            let live_key = if dark {
                "windowGlassLiveStyleDark"
            } else {
                "windowGlassLiveStyleLight"
            };
            let current_live = if dark {
                self.settings.window_glass_live_dark.clone()
            } else {
                self.settings.window_glass_live_light.clone()
            };
            let mut thumbs = Vec::new();
            for &(style_id, _name) in &LIVE_STRIP_IDS {
                let selected = current_live == style_id;
                let pic = self.look_cached_live(style_id, light);
                let key = SharedString::from(format!("look-live-{style_id}"));
                thumbs.push(self.control(
                    s,
                    div()
                        .id(key.clone())
                        .flex_1()
                        .h(s.px(40.0))
                        .rounded(s.px(7.0))
                        .overflow_hidden()
                        .cursor_pointer()
                        .bg(rgba(22, 27, 40, 0.9))
                        .border_color(if selected {
                            hex(0xeef1f7)
                        } else {
                            rgba(160, 180, 255, 0.16)
                        })
                        .when(selected, |this| this.border_2())
                        .when(!selected, |this| this.border_1())
                        .when_some(pic, |this, pic| {
                            this.child(img(pic).size_full().object_fit(ObjectFit::Cover))
                        }),
                    key,
                    interact::Ring::new(7.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| {
                        let mut patch = Map::new();
                        patch.insert(live_key.into(), json!(style_id));
                        patch.insert("windowGlassSource".into(), json!("live"));
                        patch.insert("windowGlass".into(), json!("frosted"));
                        this.update_settings(patch, cx);
                    },
                ));
            }
            Some(div().flex().gap(s.px(8.0)).children(thumbs).into_any_element())
        } else {
            None
        };

        // ---- 16 subtle-capped colour squares + uncapped Custom square ----
        let selected_preset = if dark {
            self.settings.dark_theme_preset.clone()
        } else {
            self.settings.light_theme_preset.clone()
        };
        let preset_list = if dark { &DARK_PRESETS } else { &LIGHT_PRESETS };
        let table_presets = if dark {
            self.theme.dark.clone()
        } else {
            self.theme.light.clone()
        };
        let cf_pos = colourfulness_display_step(self.settings.sidebar_contrast);
        let mut squares: Vec<AnyElement> = Vec::with_capacity(17);
        for &(preset_id, _preset_name) in preset_list {
            let is_selected = selected_preset == preset_id;
            let chrome = table_presets
                .iter()
                .find(|entry| entry.value == preset_id)
                .and_then(|entry| entry.chrome.get(cf_pos).or(entry.chrome.last()).copied())
                .unwrap_or(if dark { 0x1a1c22 } else { 0xe8eaef });
            let key = SharedString::from(format!("look-sq-{preset_id}"));
            let value = preset_id.to_string();
            squares.push(self.control(
                s,
                div()
                    .id(key.clone())
                    .size(s.px(32.0))
                    .rounded(s.px(7.0))
                    .cursor_pointer()
                    .bg(hex(chrome))
                    .border_color(if is_selected {
                        hex(0xeef1f7)
                    } else {
                        white(0.16)
                    })
                    .when(is_selected, |this| this.border_2())
                    .when(!is_selected, |this| this.border_1()),
                key,
                interact::Ring::new(7.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                move |this, _, cx| this.pick_look_square(dark, value.clone(), cx),
            ).into_any_element());
        }
        let custom_selected = selected_preset == "custom";
        let custom_hex = if dark {
            self.settings.custom_dark_tint.clone()
        } else {
            self.settings.custom_light_tint.clone()
        };
        let show_custom = self.look.show_custom_bar;
        squares.push(self.control(
            s,
            div()
                .id("look-sq-custom")
                .size(s.px(32.0))
                .rounded(s.px(7.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .when(custom_selected, |this| {
                    this.bg(hex(parse_rgb(&custom_hex, 0x1b5e3f)))
                        .border_2()
                        .border_color(hex(0xeef1f7))
                })
                .when(!custom_selected, |this| {
                    this.bg(rgba(18, 24, 38, 0.72))
                        .border_1()
                        .border_dashed()
                        .border_color(rgba(180, 195, 230, 0.45))
                })
                .child(sans(s, 17.0, 500.0, hex(0xeef1f7)).child("+")),
            "look-sq-custom",
            interact::Ring::new(7.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            move |this, _, cx| {
                this.look.show_custom_bar = !show_custom;
                cx.notify();
            },
        ).into_any_element());

        let custom_bar = self.look.show_custom_bar.then(|| {
            div()
                .flex()
                .items_center()
                .gap(s.px(8.0))
                .mt(s.px(8.0))
                .child(sans(s, 12.0, 400.0, hex(0x9aa3b8)).child("Custom (uncapped):"))
                .children(CUSTOM_SWATCHES.iter().map(|&hex_str| {
                    let key = SharedString::from(format!("look-custom-pick-{hex_str}"));
                    let rgb = parse_rgb(hex_str, 0x1b5e3f);
                    let selected = custom_selected && custom_hex.eq_ignore_ascii_case(hex_str);
                    self.control(
                        s,
                        div()
                            .id(key.clone())
                            .size(s.px(24.0))
                            .rounded(s.px(6.0))
                            .cursor_pointer()
                            .bg(hex(rgb))
                            .border_color(if selected {
                                hex(0xeef1f7)
                            } else {
                                white(0.22)
                            })
                            .when(selected, |this| this.border_2())
                            .when(!selected, |this| this.border_1()),
                        key,
                        interact::Ring::new(6.0, 1.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| this.pick_look_custom_hex(dark, hex_str, cx),
                    )
                }))
        });

        let colour_title = format!(
            "{} · {}",
            if dark {
                "Dark mode colour"
            } else {
                "Light mode colour"
            },
            theme_preset_label(dark, &selected_preset)
        );

        let controls_col = abs(s, left_x, 248.0, Some(left_w), Some(582.0))
            .flex()
            .flex_col()
            .gap(s.px(12.0))
            .child(appearance_bar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(s.px(8.0))
                    .child(label(s, "Theme"))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(s.px(12.0))
                            .children(theme_tiles),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(s.px(20.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(s.px(7.0))
                            .child(label(s, "Background"))
                            .child(bg_bar),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(s.px(7.0))
                            .child(label(s, "Accent"))
                            .child(accent_dots),
                    ),
            )
            .children(bg_thumbs)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(s.px(8.0))
                    .child(label(s, &colour_title))
                    .child(div().flex().flex_wrap().gap(s.px(9.0)).children(squares))
                    .children(custom_bar),
            );
        out.push(controls_col.into_any_element());

        // ---- Right column: Live Ghostex app preview ----
        out.push(self.render_live_app_preview(s, dark, current_bg));

        // ---- Footer: All options (queues Theme Editor) + Continue -> Panel 3 (Agents) ----
        let queued = self.flow.theme_editor_queued;
        let all_options = self.control(
            s,
            div()
                .id("look-all-options")
                .h(s.px(40.0))
                .px(s.px(14.0))
                .rounded(s.px(9.0))
                .flex()
                .items_center()
                .gap(s.px(7.0))
                .cursor_pointer()
                .when(queued, |this| {
                    this.bg(rgba(44, 56, 102, 0.55))
                        .border_1()
                        .border_color(rgba(124, 148, 232, 0.5))
                })
                .child(sans(
                    s,
                    14.5,
                    if queued { 500.0 } else { 400.0 },
                    if queued { hex(0xeef1f7) } else { hex(0x9aa3b8) },
                )
                .child(if queued {
                    "Theme editor after setup ✓"
                } else {
                    "All options"
                })),
            "look-all-options",
            interact::Ring::new(9.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                this.flow.theme_editor_queued = !this.flow.theme_editor_queued;
                if this.flow.theme_editor_queued {
                    this.show_toast("The compact Theme editor will open right after setup.", cx);
                }
                cx.notify();
            },
        );
        out.push(
            foot_actions(s, 2)
                .gap(s.px(14.0))
                .child(all_options)
                .child(self.control(
                    s,
                    cta(
                        s,
                        "look-continue",
                        "Continue",
                        true,
                        true,
                        false,
                        CtaSize::Foot,
                    ),
                    "look-continue",
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| this.go(3, cx),
                ))
                .into_any_element(),
        );
        out
    }

    fn render_live_app_preview(&mut self, s: S, dark: bool, bg_mode: &str) -> AnyElement {
        let light = !dark;
        let cf_pos = colourfulness_display_step(self.settings.sidebar_contrast);
        let preset_id = if dark {
            &self.settings.dark_theme_preset
        } else {
            &self.settings.light_theme_preset
        };
        let sb_rgb = if preset_id == "custom" {
            let custom = if dark {
                &self.settings.custom_dark_tint
            } else {
                &self.settings.custom_light_tint
            };
            parse_rgb(custom, if dark { 0x1b5e3f } else { 0xe8eaef })
        } else {
            let list = if dark {
                &self.theme.dark
            } else {
                &self.theme.light
            };
            list.iter()
                .find(|entry| &entry.value == preset_id)
                .and_then(|entry| entry.chrome.get(cf_pos).or(entry.chrome.last()).copied())
                .unwrap_or(if dark { 0x14161c } else { 0xeceef2 })
        };
        let wa_rgb = ghostex_settings_catalog::theme_colours::session_chat_background_for_chrome(
            sb_rgb,
        );
        let accent_hex = self
            .settings
            .theme_roles
            .get(appearance_key(light))
            .and_then(|obj| obj.get("accent"))
            .and_then(Value::as_str)
            .unwrap_or("#86d3f8");
        let accent_rgb = parse_rgb(accent_hex, 0x86d3f8);
        let solid = bg_mode == "solid" || self.settings.window_glass == "opaque";
        let sb_alpha = if solid { 1.0 } else { 0.84 };
        let wa_alpha = if solid { 1.0 } else { 0.78 };
        let fg = if dark { hex(0xe8ebf2) } else { hex(0x1f232b) };
        let muted = if dark { hex(0x98a0b2) } else { hex(0x5b6275) };
        let line = if dark { white(0.09) } else { black(0.11) };
        let bubble_bg = role_fill(&self.settings.theme_roles, light, "userMessage", wa_rgb, 0.12);
        let card_bg = role_fill(&self.settings.theme_roles, light, "surface", wa_rgb, 0.08);
        let box_bg = role_fill(&self.settings.theme_roles, light, "chatBox", wa_rgb, 0.06);
        let ctl_bg = role_fill(&self.settings.theme_roles, light, "control", wa_rgb, 0.10);

        let bg_image: Option<Arc<Image>> = match bg_mode {
            "art" => {
                let art_id = if dark {
                    self.settings.window_art.clone()
                } else {
                    self.settings.window_art_light.clone()
                };
                self.look_cached_image("theme-art", &art_id, artwork_thumbnail_bytes)
            }
            "live" => {
                let style = if dark {
                    self.settings.window_glass_live_dark.clone()
                } else {
                    self.settings.window_glass_live_light.clone()
                };
                self.look_cached_live(&style, light)
            }
            _ => None,
        };

        let preview_w = 740.0;
        let preview_h = 468.0;
        let sidebar_w = 218.0;
        let window_card = div()
            .relative()
            .w(s.px(preview_w))
            .h(s.px(preview_h))
            .rounded(s.px(14.0))
            .overflow_hidden()
            .border_1()
            .border_color(rgba(160, 180, 255, 0.20))
            .shadow(vec![shadow(black(0.45), 0.0, 18.0, 44.0, 0.0, s)])
            .bg(if bg_mode == "transparent" {
                gpui::linear_gradient(
                    135.0,
                    gpui::linear_color_stop(hex(0x2b3550), 0.0),
                    gpui::linear_color_stop(hex(0x4f3e52), 1.0),
                )
            } else {
                hex(wa_rgb).into()
            })
            // Artwork / Live layer behind the translucent sidebar and work area
            .when_some(bg_image, |this, pic| {
                this.child(
                    img(pic)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        .opacity(if bg_mode == "art" { 0.55 } else { 0.65 }),
                )
            })
            // Foreground sidebar + work area
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    // Sidebar
                    .child(
                        div()
                            .w(s.px(sidebar_w))
                            .h_full()
                            .flex()
                            .flex_col()
                            .bg(rgb_to_hsla(sb_rgb, sb_alpha))
                            .border_r_1()
                            .border_color(line)
                            .p(s.px(14.0))
                            .gap(s.px(8.0))
                            // Window traffic lights
                            .child(
                                div()
                                    .flex()
                                    .gap(s.px(6.0))
                                    .mb(s.px(6.0))
                                    .child(div().size(s.px(9.0)).rounded_full().bg(white(0.22)))
                                    .child(div().size(s.px(9.0)).rounded_full().bg(white(0.22)))
                                    .child(div().size(s.px(9.0)).rounded_full().bg(white(0.22))),
                            )
                            // New session button
                            .child(
                                div()
                                    .w_full()
                                    .h(s.px(30.0))
                                    .px(s.px(10.0))
                                    .rounded(s.px(7.0))
                                    .bg(ctl_bg)
                                    .border_1()
                                    .border_color(line)
                                    .flex()
                                    .items_center()
                                    .child(sans(s, 12.5, 500.0, fg).child("+ New session")),
                            )
                            .child(
                                div()
                                    .mt(s.px(6.0))
                                    .px(s.px(8.0))
                                    .py(s.px(6.0))
                                    .rounded(s.px(7.0))
                                    .bg(if dark { white(0.09) } else { black(0.07) })
                                    .child(sans(s, 12.5, 500.0, fg).child("Build floating editor")),
                            )
                            .child(
                                div()
                                    .px(s.px(8.0))
                                    .py(s.px(5.0))
                                    .child(sans(s, 12.0, 400.0, muted).child("Refine dark tints")),
                            )
                            .child(
                                div()
                                    .px(s.px(8.0))
                                    .py(s.px(5.0))
                                    .child(sans(s, 12.0, 400.0, muted).child("Update onboarding")),
                            ),
                    )
                    // Work area
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .bg(rgb_to_hsla(wa_rgb, wa_alpha))
                            // Top bar
                            .child(
                                div()
                                    .h(s.px(38.0))
                                    .px(s.px(16.0))
                                    .border_b_1()
                                    .border_color(line)
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(sans(s, 12.5, 500.0, fg).child("Ghostex"))
                                    .child(
                                        div()
                                            .px(s.px(9.0))
                                            .py(s.px(3.0))
                                            .rounded(s.px(6.0))
                                            .bg(ctl_bg)
                                            .child(sans(s, 11.5, 500.0, fg).child("Commit")),
                                    ),
                            )
                            // Chat messages + relative-fill card + composer
                            .child(
                                div()
                                    .flex_1()
                                    .p(s.px(20.0))
                                    .flex()
                                    .flex_col()
                                    .gap(s.px(14.0))
                                    // User bubble (relative fill userMessage)
                                    .child(
                                        div().w_full().flex().justify_end().child(
                                            div()
                                                .max_w(s.px(340.0))
                                                .px(s.px(13.0))
                                                .py(s.px(9.0))
                                                .rounded(s.px(12.0))
                                                .bg(bubble_bg)
                                                .border_1()
                                                .border_color(line)
                                                .child(
                                                    sans(s, 12.5, 400.0, fg)
                                                        .child("Make the window colours shift smoothly."),
                                                ),
                                        ),
                                    )
                                    // Assistant reply + card (relative fill surface)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(s.px(8.0))
                                            .child(
                                                sans(s, 12.5, 400.0, fg)
                                                    .child("Every fill now follows your chosen colour automatically."),
                                            )
                                            .child(
                                                div()
                                                    .w_full()
                                                    .p(s.px(12.0))
                                                    .rounded(s.px(10.0))
                                                    .bg(card_bg)
                                                    .border_1()
                                                    .border_color(line)
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .child(
                                                        sans(s, 12.0, 500.0, fg)
                                                            .child("Updated theme roles"),
                                                    )
                                                    .child(
                                                        div()
                                                            .px(s.px(8.0))
                                                            .py(s.px(3.0))
                                                            .rounded(s.px(5.0))
                                                            .bg(ctl_bg)
                                                            .child(
                                                                sans(s, 11.0, 500.0, hex(accent_rgb))
                                                                    .child("Ready"),
                                                            ),
                                                    ),
                                            ),
                                    )
                                    .child(div().flex_1())
                                    // Composer (relative fill chatBox + accent send button)
                                    .child(
                                        div()
                                            .w_full()
                                            .h(s.px(82.0))
                                            .p(s.px(12.0))
                                            .rounded(s.px(12.0))
                                            .bg(box_bg)
                                            .border_1()
                                            .border_color(line)
                                            .flex()
                                            .flex_col()
                                            .justify_between()
                                            .child(
                                                sans(s, 12.0, 400.0, muted)
                                                    .child("Ask an agent to build something..."),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .child(
                                                        sans(s, 11.5, 500.0, muted)
                                                            .child("Claude · Default"),
                                                    )
                                                    .child(
                                                        div()
                                                            .size(s.px(24.0))
                                                            .rounded(s.px(6.0))
                                                            .bg(hex(accent_rgb))
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .child(
                                                                sans(s, 12.5, 700.0, hex(0x111318))
                                                                    .child("↑"),
                                                            ),
                                                    ),
                                            ),
                                    ),
                            ),
                    ),
            );

        abs(s, 882.0, 180.0, Some(preview_w), None)
            .flex()
            .flex_col()
            .gap(s.px(14.0))
            .child(window_card)
            .child(
                sans(s, 13.5, 400.0, hex(0x9aa3b8))
                    .child("Your Ghostex, as you change it."),
            )
            .into_any_element()
    }
}
