//! Remote machines: the compact tile grid (remote-machine-grid.tsx (deleted 2026-10-01)) and the Add a machine / edit
//! dialog with its fields (remote-machine-dialog.tsx (deleted 2026-10-01), remote-machine-fields.tsx (deleted 2026-10-01)).
//!
//! CDXC:RemotePairing 2026-09-03 DECISION:
//! User: "simplify the machines area here like we simplified it in the mobile app exactly. It has too much info here. No need to show all available machines and a card for creating a new machine. just make it show 4 compact cards with ability to hide a machine from sidebar (disable toggle). and when i click on one of the machines then show that machine's details as a pop up in settings so i can edit it. First compact card needs to be 'Add a machine'".
//! The grid mirrors the mobile MachineCard: icon tile, name, one `user@host` (or "Easy Connect") line, and a Show-in-sidebar switch where the phone has its chevron. The switch is a sibling of the tile's button, so flipping it never opens the editor; a hidden machine renders dimmed. Every saved machine is listed; "4" is the column count, not a cap.
//!
//! CDXC:RemotePairing 2026-09-03 DECISION:
//! User: clicking a machine card shows "that machine's details as a pop up in settings so i can edit it", with "a dark overlay on the settings main area and sidebar (fully)" that closes the pop-up when clicked.
//! The dialog owns the draft while open and writes Settings.remoteMachines only on Save. Passwords keep their one-shot keychain path: the save icon posts immediately, and Save flushes a password still sitting in the field (CDXC:RemoteMachines 2026-06-09: the settings JSON never retains the secret).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, full_value_tooltip, icon, is_long_value, settings_button_sized,
    settings_icon, settings_icon_button, short_version, tooltip_text,
};
use super::super::super::model::SettingsTabId;
use super::easy_connect::{path_card, path_head};
use super::model::BadgeTone;
use super::model::*;
use super::style::*;
use super::{HEADER_GAP, RemoteTab};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, ClickEvent, Context, Entity, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

/// The dialog's inputs, one per field.
struct DialogInputs {
    code: Entity<InputState>,
    name: Entity<InputState>,
    short_name: Entity<InputState>,
    host: Entity<InputState>,
    user: Entity<InputState>,
    password: Entity<InputState>,
    port: Entity<InputState>,
    identity: Entity<InputState>,
    wsl: Entity<InputState>,
}

/// Which draft field an input edits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DraftField {
    Code,
    Name,
    ShortName,
    Host,
    User,
    Password,
    Port,
    Identity,
    Wsl,
}

impl DraftField {
    /// The input's `maxLength`.
    fn max_length(self) -> usize {
        match self {
            Self::Code => 4000,
            Self::Name => 80,
            Self::ShortName => ghostex_gx_core::machine_short_names::MACHINE_SHORT_NAME_MAX,
            Self::Host => 200,
            Self::User => 120,
            Self::Password => 500,
            Self::Port => 5,
            Self::Identity => 500,
            Self::Wsl => 120,
        }
    }
}

/// The open Add a machine / edit dialog (`RemoteMachineDialogContent`).
pub(crate) struct MachineDialog {
    /// The saved machine being edited; `None` for Add a machine.
    machine_id: Option<String>,
    machine_name: String,
    draft: MachineDraft,
    advanced_open: bool,
    focus: FocusHandle,
    inputs: DialogInputs,
    /// Pairing this computer with the one behind the code (pairing v2, GitHub #495).
    pairing: Option<ComputerPairing>,
    /// Asks where that pairing stands every `COMPUTER_PAIRING_POLL`; dropped with the dialog.
    pairing_poll: Option<gpui::Task<()>>,
    /// A code was pasted in this dialog; a saved machine's old code (whose secret is long spent)
    /// never pairs again by itself.
    code_edited: bool,
    _subscriptions: Vec<Subscription>,
}

/*
CDXC:RemotePairing 2026-11-08 DECISION:
User (GitHub #495): "Computers connecting through Easy Connect must not need their key added by hand under Advanced: give them the same flow as phones." Adding (or saving) an Easy Connect machine with a code from the other computer's Show QR window first pairs this computer with it through this computer's gxserver (`/api/pairComputer`, server/src/remote_access/computer_pairing.rs): the dialog shows three words, the other computer shows the same three under Paired devices, and once the owner approves there with Touch ID, Windows Hello or the system password this computer's Easy Connect key is on its allow-list and the machine is saved. A code without its secret (an older Copy code) saves as before, for a computer whose key is already listed.
*/
#[derive(Clone, Debug, PartialEq)]
enum ComputerPairing {
    Starting,
    Waiting {
        pending_id: String,
        words: Vec<String>,
        computer: String,
    },
    Failed(String),
}

const COMPUTER_PAIRING_POLL: std::time::Duration = std::time::Duration::from_millis(1500);
/// `/api/pairComputer` opens a tunnel and waits for the other computer's answer.
const COMPUTER_PAIRING_START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
/// Approving can wait on the other computer's Windows administrator prompt.
const COMPUTER_PAIRING_STATUS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(190);

impl RemoteTab {
    /// Opens the dialog on `machine` (edit) or empty (Add a machine), replacing a dialog the last
    /// visit left open.
    pub(super) fn open_machine_dialog(
        &mut self,
        machine: Option<&Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.store.update(cx, |store, _| {
            store.open_forms_mut().discard(SettingsTabId::Remote)
        });
        self.kept_machine_form = None;
        let draft = match machine {
            Some(machine) => machine_draft_from_settings(machine),
            None => create_remote_machine_draft(),
        };
        self.show_machine_dialog(machine, draft, false, window, cx);
    }

    /// The dialog the last visit left open (`open_machine_form`), on its draft; an edited
    /// machine that was removed since is not reopened.
    pub(super) fn restore_machine_dialog(
        &mut self,
        form: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let machine = match form.get("machineId").and_then(Value::as_str) {
            Some(id) => {
                let Some(machine) = self
                    .remote_machines(cx)
                    .into_iter()
                    .find(|machine| machine.get("id").and_then(Value::as_str) == Some(id))
                else {
                    return;
                };
                Some(machine)
            }
            None => None,
        };
        let base = match &machine {
            Some(machine) => machine_draft_from_settings(machine),
            None => create_remote_machine_draft(),
        };
        let draft = machine_draft_from_form(form.get("draft").unwrap_or(&Value::Null), base);
        let advanced_open = form.get("advancedOpen").and_then(Value::as_bool) == Some(true);
        self.show_machine_dialog(machine.as_ref(), draft, advanced_open, window, cx);
    }

    /// The open dialog and its draft, without the password (CDXC:Settings 2026-10-16 in
    /// settings_modal/restore.rs).
    pub(super) fn open_machine_form(&self) -> Option<Value> {
        let dialog = self.machine_dialog.as_ref()?;
        Some(json!({
            "kind": "machine",
            "machineId": dialog.machine_id,
            "advancedOpen": dialog.advanced_open,
            "draft": machine_draft_to_form(&dialog.draft),
        }))
    }

