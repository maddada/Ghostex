//! The "Connect a device" window (sidebar menu > Connect a Device, Settings > Remote's Connect a
//! Device and Connect a Computer): it turns Easy Connect on by itself, shows the code, turns into
//! the three words and Approve the moment a phone scans, and ends on "<phone> is connected".
//!
//! CDXC:RemotePairing 2026-11-11 DECISION:
//! User, on the Connect a device page (Q2490 "C. Three ways down the side", and "do what's shown in the option I picked above" for the Phone and Computer choice, Tailscale, Copy the code and the Settings button): the window is one title, "Connect a device" whichever way is picked, over a list of three ways at the left (Phone "Scan a code", Computer "Paste a code", Tailscale "Your own network") with the picked way at the right, and a footer with a button to Settings > Remote at the left, called "Remote", and "Need the app? Android · iPhone" at the right. A phone's code sits at the top of its way with Copy the code right under it, then the computer's name, the waiting line and the steps. His note on the code: "keep in mind the qr needs to not actually show there cuz too small, i need to click on it to show larger so can be scanned", so the code in the list is small and a click on it (it says "Click to enlarge") fills the window with it at 432px; a click on the large code or Escape goes back, and a phone that scans still turns the window into "Is this your phone?". Tailscale is a way of its own here (ways.rs, tailscale_pane.rs): what it is for, the checklist Settings > Remote's Tailscale row shows, and a button to that row for the code. Supersedes, for the layout only, the 2026-11-10 decision below (the code at 432px in the window, Copy the code beside "Need the app?", and the quiet line that named Tailscale and another computer); what the window does is unchanged.
//!
//! CDXC:RemotePairing 2026-11-10 DECISION:
//! User, on the pairing design page (Q2178 to Q2186, the suggested screen for steps 1, 2, 3, 7 and 8): one "Connect a phone" window that opens on the code replaces the Remote Setup dialog, the stop at Settings > Remote and the separate Show QR popup. The menu item opens it straight away and it turns Easy Connect on by itself (installing the helper and turning on SSH access first when they are missing, as Remote Setup's Connect did). Two plain steps that match the phone's button word for word, a line that shows it is waiting, the code at 432px (CDXC:RemotePairing 2026-10-27 in settings_modal/fields/qr.rs), "Need the app?" as one line and Copy the code. His note on step 1: "we need bit more there tho, tell the user we can use tailscale and we can connect a laptop to another laptop", so one quiet line under it says both, each leading to its place (Tailscale opens Settings > Remote's Tailscale, "another computer" turns this window into Connect a computer with its own two lines). When a phone scans, the same window becomes "Is this your phone?" with the three words as its biggest thing and Not my phone beside Approve; after Approve it says "<phone> is connected" with Connect another phone and Done. Approve follows the approvals rule (one plain click; Settings > Security's switch adds Touch ID or the password first, `owner_security::plain`).
//! SEE-ALSO: apps/desktop/src/app/connect_device_modal_lifecycle.rs (the gxserver calls, the approval, the approval card kept away while this window shows the request), apps/desktop/src/app/owner_security/ui.rs (the "Is this your phone?" notice for a request that arrives while this window is closed), apps/desktop/src/app/window/settings_modal/tabs/remote/paired_devices.rs (the same request's row in Settings > Remote), apps/desktop/src/bin/native_modal_demo/connect_device.rs (preview).
use super::native_modal_kit::*;
use super::settings_modal::fields::{QrGrid, qr_code};
use super::settings_modal::tabs::remote::SshComputerForm;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, Div, Entity, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::Value;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

#[path = "connect_device_modal/computer_way.rs"]
mod computer_way;
#[path = "connect_device_modal/ssh_tab.rs"]
mod ssh_tab;
#[path = "connect_device_modal/tailscale_pane.rs"]
mod tailscale_pane;
#[path = "connect_device_modal/ways.rs"]
mod ways;

pub(crate) use computer_way::{BackStep, JoinTarget, join_target};
use computer_way::{ComputerInputs, ComputerTab, JoinFlow};
pub(crate) use tailscale_pane::TailscalePane;
pub(crate) use ways::ConnectWay;

/// The window's one size: wide enough for the list of ways beside a way, and for the large code.
/// The app's window list (`GpuiAppModalKind::default_window_size`) reads the same two numbers.
pub(crate) const CONNECT_DEVICE_MODAL_WIDTH: f32 = 560.0;
/// First-frame height only. The window is resized to the measured layout as soon as the first
/// prepaint reports it.
pub(crate) const CONNECT_DEVICE_MODAL_INITIAL_HEIGHT: f32 = 560.0;

/// The large code's side: 432px, so an Android scanner gets large modules (CDXC:RemotePairing
/// 2026-10-27 in settings_modal/fields/qr.rs).
const QR_LARGE_SIZE: f32 = 432.0;
/// The code's side in the list of ways: a picture of the code to click, too small to scan
/// (CDXC:RemotePairing 2026-11-11 DECISION above). The "Getting a code ready" square is as tall,
/// so the window doesn't jump when the code arrives.
const QR_SMALL_SIZE: f32 = 196.0;
/// How often the window asks for the code and for a waiting phone: every second while it is open
/// (the Show QR popup asked every four, so the phone showed its words first and the computer
/// looked dead).
const POLL_EVERY: Duration = Duration::from_secs(1);
const COPIED_RESET: Duration = Duration::from_millis(1_600);

const ICON_COPY: &str = "modals/settings/copy.svg";
const ICON_CHECK: &str = "modals/settings/check.svg";
const ICON_PHONE: &str = "modals/settings/device-mobile.svg";
const ICON_COMPUTER: &str = "modals/settings/device-desktop.svg";
const ICON_CONNECTED: &str = "modals/settings/circle-check-filled.svg";
const ICON_HELPER: &str = "modals/settings/plug-connected.svg";
const ICON_ALERT: &str = "modals/settings/alert-triangle.svg";
const ICON_TAILSCALE: &str = "modals/settings/world.svg";
const ICON_SETTINGS: &str = "modals/settings/settings.svg";
const ICON_ENLARGE: &str = "modals/settings/arrows-maximize.svg";

