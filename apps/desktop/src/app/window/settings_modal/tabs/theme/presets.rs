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
use super::controls::{CARD_INSET_X, CARD_INSET_Y};
use super::controls::{ICON_DARK, ICON_LIGHT};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, DispatchPhase, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, ScrollEvent, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled as _, canvas, div, point, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::cell::Cell;
use std::rc::Rc;

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

    /// The Presets group: the heading, then every preset in one row that scrolls sideways.
    ///
    /// CDXC:Theming 2026-11-08 DECISION:
    /// User, of the preset panel: "Please make these a scrollable sideways (1 line) list and make each one 1 word max. it's ugly now please make it nicer area. also takes too much space". The presets sit in one row under the heading, about a card and its label tall. (2026-11-08, User: "presets works well but please put them in a card like the ones below have": the row sits in the same section card as Colours and Transparency and scrolls inside it, its cards passing under the card's edges, where the fades sit in the card's fill.) Labels are one word (the family's own word plus the moon or sun where a family is only Dark and Light). A thin hairline separates families, and a small quiet caption names a family above its first card only where no card says it (Catppuccin, Tokyo Night, Kanagawa): a caption over "Solarized" would say it twice. The row scrolls with a sideways swipe, Shift and the wheel, or a finger; a vertical swipe or wheel turn still scrolls the page. Like the chat box's pills (pills_strip.rs), an edge that hides more fades into the card and dims the cards it cuts, so nothing paints a dark band on glass. On open the row brings the picked card into view.
    pub(super) fn presets_section(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let presets = theme_presets();
        if presets.is_empty() {
            return None;
        }
        let row = &self.presets_row;
        let scroll = row.scroll.clone();
        let cut = row.cut.get();
        let (fade_left, fade_right) = row.fades.get();
        // The scroller's children: a hairline before each family but the first, then its cards.
        // `slots[i]` is preset i's child index, for measuring and scrolling to it.
        let mut items: Vec<AnyElement> = Vec::with_capacity(presets.len() + 8);
        let mut slots: Vec<usize> = Vec::with_capacity(presets.len());
        let mut selected_slot = None;
        let hairline = hsla(p.hairline);
        for (index, preset) in presets.iter().enumerate() {
            let first_of_family = index == 0 || presets[index - 1].family != preset.family;
            if first_of_family && index > 0 {
                items.push(
                    div()
                        .flex_shrink_0()
                        .mx(px(FAMILY_GAP / 2.0))
                        .mt(px(CAPTION_HEIGHT + CARD_TOP))
                        .w(px(1.0))
                        .h(px(PREVIEW_HEIGHT))
                        .bg(hairline)
                        .into_any_element(),
                );
            }
            let caption = (first_of_family
                && !presets
                    .iter()
                    .filter(|other| other.family == preset.family)
                    .any(|other| preset.family.starts_with(other.label.as_str())))
            .then(|| preset.family.clone());
            let selected = theme_preset_matches(values, preset);
            let slot = items.len();
            if selected && selected_slot.is_none() {
                selected_slot = Some(slot);
            }
            slots.push(slot);
            items.push(preset_card(
                p,
                preset,
                caption,
                selected,
                cut & (1 << index) != 0,
                cx,
            ));
        }
        // The settings arrive after the first render, so this waits for a picked card.
        if !self.presets_row.revealed
            && let Some(slot) = selected_slot
        {
            self.presets_row.revealed = true;
            self.presets_row.reveal.set(Some(slot));
        }
        let page = cx.entity_id();
        let fades = self.presets_row.fades.clone();
        let reveal = self.presets_row.reveal.clone();
        let cut_cell = self.presets_row.cut.clone();
        let measure = scroll.clone();
        let wheel = scroll.clone();
        let edge = hsla(p.raised);
        let body = div()
            .relative()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .id("theme-presets-row")
                    .w_full()
                    .flex()
                    .items_start()
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&scroll)
                    // The section card's inner padding, inside the scroll so the cards pass under
                    // the card's edges; it also leaves room for the picked and focus rings.
                    .px(px(CARD_INSET_X))
                    .pt(px(CARD_INSET_Y - 2.0))
                    .pb(px(CARD_INSET_Y))
                    .gap(px(CARD_GAP))
                    .children(items),
            )
            .when(fade_left && !p.glass, |area| {
                area.child(edge_fade(true, edge))
            })
            .when(fade_right && !p.glass, |area| {
                area.child(edge_fade(false, edge))
            })
            .child(
                canvas(
                    |bounds, window, _| window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
                    move |_, hitbox, window, cx| {
                        // The picked card waits for its first layout, then centres in the row.
                        if let Some(slot) = reveal.get()
                            && let Some(card) = measure.bounds_for_item(slot)
                        {
                            reveal.set(None);
                            let view = measure.bounds();
                            let centre = card.left() - view.left() + card.size.width / 2.0;
                            let target = (view.size.width / 2.0 - centre)
                                .clamp(-measure.max_offset().x, px(0.0));
                            measure.set_offset(point(target, measure.offset().y));
                            cx.notify(page);
                        }
                        // The fades and dimmed cards follow what this layout measured.
                        let offset = -f32::from(measure.offset().x);
                        let max = f32::from(measure.max_offset().x);
                        let next_fades = (offset > 0.5, max - offset > 0.5);
                        let view = measure.bounds();
                        let next_cut = slots.iter().enumerate().fold(0u64, |cut, (index, slot)| {
                            let Some(card) = measure.bounds_for_item(*slot) else {
                                return cut;
                            };
                            let left = card.left() + measure.offset().x;
                            let right = left + card.size.width;
                            let hidden = (next_fades.0 && left < view.left() + px(EDGE_FADE / 2.0))
                                || (next_fades.1 && right > view.right() - px(EDGE_FADE / 2.0));
                            if hidden { cut | (1 << index) } else { cut }
                        });
                        let fades_changed = fades.replace(next_fades) != next_fades;
                        if cut_cell.replace(next_cut) != next_cut || fades_changed {
                            cx.notify(page);
                        }
                        let handle = wheel.clone();
                        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture
                                || !hitbox.should_handle_scroll(window)
                            {
                                return;
                            }
                            // Only a sideways swipe (or Shift and the wheel, which the system
                            // turns sideways) moves the row; it never reaches the page, which
                            // would scroll down by it.
                            let delta = event.delta.pixel_delta(window.line_height());
                            if delta.x.abs() <= delta.y.abs() {
                                return;
                            }
                            cx.stop_propagation();
                            let offset = handle.offset();
                            let next = (offset.x + delta.x).clamp(-handle.max_offset().x, px(0.0));
                            if next != offset.x {
                                handle.set_offset(point(next, offset.y));
                                window.refresh();
                            }
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
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

/// The presets row's scroll and what its last paint measured.
#[derive(Default)]
pub(super) struct PresetsRow {
    scroll: ScrollHandle,
    /// An edge hides more cards: (left, right).
    fades: Rc<Cell<(bool, bool)>>,
    /// Which presets (bit per catalog index) an edge cuts.
    cut: Rc<Cell<u64>>,
    /// The picked card was brought into view (once, the first time one is picked).
    revealed: bool,
    /// The child to centre once it is laid out.
    reveal: Rc<Cell<Option<usize>>>,
}

/// A preview's size, the gap between cards and around a family's hairline, and the caption lane.
const CARD_WIDTH: f32 = 80.0;
const PREVIEW_HEIGHT: f32 = 48.0;
const CARD_GAP: f32 = 8.0;
const FAMILY_GAP: f32 = 12.0;
const CAPTION_HEIGHT: f32 = 13.0;
const CARD_TOP: f32 = 3.0;
/// The width of an edge's fade.
const EDGE_FADE: f32 = 28.0;
/// How much of itself a card keeps while an edge cuts it.
const CUT_CARD_OPACITY: f32 = 0.4;

/// The ramp over an edge of the row that hides more: the page's colour at the edge, clear inward.
fn edge_fade(toward_left: bool, edge: gpui::Hsla) -> gpui::Div {
    let ramp = div().absolute().top_0().bottom_0().w(px(EDGE_FADE));
    let ramp = if toward_left {
        ramp.left_0()
    } else {
        ramp.right_0()
    };
    ramp.bg(gpui::linear_gradient(
        90.0,
        gpui::linear_color_stop(if toward_left { edge } else { edge.opacity(0.0) }, 0.0),
        gpui::linear_color_stop(if toward_left { edge.opacity(0.0) } else { edge }, 1.0),
    ))
}

/// One preset card: its family's caption when it names one, the sidebar and work area as the
/// preset paints them, two lines of text, the palette's first four colours, and the one-word label
/// with a sun or moon for its appearance. `cut`: an edge of the row cuts it, so it dims.
fn preset_card(
    p: &SettingsPalette,
    preset: &ThemePreset,
    caption: Option<String>,
    selected: bool,
    cut: bool,
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
    let element_id = SharedString::from(format!("theme-preset-{}", preset.id));
    let focus_ring = p.ring;
    v_flex()
        .id(element_id.clone())
        .role(gpui::Role::Button)
        .accessibility_id(element_id)
        .aria_label(preset.name.clone())
        .aria_selected(selected)
        .focusable()
        .tab_stop(true)
        .flex_shrink_0()
        .w(px(CARD_WIDTH))
        .rounded(px(8.0))
        .when(cut, |this| this.opacity(CUT_CARD_OPACITY))
        .focus_visible(move |this| {
            this.shadow(vec![
                gpui::BoxShadow {
                    color: hsla(focus_ring),
                    offset: point(px(0.0), px(0.0)),
                    blur_radius: px(0.0),
                    spread_radius: px(1.0),
                    inset: false,
                },
                gpui::BoxShadow {
                    color: hsla(rgba_of(focus_ring, 0.2)),
                    offset: point(px(0.0), px(0.0)),
                    blur_radius: px(0.0),
                    spread_radius: px(4.0),
                    inset: false,
                },
            ])
        })
        .cursor_pointer()
        .tooltip(tooltip_text(preset.name.clone()))
        .on_press(cx, move |page, _window, cx| {
            page.apply_theme_preset(&id, cx)
        })
        .on_key_down(|event: &KeyDownEvent, _window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space")
                && !event.keystroke.modifiers.modified()
            {
                // GPUI clicks on key release; keep Space out of Settings' type-to-search.
                cx.stop_propagation();
            }
        })
        .child(
            // The caption lane: every card keeps it so the previews line up.
            div()
                .h(px(CAPTION_HEIGHT))
                .whitespace_nowrap()
                .text_size(px(10.0))
                .line_height(px(CAPTION_HEIGHT))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(hsla(css_fade(p.muted, 0.8)))
                .children(caption),
        )
        .child(
            div()
                .relative()
                .mt(px(CARD_TOP))
                .child(
                    h_flex()
                        .h(px(PREVIEW_HEIGHT))
                        .rounded(px(8.0))
                        .overflow_hidden()
                        .border_1()
                        .border_color(hsla(rim))
                        .hover(move |this| this.border_color(hsla(hover)))
                        .child(
                            div()
                                .w(px(20.0))
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
                .mt(px(6.0))
                .gap(px(3.0))
                .items_center()
                .child(
                    settings_icon(
                        if preset.dark { ICON_DARK } else { ICON_LIGHT },
                        11.0,
                        p.muted,
                    )
                    .flex_shrink_0(),
                )
                .child(
                    div()
                        .whitespace_nowrap()
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
