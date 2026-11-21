//! The rail's Interface pages (packages/core-ui/settings-modal/tabs/theme.tsx (deleted 2026-10-01) and theme-simple-controls.tsx (deleted 2026-10-01)):
//! Themes (Themes, Colours, Glass, Chat colours, Fine-tune colours, Terminal colours), Text & Fonts,
//! Size & Zoom and Window & Layout, one view drawing the page it was made for. Each page's rows are
//! one General catalog section (`theming`, `textFonts`, `sizeZoom`, `windowLayout`), so the same
//! search finds them.
//!
//! CDXC:Theming 2026-10-16 DECISION:
//! User: "doesn't look so good … can we please make this look nicer? … don't copy everything", then picked direction A (Calm rows) of the mockups in docs/2026-10-16/theme-page. Supersedes the 2026-09-25 layout. The Theme page's rows carry an icon tile (this page only; other pages keep their rows). Appearance is three preview cards (System, Light, Dark). Each group is one card whose rare rows fold under a More row: Colours (Dark mode colour and Light mode colour as two rows of squares, Colourfulness; More: the sidebar and work area set apart, the active pane outline), Transparency (one Off / Dark only / Always control replacing Enable transparency and Use transparency, Strength, Blur, Behind the glass with its pictures or Live gallery; More: picture position, Live speed and brightness, play only when plugged in, the four tints, menu blur) and Chat and terminal (the two themes; More: the terminal palettes), then Related on General. The three-way choices keep the existing row control ("same as what we have now dont change this") and Blur stays as it was ("dont change this"). Custom colour is a "+" square after the squares that opens the Pick Color dialog right at it; no rows open under the colour row (2026-10-17, User: "very bad to have the same colors twice here why??? pls fix no need for these 2 rows that appear when i click on the + option in the colors pls"). The custom colour's depth has no control of its own: Colourfulness sets it (2026-10-17, the user's pick: "Remove it: Colourfulness alone sets how deep the custom colour is"). The squares are "smaller and simpler": 20px, one flat colour, today's ring drawn thinner. A search hit inside a group's More opens it. The page itself is split into four since 2026-11-18 (CDXC:Settings in model.rs): Appearance and the presets lead Themes, the active pane outline moved to Window & Layout, the terminal palettes show without a More row, Theme colours is Fine-tune colours with every row under its More row, and Related on General is gone (its two links sat in Size & Zoom and Terminal colours until 2026-11-19, when the settings they opened moved onto these pages: CDXC:Settings in the settings catalog's general/theme.rs).
mod art;
mod chat_colours;
mod chat_preview;
mod colours;
mod controls;
mod font;
mod gallery;
mod interface_scale;
mod panels;
pub(crate) mod presets;
mod prompt_gallery;
pub(crate) mod role_defaults;
mod roles;
mod swatches;
mod terminal_background;
mod terminal_text;
pub(super) mod transparency;

use super::super::super::native_modal_kit::*;
use super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::fields::{
    DropdownState, FieldStates, RowSpec, SettingsPage, SliderSaver, open_tint_picker, reset_key,
    searchable_select_field, select_field, setting_row, settings_section, toggle_field,
    toggle_field_with, trigger_tint_picker,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, SettingsStoreEvent, SettingsValues};
use colours::{
    COLOURFULNESS_LAST_POSITION, area_colourfulness_patch, colourfulness_display_step,
    colourfulness_patch, colourfulness_points, colourfulness_step_index, paired_preset,
    preset_label, preset_options, preview_colours, preview_scheme_dark, swatch_paint,
};
use controls::{
    ICON_APPEARANCE, ICON_CHAT, ICON_COLOURFULNESS, ICON_DARK, ICON_LIGHT, ICON_TERMINAL,
    SWATCH_GAP, appearance_card, colourfulness_preview, colourfulness_slider, custom_swatch,
    more_options_button, swatch,
};
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, RenderImage, SharedString, Styled as _, Window, WindowAppearance, canvas, div, px,
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
    ThemeColours,
    /// Size & Zoom's More size options: Scale areas separately and what it shows.
    Size,
}

impl MoreGroup {
    fn index(self) -> usize {
        match self {
            MoreGroup::Colours => 0,
            MoreGroup::Transparency => 1,
            MoreGroup::ThemeColours => 2,
            MoreGroup::Size => 3,
        }
    }