/// The short website link that redirects to the latest Android APK.
pub(crate) const GHOSTEX_ANDROID_INSTALL_URL: &str = "https://ghostex.dev/android";
/// Ghostex for iPhone on the App Store.
pub(crate) const GHOSTEX_IPHONE_INSTALL_URL: &str = "https://apps.apple.com/app/id6771707394";

/// What the window connects: a phone (the QR) or another computer (the code to paste there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConnectDeviceKind {
    Phone,
    Computer,
}

impl ConnectDeviceKind {
    /// The open message's `device` value (`phone` when absent).
    pub(crate) fn from_open_message(message: &serde_json::Value) -> Self {
        match message.get("device").and_then(serde_json::Value::as_str) {
            Some("computer") => Self::Computer,
            _ => Self::Phone,
        }
    }
}

/// This computer's Easy Connect code, while it can pair (it carries the one-time secret).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConnectCode {
    /// What the QR encodes (the link).
    pub(crate) payload: String,
    /// The bare code Copy the code puts on the clipboard.
    pub(crate) text: String,
}

/// A phone or computer that used the code and waits for Approve, with its three words.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WaitingDevice {
    pub(crate) name: String,
    /// "iPhone", "Android" or "computer": what the device says it is, in the user's words.
    pub(crate) kind_label: String,
    pub(crate) phone: bool,
    pub(crate) words: Vec<String>,
    pub(crate) approval_id: String,
}

/// One answer to [`ConnectDeviceCommand::Poll`].
#[derive(Clone, Debug, Default)]
pub(crate) struct ConnectPoll {
    pub(crate) code: Option<ConnectCode>,
    /// This computer's name, as the phone will show it.
    pub(crate) computer_name: Option<String>,
    pub(crate) waiting: Vec<WaitingDevice>,
    /// The ids of the devices paired now, to tell when an approved one has finished pairing.
    pub(crate) paired_ids: Vec<String>,
    pub(crate) error: Option<String>,
}

/// What the window asks its host to do. The window removes its own window before `Close`,
/// `OpenRemote` and `OpenTailscale`.
pub(crate) enum ConnectDeviceCommand {
    /// Install the helper if needed and turn Easy Connect on; the host reports each step with
    /// `prepare_progress`, says whether SSH access is on with `ssh_checked`, and answers with
    /// `prepared`. It never turns SSH access on: only `EnableSsh` does.
    Prepare,
    /// Turn SSH access on (the window's Turn on SSH button, after it said what that does); the
    /// host answers `ssh_turned_on`. `install`: the Install button on Linux, which installs a
    /// missing OpenSSH server first (`/api/enableSshAccess`'s `install`).
    EnableSsh {
        install: bool,
    },
    /// Install the Easy Connect helper (the window's Install button); the host reports each step
    /// with `helper_install_progress` and answers with `helper_installed` or `needs_helper`.
    InstallHelper,
    /// Read the code and the waiting devices (opening the door for a new phone while this window
    /// shows); the host answers with `poll_answered`.
    Poll,
    /// Approve or deny a waiting device's request; the host answers with `decided`.
    Decide {
        approval_id: String,
        approve: bool,
    },
    /// The code was copied: play the copy sound.
    Copied,
    OpenExternalUrl(String),
    /// Read how Tailscale stands on this computer for the Tailscale way; the host answers with
    /// `tailscale_answered`.
    ReadTailscale,
    /// Settings > Remote (the footer's Remote button).
    OpenRemote,
    /// Settings > Remote with its Tailscale row open.
    OpenTailscale,
    /// Save the computer the SSH details tab describes (`machine`, a saved-computer entry) and its
    /// `password` for the keychain, then connect it; the window has removed itself.
    AddSshComputer {
        machine: Value,
        password: String,
    },
    /// Pair this computer with the one behind `code` (Connect); the host answers `join_started`.
    JoinComputer {
        code: String,
    },
    /// Ask how that pairing stands; the host answers `join_status`.
    JoinStatus {
        pending_id: String,
    },
    /// Stop waiting for that pairing.
    CancelJoin {
        pending_id: String,
    },
    /// The other computer approved: save it here; the host answers `join_saved`.
    SaveJoined {
        target: JoinTarget,
        key_installed: bool,
    },
    /// Read how signing in to the saved computer stands; the host answers `sign_in_state`.
    ReadSignIn {
        remote_machine_id: String,
    },
    /// Save that computer's username and password, then sign in again.
    SubmitPassword {
        remote_machine_id: String,
        user: String,
        password: String,
    },
    /// Sign in to the saved computer again.
    Reconnect {
        remote_machine_id: String,
    },
    /// Ask once to manage the computer just joined (its terminal let this one in while only its
    /// own app owned it); the host answers `join_manage_answer`.
    AskToManage {
        remote_machine_id: String,
        /// The one-time secret that computer gave this one with the join, proven through this
        /// app's SSH login there (CDXC:Security 2026-11-17 DECISION at `terminal_may_approve`).
        handover: Option<String>,
    },
    /// Connect both ways with this computer's own `code`; the host answers
    /// `connect_back_progress`.
    ConnectBack {
        remote_machine_id: String,
        code: String,
    },
    Close,
}

pub(crate) type ConnectDeviceHost = Rc<dyn Fn(ConnectDeviceCommand, &mut App)>;

