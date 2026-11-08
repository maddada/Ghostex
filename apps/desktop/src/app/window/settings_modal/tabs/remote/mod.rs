//! Settings > Remote (packages/core-ui/settings-modal/tabs/remote.tsx (deleted 2026-10-01) and its `remote-*` parts).
//!
//! CDXC:RemotePairing 2026-09-03:
//! Settings → Remote reads top to bottom as: this computer from a phone (the Easy Connect and Tailscale path cards), this computer reaching other machines (the compact saved-machine grid), then one Advanced collapsible. The Remote Setup modal deep-links into a path card through `initialRemoteSection`, which expands and scrolls to that card; `initialRemoteMachineId` scrolls to a saved machine's tile and opens its edit dialog.
//!
//! CDXC:RemotePairing 2026-09-03 DECISION:
//! User: "make easy connect and tailscale 2 options that are above each other vertically in that section, not next to each other", shown "as expandible cards so the user clicks to expand the one they want to use".
//! The two cards stack full width, both start collapsed unless a deep link names one, and opening one collapses the other so a single QR code is visible at a time.
//!
//! CDXC:RemotePairing 2026-10-16 DECISION:
//! User: "i want reopening settings to always reopen to the exact same spot i was in before (same scroll pos too and same open modals if possible)", and asked whether the open path card should be remembered too, picked "Remember it too" (supersedes the 2026-09-03 "The open card is plain UI state and is not persisted"). The expanded card (Easy Connect or Tailscale) is saved with the page's open machine dialog (`open_form`, CDXC:Settings 2026-10-16 in settings_modal/restore.rs) and reopens with Settings; an `initialRemoteSection` deep link still picks the card, and a machine dialog that open keeps closed is carried to the next open instead of being forgotten.
//!
//! CDXC:RemotePairing 2026-10-24 DECISION:
//! User: "pls make tailscale and easy connect 2 tabs here instead of 2 collapsible areas cuz it's easier to switch between the 2 that way. also the already connected machines needs to be a 3rd tab there. i don't like scrolling so much in this page to get to stuff".
//! Supersedes the 2026-09-03 expandable cards. The page opens with one tab strip (the shared `modal_bordered_segmented_control`): Easy Connect, Tailscale and Remote machines. Each tab shows its content straight away (the card's header row with its switch and badges, then its body; the saved machines and the Add tile), with Advanced below whichever tab is open. The "Connect to this computer" heading is folded into the tabs. Easy Connect is the default; the last tab is remembered the way the open card was (`open_form`'s `card`, now also `remoteMachines`). A Settings search that matches nothing on the open tab switches to the first tab with a match; deep links pick their tab.
//!
//! CDXC:RemotePairing 2026-10-24 DECISION:
//! User: "for remote tab please move advanced to a new tab", "please make the machines tab show similar card as the 2 other tabs (make it styled the same)" and "also please show the status for each on the tabs bar".
//! Advanced is a fourth tab (always open inside it, no disclosure) instead of sitting under every tab, and a search for one of its rows lands on it. The Remote machines tab draws the same path card as Easy Connect and Tailscale (icon, title, a connected-count badge, then the description and the tiles). Each tab in the strip carries its status: Easy Connect and Tailscale a dot in their card badge's colour plus the badge's word, Remote machines "connected/saved" from the connection states the app sends (`remoteMachineConnectStates`), Advanced an amber dot and "Unsaved" only while a ports or keys edit is pending.
//!
//! One owner (this page) reads everything the Remote page shows from the daemon, so the Easy
//! Connect card, the Tailscale card and Advanced render the same snapshot: while Remote is the
//! active page, a fast poll (4s: the Easy Connect status and the pairing code, which rotates after
//! a phone pairs) and a slow one (10s: SSH access, Tailscale and the paired devices, which shell
//! out on the daemon), each guarded so a slow daemon never stacks requests; both stop as soon as
//! the page is left or Settings closes (`use-remote-access.ts` (deleted 2026-10-01)).
mod advanced;
mod app_update;
mod easy_connect;
mod machines;
mod model;
mod pair_again;
mod paired_devices;
mod relay;
mod ssh_row;
mod style;
mod tailscale;

use super::super::super::native_modal_kit::{
    ModalRailItem, capture_child_bounds, modal_bordered_segmented_control,
};
use super::super::fields::{FieldStates, QrGrid, SettingsPage};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::{rail_pages, render_no_matches};
use super::super::search::should_show_section;
use super::super::store::{
    SettingsStore, SettingsStoreEvent, SmoothScroll, post_store_message, store_approve_request,
    store_copy_to_clipboard, store_gxserver_rpc,
};
use gpui::{
    AnyView, App, AppContext as _, Bounds, Context, Entity, FocusHandle, IntoElement,
    ParentElement as _, Pixels, Render, SharedString, Styled as _, Task, Window, div, px,
};
use gpui_component::v_flex;
use model::*;
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;
use style::RemoteTokens;

/// A request timeout for the polls and quick actions.
const RPC_TIMEOUT: Duration = Duration::from_secs(15);
/// `/api/enableSshAccess` waits on the admin prompt until the user answers it.
const ENABLE_SSH_TIMEOUT: Duration = Duration::from_secs(300);
/// `/api/removePairedDevice` waits on the Windows admin prompt when the key is in the
/// administrators keys file.
const REMOVE_PAIRED_DEVICE_TIMEOUT: Duration = Duration::from_secs(300);
/// Starting or stopping the sidecar.
const UPDATE_TAILCAT_TIMEOUT: Duration = Duration::from_secs(60);
/// The Easy Connect helper install downloads and may build.
const INSTALL_TAILCAT_TIMEOUT: Duration = Duration::from_secs(900);
/// How long a copy button shows its check.
const COPIED_FLASH: Duration = Duration::from_millis(1200);
/// How often the paired devices' "connected now" is re-read against the clock.
const PAIRED_DEVICE_CLOCK: Duration = Duration::from_secs(30);

