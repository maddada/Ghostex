//! The machine picker at the top of Settings > Agents, Integrations, Dictation and Updates: this
//! computer first, then the saved remote machines connected right now, with "Only connected machines shown"
//! under the list. Picking one makes those pages read and install through that machine's gxserver
//! (`store_machine_gxserver_rpc`); a picked machine that disconnects goes back to this computer.
//!
//! CDXC:RemoteMachines 2026-11-10 DECISION:
//! User (Q2108, then Q2112 "Yes, connected only everywhere"): "for machine picker, please don't show disconneted machines. just show text at the bottom saying 'Only connected shown'", later worded "Only connected machines shown". Every page's picker lists only connected machines, and a picked machine that is no longer connected falls back to this computer. Supersedes the 2026-10-26 picker that listed every saved machine with why it couldn't be picked. A connected machine that can't do a page's work (an older Ghostex, no agent there) is still listed, and the page says why.
//!
//! CDXC:RemoteMachines 2026-10-26 DECISION:
//! User: "we need to show a dropdown at the top on agents page showing all the machines in settings and allowing us to update different remote machines. also same for integrations page (all should go through gxserver on that remote machine basically)". One pick serves both pages and lasts until Settings closes; the picker shows only once a remote machine is saved.
//! SEE-ALSO: apps/desktop/src/app/remote_conn/settings_rpc.rs, server/src/machine_integrations.rs.
use super::super::catalog::SettingOption;
use super::super::fields::{
    RowSpec, SettingsPage, setting_row, settings_section, settings_select_with_footer,
};
use super::super::palette::SettingsPalette;
use super::super::store::{SettingsStore, post_store_message};
use gpui::{AnyElement, Context, Entity, IntoElement, Window};
use serde_json::{Value, json};

/// The select's value for this computer.
const THIS_COMPUTER: &str = "this-computer";
const PICKER_ID: &str = "settings-machine-picker";
const PICKER_WIDTH: f32 = 272.0;

/// One entry of the picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MachineChoice {
    /// `None` is this computer.
    pub(crate) machine_id: Option<String>,
    pub(crate) name: String,
    /// Why it can't be picked (not connected, turned off), shown after its name.
    pub(crate) unavailable: Option<String>,
}

/// The words for a machine that is not connected, from its wire state (`remoteMachineConnectStates`).
fn unavailable_reason(disabled: bool, state: Option<&str>) -> Option<String> {
    if disabled {
        return Some("Turned off".to_string());
    }
    match state {
        Some("connected") => None,
        Some("connecting" | "downloadingRemoteServerPackage") => Some("Connecting…".to_string()),
        Some("installApprovalRequired") => Some("Needs setup in Settings > Remote".to_string()),
        Some(
            "authFailed"
            | "sshFailed"
            | "tunnelFailed"
            | "tokenUnavailable"
            | "keychainFailed"
            | "installFailed"
            | "invalid"
            | "presentationSubscribeFailed"
            | "presentationStreamFailed",
        ) => Some("Can't connect".to_string()),
        Some("unsupported" | "unsupportedRemotePlatform") => Some("Not supported".to_string()),
        _ => Some("Not connected".to_string()),
    }
}

/// This computer, then each saved machine in its saved order.
pub(crate) fn machine_choices(machines: &Value, states: Option<&Value>) -> Vec<MachineChoice> {
    let mut choices = vec![MachineChoice {
        machine_id: None,
        name: "This computer".to_string(),
        unavailable: None,
    }];
    for machine in machines.as_array().into_iter().flatten() {
        let Some(id) = machine
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let name = machine
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .or_else(|| machine.get("sshHost").and_then(Value::as_str))
            .unwrap_or(id)
            .to_string();
        let disabled = machine.get("disabled").and_then(Value::as_bool) == Some(true);
        let state = states
            .and_then(|states| states.get("states"))
            .and_then(|states| states.get(id))
            .and_then(Value::as_str);
        choices.push(MachineChoice {
            machine_id: Some(id.to_string()),
            name,
            unavailable: unavailable_reason(disabled, state),
        });
    }
    choices
}

fn store_choices(store: &SettingsStore) -> Vec<MachineChoice> {
    machine_choices(
        &store.value("remoteMachines"),
        store.host_payload("remoteMachineConnectStates"),
    )
}

/// The machine the page shows, as a choice: `None` for this computer, or for a picked machine that
/// is no longer saved or no longer connected (the pick is dropped).
pub(crate) fn picked_machine_choice(
    store: &Entity<SettingsStore>,
    cx: &mut gpui::App,
) -> Option<MachineChoice> {
    let picked = store.read(cx).picked_machine()?.to_string();
    let choice = store_choices(store.read(cx))
        .into_iter()
        .find(|choice| choice.machine_id.as_deref() == Some(picked.as_str()));
    // Only once the states are known: before they arrive every machine reads as not connected.
    let known = store
        .read(cx)
        .host_payload("remoteMachineConnectStates")
        .is_some();
    let gone = choice.is_none()
        || (known
            && choice
                .as_ref()
                .is_some_and(|choice| choice.unavailable.is_some()));
    if gone {
        store.update(cx, |store, cx| store.set_picked_machine(None, cx));
        return None;
    }
    choice
}

/// Asks the app for the machines' connection states (the page just opened).
pub(crate) fn request_machine_states(store: &Entity<SettingsStore>, cx: &mut gpui::App) {
    let has_machines = store_choices(store.read(cx)).len() > 1;
    if has_machines {
        post_store_message(
            store,
            json!({ "type": "requestRemoteMachineConnectStates" }),
            cx,
        );
    }
}

