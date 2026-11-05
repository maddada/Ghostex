//! The Theme page (packages/core-ui/settings-modal/tabs/theme.tsx (deleted 2026-10-01) and theme-simple-controls.tsx (deleted 2026-10-01)):
//! Colours, Transparency, Chat and terminal, and the links to related rows on General. Its rows
//! are the General catalog's `theming` section, so the same search finds them.
//!
//! CDXC:Theming 2026-10-16 DECISION:
//! User: "doesn't look so good … can we please make this look nicer? … don't copy everything", then picked direction A (Calm rows) of the mockups in docs/2026-10-16/theme-page. Supersedes the 2026-09-25 layout. The Theme page's rows carry an icon tile (this page only; other pages keep their rows). Appearance is three preview cards (System, Light, Dark). Each group is one card whose rare rows fold under a More row: Colours (Dark mode colour and Light mode colour as two rows of squares, Colourfulness; More: the sidebar and work area set apart, the active pane outline), Transparency (one Off / Dark only / Always control replacing Enable transparency and Use transparency, Strength, Blur, Behind the glass with its pictures or Live gallery; More: picture position, Live speed and brightness, play only when plugged in, the four tints, menu blur) and Chat and terminal (the two themes; More: the terminal palettes), then Related on General. The three-way choices keep the existing row control ("same as what we have now dont change this") and Blur stays as it was ("dont change this"). Custom colour is a "+" square after the squares that opens the Pick Color dialog right at it; no rows open under the colour row (2026-10-17, User: "very bad to have the same colors twice here why??? pls fix no need for these 2 rows that appear when i click on the + option in the colors pls"). The custom colour's depth has no control of its own: Colourfulness sets it (2026-10-17, the user's pick: "Remove it: Colourfulness alone sets how deep the custom colour is"). The squares are "smaller and simpler": 20px, one flat colour, today's ring drawn thinner. A search hit inside a group's More opens it.
mod art;
mod chat_preview;
mod colours;
mod controls;
mod font;
mod gallery;
mod interface_scale;
mod swatches;
pub(super) mod transparency;

use super::super::super::native_modal_kit::*;
use super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::fields::{
    ButtonVariant, DropdownState, FieldStates, RowSpec, SettingsPage, SliderSaver,
    open_tint_picker, reset_key, searchable_select_field, select_field, setting_row,
    settings_button, settings_section, toggle_field, toggle_field_with, trigger_tint_picker,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, SettingsStoreEvent, SettingsValues};
use colours::{
    COLOURFULNESS_LAST_POSITION, colourfulness_display_step, colourfulness_patch,
    colourfulness_points, colourfulness_step_index, js_number, paired_preset, preset_label,
    preset_options, preview_colours, swatch_paint,
};
use controls::{
    ICON_APPEARANCE, ICON_CHAT, ICON_COLOURFULNESS, ICON_DARK, ICON_FONT, ICON_LIGHT, ICON_SIDEBAR,
    ICON_TERMINAL, SWATCH_GAP, appearance_card, colourfulness_preview, colourfulness_slider,
    custom_swatch, more_options_button, swatch,
};
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, RenderImage, SharedString, Styled as _, Window, WindowAppearance, div, px,
};
use gpui_component::h_flex;
use serde_json::{Map, Value, json};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use swatches::swatch_colour;

/// `GHOSTTY_THEME_UNMANAGED_VALUE`.
const GHOSTTY_THEME_UNMANAGED_VALUE: &str = "__ghostex_ghostty_theme_unmanaged__";

/// A group with its own More options (`ThemeMoreOptionsGroup`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MoreGroup {
    Colours,
    Transparency,
    ChatTerminal,
}

impl MoreGroup {
    fn index(self) -> usize {
        match self {
            MoreGroup::Colours => 0,
            MoreGroup::Transparency => 1,
            MoreGroup::ChatTerminal => 2,
        }
    }

    /// `THEME_MORE_OPTIONS_KEYS`: the rows inside the group's More options, so a search hit on one
    /// of them opens it.
    fn keys(self) -> &'static [&'static str] {
        match self {
            MoreGroup::Colours => &[
                "themeSidebarContrast",
                "themeWorkAreaContrast",
                "showActivePaneOutline",
                "workspaceActivePaneBorderColor",
            ],
            MoreGroup::Transparency => &[
                "windowGlassVideoOnlyOnPower",
                "windowGlassLiveSpeed",
                "windowGlassLiveBrightness",
                "windowGlassImagePlacement",
                "windowGlassSidebarOpacityDark",
                "windowGlassWorkAreaTintDark",
                "windowGlassSidebarOpacityLight",
                "windowGlassWorkAreaTintLight",
                "windowGlassMenuBlurRadius",
            ],
            MoreGroup::ChatTerminal => &["terminalGhosttyTheme", "terminalGhosttyLightTheme"],
        }
    }
}

