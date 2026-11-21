//! The Fine-tune colours section of the Themes page (Theme colours until 2026-11-18): the theme roles (`themeRoles`,
//! packages/settings-catalog/src/theme_roles.rs) one row each, a colour editor per appearance the
//! app shows (a swatch that opens the Pick Color dialog, the text box, an opacity slider) and a
//! Reset. The roles the Chat colours group shows (chat_colours.rs) are not repeated here. Borders,
//! Dividers, Text and Grey text show at once; the rest fold under More theme colours, which opens
//! by itself while one of them is set.
//!
//! CDXC:Theming 2026-11-14 DECISION:
//! User (Q3195, agent theming stage B): "if the user should see the roles, a compact Theme colours section on the Interface page (dependent rows hidden per the AGENTS.md rule), with a Reset to default per role". A role's field for an appearance the app does not show is hidden: Dark mode only shows the dark field, Light mode only the light one, System both. A field takes an opacity (4%) or a colour (#7dd3fc, #7dd3fc80); it saves once what is typed reads as one, and an empty field puts the theme's own colour back.
//!
//! CDXC:Theming 2026-11-18 DECISION:
//! User (Q3980 "all", Q3981 "both"): every Theme colours row is a colour editor, a swatch opening the Pick Color dialog, an opacity slider and Reset, with the text box kept for exact values; More theme colours opens itself while one of its colours is set and says how many are. The slider keeps a role's kind: an opacity stays an opacity, a colour keeps its colour and changes its alpha. Picking a colour keeps the opacity the role had. A role not set shows an empty dashed swatch and no slider until a colour is picked or typed (superseded below).
//!
//! CDXC:Theming 2026-11-18 DECISION:
//! User: "when we pick default for these colors we still need to show the actual color value and transparency we set there by default so user can adjust it slightly easily". A role not set shows what it paints now in the active theme (role_defaults.rs, from the app's own derivation, app/helpers/theme_role_defaults.rs): a filled swatch with a dashed rim, its value in grey in the text box and the slider at its opacity. Picking a colour or moving the slider starts from that value and saves it as the role's own; Reset puts the default back.

use super::super::super::super::native_modal_kit::hsla;
use super::super::super::fields::{
    CONTROL_LANE_WIDTH, ColorSaver, FieldStates, PageAction, RowSpec, SliderBinding, SliderSaver,
    open_tint_picker, set_color_saver, setting_row, step_slider, tooltip_text, trigger_tint_picker,
};
use super::super::super::palette::SettingsPalette;
use super::controls::more_options_button;
use super::role_defaults::role_default;
use super::{MoreGroup, ThemeCx, ThemeTab};
use ghostex_settings_catalog::theme_roles::{
    RoleValue, THEME_ROLES, THEME_ROLES_SETTING, ThemeRole, appearance_key, format_role_value,
    parse_role_value,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// The roles the Chat colours group shows (chat_colours.rs), so Theme colours leaves them out.
pub(super) const CHAT_COLOUR_ROLES: [&str; 4] = ["userMessage", "chatBox", "surface", "accent"];

/// How many of Fine-tune colours' own roles show without opening More theme colours: none, so the
/// group is one More row until opened (CDXC:Settings 2026-11-18 in model.rs, Q4153).
const FIRST_ROLES: usize = 0;

/// The swatch's side, and the slider track's width beside the text box.
const SWATCH: f32 = 24.0;
const TEXT_BOX: f32 = 128.0;
const READOUT: f32 = 36.0;

/// The colour the Pick Color dialog starts on for a role not set whose default is not known (the
/// preview binary).
const UNSET_PICK: &str = "#808080";

/// The appearances the app shows with this Appearance setting: `(light, label)`.
pub(super) fn shown_appearances(appearance: &str) -> Vec<(bool, &'static str)> {
    match appearance {
        "plain-light" => vec![(true, "Light")],
        "system" => vec![(false, "Dark"), (true, "Light")],
        _ => vec![(false, "Dark")],
    }
}

/// Theme colours' own roles: every role but the Chat colours group's.
fn own_roles() -> impl Iterator<Item = &'static ThemeRole> {
    THEME_ROLES
        .iter()
        .filter(|role| !CHAT_COLOUR_ROLES.contains(&role.id))
}

/// How many roles under More theme colours `saved` sets, in either appearance.
pub(super) fn more_roles_set(saved: &Value) -> usize {
    own_roles()
        .skip(FIRST_ROLES)
        .filter(|role| {
            [false, true]
                .into_iter()
                .any(|light| saved_role(saved, light, role.id).is_some())
        })
        .count()
}