/// The picker's card, or `None` while no remote machine is saved.
pub(crate) fn render_machine_picker<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    store: &Entity<SettingsStore>,
    what: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    render_machine_picker_described(
        page,
        p,
        store,
        |shown| match shown {
            Some(name) => format!(
                "Showing the {what} on {name}. Installs and updates run there, through its Ghostex."
            ),
            None => format!("Pick a connected remote machine to see and update its {what}."),
        },
        window,
        cx,
    )
}

/// [`render_machine_picker`] with the page's own sentence under it: `describe` gets the shown
/// machine's name, `None` for this computer.
pub(crate) fn render_machine_picker_described<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    store: &Entity<SettingsStore>,
    describe: impl Fn(Option<&str>) -> String,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    render_machine_picker_with(page, p, store, describe, None, Vec::new(), window, cx)
}

/// [`render_machine_picker_described`] with `actions` beside the "Machine" heading and `rows`
/// under the picker in its card (Settings > Updates: Update all machines and how each went).
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_machine_picker_with<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    store: &Entity<SettingsStore>,
    describe: impl Fn(Option<&str>) -> String,
    actions: Option<AnyElement>,
    rows: Vec<AnyElement>,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    picker_card(page, p, store, describe, actions, rows, window, cx)
}

#[allow(clippy::too_many_arguments)]
fn picker_card<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    store: &Entity<SettingsStore>,
    describe: impl Fn(Option<&str>) -> String,
    actions: Option<AnyElement>,
    rows: Vec<AnyElement>,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    let choices = store_choices(store.read(cx));
    if choices.len() < 2 {
        return None;
    }
    let picked = store.read(cx).picked_machine().map(str::to_string);
    // Until the states arrive every machine reads as not connected: the pick stays named meanwhile.
    let known = store
        .read(cx)
        .host_payload("remoteMachineConnectStates")
        .is_some();
    let choices: Vec<MachineChoice> = choices
        .into_iter()
        .filter(|choice| choice.unavailable.is_none() || (!known && choice.machine_id == picked))
        .collect();
    // An `open_select` deep link naming the picker opens it (the preview binary's open state).
    let open_now = store.update(cx, |store, _| {
        let request = store.request_mut();
        if request.open_select.as_deref() == Some(PICKER_ID) {
            request.open_select = None;
            true
        } else {
            false
        }
    });
    if open_now {
        page.field_states().pending_open_select = Some(PICKER_ID.into());
    }
    let value = picked.clone().unwrap_or_else(|| THIS_COMPUTER.to_string());
    let options: Vec<SettingOption> = choices
        .iter()
        .map(|choice| SettingOption {
            label: choice.name.clone(),
            value: choice
                .machine_id
                .clone()
                .unwrap_or_else(|| THIS_COMPUTER.to_string()),
        })
        .collect();
    let store_for_change = store.clone();
    let on_change =
        move |_page: &mut V, value: String, _window: &mut Window, cx: &mut Context<V>| {
            let machine = (value != THIS_COMPUTER).then_some(value);
            store_for_change.update(cx, |store, cx| store.set_picked_machine(machine, cx));
        };
    let select = settings_select_with_footer(
        page,
        p,
        PICKER_ID,
        &options,
        "Only connected machines shown".into(),
        &value,
        Some(PICKER_WIDTH),
        on_change,
        window,
        cx,
    );
    let shown = choices
        .iter()
        .find(|choice| choice.machine_id == picked)
        .map(|choice| choice.name.clone())
        .unwrap_or_else(|| "This computer".to_string());
    let description = describe(picked.is_some().then_some(shown.as_str()));
    let row = setting_row::<V>(
        p,
        "settings-machine-picker-row",
        RowSpec {
            label: "Machine".into(),
            subtitle: Some(description.into()),
            ..RowSpec::default()
        },
        None,
        select,
        cx,
    );
    let mut all_rows = vec![row];
    all_rows.extend(rows);
    settings_section(p, "Machine", None, actions, all_rows).map(IntoElement::into_any_element)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_computer_comes_first_then_each_machine_with_its_state() {
        let machines = json!([
            { "id": "remote-a", "name": "Kubuntu VM", "sshHost": "10.0.0.2" },
            { "id": "remote-b", "name": "", "sshHost": "10.0.0.3", "disabled": true },
            { "id": "remote-c", "name": "M5" },
            { "name": "no id" },
        ]);
        let states = json!({ "type": "remoteMachineConnectStates", "states": { "remote-a": "connected", "remote-c": "sshFailed" } });
        let choices = machine_choices(&machines, Some(&states));
        let summary: Vec<(Option<&str>, &str, Option<&str>)> = choices
            .iter()
            .map(|choice| {
                (
                    choice.machine_id.as_deref(),
                    choice.name.as_str(),
                    choice.unavailable.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (None, "This computer", None),
                (Some("remote-a"), "Kubuntu VM", None),
                (Some("remote-b"), "10.0.0.3", Some("Turned off")),
                (Some("remote-c"), "M5", Some("Can't connect")),
            ]
        );
    }

    #[test]
    fn a_machine_without_a_known_state_is_not_connected() {
        let choices = machine_choices(&json!([{ "id": "remote-a", "name": "A" }]), None);
        assert_eq!(choices[1].unavailable.as_deref(), Some("Not connected"));
        let choices = machine_choices(
            &json!([{ "id": "remote-a", "name": "A" }]),
            Some(&json!({ "states": { "remote-a": "connecting" } })),
        );
        assert_eq!(choices[1].unavailable.as_deref(), Some("Connecting…"));
    }
}