thread_local! {
    /// `rememberedThemeMoreOptionsOpen`: each group's More options stays as it was left for the rest
    /// of this app run.
    static REMEMBERED_MORE_OPEN: Cell<[bool; 3]> = const { Cell::new([false; 3]) };
}

pub(crate) fn theme_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| ThemeTab::new(store.clone(), window, cx)).into()
}

pub(crate) struct ThemeTab {
    pub(super) store: Entity<SettingsStore>,
    fields: FieldStates,
    /// Which appearance the Colourfulness preview shows (the window's own, or light for the
    /// preview binary's `theme-light-scheme`).
    pub(super) scheme_dark: bool,
    /// `pickedOnPurpose`: a colour was picked in that appearance, so the other stops following it.
    picked_dark: bool,
    picked_light: bool,
    /// `splitAreas`: More colour options sets the sidebar and work area apart.
    split_areas: bool,
    more_open: [bool; 3],
    /// The Live gallery edits the light mode's style (`editing`).
    pub(super) live_editing_light: bool,
    /// Why the last picked glass video was refused, and for which appearance.
    pub(super) video_error: Option<(bool, String)>,
    pub(super) images: HashMap<String, Arc<RenderImage>>,
    pub(super) posters: HashMap<String, Arc<gpui::Image>>,
    palette_dark_dropdown: DropdownState,
    palette_light_dropdown: DropdownState,
    font_dropdown: DropdownState,
    pending_font: Option<String>,
    monospace_font_dropdown: DropdownState,
    pending_monospace_font: Option<String>,
}

fn palette_dark_dropdown(page: &mut ThemeTab) -> &mut DropdownState {
    &mut page.palette_dark_dropdown
}

fn palette_light_dropdown(page: &mut ThemeTab) -> &mut DropdownState {
    &mut page.palette_light_dropdown
}

impl ThemeTab {
    fn new(store: Entity<SettingsStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        chat_preview::register_preview_fonts(cx);
        cx.subscribe(
            &store,
            |page: &mut Self, store, event: &SettingsStoreEvent, cx| {
                let SettingsStoreEvent::HostPayload(kind) = event;
                if kind == "windowGlassVideoFilePicked" {
                    let payload = store
                        .read(cx)
                        .host_payload(kind)
                        .cloned()
                        .unwrap_or_default();
                    let light = payload.get("appearance").and_then(Value::as_str) == Some("light");
                    page.video_error = payload
                        .get("error")
                        .and_then(Value::as_str)
                        .filter(|error| !error.is_empty())
                        .map(|error| (light, error.to_string()));
                    cx.notify();
                }
            },
        )
        .detach();
        let values = store.read(cx).values();
        // `initialThemeScheme`: Light when the window is light, otherwise Dark.
        let theme = values.string("sidebarTheme");
        let scheme_dark = if theme == "system" {
            !matches!(
                window.appearance(),
                WindowAppearance::Light | WindowAppearance::VibrantLight
            )
        } else {
            theme.starts_with("dark")
        };
        let sidebar = values.f64("themeSidebarContrast");
        let split_areas = sidebar != values.f64("themeWorkAreaContrast")
            || colours::colourfulness_step_for_points(sidebar).is_none();
        let mut page = Self {
            store,
            fields: FieldStates::default(),
            scheme_dark,
            picked_dark: false,
            picked_light: false,
            split_areas,
            more_open: REMEMBERED_MORE_OPEN.with(Cell::get),
            live_editing_light: false,
            video_error: None,
            images: HashMap::new(),
            posters: HashMap::new(),
            // : the palette lists stop at 320px.
            palette_dark_dropdown: DropdownState::with_max_height(320.0),
            palette_light_dropdown: DropdownState::with_max_height(320.0),
            font_dropdown: DropdownState::default(),
            pending_font: None,
            monospace_font_dropdown: DropdownState::default(),
            pending_monospace_font: None,
        };
        page.apply_preview_state(window, cx);
        page
    }