    fn show_machine_dialog(
        &mut self,
        machine: Option<&Value>,
        draft: MachineDraft,
        advanced_open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let make = |field: DraftField,
                    value: &str,
                    placeholder: &str,
                    window: &mut Window,
                    cx: &mut Context<Self>| {
            let value = value.to_string();
            let placeholder = placeholder.to_string();
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(field == DraftField::Password)
                    .default_value(value)
            })
        };
        let inputs = DialogInputs {
            code: make(
                DraftField::Code,
                &draft.easy_connect_code,
                "Paste the code copied from the other computer",
                window,
                cx,
            ),
            name: make(DraftField::Name, &draft.name, "Machine one", window, cx),
            short_name: make(DraftField::ShortName, &draft.short_name, "WN", window, cx),
            host: make(DraftField::Host, &draft.ssh_host, "100.77.81.4", window, cx),
            user: make(
                DraftField::User,
                &draft.ssh_user,
                "machine username",
                window,
                cx,
            ),
            password: make(
                DraftField::Password,
                "",
                if draft.ssh_password_saved {
                    "Saved in Keychain"
                } else {
                    "SSH password"
                },
                window,
                cx,
            ),
            port: make(DraftField::Port, &draft.ssh_port, "22", window, cx),
            identity: make(
                DraftField::Identity,
                &draft.ssh_identity_file,
                "~/.ssh/id_ed25519",
                window,
                cx,
            ),
            wsl: make(
                DraftField::Wsl,
                &draft.wsl_distribution,
                "Ubuntu-24.04",
                window,
                cx,
            ),
        };
        let mut subscriptions = Vec::new();
        for (field, input) in [
            (DraftField::Code, &inputs.code),
            (DraftField::Name, &inputs.name),
            (DraftField::ShortName, &inputs.short_name),
            (DraftField::Host, &inputs.host),
            (DraftField::User, &inputs.user),
            (DraftField::Password, &inputs.password),
            (DraftField::Port, &inputs.port),
            (DraftField::Identity, &inputs.identity),
            (DraftField::Wsl, &inputs.wsl),
        ] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                move |tab: &mut Self, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        let text = input.read(cx).value().to_string();
                        tab.dialog_field_changed(field, text, input.clone(), window, cx);
                    }
                },
            ));
        }
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.machine_dialog = Some(MachineDialog {
            machine_id: machine.and_then(|machine| {
                machine
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
            machine_name: machine
                .and_then(|machine| machine.get("name").and_then(Value::as_str))
                .unwrap_or_default()
                .to_string(),
            draft,
            advanced_open,
            focus,
            inputs,
            pairing: None,
            pairing_poll: None,
            code_edited: false,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    fn close_machine_dialog(&mut self, cx: &mut Context<Self>) {
        // A computer still waiting for approval stops waiting (its request lapses there).
        if let Some(Some(ComputerPairing::Waiting { pending_id, .. })) = self
            .machine_dialog
            .as_ref()
            .map(|dialog| dialog.pairing.clone())
        {
            self.rpc(
                "/api/cancelComputerPairing",
                json!({ "pendingId": pending_id }),
                super::RPC_TIMEOUT,
                cx,
                |_, _, _| {},
            );
        }
        self.machine_dialog = None;
        cx.notify();
    }

    /// Starts pairing this computer with the one in the dialog's code (CDXC:RemotePairing
    /// 2026-11-08 above), saving the machine once that computer's owner approves.
    fn start_computer_pairing(&mut self, params: Value, cx: &mut Context<Self>) {
        let Some(dialog) = self.machine_dialog.as_mut() else {
            return;
        };
        dialog.pairing = Some(ComputerPairing::Starting);
        cx.notify();
        self.rpc(
            "/api/pairComputer",
            params,
            COMPUTER_PAIRING_START_TIMEOUT,
            cx,
            |this, result, cx| {
                let Some(dialog) = this.machine_dialog.as_mut() else {
                    return;
                };
                if dialog.pairing != Some(ComputerPairing::Starting) {
                    return;
                }
                let words: Option<Vec<String>> = result.as_ref().ok().and_then(|value| {
                    value.get("words")?.as_array().map(|words| {
                        words
                            .iter()
                            .filter_map(|word| word.as_str().map(str::to_string))
                            .collect()
                    })
                });
                let pending_id = result
                    .as_ref()
                    .ok()
                    .and_then(|value| value.get("pendingId")?.as_str().map(str::to_string));
                dialog.pairing = Some(match (result, pending_id, words) {
                    (Ok(value), Some(pending_id), Some(words)) => ComputerPairing::Waiting {
                        pending_id,
                        words,
                        computer: value
                            .get("computerName")
                            .and_then(Value::as_str)
                            .unwrap_or("the other computer")
                            .to_string(),
                    },
                    (Err(error), _, _) => ComputerPairing::Failed(error),
                    _ => ComputerPairing::Failed(
                        "The other computer sent an unreadable answer.".to_string(),
                    ),
                });
                if matches!(dialog.pairing, Some(ComputerPairing::Waiting { .. })) {
                    dialog.pairing_poll = Some(cx.spawn(async move |this, cx| {
                        loop {
                            cx.background_executor().timer(COMPUTER_PAIRING_POLL).await;
                            let Ok(keep_going) =
                                this.update(cx, |this, cx| this.poll_computer_pairing(cx))
                            else {
                                break;
                            };
                            if !keep_going {
                                break;
                            }
                        }
                    }));
                }
                cx.notify();
            },
        );
    }

    /// One ask; `false` once nothing is waiting any more.
    fn poll_computer_pairing(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(Some(ComputerPairing::Waiting { pending_id, .. })) = self
            .machine_dialog
            .as_ref()
            .map(|dialog| dialog.pairing.clone())
        else {
            return false;
        };
        if self.computer_pairing_in_flight {
            return true;
        }
        self.computer_pairing_in_flight = true;
        let asked = pending_id.clone();
        self.rpc(
            "/api/computerPairingStatus",
            json!({ "pendingId": pending_id }),
            COMPUTER_PAIRING_STATUS_TIMEOUT,
            cx,
            move |this, result, cx| {
                this.computer_pairing_in_flight = false;
                let Some(dialog) = this.machine_dialog.as_mut() else {
                    return;
                };
                let still_this = matches!(
                    &dialog.pairing,
                    Some(ComputerPairing::Waiting { pending_id, .. }) if *pending_id == asked
                );
                if !still_this {
                    return;
                }
                let status = result
                    .as_ref()
                    .ok()
                    .and_then(|value| value.get("status")?.as_str().map(str::to_string));
                match (status.as_deref(), result) {
                    (Some("pending"), _) => {}
                    (Some("approved"), _) => {
                        dialog.pairing = None;
                        dialog.pairing_poll = None;
                        this.store_dialog_machine(cx);
                    }
                    (Some("denied"), _) => {
                        dialog.pairing = Some(ComputerPairing::Failed(
                            "The other computer denied this one. Show a new code there and try again.".to_string(),
                        ));
                        dialog.pairing_poll = None;
                    }
                    (_, Err(error)) => {
                        dialog.pairing = Some(ComputerPairing::Failed(error));
                        dialog.pairing_poll = None;
                    }
                    _ => {
                        dialog.pairing = Some(ComputerPairing::Failed(
                            "Nobody approved this computer in time. Show a new code on the other computer and try again.".to_string(),
                        ));
                        dialog.pairing_poll = None;
                    }
                }
                cx.notify();
            },
        );
        true
    }

    /// One field's `onChange`: the draft follows it, clipped to its `maxLength` (and digits only
    /// for the port); a pasted Easy Connect code fills the name, user and port it carries.
    fn dialog_field_changed(
        &mut self,
        field: DraftField,
        text: String,
        input: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut next: String = if field == DraftField::Port {
            text.chars().filter(char::is_ascii_digit).collect()
        } else {
            text.clone()
        };
        if next.chars().count() > field.max_length() {
            next = next.chars().take(field.max_length()).collect();
        }
        if next != text {
            let value = next.clone();
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        let Some(dialog) = self.machine_dialog.as_mut() else {
            return;
        };
        match field {
            DraftField::Code => {
                dialog.code_edited = true;
                apply_easy_connect_code(&mut dialog.draft, &next);
                // The code brings the name, user and port with it.
                let draft = dialog.draft.clone();
                for (input, value) in [
                    (&dialog.inputs.name, draft.name.clone()),
                    (&dialog.inputs.user, draft.ssh_user.clone()),
                    (&dialog.inputs.port, draft.ssh_port.clone()),
                ] {
                    if input.read(cx).value().as_ref() != value {
                        input.update(cx, |input, cx| input.set_value(value, window, cx));
                    }
                }
            }
            DraftField::Name => dialog.draft.name = next,
            DraftField::ShortName => dialog.draft.short_name = next,
            DraftField::Host => dialog.draft.ssh_host = next,
            DraftField::User => dialog.draft.ssh_user = next,
            DraftField::Password => dialog.draft.ssh_password = next,
            DraftField::Port => dialog.draft.ssh_port = next,
            DraftField::Identity => dialog.draft.ssh_identity_file = next,
            DraftField::Wsl => dialog.draft.wsl_distribution = next,
        }
        cx.notify();
    }

    /// `setTransport`: switching modes keeps the shared fields and drops only the other mode's
    /// address.
    fn set_dialog_transport(
        &mut self,
        easy_connect: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.machine_dialog.as_mut() else {
            return;
        };
        dialog.draft.easy_connect = easy_connect;
        let cleared = if easy_connect {
            dialog.draft.ssh_host.clear();
            dialog.draft.ssh_port.clear();
            vec![&dialog.inputs.host, &dialog.inputs.port]
        } else {
            dialog.draft.easy_connect_code.clear();
            dialog.draft.easy_connect_address.clear();
            vec![&dialog.inputs.code]
        };
        for input in cleared {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.notify();
    }

    /// `savePassword`: the one-shot keychain save of the edit dialog's floppy button.
    fn save_dialog_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.machine_dialog.as_mut() else {
            return;
        };
        let Some(machine_id) = dialog.machine_id.clone() else {
            return;
        };
        let password = dialog.draft.ssh_password.clone();
        let was_saved = self
            .remote_machines(cx)
            .iter()
            .find(|machine| machine.get("id").and_then(Value::as_str) == Some(machine_id.as_str()))
            .is_some_and(|machine| {
                machine.get("sshPasswordSaved").and_then(Value::as_bool) == Some(true)
            });
        if password.is_empty() && !was_saved {
            return;
        }
        let Some(dialog) = self.machine_dialog.as_mut() else {
            return;
        };
        dialog.draft.ssh_password.clear();
        dialog.draft.ssh_password_saved = !password.trim().is_empty();
        let saved = dialog.draft.ssh_password_saved;
        let input = dialog.inputs.password.clone();
        input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(
                if saved {
                    "Saved in Keychain"
                } else {
                    "SSH password"
                },
                window,
                cx,
            );
        });
        self.post(
            json!({ "password": password, "remoteMachineId": machine_id, "type": "saveRemoteMachinePassword" }),
            cx,
        );
        cx.notify();
    }

    /// Save: an Easy Connect code from the other computer's Show QR window pairs first.
    fn save_dialog_machine(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.machine_dialog.as_ref() else {
            return;
        };
        if matches!(
            dialog.pairing,
            Some(ComputerPairing::Starting | ComputerPairing::Waiting { .. })
        ) {
            return;
        }
        if dialog.draft.easy_connect
            && dialog.code_edited
            && let Some(params) = computer_pairing_params(&dialog.draft.easy_connect_code)
        {
            self.start_computer_pairing(params, cx);
            return;
        }
        self.store_dialog_machine(cx);
    }

    /// `saveMachine`: replaces or appends the machine, then sends a password still in the field.
    fn store_dialog_machine(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.machine_dialog.as_ref() else {
            return;
        };
        let Some(machine) = normalize_remote_machine_draft(&dialog.draft) else {
            return;
        };
        let password = dialog.draft.ssh_password.clone();
        let id = machine
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut machines = self.remote_machines(cx);
        match machines
            .iter()
            .position(|candidate| candidate.get("id").and_then(Value::as_str) == Some(id.as_str()))
        {
            Some(index) => machines[index] = machine,
            None => machines.push(machine),
        }
        self.save_remote_machines(normalize_remote_machines(&Value::Array(machines)), cx);
        if !password.trim().is_empty() {
            self.post(
                json!({ "password": password, "remoteMachineId": id, "type": "saveRemoteMachinePassword" }),
                cx,
            );
        }
        self.close_machine_dialog(cx);
    }

    fn remove_machine(&mut self, machine_id: String, cx: &mut Context<Self>) {
        let machines: Vec<Value> = self
            .remote_machines(cx)
            .into_iter()
            .filter(|machine| {
                machine.get("id").and_then(Value::as_str) != Some(machine_id.as_str())
            })
            .collect();
        self.save_remote_machines(machines, cx);
        self.close_machine_dialog(cx);
    }

    /// `setMachineVisible`: the tile switch writes `disabled`.
    fn set_machine_visible(&mut self, machine_id: &str, visible: bool, cx: &mut Context<Self>) {
        let machines: Vec<Value> = self
            .remote_machines(cx)
            .into_iter()
            .map(|mut machine| {
                if machine.get("id").and_then(Value::as_str) == Some(machine_id) {
                    machine["disabled"] = json!(!visible);
                }
                machine
            })
            .collect();
        self.save_remote_machines(normalize_remote_machines(&Value::Array(machines)), cx);
    }
}

/// A dialog draft as remembered between visits: every field but the password, which only ever
/// goes to the keychain.
fn machine_draft_to_form(draft: &MachineDraft) -> Value {
    json!({
        "id": draft.id,
        "name": draft.name,
        "shortName": draft.short_name,
        "easyConnect": draft.easy_connect,
        "easyConnectCode": draft.easy_connect_code,
        "easyConnectAddress": draft.easy_connect_address,
        "sshHost": draft.ssh_host,
        "sshIdentityFile": draft.ssh_identity_file,
        "reuseSshConnection": draft.reuse_ssh_connection,
        "sshPort": draft.ssh_port,
        "sshUser": draft.ssh_user,
        "wslDistribution": draft.wsl_distribution,
        "disabled": draft.disabled,
        "agentAccessOff": draft.agent_access_off,
    })
}

/// `machine_draft_to_form` read back onto `base` (the edited machine's draft, or a new one); a
/// field missing from `form` keeps `base`'s value.
fn machine_draft_from_form(form: &Value, base: MachineDraft) -> MachineDraft {
    let text = |key: &str, fallback: String| {
        form.get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(fallback)
    };
    let flag =
        |key: &str, fallback: bool| form.get(key).and_then(Value::as_bool).unwrap_or(fallback);
    MachineDraft {
        id: form
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map_or_else(|| base.id.clone(), str::to_string),
        name: text("name", base.name.clone()),
        short_name: text("shortName", base.short_name.clone()),
        easy_connect: flag("easyConnect", base.easy_connect),
        easy_connect_code: text("easyConnectCode", base.easy_connect_code.clone()),
        easy_connect_address: text("easyConnectAddress", base.easy_connect_address.clone()),
        ssh_host: text("sshHost", base.ssh_host.clone()),
        ssh_identity_file: text("sshIdentityFile", base.ssh_identity_file.clone()),
        ssh_password: String::new(),
        ssh_password_saved: base.ssh_password_saved,
        reuse_ssh_connection: flag("reuseSshConnection", base.reuse_ssh_connection),
        ssh_port: text("sshPort", base.ssh_port.clone()),
        ssh_user: text("sshUser", base.ssh_user.clone()),
        wsl_distribution: text("wslDistribution", base.wsl_distribution.clone()),
        disabled: flag("disabled", base.disabled),
        agent_access_off: flag("agentAccessOff", base.agent_access_off),
    }
}

/// The preview's `remote-edit-machine-advanced` state: the open dialog with Advanced expanded.
pub(super) fn preview_open_dialog_advanced(tab: &mut RemoteTab) {
    if let Some(dialog) = tab.machine_dialog.as_mut() {
        dialog.advanced_open = true;
    }
}

/// The preview binary's `remote-add-ec` state: Easy Connect code mode with a code pasted.
pub(super) fn preview_paste_easy_connect_code(
    tab: &mut RemoteTab,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) {
    tab.set_dialog_transport(true, window, cx);
    let Some(input) = tab
        .machine_dialog
        .as_ref()
        .map(|dialog| dialog.inputs.code.clone())
    else {
        return;
    };
    let code = json!({
        "v": 1,
        "address": "tc1q8v3k2m9x7p4r6t8w1y5z2a4c6e8g0j3l5n7q9s1u3w5y7a9c1e3g5i7k9m1o3q5s7u9w1y3",
        "name": "Studio",
        "user": "madda",
        "port": 58744,
        "sshPort": 22,
    });
    let payload = {
        use base64::Engine as _;
        format!(
            "ghostex-ec1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_string(&code).unwrap_or_default())
        )
    };
    input.update(cx, |input, cx| input.set_value(payload.clone(), window, cx));
    tab.dialog_field_changed(DraftField::Code, payload, input, window, cx);
}

/// "Remote machines": the header and the tile grid. This computer's own short name is in the
/// Current machine card above the tabs (CDXC:RemotePairing 2026-11-08 in current_machine.rs).
pub(super) fn machines_section(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    window: &Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    // Three tiles to a row in a narrow Settings window, so a name and its status stay readable
    // (four truncated both to "Macbo…" and "Setu…"; the same width as the section tabs' switch).
    let per_row = if window.viewport_size().width < px(1060.0) {
        3
    } else {
        4
    };
    let machines = tab.remote_machines(cx);
    // Every computer's pill.
    let local_name = crate::host_name::local_computer_name();
    let mut settings = serde_json::Map::new();
    settings.insert("remoteMachines".to_string(), Value::Array(machines.clone()));
    settings.insert(
        "localMachineShortName".to_string(),
        tab.store.read(cx).value("localMachineShortName"),
    );
    let short_names =
        ghostex_gx_core::machine_short_names::short_names_by_machine_id(&local_name, &settings);
    // An edited machine that went away closes its dialog.
    if let Some(dialog) = tab.machine_dialog.as_ref()
        && let Some(id) = dialog.machine_id.as_deref()
        && !machines
            .iter()
            .any(|machine| machine.get("id").and_then(Value::as_str) == Some(id))
    {
        tab.machine_dialog = None;
    }
    let mut tiles = vec![add_tile(t, cx)];
    tiles.extend(
        machines
            .iter()
            .map(|machine| machine_tile(tab, t, machine, &short_names, cx)),
    );
    let mut rows = Vec::new();
    let mut tiles = tiles.into_iter().peekable();
    while tiles.peek().is_some() {
        let mut row = h_flex().w_full().gap(px(8.0)).items_stretch();
        let mut count = 0;
        for tile in tiles.by_ref().take(per_row) {
            row = row.child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .min_w_0()
                    .flex()
                    .child(tile),
            );
            count += 1;
        }
        for _ in count..per_row {
            row = row.child(div().flex_1().flex_basis(px(0.0)).min_w_0());
        }
        rows.push(row);
    }
    // The same path card as the Easy Connect and Tailscale tabs.
    let (connected, saved) = tab.connected_machine_count(cx);
    let status = if saved == 0 {
        badge(t, "None saved", BadgeTone::Plain, false)
    } else {
        badge(
            t,
            format!("{connected}/{saved} connected"),
            if connected > 0 {
                BadgeTone::Active
            } else {
                BadgeTone::Disabled
            },
            false,
        )
    };
    let head = path_head(
        t,
        "remote-machines-head",
        ICON_DEVICE_DESKTOP,
        connected > 0,
        "Remote machines",
        vec![status],
    );
    let body = v_flex()
        .w_full()
        .min_w_0()
        .gap(px(HEADER_GAP))
        .child(
            div()
                .w_full()
                .text_size(px(13.0))
                .line_height(px(19.5))
                .text_color(hsla(t.muted))
                .child("Other computers this one connects to, over SSH or with an Easy Connect code. Their projects show up as separate sidebar sections; the switch hides a machine from the sidebar without deleting it."),
        )
        .child(v_flex().w_full().gap(px(8.0)).children(rows))
        .into_any_element();
    path_card(
        t,
        "remote-machines-card",
        head,
        div().into_any_element(),
        Some(body),
    )
}

/// The 32px icon tile of a machine tile.
fn tile_icon(t: &RemoteTokens, path: &'static str) -> AnyElement {
    div()
        .flex_shrink_0()
        .size(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .border_1()
        .border_color(hsla(t.edge(0.76)))
        .bg(hsla(t.card(0.58)))
        .child(settings_icon(
            path,
            18.0,
            css_mix(t.muted, 0.70, t.foreground),
        ))
        .into_any_element()
}

fn tile_text(t: &RemoteTokens, title: String, detail_text: String) -> AnyElement {
    tile_text_with_dot(t, title, detail_text, false)
}

/// [`tile_text`] whose one line leads with a red dot: a failing machine's short status.
fn tile_text_with_dot(
    t: &RemoteTokens,
    title: String,
    detail_text: String,
    dot: bool,
) -> AnyElement {
    v_flex()
        .flex_1()
        .min_w_0()
        .gap(px(2.0))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(t.foreground))
                .child(title),
        )
        .child(
            h_flex()
                .min_w_0()
                .gap(px(6.0))
                .items_center()
                .when(dot, |line| line.child(status_dot(t)))
                .child(detail(t, detail_text, false)),
        )
        .into_any_element()
}