pub(crate) fn remote_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| RemoteTab::new(store.clone(), window, cx))
        .into()
}

/// Which path card is open (`SettingsRemoteSection`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PathCard {
    EasyConnect,
    Tailscale,
}

impl PathCard {
    fn from_section(section: &str) -> Option<Self> {
        match section {
            "easyConnect" => Some(Self::EasyConnect),
            "tailscale" => Some(Self::Tailscale),
            _ => None,
        }
    }
}

/// The Remote page's tabs: the two ways in (`SettingsRemoteSection`) and the saved machines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RemoteSection {
    #[default]
    EasyConnect,
    Tailscale,
    Machines,
    Advanced,
}

impl RemoteSection {
    const ALL: [Self; 4] = [
        Self::EasyConnect,
        Self::Tailscale,
        Self::Machines,
        Self::Advanced,
    ];

    fn from_section(section: &str) -> Option<Self> {
        match section {
            "easyConnect" => Some(Self::EasyConnect),
            "tailscale" => Some(Self::Tailscale),
            "remoteMachines" => Some(Self::Machines),
            "remoteAdvanced" => Some(Self::Advanced),
            _ => None,
        }
    }

    /// The tab's id, which is also its Settings search section.
    fn anchor(self) -> &'static str {
        match self {
            Self::EasyConnect => "easyConnect",
            Self::Tailscale => "tailscale",
            Self::Machines => "remoteMachines",
            Self::Advanced => "remoteAdvanced",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::EasyConnect => "Easy Connect",
            Self::Tailscale => "Tailscale",
            Self::Machines => "Remote machines",
            Self::Advanced => "Advanced",
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or(0)
    }
}

impl From<PathCard> for RemoteSection {
    fn from(card: PathCard) -> Self {
        match card {
            PathCard::EasyConnect => Self::EasyConnect,
            PathCard::Tailscale => Self::Tailscale,
        }
    }
}

/// The gxserver install state native reported for a saved machine (`RemoteGxserverInstallState`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct GxserverInstall {
    pub(super) installed: bool,
    pub(super) version: Option<String>,
}

pub(crate) struct RemoteTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    // ---- what the daemon reports (`RemoteAccessState`) -----------------------------------------
    pub(super) easy_connect: Option<TailcatStatus>,
    pub(super) access: Option<AccessStatus>,
    pub(super) pairing: Option<PairingCodes>,
    pub(super) paired_devices: Option<Vec<PairedDevice>>,
    /// Phones waiting for approval, with their words (pairing v2).
    pub(super) pending_devices: Vec<PendingDevice>,
    /// The approval request being approved for a waiting phone or computer (its system prompt is up).
    pub(super) approving_device: Option<String>,
    /// A `/api/computerPairingStatus` ask is out (machines.rs).
    pub(super) computer_pairing_in_flight: bool,
    pub(super) request_error: Option<String>,
    pub(super) enabling_ssh: bool,
    pub(super) install_pending: bool,
    pub(super) install_request_error: Option<String>,
    pub(super) ssh_attempt: Option<SshEnableAttempt>,
    pub(super) removing_device: Option<String>,
    fast_in_flight: bool,
    slow_in_flight: bool,
    polling: Option<(Task<()>, Task<()>)>,
    clock: Option<Task<()>>,
    pub(super) now: i64,
    // ---- page state -----------------------------------------------------------------------------
    /// The open tab.
    pub(super) section: RemoteSection,
    /// `phone` or `computer` (the Connect segmented control).
    pub(super) connection_device: &'static str,
    /// The Show QR dialog open, and which card's pairing code it shows.
    pub(super) qr_preview: Option<PathCard>,
    pub(super) qr_preview_focus: FocusHandle,
    pub(super) manual_open: bool,
    pub(super) confirming_device: Option<String>,
    /// The per-OS SSH instructions popover open, keyed by the row that owns it.
    pub(super) os_popover: Option<(SharedString, &'static str)>,
    pub(super) os_popover_focus: FocusHandle,
    pub(super) os_button_bounds: HashMap<SharedString, Rc<Cell<Option<Bounds<Pixels>>>>>,
    pub(super) copied: HashSet<SharedString>,
    copy_tasks: HashMap<SharedString, Task<()>>,
    pub(super) advanced: advanced::AdvancedState,
    pub(super) relay: relay::RelayState,
    pub(super) machine_dialog: Option<machines::MachineDialog>,
    /// This computer's short-name field in the Remote machines card, made on its first draw.
    pub(super) local_short_name: Option<(
        Entity<gpui_component::input::InputState>,
        gpui::Subscription,
    )>,
    /// The machine dialog the last visit left open, kept closed by this open's deep link and
    /// carried to the next open until another machine dialog opens.
    pub(super) kept_machine_form: Option<Value>,
    /// The path cards' and the machine tiles' bounds last frame, for the deep-link scrolls.
    card_bounds: [Rc<Cell<Option<Bounds<Pixels>>>>; 2],
    machines_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pending_scroll: Option<ScrollTarget>,
    pub(super) installs: HashMap<String, GxserverInstall>,
    /// Each connected machine's Ghostex version and update row (`remoteAppUpdateState`).
    pub(super) app_updates: HashMap<String, Value>,
    /// Each saved machine's connection state from the app (`remoteMachineConnectStates`).
    pub(super) connect_states: HashMap<String, String>,
    /// The saved machines whose gxserver is this computer's own (`thisComputer`).
    pub(super) this_computer: HashSet<String>,
    /// Each failing machine's sanitized reason (`messages`), drawn on its tile.
    pub(super) status_messages: HashMap<String, String>,
    probed_keys: HashSet<String>,
    probe_task: Option<Task<()>>,
    probed_list: Option<Value>,
    easy_connect_qr: Option<(String, Option<QrGrid>)>,
    tailscale_qr: Option<(String, Option<QrGrid>)>,
    targeted_section: Option<String>,
    targeted_machine: Option<String>,
    was_active: bool,
    preview_applied: bool,
    /// Phones and computers from before 11.0.1 that still wait to be paired again (pair_again.rs).
    pair_again_waiting: usize,
}

