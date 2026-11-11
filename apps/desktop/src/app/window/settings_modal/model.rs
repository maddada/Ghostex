//! The Settings pages, the open request (deep links), and what the modal asks its host to do.
use serde_json::{Map, Value};
use std::rc::Rc;

/// The Settings pages (`SETTINGS_MODAL_NAVIGATION_TABS`), with their stored ids.
///
/// CDXC:Settings 2026-10-29 DECISION:
/// User (Q5, "tiers"): the General page is gone; each of its rail groups is a page of its own
/// (Chat, Sidebar & Sessions, Terminal, Notifications, Files & Links, Power & Sleep), and the rail
/// shows three labelled tiers (`RailTier`): App, Setup and More (superseded by the groups of
/// CDXC:Settings 2026-11-11 at `RailTier`). Q7: the Tools group is called
/// Files & Links. Q8: Terminal is always in the rail. Q6 ("keep mode", superseded the same day
/// by the Advanced fold, CDXC:Settings 2026-10-29 in search.rs): advanced rows fold at the bottom
/// of their page. Sleeping Sessions joined Sidebar & Sessions and
/// Experimental (with Reset to defaults) moved to About. The `settings` id still opens the first
/// page, Chat, so saved navigation, `ghostex settings open --tab settings` and old deep links keep
/// working; a General section deep link (`initialSection`) opens the page that renders it.
/// SEE-ALSO: packages/settings-catalog/src/layout.rs (`GENERAL_GROUPS`), store.rs (`initial_tab`), rail.rs (`render_rail`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SettingsTabId {
    Updates,
    Chat,
    Sidebar,
    Terminal,
    Notifications,
    FilesLinks,
    PowerSleep,
    Theme,
    Integrations,
    CloudBoxes,
    Extensions,
    OsIntegration,
    Remote,
    Sync,
    Projects,
    Agents,
    Accounts,
    Autopilot,
    Collab,
    Secrets,
    Security,
    Actions,
    OpenTargets,
    Hotkeys,
    Debugging,
    About,
    Workspaces,
    Connectors,
    Dictation,
    VoiceAgent,
}

/// The rail's groups (CDXC:Settings 2026-11-11 below): a label above each run of pages.
///
/// CDXC:Settings 2026-11-11 DECISION:
/// User: "notifications under appearance makes 0 sense, same for hotkeys. please adjust the headings for the sections in the sidebar of the settings and move settings around so things make full sense". Q2510 (arrangement A, "Each heading names its pages"): Updates and Interface stand on top with no label; Working (Chat, Sidebar & Sessions, Terminal, Files & Links, Projects), Keyboard & voice (Hotkeys, Dictation, Voice Agent), Agents & accounts (Agents, Accounts, Autopilot, Secrets, Integrations), Connections (Remote, Sync, Collab) and System (Notifications, Power & Sleep, Security, About). Extensions and the pages that fold under it are unchanged, and no page is renamed, split or merged. Every page keeps its own show/hide rule; a group whose pages are all hidden shows no label, since a label is drawn only above a shown page ([`RailTier::label_before`]). Supersedes the six groups of 2026-11-08 (Q1770: Working, Appearance, Voice, Agents & accounts, Connections, Privacy & system) and Q1771's Updates alone on top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RailTier {
    /// Updates and Interface, above the labelled groups with no label of their own.
    Top,
    /// Chat, Sidebar & Sessions, Terminal, Files & Links, Projects.
    Working,
    /// Hotkeys, Dictation, Voice Agent.
    KeyboardVoice,
    /// Agents, Accounts, Autopilot, Secrets, Integrations.
    AgentsAccounts,
    /// Remote, Sync, Collab.
    Connections,
    /// Notifications, Power & Sleep, Security, About.
    System,
    /// The Extensions page and the pages that fold under it.
    More,
}