pub(crate) struct ConnectDeviceConfig {
    pub(crate) palette: ModalPalette,
    pub(crate) kind: ConnectDeviceKind,
    /// This computer's name, until gxserver says it.
    pub(crate) computer_name: String,
    /// `Settings.remoteTailscaleEnabled`: off, the Tailscale way says so until it is read again.
    pub(crate) tailscale_enabled: bool,
    /// Approve asks for Touch ID or the password first (Settings > Security, or an older gxserver).
    pub(crate) require_presence: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum Stage {
    /// Turning things on; the line says which step.
    Preparing(Option<String>),
    /// Turning on failed; the message and Try again.
    Failed(String),
    /// The Easy Connect helper isn't on this computer: what that means and Install (with why the
    /// last install failed, when it did).
    NeedsHelper(Option<String>),
    /// The helper is installing; the line says which step.
    InstallingHelper(Option<String>),
    /// The code (or "Getting a code ready…" until it arrives), and a waiting device when one asks.
    Ready,
    /// Approved: waiting for the device to finish pairing on its next ask.
    Finishing {
        device: WaitingDevice,
        paired_before: HashSet<String>,
    },
    Connected(WaitingDevice),
}

/*
CDXC:RemotePairing 2026-11-18 DECISION:
GitHub #663: Connect a Computer kept "The other computer's code" box locked until this computer passed an SSH check, and on Windows opening the window tried to install OpenSSH Server and open port 22 in the firewall on its own; on a managed company PC the install failed and the box never unlocked. User's pick: "The code box must not wait on the SSH check, and nothing may be installed without the user agreeing first." Pasting another computer's code only needs outgoing connections (`/api/pairComputer` isn't a door call), so the box unlocks once Easy Connect is on. SSH access is needed only when something connects to this computer: this computer's own code (a phone or another computer uses it) and Connect both ways. While SSH is off, the square that code appears in says so, what turning it on does on this OS, and offers Turn on SSH; only that click (or Settings > Remote's own button) calls `/api/enableSshAccess`. Who may pair, and the owner-only code and approvals, are unchanged.
*/
/// Whether SSH access lets other devices connect to this computer, as this window knows it.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum SshAccess {
    /// On, or not read (an older gxserver): the code shows as it always did.
    On,
    /// Off: the code's square asks to turn it on.
    Off,
    /// The Turn on SSH click is running (the OS may be asking for a password).
    TurningOn,
    /// The Install click is running: installing the OpenSSH server, then starting it (Linux).
    Installing,
    /// Turning it on failed; why.
    Failed(String),
}

/*
CDXC:RemotePairing 2026-11-18 DECISION:
On Linux, Turn on SSH was a dead end when the OpenSSH server wasn't installed: it failed with a
list of three install commands and nothing to click. User's pick (Q3975): "give ppl both options
(copy and install)". While gxserver says the server isn't installed, the SSH step shows this
distribution's one install command with Copy, and Install, which installs and starts it behind one
admin password prompt (server/src/remote_access/ssh_install.rs). When the server is installed but
stopped, the step says so. Only the clicks change anything, and the #663 rule stays: the paste
box and the Headless server tab never wait on SSH. macOS and Windows never get these facts, so
their wording is unchanged.
*/
/// What gxserver knows about this computer's SSH server beside whether it answers (Linux only).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SshServer {
    /// `Some(false)`: the OpenSSH server isn't installed; `None`: unknown, or not Linux.
    pub(crate) installed: Option<bool>,
    /// This distribution's install command, while it isn't installed and the package manager is
    /// known.
    pub(crate) install_command: Option<String>,
}

impl SshServer {
    /// The `ssh` object of a `/api/remoteAccessStatus` or `/api/enableSshAccess` answer.
    pub(crate) fn of_status(ssh: &Value) -> Self {
        Self {
            installed: ssh.get("installed").and_then(Value::as_bool),
            install_command: ssh
                .get("installCommand")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|command| !command.is_empty())
                .map(str::to_string),
        }
    }
}

pub(crate) struct GpuiConnectDeviceWindow {
    host: ConnectDeviceHost,
    palette: ModalPalette,
    /// Which of the three ways shows at the right of the list.
    way: ConnectWay,
    /// The phone's code fills the window, large enough to scan.
    large_code: bool,
    computer_name: String,
    tailscale: TailscalePane,
    /// Reads Tailscale again every few seconds while its way shows.
    tailscale_refresh: Option<Task<()>>,
    require_presence: bool,
    stage: Stage,
    /// SSH access on this computer: this computer's own code waits for it, pasting doesn't.
    ssh: SshAccess,
    /// Whether the SSH server is installed here, and its install command (Linux).
    ssh_server: SshServer,
    code: Option<ConnectCode>,
    qr: Option<(String, Option<QrGrid>)>,
    waiting: Option<WaitingDevice>,
    /// Requests answered here, kept out of a poll answer that was already on its way.
    answered: HashSet<String>,
    paired_ids: HashSet<String>,
    deciding: bool,
    decide_error: Option<String>,
    poll_error: Option<String>,
    poll_in_flight: bool,
    polling: Option<Task<()>>,
    copied: bool,
    copied_generation: u64,
    fit: ModalScrollFit,
    /// Which screen the window was last sized for: a new one (the words, connected, another
    /// way, the large code, the helper's screens) is measured again, so the window fits each
    /// screen instead of the first.
    fitted_screen: Option<(u8, ConnectWay, ComputerTab, u8)>,
    focus_handle: FocusHandle,
    /// Opened as Connect a computer: no list of ways, only the computer's pane.
    computer_only: bool,
    /// Which tab of the computer's pane shows: Code, Headless server or SSH details.
    computer_tab: ComputerTab,
    /// The SSH details tab's form, built the first time that tab shows.
    ssh_form: Option<SshComputerForm>,
    computer_inputs: ComputerInputs,
    /// Why the pasted code can't be used.
    code_error: Option<String>,
    /// The pairing this computer started by entering another computer's code.
    join: Option<JoinFlow>,
    join_polling: Option<Task<()>>,
    join_poll_in_flight: bool,
    /// Asks left in which a failure is the attempt before the new one (computer_way.rs).
    stale_failure_polls: u8,
}