/// The 7px red dot of a failing machine, in its tile and at the head of its status panel.
fn status_dot(t: &RemoteTokens) -> AnyElement {
    div()
        .flex_shrink_0()
        .size(px(7.0))
        .rounded_full()
        .bg(hsla(t.p.destructive))
        .into_any_element()
}

fn add_tile(t: &RemoteTokens, cx: &mut Context<RemoteTab>) -> AnyElement {
    let hover = t.card(0.58);
    h_flex()
        .id("remote-add-machine-tile")
        .w_full()
        .min_w_0()
        .min_h(px(60.0))
        .px(px(12.0))
        .py(px(10.0))
        .gap(px(8.0))
        .items_center()
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_dashed()
        .border_color(hsla(t.edge(0.92)))
        .bg(hsla(t.card(0.38)))
        .cursor_pointer()
        .hover(move |this| this.bg(hsla(hover)))
        .on_click(cx.listener(|tab, _: &ClickEvent, window, cx| {
            tab.open_machine_dialog(None, window, cx);
        }))
        .child(tile_icon(t, icon::PLUS))
        .child(tile_text(
            t,
            "Add a machine".to_string(),
            "SSH details or an Easy Connect code".to_string(),
        ))
        .into_any_element()
}

fn machine_tile(
    tab: &RemoteTab,
    t: &RemoteTokens,
    machine: &Value,
    short_names: &std::collections::HashMap<String, String>,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let id = machine
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let name = machine
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // Its sidebar pill after its name (gx-core machine_short_names.rs).
    let name = match short_names.get(&id) {
        Some(short) => format!("{name} · {short}"),
        None => name,
    };
    let visible = machine.get("disabled").and_then(Value::as_bool) != Some(true);
    let status = machine_tile_status(
        tab.connect_states.get(&id).map(String::as_str),
        tab.this_computer.contains(&id),
    );
    let hover = t.card(0.58);
    let open_machine = machine.clone();
    let switch_id = id.clone();
    h_flex()
        .id(SharedString::from(format!("remote-machine-tile-{id}")))
        .w_full()
        .min_w_0()
        .min_h(px(60.0))
        .pr(px(10.0))
        .gap(px(8.0))
        .items_center()
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(t.edge(0.92)))
        .bg(hsla(t.card(0.38)))
        .hover(move |this| this.bg(hsla(hover)))
        .child(
            h_flex()
                .id(SharedString::from(format!("remote-machine-open-{id}")))
                .flex_1()
                .min_w_0()
                .pl(px(12.0))
                .py(px(10.0))
                .gap(px(10.0))
                .items_center()
                .cursor_pointer()
                .when(!visible, |this| this.opacity(0.55))
                .on_click(cx.listener(move |tab, _: &ClickEvent, window, cx| {
                    tab.open_machine_dialog(Some(&open_machine), window, cx);
                }))
                .child(tile_icon(t, ICON_DEVICE_DESKTOP))
                .child(tile_text_with_dot(
                    t,
                    name,
                    // A failing machine's short status, or that it is this computer, comes first;
                    // then a connected machine's Ghostex version (and its update) stands in for
                    // the SSH target. The reason is in the machine's sheet, never in the tile.
                    status
                        .as_ref()
                        .map(|status| status.text.to_string())
                        .or_else(|| super::app_update::tile_update_line(tab, &id))
                        .unwrap_or_else(|| format_remote_machine_ssh_target(machine)),
                    status.as_ref().is_some_and(|status| status.failed),
                )),
        )
        .children(super::app_update::tile_update_button(tab, t, &id, cx))
        .child(
            div()
                .id(SharedString::from(format!("remote-machine-switch-{id}")))
                .flex_shrink_0()
                .cursor_pointer()
                .on_click(cx.listener(move |tab, _: &ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    tab.set_machine_visible(&switch_id, !visible, cx);
                }))
                .child(small_switch(&t.p, visible)),
        )
        .into_any_element()
}

