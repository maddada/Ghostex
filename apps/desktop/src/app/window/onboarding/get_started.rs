//! Panel 5, Get started: the project card (folder, first agent, default session view) and the look
//! card (appearance, theme colour, Colourfulness, transparency) (panels/get-started.tsx (deleted 2026-10-01),
//! styles/get-started.css, settings-modal/theme-simple-controls.tsx (deleted 2026-10-01), styles/settings-theme.css).
use super::GpuiOnboardingWindow;
use super::interact;
use super::model::{
    COLOURFULNESS_LAST_POSITION, OnboardingSettings, ThemeSwatchPreset, colourfulness_display_step,
    colourfulness_name, colourfulness_points, colourfulness_step, default_agent_id,
    installed_agents, matching_preset, theme_preset_label, transparency_strength,
    transparency_strength_patch,
};
use super::primitives::*;
use super::stage::*;
use super::welcome::foot_actions;
use super::{FinishTarget, OnboardingCommand};
use gpui::StyledImage as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, Div, FontWeight, InteractiveElement as _, IntoElement,
    MouseButton, ObjectFit, ParentElement as _, Pixels, RenderImage, SharedString,
    StatefulInteractiveElement as _, Styled as _, canvas, div, img,
};
use gpui_component::tooltip::Tooltip;
use serde_json::Value;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Up to this many "Start with" choices keep the name-and-detail tiles in one row; more become chips.
const MAX_TILE_ROW: usize = 4;
const CARDS_LEFT: f32 = 156.0;
const CARDS_TOP: f32 = 290.0;
const CARDS_WIDTH: f32 = 1360.0;
/// The React waiter's ceiling for the first-launch create (`ADD_PROJECT_DIALOG_ADD_TIMEOUT_MS`).
const OPEN_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(crate) struct GetStartedState {
    opening: bool,
    open_error: Option<String>,
    request_id: Option<String>,
    request_deadline: Option<Instant>,
    request_path: Option<String>,
    /// Which appearance's colours the look card shows.
    dark_scheme: bool,
    picked_dark: bool,
    picked_light: bool,
    pub(crate) swatches: HashMap<String, Arc<RenderImage>>,
    slider_bounds: HashMap<&'static str, Rc<Cell<Option<Bounds<Pixels>>>>>,
    dragging: Option<&'static str>,
}

