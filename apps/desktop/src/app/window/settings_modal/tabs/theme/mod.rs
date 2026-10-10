//! The Theme page (packages/core-ui/settings-modal/tabs/theme.tsx (deleted 2026-10-01) and theme-simple-controls.tsx (deleted 2026-10-01)):
//! Colours, Transparency, Chat and terminal, App Icon (hidden while `APP_ICON_CONTROLS_VISIBLE` is
//! off) and the links to related rows on General. Its rows are the General catalog's `theming`
//! and `appIcon` sections, so the same search finds them.
//!
//! CDXC:Theming 2026-09-25 DECISION:
//! User: "I don't like our themes settings in the settings dialog ... even the advanced part shouldn't be just one advanced part", then approved the revamp mockup (docs/2026-09-25/theme-settings-revamp). Theme is three groups, each with its own More options instead of one page-wide Advanced: Colours (Appearance, the theme colour squares with Dark mode / Light mode tabs, Colourfulness; More: the sidebar and work area set apart, a custom colour per appearance, the active pane outline), Transparency (Enable transparency, Strength; More: 1 what shows behind the glass, 2 the pictures or videos, 3 their position, then the tints and Use transparency), and Chat and terminal (the two themes; More: the terminal palettes), plus links to related rows on General. A search hit inside a group's More options opens it.
mod art;
mod colours;
mod controls;
mod gallery;
mod swatches;
mod transparency;

