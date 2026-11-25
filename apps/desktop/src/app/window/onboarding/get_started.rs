//! Panel 2, Preferences: top Day-0 setup choices inline (Experience preset, agent approvals,
//! optional phone pairing after first reply, and appearance/theme with a live Native Chat preview).
//!
//! CDXC:Onboarding 2026-11-24 DECISION:
//! User: "pls make native chat default for all new useres and dont even offerthem anything about
//! terminal based chat which we're taking out soon". The Default session view (Chat | Terminal)
//! picker is removed completely; the live preview always shows Native Chat, and the top Day-0
//! friction controls (Experience preset, Agent approvals, and theme) live inline here.
use super::ArtPreview;
use super::FinishTarget;
use super::GpuiOnboardingWindow;
use super::interact;
use super::model::{
    COLOURFULNESS_LAST_POSITION, OnboardingSettings, ThemeSwatchPreset, colourfulness_display_step,
    colourfulness_name, colourfulness_points, colourfulness_step, matching_preset,
    theme_preset_label, transparency_strength, transparency_strength_patch,
};
use super::primitives::*;
use super::stage::*;
use gpui::StyledImage as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, Div, InteractiveElement as _, IntoElement, MouseButton, ObjectFit,
    ParentElement as _, Pixels, RenderImage, SharedString, StatefulInteractiveElement as _,
    Styled as _, canvas, div, img,
};
use gpui_component::tooltip::Tooltip;
use serde_json::Value;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

const CARDS_LEFT: f32 = 156.0;
const CARDS_TOP: f32 = 240.0;
const CARDS_WIDTH: f32 = 1360.0;

const ANALYTICS_TITLE: &str =
    "Share 100% Fully Anonymous Usage Analytics to Support Ghostex Development";
const ANALYTICS_SUBTITLE: &str = "Collects no personal data at all. Helps us continue supporting and building Ghostex. Completely free and MIT open-source. Turning this off stops all usage tracking (keeps only 1 anonymous weekly count; DO_NOT_TRACK=1 stops all).";

#[derive(Default)]
pub(crate) struct GetStartedState {
    /// Which appearance's colours the look card shows.
    dark_scheme: bool,
    /// The Dark | Light colour tab was picked by hand, so a system appearance change leaves it.
    scheme_picked: bool,
    picked_dark: bool,
    picked_light: bool,
    pub(crate) swatches: HashMap<String, Arc<RenderImage>>,
    slider_bounds: HashMap<&'static str, Rc<Cell<Option<Bounds<Pixels>>>>>,
    dragging: Option<&'static str>,
}

impl GetStartedState {
    /// The computer's appearance changed while the window is open: with Appearance on System the
    /// colour tab follows it, unless the user picked a tab by hand.
    pub(crate) fn system_appearance_changed(
        &mut self,
        settings: &OnboardingSettings,
        system_light: bool,
    ) {
        if settings.sidebar_theme == "system" && !self.scheme_picked {
            self.dark_scheme = !system_light;
        }
    }

    pub(crate) fn reset(&mut self, settings: &OnboardingSettings, system_light: bool) {
        self.scheme_picked = false;
        self.dark_scheme = match settings.sidebar_theme.as_str() {
            "system" => !system_light,
            theme => theme.starts_with("dark"),
        };
        self.picked_dark = false;
        self.picked_light = false;
        self.dragging = None;
    }
}

/// An appearance choice's name; System says which appearance it gives right now.
///
/// CDXC:Onboarding 2026-10-28 WHY:
/// The onboarding is always drawn dark, whatever the computer uses, and System is the default, so
/// a computer set to light (or a Linux desktop whose preference Ghostex cannot read, which counts
/// as light) opened a light app after a dark setup with nothing saying why. System now names the
/// appearance it resolves to.
pub(crate) fn appearance_label(value: &str, name: &str, system_light: bool) -> String {
    if value == "system" {
        format!("{name} · {}", if system_light { "Light" } else { "Dark" })
    } else {
        name.to_string()
    }
}

/// `APPEARANCE_CHOICES`: System, Light, Dark.
const APPEARANCE: [(&str, &str); 3] = [
    ("system", "System"),
    ("plain-light", "Light"),
    ("dark-2", "Dark"),
];

/// `NEUTRAL_SWATCH_ACCENT`: themes without a hue take a grey sheen.
fn neutral_accent(preset: &str, dark: bool) -> Option<u32> {
    match preset {
        "gray" => Some(if dark { 0xa3a3a8 } else { 0x6b6b70 }),
        "black" | "white" => Some(if dark { 0x4a4a4f } else { 0xc8c8cc }),
        _ => None,
    }
}

fn channels(color: u32) -> [f32; 3] {
    [
        ((color >> 16) & 0xff) as f32,
        ((color >> 8) & 0xff) as f32,
        (color & 0xff) as f32,
    ]
}

fn mix_hex(from: u32, to: u32, amount: f32) -> [f32; 3] {
    let (a, b) = (channels(from), channels(to));
    [
        (a[0] + (b[0] - a[0]) * amount).round(),
        (a[1] + (b[1] - a[1]) * amount).round(),
        (a[2] + (b[2] - a[2]) * amount).round(),
    ]
}

