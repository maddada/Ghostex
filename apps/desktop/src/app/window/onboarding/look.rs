//! Panel 2, "Make it yours" (Q4387: Second, right after Welcome).
//!
//! Aligned with Settings > Themes: Appearance (System / Light / Dark), Theme tiles (Default, Art
//! themes with Polar near the start, Agent-made looks, Presets), the single Artwork strip (None,
//! every artwork, Your image, Animated at the end) with its controls directly underneath, the slim
//! Window transparency card (Transparency, Show behind with Fully transparent / Wallpaper,
//! Strength, Blur, paused while an artwork is active), Accent colour dots, and the 16 subtle-capped
//! colour squares plus uncapped Custom colour, with a live Ghostex preview on the right half.
use super::super::native_modal_kit::{StripWheelLock, strip_wheel_layer};
use super::super::settings_modal::catalog::{module, settings_catalog};
use super::super::settings_modal::store::SettingsValues;
use super::super::settings_modal::tabs::theme::art::source_art;
use super::super::settings_modal::tabs::theme::artwork::{
    Backdrop, art_key, backdrop, file_preview, live_shows, painted_name, your_image_path,
};
use super::super::settings_modal::tabs::theme::colours::{
    STRENGTH_MAX, STRENGTH_MIN, STRENGTH_STEP, preview_colours, strength_from_settings,
    strength_gaps, strength_patch, swatch_paint,
};
use super::super::settings_modal::tabs::theme::gallery::poster as live_poster;
use super::super::settings_modal::tabs::theme::prompt_gallery::{
    GalleryTheme, applied_theme, artwork_thumbnail_bytes, gallery_themes, preview_bytes,
};
use super::super::settings_modal::tabs::theme::swatches::swatch_colour;
use super::super::settings_modal::tabs::theme::transparency::{
    SHOW_BEHIND, same_for_light_and_dark, transparency_choices,
};
use super::get_started::appearance_label;
use super::interact;
use super::model::{
    DARK_PRESETS, LIGHT_PRESETS, colourfulness_display_step, js_number, matching_preset,
    theme_preset_label,
};
use super::primitives::*;
use super::stage::*;
use super::welcome::foot_actions;
use super::{GpuiOnboardingWindow, OnboardingCommand};
use ghostex_settings_catalog::theme_roles::{
    RoleValue, THEME_ROLES_SETTING, appearance_key, parse_role_value,
};
use ghostex_settings_catalog::{ARTWORKS, YOUR_IMAGE_ART_ID};
use gpui::StyledImage as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, Image, ImageFormat, InteractiveElement as _, IntoElement,
    MouseButton, ObjectFit, ParentElement as _, Pixels, RenderImage, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, canvas, div, img, point, px,
};
use serde_json::{Map, Value, json};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

const ACCENT_DOTS: [&str; 7] = [
    "#86d3f8", "#7cb8f7", "#d8664f", "#7ecdc5", "#a7c080", "#d8a657", "#d69bb5",
];

const CUSTOM_SWATCHES: [&str; 6] = [
    "#1b5e3f", "#264e86", "#6b2d5c", "#7c4a1d", "#185e63", "#5c2735",
];

const LOOK_SLIDER_IDS: [&str; 6] = [
    "look-strength",
    "look-art-dim",
    "look-art-blur",
    "look-live-speed",
    "look-live-brightness",
    "look-glass-blur",
];

pub(crate) struct LookState {
    pub(crate) show_more_themes: bool,
    pub(crate) show_custom_bar: bool,
    pub(crate) picked_dark: bool,
    pub(crate) picked_light: bool,
    pub(crate) active_theme_id: Option<String>,
    pub(crate) posters: HashMap<String, Arc<Image>>,
    pub(crate) source_images: HashMap<String, Arc<RenderImage>>,
    pub(crate) scroll: ScrollHandle,
    pub(crate) artworks_scroll: ScrollHandle,
    pub(crate) artworks_wheel: StripWheelLock,
    pub(crate) artworks_revealed: Option<bool>,
    pub(crate) artworks_reveal: Rc<Cell<Option<usize>>>,
    pub(crate) slider_bounds: HashMap<&'static str, Rc<Cell<Option<Bounds<Pixels>>>>>,
    pub(crate) dragging: Option<&'static str>,
}

impl Default for LookState {
    fn default() -> Self {
        Self {
            show_more_themes: false,
            show_custom_bar: false,
            picked_dark: false,
            picked_light: false,
            active_theme_id: None,
            posters: HashMap::new(),
            source_images: HashMap::new(),
            scroll: ScrollHandle::new(),
            artworks_scroll: ScrollHandle::new(),
            artworks_wheel: StripWheelLock::default(),
            artworks_revealed: None,
            artworks_reveal: Rc::new(Cell::new(None)),
            slider_bounds: HashMap::new(),
            dragging: None,
        }
    }
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

fn role_fill(
    roles: &Value,
    light: bool,
    id: &str,
    base_rgb: u32,
    fallback: RoleValue,
) -> gpui::Hsla {
    let val = roles
        .get(appearance_key(light))
        .and_then(|obj| obj.get(id))
        .and_then(Value::as_str)
        .and_then(|raw| parse_role_value(raw).ok())
        .unwrap_or(fallback);
    let [r, g, b, a] = val.rgba_on_base(light, base_rgb);
    gpui::Hsla::from(gpui::Rgba { r, g, b, a })
}

fn catalog_number(name: &str) -> f64 {
    settings_catalog().number(module::SETTINGS, name)
}

fn slider_spec(id: &'static str) -> (f64, f64, f64) {
    match id {
        "look-strength" => (STRENGTH_MIN, STRENGTH_MAX, STRENGTH_STEP),
        "look-art-dim" => (
            catalog_number("MIN_WINDOW_ART_DIM"),
            catalog_number("MAX_WINDOW_ART_DIM"),
            1.0,
        ),
        "look-art-blur" => (
            catalog_number("MIN_WINDOW_ART_BLUR"),
            catalog_number("MAX_WINDOW_ART_BLUR"),
            1.0,
        ),
        "look-live-speed" => (
            catalog_number("MIN_WINDOW_GLASS_LIVE_SPEED"),
            catalog_number("MAX_WINDOW_GLASS_LIVE_SPEED"),
            0.25,
        ),
        "look-live-brightness" => (
            catalog_number("MIN_WINDOW_GLASS_LIVE_BRIGHTNESS"),
            catalog_number("MAX_WINDOW_GLASS_LIVE_BRIGHTNESS"),
            1.0,
        ),
        _ => (
            catalog_number("MIN_WINDOW_GLASS_BLUR_RADIUS"),
            catalog_number("MAX_WINDOW_GLASS_BLUR_RADIUS"),
            1.0,
        ),
    }
}

fn snap_slider(value: f64, min: f64, max: f64, step: f64) -> f64 {
    let snapped = ((value - min) / step).round() * step + min;
    snapped.clamp(min, max)
}

impl GpuiOnboardingWindow {
    fn look_is_dark(&self) -> bool {
        match self.settings.sidebar_theme.as_str() {
            "system" => !self.system_light,
            theme => theme.starts_with("dark"),
        }
    }