/// Role `id` of one appearance as saved, when it reads as a role value.
pub(crate) fn saved_role(saved: &Value, light: bool, id: &str) -> Option<RoleValue> {
    saved
        .get(appearance_key(light))?
        .get(id)?
        .as_str()
        .and_then(|raw| parse_role_value(raw).ok())
}

/// `themeRoles` with `role` of one appearance set to `value`, or taken off for `None`.
pub(crate) fn with_role(current: &Value, light: bool, role: &str, value: Option<String>) -> Value {
    let mut roles = current.as_object().cloned().unwrap_or_default();
    let appearance = roles
        .entry(appearance_key(light))
        .or_insert_with(|| Value::Object(Map::new()));
    if !appearance.is_object() {
        *appearance = Value::Object(Map::new());
    }
    if let Some(map) = appearance.as_object_mut() {
        match value {
            Some(value) => {
                map.insert(role.to_string(), Value::String(value));
            }
            None => {
                map.remove(role);
            }
        }
    }
    roles.retain(|_, appearance| appearance.as_object().is_none_or(|map| !map.is_empty()));
    Value::Object(roles)
}

/// Saves role `role` of one appearance as `next(current)`, through the store `store`: now when
/// `commit`, else after the debounce (a slider or the picker still moving).
fn save_role_in(
    store: &gpui::Entity<super::super::super::store::SettingsStore>,
    light: bool,
    role: &'static str,
    commit: bool,
    next: impl FnOnce(Option<RoleValue>) -> Option<RoleValue>,
    cx: &mut gpui::App,
) {
    store.update(cx, |store, cx| {
        let current = store.values().value(THEME_ROLES_SETTING);
        let Some(value) = next(saved_role(&current, light, role)) else {
            return;
        };
        let updated = with_role(&current, light, role, Some(format_role_value(value)));
        if updated == current {
            return;
        }
        if commit {
            store.update_setting(THEME_ROLES_SETTING, updated, cx);
        } else {
            store.update_setting_debounced(THEME_ROLES_SETTING, updated, cx);
        }
    });
}

/// A field key that lives for the app's run: the Pick Color dialog and slider states are keyed by
/// `&'static str`, and there are at most two per role and appearance.
fn field_key(kind: &str, role: &str, light: bool) -> &'static str {
    thread_local! {
        static KEYS: RefCell<HashMap<String, &'static str>> = RefCell::new(HashMap::new());
    }
    let key = format!("theme-role-{kind}-{role}-{}", appearance_key(light));
    KEYS.with(|keys| {
        *keys
            .borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| Box::leak(key.into_boxed_str()))
    })
}

/// A role's opacity, 0 to 1.
pub(crate) fn alpha_of(value: RoleValue) -> f32 {
    match value {
        RoleValue::Ink(alpha) | RoleValue::Colour { alpha, .. } | RoleValue::Base { alpha, .. } => {
            alpha
        }
    }
}