impl super::HoldsUnsavedInput for RemoteTab {
    /// An open Add a machine / edit dialog, an unsaved Advanced edit or a device removal
    /// confirmation.
    fn holds_unsaved_input(&self, _cx: &gpui::App) -> bool {
        self.machine_dialog.is_some()
            || self.advanced.has_draft()
            || self.confirming_device.is_some()
    }
}

impl super::open_forms::RemembersOpenForm for RemoteTab {
    /// The open tab (when it isn't the default Easy Connect) and the Add a machine / edit dialog
    /// (or the one a deep link kept closed on this open).
    fn open_form(&self, _cx: &gpui::App) -> Option<Value> {
        let machine = self
            .open_machine_form()
            .or_else(|| self.kept_machine_form.clone());
        let card = (self.section != RemoteSection::EasyConnect).then(|| self.section.anchor());
        if machine.is_none() && card.is_none() {
            return None;
        }
        Some(json!({ "kind": "remote", "card": card, "machine": machine }))
    }
}

impl SettingsPage for RemoteTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl RemoteTab {
    pub(crate) fn new(
        store: Entity<SettingsStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |tab: &mut Self, _, cx| {
            tab.sync_activity(cx);
            cx.notify();
        })
        .detach();
        cx.subscribe(
            &store,
            |tab: &mut Self, store, event: &SettingsStoreEvent, cx| {
                let SettingsStoreEvent::HostPayload(kind) = event;
                if kind == "remoteGxserverInstallState"
                    && let Some(message) = store.read(cx).host_payload(kind).cloned()
                {
                    tab.receive_install_state(&message, cx);
                }
                if kind == "remoteAppUpdateState"
                    && let Some(message) = store.read(cx).host_payload(kind).cloned()
                {
                    tab.receive_app_update_state(&message, cx);
                }
                if kind == "remoteMachineConnectStates"
                    && let Some(message) = store.read(cx).host_payload(kind).cloned()
                {
                    tab.connect_states = message
                        .get("states")
                        .and_then(Value::as_object)
                        .map(|states| {
                            states
                                .iter()
                                .filter_map(|(id, state)| {
                                    Some((id.clone(), state.as_str()?.to_string()))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    tab.this_computer = message
                        .get("thisComputer")
                        .and_then(Value::as_array)
                        .map(|ids| {
                            ids.iter()
                                .filter_map(|id| id.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    tab.status_messages = message
                        .get("messages")
                        .and_then(Value::as_object)
                        .map(|messages| {
                            messages
                                .iter()
                                .filter_map(|(id, text)| {
                                    Some((id.clone(), text.as_str()?.to_string()))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    cx.notify();
                }
            },
        )
        .detach();
        let mut tab = Self {
            store,
            fields: FieldStates::default(),
            easy_connect: None,
            access: None,
            pairing: None,
            paired_devices: None,
            pending_devices: Vec::new(),
            computer_pairing_in_flight: false,
            approving_device: None,
            request_error: None,
            enabling_ssh: false,
            install_pending: false,
            install_request_error: None,
            ssh_attempt: None,
            removing_device: None,
            fast_in_flight: false,
            slow_in_flight: false,
            polling: None,
            clock: None,
            now: now_ms(),
            section: RemoteSection::default(),
            connection_device: "phone",
            qr_preview: None,
            qr_preview_focus: cx.focus_handle(),
            manual_open: false,
            confirming_device: None,
            os_popover: None,
            os_popover_focus: cx.focus_handle(),
            os_button_bounds: HashMap::new(),
            copied: HashSet::new(),
            copy_tasks: HashMap::new(),
            advanced: advanced::AdvancedState::default(),
            relay: relay::RelayState::default(),
            machine_dialog: None,
            local_short_name: None,
            kept_machine_form: None,
            card_bounds: [Rc::new(Cell::new(None)), Rc::new(Cell::new(None))],
            machines_bounds: Rc::new(Cell::new(None)),
            pending_scroll: None,
            installs: HashMap::new(),
            app_updates: HashMap::new(),
            connect_states: HashMap::new(),
            this_computer: HashSet::new(),
            status_messages: HashMap::new(),
            probed_keys: HashSet::new(),
            probe_task: None,
            probed_list: None,
            easy_connect_qr: None,
            tailscale_qr: None,
            targeted_section: None,
            targeted_machine: None,
            was_active: false,
            preview_applied: false,
            pair_again_waiting: 0,
        };
        tab.sync_activity(cx);
        tab.restore_open_form(window, cx);
        tab
    }

    /// The tab and machine dialog the last visit left open (settings_modal/restore.rs).
    /// An open that deep-links into Remote picks its own card and keeps the dialog closed, carrying
    /// it to the next open.
    fn restore_open_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (form, deep_linked) = self.store.update(cx, |store, _| {
            store
                .open_forms_mut()
                .take_with_deep_link(SettingsTabId::Remote)
        });
        let Some(form) = form else {
            return;
        };
        // Saved before the card was remembered, the form was the machine dialog itself.
        let machine = match form.get("kind").and_then(Value::as_str) {
            Some("machine") => Some(form.clone()),
            _ => form
                .get("machine")
                .filter(|machine| machine.is_object())
                .cloned(),
        };
        if deep_linked {
            self.kept_machine_form = machine;
            return;
        }
        if let Some(section) = form
            .get("card")
            .and_then(Value::as_str)
            .and_then(RemoteSection::from_section)
        {
            self.section = section;
        }
        if let Some(machine) = machine {
            self.restore_machine_dialog(&machine, window, cx);
        }
    }

    pub(super) fn tokens(&self, cx: &App) -> RemoteTokens {
        RemoteTokens::new(&self.store.read(cx).palette())
    }

    pub(super) fn rpc_available(&self, cx: &App) -> bool {
        self.store.read(cx).request().gxserver_rpc_available
    }

    fn is_active(&self, cx: &App) -> bool {
        self.store.read(cx).active_tab() == SettingsTabId::Remote
    }

    /// The saved machines as the React draft holds them (normalized).
    pub(super) fn remote_machines(&self, cx: &App) -> Vec<Value> {
        normalize_remote_machines(&self.store.read(cx).value("remoteMachines"))
    }

    // ---- polling -----------------------------------------------------------------------------------

    /// Starts the polls, the probe pass and the paired-device clock when the page becomes the active
    /// one; stops them when it is left.
    fn sync_activity(&mut self, cx: &mut Context<Self>) {
        let active = self.is_active(cx);
        if active != self.was_active {
            self.was_active = active;
            if active {
                self.request_app_update_rows(cx);
            }
            if !active {
                self.targeted_section = None;
                self.targeted_machine = None;
            }
        }
        let rpc = self.rpc_available(cx);
        if active && rpc && self.polling.is_none() {
            self.refresh_fast(cx);
            self.refresh_slow(cx);
            let fast = cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(REMOTE_FAST_REFRESH_MS))
                        .await;
                    if this.update(cx, |this, cx| this.refresh_fast(cx)).is_err() {
                        break;
                    }
                }
            });
            let slow = cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(REMOTE_SLOW_REFRESH_MS))
                        .await;
                    if this.update(cx, |this, cx| this.refresh_slow(cx)).is_err() {
                        break;
                    }
                }
            });
            self.polling = Some((fast, slow));
        } else if !active && self.polling.is_some() {
            self.polling = None;
        }
        if active && self.clock.is_none() {
            self.clock = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(PAIRED_DEVICE_CLOCK).await;
                    if this
                        .update(cx, |this, cx| {
                            this.now = now_ms();
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        } else if !active {
            self.clock = None;
        }
        if active {
            self.schedule_install_probes(cx);
        }
    }

    /// A gxserver request whose answer lands on this page.
    pub(super) fn rpc(
        &self,
        path: &str,
        params: Value,
        timeout: Duration,
        cx: &mut Context<Self>,
        done: impl FnOnce(&mut Self, Result<Value, String>, &mut Context<Self>) + 'static,
    ) {
        let weak = cx.weak_entity();
        store_gxserver_rpc(
            &self.store,
            path,
            params,
            timeout,
            move |result, cx| {
                let _ = weak.update(cx, |this, cx| done(this, result, cx));
            },
            cx,
        );
    }

    /// `refreshFast`: the Easy Connect status and the pairing code, both or neither.
    pub(super) fn refresh_fast(&mut self, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) || self.fast_in_flight {
            return;
        }
        self.fast_in_flight = true;
        self.rpc(
            "/api/tailcatStatus",
            json!({}),
            RPC_TIMEOUT,
            cx,
            |this, status, cx| {
                let status = status.and_then(|value| read_tailcat_status_result(&value));
                match status {
                    Err(error) => {
                        this.fast_in_flight = false;
                        this.request_error = Some(error);
                        cx.notify();
                    }
                    Ok(status) => this.rpc(
                        "/api/remotePairingCode",
                        // While the Easy Connect QR is on screen a new phone must get past the
                        // allow-list (CDXC:RemotePairing 2026-10-25 in server/src/tailcat/supervisor.rs).
                        json!({ "openTunnel": this.qr_preview == Some(PathCard::EasyConnect) }),
                        RPC_TIMEOUT,
                        cx,
                        move |this, code, cx| {
                            this.fast_in_flight = false;
                            match code.and_then(|value| read_remote_pairing_code_result(&value)) {
                                Ok(code) => {
                                    this.easy_connect = Some(status);
                                    this.pairing = Some(code);
                                    this.request_error = None;
                                    // A phone that just scanned shows its words on the sheet
                                    // without waiting for the slow refresh.
                                    if this.qr_preview == Some(PathCard::EasyConnect) {
                                        this.refresh_pending_devices(cx);
                                    }
                                }
                                Err(error) => this.request_error = Some(error),
                            }
                            cx.notify();
                        },
                    ),
                }
            },
        );
    }

    /// Approve on a waiting phone or computer: Touch ID / Windows Hello / the system password,
    /// then the signed approval (`owner_security`, GitHub #495); the device finishes pairing on
    /// its next ask.
    pub(super) fn approve_pending_device(&mut self, approval_id: String, cx: &mut Context<Self>) {
        if self.approving_device.is_some() || approval_id.is_empty() {
            return;
        }
        self.approving_device = Some(approval_id.clone());
        cx.notify();
        let weak = cx.weak_entity();
        store_approve_request(
            &self.store,
            approval_id,
            move |outcome, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.approving_device = None;
                    if let Err(error) = outcome {
                        this.request_error = Some(error);
                    }
                    this.refresh_pending_devices(cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// The phones waiting for approval, while the Show QR sheet is open.
    pub(super) fn refresh_pending_devices(&mut self, cx: &mut Context<Self>) {
        self.rpc(
            "/api/pairedDevices",
            json!({}),
            RPC_TIMEOUT,
            cx,
            |this, devices, cx| {
                if let Ok(value) = devices {
                    this.pending_devices = read_pending_devices(&value);
                    if let Ok(devices) = read_paired_devices_result(&value) {
                        this.paired_devices = Some(devices);
                    }
                    cx.notify();
                }
            },
        );
    }

    /// `refreshSlow`: SSH access, Tailscale and this computer's identity, and the paired devices.
    fn refresh_slow(&mut self, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) || self.slow_in_flight {
            return;
        }
        self.slow_in_flight = true;
        self.rpc(
            "/api/securityUpgradeNotice",
            json!({ "op": "read" }),
            RPC_TIMEOUT,
            cx,
            |this, answer, cx| {
                let waiting = answer
                    .map(|answer| pair_again::waiting_of(&answer))
                    .unwrap_or(0);
                if this.pair_again_waiting != waiting {
                    this.pair_again_waiting = waiting;
                    cx.notify();
                }
            },
        );
        self.rpc(
            "/api/remoteAccessStatus",
            json!({}),
            RPC_TIMEOUT,
            cx,
            |this, access, cx| match access
                .and_then(|value| read_remote_access_status_result(&value))
            {
                Err(error) => {
                    this.slow_in_flight = false;
                    this.request_error = Some(error);
                    cx.notify();
                }
                Ok(access) => this.rpc(
                    "/api/pairedDevices",
                    json!({}),
                    RPC_TIMEOUT,
                    cx,
                    move |this, devices, cx| {
                        this.slow_in_flight = false;
                        let pending = devices.as_ref().ok().map(read_pending_devices);
                        match devices.and_then(|value| read_paired_devices_result(&value)) {
                            Ok(devices) => {
                                if access.ssh.enabled {
                                    this.ssh_attempt = None;
                                }
                                this.access = Some(access);
                                this.paired_devices = Some(devices);
                                this.pending_devices = pending.unwrap_or_default();
                            }
                            Err(error) => this.request_error = Some(error),
                        }
                        cx.notify();
                    },
                ),
            },
        );
    }

    // ---- actions -------------------------------------------------------------------------------

    /// `setEasyConnectState(update)`.
    pub(super) fn set_easy_connect_state(&mut self, update: Value, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            return;
        }
        self.rpc(
            "/api/updateTailcatState",
            update,
            UPDATE_TAILCAT_TIMEOUT,
            cx,
            |this, result, cx| {
                match result.and_then(|value| read_tailcat_status_result(&value)) {
                    Ok(status) => {
                        this.easy_connect = Some(status);
                        this.request_error = None;
                        // The address is published a moment after the sidecar starts.
                        this.refresh_fast(cx);
                    }
                    Err(error) => this.request_error = Some(error),
                }
                cx.notify();
            },
        );
    }

    /// `enableSshAccess`: one admin prompt; a cancelled or failed attempt shows under the button.
    pub(super) fn enable_ssh_access(&mut self, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) || self.enabling_ssh {
            return;
        }
        self.enabling_ssh = true;
        self.ssh_attempt = None;
        cx.notify();
        self.rpc(
            "/api/enableSshAccess",
            json!({}),
            ENABLE_SSH_TIMEOUT,
            cx,
            |this, result, cx| {
                this.enabling_ssh = false;
                match result {
                    Ok(result) => {
                        let ssh_enabled = result
                            .get("ssh")
                            .and_then(|ssh| ssh.get("enabled"))
                            .and_then(Value::as_bool)
                            == Some(true);
                        if let Some(access) = this.access.as_mut() {
                            access.ssh.enabled = ssh_enabled;
                        }
                        this.ssh_attempt = (!ssh_enabled).then(|| SshEnableAttempt {
                            outcome: result
                                .get("outcome")
                                .and_then(Value::as_str)
                                .unwrap_or("failed")
                                .to_string(),
                            message: result
                                .get("message")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        });
                    }
                    Err(error) => {
                        this.ssh_attempt = Some(SshEnableAttempt {
                            outcome: "failed".to_string(),
                            message: Some(error),
                        });
                    }
                }
                cx.notify();
            },
        );
    }

    /// `installEasyConnect`: one request at a time; the status the daemon returns carries the
    /// progress line while it installs.
    pub(super) fn install_easy_connect(&mut self, cx: &mut Context<Self>) {
        let installing = self
            .easy_connect
            .as_ref()
            .is_some_and(|status| status.installing);
        if !self.rpc_available(cx) || self.install_pending || installing {
            return;
        }
        self.install_pending = true;
        self.install_request_error = None;
        cx.notify();
        self.rpc(
            "/api/installTailcat",
            json!({}),
            INSTALL_TAILCAT_TIMEOUT,
            cx,
            |this, result, cx| {
                this.install_pending = false;
                match result.and_then(|value| read_tailcat_status_result(&value)) {
                    Ok(status) => this.easy_connect = Some(status),
                    Err(error) => this.install_request_error = Some(error),
                }
                cx.notify();
            },
        );
    }

    /// `removePairedDevice`: also drops the device's SSH key on the daemon.
    pub(super) fn remove_paired_device(&mut self, device_id: String, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            return;
        }
        self.removing_device = Some(device_id.clone());
        cx.notify();
        self.rpc(
            "/api/removePairedDevice",
            json!({ "deviceId": device_id }),
            REMOVE_PAIRED_DEVICE_TIMEOUT,
            cx,
            |this, result, cx| {
                this.removing_device = None;
                let pending = result.as_ref().ok().map(read_pending_devices);
                match result.and_then(|value| read_paired_devices_result(&value)) {
                    Ok(devices) => {
                        this.paired_devices = Some(devices);
                        this.pending_devices = pending.unwrap_or_default();
                        this.request_error = None;
                    }
                    Err(error) => this.request_error = Some(error),
                }
                cx.notify();
            },
        );
    }

    pub(super) fn select_section(&mut self, section: RemoteSection, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    /// `onTailscaleEnabledChange`: `applySettingsPatch({ remoteTailscaleEnabled })`.
    pub(super) fn set_tailscale_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let mut patch = serde_json::Map::new();
        patch.insert("remoteTailscaleEnabled".to_string(), json!(enabled));
        self.store.update(cx, |store, cx| {
            store.apply_patch(patch, "settings:control", cx)
        });
    }

    /// Copies `value` and flashes the button's check for 1.2s.
    pub(super) fn copy(&mut self, id: SharedString, value: String, cx: &mut Context<Self>) {
        store_copy_to_clipboard(&self.store, value, cx);
        self.copied.insert(id.clone());
        let clear_id = id.clone();
        self.copy_tasks.insert(
            id,
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(COPIED_FLASH).await;
                let _ = this.update(cx, |this, cx| {
                    this.copied.remove(&clear_id);
                    this.copy_tasks.remove(&clear_id);
                    cx.notify();
                });
            }),
        );
        cx.notify();
    }

    pub(super) fn post(&self, message: Value, cx: &mut App) {
        post_store_message(&self.store, message, cx);
    }

    // ---- saved machines ------------------------------------------------------------------------

    /// `onChange(remoteMachines)`: `applySettingsPatch({ remoteMachines }, 'settings:remoteMachines')`.
    pub(super) fn save_remote_machines(&mut self, machines: Vec<Value>, cx: &mut Context<Self>) {
        let mut patch = serde_json::Map::new();
        patch.insert("remoteMachines".to_string(), Value::Array(machines));
        self.store.update(cx, |store, cx| {
            store.apply_patch(patch, "settings:remoteMachines", cx)
        });
    }

    /// `remoteGxserverInstallState` from native.
    fn receive_install_state(&mut self, message: &Value, cx: &mut Context<Self>) {
        let machine_id = message
            .get("remoteMachineId")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if machine_id.is_empty() {
            return;
        }
        let version = message
            .get("version")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|version| !version.is_empty())
            .map(str::to_string);
        self.installs.insert(
            machine_id.to_string(),
            GxserverInstall {
                installed: message.get("installed").and_then(Value::as_bool) == Some(true),
                version,
            },
        );
        cx.notify();
    }

    /// CDXC:RemoteMachines 2026-08-19:
    /// The saved-machine action reads as Install for a machine without gxserver and as Update for one that already runs it, with the installed version shown on the opposite edge of the same action row. The page never inspects the remote machine itself: it asks native for the state of the saved machine id (`probeRemoteGxserverInstall`) once per SSH target, 600ms after the saved list settles, and renders the version native reports back.
    fn schedule_install_probes(&mut self, cx: &mut Context<Self>) {
        let machines = Value::Array(self.remote_machines(cx));
        if self.probed_list.as_ref() == Some(&machines) {
            return;
        }
        self.probed_list = Some(machines.clone());
        self.probe_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(
                    REMOTE_GXSERVER_INSTALL_PROBE_DEBOUNCE_MS,
                ))
                .await;
            let _ = this.update(cx, |this, cx| {
                for machine in machines.as_array().into_iter().flatten() {
                    let Some(key) = machine_probe_key(machine) else {
                        continue;
                    };
                    if !this.probed_keys.insert(key) {
                        continue;
                    }
                    let id = machine
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    this.installs.remove(&id);
                    this.post(
                        json!({ "remoteMachineId": id, "type": "probeRemoteGxserverInstall" }),
                        cx,
                    );
                }
                cx.notify();
            });
        }));
    }

    // ---- QR codes --------------------------------------------------------------------------------

    /// The encoded grid of `payload`, re-encoded only when the payload rotates.
    pub(super) fn qr_for(&mut self, tailscale: bool, payload: &str) -> Option<QrGrid> {
        let slot = if tailscale {
            &mut self.tailscale_qr
        } else {
            &mut self.easy_connect_qr
        };
        if slot.as_ref().map(|(cached, _)| cached.as_str()) != Some(payload) {
            *slot = Some((payload.to_string(), QrGrid::encode(payload)));
        }
        slot.as_ref().and_then(|(_, grid)| grid.clone())
    }

    // ---- deep links and preview states --------------------------------------------------------------

    /// `initialRemoteSection` opens its tab and scrolls to its card, `initialRemoteMachineId` opens that
    /// machine's edit dialog; each applies once per visit to the page.
    fn apply_deep_links(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_active(cx) {
            return;
        }
        let (section, machine_id) = {
            let request = self.store.read(cx).request();
            (
                request.initial_remote_section.clone(),
                request.initial_remote_machine_id.clone(),
            )
        };
        if let Some(section) = section
            && self.targeted_section.as_deref() != Some(section.as_str())
            && let Some(card) = PathCard::from_section(&section)
        {
            self.targeted_section = Some(section);
            self.section = card.into();
            // Measured again after the tab opens.
            self.card_bounds[match card {
                PathCard::EasyConnect => 0,
                PathCard::Tailscale => 1,
            }]
            .set(None);
            self.pending_scroll = Some(ScrollTarget::Card(card));
        }
        if let Some(machine_id) = machine_id
            && self.targeted_machine.as_deref() != Some(machine_id.as_str())
        {
            let machine = self.remote_machines(cx).into_iter().find(|machine| {
                machine.get("id").and_then(Value::as_str) == Some(machine_id.as_str())
            });
            if let Some(machine) = machine {
                self.targeted_machine = Some(machine_id);
                self.section = RemoteSection::Machines;
                self.open_machine_dialog(Some(&machine), window, cx);
                self.pending_scroll = Some(ScrollTarget::Machines);
            }
        }
        if !self.preview_applied {
            self.preview_applied = true;
            let preview = self.store.read(cx).request().preview_state.clone();
            if let Some(preview) = preview {
                self.apply_preview_state(&preview, window, cx);
            }
        }
    }

    /// The preview binary's page states (dialogs, popovers and disclosures no open message reaches).
    fn apply_preview_state(&mut self, state: &str, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(
            state,
            "remote-easy-connect"
                | "remote-computer"
                | "remote-qr"
                | "remote-ssh-off"
                | "remote-ssh-popover"
                | "remote-ec-off"
                | "remote-paired-confirm"
        ) {
            self.section = RemoteSection::EasyConnect;
        }
        if matches!(
            state,
            "remote-tailscale" | "remote-tailscale-manual" | "remote-tailscale-qr"
        ) {
            self.section = RemoteSection::Tailscale;
        }
        if matches!(
            state,
            "remote-machines" | "remote-add-machine" | "remote-add-ec" | "remote-edit-machine"
        ) {
            self.section = RemoteSection::Machines;
        }
        match state {
            "remote-computer" => self.connection_device = "computer",
            "remote-qr" | "remote-tailscale-qr" => {
                self.qr_preview = Some(if state == "remote-qr" {
                    PathCard::EasyConnect
                } else {
                    PathCard::Tailscale
                });
                self.qr_preview_focus.focus(window, cx);
            }
            "remote-ssh-popover" => {
                self.os_popover = Some(("easy-connect-ssh".into(), "windows"));
            }
            "remote-tailscale-manual" => self.manual_open = true,
            "remote-paired-confirm" => self.confirming_device = Some("dev-1".to_string()),
            "remote-advanced" => {
                self.section = RemoteSection::Advanced;
                self.advanced.raw_open = true;
            }
            "remote-add-machine" => self.open_machine_dialog(None, window, cx),
            "remote-add-ec" => {
                self.open_machine_dialog(None, window, cx);
                machines::preview_paste_easy_connect_code(self, window, cx);
            }
            _ => {}
        }
        cx.notify();
    }
}