    /// `THEME_MORE_OPTIONS_KEYS`: the rows inside the group's More options, so a search hit on one
    /// of them opens it.
    fn keys(self) -> &'static [&'static str] {
        match self {
            MoreGroup::Colours => &["themeSidebarContrast", "themeWorkAreaContrast"],
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
            MoreGroup::ThemeColours => &["themeRoles"],
            MoreGroup::Size => &[
                "interfaceScaleAreasSeparately",
                "agentManagerZoomPercent",
                "sessionChatZoomPercent",
                "filesZoomPercent",
                "terminalFontSize",
                "linuxDisplayScale",
            ],
        }
    }
}

thread_local! {
    /// `rememberedThemeMoreOptionsOpen`: each group's More options stays as it was left for the rest
    /// of this app run.
    static REMEMBERED_MORE_OPEN: Cell<[bool; 4]> = const { Cell::new([false; 4]) };
}

/// One of the rail's Interface pages (Themes, Text & Fonts, Size & Zoom, Window & Layout): the same
/// view, drawing the groups of page `page` (CDXC:Settings 2026-11-18 in model.rs).
pub(crate) fn theme_tab_view(
    store: &Entity<SettingsStore>,
    page: SettingsTabId,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| ThemeTab::new(store.clone(), page, window, cx))
        .into()
}

/// CDXC:Settings 2026-11-08 WHY:
/// At a narrow Settings window (640 base) the Colours rows broke: a row keeps its control on the
/// right at up to 60% of the width without shrinking it, and the sixteen squares (436pt) or the
/// Colourfulness slider (about 400pt) did not fit beside the label and its readout, so "Light mode
/// colour" wrapped one letter per line and the squares overlapped it. Below these card widths the
/// row puts its control under the label (the Appearance row's layout); the squares also wrap if
/// even that is too narrow. The widths are the label, readout, icon tile, padding and control at
/// their longest ("Midnight · follows dark mode").
const COLOUR_ROW_SIDE_BY_SIDE: f32 = 880.0;
const COLOURFULNESS_ROW_SIDE_BY_SIDE: f32 = 640.0;

pub(crate) struct ThemeTab {
    pub(super) store: Entity<SettingsStore>,
    /// Which Interface page this view draws.
    page: SettingsTabId,
    fields: FieldStates,
    /// Which appearance the Colourfulness preview shows while Appearance is System (the window's
    /// own, or light for the preview binary's `theme-light-scheme`).
    pub(super) scheme_dark: bool,
    /// `pickedOnPurpose`: a colour was picked in that appearance, so the other stops following it.
    picked_dark: bool,
    picked_light: bool,
    /// `splitAreas`: More colour options sets the sidebar and work area apart.
    split_areas: bool,
    more_open: [bool; 4],
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
    /// The Presets row's sideways scroll (presets.rs).
    presets_row: presets::PresetsRow,
    /// The agent-made looks: the one being applied or undone, and the last one applied
    /// (prompt_gallery.rs).
    prompt_gallery: prompt_gallery::PromptGallery,
    /// The Colours card's width at the last paint, so rows too narrow for their label and control
    /// side by side put the control under the label (`stacks`).
    row_width: Rc<Cell<f32>>,
}

fn palette_dark_dropdown(page: &mut ThemeTab) -> &mut DropdownState {
    &mut page.palette_dark_dropdown
}

fn palette_light_dropdown(page: &mut ThemeTab) -> &mut DropdownState {
    &mut page.palette_light_dropdown
}