impl RailTier {
    pub(crate) fn title(self) -> &'static str {
        match self {
            RailTier::Top => "",
            RailTier::Working => "Working",
            RailTier::KeyboardVoice => "Keyboard & voice",
            RailTier::AgentsAccounts => "Agents & accounts",
            RailTier::Connections => "Connections",
            RailTier::System => "System",
            RailTier::More => "More",
        }
    }

    /// The group label to draw above shown page `tab` when the page shown before it was in group
    /// `previous`: one at the start of each group's run of shown pages, none for the pages on top. Hidden
    /// pages never reach the rail, so a group with none shown gets no label.
    pub(crate) fn label_before(previous: Option<RailTier>, tab: SettingsTabId) -> Option<RailTier> {
        let tier = tab.tier();
        (previous != Some(tier) && tier != RailTier::Top).then_some(tier)
    }
}

impl SettingsTabId {
    /// The rail order: Updates and Interface, the five labelled groups (CDXC:Settings 2026-11-11 at
    /// `RailTier`) ending with About, then the Extensions page (the catalog) at the rail's bottom with the pages
    /// that fold under it ([`Self::EXTENSIONS_GROUP`]). Pages the shell filters (OS Integration,
    /// Connectors, Debugging, the built-in extensions' pages) leave the rail while they are off.
    pub(crate) const RAIL_ORDER: [SettingsTabId; 30] = [
        SettingsTabId::Updates,
        SettingsTabId::Theme,
        SettingsTabId::Chat,
        SettingsTabId::Sidebar,
        SettingsTabId::Terminal,
        SettingsTabId::FilesLinks,
        SettingsTabId::Projects,
        SettingsTabId::Hotkeys,
        SettingsTabId::Dictation,
        SettingsTabId::VoiceAgent,
        SettingsTabId::Agents,
        SettingsTabId::Accounts,
        SettingsTabId::Autopilot,
        SettingsTabId::Secrets,
        SettingsTabId::Integrations,
        SettingsTabId::Remote,
        SettingsTabId::Sync,
        SettingsTabId::Collab,
        SettingsTabId::Notifications,
        SettingsTabId::PowerSleep,
        SettingsTabId::Security,
        SettingsTabId::About,
        SettingsTabId::Extensions,
        SettingsTabId::Workspaces,
        SettingsTabId::CloudBoxes,
        SettingsTabId::Connectors,
        SettingsTabId::Actions,
        SettingsTabId::OpenTargets,
        SettingsTabId::OsIntegration,
        SettingsTabId::Debugging,
    ];

    /// CDXC:Settings 2026-10-27 DECISION:
    /// User, about the old "Store" row inside the group: "I dont like how store looks like the other ones here and i dont like that it's hidden there and i dont like that it's called store". Q640-Q641: the catalog page is called "Extensions" and is its own rail row at the bottom, drawn like the other top-level rows; clicking it opens the page, and a chevron "below the extensinos button" folds and unfolds the switched-on extensions' pages listed here. About moves up "to be last one with the ones up top so it's not next to extensions". Q642: no update count. Q643: the phone is left alone. This supersedes the 2026-10-19 Q308 decision (the page renamed "Store", first item in the group, a header that only folds); the rest of 2026-10-19 still holds: Q306 the group holds Workspaces, Connectors, Cloud Boxes, Actions, Open In, OS Integration and Debugging; Q307 the fold is "Collapsed the first time, then remembers" (`settingsModalNavigation`); a search opens it and lists only its matching pages; a deep link to one of its pages opens it; folded over the open page, its chevron row shows the selected fill. Q313: Connectors, OS Integration and Debugging are built-in extensions with "Switch on top of the gate" (CDXC:Extensions 2026-10-19 in official_extensions.rs).
    /// CDXC:VoiceMode 2026-10-25 DECISION:
    /// User: "voice agent needs to be moved under extensions in the sidebar of the settings of the gpui app too". Voice agent is the group's first page now that the voice agent is a built-in extension (official_extensions.rs). Deep links (`initialTab: "voiceAgent"`, the voice panel's Open Settings, `ghostex settings open --tab voiceAgent`) open the group like any page in it.
    /// CDXC:VoiceMode 2026-11-08 DECISION: User: "we need to call Voice page in settings "Dictation" and have "Voice Agent" as a separate page"; Q1710: Voice Agent sits right under Dictation, both in the App tier after Power & Sleep. Dictation has the new `dictation` id and `voice` opens it; Voice Agent keeps `voiceAgent` and leaves the rail while its extension is off (`settingsPages` in official_extensions.rs), since Dictation now holds the keys it needs and the Dictation history. Supersedes Q1269's single Voice page (2026-11-05) and the 2026-10-25 move under Extensions. The two pages sit in the rail's Keyboard & voice group, after Hotkeys (CDXC:Settings 2026-11-11 at `RailTier`), Voice Agent still right under Dictation.
    pub(crate) const EXTENSIONS_GROUP: [SettingsTabId; 7] = [
        SettingsTabId::Workspaces,
        SettingsTabId::CloudBoxes,
        SettingsTabId::Connectors,
        SettingsTabId::Actions,
        SettingsTabId::OpenTargets,
        SettingsTabId::OsIntegration,
        SettingsTabId::Debugging,
    ];