impl Render for RemoteTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_deep_links(window, cx);
        let t = self.tokens(cx);
        let (search, searching, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.tab_search(SettingsTabId::Remote),
                store.is_searching(),
                matching,
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if searching && !search.has_matches() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &t.p,
                SettingsTabId::Remote,
                &matching,
                move |tab, _window, cx| {
                    store.update(cx, |store, cx| store.set_active_tab(tab, cx));
                },
            )));
        } else {
            // Each tab answers the Settings search on its own; a search that matches nothing on the
            // open tab lands on the first tab that has a match.
            let shown = RemoteSection::ALL
                .map(|section| should_show_section(&search.section(section.anchor()), None));
            if searching
                && !shown[self.section.index()]
                && let Some(first) = RemoteSection::ALL
                    .into_iter()
                    .find(|section| shown[section.index()])
            {
                self.section = first;
            }
            let rpc = self.rpc_available(cx);
            // `.settings-management-layout`: the sections 16px apart in one column.
            let mut layout = v_flex().w_full().min_w_0().gap(px(SECTION_GAP));
            if self.pair_again_waiting > 0 {
                layout = layout.child(pair_again::pair_again_line(&t, self.pair_again_waiting, cx));
            }
            layout = layout.child(self.section_tabs(&t, cx));
            layout = layout.child(match self.section {
                RemoteSection::EasyConnect | RemoteSection::Tailscale => {
                    self.connect_section(&t, rpc, window, cx)
                }
                RemoteSection::Machines => {
                    let section = machines::machines_section(self, &t, window, cx);
                    div()
                        .w_full()
                        .on_children_prepainted(capture_child_bounds(
                            self.machines_bounds.clone(),
                            0,
                        ))
                        .child(section)
                        .into_any_element()
                }
                RemoteSection::Advanced if rpc => advanced::advanced_card(self, &t, window, cx),
                RemoteSection::Advanced => no_server_line(&t),
            });
            blocks.push(PageBlock {
                anchor: Some("remote".to_string()),
                element: layout.into_any_element(),
                margin_top: FIRST_BLOCK_MARGIN,
            });
        }
        self.apply_pending_scroll(window, cx);
        let mut overlays = Vec::new();
        if let Some(dialog) = machines::machine_dialog(self, &t, window, cx) {
            overlays.push(dialog);
        }
        if let Some(dialog) = easy_connect::qr_preview_dialog(self, &t, window, cx) {
            overlays.push(dialog);
        }
        div()
            .size_full()
            .child(settings_page(
                &self.store,
                SettingsTabId::Remote,
                &t.p,
                blocks,
                cx,
            ))
            .children(overlays)
    }
}