impl GpuiConnectDeviceWindow {
    pub(crate) fn new(
        config: ConnectDeviceConfig,
        host: ConnectDeviceHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let computer_inputs = ComputerInputs::new(window, cx);
        let this = Self {
            host,
            palette: config.palette,
            way: ConnectWay::of_kind(config.kind),
            large_code: false,
            computer_name: config.computer_name,
            tailscale: TailscalePane::new(config.tailscale_enabled),
            tailscale_refresh: None,
            require_presence: config.require_presence,
            stage: Stage::Preparing(None),
            ssh: SshAccess::On,
            ssh_server: SshServer::default(),
            code: None,
            qr: None,
            waiting: None,
            answered: HashSet::new(),
            paired_ids: HashSet::new(),
            deciding: false,
            decide_error: None,
            poll_error: None,
            poll_in_flight: false,
            polling: None,
            copied: false,
            copied_generation: 0,
            fit: ModalScrollFit::new(),
            fitted_screen: None,
            focus_handle,
            computer_only: config.kind == ConnectDeviceKind::Computer,
            computer_tab: ComputerTab::Code,
            ssh_form: None,
            computer_inputs,
            code_error: None,
            join: None,
            join_polling: None,
            join_poll_in_flight: false,
            stale_failure_polls: 0,
        };
        (this.host)(ConnectDeviceCommand::Prepare, cx);
        this
    }

    // ---- the host's answers ----------------------------------------------------------------------

    /// A step of turning things on started ("Installing Easy Connect…").
    pub(crate) fn prepare_progress(&mut self, note: String, cx: &mut Context<Self>) {
        if matches!(self.stage, Stage::Preparing(_)) {
            self.stage = Stage::Preparing(Some(note));
            cx.notify();
        }
    }

    /*
    CDXC:RemotePairing 2026-11-11 DECISION:
    GitHub #559: the window opened on "Getting a code ready" and stayed there on a Mac without the Easy Connect helper, with "Reinstall Ghostex" in Settings, which cannot bring the helper back (the Mac app doesn't carry it, by the user's choice: "i dont want to bundle tailcat tho since it'll increase size of app"). The square the code appears in now says the helper isn't installed yet, with one Install button, each step of the install while it runs, a plain error with Try again when it fails, and what works meanwhile; the code appears in the same window once the helper is in.
    */
    /// The Easy Connect helper is missing (`error`: why the last install failed).
    pub(crate) fn needs_helper(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.stage = Stage::NeedsHelper(error);
        cx.notify();
    }

    /// A step of the helper's install ("Downloading the build tools… 40%").
    pub(crate) fn helper_install_progress(&mut self, note: Option<String>, cx: &mut Context<Self>) {
        if matches!(
            self.stage,
            Stage::InstallingHelper(_) | Stage::NeedsHelper(_) | Stage::Preparing(_)
        ) {
            self.stage = Stage::InstallingHelper(note);
            cx.notify();
        }
    }

    /// The helper is in: the rest of turning on runs (`prepare_progress`, then `prepared`).
    pub(crate) fn helper_installed(&mut self, cx: &mut Context<Self>) {
        self.stage = Stage::Preparing(Some("Turning on Easy Connect…".to_string()));
        cx.notify();
    }

    /// Whether SSH access is on here, read while turning things on.
    pub(crate) fn ssh_checked(&mut self, on: bool, cx: &mut Context<Self>) {
        if !self.ssh_busy() {
            self.ssh = if on { SshAccess::On } else { SshAccess::Off };
            cx.notify();
        }
    }

    /// The answer to Turn on SSH: on, still off (the admin prompt was cancelled), or why it failed.
    pub(crate) fn ssh_turned_on(&mut self, result: Result<bool, String>, cx: &mut Context<Self>) {
        self.ssh = match result {
            Ok(true) => SshAccess::On,
            Ok(false) => SshAccess::Off,
            Err(message) => SshAccess::Failed(message),
        };
        cx.notify();
    }

    /// What gxserver says about the SSH server (read with SSH access, and again after a click).
    pub(crate) fn ssh_server_read(&mut self, server: SshServer, cx: &mut Context<Self>) {
        self.ssh_server = server;
        cx.notify();
    }

    /// Turn on SSH or Install is running.
    pub(super) fn ssh_busy(&self) -> bool {
        matches!(self.ssh, SshAccess::TurningOn | SshAccess::Installing)
    }

    /// The OpenSSH server isn't installed here (Linux), so Turn on SSH alone can't help.
    pub(super) fn ssh_server_missing(&self) -> bool {
        self.ssh_server.installed == Some(false)
    }

    /// This computer's own code waits for SSH access (pasting another computer's code doesn't).
    pub(super) fn own_code_needs_ssh(&self) -> bool {
        self.ssh != SshAccess::On
    }

    pub(super) fn turn_on_ssh(&mut self, cx: &mut Context<Self>) {
        self.start_ssh(false, cx);
    }

    /// The preview binary's `computer-ssh-installing` state: Install clicked.
    #[allow(dead_code)]
    pub(crate) fn preview_install_ssh(&mut self, cx: &mut Context<Self>) {
        self.install_ssh(cx);
    }

    /// Install: install the OpenSSH server, then start it (one admin prompt).
    pub(super) fn install_ssh(&mut self, cx: &mut Context<Self>) {
        self.start_ssh(true, cx);
    }

    fn start_ssh(&mut self, install: bool, cx: &mut Context<Self>) {
        if self.ssh_busy() {
            return;
        }
        self.ssh = if install {
            SshAccess::Installing
        } else {
            SshAccess::TurningOn
        };
        cx.notify();
        (self.host)(ConnectDeviceCommand::EnableSsh { install }, cx);
    }