// ---- the dialog ------------------------------------------------------------------------------------

/// A labelled field (`Field` + `FieldLabel` + control + `FieldDescription`). `info` is its
/// explanation, shown by the (i) after the label; `message` is an error or a result of checking
/// the value, which stays under the field.
fn field(
    t: &RemoteTokens,
    colors: &DialogColors,
    label: &'static str,
    control: AnyElement,
    info: Option<&str>,
    message: Option<(String, Option<gpui::Rgba>)>,
) -> AnyElement {
    let group = SharedString::from(format!("remote-dialog-field-{label}"));
    v_flex()
        .group(group.clone())
        .w_full()
        .min_w_0()
        .gap(px(6.0))
        .child(dialog_label(
            t,
            label,
            info.map(|text| SharedString::from(text.to_string())),
            group,
        ))
        .child(control)
        .children(message.map(|(text, color)| {
            div()
                .w_full()
                .text_size(px(13.0))
                .line_height(px(18.85))
                .text_color(hsla(color.unwrap_or(colors.muted)))
                .child(text)
        }))
        .into_any_element()
}

pub(super) fn machine_dialog(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> Option<AnyElement> {
    let dialog = tab.machine_dialog.as_ref()?;
    let colors = dialog_colors(t);
    let p = t.p.nested_dialog();
    let is_new = dialog.machine_id.is_none();
    let draft = dialog.draft.clone();
    let advanced_open = dialog.advanced_open;
    let focus = dialog.focus.clone();
    let machine_id = dialog.machine_id.clone();
    let title = if is_new {
        "Add a machine".to_string()
    } else {
        dialog.machine_name.clone()
    };
    let inputs = (
        dialog.inputs.code.clone(),
        dialog.inputs.name.clone(),
        dialog.inputs.short_name.clone(),
        dialog.inputs.host.clone(),
        dialog.inputs.user.clone(),
        dialog.inputs.password.clone(),
        dialog.inputs.port.clone(),
        dialog.inputs.identity.clone(),
        dialog.inputs.wsl.clone(),
    );
    let (
        code_input,
        name_input,
        short_name_input,
        host_input,
        user_input,
        password_input,
        port_input,
        identity_input,
        wsl_input,
    ) = inputs;
    let input = |state: &Entity<InputState>, window: &Window, cx: &gpui::App| {
        remote_input(&p, state, 32.0, 10.0, None, false, false, window, cx)
    };
    let easy_connect = draft.easy_connect;
    let code_reading = if easy_connect {
        Some(if !draft.easy_connect_code.trim().is_empty() {
            read_easy_connect_code_input(&draft.easy_connect_code)
        } else if !draft.easy_connect_address.is_empty() {
            CodeReading::Accepted {
                address: draft.easy_connect_address.clone(),
                name: None,
                user: None,
                ssh_port: None,
                summary: "Paired through Easy Connect.".to_string(),
            }
        } else {
            CodeReading::Empty
        })
    } else {
        None
    };
    let normalized = normalize_remote_machine_draft(&draft);
    let can_save = normalized.is_some();
    let save_reason = if easy_connect {
        if matches!(
            read_easy_connect_code_input(&draft.easy_connect_code),
            CodeReading::Accepted { .. }
        ) || !draft.easy_connect_address.is_empty()
        {
            "Enter a machine name first."
        } else {
            "Paste a valid Easy Connect code first."
        }
    } else if !draft.name.trim().is_empty() && !draft.ssh_host.trim().is_empty() {
        "Check the WSL distribution name."
    } else {
        "Enter a machine name and SSH host first."
    };
    let has_endpoint = if easy_connect {
        !draft.easy_connect_address.is_empty()
    } else {
        !draft.ssh_host.trim().is_empty()
    };
    let install = machine_id
        .as_ref()
        .and_then(|id| tab.installs.get(id).cloned());

    let mut fields: Vec<AnyElement> = Vec::new();
    if !is_new {
        let visible = !draft.disabled;
        fields.push(dialog_switch_row(
            t,
            "remote-machine-dialog-visible",
            "Show in sidebar",
            "Turn off to hide this machine from the sidebar without deleting it.",
            visible,
            move |tab| {
                if let Some(dialog) = tab.machine_dialog.as_mut() {
                    dialog.draft.disabled = visible;
                }
            },
            cx,
        ));
    }
    if let Some(reading) = &code_reading {
        // The reading of a pasted code stays under the field; only the empty field's how-to moves
        // into the (i).
        let message = match reading {
            CodeReading::Accepted { summary, .. } => Some((
                summary.clone(),
                Some(css_mix(gpui::rgb(0x4ade80), 0.78, t.foreground)),
            )),
            CodeReading::Rejected(reason) => Some((
                reason.clone(),
                Some(css_mix(gpui::rgb(0xf87171), 0.82, t.foreground)),
            )),
            CodeReading::Empty => None,
        };
        fields.push(field(
            t,
            &colors,
            "Easy Connect code",
            input(&code_input, window, cx),
            Some("On the other computer, open Settings → Remote, click Connect a computer, then Copy the code."),
            message,
        ));
    }
    fields.push(field(
        t,
        &colors,
        "Name",
        input(&name_input, window, cx),
        None,
        None,
    ));
    // CDXC:RemoteMachines 2026-11-06: the sidebar's pill for this machine (gx-core
    // machine_short_names.rs); empty uses one made from the name.
    fields.push(field(
        t,
        &colors,
        "Short name",
        input(&short_name_input, window, cx),
        Some(
            "Up to 2 characters, shown on the sidebar's computer pills. Leave it empty to use one made from the name.",
        ),
        None,
    ));
    if !easy_connect {
        fields.push(field(
            t,
            &colors,
            "SSH host",
            input(&host_input, window, cx),
            None,
            None,
        ));
    }
    fields.push(field(
        t,
        &colors,
        "SSH username",
        input(&user_input, window, cx),
        Some("The account you sign in with on the other computer."),
        None,
    ));
    let password_row = if is_new {
        input(&password_input, window, cx)
    } else {
        let can_save_password = !draft.ssh_password.trim().is_empty() || draft.ssh_password_saved;
        h_flex()
            .w_full()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .child(input(&password_input, window, cx)),
            )
            .child(
                div()
                    .id("remote-machine-password-save-tooltip")
                    .flex_shrink_0()
                    .w(px(34.0))
                    .flex()
                    .justify_center()
                    .when(!can_save_password, |this| {
                        this.tooltip(tooltip_text("Enter a password to save first."))
                    })
                    .child(settings_icon_button(
                        &p,
                        "remote-machine-password-save",
                        ICON_DEVICE_FLOPPY,
                        16.0,
                        28.0,
                        ButtonVariant::Secondary,
                        None,
                        !can_save_password,
                        |tab: &mut RemoteTab, window, cx| tab.save_dialog_password(window, cx),
                        cx,
                    )),
            )
            .into_any_element()
    };
    fields.push(field(
        t,
        &colors,
        "SSH password",
        password_row,
        Some(if is_new {
            "Use the password for this username on the other computer. If you already use SSH keys, leave this blank and set the key under Advanced."
        } else {
            "Passwords are stored in secure system storage. Leave blank and press Save to remove a saved password."
        }),
        None,
    ));
    // CDXC:RemotePairing 2026-09-05 DECISION: User: simplify adding a machine and hide the WSL version details under Advanced.
    let mut advanced = v_flex()
        .w_full()
        .pt(px(8.0))
        .border_t_1()
        .border_color(hsla(p.hairline))
        .child(div().flex().child(compact_button(
            &p,
            "remote-machine-advanced-toggle",
            "Advanced",
            Some(chevron(advanced_open, 16.0, colors.foreground)),
            Look::Filled,
            28.0,
            14.0,
            Some(colors.foreground),
            false,
            None,
            |tab: &mut RemoteTab, _window, cx| {
                if let Some(dialog) = tab.machine_dialog.as_mut() {
                    dialog.advanced_open = !dialog.advanced_open;
                }
                cx.notify();
            },
            cx,
        )));
    if advanced_open {
        let mut advanced_fields = v_flex().w_full().gap(px(12.0)).pt(px(12.0));
        // CDXC:RemoteMachines 2026-11-03 DECISION: the user chose that connecting a computer is permission enough for agents on both computers to start, message and read each other's sessions, with one switch per machine to stop it (on by default). SEE-ALSO: server/src/connected_computers/registry.rs.
        // CDXC:RemoteMachines 2026-11-09 DECISION: User: "please move agent access to advanced at the bottom". It is the first row of Advanced; turning it on works as before.
        let agent_access = !draft.agent_access_off;
        advanced_fields = advanced_fields.child(dialog_switch_row(
            t,
            "remote-machine-dialog-agent-access",
            "Agent access",
            "Agents on this computer can start, message and read sessions on this machine, and its agents can do the same here.",
            agent_access,
            move |tab| {
                if let Some(dialog) = tab.machine_dialog.as_mut() {
                    dialog.draft.agent_access_off = agent_access;
                }
            },
            cx,
        ));
        if !easy_connect {
            advanced_fields = advanced_fields.child(field(
                t,
                &colors,
                "SSH port",
                input(&port_input, window, cx),
                None,
                None,
            ));
        }
        advanced_fields = advanced_fields
            .child(field(
                t,
                &colors,
                "Identity file",
                input(&identity_input, window, cx),
                Some("Optional. Use an SSH private key instead of a password."),
                None,
            ))
            .child(field(
                t,
                &colors,
                "Windows WSL distribution",
                input(&wsl_input, window, cx),
                Some(
                    "Optional. Enter a distribution to use WSL2. Leave blank to follow Windows Environment on the remote computer, which defaults to native PowerShell.",
                ),
                None,
            ));
        // A Windows WSL machine's gxserver listens inside that distribution, where a forward in
        // the user's own SSH connection cannot land, so the setting is not offered for it.
        // Windows OpenSSH cannot multiplex, so the setting is macOS and Linux only.
        if cfg!(any(target_os = "macos", target_os = "linux"))
            && !easy_connect
            && draft.wsl_distribution.trim().is_empty()
        {
            // CDXC:RemoteMachines 2026-11-08 DECISION: GitHub #496, where the maintainer asked for
            // "an advanced toggle to be enabled in the remote tab": the machine can ride the SSH
            // connection the user already signed in to, so hosts that ask for a verification code
            // on every login (2FA) work over the master `~/.ssh/config` keeps open.
            // SEE-ALSO: helpers/remote/ssh_connection_share.rs (the check and the reused login).
            let reuse = draft.reuse_ssh_connection;
            advanced_fields = advanced_fields.child(dialog_switch_row(
                t,
                "remote-machine-dialog-reuse-ssh",
                "Reuse an existing SSH connection",
                "For computers that ask for a verification code each time you sign in. Sign in once in a terminal with ControlMaster on in your SSH config, and Ghostex uses that connection instead of signing in again.",
                reuse,
                move |tab| {
                    if let Some(dialog) = tab.machine_dialog.as_mut() {
                        dialog.draft.reuse_ssh_connection = !reuse;
                    }
                },
                cx,
            ));
        }
        advanced = advanced.child(advanced_fields);
    }
    fields.push(advanced.into_any_element());

    let mut body = v_flex().w_full().min_w_0().gap(px(12.0)).pr(px(2.0));
    if let Some(machine_id) = machine_id.as_deref()
        && let Some(panel) = machine_status_panel(tab, t, &colors, machine_id, &title, cx)
    {
        body = body.child(panel);
    }
    if is_new {
        body = body.child(div().mb(px(2.0)).child(raised_segmented(
            &p,
            "remote-machine-add-mode",
            &[
                Segment {
                    value: "ssh",
                    label: "SSH details",
                    icon: None,
                },
                Segment {
                    value: "easyConnect",
                    label: "Easy Connect code",
                    icon: None,
                },
            ],
            if easy_connect { "easyConnect" } else { "ssh" },
            true,
            |tab: &mut RemoteTab, value, window, cx| {
                tab.set_dialog_transport(value == "easyConnect", window, cx);
            },
            cx,
        )));
    }
    body = body.child(v_flex().w_full().gap(px(12.0)).children(fields));
    // A machine that is this computer runs this app's own gxserver: installing or updating it
    // there would stop that gxserver, so the row explains instead of offering the button.
    let this_computer = machine_id
        .as_ref()
        .is_some_and(|id| tab.this_computer.contains(id));
    if this_computer {
        body = body.child(
            div()
                .w_full()
                .mt(px(8.0))
                .text_size(px(13.0))
                .line_height(px(18.2))
                .text_color(hsla(colors.muted))
                .child(
                    "This is this computer. Its gxserver is the one this app runs, so it is not installed or updated from here.",
                ),
        );
    }
    if let Some(machine_id) = machine_id.clone().filter(|_| !this_computer) {
        // CDXC:RemoteMachines 2026-06-23-08:30: the gxserver install action reuses the reconnect flow, so native opens the approval modal only after SSH proves gxserver is missing, and otherwise connects the existing remote daemon.
        let installed = install.as_ref().is_some_and(|install| install.installed);
        let version_text = install
            .as_ref()
            .filter(|install| install.installed)
            .map(|install| {
                install
                    .version
                    .as_deref()
                    .and_then(short_version)
                    .map(|version| format!("gxserver {version}"))
                    .unwrap_or_else(|| "gxserver installed".to_string())
            });
        // A build string shows as its version number; the whole one is on hover (long_value.rs).
        let version_full = install
            .as_ref()
            .filter(|install| install.installed)
            .and_then(|install| install.version.clone())
            .filter(|version| is_long_value(version));
        body = body.child(
            h_flex()
                .w_full()
                .mt(px(8.0))
                .items_center()
                .gap(px(8.0))
                .justify_end()
                .children(version_text.map(|text| {
                    let line = div()
                        .id("remote-machine-gxserver-version")
                        .mr_auto()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(13.0))
                        .line_height(px(18.2))
                        .text_color(hsla(colors.muted))
                        .child(text);
                    match version_full {
                        Some(full) => full_value_tooltip(line, format!("gxserver {full}"))
                            .into_any_element(),
                        None => line.into_any_element(),
                    }
                }))
                .child(settings_button_sized(
                    &p,
                    "remote-machine-install-gxserver",
                    if installed {
                        "Update gxserver"
                    } else {
                        "Install / Connect gxserver"
                    },
                    Some(if installed { ICON_REFRESH } else { icon::DOWNLOAD }),
                    ButtonVariant::Secondary,
                    ButtonSize::Default,
                    !has_endpoint,
                    Some(
                        if easy_connect {
                            "Paste a valid Easy Connect code first."
                        } else {
                            "Enter an SSH host first."
                        }
                        .into(),
                    ),
                    move |tab: &mut RemoteTab, _window, cx| {
                        tab.post(
                            json!({ "remoteMachineId": machine_id, "type": "reconnectRemoteMachine" }),
                            cx,
                        );
                    },
                    cx,
                )),
        );
    }

    if let Some(machine_id) = machine_id.as_deref()
        && let Some(section) = super::app_update::dialog_update_section(tab, t, machine_id, cx)
    {
        body = body.child(section);
    }

    if let Some(pairing) = tab
        .machine_dialog
        .as_ref()
        .and_then(|dialog| dialog.pairing.clone())
    {
        body = body.child(computer_pairing_panel(t, &pairing));
    }
    let pairing_busy = tab.machine_dialog.as_ref().is_some_and(|dialog| {
        matches!(
            dialog.pairing,
            Some(ComputerPairing::Starting | ComputerPairing::Waiting { .. })
        )
    });

    let mut footer = h_flex().w_full().items_center().justify_end().gap(px(8.0));
    if let Some(machine_id) = dialog_machine_id(tab) {
        footer = footer.child(div().mr_auto().child(settings_button_sized(
            &p,
            "remote-machine-remove",
            "Remove",
            Some(icon::TRASH),
            ButtonVariant::DestructiveDialog,
            ButtonSize::Default,
            false,
            None,
            move |tab: &mut RemoteTab, _window, cx| tab.remove_machine(machine_id.clone(), cx),
            cx,
        )));
    }
    footer = footer
        .child(settings_button_sized(
            &p,
            "remote-machine-cancel",
            "Cancel",
            None,
            ButtonVariant::Outline,
            ButtonSize::Default,
            false,
            None,
            |tab: &mut RemoteTab, _window, cx| tab.close_machine_dialog(cx),
            cx,
        ))
        .child(settings_button_sized(
            &p,
            "remote-machine-save",
            if pairing_busy {
                "Waiting for approval…"
            } else if is_new {
                "Add machine"
            } else {
                "Save"
            },
            None,
            ButtonVariant::Default,
            ButtonSize::Default,
            !can_save || pairing_busy,
            Some(save_reason.into()),
            |tab: &mut RemoteTab, _window, cx| tab.save_dialog_machine(cx),
            cx,
        ));

    let viewport = window.viewport_size();
    let sheet = v_flex()
        .id("remote-machine-dialog")
        .track_focus(&focus)
        .occlude()
        .w(px(512.0))
        .max_w(viewport.width - px(32.0))
        .max_h(viewport.height - px(32.0))
        .p(px(16.0))
        .gap(px(12.0))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(colors.border))
        .bg(hsla(colors.background))
        .shadow(vec![gpui::BoxShadow {
            color: hsla(css_fade(colors.foreground, 0.1)),
            offset: gpui::point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(1.0),
            inset: false,
        }])
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(colors.foreground))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_key_down(cx.listener(|tab, event: &KeyDownEvent, _window, cx| {
            if event.keystroke.key == "escape" {
                cx.stop_propagation();
                tab.close_machine_dialog(cx);
            }
        }))
        .child(
            v_flex()
                .w_full()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(16.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(colors.foreground))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(hsla(colors.muted))
                        .child(if is_new {
                            "Paste an Easy Connect code or enter the other computer’s address, then use that computer’s SSH username and password to sign in."
                        } else {
                            "Connection details for this machine. Changes apply when you save."
                        }),
                ),
        )
        .child(
            div()
                .id("remote-machine-dialog-body")
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body),
        )
        .child(footer)
        .into_any_element();
    Some(dialog_overlay(
        "remote-machine-dialog-overlay",
        sheet,
        |tab: &mut RemoteTab, _window, cx| tab.close_machine_dialog(cx),
        window,
        cx,
    ))
}

