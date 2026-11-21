//! The Extensions page with a remote machine picked (../machine_picker.rs): that computer's own
//! built-in switches (Main Hub, GBots, CLIProxyAPI) from its gxserver's `/api/machineExtensions`,
//! as the same cards the Built-in tab draws, each flipped through that gxserver. The rest of the
//! page (views, the Store, Your views) belongs to this computer's app and is not shown meanwhile.
//!
//! CDXC:Extensions 2026-11-21 DECISION: the switches, who may flip them and why are in the settings
//! catalog's built_in_extensions.rs (`COMPUTER_SWITCHES`) and server/src/machine_extensions.rs.
use super::super::super::super::native_modal_kit::hsla;
use super::super::super::fields::{ButtonVariant, settings_button, small_switch_control};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::store_machine_gxserver_rpc;
use super::ExtensionsTab;
use super::cards::{
    EXTENSION_LIST_COLUMNS, GridCardSpec, GridCell, card_grid, grid_card, tabler_tile,
};
use super::data::{official_extensions, official_icon};
use super::sections::{error_banner, plain_section};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, div, px,
};
use gpui_component::v_flex;
use serde_json::{Value, json};
use std::time::Duration;

const ENDPOINT: &str = "/api/machineExtensions";
/// A change waits for that computer's app to save it.
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(crate) struct RemoteExtensions {
    /// The machine's last `status` answer.
    pub(super) status: Option<Value>,
    error: Option<String>,
    loading: bool,
    /// The switch whose change is in flight.
    busy: Option<String>,
}

/// One switch of the machine's answer: its id, whether it is on, and the switch it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteSwitch {
    pub(crate) id: String,
    pub(crate) on: bool,
    pub(crate) requires: Option<String>,
}