    /// Easy Connect is on (or couldn't be turned on): start asking for the code.
    pub(crate) fn prepared(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.stage = Stage::Ready;
                self.start_polling(cx);
            }
            Err(message) => self.stage = Stage::Failed(message),
        }
        cx.notify();
    }

    pub(crate) fn poll_answered(&mut self, poll: ConnectPoll, cx: &mut Context<Self>) {
        self.poll_in_flight = false;
        self.poll_error = poll.error;
        if let Some(name) = poll.computer_name.filter(|name| !name.trim().is_empty()) {
            self.computer_name = name;
        }
        // Without SSH access nothing could use this computer's code, so it isn't shown (or sent
        // by Connect both ways) until SSH is on.
        if self.own_code_needs_ssh() {
            self.code = None;
        } else if poll.code.is_some() || self.code.is_none() {
            self.code = poll.code;
        }
        self.paired_ids = poll.paired_ids.into_iter().collect();
        let answered = &self.answered;
        self.waiting = poll
            .waiting
            .into_iter()
            .find(|device| !answered.contains(&device.approval_id));
        if let Stage::Finishing {
            device,
            paired_before,
        } = &self.stage
            && self.paired_ids.iter().any(|id| !paired_before.contains(id))
        {
            self.stage = Stage::Connected(device.clone());
        }
        cx.notify();
    }

    /// The answer to Approve or Not my phone.
    pub(crate) fn decided(
        &mut self,
        approval_id: String,
        approved: bool,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        self.deciding = false;
        match result {
            Ok(()) => {
                self.answered.insert(approval_id.clone());
                let device = self
                    .waiting
                    .take()
                    .filter(|device| device.approval_id == approval_id);
                if approved && let Some(device) = device {
                    self.stage = Stage::Finishing {
                        device,
                        paired_before: self.paired_ids.clone(),
                    };
                }
                self.decide_error = None;
            }
            Err(message) => self.decide_error = Some(message),
        }
        cx.notify();
    }

    // ---- actions ---------------------------------------------------------------------------------

    fn start_polling(&mut self, cx: &mut Context<Self>) {
        if self.polling.is_some() {
            return;
        }
        self.poll(cx);
        self.polling = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_EVERY).await;
                if this.update(cx, |this, cx| this.poll(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        if self.poll_in_flight || (matches!(self.stage, Stage::Connected(_)) && self.join.is_none())
        {
            return;
        }
        self.poll_in_flight = true;
        (self.host)(ConnectDeviceCommand::Poll, cx);
    }

    fn install_helper(&mut self, cx: &mut Context<Self>) {
        self.stage = Stage::InstallingHelper(None);
        cx.notify();
        (self.host)(ConnectDeviceCommand::InstallHelper, cx);
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        self.stage = Stage::Preparing(None);
        cx.notify();
        (self.host)(ConnectDeviceCommand::Prepare, cx);
    }

    fn decide(&mut self, approve: bool, cx: &mut Context<Self>) {
        let Some(device) = self.waiting.as_ref() else {
            return;
        };
        if self.deciding {
            return;
        }
        self.deciding = true;
        self.decide_error = None;
        cx.notify();
        (self.host)(
            ConnectDeviceCommand::Decide {
                approval_id: device.approval_id.clone(),
                approve,
            },
            cx,
        );
    }

    fn connect_another(&mut self, cx: &mut Context<Self>) {
        self.stage = Stage::Ready;
        cx.notify();
    }

    /// What the picked way connects; the Tailscale way has no code, and asks as a phone does.
    fn kind(&self) -> ConnectDeviceKind {
        if self.computer_only {
            return ConnectDeviceKind::Computer;
        }
        match self.way {
            ConnectWay::Computer => ConnectDeviceKind::Computer,
            ConnectWay::Phone | ConnectWay::Tailscale => ConnectDeviceKind::Phone,
        }
    }

    fn copy_code(&mut self, cx: &mut Context<Self>) {
        let Some(code) = self.code.as_ref() else {
            return;
        };
        let text = code.text.clone();
        self.copy_text(text, cx);
    }

    /// Puts `text` on the clipboard and shows "Copied" for a moment.
    fn copy_text(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        (self.host)(ConnectDeviceCommand::Copied, cx);
        self.copied = true;
        self.copied_generation = self.copied_generation.wrapping_add(1);
        let generation = self.copied_generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_RESET).await;
            let _ = this.update(cx, |this, cx| {
                if this.copied_generation == generation {
                    this.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn open_url(&mut self, url: &'static str, cx: &mut Context<Self>) {
        (self.host)(ConnectDeviceCommand::OpenExternalUrl(url.to_string()), cx);
    }

    fn close_and_send(
        &mut self,
        command: ConnectDeviceCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.polling = None;
        self.tailscale_refresh = None;
        self.join_polling = None;
        window.remove_window();
        (self.host)(command, cx);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_and_send(ConnectDeviceCommand::Close, window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() == "escape" {
            // Escape leaves the large code first; the next one closes the window.
            if self.shows_large_code() {
                self.set_large_code(false, cx);
            } else {
                self.close(window, cx);
            }
            cx.stop_propagation();
        }
    }

    // ---- drawing ---------------------------------------------------------------------------------

    fn title(&self) -> String {
        if let Some(title) = self.join_title() {
            return title;
        }
        match (&self.stage, &self.waiting) {
            (Stage::Connected(device), _) => format!("{} is connected", device.name),
            (Stage::Ready, Some(device)) => pairing_question_named(device.phone, &device.name),
            _ if self.computer_only => "Connect a computer".to_string(),
            _ => "Connect a device".to_string(),
        }
    }

    fn muted(&self, text: impl Into<SharedString>) -> Div {
        div()
            .text_size(px(13.0))
            .line_height(px(19.0))
            .text_color(hsla(self.palette.muted))
            .child(text.into())
    }

    /// A word that does something inside a line of text: the foreground colour, underlined on hover.
    fn link(
        &self,
        id: &'static str,
        label: &'static str,
        on_press: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .role(gpui::Role::Link)
            .aria_label(label)
            .text_color(hsla(self.palette.foreground))
            .cursor_pointer()
            .hover(|style| style.underline())
            .on_press(cx, on_press)
            .child(label)
            .into_any_element()
    }

    fn step(&self, number: usize, parts: Vec<(&'static str, bool)>) -> AnyElement {
        let p = self.palette;
        h_flex()
            .w_full()
            .items_start()
            .gap(px(10.0))
            .child(
                div()
                    .mt(px(0.5))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(18.0))
                    .rounded_full()
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.raised))
                    .text_size(px(11.0))
                    .line_height(px(15.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.foreground))
                    .child(SharedString::from(number.to_string())),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .flex_wrap()
                    .text_size(px(13.0))
                    .line_height(px(19.0))
                    .children(parts.into_iter().map(|(text, strong)| {
                        div()
                            .text_color(hsla(if strong { p.foreground } else { p.muted }))
                            .child(text)
                    })),
            )
            .into_any_element()
    }

    fn spinner_line(&self, text: impl Into<SharedString>) -> AnyElement {
        h_flex()
            .justify_center()
            .gap(px(8.0))
            .child(modal_spinner(self.palette.muted))
            .child(self.muted(text))
            .into_any_element()
    }

    /// What went wrong turning things on, and Try again.
    fn failed_content(&self, message: String, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(px(12.0))
            .px(px(16.0))
            .text_center()
            .child(
                div()
                    .w_full()
                    .text_size(px(13.0))
                    .line_height(px(19.0))
                    .text_color(hsla(p.destructive))
                    .child(message),
            )
            .child(div().w(px(140.0)).child(modal_action_button(
                &p,
                "connect-device-retry",
                "Try again",
                None,
                ModalButtonTone::Neutral,
                false,
                |this: &mut Self, _window, cx| this.retry(cx),
                cx,
            )))
            .into_any_element()
    }

    /// What works while the helper isn't installed.
    fn meanwhile_line(&self) -> Div {
        div()
            .text_size(px(12.0))
            .line_height(px(17.0))
            .text_color(hsla(self.palette.muted))
            .child(if self.tailscale.is_on() {
                "Meanwhile, phones on Tailscale or on your own network can still connect."
            } else {
                "Meanwhile, phones on your own network can still connect."
            })
    }

    /// The helper isn't installed: what it is, Install, and what works meanwhile; after a failed
    /// install, why it failed and Try again.
    fn helper_missing_content(&self, error: Option<String>, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let failed = error.is_some();
        let (icon, icon_color, heading) = if failed {
            (ICON_ALERT, p.destructive, "The helper didn't install")
        } else {
            (
                ICON_HELPER,
                p.foreground,
                "The Easy Connect helper isn't installed yet",
            )
        };
        let explanation: SharedString = match error {
            Some(error) => error.into(),
            None => "Your phone reaches this computer through it. Ghostex builds it here once: it downloads about 65 MB of build tools and takes a few minutes.".into(),
        };
        v_flex()
            .w_full()
            .items_center()
            .gap(px(12.0))
            .px(px(16.0))
            .text_center()
            .child(modal_icon(icon, 26.0, icon_color))
            .child(
                div()
                    .w_full()
                    .text_size(px(15.0))
                    .line_height(px(21.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.foreground))
                    .child(heading),
            )
            .child(
                div()
                    .w_full()
                    .text_size(px(13.0))
                    .line_height(px(19.0))
                    .text_color(hsla(if failed { p.destructive } else { p.muted }))
                    .child(explanation),
            )
            .child(div().pt(px(4.0)).w(px(180.0)).child(modal_action_button(
                &p,
                "connect-device-install-helper",
                if failed {
                    "Try again"
                } else {
                    "Install the helper"
                },
                None,
                ModalButtonTone::Primary,
                false,
                |this: &mut Self, _window, cx| this.install_helper(cx),
                cx,
            )))
            .child(div().w_full().pt(px(4.0)).child(self.meanwhile_line()))
            .into_any_element()
    }

    /// The helper installing: its step, and that closing the window doesn't stop it.
    fn helper_installing_content(&self, note: Option<String>) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .items_center()
            .gap(px(10.0))
            .px(px(16.0))
            .text_center()
            .child(
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(modal_spinner(p.foreground))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .line_height(px(21.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child("Installing the Easy Connect helper"),
                    ),
            )
            .child(
                self.muted(note.unwrap_or_else(|| "Starting…".to_string()))
                    .w_full(),
            )
            .child(
                div()
                    .w_full()
                    .pt(px(6.0))
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .text_color(hsla(p.muted))
                    .child("It keeps going if you close this window. The code appears here when it's done."),
            )
            .into_any_element()
    }

    /// Asking for the code failed and there is none to show: gxserver or this app said why (most
    /// often this app's owner key still waits for an Approve, `owner_security::owner_refusal`).
    ///
    /// CDXC:RemotePairing 2026-11-17 WHY: On m7-arch, whose first owner key was another computer's app, this app's own key waits for an Approve, so every ask for the code is refused with what to approve. Connect a computer never drew that answer and Connect a device drew it under a square that still said "Getting a code ready…", so the window spun forever. The answer now takes the square's place, and the next ask after the Approve brings the code in.
    pub(super) fn code_refused(&self) -> bool {
        self.stage == Stage::Ready && self.code.is_none() && self.poll_error.is_some()
    }

    /// Why there is no code, in the square the code appears in.
    fn code_refused_content(&self, message: String) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(px(8.0))
            .px(px(16.0))
            .text_center()
            .child(
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(modal_icon(ICON_ALERT, 16.0, p.foreground))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .line_height(px(21.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child("Can't show the code yet"),
                    ),
            )
            .child(self.muted(message).w_full())
            .into_any_element()
    }

    /// What turning SSH access on does here: the OS's sentence, or on Linux, that the server is
    /// installed but not running, or not installed.
    /// gxserver sends `installed` only on Linux, so macOS and Windows keep their sentence.
    pub(super) fn ssh_turn_on_line(&self) -> String {
        match self.ssh_server.installed {
            Some(true) => "The OpenSSH server is installed here but not running. Ghostex starts it, after it asks for your password.".to_string(),
            Some(false) => format!(
                "The OpenSSH server isn't installed here. {}",
                self.ssh_missing_how(true)
            ),
            None => Self::ssh_turn_on_sentence().to_string(),
        }
    }

    /// How to install a missing OpenSSH server: Install (`done_screen`: its Install SSH button) or
    /// the command below, or with no known package manager, the short list.
    fn ssh_missing_how(&self, done_screen: bool) -> &'static str {
        match (self.ssh_server.install_command.is_some(), done_screen) {
            (true, true) => {
                "Install SSH sets it up after your password, or run this in a terminal:"
            }
            (true, false) => {
                "Install sets it up after your password, or run this in a terminal and then click Turn on SSH:"
            }
            (false, _) => {
                "Install it with your package manager (for example: sudo apt install openssh-server, sudo dnf install openssh-server or sudo pacman -S openssh), then click Turn on SSH."
            }
        }
    }

    /// This distribution's install command with Copy, while the server is missing and the
    /// command is known.
    pub(super) fn ssh_install_command_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.ssh_server_missing() {
            return None;
        }
        let command = self.ssh_server.install_command.clone()?;
        let p = self.palette;
        Some(
            h_flex()
                .w_full()
                .gap(px(8.0))
                .items_center()
                .child(
                    div()
                        .id("connect-device-ssh-install-command")
                        .flex_1()
                        .min_w_0()
                        .h(px(MODAL_CONTROL_HEIGHT))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .bg(hsla(p.panel))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_left()
                        .font_family(MODAL_MONO_FONT)
                        .text_size(px(12.0))
                        .text_color(hsla(p.foreground))
                        .child(command.clone()),
                )
                .child(
                    div().flex_shrink_0().child(modal_action_button(
                        &p,
                        "connect-device-copy-ssh-install",
                        if self.copied { "Copied" } else { "Copy" },
                        Some(
                            modal_icon(
                                if self.copied { ICON_CHECK } else { ICON_COPY },
                                14.0,
                                p.foreground,
                            )
                            .into_any_element(),
                        ),
                        ModalButtonTone::Neutral,
                        false,
                        move |this: &mut Self, _window, cx| this.copy_text(command.clone(), cx),
                        cx,
                    )),
                )
                .into_any_element(),
        )
    }

    /// What turning SSH access on does on this computer's OS, in one sentence.
    pub(super) fn ssh_turn_on_sentence() -> &'static str {
        if cfg!(windows) {
            "If OpenSSH Server isn't installed, Ghostex installs it and allows port 22 in the firewall, after an admin prompt."
        } else if cfg!(target_os = "macos") {
            "Ghostex turns on Remote Login, after your Mac asks for your password."
        } else {
            "Ghostex starts this computer's SSH server, after it asks for your password."
        }
    }

    /// SSH access is off (or turning it on failed): why this computer's code waits for it, what
    /// turning it on does, and Turn on SSH. Nothing is turned on until that click.
    fn ssh_ask_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let computer = self.kind() == ConnectDeviceKind::Computer;
        match self.ssh {
            SshAccess::TurningOn => {
                return self
                    .spinner_line("Turning on SSH. Your computer may ask for your password.");
            }
            SshAccess::Installing => {
                return self
                    .spinner_line("Installing OpenSSH. Your computer may ask for your password.");
            }
            _ => {}
        }
        let failed = match &self.ssh {
            SshAccess::Failed(message) => Some(message.clone()),
            _ => None,
        };
        let missing = self.ssh_server_missing();
        // Install needs the package manager gxserver found (it names its command).
        let can_install = missing && self.ssh_server.install_command.is_some();
        let here = crate::host_name::local_computer_name();
        let heading = if failed.is_some() {
            "SSH didn't turn on".to_string()
        } else if missing {
            "The OpenSSH server isn't installed".to_string()
        } else if computer {
            format!("{here}'s code needs SSH")
        } else {
            format!("The code needs SSH on {here}")
        };
        let explanation: SharedString = match failed.clone() {
            Some(message) => message.into(),
            None => format!(
                "{} {}",
                if computer {
                    "The other computer connects to this one over SSH."
                } else {
                    "Your phone connects to this computer over SSH."
                },
                if missing {
                    // The heading already says it isn't installed.
                    self.ssh_missing_how(false).to_string()
                } else {
                    self.ssh_turn_on_line()
                }
            )
            .into(),
        };
        let turn_on_label = if failed.is_some() && !can_install {
            "Try again"
        } else {
            "Turn on SSH"
        };
        v_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(px(10.0))
            .px(px(16.0))
            .text_center()
            .child(
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(modal_icon(
                        ICON_ALERT,
                        16.0,
                        if failed.is_some() {
                            p.destructive
                        } else {
                            p.foreground
                        },
                    ))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .line_height(px(21.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child(heading),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .text_size(px(13.0))
                    .line_height(px(19.0))
                    .text_color(hsla(if failed.is_some() {
                        p.destructive
                    } else {
                        p.muted
                    }))
                    .child(explanation),
            )
            .children(self.ssh_install_command_row(cx))
            .child(
                h_flex()
                    .pt(px(2.0))
                    .gap(px(8.0))
                    .justify_center()
                    .when(can_install, |row| {
                        row.child(div().w(px(160.0)).child(modal_action_button(
                            &p,
                            "connect-device-install-ssh",
                            if failed.is_some() {
                                "Install again"
                            } else {
                                "Install"
                            },
                            None,
                            ModalButtonTone::Primary,
                            false,
                            |this: &mut Self, _window, cx| this.install_ssh(cx),
                            cx,
                        )))
                    })
                    .child(div().w(px(160.0)).child(modal_action_button(
                        &p,
                        "connect-device-turn-on-ssh",
                        turn_on_label,
                        None,
                        if can_install {
                            ModalButtonTone::Neutral
                        } else {
                            ModalButtonTone::Primary
                        },
                        false,
                        |this: &mut Self, _window, cx| this.turn_on_ssh(cx),
                        cx,
                    ))),
            )
            .when(computer, |this| {
                this.child(
                    self.muted("Pasting the other computer's code below works without it.")
                        .w_full()
                        .text_size(px(12.0)),
                )
            })
            .into_any_element()
    }

    /// The dashed square the code appears in (at least `height` tall, taller for the helper's
    /// screens): what is happening, or what went wrong.
    fn code_placeholder(&self, height: f32, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let content = match &self.stage {
            Stage::Failed(message) => self.failed_content(message.clone(), cx),
            Stage::NeedsHelper(error) => self.helper_missing_content(error.clone(), cx),
            Stage::InstallingHelper(note) => self.helper_installing_content(note.clone()),
            Stage::Preparing(Some(note)) => self.spinner_line(note.clone()),
            Stage::Ready if self.own_code_needs_ssh() => self.ssh_ask_content(cx),
            Stage::Ready if self.code_refused() => {
                self.code_refused_content(self.poll_error.clone().unwrap_or_default())
            }
            _ => self.spinner_line("Getting a code ready…"),
        };
        div()
            .w_full()
            .min_h(px(height))
            .py(px(16.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_dashed()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.panel))
            .child(content)
            .into_any_element()
    }

    fn qr_grid(&mut self) -> Option<QrGrid> {
        let payload = self.code.as_ref()?.payload.clone();
        if self.qr.as_ref().map(|(cached, _)| cached.as_str()) != Some(payload.as_str()) {
            self.qr = Some((payload.clone(), QrGrid::encode(&payload)));
        }
        self.qr.as_ref().and_then(|(_, grid)| grid.clone())
    }

    /// "Is this your phone?": the device, its three words as the biggest thing, and the two answers.
    fn ask_body(&self, device: &WaitingDevice, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let deciding = self.deciding;
        v_flex()
            .w_full()
            .gap(px(16.0))
            .child(
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(modal_icon(
                        if device.phone {
                            ICON_PHONE
                        } else {
                            ICON_COMPUTER
                        },
                        16.0,
                        p.foreground,
                    ))
                    .child(device.name.clone())
                    .child(self.muted(format!("· {}", device.kind_label))),
            )
            .child(pairing_words(&p, &device.words, false))
            .child(self.muted(pairing_words_sentence(device.phone)))
            .children(
                self.decide_error
                    .clone()
                    .map(|error| modal_error(&p, error)),
            )
            .child(modal_footer(vec![
                modal_action_button(
                    &p,
                    "connect-device-deny",
                    pairing_deny_label(device.phone),
                    None,
                    ModalButtonTone::Neutral,
                    deciding,
                    |this: &mut Self, _window, cx| this.decide(false, cx),
                    cx,
                ),
                modal_action_button(
                    &p,
                    "connect-device-approve",
                    "Approve",
                    deciding.then(|| modal_spinner(p.primary_button_text())),
                    ModalButtonTone::Primary,
                    deciding,
                    |this: &mut Self, _window, cx| this.decide(true, cx),
                    cx,
                ),
            ]))
            .when(self.require_presence, |this| {
                this.child(
                    self.muted(approve_presence_hint())
                        .text_size(px(12.0))
                        .text_center(),
                )
            })
            .into_any_element()
    }

    fn finishing_body(&self, device: &WaitingDevice) -> AnyElement {
        v_flex()
            .w_full()
            .gap(px(16.0))
            .child(pairing_words(&self.palette, &device.words, false))
            .child(self.spinner_line(format!(
                "Approved. Waiting for {} to finish connecting…",
                device.name
            )))
            .into_any_element()
    }

    fn connected_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .gap(px(20.0))
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .gap(px(10.0))
                    .child(
                        div()
                            .mt(px(1.0))
                            .flex_shrink_0()
                            .child(modal_icon(ICON_CONNECTED, 18.0, p.success)),
                    )
                    .child(self.muted(match (self.kind(), &self.stage) {
                        (ConnectDeviceKind::Computer, Stage::Connected(device)) => format!(
                            "It can open your chats and terminals on {} from anywhere. To open {} from here too, click Connect both ways on {}.",
                            self.computer_name, device.name, device.name
                        ),
                        _ => format!(
                            "It can open your chats and terminals on {} from anywhere. Remove it any time in Settings > Remote.",
                            self.computer_name
                        ),
                    }).flex_1().min_w_0()),
            )
            .child(modal_footer(vec![
                modal_action_button(
                    &p,
                    "connect-device-another",
                    match self.kind() {
                        ConnectDeviceKind::Phone => "Connect another phone",
                        ConnectDeviceKind::Computer => "Connect another computer",
                    },
                    None,
                    ModalButtonTone::Neutral,
                    false,
                    |this: &mut Self, _window, cx| this.connect_another(cx),
                    cx,
                ),
                modal_action_button(
                    &p,
                    "connect-device-done",
                    "Done",
                    None,
                    ModalButtonTone::Primary,
                    false,
                    |this: &mut Self, window, cx| this.close(window, cx),
                    cx,
                ),
            ]))
            .into_any_element()
    }
}