/// The first thing in a failing machine's sheet: what failed, why in a sentence, and the fixes.
///
/// CDXC:RemoteMachines 2026-11-08 DECISION: User (Q2018, Q2019 on the remote tile page): a
/// machine's failure and its fix live at the top of the machine's own sheet, not in its tile. The
/// panel has the state's title with the tile's red dot, the reason the app sent (or the state's
/// own words), and Reconnect, Reinstall on <machine> (only for a gxserver package Ghostex installed
/// there: the connect then replaces it even at the same build) and Copy details, which copies the
/// state and the full reason for a bug report.
fn machine_status_panel(
    tab: &RemoteTab,
    t: &RemoteTokens,
    colors: &DialogColors,
    machine_id: &str,
    machine_name: &str,
    cx: &mut Context<RemoteTab>,
) -> Option<AnyElement> {
    let state = tab
        .connect_states
        .get(machine_id)
        .map(String::as_str)
        .filter(|state| ghostex_gx_core::machine_state_is_failure(state))?;
    if tab.this_computer.contains(machine_id) {
        return None;
    }
    let (title, default_reason) = ghostex_gx_core::machine_state_copy(state);
    let reason = tab
        .status_messages
        .get(machine_id)
        .map(|message| message.trim().to_string())
        .filter(|message| !message.is_empty())
        .or_else(|| default_reason.map(str::to_string));
    let managed = tab
        .installs
        .get(machine_id)
        .is_some_and(|install| install.installed && install.managed);
    let details = format!(
        "{machine_name}: {title} ({state})\n{}",
        reason.clone().unwrap_or_default()
    );
    let p = t.p.nested_dialog();
    let copy_id = SharedString::from(format!("remote-machine-status-copy-{machine_id}"));
    let copied = tab.copied.contains(&copy_id);
    let reconnect_id = machine_id.to_string();
    let reinstall_id = machine_id.to_string();
    let mut actions = h_flex()
        .w_full()
        .flex_wrap()
        .gap(px(6.0))
        .pl(px(15.0))
        .pt(px(4.0))
        .child(settings_button_sized(
            &p,
            "remote-machine-status-reconnect",
            "Reconnect",
            Some(ICON_REFRESH),
            ButtonVariant::Default,
            ButtonSize::Sm,
            false,
            None,
            move |tab: &mut RemoteTab, _window, cx| {
                tab.post(
                    json!({ "remoteMachineId": reconnect_id, "type": "reconnectRemoteMachine" }),
                    cx,
                );
            },
            cx,
        ));
    if managed {
        actions = actions.child(settings_button_sized(
            &p,
            "remote-machine-status-reinstall",
            format!("Reinstall on {machine_name}"),
            Some(icon::DOWNLOAD),
            ButtonVariant::Secondary,
            ButtonSize::Sm,
            false,
            None,
            move |tab: &mut RemoteTab, _window, cx| {
                tab.post(
                    json!({
                        "reinstall": true,
                        "remoteMachineId": reinstall_id,
                        "type": "reconnectRemoteMachine",
                    }),
                    cx,
                );
            },
            cx,
        ));
    }
    actions = actions.child(settings_button_sized(
        &p,
        copy_id.clone(),
        if copied { "Copied" } else { "Copy details" },
        Some(ICON_COPY),
        ButtonVariant::Secondary,
        ButtonSize::Sm,
        false,
        None,
        move |tab: &mut RemoteTab, _window, cx| tab.copy(copy_id.clone(), details.clone(), cx),
        cx,
    ));
    Some(
        v_flex()
            .id("remote-machine-status-panel")
            .w_full()
            .gap(px(2.0))
            .p(px(12.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(colors.border))
            .child(
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(status_dot(t))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(colors.foreground))
                            .child(title),
                    ),
            )
            .children(reason.map(|reason| {
                div()
                    .pl(px(15.0))
                    .text_size(px(13.0))
                    .line_height(px(19.0))
                    .text_color(hsla(colors.muted))
                    .child(reason)
            }))
            .child(actions)
            .into_any_element(),
    )
}