    /// Whether the page folds under the rail's Extensions row (the Extensions page itself does not).
    pub(crate) fn in_extensions_group(self) -> bool {
        Self::EXTENSIONS_GROUP.contains(&self)
    }

    /// The rail group the page is listed under (CDXC:Settings 2026-11-11 at `RailTier`).
    pub(crate) fn tier(self) -> RailTier {
        match self {
            SettingsTabId::Updates | SettingsTabId::Theme => RailTier::Top,
            SettingsTabId::Chat
            | SettingsTabId::Sidebar
            | SettingsTabId::Terminal
            | SettingsTabId::FilesLinks
            | SettingsTabId::Projects => RailTier::Working,
            SettingsTabId::Hotkeys | SettingsTabId::Dictation | SettingsTabId::VoiceAgent => {
                RailTier::KeyboardVoice
            }
            SettingsTabId::Agents
            | SettingsTabId::Accounts
            | SettingsTabId::Autopilot
            | SettingsTabId::Secrets
            | SettingsTabId::Integrations => RailTier::AgentsAccounts,
            SettingsTabId::Remote | SettingsTabId::Sync | SettingsTabId::Collab => {
                RailTier::Connections
            }
            SettingsTabId::Notifications
            | SettingsTabId::PowerSleep
            | SettingsTabId::Security
            | SettingsTabId::About => RailTier::System,
            _ => RailTier::More,
        }
    }