impl Render for GpuiConnectDeviceWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        // A phone that scanned takes the window from the large code.
        if !self.shows_code_screen() {
            self.large_code = false;
        }
        let screen = self.screen();
        if self.fitted_screen.is_some_and(|fitted| {
            fitted != (screen, self.way, self.computer_tab, self.ssh_fit_key())
        }) {
            self.fit = ModalScrollFit::new();
        }
        self.fitted_screen = Some((screen, self.way, self.computer_tab, self.ssh_fit_key()));
        let body = match (self.stage.clone(), self.waiting.clone()) {
            _ if self.join.is_some() => self.join_body(window, cx),
            (Stage::Connected(_), _) => self.connected_body(cx),
            (Stage::Finishing { device, .. }, _) => self.finishing_body(&device),
            (Stage::Ready, Some(device)) => self.ask_body(&device, cx),
            _ if self.shows_large_code() => self.large_code_body(cx),
            _ if self.computer_only => self.computer_way(window, cx),
            _ => self.ways_body(window, cx),
        };
        let header = div()
            .text_size(px(16.0))
            .line_height(px(20.8))
            .text_color(hsla(p.foreground))
            .child(self.title())
            .into_any_element();
        modal_shell_scrolling(
            &p,
            "ghostex-gpui-connect-device-modal",
            "ghostex-gpui-connect-device-modal-body",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            header,
            body,
            None,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiConnectDeviceWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
    }
}