/// `#rrggbb` of what a role paints in one appearance over `base_rgb`.
pub(crate) fn hex_of(value: RoleValue, light: bool, base_rgb: u32) -> String {
    let [r, g, b, _] = value.rgba_on_base(light, base_rgb);
    let byte = |channel: f32| (channel * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

/// How a role editor's slider reads: the role's opacity, or how see-through it is (Your messages
/// in the Chat colours group).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SliderReads {
    Opacity,
    SeeThrough,
}

impl ThemeTab {
    fn save_role(
        &mut self,
        light: bool,
        role: &'static str,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            let current = store.values().value(THEME_ROLES_SETTING);
            let next = with_role(&current, light, role, value);
            if next != current {
                store.update_setting(THEME_ROLES_SETTING, next, cx);
            }
        });
    }

    /// The Pick Color dialog of role `role` in one appearance, opened on its colour; its picks keep
    /// the role's opacity, or for a role not set the opacity of `default`, what it paints now.
    pub(super) fn open_role_picker(
        &mut self,
        role: &'static str,
        light: bool,
        hex: &str,
        default: Option<RoleValue>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = field_key("colour", role, light);
        open_tint_picker(self, key, hex, window, cx);
        let saver: ColorSaver = Rc::new(move |store, hex, commit, cx| {
            let Ok(RoleValue::Colour { rgb, .. }) = parse_role_value(hex) else {
                return;
            };
            save_role_in(
                store,
                light,
                role,
                commit,
                |current| {
                    Some(RoleValue::Colour {
                        rgb,
                        alpha: current.or(default).map_or(1.0, alpha_of),
                    })
                },
                cx,
            );
        });
        set_color_saver(self, key, saver);
    }

    /// One appearance's editor of role `role`: the swatch (with its Pick Color dialog while open),
    /// the text box and, while the role is set, its opacity slider and readout.
    #[allow(clippy::too_many_arguments)]
    fn role_editor_line(
        &mut self,
        p: &SettingsPalette,
        saved: &Value,
        base_rgb: u32,
        role: &'static str,
        light: bool,
        label: Option<&'static str>,
        reads: SliderReads,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let raw = saved
            .get(appearance_key(light))
            .and_then(|roles| roles.get(role))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let value = parse_role_value(&raw).ok();
        // What the role paints while not set, which the row shows (muted, with a dashed rim) so
        // the user adjusts it from there; a pick or a drag saves it as the role's own.
        let default = role_default(saved, role, light);
        let shown = value.or(default);
        let on_default = value.is_none() && default.is_some();
        let hex = shown.map_or_else(
            || UNSET_PICK.to_string(),
            |shown| hex_of(shown, light, base_rgb),
        );
        // The swatch, and its dialog while it is open.
        let colour_key = field_key("colour", role, light);
        let picker_open = self
            .fields
            .colors
            .get(colour_key)
            .is_some_and(|state| state.picker.open);
        let picker = if picker_open {
            trigger_tint_picker(self, p, colour_key, &hex, window, cx)
        } else {
            None
        };
        let rim = hsla(p.hairline);
        let dashed = hsla(p.foreground_alpha(0.4));
        let fill = shown.map(|shown| {
            let [r, g, b, a] = shown.rgba_on_base(light, base_rgb);
            gpui::Hsla::from(gpui::Rgba { r, g, b, a })
        });
        let pick_hex = hex.clone();
        let swatch = div()
            .id(SharedString::from(format!("{colour_key}-swatch")))
            .relative()
            .flex_shrink_0()
            .size(px(SWATCH))
            .rounded(px(6.0))
            .border_1()
            .cursor_pointer()
            .tooltip(tooltip_text(if on_default {
                "The theme's own colour. Pick a colour to change it"
            } else {
                "Pick a colour"
            }))
            .map(|swatch| match fill {
                // The surface under the colour, so a see-through one reads as see-through.
                Some(fill) => swatch
                    .when(on_default, |swatch| swatch.border_dashed())
                    .border_color(if on_default { dashed } else { rim })
                    .bg(hsla(p.surface))
                    .child(div().size_full().rounded(px(5.0)).bg(fill)),
                None => swatch.border_dashed().border_color(dashed),
            })
            .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| {
                page.open_role_picker(role, light, &pick_hex, default, window, cx)
            }))
            .children(picker);
        // The text box: any opacity or colour, saved once it reads as one; empty resets. Empty, it
        // shows the default in grey; the default is in the field's id because a field keeps the
        // placeholder it was made with.
        let placeholder = default.map_or_else(|| "default".to_string(), format_role_value);
        let id = SharedString::from(format!(
            "theme-role-{role}-{}-{placeholder}",
            appearance_key(light)
        ));
        let input = FieldStates::text_state(
            self,
            &id,
            &raw,
            Some(&placeholder),
            move |page: &mut Self, text, _window, cx| {
                let text = text.trim();
                if text.is_empty() {
                    page.save_role(light, role, None, cx);
                } else if let Ok(value) = parse_role_value(text) {
                    page.save_role(light, role, Some(format_role_value(value)), cx);
                }
            },
            window,
            cx,
        );
        let text_box = super::super::super::fields::settings_text_input(
            p,
            &input,
            Some(TEXT_BOX),
            true,
            window,
            cx,
        );
        // The opacity slider, at the role's opacity or its default's.
        let slider = shown.map(|value| {
            let alpha = alpha_of(value);
            let shown = match reads {
                SliderReads::Opacity => alpha,
                SliderReads::SeeThrough => 1.0 - alpha,
            };
            let percent = (shown * 100.0).round();
            let binding = SliderBinding {
                key: field_key("alpha", role, light),
                min: 0.0,
                max: 100.0,
                step: 1.0,
            };
            let saver: SliderSaver = Rc::new(move |store, percent, commit, cx| {
                let shown = (percent / 100.0).clamp(0.0, 1.0) as f32;
                let alpha = match reads {
                    SliderReads::Opacity => shown,
                    SliderReads::SeeThrough => 1.0 - shown,
                };
                save_role_in(
                    store,
                    light,
                    role,
                    commit,
                    |current| {
                        current.or(default).map(|current| match current {
                            RoleValue::Ink(_) => RoleValue::Ink(alpha),
                            RoleValue::Colour { rgb, .. } => RoleValue::Colour { rgb, alpha },
                            RoleValue::Base { step, .. } => RoleValue::Base { step, alpha },
                        })
                    },
                    cx,
                );
            });
            let track = step_slider(self, p, binding, f64::from(percent), saver, window, cx);
            // An opacity (8%) already reads in the text box; a colour's or base step's opacity does not.
            let readout = matches!(value, RoleValue::Colour { .. } | RoleValue::Base { .. }).then(|| {
                div()
                    .flex_shrink_0()
                    .w(px(READOUT))
                    .text_right()
                    .text_size(px(13.0))
                    .text_color(hsla(p.muted))
                    .child(format!("{percent}%"))
            });
            h_flex()
                .flex_1()
                .min_w_0()
                .gap(px(8.0))
                .items_center()
                .child(div().flex_1().min_w_0().child(track))
                .children(readout)
        });
        h_flex()
            .w(px(CONTROL_LANE_WIDTH))
            .max_w_full()
            .gap(px(8.0))
            .items_center()
            .when_some(label, |line, label| {
                line.child(
                    div()
                        .flex_shrink_0()
                        .w(px(34.0))
                        .text_size(px(12.0))
                        .text_color(hsla(p.muted))
                        .child(label),
                )
            })
            .child(swatch)
            .child(text_box)
            .when_some(slider, |line, slider| line.child(slider))
            .into_any_element()
    }

    /// Role `role`'s editor for every appearance the app shows, one line each, and the Reset that
    /// takes it off in all of them (`None` while it is set in none).
    pub(super) fn role_editor(
        &mut self,
        t: &ThemeCx,
        role: &'static str,
        reads: SliderReads,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (AnyElement, Option<PageAction<Self>>) {
        let p = t.p;
        let saved = t.values.value(THEME_ROLES_SETTING);
        let appearances = shown_appearances(&t.values.string("sidebarTheme"));
        let both = appearances.len() > 1;
        let mut lines = Vec::new();
        let mut set_here = Vec::new();
        for &(light, label) in &appearances {
            if saved
                .get(appearance_key(light))
                .and_then(|roles| roles.get(role))
                .is_some()
            {
                set_here.push(light);
            }
            let base_rgb = ghostex_settings_catalog::theme_colours::work_area_background_for_variant(
                t.values.map(),
                light,
            );
            lines.push(self.role_editor_line(
                &p,
                &saved,
                base_rgb,
                role,
                light,
                both.then_some(label),
                reads,
                window,
                cx,
            ));
        }
        let reset: Option<PageAction<Self>> = (!set_here.is_empty()).then(|| {
            Rc::new(
                move |page: &mut Self, _window: &mut Window, cx: &mut Context<Self>| {
                    for light in set_here.clone() {
                        page.save_role(light, role, None, cx);
                    }
                },
            ) as PageAction<Self>
        });
        (
            v_flex().gap(px(8.0)).children(lines).into_any_element(),
            reset,
        )
    }

    pub(super) fn theme_colours_section(
        &mut self,
        t: &ThemeCx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !t.visible(THEME_ROLES_SETTING) {
            return None;
        }
        let p = t.p;
        let saved = t.values.value(THEME_ROLES_SETTING);
        let open = self.more_shown(MoreGroup::ThemeColours, t.searching, &|key| t.visible(key));
        let set_under_more = more_roles_set(&saved);
        let mut rows = Vec::new();
        for (index, role) in own_roles().enumerate() {
            if index == FIRST_ROLES && !t.searching {
                let more_open = self.more_open[MoreGroup::ThemeColours.index()];
                let summary = if set_under_more == 0 {
                    SharedString::from(
                        "Borders, dividers, text, links, controls, menus, rows, status colours",
                    )
                } else if set_under_more == 1 {
                    SharedString::from("1 of these is set")
                } else {
                    SharedString::from(format!("{set_under_more} of these are set"))
                };
                rows.push(more_options_button(
                    &p,
                    "theme-more-theme-colours",
                    "More theme colours",
                    summary,
                    open,
                    move |page, _window, cx| {
                        page.set_more_open(MoreGroup::ThemeColours, !more_open, cx)
                    },
                    cx,
                ));
            }
            if index >= FIRST_ROLES && !open {
                continue;
            }
            let (control, reset) = self.role_editor(t, role.id, SliderReads::Opacity, window, cx);
            rows.push(setting_row(
                &p,
                format!("theme-role-{}", role.id),
                RowSpec::new(role.title)
                    .description(role.about)
                    .modified(reset.is_some())
                    .marker_after_label(),
                reset,
                control,
                cx,
            ));
        }
        super::super::super::fields::settings_section(
            &p,
            "Fine-tune colours",
            Some(SharedString::from(
                "Pick a colour, or drag its opacity so the glass shows through. A value can also be an opacity of the text colour (4%), a step off the window colour (base+4% or base+4%/80%) or a colour (#7dd3fc). Empty keeps the theme's own; your messages, the chat box, cards and the accent are in Chat colours.",
            )),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }
}