    /// The preview binary's `theme-*` states open the groups they show.
    fn apply_preview_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self
            .store
            .read(cx)
            .request()
            .preview_state
            .clone()
            .unwrap_or_default();
        if state == "theme-more" || state.starts_with("theme-glass") || state == "theme-custom" {
            self.more_open = [true; 3];
        }
        if state == "theme-light-scheme" {
            self.scheme_dark = false;
        }
        // The preview binary's `theme-custom-picker` opens the dark "+" square's Pick Color dialog.
        if state == "theme-custom-picker" {
            let tint = self.store.read(cx).values().string(custom_tint_key(true));
            open_tint_picker(self, custom_tint_key(true), &tint, window, cx);
        }
        if state.starts_with("theme-glass") {
            let store = self.store.clone();
            store.update(cx, |store, cx| {
                store.scroll_to_section(SettingsTabId::Theme, "transparency", cx)
            });
        }
    }

    fn more_shown(
        &self,
        group: MoreGroup,
        searching: bool,
        visible: &dyn Fn(&str) -> bool,
    ) -> bool {
        self.more_open[group.index()] || more_has_hit(group, searching, visible)
    }

    fn set_more_open(&mut self, group: MoreGroup, open: bool, cx: &mut Context<Self>) {
        self.more_open[group.index()] = open;
        let remembered = self.more_open;
        REMEMBERED_MORE_OPEN.with(|cell| cell.set(remembered));
        cx.notify();
    }

    pub(super) fn save(&mut self, key: &str, value: Value, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| store.update_setting(key, value, cx));
    }

    pub(super) fn save_patch(&mut self, patch: Map<String, Value>, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.apply_patch(patch, "settings:control", cx)
        });
    }

    /// `selectDarkPreset` / `selectLightPreset`: a colour for one appearance, and the same colour
    /// for the other while it still follows (it matched and was not picked there on purpose).
    fn select_preset(&mut self, dark: bool, preset: String, cx: &mut Context<Self>) {
        let values = self.store.read(cx).values();
        let (own_key, other_key) = if dark {
            ("darkThemePreset", "lightThemePreset")
        } else {
            ("lightThemePreset", "darkThemePreset")
        };
        let own = values.string(own_key);
        let other = values.string(other_key);
        let paired =
            own != "custom" && paired_preset(dark, &own).as_deref() == Some(other.as_str());
        let other_picked = if dark {
            self.picked_light
        } else {
            self.picked_dark
        };
        let mut patch = Map::new();
        patch.insert(own_key.to_string(), json!(preset));
        if !other_picked
            && paired
            && let Some(follow) = paired_preset(dark, &preset)
        {
            patch.insert(other_key.to_string(), json!(follow));
        }
        if dark {
            self.picked_dark = true;
        } else {
            self.picked_light = true;
        }
        self.save_patch(patch, cx);
    }
}

/// The tint key of one mode's custom colour.
fn custom_tint_key(dark: bool) -> &'static str {
    if dark {
        "customSidebarTitlebarBackgroundTintColor"
    } else {
        "customSidebarTitlebarLightBackgroundTintColor"
    }
}

/// `moreHasHit`: while searching, a hit on a row inside the group's More options opens it.
fn more_has_hit(group: MoreGroup, searching: bool, visible: &dyn Fn(&str) -> bool) -> bool {
    searching && group.keys().iter().any(|key| visible(key))
}

impl SettingsPage for ThemeTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

/// What every Theme group reads while it renders.
pub(super) struct ThemeCx {
    pub(super) p: SettingsPalette,
    pub(super) values: SettingsValues,
    pub(super) searching: bool,
    visible: Rc<dyn Fn(&str) -> bool>,
    /// `hud.windowGlassBlockedBySystem`: the system's own transparency switch keeps the window opaque.
    pub(super) glass_blocked: bool,
    /// `nativeFilePickerAvailable`: the host can open file dialogs (`hud.appIconPickerUnavailable` is off).
    pub(super) native_picker: bool,
}

impl ThemeCx {
    pub(super) fn visible(&self, key: &str) -> bool {
        (self.visible)(key)
    }

    /// `getSettingModificationProps(key)` with the asterisk after the label.
    pub(super) fn spec_plain(&self, key: &str, label: &str, description: &str) -> RowSpec {
        RowSpec::new(label.to_string())
            .description(description.to_string())
            .keyed(&self.values, key)
            .marker_after_label()
    }