    fn detect_onboarding_active_theme_id(
        &self,
        themes: &[GalleryTheme],
        values: &SettingsValues,
    ) -> Option<String> {
        if let Some((theme, _, _)) = applied_theme(themes, values) {
            return Some(theme.id.clone());
        }
        if let Some(active) = &self.look.active_theme_id {
            return Some(active.clone());
        }
        let dark = self.look_is_dark();
        let art = if dark {
            self.settings.window_art.as_str()
        } else {
            self.settings.window_art_light.as_str()
        };
        if let Some(art_theme) = themes
            .iter()
            .find(|theme| theme.kind == "art" && theme.id == art)
        {
            return Some(art_theme.id.clone());
        }
        let custom_tint = if dark {
            self.settings.custom_dark_tint.trim().to_lowercase()
        } else {
            self.settings.custom_light_tint.trim().to_lowercase()
        };
        let preset_val = if dark {
            self.settings.dark_theme_preset.as_str()
        } else {
            self.settings.light_theme_preset.as_str()
        };
        if preset_val == "custom" {
            let tint_key = if dark {
                "customSidebarTitlebarBackgroundTintColor"
            } else {
                "customSidebarTitlebarLightBackgroundTintColor"
            };
            if let Some(matched) = themes.iter().find(|theme| {
                theme.id != "default"
                    && theme
                        .changes
                        .get(tint_key)
                        .and_then(Value::as_str)
                        .is_some_and(|hex| hex.eq_ignore_ascii_case(&custom_tint))
            }) {
                return Some(matched.id.clone());
            }
        }
        if preset_val == "gray" && self.settings.window_art == "none" {
            return Some("default".to_string());
        }
        None
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
        let style = if style == "video" { "ink" } else { style };
        let key = format!("live-{style}-{}", if light { "light" } else { "dark" });
        if let Some(image) = self.look.posters.get(&key) {
            return Some(image.clone());
        }
        let image = live_poster(style, light)?;
        self.look.posters.insert(key, image.clone());
        Some(image)
    }

    fn look_cached_source_art(&mut self, source: &str) -> Option<Arc<RenderImage>> {
        if let Some(image) = self.look.source_images.get(source) {
            return Some(image.clone());
        }
        let image = source_art(source)?;
        self.look
            .source_images
            .insert(source.to_string(), image.clone());
        Some(image)
    }

