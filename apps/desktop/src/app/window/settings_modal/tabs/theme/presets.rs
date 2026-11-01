//! The Presets group of the Interface page: twenty theme presets (Catppuccin, Dracula, Solarized,
//! Tokyo Night, Nord, Kanagawa, Everforest, Gruvbox Material) above the colours. A preset writes
//! the existing colour settings in one save; it is not a stored choice, so the card that matches
//! the current settings shows as selected and any later edit simply leaves no card selected.
//!
//! CDXC:Theming 2026-11-01 WHY:
//! A preset sets its own appearance's colour to the custom colour Colourfulness Full paints as the
//! theme's background, that appearance's terminal palette, Colourfulness Full for both areas, the
//! terminal background to Follow theme, and the app's appearance to the preset's. The other
//! appearance takes the family's partner the same way the colour squares pair: a light preset
//! always brings its dark partner along, a dark preset brings its light partner only while light
//! mode was not picked on purpose on this page. Chat theme, terminal theme, transparency, the
//! outline, fonts and text strength stay as they are. The table is `THEME_PRESETS` in
//! packages/settings-catalog/src/data/theme_presets.rs.
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::{settings_icon, settings_section, tooltip_text};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::SettingsValues;
use super::ThemeTab;
use super::colours::{
    COLOURFULNESS_LAST_POSITION, Rgb, colourfulness_patch, foreground_for, preview_colours,
};
use super::controls::{CARD_INSET_X, CARD_INSET_Y, ICON_DARK, ICON_LIGHT};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};

/// One row of `THEME_PRESETS`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThemePreset {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) family: String,
    pub(crate) label: String,
    pub(crate) dark: bool,
    pub(crate) partner: Option<String>,
    pub(crate) tint: String,
    pub(crate) palette: String,
    pub(crate) swatch: Vec<String>,
}