    /// `getSettingModificationProps(key)`.
    pub(super) fn spec(&self, key: &str, label: &str, description: &str) -> RowSpec {
        RowSpec::new(label.to_string())
            .description(description.to_string())
            .keyed(&self.values, key)
            .marker_after_label()
    }
}

/// `APPEARANCE_CHOICES`: System, Light, Dark, labelled as `SIDEBAR_THEME_SETTING_OPTIONS` labels them.
fn appearance_choices() -> Vec<SettingOption> {
    let labels = settings_catalog().options(module::SETTINGS, "SIDEBAR_THEME_SETTING_OPTIONS");
    ["system", "plain-light", "dark-2"]
        .iter()
        .map(|value| SettingOption {
            label: labels
                .iter()
                .find(|option| option.value == *value)
                .map(|option| option.label.clone())
                .unwrap_or_else(|| value.to_string()),
            value: value.to_string(),
        })
        .collect()
}

impl ThemeTab {
    fn colours_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = t.p;
        let values = &t.values;
        let visible_fn = t.visible.clone();
        let visible = |key: &str| visible_fn(key);
        let dark_colour_visible = [
            "darkThemePreset",
            "customSidebarTitlebarBackgroundTintColor",
        ]
        .iter()
        .any(|key| visible(key));
        let light_colour_visible = [
            "lightThemePreset",
            "customSidebarTitlebarLightBackgroundTintColor",
        ]
        .iter()
        .any(|key| visible(key));
        let colours_visible = visible("sidebarTheme")
            || dark_colour_visible
            || light_colour_visible
            || visible("themeSidebarContrast")
            || visible("workAreaFrame")
            || more_has_hit(MoreGroup::Colours, t.searching, &visible);
        if !colours_visible {
            return None;
        }
        let mut rows: Vec<AnyElement> = Vec::new();
        if visible("sidebarTheme") {
            rows.push(self.appearance_row(t, window, cx));
        }
        if dark_colour_visible {
            rows.push(self.theme_colour_row(t, true, window, cx));
        }
        if light_colour_visible {
            rows.push(self.theme_colour_row(t, false, window, cx));
        }
        if visible("themeSidebarContrast") || visible("themeWorkAreaContrast") {
            let step = colourfulness_step_index(values);
            let control = h_flex()
                .items_center()
                .gap(px(12.0))
                .child(match step {
                    Some(step) => {
                        let saver: SliderSaver = Rc::new(
                            |store: &Entity<SettingsStore>, value, _commit, cx: &mut App| {
                                let step =
                                    value.round().clamp(0.0, COLOURFULNESS_LAST_POSITION as f64)
                                        as usize;
                                store.update(cx, |store, cx| {
                                    if colourfulness_step_index(&store.values()) != Some(step) {
                                        store.apply_patch(
                                            colourfulness_patch(step),
                                            "settings:control",
                                            cx,
                                        );
                                    }
                                });
                            },
                        );
                        colourfulness_slider(
                            self,
                            &p,
                            "themeColourfulness",
                            step,
                            saver,
                            window,
                            cx,
                        )
                    }
                    None => div()
                        .text_size(px(13.0))
                        .line_height(px(18.57))
                        .text_color(hsla(p.muted))
                        .whitespace_nowrap()
                        .child("Set per area")
                        .into_any_element(),
                })
                .child(colourfulness_preview(
                    &p,
                    preview_colours(values, self.scheme_dark),
                ))
                .into_any_element();
            rows.push(setting_row(
                &p,
                "themeColourfulness",
                t.spec_plain(
                    "themeSidebarContrast",
                    "Colourfulness",
                    "How much of the colour shows in the sidebar and work area. Subtle is deeper and nearly neutral, Vivid lighter and more colourful, and in dark mode Full is the colour you picked itself.",
                )
                .icon(ICON_COLOURFULNESS),
                Some(reset_key::<Self>("themeSidebarContrast")),
                control,
                cx,
            ));
        }
        if visible("workAreaFrame") {
            rows.push(toggle_field(
                self,
                &p,
                "workAreaFrame",
                t.spec_plain(
                    "workAreaFrame",
                    "Frame the work area",
                    "Show the work area as a card inside the window, with a thin margin all around, a hairline border and a light shadow. Off keeps the work area edge to edge.",
                ),
                values.bool("workAreaFrame"),
                cx,
            ));
        }
        let shown = self.more_shown(MoreGroup::Colours, t.searching, &visible);
        if !t.searching {
            let open = self.more_open[MoreGroup::Colours.index()];
            rows.push(more_options_button(
                &p,
                "theme-more-colours",
                "More colour options",
                "Sidebar and work area apart, active pane outline",
                shown,
                move |page, _window, cx| page.set_more_open(MoreGroup::Colours, !open, cx),
                cx,
            ));
        }
        if shown {
            rows.extend(self.more_colour_rows(t, window, cx));
        }
        settings_section(
            &p,
            "Colours",
            Some("The colour of the sidebar and window in each mode.".into()),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }

    /// Appearance: System, Light and Dark as three preview cards, each a small window in that
    /// appearance's colours.
    fn appearance_row(
        &mut self,
        t: &ThemeCx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = t.p;
        let values = &t.values;
        let current = values.string("sidebarTheme");
        let dark = preview_colours(values, true);
        let light = preview_colours(values, false);
        let cards: Vec<AnyElement> = appearance_choices()
            .into_iter()
            .map(|option| {
                let windows = match option.value.as_str() {
                    "system" => vec![light, dark],
                    "plain-light" => vec![light],
                    _ => vec![dark],
                };
                let selected = option.value == current;
                let value = option.value.clone();
                appearance_card(
                    &p,
                    SharedString::from(format!("theme-appearance-{}", option.value)),
                    &option.label,
                    windows,
                    selected,
                    move |page, cx| page.save("sidebarTheme", json!(value.clone()), cx),
                    cx,
                )
            })
            .collect();
        let control = h_flex()
            .w_full()
            .gap(px(16.0))
            .children(cards)
            .into_any_element();
        setting_row(
            &p,
            "sidebarTheme",
            t.spec_plain(
                "sidebarTheme",
                "Appearance",
                "System follows your computer’s light or dark mode.",
            )
            .icon(ICON_APPEARANCE)
            .wide(),
            Some(reset_key::<Self>("sidebarTheme")),
            control,
            cx,
        )
    }

    /// Dark mode colour or Light mode colour: the sixteen squares and the "+" Custom square. The
    /// readout names the colour and says when it still follows the other mode.
    fn theme_colour_row(
        &mut self,
        t: &ThemeCx,
        dark: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = t.p;
        let values = &t.values;
        let key = if dark {
            "darkThemePreset"
        } else {
            "lightThemePreset"
        };
        let mode = if dark { "dark" } else { "light" };
        let selected = values.string(key);
        let step = colourfulness_display_step(values.f64("themeSidebarContrast"));
        let mut squares: Vec<AnyElement> = preset_options(dark)
            .into_iter()
            .filter(|(value, _)| value != "custom")
            .map(|(value, label)| {
                let colour = swatch_colour(&swatch_paint(dark, &value, step));
                let is_selected = value == selected;
                let preset = value.clone();
                swatch(
                    &p,
                    SharedString::from(format!("theme-colour-{mode}-{value}")),
                    label,
                    colour,
                    is_selected,
                    move |page, cx| page.select_preset(dark, preset.clone(), cx),
                    cx,
                )
            })
            .collect();
        let tint_key = custom_tint_key(dark);
        let tint = values.string(tint_key);
        let picker = trigger_tint_picker(self, &p, tint_key, &tint, window, cx);
        squares.push(custom_swatch(
            &p,
            SharedString::from(format!("theme-colour-{mode}-custom")),
            selected == "custom",
            picker,
            move |page, window, cx| page.pick_custom_colour(dark, window, cx),
            cx,
        ));
        let other_key = if dark {
            "lightThemePreset"
        } else {
            "darkThemePreset"
        };
        let other = values.string(other_key);
        let paired = selected != "custom"
            && paired_preset(dark, &selected).as_deref() == Some(other.as_str());
        let picked_here = if dark {
            self.picked_dark
        } else {
            self.picked_light
        };
        let other_picked = if dark {
            self.picked_light
        } else {
            self.picked_dark
        };
        let name = if selected == "custom" {
            "Custom".to_string()
        } else {
            preset_label(dark, &selected)
        };
        // Light mode follows the dark pick until a light colour is picked on purpose; dark mode
        // follows only when light was picked first.
        let follows = paired && !picked_here && (!dark || other_picked);
        let readout = if follows {
            format!(
                "{name} · follows {} mode",
                if dark { "light" } else { "dark" }
            )
        } else {
            name
        };
        let control = h_flex()
            .gap(px(SWATCH_GAP))
            .items_center()
            .children(squares)
            .into_any_element();
        setting_row(
            &p,
            if dark {
                "themeColourDark"
            } else {
                "themeColourLight"
            },
            t.spec_plain(
                key,
                if dark {
                    "Dark mode colour"
                } else {
                    "Light mode colour"
                },
                if dark {
                    "The colour of the sidebar and window in dark mode. Picking one gives light mode the same colour until you pick one there. The + square picks any tint."
                } else {
                    "The colour of the sidebar and window in light mode. It follows the dark mode colour until you pick one here. The + square picks any tint."
                },
            )
            .readout(readout)
            .icon(if dark { ICON_DARK } else { ICON_LIGHT }),
            Some(reset_key::<Self>(key)),
            control,
            cx,
        )
    }