impl ThemeTab {
    fn new(
        store: Entity<SettingsStore>,
        page: SettingsTabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
        // `initialThemeScheme`: what System shows, the window's own appearance; a set Appearance
        // wins over it while rendering (`preview_scheme_dark`).
        let scheme_dark = !matches!(
            window.appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        let sidebar = values.f64("themeSidebarContrast");
        let split_areas = sidebar != values.f64("themeWorkAreaContrast")
            || colours::colourfulness_step_for_points(sidebar).is_none();
        let separately =
            ghostex_settings_catalog::interface_scale::scale_areas_separately(values.map());
        let mut page = Self {
            store,
            page,
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
            presets_row: presets::PresetsRow::default(),
            prompt_gallery: prompt_gallery::PromptGallery::default(),
            row_width: Rc::new(Cell::new(f32::INFINITY)),
        };
        // More theme colours opens by itself while one of its colours is set (roles.rs).
        if roles::more_roles_set(&page.store.read(cx).values().value("themeRoles")) > 0 {
            page.more_open[MoreGroup::ThemeColours.index()] = true;
        }
        // More size options opens by itself while the areas are sized apart, so their sliders show.
        if separately {
            page.more_open[MoreGroup::Size.index()] = true;
        }
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
        if state == "theme-more"
            || state.starts_with("theme-glass")
            || state == "theme-custom"
            || state == "theme-colours"
        {
            self.more_open = [true; 4];
        }
        // The preview binary's `theme-role-picker` opens Your messages' Pick Color dialog.
        if state == "theme-role-picker" {
            let roles = self.store.read(cx).values().value("themeRoles");
            let hex = roles["dark"]["userMessage"]
                .as_str()
                .map_or("#808080", |raw| raw.get(..7).unwrap_or(raw))
                .to_string();
            self.open_role_picker("userMessage", false, &hex, None, window, cx);
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
        let colours_visible = dark_colour_visible
            || light_colour_visible
            || visible("themeSidebarContrast")
            || more_has_hit(MoreGroup::Colours, t.searching, &visible);
        if !colours_visible {
            return None;
        }
        let mut rows: Vec<AnyElement> = Vec::new();
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
                    preview_colours(
                        values,
                        preview_scheme_dark(&values.string("sidebarTheme"), self.scheme_dark),
                    ),
                ))
                .into_any_element();
            rows.push(setting_row(
                &p,
                "themeColourfulness",
                t.spec_plain(
                    "themeSidebarContrast",
                    "Colourfulness",
                    "How much of the colour shows in the sidebar and work area. Subtle is deeper and nearly neutral, Vivid lighter and more colourful, and Full is the colour you picked itself.",
                )
                .icon(ICON_COLOURFULNESS)
                .stacked(self.stacks(COLOURFULNESS_ROW_SIDE_BY_SIDE)),
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
                "The sidebar and work area apart",
                shown,
                move |page, _window, cx| page.set_more_open(MoreGroup::Colours, !open, cx),
                cx,
            ));
        }
        if shown {
            rows.extend(self.more_colour_rows(t, window, cx));
        }
        settings_section(&p, "Colours", None, None, rows).map(IntoElement::into_any_element)
    }

    /// The window frame rows of the Advanced fold: the window buttons the sidebar's collapse keeps,
    /// then the side the close buttons of dialogs and toasts sit on. macOS only; None elsewhere or
    /// when a search hides both.
    ///
    /// CDXC:Settings 2026-11-08 DECISION:
    /// User: "please also move this whole setting under a new advanced area in the interface page in settings". Both rows moved out of Colours into the page's collapsed Advanced fold (`ADVANCED_MAIN_SETTING_KEYS`), as About does with Experimental: a search shows the matching rows in this section in place and the fold stays out. Superseded 2026-11-18 (CDXC:Settings in model.rs, Q4153): the rows are the Window & Layout page's Window frame group, shown without a fold.
    fn window_frame_section(&mut self, t: &ThemeCx, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !cfg!(target_os = "macos") {
            return None;
        }
        let p = t.p;
        let values = &t.values;
        let mut rows: Vec<AnyElement> = Vec::new();
        if t.visible("showWindowButtonsWhenSidebarCollapsed") {
            rows.push(toggle_field(
                self,
                &p,
                "showWindowButtonsWhenSidebarCollapsed",
                t.spec_plain(
                    "showWindowButtonsWhenSidebarCollapsed",
                    "Show the window buttons when the sidebar is collapsed",
                    "Keep the red, yellow and green window buttons in the window's top left while the sidebar is collapsed. Off hides them until you open the floating sidebar, which carries them in its top row. Full screen keeps its own title bar either way.",
                ),
                values.bool("showWindowButtonsWhenSidebarCollapsed"),
                cx,
            ));
        }
        // CDXC:AppModal 2026-11-10 DECISION in window/round_close_button.rs: dialogs and toasts
        // close from their top left on macOS; this row puts the X back on the right.
        if t.visible("closeButtonsOnRight") {
            rows.push(toggle_field(
                self,
                &p,
                "closeButtonsOnRight",
                t.spec_plain(
                    "closeButtonsOnRight",
                    "Close buttons on the right",
                    "Put the X that closes dialogs and pop-up messages at their top right. Off keeps it at their top left, where macOS puts a window's close button.",
                ),
                values.bool("closeButtonsOnRight"),
                cx,
            ));
        }
        settings_section(&p, "Window frame", None, None, rows).map(IntoElement::into_any_element)
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
            .flex_wrap()
            .gap(px(SWATCH_GAP))
            .items_center()
            .children(squares)
            .into_any_element();
        let stacked = self.stacks(COLOUR_ROW_SIDE_BY_SIDE);
        let row = setting_row(
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
            .icon(if dark { ICON_DARK } else { ICON_LIGHT })
            .stacked(stacked),
            Some(reset_key::<Self>(key)),
            control,
            cx,
        );
        if dark { self.measure_row(row, cx) } else { row }
    }

    /// Whether a row needing `side_by_side` points for its label and control together stacks them.
    fn stacks(&self, side_by_side: f32) -> bool {
        self.row_width.get() < side_by_side
    }

    /// `row` with its width recorded in `row_width`, redrawing the page when it changes.
    fn measure_row(&self, row: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let width = self.row_width.clone();
        let page = cx.entity_id();
        div()
            .relative()
            .w_full()
            .child(row)
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let next = f32::from(bounds.size.width);
                        if (width.replace(next) - next).abs() > 0.5 {
                            cx.notify(page);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
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
                    let position =
                        value.round().clamp(0.0, COLOURFULNESS_LAST_POSITION as f64) as usize;
                    store.update(cx, |store, cx| {
                        if store.values().f64(key) != colourfulness_points(position) {
                            store.apply_patch(
                                area_colourfulness_patch(key, position),
                                "settings:control",
                                cx,
                            );
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
        rows
    }

    /// Window & Layout's Panes: the active pane outline and its colour (in More colour options
    /// until 2026-11-18).
    fn panes_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = t.p;
        let values = t.values.clone();
        let mut rows: Vec<AnyElement> = Vec::new();
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
        settings_section(&p, "Panes", None, None, rows).map(IntoElement::into_any_element)
    }

    /// Themes: Appearance's three preview cards, then one row of the agent-made looks and the
    /// presets (presets.rs), the applied look's line and Describe a look (prompt_gallery.rs).
    fn themes_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut rows: Vec<AnyElement> = Vec::new();
        if t.visible("sidebarTheme") {
            rows.push(self.appearance_row(t, window, cx));
        }
        if !t.searching {
            let p = t.p;
            rows.extend(self.presets_strip(&p, &t.values, cx));
            rows.extend(self.look_applied_row(&p, &t.values, cx));
            rows.extend(self.background_row(&p, &t.values, cx));
            rows.push(self.artworks_row(&p, &t.values, cx));
            rows.push(self.describe_look_row(&p, cx));
        }
        settings_section(&t.p, "Themes", None, None, rows).map(IntoElement::into_any_element)
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

    /// Chat theme or Terminal theme: Follow app, Light, Dark or System.
    pub(super) fn appearance_override_row(
        &mut self,
        t: &ThemeCx,
        key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !t.visible(key) {
            return None;
        }
        let (label, description, icon) = if key == "sessionChatTheme" {
            (
                "Chat theme",
                "Follow the app theme, or choose a separate appearance for chat.",
                ICON_CHAT,
            )
        } else {
            (
                "Terminal theme",
                "Follow the app theme, or choose a separate appearance for terminals.",
                ICON_TERMINAL,
            )
        };
        let options = settings_catalog().options(module::SETTINGS, "SESSION_CHAT_THEME_OPTIONS");
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let p = t.p;
        Some(select_field(
            self,
            &p,
            key,
            t.spec(key, label, description).icon(icon),
            Some(reset_key::<Self>(key)),
            &options,
            &t.values.choice(key, &allowed),
            None,
            move |page: &mut Self, next, _window, cx| page.save(key, json!(next), cx),
            window,
            cx,
        ))
    }

    /// Terminal colours: Terminal theme, the two terminal palettes, then the terminal's background
    /// (terminal_background.rs).
    fn terminal_colours_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = t.p;
        let values = t.values.clone();
        let visible_fn = t.visible.clone();
        let visible = |key: &str| visible_fn(key);
        let mut rows: Vec<AnyElement> = Vec::new();
        rows.extend(self.appearance_override_row(t, "terminalColorScheme", window, cx));
        let base = settings_catalog().options(module::SETTINGS, "GHOSTTY_THEME_SETTING_OPTIONS");
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
                    "Uses the dark theme set in Ghostex's terminal config, or Ghostex Kanagawa Dragon when none is set.",
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
        rows.extend(self.terminal_background_rows(t, window, cx));
        settings_section(&p, "Terminal colours", None, None, rows)
            .map(IntoElement::into_any_element)
    }
}

/// The catalog section an Interface page renders (CDXC:Settings 2026-11-18 in the settings
/// catalog's general/theme.rs).
fn page_section(page: SettingsTabId) -> &'static str {
    match page {
        SettingsTabId::TextFonts => "textFonts",
        SettingsTabId::SizeZoom => "sizeZoom",
        SettingsTabId::WindowLayout => "windowLayout",
        _ => "theming",
    }
}

