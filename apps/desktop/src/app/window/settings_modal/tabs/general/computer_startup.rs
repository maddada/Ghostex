//! When your computer starts, first on Power & Sleep: start the server at login, and under it the
//! two switches that only work while it is on (CDXC:ComputerStartup 2026-11-06 in
//! packages/settings-catalog/src/general/system.rs). gxserver adds and removes the login entry when
//! the first switch changes (server/src/computer_startup/).
use super::super::super::fields::{
    PageAction, setting_row, settings_section, settings_switch, toggle_field_with,
};
use super::super::super::page::PageBlock;
use super::super::super::palette::SettingsPalette;
use super::{GeneralCx, GeneralTab, save};
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div,
};
use serde_json::{Map, json};
use std::rc::Rc;

const SECTION: &str = "computerStartup";
const START_SERVER: &str = "startServerAtLogin";
const OPEN_APP: &str = "openAppAtLogin";
const RESUME_SESSIONS: &str = "resumeSessionsAfterRestart";
const NEEDS_SERVER: &str = "Turn on Start Ghostex's server when you log in first.";

pub(super) fn computer_startup_section(
    _page: &mut GeneralTab,
    g: &GeneralCx,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible(SECTION, g.power_visible) {
        return None;
    }
    let server_on = g.values.bool(START_SERVER);
    let mut rows: Vec<AnyElement> = Vec::new();
    if g.visible(SECTION, START_SERVER) {
        let spec = g.spec(
            START_SERVER,
            "Start Ghostex's server when you log in",
            "Runs Ghostex's server in the background from the moment you log in, so your other computers, your phone and coordinators keep working while the app is closed.",
        );
        // Resetting or turning off the server switch turns its two children off with it, so
        // turning it back on later starts from a plain server start.
        let reset: PageAction<GeneralTab> = Rc::new(|page: &mut GeneralTab, _window, cx| {
            let store = page.store.clone();
            store.update(cx, |store, cx| {
                for key in [START_SERVER, OPEN_APP, RESUME_SESSIONS] {
                    store.reset_setting(key, cx);
                }
            });
        });
        rows.push(toggle_field_with(
            &g.p,
            START_SERVER,
            spec,
            server_on,
            Some(reset),
            |page: &mut GeneralTab, next, _window, cx| {
                let mut patch = Map::new();
                patch.insert(START_SERVER.to_string(), json!(next));
                if !next {
                    patch.insert(OPEN_APP.to_string(), json!(false));
                    patch.insert(RESUME_SESSIONS.to_string(), json!(false));
                }
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    store.apply_patch(patch, "settings:control", cx)
                });
            },
            cx,
        ));
    }
    for (key, label, description) in [
        (
            OPEN_APP,
            "Also open the Ghostex app",
            "Opens the Ghostex window after the server starts at login.",
        ),
        (
            RESUME_SESSIONS,
            "Resume sessions after a restart or power loss",
            "Continues the agent sessions that were running when the computer shut down, restarted or lost power, each in its own chat. Sessions you closed, slept or that had already ended stay as they were. Works with or without the app opening.",
        ),
    ] {
        if !g.visible(SECTION, key) {
            continue;
        }
        let spec = g.spec(key, label, description).dependent();
        if server_on {
            rows.push(toggle_field_with(
                &g.p,
                key,
                spec,
                g.values.bool(key),
                Some(super::super::super::fields::reset_key::<GeneralTab>(key)),
                move |page: &mut GeneralTab, next, _window, cx| save(page, key, json!(next), cx),
                cx,
            ));
        } else {
            rows.push(locked_child_row(&g.p, key, spec, cx));
        }
    }
    settings_section(&g.p, "When your computer starts", None, None, rows)
        .map(|section| PageBlock::section(SECTION, section))
}

/// A child while the server switch is off: the whole row greyed once (label and switch alike),
/// shown off whatever is stored, and not clickable; hovering the switch says what to turn on.
fn locked_child_row(
    p: &SettingsPalette,
    key: &'static str,
    spec: super::super::super::fields::RowSpec,
    cx: &mut Context<GeneralTab>,
) -> AnyElement {
    let control = div()
        .id(ElementId::Name(SharedString::from(format!("{key}-switch"))))
        .flex_shrink_0()
        .tooltip(super::super::super::fields::tooltip_text(NEEDS_SERVER))
        .child(settings_switch(p, false, false))
        .into_any_element();
    div()
        .w_full()
        .opacity(0.5)
        .child(setting_row(p, key, spec, None, control, cx))
        .into_any_element()
}
