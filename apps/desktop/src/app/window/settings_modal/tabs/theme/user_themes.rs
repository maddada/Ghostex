//! Themes > Your themes: looks the user saved, to switch between with one click. Save current
//! theme names the look on screen and keeps every Themes setting it is made of (`savedThemes`);
//! each saved look is a tile showing a small Ghostex window in its own colours (sidebar, work
//! area, accent, and its artwork when it has one); its … button opens Rename, Update to current
//! look and Delete under the strip; switching keeps Undo until the next switch.
//!
//! CDXC:Theming 2026-11-22 DECISION:
//! User (Q4473 note on the Themes reorganisation): "let the user save his current theme so he can return to it later and switch between themes; show a small preview for his themes: a low-fidelity Ghostex window in the theme's colours so he recognises it". A saved theme is a snapshot of the Themes page's own settings (the `theming` catalog section and the custom colours' depths), without the per-computer files a look does not own (the glass picture and videos, the terminal background image) and without Your themes itself. They are one `savedThemes` setting in the Appearance sync category, so they follow the user to their other computers; a Your image picture stays on the computer it was picked on (`windowArtImage` is left out of sync), so there a theme that uses it paints a solid window until a picture is chosen. Switching saves the snapshot in one settings save; nothing else on the page changes. There was no stored user theme before this (the theme editor and the agent-made looks write the settings directly), so this is the first schema: `[{ id, name, savedAt, settings: { <key>: <value> } }]`.
//! SEE-ALSO: packages/settings-catalog/src/general/theme.rs (`savedThemes`), packages/gx-core/src/settings_sync/settings_keys.rs (`APPEARANCE_LEFT_OUT`).
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::settings_catalog;
use super::super::super::fields::{
    ButtonVariant, SizedButtonSize, SizedButtonVariant, settings_icon_button,
    settings_sized_button, settings_text_input,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::SettingsValues;
use super::artwork::{Backdrop, backdrop, file_preview, your_image_path};
use super::colours::{Rgb, accent_for_tint, preset_controls, preview_colours, preview_scheme_dark};
use super::presets::{TileSpec, theme_tile};
use super::prompt_gallery::screenshot_preview;
use super::role_defaults::role_default;
use super::roles::{hex_of, saved_role};
use super::{ThemeTab, gallery};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};

/// The setting the saved themes live in.
const SAVED_THEMES_KEY: &str = "savedThemes";

/// Themes page settings a saved theme leaves out: the files that belong to the computer, and the
/// saved themes themselves.
const NOT_IN_A_THEME: &[&str] = &[
    SAVED_THEMES_KEY,
    "windowGlassImageDark",
    "windowGlassImageLight",
    "windowGlassVideoDark",
    "windowGlassVideoLight",
    "terminalBackgroundImage",
];

/// Settings a look is made of that have no row of their own: the custom colours' depths, which
/// Colourfulness saves with the rows.
const WITHOUT_ROWS: &[&str] = &[
    "customSidebarTitlebarBackgroundDarknessPercent",
    "customSidebarTitlebarLightBackgroundLightnessPercent",
];

/// One saved theme.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SavedTheme {
    pub(super) id: String,
    pub(super) name: String,
    saved_at: String,
    pub(super) settings: Map<String, Value>,
}

impl SavedTheme {
    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "savedAt": self.saved_at,
            "settings": self.settings,
        })
    }
}