impl Render for ThemeTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (t, matching) = {
            let store = self.store.read(cx);
            let hud = store.hud().cloned().unwrap_or_default();
            let native_picker =
                hud.get("appIconPickerUnavailable").and_then(Value::as_bool) != Some(true);
            // `rowVisible(settingsSearch[section], key)` for this page's catalog section, looked up
            // once per render.
            let section_id = page_section(self.page);
            let theming_keys: HashMap<String, bool> = settings_catalog()
                .general_section(section_id)
                .map(|section| {
                    section
                        .settings
                        .iter()
                        .map(|row| {
                            (
                                row.key.clone(),
                                store.theme_row_visible(section_id, &row.key),
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
        let tab = self.page;
        let mut blocks: Vec<PageBlock> = Vec::new();
        match tab {
            SettingsTabId::TextFonts => {
                let (fonts, text) = self.font_sections(&t, window, cx);
                blocks.extend(fonts.map(|section| PageBlock::section("fonts", section)));
                blocks.extend(
                    self.terminal_text_section(&t, window, cx)
                        .map(|section| PageBlock::section("terminalText", section)),
                );
                blocks.extend(text.map(|section| PageBlock::section("text", section)));
            }
            SettingsTabId::SizeZoom => {
                blocks.extend(
                    self.interface_scale_section(&t, window, cx)
                        .map(|section| PageBlock::section("size", section)),
                );
                blocks.extend(
                    self.panel_sizes_section(&t, window, cx)
                        .map(|section| PageBlock::section("panelSizes", section)),
                );
            }
            SettingsTabId::WindowLayout => {
                blocks.extend(
                    self.window_frame_section(&t, cx)
                        .map(|section| PageBlock::section("windowFrame", section)),
                );
                blocks.extend(
                    self.panes_section(&t, window, cx)
                        .map(|section| PageBlock::section("panes", section)),
                );
                blocks.extend(
                    self.panels_section(&t, window, cx)
                        .map(|section| PageBlock::section("panels", section)),
                );
            }
            _ => {
                // Q4153's order: Themes (Appearance, the looks and presets, Describe a look),
                // Colours, Glass, Chat colours, Fine-tune colours, Terminal colours.
                blocks.extend(
                    self.themes_section(&t, window, cx)
                        .map(|section| PageBlock::section("themes", section)),
                );
                blocks.extend(
                    self.colours_section(&t, window, cx)
                        .map(|section| PageBlock::section("theming", section)),
                );
                blocks.extend(
                    self.transparency_section(&t, window, cx)
                        .map(|section| PageBlock::section("transparency", section)),
                );
                blocks.extend(
                    self.chat_colours_section(&t, window, cx)
                        .map(|section| PageBlock::section("chatColours", section)),
                );
                blocks.extend(
                    self.theme_colours_section(&t, window, cx)
                        .map(|section| PageBlock::section("themeColours", section)),
                );
                blocks.extend(
                    self.terminal_colours_section(&t, window, cx)
                        .map(|section| PageBlock::section("terminalColours", section)),
                );
            }
        }
        // The preview binary's `pick-color` state (and a host without a system colour panel) opens
        // the terminal background's Pick Color dialog, once its row has drawn (terminal_background.rs).
        if tab == SettingsTabId::Theme
            && self.store.read(cx).request().open_color_picker.as_deref()
                == Some("workspaceBackgroundColor")
        {
            self.store
                .update(cx, |store, _| store.request_mut().open_color_picker = None);
            super::super::fields::open_picker(
                self,
                "workspaceBackgroundColor",
                "#121212",
                window,
                cx,
            );
        }
        if blocks.is_empty() && t.searching {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                tab,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        settings_page(&self.store, tab, &p, blocks, cx)
    }
}
