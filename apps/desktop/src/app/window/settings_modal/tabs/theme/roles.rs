//! The Theme colours section of the Interface page: the theme roles (`themeRoles`,
//! packages/settings-catalog/src/theme_roles.rs) one row each, with a field per appearance the app
//! shows and a Reset. Borders, Dividers, Text, Grey text and Accent show at once; the rest fold under
//! More theme colours.
//!
//! CDXC:Theming 2026-11-14 DECISION:
//! User (Q3195, agent theming stage B): "if the user should see the roles, a compact Theme colours section on the Interface page (dependent rows hidden per the AGENTS.md rule), with a Reset to default per role". A role's field for an appearance the app does not show is hidden: Dark mode only shows the dark field, Light mode only the light one, System both. A field takes an opacity (4%) or a colour (#7dd3fc, #7dd3fc80); it saves once what is typed reads as one, and an empty field puts the theme's own colour back.

use super::super::super::fields::{FieldStates, PageAction, RowSpec, setting_row};
use super::controls::more_options_button;
use super::{MoreGroup, ThemeCx, ThemeTab};
use ghostex_settings_catalog::theme_roles::{
    THEME_ROLES, THEME_ROLES_SETTING, appearance_key, format_role_value, parse_role_value,
};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    px,
};
use gpui_component::h_flex;
use serde_json::{Map, Value};
use std::rc::Rc;

/// The roles shown without opening More theme colours.
const FIRST_ROLES: usize = 5;

/// The appearances the app shows with this Appearance setting: `(light, label)`.
fn shown_appearances(appearance: &str) -> Vec<(bool, &'static str)> {
    match appearance {
        "plain-light" => vec![(true, "Light")],
        "system" => vec![(false, "Dark"), (true, "Light")],
        _ => vec![(false, "Dark")],
    }
}

/// `themeRoles` with `role` of one appearance set to `value`, or taken off for `None`.
fn with_role(current: &Value, light: bool, role: &str, value: Option<String>) -> Value {
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
        let appearances = shown_appearances(&t.values.string("sidebarTheme"));
        let both = appearances.len() > 1;
        let open = self.more_shown(MoreGroup::ThemeColours, t.searching, &|key| t.visible(key));
        let mut rows = Vec::new();
        for (index, role) in THEME_ROLES.iter().enumerate() {
            if index == FIRST_ROLES && !t.searching {
                let more_open = self.more_open[MoreGroup::ThemeColours.index()];
                rows.push(more_options_button(
                    &p,
                    "theme-more-theme-colours",
                    "More theme colours",
                    "Cards, controls, menus, rows, your messages, the chat box, status colours",
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
            let mut fields = Vec::new();
            let mut set_here = Vec::new();
            for &(light, label) in &appearances {
                let saved_value = saved
                    .get(appearance_key(light))
                    .and_then(|roles| roles.get(role.id))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !saved_value.is_empty() {
                    set_here.push(light);
                }
                let id =
                    SharedString::from(format!("theme-role-{}-{}", role.id, appearance_key(light)));
                let role_id = role.id;
                let input = FieldStates::text_state(
                    self,
                    &id,
                    &saved_value,
                    Some("default"),
                    move |page: &mut Self, text, _window, cx| {
                        let text = text.trim();
                        if text.is_empty() {
                            page.save_role(light, role_id, None, cx);
                        } else if let Ok(value) = parse_role_value(text) {
                            page.save_role(light, role_id, Some(format_role_value(value)), cx);
                        }
                    },
                    window,
                    cx,
                );
                fields.push(
                    h_flex()
                        .gap(px(6.0))
                        .items_center()
                        .when_label(both, label, &p)
                        .child(super::super::super::fields::settings_text_input(
                            &p,
                            &input,
                            Some(96.0),
                            true,
                            window,
                            cx,
                        ))
                        .into_any_element(),
                );
            }
            let modified = !set_here.is_empty();
            let reset: Option<PageAction<Self>> = modified.then(|| {
                let role_id = role.id;
                Rc::new(
                    move |page: &mut Self, _window: &mut Window, cx: &mut Context<Self>| {
                        for light in set_here.clone() {
                            page.save_role(light, role_id, None, cx);
                        }
                    },
                ) as PageAction<Self>
            });
            let control = h_flex().gap(px(10.0)).items_center().children(fields);
            rows.push(setting_row(
                &p,
                format!("theme-role-{}", role.id),
                RowSpec::new(role.title)
                    .description(role.about)
                    .modified(modified)
                    .marker_after_label(),
                reset,
                control.into_any_element(),
                cx,
            ));
        }
        super::super::super::fields::settings_section(
            &p,
            "Theme colours",
            Some(SharedString::from(
                "Each one an opacity of the text colour over what is under it (4%), which stays see-through on glass, or a colour (#7dd3fc). Empty keeps the theme's own.",
            )),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }
}

/// The small Dark or Light label before a field, shown only when both appearances are.
trait WhenLabel {
    fn when_label(
        self,
        show: bool,
        label: &'static str,
        p: &super::super::super::palette::SettingsPalette,
    ) -> Self;
}

impl WhenLabel for gpui::Div {
    fn when_label(
        self,
        show: bool,
        label: &'static str,
        p: &super::super::super::palette::SettingsPalette,
    ) -> Self {
        if !show {
            return self;
        }
        self.child(
            div()
                .text_size(px(12.0))
                .text_color(super::super::super::super::native_modal_kit::hsla(p.muted))
                .child(label),
        )
    }
}