/// The saved themes, skipping any entry that does not read as one.
pub(super) fn saved_themes(values: &SettingsValues) -> Vec<SavedTheme> {
    values
        .value(SAVED_THEMES_KEY)
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let text =
                        |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_string);
                    Some(SavedTheme {
                        id: text("id").filter(|id| !id.is_empty())?,
                        name: text("name").unwrap_or_default(),
                        saved_at: text("savedAt").unwrap_or_default(),
                        settings: entry.get("settings")?.as_object()?.clone(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The keys a saved theme keeps.
fn theme_keys() -> Vec<String> {
    let mut keys: Vec<String> = settings_catalog()
        .general_section("theming")
        .map(|section| section.settings.iter().map(|row| row.key.clone()).collect())
        .unwrap_or_default();
    keys.retain(|key| !NOT_IN_A_THEME.contains(&key.as_str()));
    keys.extend(WITHOUT_ROWS.iter().map(|key| key.to_string()));
    keys
}

/// The look on screen as a saved theme's settings.
fn snapshot(values: &SettingsValues) -> Map<String, Value> {
    theme_keys()
        .into_iter()
        .map(|key| {
            let value = values.value(&key);
            (key, value)
        })
        .filter(|(_, value)| !value.is_null())
        .collect()
}

/// Whether two saved values are the same (numbers by value, so 20, 20.0 and "20" match).
fn same_value(a: &Value, b: &Value) -> bool {
    let number = |value: &Value| match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    };
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|other| same_value(value, other)))
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_value(a, b))
        }
        (Value::Number(_), _) | (_, Value::Number(_)) => match (number(a), number(b)) {
            (Some(a), Some(b)) => (a - b).abs() < 1e-9,
            _ => false,
        },
        _ => a == b,
    }
}

/// Whether the look on screen is saved theme `theme`.
fn is_current(theme: &SavedTheme, values: &SettingsValues) -> bool {
    theme
        .settings
        .iter()
        .all(|(key, saved)| same_value(saved, &values.value(key)))
}