    pub(super) fn apply_onboarding_gallery_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(theme) = gallery_themes().into_iter().find(|theme| theme.id == id) else {
            return;
        };
        let mut changes = theme.changes.clone();
        let partner = theme.kind == "preset" && (!theme.dark || !self.look.picked_light);
        if partner {
            changes.extend(theme.partner_changes.clone());
        }
        if let Some(background) = &theme.default_background {
            changes.extend(theme.background_patch(background, partner));
        }
        if theme.kind == "preset" {
            if theme.dark {
                self.look.picked_dark = true;
            } else {
                self.look.picked_light = true;
            }
        } else if !self.look_is_dark() {
            changes.insert("sidebarTheme".into(), json!("dark-2"));
        }
        let mut patch = Map::new();
        // Reset previous themes' role overrides so switching themes never leaks stale role keys.
        let prev_roles_non_empty = self
            .settings
            .theme_roles
            .as_object()
            .is_some_and(|obj| !obj.is_empty());
        let mut roles = Map::new();
        let mut touched_roles = prev_roles_non_empty;
        for (key, value) in changes {
            if let Some((mode @ ("dark" | "light"), role_id)) = key.split_once('.') {
                touched_roles = true;
                if value.as_str() == Some("default") {
                    continue;
                }
                let obj = roles
                    .entry(mode.to_string())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(map) = obj.as_object_mut() {
                    map.insert(role_id.to_string(), value);
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
                    if matches!(default, ghostex_settings_catalog::json::J::Bool(_))
                        && let Ok(flag) = text.parse::<bool>()
                    {
                        patch.insert(key, json!(flag));
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
        self.look.artworks_revealed = None;
        self.update_settings(patch, cx);
    }

    fn pick_look_no_artwork(
        &mut self,
        dark: bool,
        enable_transparency: bool,
        cx: &mut Context<Self>,
    ) {
        let values = self.settings.to_settings_values();
        let mut patch = Map::new();
        patch.insert(art_key(dark).into(), json!("none"));
        if live_shows(&values, dark) {
            patch.insert("windowGlassSource".into(), json!("desktopAndWindows"));
        }
        if enable_transparency {
            let glass = self.settings.window_glass.as_str();
            if glass == "opaque" {
                patch.insert(
                    "windowGlass".into(),
                    json!(if dark { "auto" } else { "frosted" }),
                );
            } else if glass == "auto" && !dark {
                patch.insert("windowGlass".into(), json!("frosted"));
            }
        }
        self.update_settings(patch, cx);
    }

    fn pick_look_artwork(&mut self, dark: bool, id: &str, cx: &mut Context<Self>) {
        let mut patch = Map::new();
        patch.insert(art_key(dark).into(), json!(id));
        self.update_settings(patch, cx);
    }

    fn pick_look_your_image(&mut self, dark: bool, change: bool, cx: &mut Context<Self>) {
        let values = self.settings.to_settings_values();
        if change || your_image_path(&values, dark).is_none() {
            (self.host)(OnboardingCommand::PickWindowArtImage { light: !dark }, cx);
            return;
        }
        let mut patch = Map::new();
        patch.insert(art_key(dark).into(), json!(YOUR_IMAGE_ART_ID));
        self.update_settings(patch, cx);
    }

    fn pick_look_live(&mut self, dark: bool, cx: &mut Context<Self>) {
        let values = self.settings.to_settings_values();
        let glass = values.string("windowGlass");
        let mut patch = Map::new();
        if values.string(art_key(dark)) != "none" {
            patch.insert(art_key(dark).into(), json!("none"));
        }
        patch.insert("windowGlassSource".into(), json!("live"));
        if glass == "opaque" {
            patch.insert(
                "windowGlass".into(),
                json!(if dark { "auto" } else { "frosted" }),
            );
        } else if glass == "auto" && !dark {
            patch.insert("windowGlass".into(), json!("frosted"));
        }
        self.update_settings(patch, cx);
    }

    fn set_look_slider(&mut self, id: &'static str, raw: f64, cx: &mut Context<Self>) {
        let (min, max, step) = slider_spec(id);
        let value = snap_slider(raw, min, max, step);
        match id {
            "look-strength" => {
                let values = self.settings.to_settings_values();
                if strength_from_settings(&values).0 == Some(value) {
                    return;
                }
                self.update_settings(strength_patch(value, strength_gaps(&values)), cx);
            }
            "look-art-dim" => {
                if (self.settings.window_art_dim - value).abs() < 1e-6 {
                    return;
                }
                let mut patch = Map::new();
                patch.insert("windowArtDim".into(), js_number(value));
                self.update_settings(patch, cx);
            }
            "look-art-blur" => {
                if (self.settings.window_art_blur - value).abs() < 1e-6 {
                    return;
                }
                let mut patch = Map::new();
                patch.insert("windowArtBlur".into(), js_number(value));
                self.update_settings(patch, cx);
            }
            "look-live-speed" => {
                if (self.settings.window_glass_live_speed - value).abs() < 1e-6 {
                    return;
                }
                let mut patch = Map::new();
                patch.insert("windowGlassLiveSpeed".into(), js_number(value));
                self.update_settings(patch, cx);
            }
            "look-live-brightness" => {
                if (self.settings.window_glass_live_brightness - value).abs() < 1e-6 {
                    return;
                }
                let mut patch = Map::new();
                patch.insert("windowGlassLiveBrightness".into(), js_number(value));
                self.update_settings(patch, cx);
            }
            "look-glass-blur" => {
                if (self.settings.window_glass_blur_radius - value).abs() < 1e-6 {
                    return;
                }
                let mut patch = Map::new();
                patch.insert("windowGlassBlurRadius".into(), js_number(value));
                self.update_settings(patch, cx);
            }
            _ => {}
        }
    }

    pub(super) fn look_slider_key(&mut self, right: bool, cx: &mut Context<Self>) -> bool {
        if self.panel != 2 {
            return false;
        }
        let Some(&id) = LOOK_SLIDER_IDS
            .iter()
            .find(|&&id| self.focus.is_focused(&format!("slider-{id}")))
        else {
            return false;
        };
        let (min, max, step) = slider_spec(id);
        let current = match id {
            "look-strength" => strength_from_settings(&self.settings.to_settings_values()).1,
            "look-art-dim" => self.settings.window_art_dim,
            "look-art-blur" => self.settings.window_art_blur,
            "look-live-speed" => self.settings.window_glass_live_speed,
            "look-live-brightness" => self.settings.window_glass_live_brightness,
            _ => self.settings.window_glass_blur_radius,
        };
        let delta = if right { step } else { -step };
        self.set_look_slider(id, (current + delta).clamp(min, max), cx);
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn look_slider_row(
        &mut self,
        s: S,
        id: &'static str,
        name: &str,
        value: f64,
        value_text: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (min, max, _step) = slider_spec(id);
        let span = (max - min).max(1e-6);
        let fraction = ((value - min) / span).clamp(0.0, 1.0) as f32;
        let bounds = self
            .look
            .slider_bounds
            .entry(id)
            .or_insert_with(|| Rc::new(Cell::new(None)))
            .clone();
        let bounds_for_paint = bounds.clone();
        let value_at = move |x: Pixels| -> Option<f64> {
            let bounds = bounds.get()?;
            let left = bounds.left().as_f32();
            let width = bounds.size.width.as_f32().max(1.0);
            let frac = ((x.as_f32() - left) / width).clamp(0.0, 1.0) as f64;
            Some(min + (max - min) * frac)
        };
        let value_down = value_at.clone();
        let value_move = value_at;
        let key = SharedString::from(format!("slider-{id}"));
        let focused = self.focus.is_visible(&key);
        let track = div()
            .id(key.clone())
            .relative()
            .flex_1()
            .min_w_0()
            .h(s.px(16.0))
            .cursor_pointer()
            .child(
                canvas(
                    move |bounds, _, _| bounds_for_paint.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .left_0()
                .top_0()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(s.px(6.0))
                    .h(s.px(4.0))
                    .rounded(s.px(2.0))
                    .bg(white(0.12)),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(s.px(6.0))
                    .w(gpui::relative(fraction))
                    .h(s.px(4.0))
                    .rounded(s.px(2.0))
                    .bg(rgba(124, 184, 247, 0.72)),
            )
            .child(
                div()
                    .absolute()
                    .top(s.px(1.0))
                    .left(gpui::relative(fraction))
                    .ml(s.px(-7.0 * (2.0 * fraction)))
                    .size(s.px(14.0))
                    .rounded_full()
                    .bg(hex(0xeef1f7))
                    .shadow(vec![shadow(
                        rgba(124, 184, 247, if focused { 0.55 } else { 0.25 }),
                        0.0,
                        0.0,
                        0.0,
                        3.0,
                        s,
                    )]),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    this.look.dragging = Some(id);
                    if let Some(next) = value_down(event.position.x) {
                        this.set_look_slider(id, next, cx);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                    if this.look.dragging != Some(id) || !event.dragging() {
                        return;
                    }
                    if let Some(next) = value_move(event.position.x) {
                        this.set_look_slider(id, next, cx);
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.look.dragging = None;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.look.dragging = None;
                    cx.notify();
                }),
            );
        let track = self.focusable(track, key, cx);
        div()
            .flex()
            .items_center()
            .gap(s.px(10.0))
            .h(s.px(26.0))
            .child(
                sans(s, 12.5, 400.0, hex(0xd5dce8))
                    .w(s.px(88.0))
                    .flex_none()
                    .child(name.to_string()),
            )
            .child(track)
            .child(
                sans(s, 12.0, 400.0, hex(0x9aa3b8))
                    .w(s.px(44.0))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(value_text),
            )
            .into_any_element()
    }

    fn pick_look_square(&mut self, dark: bool, preset: String, cx: &mut Context<Self>) {
        let mut patch = Map::new();
        let (key, other_key, other_picked) = if dark {
            (
                "darkThemePreset",
                "lightThemePreset",
                self.look.picked_light,
            )
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

    pub(super) fn render_look(
        &mut self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let dark = self.look_is_dark();
        let light = !dark;
        let values = self.settings.to_settings_values();
        let picked = backdrop(&values, dark);
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
                .child("Pick a theme, artwork or transparency, and your colours. You can change any of it later.")
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
                [
                    ("system", "System"),
                    ("plain-light", "Light"),
                    ("dark-2", "Dark"),
                ]
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
                            .h(s.px(30.0))
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
                            .child(
                                sans(
                                    s,
                                    13.0,
                                    if selected { 600.0 } else { 400.0 },
                                    if selected {
                                        hex(0xeef1f7)
                                    } else {
                                        hex(0x9aa3b8)
                                    },
                                )
                                .child(label_text),
                            ),
                        key,
                        interact::Ring::new(7.0, 1.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| {
                            let mut patch = Map::new();
                            patch.insert("sidebarTheme".into(), json!(value));
                            this.look.artworks_revealed = None;
                            this.update_settings(patch, cx);
                        },
                    )
                }),
            );

        // ---- Theme tiles (4 columns, 7 initial themes + "+N More themes" expanding all) ----
        const INITIAL_TILES: usize = 7;
        let active_theme_id = self.detect_onboarding_active_theme_id(&themes, &values);
        let expanded = self.look.show_more_themes;
        let tile_limit = if expanded {
            themes.len()
        } else {
            INITIAL_TILES.min(themes.len())
        };
        let remaining = themes.len().saturating_sub(INITIAL_TILES);
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
            let selected = active_theme_id.as_deref() == Some(id.as_str());
            let pic = self.look_cached_image("theme-preview", &id, preview_bytes);
            let key = SharedString::from(format!("look-theme-{id}"));
            theme_tiles.push(
                self.control(
                    s,
                    div()
                        .id(key.clone())
                        .w(s.px(174.0))
                        .flex()
                        .flex_col()
                        .gap(s.px(4.0))
                        .cursor_pointer()
                        .child(
                            div()
                                .w_full()
                                .h(s.px(56.0))
                                .rounded(s.px(8.0))
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
                                12.0,
                                if selected { 600.0 } else { 400.0 },
                                if selected {
                                    hex(0xeef1f7)
                                } else {
                                    hex(0x9aa3b8)
                                },
                            )
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(short),
                        ),
                    key,
                    interact::Ring::new(8.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| this.apply_onboarding_gallery_theme(&id, cx),
                )
                .into_any_element(),
            );
        }
        if remaining > 0 {
            theme_tiles.push(
                self.control(
                    s,
                    div()
                        .id("look-theme-more")
                        .w(s.px(174.0))
                        .flex()
                        .flex_col()
                        .gap(s.px(4.0))
                        .cursor_pointer()
                        .child(
                            div()
                                .w_full()
                                .h(s.px(56.0))
                                .rounded(s.px(8.0))
                                .bg(rgba(27, 35, 64, 0.78))
                                .border_1()
                                .border_color(rgba(160, 180, 255, 0.22))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(sans(s, 14.5, 600.0, hex(0xdbe2f5)).child(if expanded {
                                    "Fewer".to_string()
                                } else {
                                    format!("+{remaining}")
                                })),
                        )
                        .child(sans(s, 12.0, 400.0, hex(0x9aa3b8)).child(if expanded {
                            "Show fewer"
                        } else {
                            "More themes"
                        })),
                    "look-theme-more",
                    interact::Ring::new(8.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| {
                        this.look.show_more_themes = !expanded;
                        cx.notify();
                    },
                )
                .into_any_element(),
            );
        }

        // ---- Artwork strip: None, 32 artworks, Your image, Animated (reuses Settings' catalog & helpers) ----
        let artwork_section =
            self.render_look_artwork_section(s, now, dark, &values, &picked, &themes, cx);

        // ---- Slim Window transparency card ----
        let transparency_section =
            self.render_look_transparency_section(s, dark, &values, &picked, cx);

        // ---- Accent dots + 16 colour squares + Custom ----
        let current_accent = self
            .settings
            .theme_roles
            .get(appearance_key(light))
            .and_then(|obj| obj.get("accent"))
            .and_then(Value::as_str)
            .unwrap_or("#86d3f8")
            .to_string();
        let accent_dots =
            div()
                .flex()
                .items_center()
                .gap(s.px(10.0))
                .children(ACCENT_DOTS.iter().map(|&color_hex| {
                    let selected = current_accent.eq_ignore_ascii_case(color_hex);
                    let key = SharedString::from(format!("look-accent-{color_hex}"));
                    let rgb = parse_rgb(color_hex, 0x86d3f8);
                    self.control(
                        s,
                        div()
                            .id(key.clone())
                            .size(s.px(22.0))
                            .rounded_full()
                            .cursor_pointer()
                            .bg(hex(rgb))
                            .border_color(if selected { hex(0xeef1f7) } else { white(0.18) })
                            .when(selected, |this| this.border_2())
                            .when(!selected, |this| this.border_1()),
                        key,
                        interact::Ring::new(11.0, 1.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| this.pick_look_accent(color_hex, cx),
                    )
                }));

        let selected_preset = if dark {
            self.settings.dark_theme_preset.clone()
        } else {
            self.settings.light_theme_preset.clone()
        };
        let preset_list = if dark { &DARK_PRESETS } else { &LIGHT_PRESETS };
        let cf_pos = colourfulness_display_step(self.settings.sidebar_contrast);
        let mut squares: Vec<AnyElement> = Vec::with_capacity(17);
        for &(preset_id, _preset_name) in preset_list {
            let is_selected = selected_preset == preset_id;
            let colour = swatch_colour(&swatch_paint(dark, preset_id, cf_pos));
            let key = SharedString::from(format!("look-sq-{preset_id}"));
            let value = preset_id.to_string();
            squares.push(
                self.control(
                    s,
                    div()
                        .id(key.clone())
                        .size(s.px(30.0))
                        .rounded(s.px(7.0))
                        .cursor_pointer()
                        .bg(gpui::Hsla::from(colour.gpui()))
                        .border_color(if is_selected {
                            hex(0xeef1f7)
                        } else {
                            white(0.18)
                        })
                        .when(is_selected, |this| this.border_2())
                        .when(!is_selected, |this| this.border_1()),
                    key,
                    interact::Ring::new(7.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| this.pick_look_square(dark, value.clone(), cx),
                )
                .into_any_element(),
            );
        }
        let custom_selected = selected_preset == "custom";
        let custom_hex = if dark {
            self.settings.custom_dark_tint.clone()
        } else {
            self.settings.custom_light_tint.clone()
        };
        let show_custom = self.look.show_custom_bar;
        squares.push(
            self.control(
                s,
                div()
                    .id("look-sq-custom")
                    .size(s.px(30.0))
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
                    .child(sans(s, 16.0, 500.0, hex(0xeef1f7)).child("+")),
                "look-sq-custom",
                interact::Ring::new(7.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                move |this, _, cx| {
                    this.look.show_custom_bar = !show_custom;
                    cx.notify();
                },
            )
            .into_any_element(),
        );

        let custom_bar = self.look.show_custom_bar.then(|| {
            div()
                .flex()
                .items_center()
                .gap(s.px(8.0))
                .mt(s.px(6.0))
                .child(sans(s, 12.0, 400.0, hex(0x9aa3b8)).child("Custom (uncapped):"))
                .children(CUSTOM_SWATCHES.iter().map(|&hex_str| {
                    let key = SharedString::from(format!("look-custom-pick-{hex_str}"));
                    let rgb = parse_rgb(hex_str, 0x1b5e3f);
                    let selected = custom_selected && custom_hex.eq_ignore_ascii_case(hex_str);
                    self.control(
                        s,
                        div()
                            .id(key.clone())
                            .size(s.px(22.0))
                            .rounded(s.px(6.0))
                            .cursor_pointer()
                            .bg(hex(rgb))
                            .border_color(if selected { hex(0xeef1f7) } else { white(0.22) })
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

        let controls_col = abs(s, left_x, 244.0, Some(left_w), Some(590.0))
            .id("look-controls-scroll")
            .overflow_y_scroll()
            .track_scroll(&self.look.scroll)
            .pr(s.px(6.0))
            .pb(s.px(16.0))
            .flex()
            .flex_col()
            .gap(s.px(11.0))
            .child(appearance_bar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(s.px(7.0))
                    .child(label(s, "Theme"))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(s.px(10.0))
                            .children(theme_tiles),
                    ),
            )
            .child(artwork_section)
            .child(transparency_section)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(s.px(12.0))
                    .child(label(s, "Accent"))
                    .child(accent_dots),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(s.px(7.0))
                    .child(label(s, &colour_title))
                    .child(div().flex().flex_wrap().gap(s.px(8.0)).children(squares))
                    .children(custom_bar),
            );
        out.push(controls_col.into_any_element());

        // ---- Right column: Live Ghostex app preview ----
        out.push(self.render_live_app_preview(s, dark, &values, &picked));

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
                .gap(s.px(8.0))
                .cursor_pointer()
                .border_1()
                .when(queued, |this| {
                    this.bg(rgba(44, 56, 102, 0.58))
                        .border_color(rgba(124, 148, 232, 0.58))
                })
                .when(!queued, |this| {
                    this.bg(rgba(22, 29, 46, 0.65))
                        .border_color(rgba(160, 180, 255, 0.24))
                })
                .child(
                    div()
                        .size(s.px(15.0))
                        .rounded(s.px(4.0))
                        .border_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(queued, |this| {
                            this.bg(hex(0x86d3f8))
                                .border_color(hex(0x86d3f8))
                                .child(sans(s, 10.5, 700.0, hex(0x0f1420)).child("✓"))
                        })
                        .when(!queued, |this| this.border_color(rgba(180, 198, 240, 0.45))),
                )
                .child(
                    sans(
                        s,
                        14.0,
                        if queued { 500.0 } else { 400.0 },
                        if queued { hex(0xeef1f7) } else { hex(0xc9d2e6) },
                    )
                    .child(if queued {
                        "Theme editor after setup"
                    } else {
                        "All options"
                    }),
                ),
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

    #[allow(clippy::too_many_arguments)]
    fn render_look_artwork_tile(
        &self,
        s: S,
        key: SharedString,
        title: String,
        selected: bool,
        preview: AnyElement,
        cx: &mut Context<Self>,
        run: impl Fn(&mut Self, &mut gpui::Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        self.control(
            s,
            div()
                .id(key.clone())
                .w(s.px(88.0))
                .flex_none()
                .flex()
                .flex_col()
                .gap(s.px(4.0))
                .cursor_pointer()
                .child(
                    div()
                        .w_full()
                        .h(s.px(46.0))
                        .rounded(s.px(7.0))
                        .overflow_hidden()
                        .bg(rgba(22, 27, 40, 0.9))
                        .border_color(if selected {
                            hex(0xeef1f7)
                        } else {
                            rgba(160, 180, 255, 0.16)
                        })
                        .when(selected, |this| this.border_2())
                        .when(!selected, |this| this.border_1())
                        .child(preview),
                )
                .child(
                    sans(
                        s,
                        11.5,
                        if selected { 600.0 } else { 400.0 },
                        if selected {
                            hex(0xeef1f7)
                        } else {
                            hex(0x9aa3b8)
                        },
                    )
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(title),
                ),
            key,
            interact::Ring::new(7.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            run,
        )
        .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_look_artwork_section(
        &mut self,
        s: S,
        now: Instant,
        dark: bool,
        values: &SettingsValues,
        picked: &Backdrop,
        themes: &[GalleryTheme],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let light = !dark;
        let labels: Vec<(String, String)> = themes
            .iter()
            .map(|theme| {
                let short = theme.label.clone().unwrap_or_else(|| {
                    theme
                        .name
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string()
                });
                (theme.id.clone(), short)
            })
            .collect();

        let mut tiles: Vec<AnyElement> = Vec::with_capacity(ARTWORKS.len() + 3);
        tiles.push(
            self.render_look_artwork_tile(
                s,
                "look-artwork-none".into(),
                "None".into(),
                *picked == Backdrop::None,
                div()
                    .size_full()
                    .bg(rgba(28, 34, 52, 0.72))
                    .into_any_element(),
                cx,
                move |this, _, cx| this.pick_look_no_artwork(dark, false, cx),
            ),
        );
        for artwork in ARTWORKS {
            let id = artwork.id;
            let short = labels
                .iter()
                .find(|(theme_id, _)| theme_id == id)
                .map(|(_, label)| label.clone())
                .unwrap_or_else(|| artwork.name.to_string());
            let pic = self.look_cached_image("theme-art", id, artwork_thumbnail_bytes);
            let preview = div()
                .size_full()
                .when_some(pic, |this, pic| {
                    this.child(img(pic).size_full().object_fit(ObjectFit::Cover))
                })
                .into_any_element();
            tiles.push(self.render_look_artwork_tile(
                s,
                SharedString::from(format!("look-artwork-{id}")),
                short,
                *picked == Backdrop::Artwork(id),
                preview,
                cx,
                move |this, _, cx| this.pick_look_artwork(dark, id, cx),
            ));
        }

        // Your image…
        let path = your_image_path(values, dark);
        let yours_picked = *picked == Backdrop::YourImage;
        let yours_preview = match path.clone() {
            Some(path) => file_preview(path, false),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(28, 34, 52, 0.72))
                .child(sans(s, 17.0, 500.0, hex(0x9aa3b8)).child("+"))
                .into_any_element(),
        };
        tiles.push(self.render_look_artwork_tile(
            s,
            "look-artwork-yours".into(),
            "Your image".into(),
            yours_picked,
            yours_preview,
            cx,
            move |this, _, cx| this.pick_look_your_image(dark, yours_picked, cx),
        ));

        // Animated at the end of the Artwork strip.
        let active_style = values.string(if dark || values.string("windowGlass") == "auto" {
            "windowGlassLiveStyleDark"
        } else {
            "windowGlassLiveStyleLight"
        });
        let live_pic = self.look_cached_live(&active_style, light);
        let live_preview = div()
            .size_full()
            .when_some(live_pic, |this, pic| {
                this.child(img(pic).size_full().object_fit(ObjectFit::Cover))
            })
            .into_any_element();
        tiles.push(self.render_look_artwork_tile(
            s,
            "look-artwork-live".into(),
            "Animated".into(),
            *picked == Backdrop::Live,
            live_preview,
            cx,
            move |this, _, cx| this.pick_look_live(dark, cx),
        ));

        let picked_slot = match picked {
            Backdrop::None => None,
            Backdrop::Artwork(id) => ARTWORKS
                .iter()
                .position(|artwork| artwork.id == *id)
                .map(|index| index + 1),
            Backdrop::YourImage => Some(ARTWORKS.len() + 1),
            Backdrop::Live => Some(ARTWORKS.len() + 2),
        };
        if let Some(slot) = picked_slot
            && self.look.artworks_revealed != Some(dark)
        {
            self.look.artworks_revealed = Some(dark);
            self.look.artworks_reveal.set(Some(slot));
        }
        let scroll = self.look.artworks_scroll.clone();
        let wheel = strip_wheel_layer(&scroll, &self.look.artworks_wheel);
        let reveal = self.look.artworks_reveal.clone();
        let measure = scroll.clone();
        let entity = cx.entity_id();

        let strip = div()
            .relative()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .id("look-artworks-strip")
                    .w_full()
                    .flex()
                    .items_start()
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&scroll)
                    .gap(s.px(8.0))
                    .pb(s.px(2.0))
                    .children(tiles),
            )
            .child(wheel)
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, _, cx| {
                        if let Some(slot) = reveal.get()
                            && let Some(tile) = measure.bounds_for_item(slot)
                        {
                            reveal.set(None);
                            let view = measure.bounds();
                            let centre = tile.left() - view.left() + tile.size.width / 2.0;
                            let target = (view.size.width / 2.0 - centre)
                                .clamp(-measure.max_offset().x, px(0.0));
                            measure.set_offset(point(target, measure.offset().y));
                            cx.notify(entity);
                        }
                    },
                )
                .absolute()
                .size_0(),
            );

        // Controls directly under the Artwork strip for whichever tile is picked.
        let controls: Option<AnyElement> = match picked {
            Backdrop::Artwork(_) | Backdrop::YourImage => {
                let (exact, nearest) = strength_from_settings(values);
                let strength_label = if exact.is_none() {
                    "Custom".to_string()
                } else {
                    format!("{}%", nearest.round() as i64)
                };
                let dim_label = format!("{}%", self.settings.window_art_dim.round() as i64);
                let blur_label = format!("{} pt", self.settings.window_art_blur.round() as i64);
                let no_pic = (*picked == Backdrop::YourImage && path.is_none()).then(|| {
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(s.px(10.0))
                        .mb(s.px(4.0))
                        .child(
                            sans(s, 12.0, 400.0, hex(0x9aa3b8))
                                .child("No picture chosen yet, so the window stays solid."),
                        )
                        .child(
                            self.control(
                                s,
                                div()
                                    .id("look-artwork-choose-pic")
                                    .px(s.px(10.0))
                                    .py(s.px(5.0))
                                    .rounded(s.px(7.0))
                                    .bg(rgba(44, 56, 102, 0.7))
                                    .border_1()
                                    .border_color(rgba(124, 148, 232, 0.45))
                                    .cursor_pointer()
                                    .child(
                                        sans(s, 12.0, 500.0, hex(0xeef1f7))
                                            .child("Choose picture…"),
                                    ),
                                "look-artwork-choose-pic",
                                interact::Ring::new(7.0, 1.0),
                                interact::Keys::EnterSpace,
                                cx,
                                move |this, _, cx| this.pick_look_your_image(dark, true, cx),
                            ),
                        )
                });
                let strength_row = self.look_slider_row(
                    s,
                    "look-strength",
                    "Strength",
                    nearest,
                    strength_label,
                    cx,
                );
                let dim_row = self.look_slider_row(
                    s,
                    "look-art-dim",
                    "Dimming",
                    self.settings.window_art_dim,
                    dim_label,
                    cx,
                );
                let blur_row = self.look_slider_row(
                    s,
                    "look-art-blur",
                    "Blur",
                    self.settings.window_art_blur,
                    blur_label,
                    cx,
                );
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(s.px(3.0))
                        .px(s.px(12.0))
                        .py(s.px(8.0))
                        .rounded(s.px(10.0))
                        .bg(rgba(18, 24, 38, 0.62))
                        .border_1()
                        .border_color(rgba(160, 180, 255, 0.12))
                        .children(no_pic)
                        .child(strength_row)
                        .child(dim_row)
                        .child(blur_row)
                        .into_any_element(),
                )
            }
            Backdrop::Live => {
                let single =
                    values.string("windowGlass") == "auto" || same_for_light_and_dark(values);
                let style_key = if dark || single {
                    "windowGlassLiveStyleDark"
                } else {
                    "windowGlassLiveStyleLight"
                };
                let current_style = values.string(style_key);
                let options =
                    settings_catalog().options(module::SETTINGS, "WINDOW_GLASS_LIVE_STYLE_OPTIONS");
                let mut cards: Vec<AnyElement> = Vec::with_capacity(options.len());
                for option in options {
                    let style_id = option.value;
                    let style_name = option.label;
                    let selected = current_style == style_id;
                    let pic = self.look_cached_live(&style_id, light);
                    let key = SharedString::from(format!("look-live-{style_id}"));
                    cards.push(
                        self.control(
                            s,
                            div()
                                .id(key.clone())
                                .w(s.px(166.0))
                                .flex()
                                .flex_col()
                                .gap(s.px(3.0))
                                .cursor_pointer()
                                .child(
                                    div()
                                        .w_full()
                                        .h(s.px(42.0))
                                        .rounded(s.px(7.0))
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
                                            this.child(
                                                img(pic).size_full().object_fit(ObjectFit::Cover),
                                            )
                                        }),
                                )
                                .child(
                                    sans(
                                        s,
                                        11.5,
                                        if selected { 600.0 } else { 400.0 },
                                        if selected {
                                            hex(0xeef1f7)
                                        } else {
                                            hex(0x9aa3b8)
                                        },
                                    )
                                    .child(style_name),
                                ),
                            key,
                            interact::Ring::new(7.0, 1.0),
                            interact::Keys::EnterSpace,
                            cx,
                            move |this, _, cx| {
                                let mut patch = Map::new();
                                patch.insert(style_key.into(), json!(style_id));
                                if single {
                                    patch.insert(
                                        "windowGlassLiveStyleLight".into(),
                                        json!(style_id),
                                    );
                                }
                                patch.insert("windowGlassSource".into(), json!("live"));
                                this.update_settings(patch, cx);
                            },
                        )
                        .into_any_element(),
                    );
                }
                let (exact, nearest) = strength_from_settings(values);
                let strength_label = if exact.is_none() {
                    "Custom".to_string()
                } else {
                    format!("{}%", nearest.round() as i64)
                };
                let speed = self.settings.window_glass_live_speed;
                let speed_label = if (speed - speed.round()).abs() < 1e-6 {
                    format!("{}x", speed as i64)
                } else {
                    format!("{speed:.2}x").replace(".50x", ".5x")
                };
                let brightness_label = format!(
                    "{}%",
                    self.settings.window_glass_live_brightness.round() as i64
                );
                let strength_row = self.look_slider_row(
                    s,
                    "look-strength",
                    "Strength",
                    nearest,
                    strength_label,
                    cx,
                );
                let speed_row =
                    self.look_slider_row(s, "look-live-speed", "Speed", speed, speed_label, cx);
                let brightness_row = self.look_slider_row(
                    s,
                    "look-live-brightness",
                    "Brightness",
                    self.settings.window_glass_live_brightness,
                    brightness_label,
                    cx,
                );
                let only_on_power = self.settings.window_glass_video_only_on_power;
                let power_thumb = self.thumb("look-live-power", only_on_power, now);
                let power_row = div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(s.px(26.0))
                    .mt(s.px(2.0))
                    .child(sans(s, 12.5, 400.0, hex(0xd5dce8)).child("Play only when plugged in"))
                    .child(self.control(
                        s,
                        toggle(
                            s,
                            "look-live-power-toggle",
                            only_on_power,
                            power_thumb,
                            ToggleSize::Sm,
                            false,
                        ),
                        "look-live-power-toggle",
                        interact::Ring::new(9.5, 1.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| {
                            let mut patch = Map::new();
                            patch.insert(
                                "windowGlassVideoOnlyOnPower".into(),
                                json!(!only_on_power),
                            );
                            this.update_settings(patch, cx);
                        },
                    ));
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(s.px(8.0))
                        .px(s.px(12.0))
                        .py(s.px(10.0))
                        .rounded(s.px(10.0))
                        .bg(rgba(18, 24, 38, 0.62))
                        .border_1()
                        .border_color(rgba(160, 180, 255, 0.12))
                        .child(div().flex().flex_wrap().gap(s.px(8.0)).children(cards))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(s.px(3.0))
                                .pt(s.px(4.0))
                                .border_t_1()
                                .border_color(rgba(160, 180, 255, 0.10))
                                .child(strength_row)
                                .child(speed_row)
                                .child(brightness_row)
                                .child(power_row),
                        )
                        .into_any_element(),
                )
            }
            Backdrop::None => None,
        };

        let active_name = match picked {
            Backdrop::None => "None".to_string(),
            Backdrop::Artwork(id) => ghostex_settings_catalog::artwork_by_id(id)
                .map(|a| a.name.to_string())
                .unwrap_or_else(|| (*id).to_string()),
            Backdrop::YourImage => "Your image".to_string(),
            Backdrop::Live => "Animated".to_string(),
        };

        div()
            .flex()
            .flex_col()
            .gap(s.px(7.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label(s, "Artwork"))
                    .child(sans(s, 11.5, 400.0, hex(0x8c95a8)).child(active_name)),
            )
            .child(strip)
            .children(controls)
            .into_any_element()
    }

    fn render_look_transparency_section(
        &mut self,
        s: S,
        dark: bool,
        values: &SettingsValues,
        picked: &Backdrop,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(name) = painted_name(values, dark) {
            return div()
                .flex()
                .flex_col()
                .gap(s.px(6.0))
                .child(label(s, "Window transparency"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(s.px(12.0))
                        .px(s.px(12.0))
                        .py(s.px(9.0))
                        .rounded(s.px(10.0))
                        .bg(rgba(18, 24, 38, 0.52))
                        .border_1()
                        .border_color(rgba(160, 180, 255, 0.12))
                        .child(
                            sans(s, 12.0, 400.0, hex(0x9aa3b8))
                                .flex_1()
                                .min_w_0()
                                .child(format!(
                                    "{name} is behind the window in {} mode, so transparency is paused.",
                                    if dark { "dark" } else { "light" }
                                )),
                        )
                        .child(self.control(
                            s,
                            div()
                                .id("look-transparency-resume")
                                .flex_none()
                                .px(s.px(10.0))
                                .py(s.px(5.0))
                                .rounded(s.px(7.0))
                                .bg(rgba(44, 56, 102, 0.72))
                                .border_1()
                                .border_color(rgba(124, 148, 232, 0.45))
                                .cursor_pointer()
                                .child(
                                    sans(s, 12.0, 500.0, hex(0xeef1f7)).child("Use transparency"),
                                ),
                            "look-transparency-resume",
                            interact::Ring::new(7.0, 1.0),
                            interact::Keys::EnterSpace,
                            cx,
                            move |this, _, cx| this.pick_look_no_artwork(dark, true, cx),
                        )),
                )
                .into_any_element();
        }

        let glass = values.string("windowGlass");
        let source = values.string("windowGlassSource");
        let choices = transparency_choices();
        let glass_bar = div()
            .flex()
            .gap(s.px(4.0))
            .p(s.px(3.0))
            .rounded(s.px(8.0))
            .bg(rgba(14, 19, 32, 0.75))
            .border_1()
            .border_color(rgba(160, 180, 255, 0.12))
            .children(choices.into_iter().map(|opt| {
                let selected = glass == opt.value;
                let value = opt.value;
                let key = SharedString::from(format!("look-glass-{value}"));
                self.control(
                    s,
                    div()
                        .id(key.clone())
                        .flex_1()
                        .h(s.px(28.0))
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
                        .child(
                            sans(
                                s,
                                12.5,
                                if selected { 600.0 } else { 400.0 },
                                if selected {
                                    hex(0xeef1f7)
                                } else {
                                    hex(0x9aa3b8)
                                },
                            )
                            .child(opt.label),
                        ),
                    key,
                    interact::Ring::new(6.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| {
                        let mut patch = Map::new();
                        patch.insert("windowGlass".into(), json!(value));
                        this.update_settings(patch, cx);
                    },
                )
            }));

        let active_here = glass != "opaque" && !(glass == "auto" && !dark);
        let mut body = div()
            .flex()
            .flex_col()
            .gap(s.px(8.0))
            .px(s.px(12.0))
            .py(s.px(10.0))
            .rounded(s.px(10.0))
            .bg(rgba(18, 24, 38, 0.62))
            .border_1()
            .border_color(rgba(160, 180, 255, 0.12))
            .child(glass_bar);

        if active_here {
            let mut tiles: Vec<AnyElement> = Vec::with_capacity(SHOW_BEHIND.len());
            for &(value, title, sub_text) in &SHOW_BEHIND {
                let selected = source == value;
                let swatch = self.look_cached_source_art(value);
                let key = SharedString::from(format!("look-show-behind-{value}"));
                tiles.push(
                    self.control(
                        s,
                        div()
                            .id(key.clone())
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(s.px(9.0))
                            .px(s.px(9.0))
                            .py(s.px(6.0))
                            .rounded(s.px(8.0))
                            .cursor_pointer()
                            .bg(if selected {
                                rgba(44, 56, 102, 0.45)
                            } else {
                                rgba(14, 19, 32, 0.55)
                            })
                            .border_color(if selected {
                                hex(0xeef1f7)
                            } else {
                                rgba(160, 180, 255, 0.16)
                            })
                            .when(selected, |this| this.border_2())
                            .when(!selected, |this| this.border_1())
                            .child(
                                div()
                                    .relative()
                                    .flex_none()
                                    .w(s.px(34.0))
                                    .h(s.px(24.0))
                                    .rounded(s.px(5.0))
                                    .overflow_hidden()
                                    .when_some(swatch, |this, image| {
                                        this.child(
                                            img(image)
                                                .absolute()
                                                .inset_0()
                                                .size_full()
                                                .object_fit(ObjectFit::Fill),
                                        )
                                    })
                                    .when(value == "desktopAndWindows", |this| {
                                        this.child(
                                            div()
                                                .absolute()
                                                .left(s.px(4.0))
                                                .top(s.px(4.0))
                                                .w(s.px(15.0))
                                                .h(s.px(11.0))
                                                .rounded(s.px(2.0))
                                                .bg(white(0.38)),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        sans(
                                            s,
                                            12.0,
                                            if selected { 600.0 } else { 500.0 },
                                            hex(0xeef1f7),
                                        )
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(title),
                                    )
                                    .child(
                                        sans(s, 10.5, 400.0, hex(0x9aa3b8))
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .child(sub_text),
                                    ),
                            ),
                        key,
                        interact::Ring::new(8.0, 1.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| {
                            let mut patch = Map::new();
                            patch.insert("windowGlassSource".into(), json!(value));
                            this.update_settings(patch, cx);
                        },
                    )
                    .into_any_element(),
                );
            }
            if source == "live" {
                body = body.child(sans(s, 11.5, 400.0, hex(0x8c95a8)).child(
                    "Animated is active in Artwork above. Pick one of these to see through the window instead.",
                ));
            }
            body = body.child(div().flex().gap(s.px(8.0)).children(tiles));
            if *picked == Backdrop::None {
                let (exact, nearest) = strength_from_settings(values);
                let strength_label = if exact.is_none() {
                    "Custom".to_string()
                } else {
                    format!("{}%", nearest.round() as i64)
                };
                let blur_label = format!(
                    "{} pt",
                    self.settings.window_glass_blur_radius.round() as i64
                );
                let strength_row = self.look_slider_row(
                    s,
                    "look-strength",
                    "Strength",
                    nearest,
                    strength_label,
                    cx,
                );
                let blur_row = self.look_slider_row(
                    s,
                    "look-glass-blur",
                    "Blur",
                    self.settings.window_glass_blur_radius,
                    blur_label,
                    cx,
                );
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(s.px(3.0))
                        .child(strength_row)
                        .child(blur_row),
                );
            }
        } else if glass == "auto" && !dark {
            body = body.child(sans(s, 11.5, 400.0, hex(0x8c95a8)).child(
                "Transparency is set to Dark only. Pick Always to show it in light mode too.",
            ));
        }

        div()
            .flex()
            .flex_col()
            .gap(s.px(6.0))
            .child(label(s, "Window transparency"))
            .child(body)
            .into_any_element()
    }

    fn render_live_app_preview(
        &mut self,
        s: S,
        dark: bool,
        values: &SettingsValues,
        picked: &Backdrop,
    ) -> AnyElement {
        let light = !dark;
        let (sb_colour, wa_colour, _) = preview_colours(values, dark);
        let to_u32 = |c: [f64; 3]| -> u32 {
            ((c[0].round().clamp(0.0, 255.0) as u32) << 16)
                | ((c[1].round().clamp(0.0, 255.0) as u32) << 8)
                | (c[2].round().clamp(0.0, 255.0) as u32)
        };
        let sb_rgb = to_u32(sb_colour.0);
        let wa_rgb = to_u32(wa_colour.0);
        let accent_hex = self
            .settings
            .theme_roles
            .get(appearance_key(light))
            .and_then(|obj| obj.get("accent"))
            .and_then(Value::as_str)
            .unwrap_or("#86d3f8");
        let accent_rgb = parse_rgb(accent_hex, 0x86d3f8);

        let glass = self.settings.window_glass.as_str();
        let glass_active = glass != "opaque" && !(glass == "auto" && !dark);
        let solid = matches!(picked, Backdrop::None) && !glass_active;
        let (sb_tint, wa_tint) = if dark {
            (
                self.settings.glass_sidebar_dark,
                self.settings.glass_work_area_dark,
            )
        } else {
            (
                self.settings.glass_sidebar_light,
                self.settings.glass_work_area_light,
            )
        };
        let sb_alpha = if solid {
            1.0
        } else {
            (sb_tint / 100.0).clamp(0.15, 0.96) as f32
        };
        let wa_alpha = if solid {
            1.0
        } else {
            (wa_tint / 100.0).clamp(0.15, 0.96) as f32
        };

        let fg = role_fill(
            &self.settings.theme_roles,
            light,
            "text",
            wa_rgb,
            RoleValue::Colour {
                rgb: if dark {
                    [0xe8, 0xeb, 0xf2]
                } else {
                    [0x1f, 0x23, 0x2b]
                },
                alpha: 1.0,
            },
        );
        let muted = role_fill(
            &self.settings.theme_roles,
            light,
            "mutedText",
            wa_rgb,
            RoleValue::Colour {
                rgb: if dark {
                    [0x98, 0xa0, 0xb2]
                } else {
                    [0x5b, 0x62, 0x75]
                },
                alpha: 1.0,
            },
        );
        let line = role_fill(
            &self.settings.theme_roles,
            light,
            "border",
            wa_rgb,
            RoleValue::Ink(if dark { 0.09 } else { 0.11 }),
        );
        let selection_bg = role_fill(
            &self.settings.theme_roles,
            light,
            "selection",
            sb_rgb,
            RoleValue::Ink(if dark { 0.09 } else { 0.07 }),
        );
        let bubble_bg = role_fill(
            &self.settings.theme_roles,
            light,
            "userMessage",
            wa_rgb,
            RoleValue::Base {
                step: 0.12,
                alpha: 1.0,
            },
        );
        let card_bg = role_fill(
            &self.settings.theme_roles,
            light,
            "surface",
            wa_rgb,
            RoleValue::Base {
                step: 0.08,
                alpha: 1.0,
            },
        );
        let box_bg = role_fill(
            &self.settings.theme_roles,
            light,
            "chatBox",
            wa_rgb,
            RoleValue::Base {
                step: 0.06,
                alpha: 1.0,
            },
        );
        let ctl_bg = role_fill(
            &self.settings.theme_roles,
            light,
            "control",
            wa_rgb,
            RoleValue::Base {
                step: 0.10,
                alpha: 1.0,
            },
        );

        let bg_layer: Option<AnyElement> = match picked {
            Backdrop::Artwork(id) => {
                let dim = (self.settings.window_art_dim / 100.0).clamp(0.0, 0.85) as f32;
                let pic = self.look_cached_image("theme-art", id, artwork_thumbnail_bytes);
                pic.map(|pic| {
                    div()
                        .absolute()
                        .inset_0()
                        .child(img(pic).size_full().object_fit(ObjectFit::Cover))
                        .child(div().absolute().inset_0().bg(if dark {
                            black(dim)
                        } else {
                            white(dim)
                        }))
                        .into_any_element()
                })
            }
            Backdrop::YourImage => your_image_path(values, dark).map(|path| {
                let dim = (self.settings.window_art_dim / 100.0).clamp(0.0, 0.85) as f32;
                div()
                    .absolute()
                    .inset_0()
                    .child(img(path).size_full().object_fit(ObjectFit::Cover))
                    .child(div().absolute().inset_0().bg(if dark {
                        black(dim)
                    } else {
                        white(dim)
                    }))
                    .into_any_element()
            }),
            Backdrop::Live => {
                let style = if dark || glass == "auto" {
                    self.settings.window_glass_live_dark.clone()
                } else {
                    self.settings.window_glass_live_light.clone()
                };
                let brightness =
                    (self.settings.window_glass_live_brightness / 100.0).clamp(0.15, 1.0) as f32;
                self.look_cached_live(&style, light).map(|pic| {
                    img(pic)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        .opacity(brightness)
                        .into_any_element()
                })
            }
            Backdrop::None => None,
        };

        let preview_w = 740.0;
        let preview_h = 468.0;
        let sidebar_w = 218.0;
        let wallpaper_bg = self.settings.window_glass_source == "wallpaper";
        let window_card = div()
            .relative()
            .w(s.px(preview_w))
            .h(s.px(preview_h))
            .rounded(s.px(14.0))
            .overflow_hidden()
            .border_1()
            .border_color(rgba(160, 180, 255, 0.20))
            .shadow(vec![shadow(black(0.45), 0.0, 18.0, 44.0, 0.0, s)])
            .bg(if *picked == Backdrop::None && glass_active {
                if wallpaper_bg {
                    gpui::linear_gradient(
                        135.0,
                        gpui::linear_color_stop(hex(0x2f6f8f), 0.0),
                        gpui::linear_color_stop(hex(0xd8a24b), 1.0),
                    )
                } else {
                    gpui::linear_gradient(
                        135.0,
                        gpui::linear_color_stop(hex(0x2b3550), 0.0),
                        gpui::linear_color_stop(hex(0x4f3e52), 1.0),
                    )
                }
            } else {
                hex(wa_rgb).into()
            })
            .children(bg_layer)
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
                            .child(
                                div()
                                    .flex()
                                    .gap(s.px(6.0))
                                    .mb(s.px(6.0))
                                    .child(
                                        div()
                                            .size(s.px(9.0))
                                            .rounded_full()
                                            .bg(if dark { white(0.22) } else { black(0.18) }),
                                    )
                                    .child(
                                        div()
                                            .size(s.px(9.0))
                                            .rounded_full()
                                            .bg(if dark { white(0.22) } else { black(0.18) }),
                                    )
                                    .child(
                                        div()
                                            .size(s.px(9.0))
                                            .rounded_full()
                                            .bg(if dark { white(0.22) } else { black(0.18) }),
                                    ),
                            )
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
                                    .bg(selection_bg)
                                    .child(
                                        sans(s, 12.5, 500.0, fg).child("Build floating editor"),
                                    ),
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
                            .child(
                                div()
                                    .flex_1()
                                    .p(s.px(20.0))
                                    .flex()
                                    .flex_col()
                                    .gap(s.px(14.0))
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
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(s.px(8.0))
                                            .child(
                                                sans(s, 12.5, 400.0, fg).child(
                                                    "Every fill now follows your chosen colour automatically.",
                                                ),
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
                                                            .rounded(s.px(6.0))
                                                            .bg(rgb_to_hsla(
                                                                accent_rgb,
                                                                if dark { 0.22 } else { 0.16 },
                                                            ))
                                                            .border_1()
                                                            .border_color(rgb_to_hsla(
                                                                accent_rgb,
                                                                if dark { 0.45 } else { 0.36 },
                                                            ))
                                                            .flex()
                                                            .items_center()
                                                            .gap(s.px(5.0))
                                                            .child(
                                                                div()
                                                                    .size(s.px(6.0))
                                                                    .rounded(s.px(3.0))
                                                                    .bg(hex(accent_rgb)),
                                                            )
                                                            .child(
                                                                sans(s, 11.0, 600.0, fg)
                                                                    .child("Ready"),
                                                            ),
                                                    ),
                                            ),
                                    )
                                    .child(div().flex_1())
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
            .child(sans(s, 13.5, 400.0, hex(0x9aa3b8)).child("Your Ghostex, as you change it."))
            .into_any_element()
    }
}