    /// The catalog group (`GENERAL_GROUPS`) a page General split into renders, by the group's id.
    pub(crate) fn general_group(self) -> Option<&'static str> {
        match self {
            SettingsTabId::Chat => Some("chat"),
            SettingsTabId::Sidebar => Some("sidebar"),
            SettingsTabId::Terminal => Some("terminal"),
            SettingsTabId::Notifications => Some("notifications"),
            SettingsTabId::FilesLinks => Some("tools"),
            SettingsTabId::PowerSleep => Some("system"),
            _ => None,
        }
    }

    /// Whether the page is one General split into (rendered by `tabs/general`).
    pub(crate) fn is_general_page(self) -> bool {
        self.general_group().is_some()
    }

    /// The page that renders catalog group `group`.
    pub(crate) fn general_page(group: &str) -> Option<Self> {
        Self::RAIL_ORDER
            .into_iter()
            .find(|tab| tab.general_group() == Some(group))
    }

    /// The id the settings file and the open message use (`SettingsModalTab`).
    pub(crate) fn id(self) -> &'static str {
        match self {
            SettingsTabId::Updates => "updates",
            SettingsTabId::Chat => "chat",
            SettingsTabId::Sidebar => "sidebar",
            SettingsTabId::Terminal => "terminal",
            SettingsTabId::Notifications => "notifications",
            SettingsTabId::FilesLinks => "tools",
            SettingsTabId::PowerSleep => "system",
            SettingsTabId::Theme => "theme",
            SettingsTabId::Integrations => "integrations",
            SettingsTabId::CloudBoxes => "cloudBoxes",
            SettingsTabId::Extensions => "extensions",
            SettingsTabId::OsIntegration => "osIntegration",
            SettingsTabId::Remote => "remote",
            SettingsTabId::Sync => "sync",
            SettingsTabId::Projects => "projects",
            SettingsTabId::Agents => "agents",
            SettingsTabId::Accounts => "accounts",
            SettingsTabId::Autopilot => "autopilot",
            SettingsTabId::Collab => "collab",
            SettingsTabId::Secrets => "secrets",
            SettingsTabId::Security => "security",
            SettingsTabId::Actions => "actions",
            SettingsTabId::OpenTargets => "openTargets",
            SettingsTabId::Hotkeys => "hotkeys",
            SettingsTabId::Debugging => "debugging",
            SettingsTabId::About => "about",
            SettingsTabId::Workspaces => "workspaces",
            SettingsTabId::Connectors => "connectors",
            SettingsTabId::Dictation => "dictation",
            SettingsTabId::VoiceAgent => "voiceAgent",
        }
    }

    /// `settings`, the old General page's id, opens Chat (CDXC:Settings 2026-10-29 above).
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        if id == "settings" {
            return Some(SettingsTabId::Chat);
        }
        if id == "interface" {
            return Some(SettingsTabId::Theme);
        }
        if id == "voice" {
            return Some(SettingsTabId::Dictation);
        }
        Self::RAIL_ORDER.into_iter().find(|tab| tab.id() == id)
    }

    /// The rail label.
    pub(crate) fn title(self) -> &'static str {
        match self {
            SettingsTabId::Updates => "Updates",
            SettingsTabId::Chat => "Chat",
            SettingsTabId::Sidebar => "Sidebar & Sessions",
            SettingsTabId::Terminal => "Terminal",
            SettingsTabId::Notifications => "Notifications",
            SettingsTabId::FilesLinks => "Files & Links",
            SettingsTabId::PowerSleep => "Power & Sleep",
            // CDXC:Settings 2026-10-31 DECISION: User: "let's also rename the theme page in settings to Interface". Only its name changed: its stored id stays `theme`, so saved navigation, `ghostex settings open --tab theme` and old deep links keep working, and `interface` opens it too.
            SettingsTabId::Theme => "Interface",
            SettingsTabId::Integrations => "Integrations",
            SettingsTabId::CloudBoxes => "Cloud Boxes",
            SettingsTabId::Extensions => "Extensions",
            SettingsTabId::OsIntegration => "OS Integration",
            SettingsTabId::Remote => "Remote",
            SettingsTabId::Sync => "Sync",
            SettingsTabId::Projects => "Projects",
            SettingsTabId::Agents => "Agents",
            SettingsTabId::Accounts => "Accounts",
            SettingsTabId::Autopilot => "Autopilot",
            SettingsTabId::Collab => "Collab",
            SettingsTabId::Secrets => "Secrets",
            SettingsTabId::Security => "Security",
            SettingsTabId::Actions => "Actions",
            SettingsTabId::OpenTargets => "Open In",
            SettingsTabId::Hotkeys => "Hotkeys",
            SettingsTabId::Debugging => "Debugging",
            SettingsTabId::About => "About",
            SettingsTabId::Workspaces => "Workspaces",
            SettingsTabId::Connectors => "Connectors",
            SettingsTabId::Dictation => "Dictation",
            SettingsTabId::VoiceAgent => "Voice Agent",
        }
    }

    /// The rail icon (the Tabler icon `sidebar-pages.ts` (deleted 2026-10-01) draws).
    pub(crate) fn icon(self) -> &'static str {
        match self {
            SettingsTabId::Updates => "modals/settings/circle-arrow-up.svg",
            SettingsTabId::Chat => "modals/settings/message-circle.svg",
            SettingsTabId::Sidebar => "modals/settings/layout-sidebar.svg",
            SettingsTabId::Terminal => "modals/settings/terminal-2.svg",
            SettingsTabId::Notifications => "modals/settings/bell.svg",
            SettingsTabId::FilesLinks => "modals/settings/file-text.svg",
            SettingsTabId::PowerSleep => "modals/settings/moon.svg",
            SettingsTabId::Theme => "modals/settings/palette.svg",
            SettingsTabId::Integrations => "modals/settings/tools.svg",
            SettingsTabId::CloudBoxes => "modals/settings/box.svg",
            SettingsTabId::Extensions => "modals/settings/puzzle.svg",
            SettingsTabId::OsIntegration => "modals/settings/device-desktop.svg",
            SettingsTabId::Remote => "modals/settings/cloud.svg",
            SettingsTabId::Sync => "modals/settings/refresh.svg",
            SettingsTabId::Projects => "modals/settings/folder-open.svg",
            SettingsTabId::Agents => "modals/settings/code-dots.svg",
            SettingsTabId::Accounts => "modals/settings/users.svg",
            SettingsTabId::Autopilot => "modals/settings/plane.svg",
            SettingsTabId::Collab => "modals/settings/users-group.svg",
            SettingsTabId::Secrets => "modals/settings/key.svg",
            SettingsTabId::Security => "modals/settings/shield-check.svg",
            SettingsTabId::Actions => "modals/settings/player-play.svg",
            SettingsTabId::OpenTargets => "modals/settings/external-link.svg",
            SettingsTabId::Hotkeys => "modals/settings/keyboard.svg",
            SettingsTabId::Debugging => "modals/settings/bug.svg",
            SettingsTabId::About => "modals/settings/info-circle.svg",
            SettingsTabId::Workspaces => "modals/settings/briefcase.svg",
            SettingsTabId::Connectors => "modals/settings/plug-connected.svg",
            SettingsTabId::Dictation => "modals/settings/microphone.svg",
            SettingsTabId::VoiceAgent => "titlebar/headset.svg",
        }
    }
}