/// `#rrggbb` (or `#rrggbbaa`) as a colour.
fn colour_of(hex: &str) -> gpui::Rgba {
    let digits = hex.trim().trim_start_matches('#');
    let channel = |start: usize| {
        digits
            .get(start..start + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .map_or(0.0, |value| f32::from(value) / 255.0)
    };
    gpui::Rgba {
        r: channel(0),
        g: channel(2),
        b: channel(4),
        a: 1.0,
    }
}

fn rgb_u32(colour: Rgb) -> u32 {
    let [r, g, b] = colour
        .0
        .map(|channel| channel.round().clamp(0.0, 255.0) as u32);
    (r << 16) | (g << 8) | b
}

/// What a theme's little window paints: its sidebar, work area, text and accent, and how much of
/// the backdrop each area hides (1 when the window is solid).
struct MiniColours {
    sidebar: gpui::Rgba,
    work: gpui::Rgba,
    ink: gpui::Rgba,
    accent: gpui::Rgba,
    sidebar_cover: f32,
    work_cover: f32,
}

fn mini_colours(values: &SettingsValues, dark: bool, backdrop_shows: bool) -> MiniColours {
    let (chrome, work, ink) = preview_colours(values, dark);
    let light = !dark;
    let roles = values.value(ghostex_settings_catalog::theme_roles::THEME_ROLES_SETTING);
    let accent = saved_role(&roles, light, "accent")
        .or_else(|| role_default(&roles, "accent", light))
        .map(|role| colour_of(&hex_of(role, light, rgb_u32(chrome))))
        .unwrap_or_else(|| {
            let preset_key = if dark {
                "darkThemePreset"
            } else {
                "lightThemePreset"
            };
            let preset = values.string(preset_key);
            let tint = if preset == "custom" {
                values.string(if dark {
                    "customSidebarTitlebarBackgroundTintColor"
                } else {
                    "customSidebarTitlebarLightBackgroundTintColor"
                })
            } else {
                preset_controls(dark, &preset)
                    .map(|(_, tint)| tint)
                    .unwrap_or_default()
            };
            accent_for_tint(&tint, light).gpui()
        });
    let cover = |key: &str| (values.f64(key) / 100.0).clamp(0.0, 1.0) as f32;
    // Over artwork the little window's panels are kept lighter than the window's own tints, so
    // the artwork reads at 80x48 and the tile is recognisable at a glance.
    let cover = |key: &str| cover(key) * 0.6;
    let (sidebar_cover, work_cover) = if backdrop_shows {
        if dark {
            (
                cover("windowGlassSidebarOpacityDark"),
                cover("windowGlassWorkAreaTintDark"),
            )
        } else {
            (
                cover("windowGlassSidebarOpacityLight"),
                cover("windowGlassWorkAreaTintLight"),
            )
        }
    } else {
        (1.0, 1.0)
    };
    MiniColours {
        sidebar: chrome.gpui(),
        work: work.gpui(),
        ink: ink.gpui(),
        accent,
        sidebar_cover,
        work_cover,
    }
}

/// A low-fidelity Ghostex window in a theme's colours: the sidebar with its rows, the work area
/// with a reply, your message in the accent and the chat box, over the theme's backdrop.
fn mini_window(colours: &MiniColours, backdrop: Option<AnyElement>) -> AnyElement {
    let ink = hsla(colours.ink);
    let bar = |width: f32, alpha: f32| {
        div()
            .h(px(2.5))
            .w(gpui::relative(width))
            .rounded(px(1.5))
            .bg(ink.opacity(alpha))
    };
    let accent = hsla(colours.accent);
    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .rounded(px(7.0))
        .bg(hsla(colours.work))
        .children(backdrop)
        .child(
            h_flex()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(
                    v_flex()
                        .w(px(24.0))
                        .h_full()
                        .flex_shrink_0()
                        .px(px(3.0))
                        .py(px(4.0))
                        .gap(px(3.0))
                        .bg(hsla(colours.sidebar).opacity(colours.sidebar_cover))
                        .border_r_1()
                        .border_color(ink.opacity(0.08))
                        .child(h_flex().gap(px(1.5)).children(
                            (0..3).map(|_| div().size(px(2.5)).rounded_full().bg(ink.opacity(0.3))),
                        ))
                        .child(
                            div()
                                .w_full()
                                .h(px(6.0))
                                .rounded(px(2.0))
                                .bg(ink.opacity(0.16)),
                        )
                        .child(bar(0.8, 0.22))
                        .child(bar(0.6, 0.16))
                        .child(bar(0.7, 0.16)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .h_full()
                        .min_w_0()
                        .p(px(4.0))
                        .gap(px(3.0))
                        .bg(hsla(colours.work).opacity(colours.work_cover))
                        .child(bar(0.7, 0.28))
                        .child(bar(0.5, 0.2))
                        .child(
                            h_flex().w_full().justify_end().child(
                                div()
                                    .w(gpui::relative(0.55))
                                    .h(px(7.0))
                                    .rounded(px(3.0))
                                    .bg(accent.opacity(0.4))
                                    .border_1()
                                    .border_color(accent.opacity(0.8)),
                            ),
                        )
                        .child(
                            div()
                                .mt_auto()
                                .w_full()
                                .h(px(8.0))
                                .rounded(px(3.0))
                                .border_1()
                                .border_color(ink.opacity(0.2))
                                .bg(ink.opacity(0.05)),
                        ),
                ),
        )
        .into_any_element()
}

/// What Your themes is doing: naming a theme (a new one, or renaming `Some(id)`), the theme whose
/// actions show, the look before the last switch (for Undo), and the strip's scroll.
#[derive(Default)]
pub(super) struct UserThemes {
    naming: Option<Option<String>>,
    name_input: Option<Entity<InputState>>,
    focus_name: bool,
    _name_subscription: Option<Subscription>,
    managing: Option<String>,
    before_switch: Option<(String, Map<String, Value>)>,
    scroll: ScrollHandle,
    wheel: StripWheelLock,
}

impl ThemeTab {
    fn save_saved_themes(&mut self, themes: &[SavedTheme], cx: &mut Context<Self>) {
        let list: Vec<Value> = themes.iter().map(SavedTheme::to_json).collect();
        self.save(SAVED_THEMES_KEY, Value::Array(list), cx);
    }

    /// Opens the name field: for a new theme (`target` `None`) or to rename one.
    fn start_naming(
        &mut self,
        target: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let values = self.store.read(cx).values();
        let themes = saved_themes(&values);
        let initial = match &target {
            Some(id) => themes
                .iter()
                .find(|theme| &theme.id == id)
                .map(|theme| theme.name.clone())
                .unwrap_or_default(),
            None => format!("My theme {}", themes.len() + 1),
        };
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(initial)
                .placeholder("Theme name")
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |page: &mut Self, _input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    page.finish_naming(cx);
                }
            },
        );
        self.user_themes.naming = Some(target);
        self.user_themes.name_input = Some(input);
        self.user_themes._name_subscription = Some(subscription);
        self.user_themes.focus_name = true;
        cx.notify();
    }

    /// The preview binary's states: the name field of Save current theme open, or the first saved
    /// theme's actions.
    pub(super) fn preview_user_themes(
        &mut self,
        naming: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if naming {
            self.start_naming(None, window, cx);
        } else {
            let values = self.store.read(cx).values();
            self.user_themes.managing = saved_themes(&values).first().map(|theme| theme.id.clone());
        }
    }

    fn cancel_naming(&mut self, cx: &mut Context<Self>) {
        self.user_themes.naming = None;
        self.user_themes.name_input = None;
        self.user_themes._name_subscription = None;
        cx.notify();
    }

    /// Save or Enter in the name field: saves the look on screen as a new theme, or the new name.
    fn finish_naming(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.user_themes.naming.clone() else {
            return;
        };
        let name = self
            .user_themes
            .name_input
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_string())
            .unwrap_or_default();
        if name.is_empty() {
            return;
        }
        let values = self.store.read(cx).values();
        let mut themes = saved_themes(&values);
        match target {
            Some(id) => {
                if let Some(theme) = themes.iter_mut().find(|theme| theme.id == id) {
                    theme.name = name;
                }
            }
            None => {
                let now = chrono::Utc::now();
                themes.push(SavedTheme {
                    id: format!("theme-{}", now.timestamp_millis()),
                    name,
                    saved_at: now.to_rfc3339(),
                    settings: snapshot(&values),
                });
            }
        }
        self.save_saved_themes(&themes, cx);
        self.cancel_naming(cx);
    }

    /// Switches to saved theme `id` in one save, keeping the look before it for Undo.
    fn switch_to_saved_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        let Some(theme) = saved_themes(&values)
            .into_iter()
            .find(|theme| theme.id == id)
        else {
            return;
        };
        let keys = theme_keys();
        let patch: Map<String, Value> = theme
            .settings
            .iter()
            .filter(|(key, _)| keys.contains(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if patch.is_empty() {
            return;
        }
        self.user_themes.before_switch = Some((theme.name.clone(), snapshot(&values)));
        self.save_patch(patch, cx);
    }

    fn undo_switch(&mut self, cx: &mut Context<Self>) {
        if let Some((_, before)) = self.user_themes.before_switch.take() {
            self.save_patch(before, cx);
        }
    }

    fn update_saved_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        let mut themes = saved_themes(&values);
        if let Some(theme) = themes.iter_mut().find(|theme| theme.id == id) {
            theme.settings = snapshot(&values);
            theme.saved_at = chrono::Utc::now().to_rfc3339();
        }
        self.save_saved_themes(&themes, cx);
    }

    fn delete_saved_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        let themes: Vec<SavedTheme> = saved_themes(&values)
            .into_iter()
            .filter(|theme| theme.id != id)
            .collect();
        self.user_themes.managing = None;
        self.save_saved_themes(&themes, cx);
    }

    /// The backdrop a saved theme's little window shows in `dark` (or light) mode: its artwork's
    /// thumbnail, Your image's picture or its Live poster.
    fn mini_backdrop(&mut self, values: &SettingsValues, dark: bool) -> Option<AnyElement> {
        match backdrop(values, dark) {
            Backdrop::Artwork(id) => self
                .artwork_thumbnail(id)
                .map(|picture| screenshot_preview(picture, false)),
            Backdrop::YourImage => {
                your_image_path(values, dark).map(|path| file_preview(path, false))
            }
            Backdrop::Live => {
                let style = values.string(if dark {
                    "windowGlassLiveStyleDark"
                } else {
                    "windowGlassLiveStyleLight"
                });
                gallery::poster(if style == "video" { "ink" } else { &style }, !dark)
                    .map(|poster| screenshot_preview(poster, false))
            }
            Backdrop::None => None,
        }
    }

    /// Your themes: the heading with Save current theme, the name field while naming, the strip
    /// of saved themes and the actions of the one whose … was pressed.
    pub(super) fn user_themes_rows(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let themes = saved_themes(values);
        if self.user_themes.focus_name
            && let Some(input) = &self.user_themes.name_input
        {
            self.user_themes.focus_name = false;
            let input = input.clone();
            input.update(cx, |input, cx| input.focus(window, cx));
        }
        let mut rows = Vec::new();
        let current = themes
            .iter()
            .find(|theme| is_current(theme, values))
            .map(|theme| theme.id.clone());
        let undo = self
            .user_themes
            .before_switch
            .as_ref()
            .map(|(name, _)| name.clone());
        let current_name = current
            .as_ref()
            .and_then(|id| themes.iter().find(|theme| &theme.id == id))
            .map(|theme| theme.name.clone());
        let description = match (&undo, themes.is_empty()) {
            (Some(name), _) => format!("Switched to {name}. Undo goes back to the look you had before."),
            // Save current theme is off while the look on screen is already saved; this says why.
            (None, false) if let Some(name) = &current_name => format!("The look on screen is saved as {name}. Click another to switch to it; its \u{2026} button renames, updates or deletes it."),
            (None, true) => "Save the look you have now, with its colours, artwork and transparency, to switch back to it later.".to_string(),
            (None, false) => "Your saved looks. Click one to switch to it; its \u{2026} button renames, updates or deletes it.".to_string(),
        };
        let mut actions: Vec<AnyElement> = Vec::new();
        if undo.is_some() {
            actions.push(settings_sized_button(
                p,
                "theme-user-undo",
                "Undo",
                Some("modals/settings/arrow-back-up.svg"),
                None,
                SizedButtonVariant::Secondary,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| page.undo_switch(cx),
                cx,
            ));
        }
        actions.push(settings_sized_button(
            p,
            "theme-user-save",
            "Save current theme",
            Some("modals/settings/plus.svg"),
            None,
            SizedButtonVariant::Secondary,
            SizedButtonSize::Sm,
            current.is_some() || self.user_themes.naming.is_some(),
            None,
            |page: &mut Self, window, cx| page.start_naming(None, window, cx),
            cx,
        ));
        rows.push(
            h_flex()
                .w_full()
                .gap(px(16.0))
                .px(px(16.0))
                .pt(px(12.0))
                .pb(px(
                    if themes.is_empty() && self.user_themes.naming.is_none() {
                        12.0
                    } else {
                        2.0
                    },
                ))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(19.0))
                                .text_color(hsla(p.foreground))
                                .child("Your themes"),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(18.57))
                                .text_color(hsla(p.muted))
                                .child(description),
                        ),
                )
                .child(h_flex().gap(px(6.0)).children(actions))
                .into_any_element(),
        );
        if self.user_themes.naming == Some(None) {
            rows.push(self.name_row(p, "Name this theme", window, cx));
        }
        if !themes.is_empty() {
            let mut tiles = Vec::new();
            for theme in &themes {
                let mut map = values.map().clone();
                map.extend(theme.settings.clone());
                let theme_values = SettingsValues::from_map(map);
                let dark =
                    preview_scheme_dark(&theme_values.string("sidebarTheme"), self.scheme_dark);
                let backdrop_element = self.mini_backdrop(&theme_values, dark);
                let colours = mini_colours(&theme_values, dark, backdrop_element.is_some());
                let preview = mini_window(&colours, backdrop_element);
                let id = theme.id.clone();
                let manage_id = theme.id.clone();
                let managing = self.user_themes.managing.as_deref() == Some(theme.id.as_str());
                let more = click_shield(div().rounded(px(7.0)).bg(hsla(p.popup_background)).child(
                    settings_icon_button(
                        p,
                        SharedString::from(format!("theme-user-more-{}", theme.id)),
                        "modals/settings/dots.svg",
                        12.0,
                        22.0,
                        ButtonVariant::Outline,
                        Some("Rename, update or delete".into()),
                        false,
                        move |page: &mut Self, _window, cx| {
                            page.user_themes.managing = if managing {
                                None
                            } else {
                                Some(manage_id.clone())
                            };
                            page.cancel_naming(cx);
                        },
                        cx,
                    ),
                ));
                let selected = current.as_deref() == Some(theme.id.as_str());
                tiles.push(theme_tile(
                    p,
                    TileSpec {
                        id: SharedString::from(format!("theme-user-{}", theme.id)),
                        name: theme.name.clone().into(),
                        label: theme.name.clone().into(),
                        dark,
                        caption: None,
                        selected,
                        cut: false,
                    },
                    vec![preview],
                    Some(more),
                    move |page, _window, cx| {
                        if !selected {
                            page.switch_to_saved_theme(&id, cx);
                        }
                    },
                    cx,
                ));
            }
            let scroll = self.user_themes.scroll.clone();
            let wheel = strip_wheel_layer(&scroll, &self.user_themes.wheel);
            rows.push(
                div()
                    .relative()
                    .w_full()
                    .min_w_0()
                    .child(
                        div()
                            .id("theme-user-row")
                            .w_full()
                            .flex()
                            .items_start()
                            .overflow_x_scroll()
                            .restrict_scroll_to_axis()
                            .track_scroll(&scroll)
                            .px(px(16.0))
                            .pt(px(6.0))
                            .pb(px(12.0))
                            .gap(px(8.0))
                            .children(tiles),
                    )
                    .child(wheel)
                    .into_any_element(),
            );
        }
        if let Some(id) = self.user_themes.managing.clone()
            && let Some(theme) = themes.iter().find(|theme| theme.id == id)
        {
            if self.user_themes.naming == Some(Some(id.clone())) {
                rows.push(self.name_row(p, "Rename", window, cx));
            } else {
                rows.push(self.manage_row(p, theme, cx));
            }
        }
        rows
    }

    /// The name field with Save and Cancel.
    fn name_row(
        &mut self,
        p: &SettingsPalette,
        label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(input) = self.user_themes.name_input.clone() else {
            return div().into_any_element();
        };
        h_flex()
            .w_full()
            .gap(px(8.0))
            .px(px(16.0))
            .pb(px(12.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(17.14))
                    .text_color(hsla(p.muted))
                    .child(label),
            )
            .child(settings_text_input(
                p,
                &input,
                Some(240.0),
                false,
                window,
                cx,
            ))
            .child(settings_sized_button(
                p,
                "theme-user-name-save",
                "Save",
                None,
                None,
                SizedButtonVariant::Primary,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| page.finish_naming(cx),
                cx,
            ))
            .child(settings_sized_button(
                p,
                "theme-user-name-cancel",
                "Cancel",
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| page.cancel_naming(cx),
                cx,
            ))
            .into_any_element()
    }

    /// The actions of the theme whose … was pressed.
    fn manage_row(
        &mut self,
        p: &SettingsPalette,
        theme: &SavedTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rename_id = theme.id.clone();
        let update_id = theme.id.clone();
        let delete_id = theme.id.clone();
        let button = |id: &'static str,
                      label: &'static str,
                      icon: &'static str,
                      variant: SizedButtonVariant,
                      press: Box<dyn Fn(&mut Self, &mut Window, &mut Context<Self>)>,
                      cx: &mut Context<Self>| {
            settings_sized_button(
                p,
                id,
                label,
                Some(icon),
                None,
                variant,
                SizedButtonSize::Sm,
                false,
                None,
                move |page: &mut Self, window, cx| press(page, window, cx),
                cx,
            )
        };
        let rename = button(
            "theme-user-rename",
            "Rename",
            "modals/settings/pencil.svg",
            SizedButtonVariant::Secondary,
            Box::new(move |page, window, cx| {
                page.start_naming(Some(rename_id.clone()), window, cx)
            }),
            cx,
        );
        let update = button(
            "theme-user-update",
            "Update to current look",
            "modals/settings/refresh.svg",
            SizedButtonVariant::Secondary,
            Box::new(move |page, _window, cx| page.update_saved_theme(&update_id, cx)),
            cx,
        );
        let delete = button(
            "theme-user-delete",
            "Delete",
            "modals/settings/trash.svg",
            SizedButtonVariant::Destructive,
            Box::new(move |page, _window, cx| page.delete_saved_theme(&delete_id, cx)),
            cx,
        );
        let done = button(
            "theme-user-done",
            "Done",
            "modals/settings/check.svg",
            SizedButtonVariant::Outline,
            Box::new(|page, _window, cx| {
                page.user_themes.managing = None;
                cx.notify();
            }),
            cx,
        );
        h_flex()
            .w_full()
            .flex_wrap()
            .gap(px(8.0))
            .px(px(16.0))
            .pb(px(12.0))
            .child(
                div()
                    .min_w_0()
                    .max_w(px(220.0))
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.0))
                    .line_height(px(28.0))
                    .text_color(hsla(p.foreground))
                    .child(theme.name.clone()),
            )
            .child(rename)
            .child(update)
            .child(delete)
            .child(done)
            .into_any_element()
    }
}