/// The presets in the catalog's order.
pub(crate) fn theme_presets() -> Vec<ThemePreset> {
    let text = |entry: &Value, key: &str| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    settings_catalog()
        .module_value(module::SETTINGS, "THEME_PRESETS")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| ThemePreset {
                    id: text(entry, "id"),
                    name: text(entry, "name"),
                    family: text(entry, "family"),
                    label: text(entry, "label"),
                    dark: text(entry, "mode") == "dark",
                    partner: entry
                        .get("partner")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    tint: text(entry, "tintColor"),
                    palette: text(entry, "terminalPalette"),
                    swatch: entry
                        .get("swatch")
                        .and_then(Value::as_array)
                        .map(|colours| {
                            colours
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The settings one appearance's colour, custom tint and terminal palette are saved under.
fn mode_keys(dark: bool) -> (&'static str, &'static str, &'static str) {
    if dark {
        (
            "darkThemePreset",
            "customSidebarTitlebarBackgroundTintColor",
            "terminalGhosttyTheme",
        )
    } else {
        (
            "lightThemePreset",
            "customSidebarTitlebarLightBackgroundTintColor",
            "terminalGhosttyLightTheme",
        )
    }
}

fn appearance_value(dark: bool) -> &'static str {
    if dark { "dark-2" } else { "plain-light" }
}

/// Everything one preset writes, in one patch. `partner` is the other appearance's preset when it
/// follows along (the caller decides; see the module comment), or `None` to leave that
/// appearance's colour and palette as they are.
pub(crate) fn theme_preset_patch(
    preset: &ThemePreset,
    partner: Option<&ThemePreset>,
) -> Map<String, Value> {
    let mut patch = Map::new();
    patch.insert("sidebarTheme".into(), json!(appearance_value(preset.dark)));
    for mode in std::iter::once(preset).chain(partner) {
        let (colour_key, tint_key, palette_key) = mode_keys(mode.dark);
        patch.insert(colour_key.into(), json!("custom"));
        patch.insert(tint_key.into(), json!(mode.tint));
        patch.insert(palette_key.into(), json!(mode.palette));
    }
    patch.extend(colourfulness_patch(COLOURFULNESS_LAST_POSITION));
    patch.insert("terminalBackgroundMode".into(), json!("theme"));
    patch
}

/// Whether the current settings still hold everything `preset` always writes: the appearance, its
/// own appearance's colour, tint and palette, Colourfulness Full with both depths, and the theme
/// terminal background. The partner's keys are left out, because a preset does not always write
/// them.
pub(crate) fn theme_preset_matches(values: &SettingsValues, preset: &ThemePreset) -> bool {
    let (colour_key, tint_key, palette_key) = mode_keys(preset.dark);
    values.string("sidebarTheme") == appearance_value(preset.dark)
        && values.string(colour_key) == "custom"
        && values
            .string(tint_key)
            .trim()
            .eq_ignore_ascii_case(&preset.tint)
        && values.string(palette_key) == preset.palette
        && values.string("terminalBackgroundMode") == "theme"
        && colourfulness_patch(COLOURFULNESS_LAST_POSITION)
            .iter()
            .all(|(key, value)| value.as_f64() == Some(values.f64(key)))
}

/// The patch picking preset `id` saves, and whether it is a dark preset. `light_picked`: light
/// mode's colour was picked on purpose on this page, so a dark preset leaves it alone.
pub(crate) fn theme_preset_choice(
    id: &str,
    light_picked: bool,
) -> Option<(bool, Map<String, Value>)> {
    let presets = theme_presets();
    let preset = presets.iter().find(|preset| preset.id == id)?;
    let partner = preset
        .partner
        .as_deref()
        .and_then(|partner| presets.iter().find(|other| other.id == partner))
        .filter(|_| !preset.dark || !light_picked);
    Some((preset.dark, theme_preset_patch(preset, partner)))
}

impl ThemeTab {
    /// Applies one preset as a single settings save and records its appearance as picked on
    /// purpose, the way picking a colour square does.
    pub(super) fn apply_theme_preset(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some((dark, patch)) = theme_preset_choice(id, self.picked_light) else {
            return;
        };
        if dark {
            self.picked_dark = true;
        } else {
            self.picked_light = true;
        }
        self.save_patch(patch, cx);
    }

    /// The Presets group: the families side by side, each with its cards.
    pub(super) fn presets_section(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let presets = theme_presets();
        let mut families: Vec<(String, Vec<AnyElement>)> = Vec::new();
        for preset in &presets {
            let card = preset_card(p, preset, theme_preset_matches(values, preset), cx);
            match families
                .iter_mut()
                .find(|(family, _)| *family == preset.family)
            {
                Some((_, cards)) => cards.push(card),
                None => families.push((preset.family.clone(), vec![card])),
            }
        }
        let groups: Vec<AnyElement> = families
            .into_iter()
            .map(|(family, cards)| {
                v_flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .line_height(px(14.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(hsla(p.muted))
                            .child(family.to_uppercase()),
                    )
                    .child(h_flex().flex_wrap().gap(px(8.0)).children(cards))
                    .into_any_element()
            })
            .collect();
        let body = h_flex()
            .w_full()
            .px(px(CARD_INSET_X))
            .py(px(CARD_INSET_Y))
            .flex_wrap()
            .items_start()
            .gap_x(px(20.0))
            .gap_y(px(16.0))
            .children(groups)
            .into_any_element();
        settings_section(
            p,
            "Presets",
            Some("Sets the colours below together. You can change each of them afterwards.".into()),
            None,
            vec![body],
        )
        .map(IntoElement::into_any_element)
    }
}

/// One preset card: the sidebar and work area as the preset paints them, two lines of text, the
/// palette's first four colours, and the name with a sun or moon for its appearance.
fn preset_card(
    p: &SettingsPalette,
    preset: &ThemePreset,
    selected: bool,
    cx: &mut Context<ThemeTab>,
) -> AnyElement {
    let (sidebar, work, foreground) = preview_colours(
        &SettingsValues::from_map(theme_preset_patch(preset, None)),
        preset.dark,
    );
    let ink = |alpha: f32| hsla(css_fade(foreground_for(work).gpui(), alpha));
    let line = move |width: f32, alpha: f32| {
        div()
            .h(px(3.0))
            .w(gpui::relative(width))
            .rounded(px(2.0))
            .bg(ink(alpha))
    };
    let dots = preset.swatch.iter().map(|hex| {
        div()
            .size(px(7.0))
            .rounded_full()
            .bg(hsla(colour_of(hex).gpui()))
    });
    let ring = p.foreground_alpha(0.85);
    let rim = p.foreground_alpha(0.1);
    let hover = p.foreground_alpha(0.45);
    let id = preset.id.clone();
    v_flex()
        .id(SharedString::from(format!("theme-preset-{}", preset.id)))
        .w(px(84.0))
        .gap(px(5.0))
        .cursor_pointer()
        .tooltip(tooltip_text(preset.name.clone()))
        .on_click(
            cx.listener(move |page, _: &ClickEvent, _window, cx| page.apply_theme_preset(&id, cx)),
        )
        .child(
            div()
                .relative()
                .child(
                    h_flex()
                        .h(px(52.0))
                        .rounded(px(8.0))
                        .overflow_hidden()
                        .border_1()
                        .border_color(hsla(rim))
                        .hover(move |this| this.border_color(hsla(hover)))
                        .child(
                            div()
                                .w(px(22.0))
                                .h_full()
                                .rounded_tl(px(7.0))
                                .rounded_bl(px(7.0))
                                .bg(hsla(sidebar.gpui())),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .h_full()
                                .gap(px(4.0))
                                .px(px(7.0))
                                .pt(px(7.0))
                                .border_l_1()
                                .border_color(hsla(css_fade(foreground.gpui(), 0.08)))
                                .rounded_tr(px(7.0))
                                .rounded_br(px(7.0))
                                .bg(hsla(work.gpui()))
                                .child(line(0.8, 0.8))
                                .child(line(0.55, 0.45))
                                .child(h_flex().mt(px(4.0)).gap(px(3.0)).children(dots)),
                        ),
                )
                .when(selected, |this| {
                    this.child(
                        div()
                            .absolute()
                            .left(px(-3.5))
                            .top(px(-3.5))
                            .right(px(-3.5))
                            .bottom(px(-3.5))
                            .rounded(px(10.5))
                            .border(px(1.5))
                            .border_color(hsla(ring)),
                    )
                }),
        )
        .child(
            h_flex()
                .gap(px(3.0))
                .items_start()
                .child(
                    settings_icon(
                        if preset.dark { ICON_DARK } else { ICON_LIGHT },
                        11.0,
                        p.muted,
                    )
                    .flex_shrink_0()
                    .mt(px(1.0)),
                )
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(11.0))
                        .line_height(px(13.0))
                        .text_color(hsla(if selected { p.foreground } else { p.muted }))
                        .when(selected, |this| this.font_weight(gpui::FontWeight::MEDIUM))
                        .child(preset.label.clone()),
                ),
        )
        .into_any_element()
}

/// A `#rrggbb` colour from the catalog.
fn colour_of(hex: &str) -> Rgb {
    let channel = |start: usize| {
        hex.get(start..start + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .map_or(0.0, f64::from)
    };
    Rgb([channel(1), channel(3), channel(5)])
}

#[cfg(test)]
mod tests {
    use super::super::colours::{hex_of, preview_scheme_dark};
    use super::*;

    /// The work area each preset paints at Full: the theme's own background. Solarized Dark's
    /// `#002b36` keeps red 3, the least the dark work area's step toward white leaves.
    const WORK_AREA: [(&str, &str); 20] = [
        ("mocha", "#1e1e2e"),
        ("macchiato", "#24273a"),
        ("frappe", "#303446"),
        ("latte", "#eff1f5"),
        ("dracula", "#282a36"),
        ("alucard", "#fffbeb"),
        ("sol-dark", "#032b36"),
        ("sol-light", "#fdf6e3"),
        ("tn-night", "#1a1b26"),
        ("tn-storm", "#24283b"),
        ("tn-moon", "#222436"),
        ("tn-day", "#e1e2e7"),
        ("nord", "#2e3440"),
        ("kg-wave", "#1f1f28"),
        ("kg-dragon", "#181616"),
        ("kg-lotus", "#f2ecbc"),
        ("ef-dark", "#2d353b"),
        ("ef-light", "#fdf6e3"),
        ("gm-dark", "#282828"),
        ("gm-light", "#fbf1c7"),
    ];

    fn apply(start: &[(&str, Value)], id: &str, light_picked: bool) -> SettingsValues {
        let mut map: Map<String, Value> = start
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect();
        map.extend(theme_preset_choice(id, light_picked).unwrap().1);
        SettingsValues::from_map(map)
    }

    #[test]
    fn every_preset_previews_its_theme_background() {
        let presets = theme_presets();
        assert_eq!(presets.len(), WORK_AREA.len());
        for (id, background) in WORK_AREA {
            let preset = presets.iter().find(|preset| preset.id == id).unwrap();
            let values = SettingsValues::from_map(theme_preset_patch(preset, None));
            let (sidebar, work, _) = preview_colours(&values, preset.dark);
            // The sidebar is the picked colour itself, the work area the theme's background.
            assert_eq!(hex_of(sidebar), preset.tint, "{id}");
            assert_eq!(hex_of(work), background, "{id}");
        }
    }

    #[test]
    fn a_preset_writes_its_appearance_colours_full_and_the_theme_terminal_background() {
        let (dark, patch) = theme_preset_choice("mocha", false).unwrap();
        assert!(dark);
        let expected = json!({
            "sidebarTheme": "dark-2",
            "darkThemePreset": "custom",
            "customSidebarTitlebarBackgroundTintColor": "#1c1c2c",
            "terminalGhosttyTheme": "Ghostex Catppuccin Mocha",
            "lightThemePreset": "custom",
            "customSidebarTitlebarLightBackgroundTintColor": "#eaecf2",
            "terminalGhosttyLightTheme": "Ghostex Catppuccin Latte",
            "themeSidebarContrast": -24,
            "themeWorkAreaContrast": -24,
            "customSidebarTitlebarBackgroundDarknessPercent": 85,
            "customSidebarTitlebarLightBackgroundLightnessPercent": 84,
            "terminalBackgroundMode": "theme",
        });
        assert_eq!(Value::Object(patch), expected);
    }

    #[test]
    fn the_other_appearance_follows_like_the_colour_squares() {
        let has_light = |id, light_picked| {
            theme_preset_choice(id, light_picked)
                .unwrap()
                .1
                .contains_key("lightThemePreset")
        };
        let has_dark = |id, light_picked| {
            theme_preset_choice(id, light_picked)
                .unwrap()
                .1
                .contains_key("darkThemePreset")
        };
        // A dark preset brings its light partner until light mode is picked on purpose.
        assert!(has_light("mocha", false));
        assert!(!has_light("mocha", true));
        // A light preset always brings its dark partner.
        assert!(has_dark("latte", true));
        assert!(has_dark("latte", false));
        // Nord has no light partner, so light mode keeps what it had.
        assert!(!has_light("nord", false));
        let patch = theme_preset_choice("nord", false).unwrap().1;
        assert!(!patch.contains_key("terminalGhosttyLightTheme"));
        assert!(!patch.contains_key("customSidebarTitlebarLightBackgroundTintColor"));
        // Full and both depths are written either way.
        assert_eq!(
            patch["customSidebarTitlebarLightBackgroundLightnessPercent"],
            json!(84)
        );
        assert!(theme_preset_choice("no-such-preset", false).is_none());
    }

    #[test]
    fn a_preset_keeps_other_settings_and_shows_as_picked_until_edited() {
        let start = [
            ("sessionChatTheme", json!("light")),
            ("terminalColorScheme", json!("dark")),
            ("windowGlass", json!("frosted")),
            ("sessionChatTextStrengthPercent", json!(40)),
            ("lightThemePreset", json!("slate")),
        ];
        let values = apply(&start, "kg-wave", true);
        for (key, value) in &start[..4] {
            assert_eq!(&values.map()[*key], value, "{key}");
        }
        // Light mode was picked on purpose, so it keeps Slate.
        assert_eq!(values.string("lightThemePreset"), "slate");
        let presets = theme_presets();
        let picked: Vec<&str> = presets
            .iter()
            .filter(|preset| theme_preset_matches(&values, preset))
            .map(|preset| preset.id.as_str())
            .collect();
        assert_eq!(picked, ["kg-wave"]);
        // A hand edit afterwards leaves no card picked.
        let mut edited = values.map().clone();
        edited.insert("themeWorkAreaContrast".into(), json!(-20));
        let edited = SettingsValues::from_map(edited);
        assert!(
            !presets
                .iter()
                .any(|preset| theme_preset_matches(&edited, preset))
        );
    }

    #[test]
    fn any_change_to_what_a_preset_always_writes_unpicks_it() {
        let presets = theme_presets();
        for preset in &presets {
            let patch = theme_preset_patch(preset, None);
            assert!(
                theme_preset_matches(&SettingsValues::from_map(patch.clone()), preset),
                "{}",
                preset.id
            );
            for key in patch.keys() {
                let mut edited = patch.clone();
                let changed = match key.as_str() {
                    "terminalBackgroundMode" => json!("pure"),
                    "sidebarTheme" => json!("system"),
                    "darkThemePreset" | "lightThemePreset" => json!("gray"),
                    "themeSidebarContrast" | "themeWorkAreaContrast" => json!(-23.5),
                    "customSidebarTitlebarBackgroundDarknessPercent" => json!(90),
                    "customSidebarTitlebarLightBackgroundLightnessPercent" => json!(90),
                    _ => json!("#123456"),
                };
                edited.insert(key.clone(), changed);
                assert!(
                    !theme_preset_matches(&SettingsValues::from_map(edited), preset),
                    "{}: {key}",
                    preset.id
                );
            }
        }
    }

    #[test]
    fn the_colourfulness_preview_follows_the_appearance_a_preset_sets() {
        // The page opened in dark mode (window dark, Appearance Dark), then Latte was picked.
        let values = apply(&[("sidebarTheme", json!("dark-2"))], "latte", false);
        let dark = preview_scheme_dark(&values.string("sidebarTheme"), true);
        assert!(!dark);
        let (sidebar, work, _) = preview_colours(&values, dark);
        assert_eq!(hex_of(sidebar), "#eaecf2");
        assert_eq!(hex_of(work), "#eff1f5");
        // And back to a dark preset from a light window.
        let values = apply(&[("sidebarTheme", json!("plain-light"))], "mocha", true);
        assert!(preview_scheme_dark(&values.string("sidebarTheme"), false));
        // System still shows the window's own appearance.
        assert!(preview_scheme_dark("system", true));
        assert!(!preview_scheme_dark("system", false));
    }
}