impl GetStartedState {
    pub(crate) fn reset(&mut self, settings: &OnboardingSettings, system_light: bool) {
        self.opening = false;
        self.open_error = None;
        self.request_id = None;
        self.request_deadline = None;
        self.request_path = None;
        self.dark_scheme = match settings.sidebar_theme.as_str() {
            "system" => !system_light,
            theme => theme.starts_with("dark"),
        };
        self.picked_dark = false;
        self.picked_light = false;
        self.dragging = None;
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
    fn start_with(&self) -> String {
        self.flow
            .start_with
            .clone()
            .or_else(|| default_agent_id(&self.settings, &self.agents))
            .unwrap_or_else(|| "terminal".to_string())
    }

    fn open_ghostex(&mut self, cx: &mut Context<Self>) {
        let folder = self
            .picked_folder
            .clone()
            .unwrap_or_default()
            .trim()
            .to_string();
        if !folder.is_empty() {
            if self.get_started.opening {
                return;
            }
            let request_id = format!(
                "first-launch-project-{}-{}",
                chrono::Utc::now().timestamp_millis(),
                self.next_request_id()
            );
            self.get_started.opening = true;
            self.get_started.open_error = None;
            self.get_started.request_id = Some(request_id.clone());
            self.get_started.request_path = Some(folder.clone());
            self.get_started.request_deadline = Some(Instant::now() + OPEN_TIMEOUT);
            let agent_id = self.start_with();
            self.send(
                OnboardingCommand::FinishFirstLaunch {
                    request_id,
                    agent_id,
                    path: folder,
                },
                cx,
            );
            cx.notify();
            return;
        }
        if self.has_projects {
            self.finish(FinishTarget::None, cx);
        }
    }

    pub(super) fn get_started_finish_result(
        &mut self,
        request_id: &str,
        ok: bool,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.get_started.request_id.as_deref() != Some(request_id) {
            return;
        }
        self.get_started.request_id = None;
        self.get_started.request_deadline = None;
        if ok {
            self.flow.finished_path = self.get_started.request_path.take();
            self.flow.finished = true;
            self.panel = super::stage::PANEL_COUNT;
            self.scene_started = Instant::now();
            self.mount_panel(self.scene_started, cx);
        } else {
            self.get_started.opening = false;
            self.get_started.open_error = Some(
                error
                    .map(|error| error.trim().to_string())
                    .filter(|error| !error.is_empty())
                    .unwrap_or_else(|| "Ghostex could not open the project.".to_string()),
            );
        }
        cx.notify();
    }

    pub(super) fn tick_get_started(&mut self, now: Instant) {
        if let Some(deadline) = self.get_started.request_deadline
            && now >= deadline
        {
            self.get_started.request_deadline = None;
            self.get_started.request_id = None;
            self.get_started.opening = false;
            self.get_started.open_error = Some("Opening the project timed out.".to_string());
        }
    }

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
        if turning_on && cfg!(target_os = "windows") {
            notes.push("On Windows, turning it on takes effect the next time Ghostex starts.");
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
                self.update_settings(transparency_strength_patch(value), cx);
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
        if self.panel != 5 || self.flow.finished {
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

    pub(super) fn render_get_started(
        &mut self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let default_agent = default_agent_id(&self.settings, &self.agents);
        let installed = installed_agents(&self.agents);
        let mut ordered: Vec<_> = installed
            .iter()
            .filter(|agent| Some(agent.agent_id.as_str()) == default_agent.as_deref())
            .cloned()
            .collect();
        ordered.extend(
            installed
                .iter()
                .filter(|agent| Some(agent.agent_id.as_str()) != default_agent.as_deref())
                .cloned(),
        );
        let mut tiles: Vec<(String, String, &str)> = ordered
            .iter()
            .map(|agent| {
                (
                    agent.agent_id.clone(),
                    agent.name.clone(),
                    if Some(agent.agent_id.as_str()) == default_agent.as_deref() {
                        "Default agent"
                    } else {
                        "Switch anytime"
                    },
                )
            })
            .collect();
        tiles.push(("terminal".into(), "Terminal".into(), "No agent yet"));
        let start_with = self.start_with();
        let session_view = self.settings.preferred_interface.clone();
        let folder = self
            .picked_folder
            .clone()
            .unwrap_or_default()
            .trim()
            .to_string();
        let can_open = !folder.is_empty() || self.has_projects;
        let opening = self.get_started.opening;

        let mut out = Vec::new();
        out.push(eyebrow(s, 486.0, 150.0, Some(700.0), "Get started").into_any_element());
        out.push(
            heading(
                s,
                336.0,
                182.0,
                1000.0,
                48.0,
                "Open your first project in Ghostex.",
                None,
                true,
            )
            .into_any_element(),
        );
        out.push(
            sub(s, 336.0, 252.0, 1000.0, 16.5, true)
                .child("One folder, one agent, one default view and a look you like. Everything else can change later.")
                .into_any_element(),
        );
        let option =
            |id: SharedString, selected: bool, name: &str, detail: Option<&str>, height: f32| {
                let (border, bg) = opt_colors(&id, selected);
                div()
                    .id(id)
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h(s.px(height))
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
                    .child(
                        nm(s, 17.0, 500.0)
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(name.to_string()),
                    )
                    .when_some(detail, |this, detail| {
                        this.child(
                            ss(s, 13.5)
                                .mt(s.px(7.0))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(detail.to_string()),
                        )
                    })
            };
        // `.choose { transition: color 0.2s }`. Built before the tiles: tab order follows the page.
        let choose_color =
            interact::hover_color("choose-folder", "color", hex(0x2f4fae), hex(0x6d93ff), 200);
        let choose_folder = self.control(
            s,
            sans(s, 14.5, 400.0, choose_color)
                .id("choose-folder")
                .relative()
                .cursor_pointer()
                .child("Choose folder"),
            "choose-folder",
            interact::Ring::new(0.0, 0.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| this.send(OnboardingCommand::PickProjectFolder, cx),
        );
        let tiles_row: AnyElement = if tiles.len() <= MAX_TILE_ROW {
            div()
                .flex()
                .gap(s.px(8.0))
                .mb(s.px(20.0))
                .children(tiles.iter().map(|(id, name, detail)| {
                    let id_owned = id.clone();
                    let key = SharedString::from(format!("start-{id}"));
                    self.control(
                        s,
                        option(key.clone(), start_with == *id, name, Some(detail), 72.0),
                        key,
                        interact::Ring::new(10.0, 1.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| {
                            this.flow.start_with = Some(id_owned.clone());
                            cx.notify();
                        },
                    )
                }))
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_wrap()
                .gap(s.px(8.0))
                .mb(s.px(20.0))
                .children(tiles.iter().map(|(id, name, detail)| {
                    let id_owned = id.clone();
                    let detail = detail.to_string();
                    let selected = start_with == *id;
                    let key = SharedString::from(format!("start-{id}"));
                    let (border, bg) = opt_colors(&key, selected);
                    let tile = div()
                        .id(key.clone())
                        .relative()
                        .flex()
                        .items_center()
                        .h(s.px(40.0))
                        .px(s.px(14.0))
                        .rounded(s.px(10.0))
                        .border_1()
                        .font_family(super::fonts::dm_sans())
                        .font_weight(FontWeight(500.0))
                        .text_size(s.px(14.5))
                        .text_color(hex(0xeef1f7))
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .border_color(border)
                        .bg(bg)
                        .when(selected, |this| {
                            this.shadow(vec![inset_shadow(white(0.04), 0.0, 1.0, 0.0, 0.0, s)])
                        })
                        .tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
                        .child(name.clone());
                    self.control(
                        s,
                        tile,
                        key,
                        interact::Ring::new(10.0, 1.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| {
                            this.flow.start_with = Some(id_owned.clone());
                            cx.notify();
                        },
                    )
                }))
                .into_any_element()
        };
        let project_card = pcard(s)
            .child(div().mb(s.px(14.0)).child(label(s, "Project folder")))
            .child(
                div()
                    .relative()
                    .h(s.px(50.0))
                    .mb(s.px(24.0))
                    .flex()
                    .items_center()
                    .gap(s.px(14.0))
                    .pl(s.px(26.0))
                    .pr(s.px(18.0))
                    .rounded(s.px(10.0))
                    .border_1()
                    .border_color(white(0.06))
                    .bg(white(0.02))
                    .child(icon(s, "folder", 22.0, 1.6, hex(0x8e97a8)))
                    .child(
                        sans(
                            s,
                            16.5,
                            400.0,
                            if folder.is_empty() {
                                hex(0x8e97a8)
                            } else {
                                hex(0xeef1f7)
                            },
                        )
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(if folder.is_empty() {
                            "Choose a folder to start in".to_string()
                        } else {
                            folder.clone()
                        }),
                    )
                    .child(choose_folder),
            )
            .child(div().mb(s.px(14.0)).child(label(s, "Start with")))
            .child(tiles_row)
            .child(div().mb(s.px(14.0)).child(label(s, "Default session view")))
            .child(
                div().flex().gap(s.px(8.0)).children(
                    [
                        ("chat", "Chat", "Cleaner agent conversation"),
                        ("terminal", "Terminal", "Raw CLI interface"),
                    ]
                    .into_iter()
                    .map(|(id, name, detail)| {
                        let key = SharedString::from(format!("view-{id}"));
                        self.control(
                            s,
                            option(key.clone(), session_view == id, name, Some(detail), 72.0),
                            key,
                            interact::Ring::new(10.0, 1.0),
                            interact::Keys::EnterSpace,
                            cx,
                            move |this, _, cx| {
                                this.update_one(
                                    "preferredAgentInterface",
                                    Value::String(id.to_string()),
                                    cx,
                                );
                            },
                        )
                    }),
                ),
            );
        let look_card = self.render_look_card(s, now, cx);
        let note: AnyElement = if let Some(error) = &self.get_started.open_error {
            div()
                .text_color(hex(0xff6b62))
                .child(error.clone())
                .into_any_element()
        } else if opening {
            div()
                .child("Adding the project and opening its first session…")
                .into_any_element()
        } else if can_open {
            div()
                .child("That's it. The workspace teaches the deeper features once you are inside.")
                .into_any_element()
        } else {
            div()
                .child("Choose a folder above to open your first project.")
                .into_any_element()
        };
        out.push(
            abs(s, CARDS_LEFT, CARDS_TOP, Some(CARDS_WIDTH), None)
                .flex()
                .flex_col()
                .gap(s.px(40.0))
                .child(
                    div()
                        .flex()
                        .items_stretch()
                        .gap(s.px(40.0))
                        .child(project_card)
                        .child(look_card),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .justify_center()
                        .font_family(super::fonts::dm_sans())
                        .text_size(s.px(15.0))
                        .line_height(s.px(15.0 * 1.45))
                        .text_color(hex(0xb2b9c6))
                        .child(note),
                )
                .into_any_element(),
        );
        out.push(
            foot_actions(s, 5)
                .child({
                    let later = ghost(
                        s,
                        "advanced-later",
                        "Advanced settings later",
                        16.0,
                        opening,
                    );
                    if opening {
                        later
                    } else {
                        self.control(
                            s,
                            later,
                            "advanced-later",
                            interact::Ring::new(0.0, 0.0),
                            interact::Keys::EnterSpace,
                            cx,
                            |this, _, cx| {
                                if !this.get_started.opening {
                                    this.finish(FinishTarget::Settings(None), cx);
                                }
                            },
                        )
                    }
                })
                .child({
                    let open = cta(
                        s,
                        "open-ghostex",
                        if opening {
                            div()
                                .flex()
                                .items_center()
                                .gap(s.px(8.0))
                                .child(spinner(s, 18.0, 2.0, self.opened_at, now))
                                .child("Opening…")
                                .into_any_element()
                        } else {
                            div().child("Open Ghostex").into_any_element()
                        },
                        true,
                        !opening,
                        !can_open || opening,
                        CtaSize::Foot,
                    );
                    if !can_open || opening {
                        open
                    } else {
                        self.control(
                            s,
                            open,
                            "open-ghostex",
                            interact::Ring::new(10.0, 1.0),
                            interact::Keys::EnterSpace,
                            cx,
                            move |this, _, cx| {
                                if can_open && !this.get_started.opening {
                                    this.open_ghostex(cx);
                                }
                            },
                        )
                    }
                })
                .into_any_element(),
        );
        out
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
                        .child(nm(s, 17.0, 500.0).child(name.to_string())),
                    key,
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| {
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
                // the swatch keeps `cursor: default` (settings-theme.css outranks the page's button rule).
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
        let colourfulness_value =
            colourfulness.map_or(COLOURFULNESS_LAST_POSITION / 2, |step| step) as f64;
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
                .child("More theme options in Settings → Theme ›"),
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
            .child(more_link)
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