    /// The rows under More colour options.
    fn more_colour_rows(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let p = t.p;
        let values = t.values.clone();
        let mut rows: Vec<AnyElement> = Vec::new();
        if t.visible("themeSidebarContrast") || t.visible("themeWorkAreaContrast") {
            rows.push(toggle_field_with(
                &p,
                "themeSplitAreas",
                t.spec_plain(
                    "themeWorkAreaContrast",
                    "Set the sidebar and work area separately",
                    "Off: one Colourfulness for both.",
                ),
                self.split_areas,
                Some(reset_key::<Self>("themeWorkAreaContrast")),
                |page: &mut Self, checked, _window, cx| {
                    page.split_areas = checked;
                    if !checked {
                        let sidebar = page.store.read(cx).values().f64("themeSidebarContrast");
                        page.save_patch(
                            colourfulness_patch(colourfulness_display_step(sidebar)),
                            cx,
                        );
                    }
                    cx.notify();
                },
                cx,
            ));
        }
        for (key, label, visible) in [
            (
                "themeSidebarContrast",
                "Sidebar colourfulness",
                self.split_areas && t.visible("themeSidebarContrast"),
            ),
            (
                "themeWorkAreaContrast",
                "Work area colourfulness",
                self.split_areas && t.visible("themeWorkAreaContrast"),
            ),
        ] {
            if !visible {
                continue;
            }
            let step = colourfulness_display_step(values.f64(key));
            let saver: SliderSaver = Rc::new(
                move |store: &Entity<SettingsStore>, value, _commit, cx: &mut App| {
                    let points = colourfulness_points(
                        value.round().clamp(0.0, COLOURFULNESS_LAST_POSITION as f64) as usize,
                    );
                    store.update(cx, |store, cx| {
                        if store.values().f64(key) != points {
                            store.update_setting(key, js_number(points), cx);
                        }
                    });
                },
            );
            let slider = colourfulness_slider(self, &p, key, step, saver, window, cx);
            rows.push(setting_row(
                &p,
                format!("{key}-step"),
                RowSpec::new(label)
                    .keyed(&values, key)
                    .dependent()
                    .marker_after_label(),
                Some(reset_key::<Self>(key)),
                slider,
                cx,
            ));
        }
        if t.visible("showActivePaneOutline") {
            rows.push(toggle_field(
                self,
                &p,
                "showActivePaneOutline",
                t.spec_plain(
                    "showActivePaneOutline",
                    "Show active pane outline",
                    "Show an outline around the currently focused pane.",
                ),
                values.bool("showActivePaneOutline"),
                cx,
            ));
        }
        if values.bool("showActivePaneOutline") && t.visible("workspaceActivePaneBorderColor") {
            rows.push(super::super::fields::web_color_picker_field(
                self,
                &p,
                "workspaceActivePaneBorderColor",
                t.spec_plain(
                    "workspaceActivePaneBorderColor",
                    "Active pane outline colour",
                    "Color of the outline around the currently focused pane.",
                )
                .dependent(),
                Some(reset_key::<Self>("workspaceActivePaneBorderColor")),
                &values.string("workspaceActivePaneBorderColor"),
                window,
                cx,
            ));
        }
        rows
    }