use super::super::super::native_modal_kit::*;
use super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::fields::{
    ButtonVariant, DropdownState, FieldStates, RowSpec, SettingsPage, SliderSaver,
    app_icon_picker_field, reset_key, searchable_select_field, segmented_field, select_field,
    setting_row, settings_button, settings_section, toggle_field, toggle_field_with,
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
    colourfulness_preview, colourfulness_slider, more_options_button, scheme_tabs, swatch,
};
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, RenderImage, SharedString, Styled as _, Window, WindowAppearance, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

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
                "customSidebarTitlebarBackgroundDarknessPercent",
                "customSidebarTitlebarBackgroundTintColor",
                "customSidebarTitlebarLightBackgroundLightnessPercent",
                "customSidebarTitlebarLightBackgroundTintColor",
                "showActivePaneOutline",
                "workspaceActivePaneBorderColor",
            ],
            MoreGroup::Transparency => &[
                "windowGlassSource",
                "windowGlassImageDark",
                "windowGlassImageLight",
                "windowGlassVideoDark",
                "windowGlassVideoLight",
                "windowGlassVideoOnlyOnPower",
                "windowGlassLiveStyleDark",
                "windowGlassLiveStyleLight",
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
    /// Which appearance's colours the squares show (`scheme`).
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
    swatches: HashMap<String, Arc<RenderImage>>,
    pub(super) images: HashMap<String, Arc<RenderImage>>,
    pub(super) posters: HashMap<String, Arc<gpui::Image>>,
    palette_dark_dropdown: DropdownState,
    palette_light_dropdown: DropdownState,
    /// The App Icon list was requested for this open.
    requested_app_icons: bool,
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
            swatches: HashMap::new(),
            images: HashMap::new(),
            posters: HashMap::new(),
            // : the palette lists stop at 320px.
            palette_dark_dropdown: DropdownState::with_max_height(320.0),
            palette_light_dropdown: DropdownState::with_max_height(320.0),
            requested_app_icons: false,
        };
        page.apply_preview_state(cx);
        page
    }

    /// The preview binary's `theme-*` states open the groups they show.
    fn apply_preview_state(&mut self, cx: &mut Context<Self>) {
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

    /// The squares of one appearance at a Colourfulness step, drawn once and kept.
    fn swatch_image(&mut self, dark: bool, preset: &str, step: usize) -> Option<Arc<RenderImage>> {
        let key = format!("{dark}:{preset}:{step}");
        if let Some(image) = self.swatches.get(&key) {
            return Some(image.clone());
        }
        let image = swatches::swatch_image(&swatch_paint(dark, preset, step))?;
        self.swatches.insert(key, image.clone());
        Some(image)
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

    /// `getSettingModificationProps(key)` with the advanced marker off (`advanced={false}`).
    pub(super) fn spec_plain(&self, key: &str, label: &str, description: &str) -> RowSpec {
        RowSpec::new(label.to_string())
            .description(description.to_string())
            .keyed(&self.values, key)
            .advanced(false)
    }

    /// `getSettingModificationProps(key)`.
    pub(super) fn spec(&self, key: &str, label: &str, description: &str) -> RowSpec {
        RowSpec::new(label.to_string())
            .description(description.to_string())
            .keyed(&self.values, key)
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
        let colours_visible = ["sidebarTheme", "darkThemePreset", "lightThemePreset"]
            .iter()
            .any(|key| visible(key))
            || visible("themeSidebarContrast")
            || more_has_hit(MoreGroup::Colours, t.searching, &visible);
        if !colours_visible {
            return None;
        }
        let mut rows: Vec<AnyElement> = Vec::new();
        if visible("sidebarTheme") {
            let options = appearance_choices();
            let value = values.string("sidebarTheme");
            rows.push(segmented_field(
                &p,
                "sidebarTheme",
                t.spec_plain(
                    "sidebarTheme",
                    "Appearance",
                    "System follows your computer’s light or dark mode.",
                ),
                Some(reset_key::<Self>("sidebarTheme")),
                &options,
                Some(value.as_str()),
                None,
                |page: &mut Self, next, _window, cx| page.save("sidebarTheme", json!(next), cx),
                cx,
            ));
        }
        let dark = self.scheme_dark;
        if visible("darkThemePreset") || visible("lightThemePreset") {
            rows.push(self.theme_colour_row(t, dark, cx));
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
                .child(colourfulness_preview(&p, preview_colours(values, dark)))
                .into_any_element();
            rows.push(setting_row(
                &p,
                "themeColourfulness",
                t.spec_plain(
                    "themeSidebarContrast",
                    "Colourfulness",
                    "How much of the colour shows in the sidebar and work area. Vivid is lighter and more colourful, Subtle is deeper and nearly neutral.",
                ),
                Some(reset_key::<Self>("themeSidebarContrast")),
                control,
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
                "separate sidebar and work area, custom colour, active pane outline",
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
            Some("Pick a colour and how see-through Ghostex is. Each group has its own More options.".into()),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }

    /// Theme colour: the Dark mode / Light mode tabs, the sixteen squares and the pairing note.
    fn theme_colour_row(&mut self, t: &ThemeCx, dark: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = t.p;
        let values = &t.values;
        let key = if dark {
            "darkThemePreset"
        } else {
            "lightThemePreset"
        };
        let selected = values.string(key);
        let step = colourfulness_display_step(values.f64("themeSidebarContrast"));
        let squares: Vec<AnyElement> = preset_options(dark)
            .into_iter()
            .map(|(value, label)| {
                let image = self.swatch_image(dark, &value, step);
                let is_selected = value == selected;
                let preset = value.clone();
                swatch(
                    &p,
                    SharedString::from(format!(
                        "theme-colour-{}-{value}",
                        if dark { "dark" } else { "light" }
                    )),
                    label,
                    image,
                    is_selected,
                    move |page, cx| page.select_preset(dark, preset.clone(), cx),
                    cx,
                )
            })
            .collect();
        let other_key = if dark {
            "lightThemePreset"
        } else {
            "darkThemePreset"
        };
        let own = values.string(key);
        let other = values.string(other_key);
        let paired =
            own != "custom" && paired_preset(dark, &own).as_deref() == Some(other.as_str());
        let other_picked = if dark {
            self.picked_light
        } else {
            self.picked_dark
        };
        let other_name = if dark { "Light" } else { "Dark" };
        let note = if paired && !other_picked && other != "custom" {
            format!(
                "{other_name} mode follows with {} until you pick one there.",
                preset_label(!dark, &other)
            )
        } else {
            format!(
                "{other_name} mode keeps its own colour ({}).",
                if other == "custom" {
                    "Custom".to_string()
                } else {
                    preset_label(!dark, &other)
                }
            )
        };
        let readout = if selected == "custom" {
            "Custom".to_string()
        } else {
            preset_label(dark, &selected)
        };
        let picker = v_flex()
            .w_full()
            .gap(px(12.0))
            .child(h_flex().child(scheme_tabs(&p, dark, cx)))
            .child(h_flex().w_full().gap(px(8.0)).children(squares))
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(18.57))
                    .text_color(hsla(p.muted))
                    .child(note),
            )
            .into_any_element();
        let spec = t
            .spec_plain(
                key,
                "Theme colour",
                "The colour of the sidebar and window in each mode. Picking one gives the other mode the same colour until you pick one there.",
            )
            .readout(readout)
            .wide();
        setting_row(
            &p,
            "themeColour",
            spec,
            Some(reset_key::<Self>(key)),
            picker,
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
                RowSpec::new(label).keyed(&values, key).dependent(),
                Some(reset_key::<Self>(key)),
                slider,
                cx,
            ));
        }
        rows.extend(self.custom_colour_rows(t, true, window, cx));
        rows.extend(self.custom_colour_rows(t, false, window, cx));
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

    /// Custom colour in dark (or light) mode, and its tint and depth while it is on.
    ///
    /// CDXC:Theming 2026-06-15 WHY:
    /// The custom background is a constrained contrast slider plus an in-app tint picker, not a freeform colour: calibrated backgrounds keep sidebar row states predictable, and the tint picker must not be the system colour panel.
    fn custom_colour_rows(
        &mut self,
        t: &ThemeCx,
        dark: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let p = t.p;
        let values = t.values.clone();
        let (preset_key, tint_key, depth_key, other_key) = if dark {
            (
                "darkThemePreset",
                "customSidebarTitlebarBackgroundTintColor",
                "customSidebarTitlebarBackgroundDarknessPercent",
                "lightThemePreset",
            )
        } else {
            (
                "lightThemePreset",
                "customSidebarTitlebarLightBackgroundTintColor",
                "customSidebarTitlebarLightBackgroundLightnessPercent",
                "darkThemePreset",
            )
        };
        let mut rows = Vec::new();
        if !(t.visible(tint_key) || t.visible(depth_key)) {
            return rows;
        }
        let custom = values.string(preset_key) == "custom";
        rows.push(toggle_field_with(
            &p,
            if dark {
                "themeCustomDark"
            } else {
                "themeCustomLight"
            },
            t.spec_plain(
                preset_key,
                if dark {
                    "Custom colour in dark mode"
                } else {
                    "Custom colour in light mode"
                },
                if dark {
                    "Pick any tint instead of a colour square for dark mode."
                } else {
                    "Pick any tint instead of a colour square for light mode."
                },
            ),
            custom,
            Some(reset_key::<Self>(preset_key)),
            move |page: &mut Self, checked, _window, cx| {
                let next = if checked {
                    "custom".to_string()
                } else {
                    let other = page.store.read(cx).values().string(other_key);
                    paired_preset(!dark, &other).unwrap_or_else(|| "gray".to_string())
                };
                page.save(preset_key, json!(next), cx);
            },
            cx,
        ));
        if custom && t.visible(tint_key) {
            rows.push(super::super::fields::web_color_picker_field(
                self,
                &p,
                tint_key,
                t.spec(
                    tint_key,
                    if dark {
                        "Dark mode tint"
                    } else {
                        "Light mode tint"
                    },
                    if dark {
                        "Applies a subtle hue to the dark sidebar and window chrome background."
                    } else {
                        "Applies a subtle hue to the light sidebar and window chrome background."
                    },
                )
                .dependent(),
                Some(reset_key::<Self>(tint_key)),
                &values.string(tint_key),
                window,
                cx,
            ));
        }
        if custom && t.visible(depth_key) {
            let catalog = settings_catalog();
            let (min, max) = if dark {
                (
                    catalog.number(
                        module::SETTINGS,
                        "MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT",
                    ),
                    catalog.number(
                        module::SETTINGS,
                        "MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT",
                    ),
                )
            } else {
                (
                    catalog.number(
                        module::SETTINGS,
                        "MIN_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT",
                    ),
                    catalog.number(
                        module::SETTINGS,
                        "MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT",
                    ),
                )
            };
            rows.push(super::super::fields::slider_number_field(
                self,
                &p,
                t.spec(
                    depth_key,
                    if dark {
                        "Dark mode depth"
                    } else {
                        "Light mode depth"
                    },
                    if dark {
                        "85 is softer gray; 100 is black. Text and icons adjust automatically."
                    } else {
                        "60 is a deeper gray; 100 is white. Text and icons adjust automatically."
                    },
                )
                .dependent(),
                Some(reset_key::<Self>(depth_key)),
                super::super::fields::SliderBinding {
                    key: depth_key,
                    min,
                    max,
                    // Half steps, so the box shows the exact depth Colourfulness writes (95.5).
                    step: 0.5,
                },
                values.f64(depth_key),
                window,
                cx,
            ));
        }
        rows
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
        for (key, label, description) in [
            (
                "sessionChatTheme",
                "Chat theme",
                "Follow the app theme, or choose a separate appearance for chat.",
            ),
            (
                "terminalColorScheme",
                "Terminal theme",
                "Follow the app theme, or choose a separate appearance for terminals.",
            ),
        ] {
            if !visible(key) {
                continue;
            }
            rows.push(select_field(
                self,
                &p,
                key,
                t.spec(key, label, description),
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
                "More chat and terminal options",
                "terminal palettes",
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
                        "Uses your configured Ghostty dark theme, or GitHub Dark when no theme is configured.",
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
                        "Uses your configured Ghostty light theme, or GitHub Light when no theme is configured.",
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
        settings_section(&p, "Chat and terminal", None, None, rows)
            .map(IntoElement::into_any_element)
    }

    /// App Icon: shown only while `APP_ICON_CONTROLS_VISIBLE` is on.
    ///
    /// CDXC:Icons 2026-06-28 WHY:
    /// The App Icon section is a custom-image control, not a bundled preset picker: one preview, one Select Image action, and an inline X on the custom preview to restore the default icon, with no separate reset or folder-reveal actions.
    fn app_icon_section(&mut self, t: &ThemeCx, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = t.p;
        let state = self.store.read(cx).host_payload("appIconState").cloned();
        let error = self.store.read(cx).app_icon_error().map(str::to_string);
        let selected = state
            .as_ref()
            .and_then(|state| state.get("selectedId"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let selected_name = state
            .as_ref()
            .and_then(|state| state.get("icons"))
            .and_then(Value::as_array)
            .and_then(|icons| {
                icons
                    .iter()
                    .find(|icon| icon.get("id").and_then(Value::as_str) == Some(selected.as_str()))
            })
            .and_then(|icon| icon.get("name").and_then(Value::as_str))
            .map(str::to_string);
        let field = app_icon_picker_field(
            &p,
            RowSpec::new("App Icon"),
            selected_name,
            selected.is_empty(),
            error,
            |page: &mut Self, _window, cx| {
                let store = page.store.clone();
                store.update(cx, |store, cx| store.choose_app_icon_file(cx));
            },
            |page: &mut Self, _window, cx| {
                let store = page.store.clone();
                store.update(cx, |store, cx| store.select_app_icon(String::new(), cx));
            },
            cx,
        );
        settings_section(
            &p,
            "App Icon",
            Some("Changes the Dock and app-switcher icon. The app file icon may also change when the operating system allows it.".into()),
            None,
            vec![field],
        )
        .map(IntoElement::into_any_element)
    }

    /// Related settings on General: each Open searches General for the row.
    fn related_section(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let rows = [
            ("Chat font and size", "Chat font"),
            ("Sidebar size", "Sidebar Interface Size"),
            (
                "Terminal background colour and image",
                "Terminal background",
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (label, query))| {
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
                        store.set_active_tab(SettingsTabId::General, cx);
                    });
                },
                cx,
            );
            setting_row(
                p,
                format!("theme-related-row-{index}"),
                RowSpec::new(label),
                None,
                button,
                cx,
            )
        })
        .collect();
        settings_section(p, "Related settings on General", None, None, rows)
            .map(IntoElement::into_any_element)
    }
}

impl Render for ThemeTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (t, matching, app_icon_visible) = {
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
            let app_icon_visible = settings_catalog()
                .flag(module::SEARCH_CATALOG, "APP_ICON_CONTROLS_VISIBLE")
                && native_picker
                && store.theme_row_visible("appIcon", "appIconSourceId");
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
                app_icon_visible,
            )
        };
        // The App Icon list is requested once when the page opens (`listAppIcons`).
        if app_icon_visible && !self.requested_app_icons {
            self.requested_app_icons = true;
            let store = self.store.clone();
            cx.defer(move |cx| {
                super::super::store::post_store_message(
                    &store,
                    json!({ "type": "listAppIcons" }),
                    cx,
                );
            });
        }
        let p = t.p;
        let mut blocks: Vec<PageBlock> = Vec::new();
        let colours = self.colours_section(&t, window, cx);
        let transparency = self.transparency_section(&t, window, cx);
        let chat_terminal = self.chat_terminal_section(&t, window, cx);
        let app_icon = if app_icon_visible {
            self.app_icon_section(&t, cx)
        } else {
            None
        };
        let anything = colours.is_some()
            || transparency.is_some()
            || chat_terminal.is_some()
            || app_icon.is_some()
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
        blocks.extend(colours.map(|section| PageBlock::section("theming", section)));
        blocks.extend(transparency.map(|section| PageBlock::section("transparency", section)));
        blocks.extend(chat_terminal.map(|section| PageBlock::section("chatTerminal", section)));
        blocks.extend(app_icon.map(|section| PageBlock::section("appIcon", section)));
        if !t.searching {
            blocks.extend(
                self.related_section(&p, cx)
                    .map(|section| PageBlock::section("related", section)),
            );
        }
        settings_page(&self.store, SettingsTabId::Theme, &p, blocks, cx)
    }
}