/// The deep-link fields of a Settings `open` message (`modal-host.tsx` (deleted 2026-10-01)), kept for the pages that
/// honour them. Only an open of the `settings` kind carries them; the other kinds pick the start
/// page (`getSettingsInitialTab`).
#[derive(Clone, Debug, Default)]
pub(crate) struct SettingsOpenRequest {
    /// `initialTab` as the React modal received it: the `initialTab` field of a `settings` open,
    /// else the page the modal kind names, else `settings`.
    pub(crate) initial_tab: Option<SettingsTabId>,
    /// A section to scroll to (`MainSettingsScrollTargetId` for the General pages, which also
    /// picks the page that renders it: `SettingsStore::initial_tab`).
    pub(crate) initial_section: Option<String>,
    /// `createTag`: open Sidebar Tags' New tag form on arrival.
    pub(crate) initial_sidebar_tags_action: Option<String>,
    pub(crate) initial_search_query: Option<String>,
    pub(crate) initial_remote_machine_id: Option<String>,
    /// `easyConnect` or `tailscale`.
    pub(crate) initial_remote_section: Option<String>,
    /// `agentHooks`.
    pub(crate) initial_agents_section: Option<String>,
    /// The remote machine Agents and Integrations open on (their machine picker).
    pub(crate) initial_machine_id: Option<String>,
    /// The agent (`claude`, `codex`) whose accounts the Accounts page opens on: an account
    /// toast's Open Accounts button (`GpuiAppToastAction::OpenAccounts`).
    pub(crate) initial_accounts_provider: Option<String>,
    /// The saved account whose sign-in starts as the Accounts page opens.
    pub(crate) initial_accounts_reconnect: Option<String>,
    /// The voice agent key field (`xai`, `openai`) Voice agent focuses on arrival: Dictate's Add
    /// key / Check key (`native_chat/dictation.rs` `open_voice_agent_key`).
    pub(crate) initial_voice_agent_key: Option<String>,
    pub(crate) initial_custom_view_id: Option<String>,
    pub(crate) initial_view_scope_key: Option<String>,
    /// The project the Projects page opens on: `initialProjectId`, else the opening window's
    /// current project (filled in by the host).
    pub(crate) initial_project_id: Option<String>,
    /// Opens the Pick Color dialog of this colour setting (the preview binary's `pick-color`
    /// state, and the host where it has no system colour panel; no open message carries it).
    pub(crate) open_color_picker: Option<String>,
    /// Opens this setting's dropdown on arrival (the preview binary's `select` state).
    pub(crate) open_select: Option<String>,
    /// The host can reach the local gxserver (the React page's `tailcatRpc` was defined): set
    /// by the host after reading the open message, never by the message itself.
    pub(crate) gxserver_rpc_available: bool,
    /// Read aloud's Voice choices, `(value, label)`, System voice first: this computer's voices,
    /// listed by the host when Settings opens (`app/read_aloud/voices.rs`). Empty in the preview
    /// binary, which offers System voice alone.
    pub(crate) read_aloud_voices: Vec<(String, String)>,
    /// The preview binary's state name, for page states no open message reaches (an open
    /// dialog, an expanded row, a scripted interaction). Always `None` in the app.
    pub(crate) preview_state: Option<String>,
}