/// `.settings-management-layout { gap: 16px }`.
const SECTION_GAP: f32 = 16.0;
/// `.settings-remote-from-phone { gap: 12px }` (and the machines section's).
const HEADER_GAP: f32 = 12.0;
/// The space above the page's column (`.settings-tab-scroll`'s top padding).
const FIRST_BLOCK_MARGIN: f32 = 12.0;

/// Where a deep link scrolls once the page is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScrollTarget {
    /// `scrollIntoView({ block: 'start' })` on a path card.
    Card(PathCard),
    /// `scrollIntoView({ block: 'center' })` on the machine tiles.
    Machines,
}

impl RemoteTab {
    /// How many saved machines are connected right now, and how many are saved.
    pub(super) fn connected_machine_count(&self, cx: &App) -> (usize, usize) {
        let machines = self.remote_machines(cx);
        let connected = machines
            .iter()
            .filter_map(|machine| machine.get("id").and_then(Value::as_str))
            .filter(|id| self.connect_states.get(*id).map(String::as_str) == Some("connected"))
            .count();
        (connected, machines.len())
    }

    /// The tab strip: each tab's name and its status (the card badge's dot and word for Easy
    /// Connect and Tailscale, "connected/saved" for Remote machines, "Unsaved" for Advanced while an
    /// edit is pending).
    fn section_tabs(&mut self, t: &RemoteTokens, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (connected, saved) = self.connected_machine_count(cx);
        let tailscale = tailscale::tailscale_status_badge(self, cx);
        let easy_connect = easy_connect_status_badge(self.easy_connect.as_ref());
        let items: Vec<ModalRailItem> = RemoteSection::ALL
            .iter()
            .map(|section| {
                let (trailing, dot): (Option<SharedString>, Option<gpui::Rgba>) = match section {
                    RemoteSection::EasyConnect => {
                        (Some(easy_connect.0.into()), Some(t.tone(easy_connect.1)))
                    }
                    RemoteSection::Tailscale => tailscale
                        .map(|(label, tone)| (Some(label.into()), Some(t.tone(tone))))
                        .unwrap_or((None, None)),
                    RemoteSection::Machines => (
                        (saved > 0).then(|| format!("{connected}/{saved}").into()),
                        None,
                    ),
                    RemoteSection::Advanced => {
                        if self.advanced.has_draft() {
                            (Some("Unsaved".into()), Some(t.tone(BadgeTone::NeedsSetup)))
                        } else {
                            (None, None)
                        }
                    }
                };
                ModalRailItem {
                    label: section.label().into(),
                    trailing,
                    dot,
                }
            })
            .collect();
        modal_bordered_segmented_control(
            t.foreground,
            t.edge(0.90),
            t.muted,
            "remote-section-tabs",
            &items,
            self.section.index(),
            |tab: &mut Self, index, _window, cx| {
                if let Some(section) = RemoteSection::ALL.get(index) {
                    tab.select_section(*section, cx);
                }
            },
            cx,
        )
        .w_full()
        .into_any_element()
    }