fn dialog_machine_id(tab: &RemoteTab) -> Option<String> {
    tab.machine_dialog
        .as_ref()
        .and_then(|dialog| dialog.machine_id.clone())
}

/// The dialog's pairing line: the three words to compare, or why pairing stopped.
fn computer_pairing_panel(t: &RemoteTokens, pairing: &ComputerPairing) -> AnyElement {
    let frame = v_flex()
        .w_full()
        .gap(px(4.0))
        .p(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(t.edge(0.70)));
    match pairing {
        ComputerPairing::Starting => frame
            .child(detail(t, "Asking the other computer to let this one in…", true))
            .into_any_element(),
        ComputerPairing::Waiting {
            words, computer, ..
        } => frame
            .items_center()
            .child(detail(
                t,
                format!("Approve this computer on {computer}. Check it shows the same three words:"),
                true,
            ))
            .child(
                div()
                    .text_size(px(18.0))
                    .line_height(px(26.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(t.foreground))
                    .child(words.join(" · ")),
            )
            .child(detail(
                t,
                "It's under Settings > Remote > Paired devices there, waiting for Touch ID, Windows Hello or the system password.",
                true,
            ))
            .into_any_element(),
        ComputerPairing::Failed(message) => frame
            .child(
                div()
                    .text_color(hsla(t.p.destructive))
                    .child(message.clone()),
            )
            .into_any_element(),
    }
}