fn optional_text(message: &Value, key: &str) -> Option<String> {
    message.get(key).and_then(Value::as_str).map(str::to_string)
}

impl SettingsOpenRequest {
    /// Reads an `open` message for one of the Settings kinds (`settings`, `hotkeys`,
    /// `configureAgents`, `configureActions`, `openTargets`), the way modal-host.tsx (deleted 2026-10-01) does.
    pub(crate) fn from_open_message(modal_id: &str, message: &Value) -> Self {
        let kind_tab = match modal_id {
            "configureAgents" => Some(SettingsTabId::Agents),
            "configureActions" => Some(SettingsTabId::Actions),
            "hotkeys" => Some(SettingsTabId::Hotkeys),
            "openTargets" => Some(SettingsTabId::OpenTargets),
            _ => None,
        };
        if modal_id != "settings" {
            return Self {
                initial_tab: kind_tab,
                ..Self::default()
            };
        }
        Self {
            initial_tab: message
                .get("initialTab")
                .and_then(Value::as_str)
                .and_then(SettingsTabId::from_id),
            initial_section: optional_text(message, "initialSection"),
            initial_sidebar_tags_action: optional_text(message, "initialSidebarTagsAction")
                .filter(|action| action == "createTag"),
            initial_search_query: optional_text(message, "initialSearchQuery"),
            initial_remote_machine_id: optional_text(message, "initialRemoteMachineId")
                .filter(|id| !id.trim().is_empty()),
            initial_remote_section: optional_text(message, "initialRemoteSection")
                .filter(|section| section == "easyConnect" || section == "tailscale"),
            initial_agents_section: optional_text(message, "initialAgentsSection")
                .filter(|section| section == "agentHooks"),
            initial_machine_id: optional_text(message, "initialMachineId")
                .filter(|id| !id.trim().is_empty()),
            initial_accounts_provider: optional_text(message, "initialAccountsProvider")
                .filter(|provider| provider == "claude" || provider == "codex"),
            initial_accounts_reconnect: optional_text(message, "initialAccountsReconnect")
                .filter(|id| !id.trim().is_empty()),
            initial_voice_agent_key: optional_text(message, "initialVoiceAgentKey")
                .filter(|provider| provider == "xai" || provider == "openai"),
            initial_custom_view_id: optional_text(message, "initialCustomViewId"),
            initial_view_scope_key: optional_text(message, "initialViewScopeKey"),
            initial_project_id: optional_text(message, "initialProjectId")
                .filter(|id| !id.trim().is_empty()),
            open_color_picker: None,
            open_select: None,
            gxserver_rpc_available: false,
            read_aloud_voices: Vec::new(),
            preview_state: None,
        }
    }

    /// `initialTab` as the React modal saw it (Chat, the old `settings` page, when nothing named one).
    pub(crate) fn requested_tab(&self) -> SettingsTabId {
        self.initial_tab.unwrap_or(SettingsTabId::Chat)
    }
}