/// One theme colour square (`swatchStyle`): a linear gradient from a lighter top-left to the
/// theme's chrome, two soft accent glows, and the square's sheen, drawn into a bitmap because GPUI
/// has no radial gradients.
///
/// `position` is a Colourfulness position; `step` runs 0 (Subtle) to 4 (Vivid) through the
/// in-between positions, so the five named points paint exactly as the five steps did.
fn swatch_image(
    dark: bool,
    preset: &ThemeSwatchPreset,
    position: usize,
) -> Option<Arc<RenderImage>> {
    let size = 48u32;
    let accent = neutral_accent(&preset.value, dark).unwrap_or(preset.accent);
    let chrome_at = |position: usize| {
        preset
            .chrome
            .get(position.min(COLOURFULNESS_LAST_POSITION))
            .copied()
            .unwrap_or(0)
    };
    let step = position as f32 / 8.0;
    let top_chrome = chrome_at(position + 16);
    let top = if dark {
        mix_hex(top_chrome, accent, 0.34 + step * 0.03)
    } else {
        channels(top_chrome)
    };
    let base = if dark {
        mix_hex(chrome_at(position), accent, 0.1)
    } else {
        channels(chrome_at(position))
    };
    let accent_rgb = channels(accent);
    let glow = ((0.26 + step * 0.07) * 255.0).round() / 255.0;
    let rim = ((0.1 + step * 0.03) * 255.0).round() / 255.0;
    let (sin150, cos150) = (150f32.to_radians().sin(), 150f32.to_radians().cos());
    let (sin160, cos160) = (160f32.to_radians().sin(), 160f32.to_radians().cos());
    let mut bgra = vec![0u8; (size * size * 4) as usize];
    for row in 0..size {
        let y = (row as f32 + 0.5) / size as f32;
        for column in 0..size {
            let x = (column as f32 + 0.5) / size as f32;
            // linear-gradient(150deg, top 0%, base 88%)
            let length = sin150.abs() + cos150.abs();
            let t = (((x - 0.5) * sin150 - (y - 0.5) * cos150) / length + 0.5) / 0.88;
            let t = t.clamp(0.0, 1.0);
            let mut color = [
                top[0] + (base[0] - top[0]) * t,
                top[1] + (base[1] - top[1]) * t,
                top[2] + (base[2] - top[2]) * t,
            ];
            let over = |color: &mut [f32; 3], source: [f32; 3], alpha: f32| {
                for index in 0..3 {
                    color[index] += (source[index] - color[index]) * alpha;
                }
            };
            // radial-gradient(90% 70% at 100% 100%, accent rim 0%, transparent 70%)
            let r2 = (((x - 1.0) / 0.9).powi(2) + ((y - 1.0) / 0.7).powi(2)).sqrt();
            over(
                &mut color,
                accent_rgb,
                rim * (1.0 - (r2 / 0.7).clamp(0.0, 1.0)),
            );
            // radial-gradient(130% 100% at 20% 8%, accent glow 0%, transparent 62%)
            let r1 = (((x - 0.2) / 1.3).powi(2) + ((y - 0.08) / 1.0).powi(2)).sqrt();
            over(
                &mut color,
                accent_rgb,
                glow * (1.0 - (r1 / 0.62).clamp(0.0, 1.0)),
            );
            // The sheen: linear-gradient(160deg, white 16% 0%, transparent 42%).
            let sheen_length = sin160.abs() + cos160.abs();
            let s = ((x - 0.5) * sin160 - (y - 0.5) * cos160) / sheen_length + 0.5;
            over(
                &mut color,
                [255.0, 255.0, 255.0],
                0.16 * (1.0 - (s / 0.42).clamp(0.0, 1.0)),
            );
            let index = ((row * size + column) * 4) as usize;
            bgra[index] = color[2].round().clamp(0.0, 255.0) as u8;
            bgra[index + 1] = color[1].round().clamp(0.0, 255.0) as u8;
            bgra[index + 2] = color[0].round().clamp(0.0, 255.0) as u8;
            bgra[index + 3] = 255;
        }
    }
    let buffer = image::RgbaImage::from_raw(size, size, bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

impl GpuiOnboardingWindow {
    fn update_one(&mut self, key: &str, value: Value, cx: &mut Context<Self>) {
        let mut patch = serde_json::Map::new();
        patch.insert(key.to_string(), value);
        self.update_settings(patch, cx);
    }

    /// CDXC:Onboarding 2026-09-23 DECISION:
    /// User: "lets somehow have a switch that lets you see just dark or just light themes no need to show all of them there / and by default we just pick the same color in the other scheme". Theme shows one appearance's cards behind a Dark | Light switch, and picking a look fills in the matching look for the other appearance until that one is picked by hand.
    fn pick_theme_preset(&mut self, dark: bool, preset: String, cx: &mut Context<Self>) {
        let mut patch = serde_json::Map::new();
        let (key, other_key, picked_other) = if dark {
            (
                "darkThemePreset",
                "lightThemePreset",
                self.get_started.picked_light,
            )
        } else {
            (
                "lightThemePreset",
                "darkThemePreset",
                self.get_started.picked_dark,
            )
        };
        if !picked_other && let Some(matching) = matching_preset(dark, &preset) {
            patch.insert(other_key.to_string(), Value::String(matching.to_string()));
        }
        patch.insert(key.to_string(), Value::String(preset));
        if dark {
            self.get_started.picked_dark = true;
        } else {
            self.get_started.picked_light = true;
        }
        self.update_settings(patch, cx);
    }

    /// CDXC:Onboarding 2026-09-23 DECISION:
    /// User: "we should automatically activate night mode for them if they enable transparency (switch it from auto/light to dark and indicate this with a toast". Turning transparency on here also sets Appearance to Dark (glass only shows in dark mode) and says so in a toast; transparency is one row (switch, strength slider, value).
    fn toggle_transparency(&mut self, cx: &mut Context<Self>) {
        let turning_on = self.settings.window_glass == "opaque";
        let mut patch = serde_json::Map::new();
        let next = if !turning_on {
            "opaque".to_string()
        } else if self.settings.window_glass == "opaque" {
            "auto".to_string()
        } else {
            self.settings.window_glass.clone()
        };
        patch.insert("windowGlass".into(), Value::String(next));
        let mut notes = Vec::new();
        if turning_on && !self.settings.sidebar_theme.starts_with("dark") {
            patch.insert("sidebarTheme".into(), Value::String("dark-2".into()));
            self.get_started.dark_scheme = true;
            notes.push("Switched to Dark appearance so the transparency shows.");
        }
        if !notes.is_empty() {
            self.show_toast(notes.join(" "), cx);
        }
        self.update_settings(patch, cx);
    }

    fn set_slider(&mut self, slider: &'static str, value: f64, cx: &mut Context<Self>) {
        match slider {
            "colourfulness" => {
                let index = (value.round().max(0.0) as usize).min(COLOURFULNESS_LAST_POSITION);
                if colourfulness_step(&self.settings) == Some(index) {
                    return;
                }
                if let Some(patch) = self.theme.colourfulness_patches.get(index).cloned() {
                    self.update_settings(patch, cx);
                }
            }
            _ => {
                if self.settings.window_glass == "opaque" {
                    return;
                }
                let value = (value / 5.0).round() * 5.0;
                if transparency_strength(&self.settings).0 == Some(value) {
                    return;
                }
                self.update_settings(transparency_strength_patch(value, &self.settings), cx);
            }
        }
    }

    /// Arrow keys on a focused slider move it one step (a focused range input keeps them from paging).
    pub(super) fn slider_key(&mut self, right: bool, cx: &mut Context<Self>) -> bool {
        let slider = if self.focus.is_focused("slider-colourfulness") {
            "colourfulness"
        } else if self.focus.is_focused("slider-transparency") {
            "transparency"
        } else {
            return false;
        };
        if self.panel != 2 {
            return false;
        }
        let delta = if right { 1.0 } else { -1.0 };
        match slider {
            "colourfulness" => {
                let current = colourfulness_step(&self.settings)
                    .unwrap_or(COLOURFULNESS_LAST_POSITION / 2)
                    as f64;
                let last = COLOURFULNESS_LAST_POSITION as f64;
                self.set_slider(slider, (current + delta).clamp(0.0, last), cx);
            }
            _ => {
                let current = transparency_strength(&self.settings).1;
                self.set_slider(slider, (current + delta * 5.0).clamp(0.0, 100.0), cx);
            }
        }
        true
    }

    /// The preset the Experience choice shows: what the settings match, else Regular, which a
    /// first run starts on.
    pub(super) fn experience_shown(&self) -> ghostex_settings_catalog::experience::Preset {
        self.settings
            .experience()
            .unwrap_or(ghostex_settings_catalog::experience::FIRST_RUN_PRESET)
    }

    /// Leaving step 2 keeps the preselected preset: settings that match none get Regular's.
    pub(super) fn keep_experience(&mut self, cx: &mut Context<Self>) {
        if self.settings.experience().is_none() {
            self.pick_experience(ghostex_settings_catalog::experience::FIRST_RUN_PRESET, cx);
        }
    }

    fn pick_experience(
        &mut self,
        preset: ghostex_settings_catalog::experience::Preset,
        cx: &mut Context<Self>,
    ) {
        let patch: serde_json::Map<String, serde_json::Value> =
            ghostex_settings_catalog::experience::writes_all(preset)
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect();
        self.update_settings(patch, cx);
    }

    fn toggle_pair_phone(&mut self, cx: &mut Context<Self>) {
        self.flow.phone_queued = !self.flow.phone_queued;
        cx.notify();
    }

    fn toggle_analytics(&mut self, cx: &mut Context<Self>) {
        let next = !self.settings.analytics_enabled;
        self.update_one("analyticsEnabled", Value::Bool(next), cx);
    }

    /// Persist `analyticsEnabled` explicitly when advancing past Step 2 or finishing setup
    /// so gxserver's pre-onboarding flush hold always releases with the user's choice on disk.
    pub(super) fn persist_analytics_choice(&mut self, cx: &mut Context<Self>) {
        self.update_one(
            "analyticsEnabled",
            Value::Bool(self.settings.analytics_enabled),
            cx,
        );
    }

    /// CDXC:Telemetry 2026-11-25 DECISION (Q24 = strip, Q29):
    /// Compact privacy strip tucked right above the Back / Next footer on Step 2 with the
    /// unified title, zero-personal-data + MIT open-source + 3-state explanation, and no
    /// "check code" link ("remove check code here no need").
    fn render_analytics_strip(&mut self, s: S, now: Instant, cx: &mut Context<Self>) -> AnyElement {
        let analytics_on = self.settings.analytics_enabled;
        let thumb = self.thumb("onboarding-analytics", analytics_on, now);
        let analytics_switch = self.control(
            s,
            toggle(
                s,
                "onboarding-analytics-toggle",
                analytics_on,
                thumb,
                ToggleSize::Lg,
                false,
            ),
            "onboarding-analytics-toggle",
            interact::Ring::new(17.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                cx.stop_propagation();
                this.toggle_analytics(cx);
            },
        );
        self.control(
            s,
            div()
                .id("onboarding-analytics-strip")
                .relative()
                .w_full()
                .py(s.px(12.0))
                .px(s.px(18.0))
                .rounded(s.px(12.0))
                .border_1()
                .border_color(white(0.10))
                .bg(rgba(18, 21, 31, 0.85))
                .flex()
                .items_center()
                .gap(s.px(14.0))
                .cursor_pointer()
                .child(
                    ibox(s, 34.0, 9.0)
                        .bg(rgba(28, 34, 51, 0.9))
                        .border_color(white(0.11))
                        .child(icon(s, "shield", 17.0, 1.6, hex(0x9fc0ff))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(nm(s, 14.5, 600.0).child(ANALYTICS_TITLE))
                        .child(ss(s, 12.5).mt(s.px(3.0)).child(ANALYTICS_SUBTITLE)),
                )
                .child(analytics_switch),
            "onboarding-analytics-strip",
            interact::Ring::new(12.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| this.toggle_analytics(cx),
        )
        .into_any_element()
    }

    pub(super) fn render_get_started(
        &mut self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        use ghostex_settings_catalog::experience::Preset;
        let shown_preset = self.experience_shown();

        let mut out = Vec::new();
        out.push(eyebrow(s, 486.0, 108.0, Some(700.0), "Step 2 · Preferences").into_any_element());
        out.push(
            heading(
                s,
                336.0,
                138.0,
                1000.0,
                46.0,
                "Make Ghostex yours.",
                None,
                true,
            )
            .into_any_element(),
        );
        out.push(
            sub(s, 336.0, 200.0, 1000.0, 16.0, true)
                .child(
                    "Choose how much detail Native Chat shows, how agents ask before they act, and a look you like.",
                )
                .into_any_element(),
        );

        let experience_picks = div()
            .flex()
            .gap(s.px(8.0))
            .children(Preset::ALL.into_iter().map(|preset| {
                let selected = preset == shown_preset;
                let key = SharedString::from(format!("experience-{}", preset.id()));
                let (border, bg) = opt_colors(&key, selected);
                self.control(
                    s,
                    div()
                        .id(key.clone())
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .h(s.px(42.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .px(s.px(12.0))
                        .rounded(s.px(10.0))
                        .border_1()
                        .border_color(border)
                        .bg(bg)
                        .cursor_pointer()
                        .when(selected, |this| {
                            this.shadow(vec![inset_shadow(white(0.04), 0.0, 1.0, 0.0, 0.0, s)])
                        })
                        .child(nm(s, 15.5, 500.0).child(preset.label())),
                    key,
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| {
                        this.pick_experience(preset, cx);
                    },
                )
            }));

        let phone_on = self.flow.phone_queued;
        let phone_thumb = self.thumb("pair-phone-inline", phone_on, now);
        let phone_switch = self.control(
            s,
            toggle(
                s,
                "pair-phone-inline-toggle",
                phone_on,
                phone_thumb,
                ToggleSize::Sm,
                false,
            ),
            "pair-phone-inline-toggle",
            interact::Ring::new(9.5, 1.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                cx.stop_propagation();
                this.toggle_pair_phone(cx);
            },
        );
        let phone_row = self.control(
            s,
            div()
                .id("pair-phone-inline-row")
                .relative()
                .mt(s.px(14.0))
                .py(s.px(10.0))
                .px(s.px(12.0))
                .rounded(s.px(10.0))
                .border_1()
                .border_color(white(0.07))
                .bg(white(0.02))
                .flex()
                .items_center()
                .gap(s.px(12.0))
                .cursor_pointer()
                .child(icon(s, "phone", 16.0, 1.6, hex(0xcfd5e0)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(nm(s, 14.0, 500.0).child("Pair my phone after my first reply"))
                        .child(
                            ss(s, 12.0)
                                .mt(s.px(2.0))
                                .child("Opens the QR code to follow agents on iPhone or Android."),
                        ),
                )
                .child(phone_switch),
            "pair-phone-inline-row",
            interact::Ring::new(10.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| this.toggle_pair_phone(cx),
        );

        let project_card = pcard(s)
            .child(div().mb(s.px(10.0)).child(label(s, "Chat experience")))
            .child(experience_picks)
            .child(
                ss(s, 12.5)
                    .mt(s.px(8.0))
                    .mb(s.px(16.0))
                    .child(shown_preset.description()),
            )
            .child(div().mb(s.px(10.0)).child(label(s, "Agent approvals")))
            .child(self.render_agent_approvals(s, cx))
            .child(phone_row)
            .child(
                div()
                    .mt(s.px(16.0))
                    .mb(s.px(8.0))
                    .child(label(s, "Native Chat preview")),
            )
            .child(self.render_look_preview(s, cx));
        let look_card = self.render_look_card(s, now, cx);
        let analytics_strip = self.render_analytics_strip(s, now, cx);
        out.push(
            abs(s, CARDS_LEFT, CARDS_TOP, Some(CARDS_WIDTH), None)
                .flex()
                .flex_col()
                .gap(s.px(16.0))
                .child(
                    div()
                        .flex()
                        .items_stretch()
                        .gap(s.px(40.0))
                        .child(project_card)
                        .child(look_card),
                )
                .child(analytics_strip)
                .into_any_element(),
        );
        out.push(
            foot_actions(s, 2)
                .child(self.control(
                    s,
                    cta(
                        s,
                        "preferences-next",
                        "Next",
                        true,
                        true,
                        false,
                        CtaSize::Foot,
                    ),
                    "preferences-next",
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| {
                        this.keep_experience(cx);
                        this.persist_analytics_choice(cx);
                        this.go(3, cx);
                    },
                ))
                .into_any_element(),
        );
        out
    }

    /// CDXC:Onboarding 2026-11-13 DECISION:
    /// User, on the first-run showcase page (Q3049), picked "Yes, live on Get started once they
    /// pick": the first run stays dark, so picking Light (or a theme colour, Colourfulness or
    /// Transparency) used to change nothing on screen. A small Ghostex window here follows every
    /// choice at once: the appearance System resolves to, the theme colour at its Colourfulness,
    /// the stage showing through when Transparency is on, and Chat or Terminal in its session.
    ///
    /// CDXC:Onboarding 2026-11-25: with an artwork, the small window draws the real backdrop
    /// (`art_preview_for`) under its sidebar and work area at the tints the window uses over art,
    /// so the Window art pick (and Black and white) shows here as it will in the app.
    fn render_look_preview(&self, s: S, cx: &mut Context<Self>) -> AnyElement {
        // The preview spans the card's 620-point column.
        const PREVIEW_WIDTH: f32 = 620.0;
        let art = self.art_preview_for(None, cx);
        let settings = &self.settings;
        let dark = match settings.sidebar_theme.as_str() {
            "system" => !self.system_light,
            theme => theme.starts_with("dark"),
        };
        let (presets, preset) = if dark {
            (&self.theme.dark, &settings.dark_theme_preset)
        } else {
            (&self.theme.light, &settings.light_theme_preset)
        };
        let position = colourfulness_display_step(settings.sidebar_contrast);
        let found = presets.iter().find(|candidate| &candidate.value == preset);
        let chrome = found
            .and_then(|preset| preset.chrome.get(position).copied())
            .unwrap_or(if dark { 0x2b2d31 } else { 0xf2f2f4 });
        let accent = found
            .map(|preset| neutral_accent(&preset.value, dark).unwrap_or(preset.accent))
            .unwrap_or(0x5a7deb);
        let glass_on = dark && settings.window_glass != "opaque";
        let (sidebar_alpha, work_alpha) = if let Some(art) = &art {
            (art.sidebar, art.work_area)
        } else if glass_on {
            (
                (settings.glass_sidebar_dark / 100.0) as f32,
                (settings.glass_work_area_dark / 100.0) as f32,
            )
        } else {
            (1.0, 1.0)
        };
        let paint = |rgb: [f32; 3], alpha: f32| {
            rgba(
                rgb[0] as u8,
                rgb[1] as u8,
                rgb[2] as u8,
                alpha.clamp(0.0, 1.0),
            )
        };
        let sidebar = paint(channels(chrome), sidebar_alpha);
        let work = paint(
            mix_hex(
                chrome,
                if dark { 0xffffff } else { 0xffffff },
                if dark { 0.04 } else { 0.55 },
            ),
            work_alpha,
        );
        let (text, muted, line) = if dark {
            (hex(0xeef1f7), hex(0x9ea7b6), white(0.08))
        } else {
            (hex(0x1d2330), hex(0x5b6474), black(0.08))
        };
        let accent_color = paint(channels(accent), 1.0);
        let accent_wash = paint(channels(accent), if dark { 0.28 } else { 0.18 });
        let session = |agent: &str, name: &str, lit: bool| {
            div()
                .flex()
                .items_center()
                .gap(s.px(6.0))
                .h(s.px(20.0))
                .px(s.px(6.0))
                .rounded(s.px(5.0))
                .when(lit, |this| this.bg(accent_wash))
                // The monochrome logos are drawn white, so a light window shows a dot instead.
                .child(if dark {
                    agent_logo(s, &self.catalog, agent, 10.0)
                } else {
                    div()
                        .size(s.px(6.0))
                        .mx(s.px(2.0))
                        .rounded_full()
                        .bg(muted)
                        .into_any_element()
                })
                .child(
                    sans(s, 10.0, 400.0, text)
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(name.to_string()),
                )
        };
        // Always preview Native Chat (terminal-based chat is never offered to new users).
        let body: AnyElement = div()
            .flex()
            .flex_col()
            .gap(s.px(6.0))
            .child(
                sans(s, 10.0, 400.0, text)
                    .self_end()
                    .py(s.px(4.0))
                    .px(s.px(8.0))
                    .rounded(s.px(7.0))
                    .bg(accent_wash)
                    .child("Add a dark mode toggle"),
            )
            .child(
                sans(s, 10.0, 400.0, muted)
                    .py(s.px(4.0))
                    .px(s.px(8.0))
                    .rounded(s.px(7.0))
                    .border_1()
                    .border_color(line)
                    .child("Edited 3 files. The toggle is in Settings."),
            )
            .into_any_element();
        div()
            .relative()
            .h(s.px(118.0))
            .flex()
            .flex_col()
            .rounded(s.px(10.0))
            .overflow_hidden()
            .when_some(art, |this, art| {
                let size = art.image.size(0);
                let tall = size.height.0 as f32 / (size.width.0 as f32).max(1.0);
                // The artwork rises from the window's bottom, so the preview shows its bottom.
                this.bg(hex(art.base)).child(
                    img(art.image)
                        .absolute()
                        .left_0()
                        .bottom_0()
                        .w(s.px(PREVIEW_WIDTH))
                        .h(s.px(PREVIEW_WIDTH * tall))
                        .object_fit(ObjectFit::Fill),
                )
            })
            .border_1()
            .border_color(if dark { white(0.12) } else { black(0.12) })
            .child(
                div()
                    .flex_1()
                    .flex()
                    .child(
                        div()
                            .w(s.px(160.0))
                            .flex_none()
                            .bg(sidebar)
                            .border_r_1()
                            .border_color(line)
                            .pt(s.px(8.0))
                            .px(s.px(6.0))
                            .flex()
                            .flex_col()
                            .gap(s.px(3.0))
                            .child(lights(s, true).ml(s.px(4.0)).mb(s.px(6.0)))
                            .child(session("claude", "dark-mode-toggle", true))
                            .child(session("codex", "port-tests", false))
                            .child(session("cursor", "landing-copy", false)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .bg(work)
                            .p(s.px(10.0))
                            .flex()
                            .flex_col()
                            .justify_between()
                            .child(body)
                            .child(
                                div()
                                    .h(s.px(18.0))
                                    .rounded(s.px(6.0))
                                    .border_1()
                                    .border_color(line)
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .px(s.px(3.0))
                                    .child(div().size(s.px(12.0)).rounded_full().bg(accent_color)),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// CDXC:Onboarding 2026-10-11 DECISION:
    /// User: add the "Agent approvals" setting (Keep default / Skip permissions) to the last page of the setup flow, with the same shared segmented control and the same setting key as Settings -> Agents -> Defaults so the two always agree, plus a one-line explanation. The line already warns what Skip permissions does, so the pick saves at once without Settings' "Skip permissions?" confirmation.
    fn render_agent_approvals(&mut self, s: S, cx: &mut Context<Self>) -> AnyElement {
        use super::super::native_modal_kit::{ModalRailItem, modal_bordered_segmented_control};
        let items = ["Keep default", "Full permissions"].map(|label| ModalRailItem {
            label: label.into(),
            trailing: None,
            dot: None,
        });
        let control = modal_bordered_segmented_control(
            gpui::rgb(0xeef1f7),
            gpui::rgba(0xffffff1f),
            gpui::rgb(0x9ea7b6),
            "onboarding-agent-approvals",
            &items,
            usize::from(self.settings.accept_all),
            |this: &mut Self, index, _window, cx| {
                this.update_one("agentAcceptAllEnabled", Value::Bool(index == 1), cx);
            },
            cx,
        )
        .w_full();
        div()
            .flex()
            .flex_col()
            .child(control)
            .child(ss(s, 13.0).mt(s.px(10.0)).child(
                "Full permissions: the agent won't ask for permission before edits or commands. At your own risk. Change it in Settings → Agents.",
            ))
            .into_any_element()
    }

    /// CDXC:Onboarding 2026-09-25 DECISION:
    /// User, of the Theme settings revamp: "in the setup modal, we'll just have the simple thing, and then we need to tell it that you can go to settings to modify the theme even more." The look card keeps only the simple controls of Settings -> Theme (Appearance, the colour squares behind Dark | Light tabs, Colourfulness, one Transparency row) and ends with "More theme options in Settings -> Theme", which opens Settings on the Theme page. This narrows the 2026-09-23 request to "add the transparency setting and theme (just the non advanced stuff) to the onboarding setup's last page"; Custom stays on the Theme page.
    fn render_look_card(&mut self, s: S, now: Instant, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings.clone();
        let dark_scheme = self.get_started.dark_scheme;
        // Built in the page's DOM order (appearance, scheme, swatches, sliders, link): tab order.
        let appearance = APPEARANCE
            .iter()
            .map(|(value, name)| {
                let selected = settings.sidebar_theme == *value;
                let value: &'static str = value;
                let key = SharedString::from(format!("appearance-{value}"));
                let (border, bg) = opt_colors(&key, selected);
                self.control(
                    s,
                    div()
                        .id(key.clone())
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .h(s.px(40.0))
                        .flex()
                        .flex_col()
                        .justify_center()
                        .px(s.px(16.0))
                        .rounded(s.px(10.0))
                        .border_1()
                        .border_color(border)
                        .bg(bg)
                        .cursor_pointer()
                        .when(selected, |this| {
                            this.shadow(vec![inset_shadow(white(0.04), 0.0, 1.0, 0.0, 0.0, s)])
                        })
                        .child(nm(s, 17.0, 500.0).child(appearance_label(
                            value,
                            name,
                            self.system_light,
                        ))),
                    key,
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| {
                        // The colour tab follows the pick, so the preview and the squares agree,
                        // unless a tab was chosen by hand.
                        if !this.get_started.scheme_picked {
                            this.get_started.dark_scheme = match value {
                                "system" => !this.system_light,
                                theme => theme.starts_with("dark"),
                            };
                        }
                        this.update_one("sidebarTheme", Value::String(value.to_string()), cx);
                    },
                )
            })
            .collect::<Vec<_>>();
        let schemes = [(true, "Dark"), (false, "Light")]
            .into_iter()
            .map(|(dark, name)| {
                let selected = dark_scheme == dark;
                let key = SharedString::from(format!("scheme-{name}"));
                // `.plook-scheme button`: the host's 120ms button transition on selection.
                let bg = interact::tween_color(
                    &key,
                    "bg",
                    rgba(90, 125, 235, if selected { 0.22 } else { 0.0 }),
                    interact::BUTTON_MS,
                );
                let color = interact::tween_color(
                    &key,
                    "color",
                    if selected {
                        hex(0xeef1f7)
                    } else {
                        hex(0x7c8598)
                    },
                    interact::BUTTON_MS,
                );
                self.control(
                    s,
                    div()
                        .id(key.clone())
                        .relative()
                        .py(s.px(4.0))
                        .px(s.px(12.0))
                        .rounded(s.px(6.0))
                        .font_family(super::fonts::dm_sans())
                        .text_size(s.px(12.5))
                        .line_height(s.px(12.5 * LH_DM_SANS))
                        .cursor_pointer()
                        .bg(bg)
                        .text_color(color)
                        .child(name),
                    key,
                    interact::Ring::new(6.0, 0.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| {
                        this.get_started.dark_scheme = dark;
                        this.get_started.scheme_picked = true;
                        cx.notify();
                    },
                )
            })
            .collect::<Vec<_>>();
        let transparency_on = settings.window_glass != "opaque";
        let colourfulness = colourfulness_step(&settings);
        let (exact, nearest) = transparency_strength(&settings);
        let display_step = colourfulness_display_step(settings.sidebar_contrast);
        let presets: Vec<ThemeSwatchPreset> = if dark_scheme {
            self.theme.dark.clone()
        } else {
            self.theme.light.clone()
        };
        let selected_preset = if dark_scheme {
            settings.dark_theme_preset.clone()
        } else {
            settings.light_theme_preset.clone()
        };
        for preset in &presets {
            let key = format!("{}-{}-{}", dark_scheme, preset.value, display_step);
            if !self.get_started.swatches.contains_key(&key)
                && let Some(image) = swatch_image(dark_scheme, preset, display_step)
            {
                self.get_started.swatches.insert(key, image);
            }
        }
        let swatches = presets
            .iter()
            .map(|preset| {
                let key = format!("{}-{}-{}", dark_scheme, preset.value, display_step);
                let image = self.get_started.swatches.get(&key).cloned();
                let selected = preset.value == selected_preset;
                let value = preset.value.clone();
                let label_text = theme_preset_label(dark_scheme, &preset.value);
                let key = SharedString::from(format!("swatch-{}-{}", dark_scheme, preset.value));
                // `.theme-colour-square { transition: transform 120ms ease }`, lifted 1px on hover;
                // the swatch keeps `cursor: default` (settings-theme.css (deleted 2026-11-06) outranks the page's button rule).
                let lift = interact::tween_value(
                    &key,
                    "lift",
                    if interact::hovered(&key) { -1.0 } else { 0.0 },
                    120,
                );
                let swatch = div()
                    .id(key.clone())
                    .flex_1()
                    .min_w_0()
                    .relative()
                    .tooltip(move |window, cx| Tooltip::new(label_text.clone()).build(window, cx))
                    .child(
                        div()
                            .relative()
                            .top(s.px(lift))
                            .w_full()
                            .h(s.px(33.0))
                            .rounded(s.px(8.0))
                            .shadow(vec![
                                inset_shadow(white(0.14), 0.0, 1.0, 0.0, 0.0, s),
                                inset_shadow(white(0.06), 0.0, 0.0, 0.0, 1.0, s),
                                shadow(black(0.3), 0.0, 4.0, 10.0, 0.0, s),
                            ])
                            .when_some(image, |this, image| {
                                this.child(
                                    img(image)
                                        .absolute()
                                        .left_0()
                                        .top_0()
                                        .size_full()
                                        .rounded(s.px(8.0))
                                        .object_fit(ObjectFit::Fill),
                                )
                            })
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .size_full()
                                    .rounded(s.px(8.0))
                                    .shadow(vec![
                                        inset_shadow(white(0.14), 0.0, 1.0, 0.0, 0.0, s),
                                        inset_shadow(white(0.06), 0.0, 0.0, 0.0, 1.0, s),
                                    ]),
                            )
                            .when(selected, |this| {
                                this.child(
                                    div()
                                        .absolute()
                                        .left(s.px(-4.0))
                                        .top(s.px(-4.0))
                                        .right(s.px(-4.0))
                                        .bottom(s.px(-4.0))
                                        .rounded(s.px(12.0))
                                        .border(s.px(2.0))
                                        .border_color(hex(0x5a7deb)),
                                )
                            }),
                    );
                self.control(
                    s,
                    swatch,
                    key,
                    interact::Ring::new(8.0, 0.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| {
                        let dark = this.get_started.dark_scheme;
                        this.pick_theme_preset(dark, value.clone(), cx);
                    },
                )
                .into_any_element()
            })
            .collect::<Vec<_>>();
        let readout = theme_preset_label(dark_scheme, &selected_preset);
        let colourfulness_value = colourfulness.unwrap_or(COLOURFULNESS_LAST_POSITION / 2) as f64;
        let colourfulness_text = colourfulness.map_or("Custom".to_string(), |step| {
            colourfulness_name(colourfulness_points(step)).to_string()
        });
        let transparency_text = if !transparency_on {
            "Off".to_string()
        } else if exact.is_none() {
            "Custom".to_string()
        } else {
            format!("{}%", nearest as i64)
        };
        let transparency_thumb = self.thumb("transparency", transparency_on, now);
        let colourfulness_row = self.slider_row(
            s,
            "colourfulness",
            "Colourfulness",
            None,
            colourfulness_value / COLOURFULNESS_LAST_POSITION as f64,
            &colourfulness_text,
            false,
            (0.0, COLOURFULNESS_LAST_POSITION as f64),
            cx,
        );
        let transparency_toggle = self
            .control(
                s,
                toggle(
                    s,
                    "transparency-toggle",
                    transparency_on,
                    transparency_thumb,
                    ToggleSize::Sm,
                    false,
                ),
                "transparency-toggle",
                interact::Ring::new(9.5, 1.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| {
                    cx.stop_propagation();
                    this.toggle_transparency(cx);
                },
            )
            .into_any_element();
        let transparency_row = self.slider_row(
            s,
            "transparency",
            "Transparency",
            Some(transparency_toggle),
            nearest / 100.0,
            &transparency_text,
            !transparency_on,
            (0.0, 100.0),
            cx,
        );
        // `.plook-more-link`: the host's 120ms button transition.
        let more_color = interact::hover_color(
            "more-theme-options",
            "color",
            hex(0x8fb0ff),
            hex(0xb3c8ff),
            interact::BUTTON_MS,
        );
        let more_link = self.control(
            s,
            sans(s, 12.5, 400.0, more_color)
                .id("more-theme-options")
                .relative()
                .self_start()
                .mt(s.px(10.0))
                .cursor_pointer()
                .child("More theme options in Settings → Themes ›"),
            "more-theme-options",
            interact::Ring::new(0.0, 0.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| this.finish(FinishTarget::Settings(Some("theme")), cx),
        );
        pcard(s)
            .child(div().mb(s.px(10.0)).child(label(s, "Appearance")))
            .child(
                div()
                    .flex()
                    .gap(s.px(8.0))
                    .mb(s.px(16.0))
                    .children(appearance),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(s.px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(label(s, "Theme colour"))
                            .child(
                                div()
                                    .font_family("Inter")
                                    .text_size(s.px(11.5))
                                    .text_color(hex(0x7c8598))
                                    .ml(s.px(6.0))
                                    .child(readout),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .p(s.px(2.0))
                            .rounded(s.px(8.0))
                            .border_1()
                            .border_color(white(0.07))
                            .bg(white(0.02))
                            .children(schemes),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(s.px(6.0))
                    .mb(s.px(14.0))
                    .children(swatches),
            )
            .child(colourfulness_row)
            .child(transparency_row)
            .child(self.render_window_art(s, now, cx))
            .child(more_link)
            .into_any_element()
    }

    /// Whether the look being picked is the light one: Appearance, with System as the computer has it.
    fn look_light(&self) -> bool {
        match self.settings.sidebar_theme.as_str() {
            "system" => self.system_light,
            theme => !theme.starts_with("dark"),
        }
    }

    /// The window's backdrop as the settings picked here would draw it, in `style` when given
    /// (else the picked one), or `None` without an artwork or while it is being drawn.
    fn art_preview_for(&self, style: Option<&str>, cx: &mut Context<Self>) -> Option<ArtPreview> {
        let mut object = self.settings.to_settings_values().map().clone();
        if let Some(style) = style {
            object.insert("windowArtStyle".into(), Value::String(style.to_string()));
        }
        let source = self.art_preview.clone();
        source(&object, self.look_light(), cx)
    }

    /// CDXC:Onboarding 2026-11-25 DECISION:
    /// User, of PR 757's art styles: "the dither and ascii are nice! Will offer them in the onboarding so people can pick if they want them on or off", then on the Live page picked: Picture, ASCII, Dither and the Black and white switch, in the Appearance card on Step 2 (setup stays 3 steps), as picture tiles plus a live preview, Picture already picked with a small "New" tag on ASCII and Dither. Their note: "allow black and white even if picking image pls ... if i choose black and white i want it to just affect the image yea? so i can pick a tint color as i like and its not affected by the image's color". Each tile is the real backdrop (`window_art_style_preview` in helpers/window_art.rs: the theme's artwork drawn in that style, dimmed, under the work area's tint), in the appearance being picked, so a style that is faint in light mode looks faint here too. Black and white greys only the artwork; the theme colour laid over it keeps its colour. Today's default stays Picture with Black and white off.
    fn render_window_art(&mut self, s: S, now: Instant, cx: &mut Context<Self>) -> AnyElement {
        // About a third of the card's 620-point column, less the gaps.
        const TILE_WIDTH: f32 = 200.0;
        const TILE_HEIGHT: f32 = 62.0;
        // Each tile is a close-up of the backdrop's bottom left, where the artwork is strongest,
        // so ASCII characters and Dither dots still read at this size.
        const TILE_ZOOM: f32 = 2.2;
        const STYLES: [(&str, &str, bool); 3] = [
            ("picture", "Picture", false),
            ("ascii", "ASCII", true),
            ("dither", "Dither", true),
        ];
        let picked = self.settings.window_art_style.clone();
        let readout = STYLES
            .iter()
            .find(|(value, _, _)| *value == picked)
            .map_or("Picture", |(_, name, _)| *name);
        let tiles = STYLES
            .into_iter()
            .map(|(value, name, new)| {
                let preview = self.art_preview_for(Some(value), cx);
                let selected = picked == value;
                let key = SharedString::from(format!("window-art-{value}"));
                let lift = interact::tween_value(
                    &key,
                    "lift",
                    if interact::hovered(&key) { -1.0 } else { 0.0 },
                    120,
                );
                let picture = div()
                    .relative()
                    .w_full()
                    .h(s.px(TILE_HEIGHT))
                    .rounded(s.px(8.0))
                    .overflow_hidden()
                    .bg(white(0.04))
                    .when_some(preview, |this, preview| {
                        let size = preview.image.size(0);
                        let tall = size.height.0 as f32 / (size.width.0 as f32).max(1.0);
                        // The artwork rises from the window's bottom, so the tile shows its bottom.
                        this.bg(hex(preview.base))
                            .child(
                                img(preview.image)
                                    .absolute()
                                    .left_0()
                                    .bottom_0()
                                    .w(s.px(TILE_WIDTH * TILE_ZOOM))
                                    .h(s.px(TILE_WIDTH * TILE_ZOOM * tall))
                                    .object_fit(ObjectFit::Fill),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .size_full()
                                    .bg(hex(preview.base).opacity(preview.work_area)),
                            )
                    })
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .size_full()
                            .rounded(s.px(8.0))
                            .shadow(vec![
                                inset_shadow(white(0.14), 0.0, 1.0, 0.0, 0.0, s),
                                inset_shadow(white(0.06), 0.0, 0.0, 0.0, 1.0, s),
                            ]),
                    );
                let framed = div()
                    .relative()
                    .top(s.px(lift))
                    .shadow(vec![shadow(black(0.3), 0.0, 4.0, 10.0, 0.0, s)])
                    .rounded(s.px(8.0))
                    .child(picture)
                    .when(selected, |this| {
                        this.child(
                            div()
                                .absolute()
                                .left(s.px(-4.0))
                                .top(s.px(-4.0))
                                .right(s.px(-4.0))
                                .bottom(s.px(-4.0))
                                .rounded(s.px(12.0))
                                .border(s.px(2.0))
                                .border_color(hex(0x5a7deb)),
                        )
                    });
                let caption = div()
                    .mt(s.px(9.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(s.px(6.0))
                    .child(
                        sans(
                            s,
                            13.0,
                            if selected { 500.0 } else { 400.0 },
                            if selected {
                                hex(0xeef1f7)
                            } else {
                                hex(0xaeb6c4)
                            },
                        )
                        .child(name),
                    )
                    .when(new, |this| {
                        this.child(
                            div()
                                .flex_none()
                                .px(s.px(5.0))
                                .py(s.px(2.0))
                                .rounded(s.px(4.0))
                                .border_1()
                                .border_color(white(0.14))
                                .bg(white(0.06))
                                .child(label_sized(s, "New", 9.0)),
                        )
                    });
                self.control(
                    s,
                    div()
                        .id(key.clone())
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .cursor_pointer()
                        .child(framed)
                        .child(caption),
                    key,
                    interact::Ring::new(8.0, 0.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| {
                        this.update_one("windowArtStyle", Value::String(value.to_string()), cx);
                    },
                )
                .into_any_element()
            })
            .collect::<Vec<_>>();
        let mono_on = self.settings.window_art_monochrome;
        let mono_thumb = self.thumb("art-monochrome", mono_on, now);
        let mono_toggle = self.control(
            s,
            toggle(
                s,
                "art-monochrome-toggle",
                mono_on,
                mono_thumb,
                ToggleSize::Sm,
                false,
            ),
            "art-monochrome-toggle",
            interact::Ring::new(9.5, 1.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                cx.stop_propagation();
                let next = !this.settings.window_art_monochrome;
                this.update_one("windowArtMonochrome", Value::Bool(next), cx);
            },
        );
        div()
            .mt(s.px(12.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .mb(s.px(12.0))
                    .child(label(s, "Window art"))
                    .child(
                        div()
                            .font_family("Inter")
                            .text_size(s.px(11.5))
                            .text_color(hex(0x7c8598))
                            .ml(s.px(6.0))
                            .child(readout),
                    ),
            )
            .child(div().flex().gap(s.px(10.0)).children(tiles))
            .child(
                div()
                    .mt(s.px(12.0))
                    .flex()
                    .items_center()
                    .gap(s.px(14.0))
                    .h(s.px(34.0))
                    .child(
                        sans(s, 15.0, 400.0, hex(0xeef1f7))
                            .flex_none()
                            .child("Black and white"),
                    )
                    .child(mono_toggle)
                    .child(
                        ss(s, 12.5)
                            .mt(s.px(0.0))
                            .flex_1()
                            .min_w_0()
                            .child("Greys out the artwork only; your theme colour stays."),
                    ),
            )
            .into_any_element()
    }

    /// `.pstep`: label, an optional switch, the range and its value. `fraction` is where the thumb sits.
    #[allow(clippy::too_many_arguments)]
    fn slider_row(
        &mut self,
        s: S,
        id: &'static str,
        name: &str,
        leading: Option<AnyElement>,
        fraction: f64,
        value_text: &str,
        disabled: bool,
        (min, max): (f64, f64),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bounds = self
            .get_started
            .slider_bounds
            .entry(id)
            .or_insert_with(|| Rc::new(Cell::new(None)))
            .clone();
        let bounds_for_paint = bounds.clone();
        let value_at = move |x: Pixels| -> Option<f64> {
            let bounds = bounds.get()?;
            let left = bounds.left().as_f32();
            let width = bounds.size.width.as_f32().max(1.0);
            let fraction = ((x.as_f32() - left) / width).clamp(0.0, 1.0) as f64;
            Some(min + (max - min) * fraction)
        };
        let value_down = value_at.clone();
        let value_move = value_at;
        let key = SharedString::from(format!("slider-{id}"));
        // The thumb's focus glow is `:focus-visible` only (a clicked range shows the resting one).
        let focused = self.focus.is_visible(&key);
        let fraction = fraction.clamp(0.0, 1.0) as f32;
        let track = div()
            .id(key.clone())
            .relative()
            .flex_1()
            .min_w_0()
            .h(s.px(18.0))
            .when(disabled, |this| this.opacity(0.35))
            .when(!disabled, |this| this.cursor_pointer())
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
                    .top(s.px(7.0))
                    .h(s.px(4.0))
                    .rounded(s.px(2.0))
                    .bg(white(0.1)),
            )
            .child(
                div()
                    .absolute()
                    .top(s.px(1.0))
                    .left(gpui::relative(fraction))
                    .ml(s.px(-8.0 * (2.0 * fraction)))
                    .size(s.px(16.0))
                    .rounded_full()
                    .bg(hex(0xeef1f7))
                    .shadow(vec![shadow(
                        rgba(90, 125, 235, if focused { 0.55 } else { 0.25 }),
                        0.0,
                        0.0,
                        0.0,
                        4.0,
                        s,
                    )]),
            )
            .when(!disabled, |this| {
                this.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        this.get_started.dragging = Some(id);
                        if let Some(value) = value_down(event.position.x) {
                            this.set_slider(id, value, cx);
                        }
                        cx.notify();
                    }),
                )
                .on_mouse_move(
                    cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                        if this.get_started.dragging != Some(id) || !event.dragging() {
                            return;
                        }
                        if let Some(value) = value_move(event.position.x) {
                            this.set_slider(id, value, cx);
                        }
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.get_started.dragging = None;
                        cx.notify();
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.get_started.dragging = None;
                        cx.notify();
                    }),
                )
            });
        // A disabled range is out of the tab order.
        let track = if disabled {
            track
        } else {
            self.focusable(track, key, cx)
        };
        div()
            .flex()
            .items_center()
            .gap(s.px(14.0))
            .h(s.px(34.0))
            .mb(s.px(6.0))
            .child(
                sans(s, 15.0, 400.0, hex(0xeef1f7))
                    .w(s.px(110.0))
                    .flex_none()
                    .child(name.to_string()),
            )
            .children(leading)
            .child(track)
            .child(
                sans(
                    s,
                    13.0,
                    400.0,
                    if disabled {
                        hex(0x6f7788)
                    } else {
                        hex(0xaeb6c4)
                    },
                )
                .w(s.px(64.0))
                .flex_none()
                .flex()
                .justify_end()
                .child(value_text.to_string()),
            )
            .into_any_element()
    }
}

/// `.opt`: `transition: border-color 0.2s, background 0.2s`; `:hover:not(.sel)` lightens the border.
fn opt_colors(key: &str, selected: bool) -> (gpui::Hsla, gpui::Hsla) {
    let border = interact::tween_color(
        key,
        "border",
        if selected {
            rgba(90, 125, 235, 0.42)
        } else if interact::hovered(key) {
            white(0.18)
        } else {
            white(0.07)
        },
        200,
    );
    let bg = interact::tween_color(
        key,
        "bg",
        if selected {
            rgba(28, 38, 68, 0.4)
        } else {
            white(0.015)
        },
        200,
    );
    (border, bg)
}

/// `.pcard`: the Get started cards.
fn pcard(s: S) -> Div {
    glass(s)
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .pt(s.px(18.0))
        .px(s.px(20.0))
        .pb(s.px(20.0))
        .bg(gpui::linear_gradient(
            180.0,
            gpui::linear_color_stop(rgba(18, 21, 29, 0.55), 0.0),
            gpui::linear_color_stop(rgba(10, 12, 18, 0.55), 1.0),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_colour_tab_follows_a_system_change_unless_picked() {
        let mut settings = OnboardingSettings::from_object(&serde_json::Map::new());
        settings.sidebar_theme = "system".to_string();
        let mut state = GetStartedState::default();
        // Opened before the Linux desktop portal answered: read as light.
        state.reset(&settings, true);
        assert!(!state.dark_scheme);
        state.system_appearance_changed(&settings, false);
        assert!(state.dark_scheme, "the portal said dark");
        state.scheme_picked = true;
        state.system_appearance_changed(&settings, true);
        assert!(state.dark_scheme, "a hand-picked tab stays");
        settings.sidebar_theme = "plain-light".to_string();
        let mut fixed = GetStartedState::default();
        fixed.reset(&settings, false);
        fixed.system_appearance_changed(&settings, false);
        assert!(!fixed.dark_scheme, "Light does not follow the system");
    }

    #[test]
    fn system_appearance_names_what_it_resolves_to() {
        assert_eq!(appearance_label("system", "System", true), "System · Light");
        assert_eq!(appearance_label("system", "System", false), "System · Dark");
        assert_eq!(appearance_label("dark-2", "Dark", true), "Dark");
    }
}