/// The switches of a `status` answer, in its order.
pub(crate) fn remote_switches(status: Option<&Value>) -> Vec<RemoteSwitch> {
    status
        .and_then(|status| status.get("switches"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(RemoteSwitch {
                id: entry.get("id")?.as_str()?.to_string(),
                on: entry.get("on").and_then(Value::as_bool) == Some(true),
                requires: entry
                    .get("requires")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

/// The switches shown: one whose required switch is off is hidden (CDXC:Settings 2026-11-14 in
/// sections.rs), and its required switch's card says it unlocks it.
pub(crate) fn shown_switches(switches: &[RemoteSwitch]) -> Vec<&RemoteSwitch> {
    switches
        .iter()
        .filter(|switch| {
            switch.requires.as_ref().is_none_or(|required| {
                switches
                    .iter()
                    .any(|other| &other.id == required && other.on)
            })
        })
        .collect()
}

impl ExtensionsTab {
    /// Follows the machine picker: another machine drops the last one's answer and reads the new
    /// machine's switches.
    pub(super) fn follow_machine(&mut self, cx: &mut Context<Self>) {
        let choice = super::super::machine_picker::picked_machine_choice(&self.store, cx);
        let machine = choice.as_ref().and_then(|choice| choice.machine_id.clone());
        self.shown_machine_name = choice.map(|choice| choice.name);
        if machine == self.shown_machine {
            return;
        }
        self.shown_machine = machine;
        self.remote = RemoteExtensions::default();
        self.load_remote_extensions(cx);
    }

    fn shown_machine_label(&self) -> String {
        self.shown_machine_name
            .clone()
            .unwrap_or_else(|| "the remote machine".to_string())
    }

    fn remote_request(
        &mut self,
        params: Value,
        on_done: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let machine = self.shown_machine.clone();
        if machine.is_none() {
            return;
        }
        self.remote.loading = true;
        cx.notify();
        let this = cx.weak_entity();
        store_machine_gxserver_rpc(
            &self.store.clone(),
            machine.clone().as_deref(),
            ENDPOINT,
            params,
            TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if page.shown_machine != machine {
                        return;
                    }
                    page.remote.loading = false;
                    match result {
                        Ok(status) => {
                            page.remote.status = Some(status);
                            page.remote.error = None;
                        }
                        Err(error) => page.remote.error = Some(error),
                    }
                    on_done(page, cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    pub(super) fn load_remote_extensions(&mut self, cx: &mut Context<Self>) {
        self.remote_request(json!({ "action": "status" }), |_, _| {}, cx);
    }

    fn set_remote_switch(&mut self, id: String, title: String, on: bool, cx: &mut Context<Self>) {
        self.remote.busy = Some(id.clone());
        let machine = self.shown_machine_label();
        self.remote_request(
            json!({ "action": "set", "id": id, "on": on }),
            move |page, cx| {
                page.remote.busy = None;
                // A refusal keeps the last answer and says why in a toast; the banner is for a
                // machine that can't be read at all.
                if let Some(error) = page.remote.error.take() {
                    let verb = if on { "on" } else { "off" };
                    page.store.update(cx, |store, cx| {
                        store.toast(
                            "warning",
                            &format!("{title} not turned {verb} on {machine}"),
                            &error,
                            cx,
                        )
                    });
                }
            },
            cx,
        );
    }

    /// The machine's switches: an error with Try again, the cards, or what is loading.
    pub(super) fn render_remote_section(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let machine = self.shown_machine_label();
        let switches = remote_switches(self.remote.status.as_ref());
        let loading = self.remote.loading;
        let mut body: Vec<AnyElement> = Vec::new();
        if let Some(error) = self.remote.error.clone() {
            body.push(error_banner(p, error));
            body.push(settings_button(
                p,
                "remote-extensions-retry",
                "Try again",
                Some("modals/settings/refresh.svg"),
                ButtonVariant::Outline,
                loading,
                None,
                |page: &mut Self, _window, cx| page.load_remote_extensions(cx),
                cx,
            ));
        } else if self.remote.status.is_none() {
            body.push(
                div()
                    .text_color(hsla(p.muted))
                    .child(format!("Reading {machine}'s extensions…"))
                    .into_any_element(),
            );
        } else {
            let mut cells: Vec<GridCell> = Vec::new();
            for switch in shown_switches(&switches) {
                if let Some(cell) = self.remote_cell(p, switch, &switches, cx) {
                    cells.push(cell);
                }
            }
            body.push(card_grid(cells, None, EXTENSION_LIST_COLUMNS));
        }
        plain_section(
            p,
            format!("Built-in on {machine}"),
            div()
                .child(format!(
                    "Features Ghostex on {machine} runs itself. Turning one on here turns it on there; this computer's own switches stay as they are."
                ))
                .into_any_element(),
            None,
            v_flex().w_full().gap(px(12.0)).children(body).into_any_element(),
        )
    }

    /// One switch's card, drawn like its Built-in card; `None` for an id this app doesn't know.
    fn remote_cell(
        &mut self,
        p: &SettingsPalette,
        switch: &RemoteSwitch,
        switches: &[RemoteSwitch],
        cx: &mut Context<Self>,
    ) -> Option<GridCell> {
        let extension = official_extensions()
            .iter()
            .find(|extension| extension.id == switch.id)?
            .clone();
        let busy = self.remote.busy.is_some() || self.remote.loading;
        let id = switch.id.clone();
        let title = extension.title.clone();
        let control = small_switch_control(
            p,
            SharedString::from(format!("remote-official-{}-switch", extension.id)),
            title.clone(),
            switch.on,
            busy,
            busy.then(|| SharedString::from("Saving on the remote machine…")),
            move |page: &mut Self, next, _window, cx| {
                page.set_remote_switch(id.clone(), title.clone(), next, cx)
            },
            cx,
        );
        let (control, toggle) = control.into_row_parts();
        // While it is off, a card others need names them first (their cards hide until it is on).
        let unlocks: Vec<&str> = switches
            .iter()
            .filter(|other| other.requires.as_deref() == Some(switch.id.as_str()))
            .filter_map(|other| {
                official_extensions()
                    .iter()
                    .find(|extension| extension.id == other.id)
                    .map(|extension| extension.title.as_str())
            })
            .collect();
        let description = if !switch.on && !unlocks.is_empty() {
            format!(
                "Needed for {}. {}",
                unlocks.join(" and "),
                extension.description
            )
        } else {
            extension.description.clone()
        };
        let card = grid_card(
            p,
            GridCardSpec {
                id: SharedString::from(format!("remote-official-{}", extension.id)),
                icon: tabler_tile(p, official_icon(&extension.id), false),
                control: Some(control.into_any_element()),
                leading: None,
                title: extension.title.clone().into(),
                badge: extension.badge.clone().map(Into::into),
                description: description.into(),
                extra: None,
                scope_summary: None,
                meta: SharedString::default(),
                status: None,
                actions: Vec::new(),
                editing: false,
                enabled: switch.on,
                dragging: false,
                source_url: None,
                open: None,
                toggle,
            },
        );
        Some(GridCell::card(card))
    }
}
