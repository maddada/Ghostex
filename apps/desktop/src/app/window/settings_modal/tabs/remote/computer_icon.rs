//! A computer's icon picker, under This computer and in a saved computer's dialog: a grid of the
//! icons a computer can wear (gx-core computer_icons.rs), the first tile going back to the one its
//! kind gives it. A pick is saved on that computer, by its gxserver, through the app
//! (gx_store/computer_icons.rs), so every device that reaches it shows the same icon.
use super::super::super::super::native_modal_kit::*;
use super::RemoteTab;
use super::style::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// Every computer's pick as the app last sent it (`computerIcons`).
#[derive(Default)]
pub(crate) struct ComputerIcons {
    /// The picked icon by machine id (`local` for this one); `None` for one that uses its default.
    picked: HashMap<String, Option<String>>,
    /// The drawing this computer wears unpicked.
    local_default: Option<String>,
    /// The saved computers connected now, whose icon can be changed from here.
    connected: HashSet<String>,
}

impl ComputerIcons {
    /// The icon the app last said was picked for that computer.
    pub(super) fn picked(&self, machine_id: &str) -> Option<&str> {
        self.picked.get(machine_id)?.as_deref()
    }
}

impl RemoteTab {
    /// `computerIcons` from the app.
    pub(super) fn receive_computer_icons(&mut self, message: &Value, cx: &mut Context<Self>) {
        let icons = &mut self.computer_icons;
        icons.picked = message
            .get("picked")
            .and_then(Value::as_object)
            .map(|picked| {
                picked
                    .iter()
                    .map(|(id, icon)| (id.clone(), icon.as_str().map(str::to_string)))
                    .collect()
            })
            .unwrap_or_default();
        icons.local_default = message
            .get("localDefaultAsset")
            .and_then(Value::as_str)
            .map(str::to_string);
        icons.connected = message
            .get("connected")
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        cx.notify();
    }

    /// Asks the app for every computer's pick (the page became visible).
    pub(super) fn request_computer_icons(&self, cx: &mut gpui::App) {
        self.post(json!({ "type": "computerIcon", "op": "read" }), cx);
    }

    fn pick_computer_icon(&mut self, machine_id: &str, icon: Option<&str>, cx: &mut Context<Self>) {
        self.computer_icons
            .picked
            .insert(machine_id.to_string(), icon.map(str::to_string));
        self.post(
            json!({
                "type": "computerIcon",
                "op": "set",
                "machineId": machine_id,
                "icon": icon.unwrap_or_default(),
            }),
            cx,
        );
        cx.notify();
    }
}

/// The drawing a saved computer wears unpicked, as far as Settings can tell: its saved system
/// mark is the app's to know, so the plain desktop stands in.
const REMOTE_DEFAULT_ASSET: &str = "titlebar/device-desktop.svg";

/// The icon grid for `machine_id` (`local` for this computer). A saved computer that is not
/// connected shows its grid faded with a line saying why.
pub(super) fn computer_icon_picker(
    tab: &RemoteTab,
    t: &RemoteTokens,
    machine_id: &str,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let local = machine_id == ghostex_gx_core::LOCAL_MACHINE_ID;
    let enabled = local || tab.computer_icons.connected.contains(machine_id);
    let picked = tab
        .computer_icons
        .picked
        .get(machine_id)
        .cloned()
        .flatten()
        .or_else(|| {
            // A saved computer's icon as last read there, until the app sends a fresher one.
            (!local)
                .then(|| {
                    tab.remote_machines(cx)
                        .iter()
                        .find(|machine| {
                            machine.get("id").and_then(Value::as_str) == Some(machine_id)
                        })
                        .and_then(|machine| machine.get("icon"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .flatten()
        });
    let default_asset: SharedString = if local {
        tab.computer_icons
            .local_default
            .clone()
            .unwrap_or_else(|| REMOTE_DEFAULT_ASSET.to_string())
            .into()
    } else {
        REMOTE_DEFAULT_ASSET.into()
    };
    let tile = |key: String,
                label: &'static str,
                asset: SharedString,
                icon: Option<&'static str>,
                selected: bool,
                cx: &mut Context<RemoteTab>| {
        let machine = machine_id.to_string();
        div()
            .id(SharedString::from(format!(
                "computer-icon-{machine_id}-{key}"
            )))
            .size(px(32.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .border_1()
            .border_color(hsla(if selected { t.p.primary } else { t.edge(0.92) }))
            .when(selected, |tile| tile.bg(hsla(t.card(0.9))))
            .when(enabled, |tile| {
                tile.cursor_pointer()
                    .hover(move |tile| tile.bg(hsla(t.card(0.6))))
                    .on_click(cx.listener(move |tab, _: &ClickEvent, _window, cx| {
                        tab.pick_computer_icon(&machine, icon, cx);
                    }))
            })
            .tooltip(super::super::super::fields::tooltip_text(label))
            .child(
                svg()
                    .path(asset)
                    .size(px(16.0))
                    .text_color(hsla(if selected { t.foreground } else { t.muted })),
            )
            .into_any_element()
    };
    let mut tiles = vec![tile(
        "default".to_string(),
        "Default",
        default_asset,
        None,
        picked.is_none(),
        cx,
    )];
    for icon in ghostex_gx_core::computer_icons::COMPUTER_ICONS.iter() {
        tiles.push(tile(
            icon.id.to_string(),
            icon.label,
            icon.asset.into(),
            Some(icon.id),
            picked.as_deref() == Some(icon.id),
            cx,
        ));
    }
    v_flex()
        .w_full()
        .min_w_0()
        .gap(px(6.0))
        .child(
            h_flex()
                .w_full()
                .flex_wrap()
                .gap(px(6.0))
                .when(!enabled, |grid| grid.opacity(0.5))
                .children(tiles),
        )
        .when(!enabled, |picker| {
            picker.child(detail(
                t,
                "Connect to this computer to change its icon.",
                true,
            ))
        })
        .into_any_element()
}