    /// The Easy Connect or Tailscale tab: its card, open, and the request error.
    fn connect_section(
        &mut self,
        t: &RemoteTokens,
        rpc: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mut section = v_flex().w_full().min_w_0().gap(px(HEADER_GAP));
        if !rpc {
            section = section.child(no_server_line(t));
        } else {
            let (card, bounds) = if self.section == RemoteSection::Tailscale {
                (
                    tailscale::tailscale_card(self, t, window, cx),
                    self.card_bounds[1].clone(),
                )
            } else {
                (
                    easy_connect::easy_connect_card(self, t, window, cx),
                    self.card_bounds[0].clone(),
                )
            };
            section = section.child(
                div()
                    .w_full()
                    .on_children_prepainted(capture_child_bounds(bounds, 0))
                    .child(card),
            );
        }
        if let Some(error) = self.request_error.clone() {
            section = section.child(style::error_line(t, error));
        }
        section.into_any_element()
    }

    /// Scrolls a deep link's card or tiles into view once the page shows them, the way React ran
    /// `scrollIntoView({ behavior: 'smooth' })` after a frame.
    fn apply_pending_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.pending_scroll else {
            return;
        };
        let cell = match target {
            ScrollTarget::Card(PathCard::EasyConnect) => self.card_bounds[0].clone(),
            ScrollTarget::Card(PathCard::Tailscale) => self.card_bounds[1].clone(),
            ScrollTarget::Machines => self.machines_bounds.clone(),
        };
        let Some(bounds) = cell.get() else {
            cx.on_next_frame(window, |_, _, cx| cx.notify());
            return;
        };
        self.pending_scroll = None;
        let (handle, tracker) = self.store.update(cx, |store, _| {
            (
                store.scroll_handle(SettingsTabId::Remote),
                store.tracker(SettingsTabId::Remote),
            )
        });
        let viewport = handle.bounds();
        let offset = handle.offset();
        let content_top = bounds.origin.y - viewport.origin.y - offset.y;
        let target_top = match target {
            ScrollTarget::Machines => {
                content_top - (viewport.size.height - bounds.size.height) / 2.0
            }
            ScrollTarget::Card(_) => content_top,
        };
        let to = f32::from(target_top.max(px(0.0)).min(handle.max_offset().y));
        tracker.borrow_mut().smooth = Some(SmoothScroll::new(-f32::from(offset.y), to));
        window.request_animation_frame();
    }
}

/// The line a pairing tab shows when the page has no gxserver to talk to.
fn no_server_line(t: &RemoteTokens) -> gpui::AnyElement {
    div()
        .w_full()
        .text_size(px(13.0))
        .line_height(px(19.5))
        .text_color(gpui::Hsla::from(t.muted))
        .child(
            "Pairing needs the Ghostex server on this computer. Open Settings from the Ghostex app to set up your devices.",
        )
        .into_any_element()
}