/// What the Settings modal asks the app to do. The native twin of the React modal's props
/// (`onPatch`, `onChange`, `onClose`) and of every `vscode.postMessage` it made.
pub(crate) enum SettingsModalCommand {
    /// `updateSettingsPatch`: a granular save merged onto the stored settings.
    SavePatch {
        patch: Map<String, Value>,
        source: String,
    },
    /// `updateSettings`: a whole-settings save (presets, Reset to defaults, the Ghostty buttons).
    SaveSettings {
        settings: Map<String, Value>,
        source: String,
    },
    /// A message the React modal posted with `vscode.postMessage` (it reached the app as
    /// `{ type: "sidebarCommand", message }`), handled exactly as that message is.
    PostMessage(Value),
    /// A toast in the app's toast window.
    Toast {
        level: String,
        title: String,
        description: String,
    },
    /// `ColorField`'s swatch (`<input type="color">`): the system colour panel, owned by the
    /// Settings window, starting at `initial` (`#rrggbb`). Each colour it picks comes back
    /// through `GpuiSettingsModalWindow::receive_system_color(key, ..)`, which saves it as the
    /// input's `onChange` did.
    PickSystemColor { key: String, initial: String },
    /// A POST to the local gxserver (`/api/tailcatStatus`, `/api/agentCliMaintenance`, ...) the
    /// way the React page's `RemoteSetupRpc` / `AgentCliConnection` called it; `reply` runs on
    /// the main thread with the envelope's `result` or the error message.
    GxserverRpc {
        path: String,
        params: Value,
        timeout: std::time::Duration,
        reply: GxserverRpcReply,
    },
    /// Approves approval request `id` of this computer's gxserver for the owner: the system
    /// prompt (Touch ID / Windows Hello / polkit), then the signed approval (the app's
    /// `owner_security`, GitHub #495). Settings > Remote's Approve on a waiting phone or computer.
    ApproveRequest { id: String, reply: GxserverRpcReply },
    /// [`Self::GxserverRpc`] sent to a connected remote machine's gxserver through its tunnel
    /// (`remote_conn/settings_rpc.rs`): Settings > Agents and Integrations with that machine picked.
    RemoteGxserverRpc {
        machine_id: String,
        path: String,
        params: Value,
        timeout: std::time::Duration,
        reply: GxserverRpcReply,
    },
    /// Writes `text` to the clipboard with the app's copy feedback (`playCopySound` and the
    /// "Copied!" bubble) the React copy buttons gave.
    CopyToClipboard(String),
    /// A plain HTTP GET the React page made with `fetch` (an extension's README, changelog and
    /// screenshots from the catalog). A `url` that starts with `/` is a gxserver path (the
    /// extension icons under `/ext/<id>/...`); `reply` runs on the main thread with the body.
    HttpGet { url: String, reply: HttpGetReply },
    /// A message the React page posted to the app-modal host itself (`postAppModalHostMessage`:
    /// `accountTitlebarChanged`, `accountSetup`), handled exactly as the host bridge handles it.
    HostMessage(Value),
    /// Stores the dialogs and forms Settings has open (`OPEN_FORMS_STORAGE_KEY`, see
    /// settings_modal/restore.rs); `None` removes the stored value.
    SaveOpenForms(Option<String>),
    /// The modal removed its own window.
    Close,
    /// A sign-in the Accounts page started finished after the page closed: show Settings at
    /// Accounts (`tabs/accounts/sign_in_watch.rs`).
    OpenAccounts,
}

/// The answer of a [`SettingsModalCommand::GxserverRpc`].
pub(crate) type GxserverRpcReply = Box<dyn FnOnce(Result<Value, String>, &mut gpui::App)>;

/// The answer of a [`SettingsModalCommand::HttpGet`].
pub(crate) type HttpGetReply = Box<dyn FnOnce(Result<Vec<u8>, String>, &mut gpui::App)>;

pub(crate) type SettingsModalHost = Rc<dyn Fn(SettingsModalCommand, &mut gpui::App)>;

#[cfg(test)]
mod tests {
    use super::{RailTier, SettingsTabId};

    /// CDXC:Settings 2026-10-29: General's groups are pages of their own and `settings` still
    /// opens Chat.
    #[test]
    fn general_split_into_pages() {
        assert_eq!(
            SettingsTabId::from_id("settings"),
            Some(SettingsTabId::Chat)
        );
        assert_eq!(
            SettingsTabId::from_id("tools"),
            Some(SettingsTabId::FilesLinks)
        );
        assert_eq!(SettingsTabId::FilesLinks.title(), "Files & Links");
        assert_eq!(
            SettingsTabId::general_page("system"),
            Some(SettingsTabId::PowerSleep)
        );
        for tab in SettingsTabId::RAIL_ORDER {
            assert_eq!(SettingsTabId::from_id(tab.id()), Some(tab), "{tab:?}");
            assert_eq!(tab.is_general_page(), tab.general_group().is_some());
        }
    }

