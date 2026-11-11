//! The Commands pane rows of Settings > Terminal: its height on double-click, its side, and
//! Auto-minimize.
//!
//! CDXC:Settings 2026-11-11 DECISION:
//! User: "move settings around so things make full sense"; Q2511 ("Move all five"): the Commands pane rows moved here from Sidebar & Sessions, because the pane is a terminal under or beside the work area, not part of the sidebar. Each row keeps its name, its key and its place in the Advanced fold (Commands pane side is an everyday row).
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{PageAction, select_field};
use super::{GeneralCx, GeneralTab};
use gpui::{AnyElement, Context, Window};
use serde_json::{Map, json};
use std::rc::Rc;

pub(super) fn commands_pane_rows(
    page: &mut GeneralTab,
    g: &GeneralCx,
    s: &str,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<AnyElement> {
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.slider(
        g,
        s,
        "commandsPanelDefaultHeightPx",
        "Commands pane height on double-click",
        "Used when opening the command pane (F12 or sidebar) and when double-clicking its top resize rail.",
        (
            g.number("MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX"),
            g.number("MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX"),
            1.0,
        ),
        false,
        window,
        cx,
    ));
    rows.extend(page.select(
        g,
        s,
        "commandsPanelSide",
        "Commands pane side",
        "Where terminal Actions and F12 open the command pane: below the workspace or as a column to its right.",
        g.options("COMMANDS_PANEL_SIDE_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q21): Auto-minimize Commands pane and Minimize
    // after are one select: Off, or how long the pane stays open after focus and the pointer leave.
    if g.any_visible(
        s,
        &[
            "commandsPanelAutoMinimize",
            "commandsPanelAutoMinimizeDelaySeconds",
        ],
    ) {
        let delay = g
            .values
            .number_string("commandsPanelAutoMinimizeDelaySeconds");
        let mut options = vec![SettingOption {
            label: "Off".to_string(),
            value: "off".to_string(),
        }];
        options.extend(g.options("COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS"));
        if !options.iter().any(|option| option.value == delay) {
            options.insert(
                1,
                SettingOption {
                    label: format!("After {delay} seconds"),
                    value: delay.clone(),
                },
            );
        }
        let value = if g.values.bool("commandsPanelAutoMinimize") {
            delay
        } else {
            "off".to_string()
        };
        let spec = g
            .spec(
                "commandsPanelAutoMinimize",
                "Auto-minimize Commands pane",
                "Minimize the Commands pane this long after you move focus elsewhere and the pointer leaves it. Commands keep running. Off keeps the pane open.",
            )
            .modified(
                g.values.is_modified("commandsPanelAutoMinimize")
                    || g.values.is_modified("commandsPanelAutoMinimizeDelaySeconds"),
            );
        let reset: PageAction<GeneralTab> = Rc::new(|page: &mut GeneralTab, _window, cx| {
            let store = page.store.clone();
            store.update(cx, |store, cx| {
                store.reset_setting("commandsPanelAutoMinimize", cx);
                store.reset_setting("commandsPanelAutoMinimizeDelaySeconds", cx);
            });
        });
        rows.push(select_field(
            page,
            &g.p,
            "commandsPanelAutoMinimize",
            spec,
            Some(reset),
            &options,
            &value,
            None,
            |page: &mut GeneralTab, next, _window, cx| {
                let mut patch = Map::new();
                if next == "off" {
                    patch.insert("commandsPanelAutoMinimize".to_string(), json!(false));
                } else if let Ok(seconds) = next.parse::<f64>() {
                    patch.insert("commandsPanelAutoMinimize".to_string(), json!(true));
                    patch.insert(
                        "commandsPanelAutoMinimizeDelaySeconds".to_string(),
                        json!(seconds.round() as i64),
                    );
                }
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    store.apply_patch(patch, "settings:control", cx)
                });
            },
            window,
            cx,
        ));
    }
    rows
}