    /// The "+" square: the custom colour for that mode, and its Pick Color dialog opened at the
    /// square on the saved tint.
    ///
    /// CDXC:Theming 2026-10-23 DECISION:
    /// After a report that a custom light mode colour was not used, the user asked whether a custom colour should be drawn as strongly as the presets: "Yes, same strength as presets". A custom light colour is drawn at the tinted light presets' depth (one point deeper than the depth Colourfulness saves, `CUSTOM_LIGHT_THEME_DEPTH_SHIFT` in helpers/titlebar/chrome_tints.rs), so it shows as strongly as a preset of the same hue. Supersedes the 2026-06-15 rule that drew it at the saved depth; the custom colour is still a calibrated tint chosen in the in-app picker, not a freeform background.
    fn pick_custom_colour(&mut self, dark: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.select_preset(dark, "custom".to_string(), cx);
        let tint_key = custom_tint_key(dark);
        let tint = self.store.read(cx).values().string(tint_key);
        open_tint_picker(self, tint_key, &tint, window, cx);
    }

    fn chat_terminal_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = t.p;
        let values = t.values.clone();
        let visible_fn = t.visible.clone();
        let visible = |key: &str| visible_fn(key);
        if !(visible("sessionChatTheme")
            || visible("terminalColorScheme")
            || more_has_hit(MoreGroup::ChatTerminal, t.searching, &visible))
        {
            return None;
        }
        let options = settings_catalog().options(module::SETTINGS, "SESSION_CHAT_THEME_OPTIONS");
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let mut rows: Vec<AnyElement> = Vec::new();
        for (key, label, description, icon) in [
            (
                "sessionChatTheme",
                "Chat theme",
                "Follow the app theme, or choose a separate appearance for chat.",
                ICON_CHAT,
            ),
            (
                "terminalColorScheme",
                "Terminal theme",
                "Follow the app theme, or choose a separate appearance for terminals.",
                ICON_TERMINAL,
            ),
        ] {
            if !visible(key) {
                continue;
            }
            rows.push(select_field(
                self,
                &p,
                key,
                t.spec(key, label, description).icon(icon),
                Some(reset_key::<Self>(key)),
                &options,
                &values.choice(key, &allowed),
                None,
                move |page: &mut Self, next, _window, cx| page.save(key, json!(next), cx),
                window,
                cx,
            ));
        }
        let shown = self.more_shown(MoreGroup::ChatTerminal, t.searching, &visible);
        if !t.searching {
            let open = self.more_open[MoreGroup::ChatTerminal.index()];
            rows.push(more_options_button(
                &p,
                "theme-more-chat-terminal",
                "Terminal palettes",
                "The terminal colours in dark mode and in light mode",
                shown,
                move |page, _window, cx| page.set_more_open(MoreGroup::ChatTerminal, !open, cx),
                cx,
            ));
        }
        if shown {
            let base =
                settings_catalog().options(module::SETTINGS, "GHOSTTY_THEME_SETTING_OPTIONS");
            // `getGhosttyThemeSettingOptions`: a configured theme outside the bundled list stays listed.
            let with_current = |current: &str| {
                let mut options = base.clone();
                if !current.is_empty() && !options.iter().any(|option| option.value == current) {
                    options.insert(
                        0,
                        SettingOption {
                            label: current.to_string(),
                            value: current.to_string(),
                        },
                    );
                }
                options
            };
            if visible("terminalGhosttyTheme") {
                let current = values.string("terminalGhosttyTheme");
                let options = with_current(&current);
                let shown_value = if current.is_empty() {
                    GHOSTTY_THEME_UNMANAGED_VALUE.to_string()
                } else {
                    current
                };
                rows.push(searchable_select_field(
                    self,
                    &p,
                    "terminalGhosttyTheme",
                    t.spec(
                        "terminalGhosttyTheme",
                        "Terminal palette in dark mode",
                        "Uses the dark theme set in Ghostex's terminal config, or GitHub Dark when none is set.",
                    ),
                    Some(reset_key::<Self>("terminalGhosttyTheme")),
                    &options,
                    &shown_value,
                    palette_dark_dropdown,
                    |page: &mut Self, next, _window, cx| {
                        let value = if next == GHOSTTY_THEME_UNMANAGED_VALUE {
                            String::new()
                        } else {
                            next
                        };
                        page.save("terminalGhosttyTheme", json!(value), cx);
                    },
                    window,
                    cx,
                ));
            }
            if visible("terminalGhosttyLightTheme") {
                let current = values.string("terminalGhosttyLightTheme");
                let options: Vec<SettingOption> = with_current(&current)
                    .into_iter()
                    .filter(|option| option.value != GHOSTTY_THEME_UNMANAGED_VALUE)
                    .collect();
                rows.push(searchable_select_field(
                    self,
                    &p,
                    "terminalGhosttyLightTheme",
                    t.spec(
                        "terminalGhosttyLightTheme",
                        "Terminal palette in light mode",
                        "Uses the light theme set in Ghostex's terminal config, or GitHub Light when none is set.",
                    ),
                    Some(reset_key::<Self>("terminalGhosttyLightTheme")),
                    &options,
                    &current,
                    palette_light_dropdown,
                    |page: &mut Self, next, _window, cx| {
                        page.save("terminalGhosttyLightTheme", json!(next), cx)
                    },
                    window,
                    cx,
                ));
            }
        }
        settings_section(
            &p,
            "Chat and terminal",
            Some("Follow the app, or give them their own appearance.".into()),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }

    /// Related settings on the Sidebar & Sessions and Terminal pages: each Open searches
    /// that page for the row.
    fn related_section(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let rows = [
            (
                "Sidebar size",
                "Sidebar Interface Size",
                ICON_SIDEBAR,
                SettingsTabId::Sidebar,
            ),
            (
                "Terminal background colour and image",
                "Terminal background",
                ICON_TERMINAL,
                SettingsTabId::Terminal,
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (label, query, icon, tab))| {
            let button = settings_button(
                p,
                SharedString::from(format!("theme-related-{index}")),
                "Open",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    let store = page.store.clone();
                    store.update(cx, |store, cx| {
                        store.set_search_query(query.to_string(), cx);
                        store.set_active_tab(tab, cx);
                    });
                },
                cx,
            );
            setting_row(
                p,
                format!("theme-related-row-{index}"),
                RowSpec::new(label).icon(icon),
                None,
                button,
                cx,
            )
        })
        .collect();
        settings_section(p, "Related on General", None, None, rows)
            .map(IntoElement::into_any_element)
    }
}

impl Render for ThemeTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (t, matching) = {
            let store = self.store.read(cx);
            let hud = store.hud().cloned().unwrap_or_default();
            let native_picker =
                hud.get("appIconPickerUnavailable").and_then(Value::as_bool) != Some(true);
            // `rowVisible(settingsSearch.theming, key)`, looked up once per render.
            let theming_keys: HashMap<String, bool> = settings_catalog()
                .general_section("theming")
                .map(|section| {
                    section
                        .settings
                        .iter()
                        .map(|row| {
                            (
                                row.key.clone(),
                                store.theme_row_visible("theming", &row.key),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let searching = store.is_searching();
            let visible: Rc<dyn Fn(&str) -> bool> = Rc::new(move |key: &str| {
                !searching || theming_keys.get(key).copied().unwrap_or(false)
            });
            let matching: Vec<SettingsTabId> = if searching {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                ThemeCx {
                    p: store.palette(),
                    values: store.values(),
                    searching,
                    visible,
                    glass_blocked: hud
                        .get("windowGlassBlockedBySystem")
                        .and_then(Value::as_bool)
                        == Some(true),
                    native_picker,
                },
                matching,
            )
        };
        let p = t.p;
        let mut blocks: Vec<PageBlock> = Vec::new();
        let colours = self.colours_section(&t, window, cx);
        let font = self.font_section(&t, window, cx);
        let interface_scale = self.interface_scale_section(&t, window, cx);
        let transparency = self.transparency_section(&t, window, cx);
        let chat_terminal = self.chat_terminal_section(&t, window, cx);
        let anything = colours.is_some()
            || font.is_some()
            || interface_scale.is_some()
            || transparency.is_some()
            || chat_terminal.is_some()
            || !t.searching;
        if !anything {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Theme,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        // CDXC:Settings 2026-10-31: User, of the Text section: "move this section to the top of the
        // interface page in settings".
        blocks.extend(font.map(|section| PageBlock::section("font", section)));
        blocks.extend(interface_scale.map(|section| PageBlock::section("interfaceScale", section)));
        blocks.extend(colours.map(|section| PageBlock::section("theming", section)));
        blocks.extend(transparency.map(|section| PageBlock::section("transparency", section)));
        blocks.extend(chat_terminal.map(|section| PageBlock::section("chatTerminal", section)));
        if !t.searching {
            blocks.extend(
                self.related_section(&p, cx)
                    .map(|section| PageBlock::section("related", section)),
            );
        }
        settings_page(&self.store, SettingsTabId::Theme, &p, blocks, cx)
    }
}