    /// CDXC:Settings 2026-11-11 (Q2510): Updates and Interface on top, then the five groups in the
    /// user's order, each one unbroken run, then Extensions and its folded pages.
    #[test]
    fn rail_lists_the_top_pages_then_five_groups_then_extensions() {
        use SettingsTabId as T;
        let labelled: Vec<(&str, Vec<&str>)> = {
            let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
            for tab in SettingsTabId::RAIL_ORDER {
                if tab == T::Extensions || tab.in_extensions_group() {
                    continue;
                }
                match groups.last_mut() {
                    Some((title, pages)) if *title == tab.tier().title() => pages.push(tab.title()),
                    _ => groups.push((tab.tier().title(), vec![tab.title()])),
                }
            }
            groups
        };
        assert_eq!(
            labelled,
            [
                ("", vec!["Updates", "Interface"]),
                (
                    "Working",
                    vec![
                        "Chat",
                        "Sidebar & Sessions",
                        "Terminal",
                        "Files & Links",
                        "Projects"
                    ]
                ),
                (
                    "Keyboard & voice",
                    vec!["Hotkeys", "Dictation", "Voice Agent"]
                ),
                (
                    "Agents & accounts",
                    vec!["Agents", "Accounts", "Autopilot", "Secrets", "Integrations"]
                ),
                ("Connections", vec!["Remote", "Sync", "Collab"]),
                (
                    "System",
                    vec!["Notifications", "Power & Sleep", "Security", "About"]
                ),
            ]
        );
        let mut runs: Vec<RailTier> = SettingsTabId::RAIL_ORDER
            .iter()
            .map(|tab| tab.tier())
            .collect();
        runs.dedup();
        assert_eq!(
            runs,
            [
                RailTier::Top,
                RailTier::Working,
                RailTier::KeyboardVoice,
                RailTier::AgentsAccounts,
                RailTier::Connections,
                RailTier::System,
                RailTier::More
            ]
        );
    }

    /// CDXC:VoiceMode 2026-11-08: Voice Agent sits right under Dictation in their group; `voice`
    /// opens Dictation and `voiceAgent` Voice Agent.
    #[test]
    fn voice_agent_follows_dictation() {
        let order = SettingsTabId::RAIL_ORDER;
        let position = |tab| order.iter().position(|t| *t == tab).unwrap();
        assert_eq!(
            position(SettingsTabId::VoiceAgent),
            position(SettingsTabId::Dictation) + 1
        );
        for tab in [SettingsTabId::Dictation, SettingsTabId::VoiceAgent] {
            assert_eq!(tab.tier(), RailTier::KeyboardVoice);
            assert!(!tab.in_extensions_group());
        }
        assert_eq!(SettingsTabId::Dictation.title(), "Dictation");
        assert_eq!(SettingsTabId::VoiceAgent.title(), "Voice Agent");
        assert_eq!(
            SettingsTabId::from_id("dictation"),
            Some(SettingsTabId::Dictation)
        );
        assert_eq!(
            SettingsTabId::from_id("voice"),
            Some(SettingsTabId::Dictation)
        );
        assert_eq!(
            SettingsTabId::from_id("voiceAgent"),
            Some(SettingsTabId::VoiceAgent)
        );
    }

    /// CDXC:Settings 2026-10-27: About ends the everyday pages, then the Extensions page, then the
    /// pages that fold under it; the page keeps its `extensions` id for deep links.
    #[test]
    fn extensions_page_sits_below_about_with_its_folding_pages_after_it() {
        let order = SettingsTabId::RAIL_ORDER;
        let position = |tab| order.iter().position(|t| *t == tab).unwrap();
        assert_eq!(
            position(SettingsTabId::Extensions),
            position(SettingsTabId::About) + 1
        );
        assert!(!SettingsTabId::Extensions.in_extensions_group());
        assert!(!SettingsTabId::About.in_extensions_group());
        for tab in SettingsTabId::EXTENSIONS_GROUP {
            assert!(
                position(tab) > position(SettingsTabId::Extensions),
                "{tab:?}"
            );
        }
        assert_eq!(
            SettingsTabId::EXTENSIONS_GROUP[0],
            SettingsTabId::Workspaces
        );
        assert_eq!(
            SettingsTabId::from_id("extensions"),
            Some(SettingsTabId::Extensions)
        );
        assert_eq!(SettingsTabId::Extensions.title(), "Extensions");
    }
}
