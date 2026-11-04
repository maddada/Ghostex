//! The Commands tab: the command population, ranking, grouping and rows of
//! packages/core-ui/command-palette.tsx (deleted 2026-10-01), and what running a row does.
//!
//! Ported from `apps/desktop/sidebar/native-quick-access/commands.ts` (deleted; see git history).
//!
//! SEE-ALSO: packages/settings-catalog/src/hotkey_definitions.rs (the hotkey rows; `hotkey_action_icon` below gives each its icon).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::data::QuickAccessData;
use super::hotkeys::normalize_hotkey_settings;
use super::text::{
    format_hotkey_label, is_format_character, is_letter_or_number, js_lower, locale_compare,
    normalize_hotkey_text,
};
use super::wire::{QuickAccessGroup, QuickAccessIcon, QuickAccessRow};
use crate::app_lifecycle::{AppLifecycleAction, APP_LIFECYCLE_ACTIONS, APP_LIFECYCLE_MESSAGE_TYPE};
use crate::sidebar_view::text::js_trim;
use ghostex_settings_catalog::{built_in_extensions, hotkey_definitions, HotkeyDefinition, J};

const GHOSTEX_CHANGELOG_URL: &str = "https://github.com/maddada/ghostex/releases";
const DEFAULT_SIDEBAR_COMMAND_ICON: &str = "playerPlay";

const PANE_ACTION_COMMAND_IDS: &[&str] = &[
    "openBrowserPane",
    "splitMore",
    "splitMoreDown",
    "rotatePanesClockwise",
    "mergeAllTabs",
    "renameActiveSession",
    "delayedSend",
    "forkSession",
    "reloadSession",
    "sleepFocusedSession",
    "wakeFocusedSession",
    "closeFocusedSession",
    "popOutPane",
];

/// `(commandId, modal, searchText, title)`.
const APP_MODAL_COMMANDS: &[(&str, &str, &str, &str)] = &[
    (
        "previousSessions",
        "previousSessions",
        "Reopen a Session history restore previous sessions old sessions",
        "Reopen a Session",
    ),
    (
        "agentsHub",
        "agentsHub",
        "Agents Hub agents profiles skills prompts modal",
        "Agents Hub",
    ),
    (
        "configureAgents",
        "configureAgents",
        "Configure Agents agents settings modal",
        "Configure Agents",
    ),
    (
        "actions",
        "configureActions",
        "Actions configure project actions settings modal",
        "Actions",
    ),
    (
        "openTargets",
        "openTargets",
        "Open Targets open in editors settings modal",
        "Open Targets",
    ),
    (
        "addProject",
        "addProject",
        "Add Project add folder workspace clone repository projects",
        "Add Project",
    ),
];

/// `(commandId, searchText, title)`; the message each posts is [`sidebar_message`].
const SIDEBAR_MESSAGE_COMMANDS: &[(&str, &str, &str)] = &[
    ("quickBrowserTab", "Quick Browser Tab browser chat", "Quick Browser Tab"),
    (
        "automations",
        "All Automations schedules agents timers dates recurring",
        "All Automations",
    ),
    (
        "searchByText",
        "Search by Text Find Prompts previous sessions history gx f",
        "Find Prompts",
    ),
    (
        "extensions",
        "Extensions store installed add-ons official built-in features components VS Code code-server CEF gxserver Beads bd runtimes",
        "Extension Store",
    ),
    (
        "openCurrentProjectInFinder",
        "Open File/Folder Location current project open folder workspace",
        "Open File/Folder Location",
    ),
    (
        "setupGhostex",
        "Ghostex setup onboarding first launch guide modal",
        "Setup",
    ),
    (
        "changelog",
        "Changelog release notes releases github browser",
        "Changelog",
    ),
];

fn sidebar_message(command_id: &str) -> Value {
    match command_id {
        "quickBrowserTab" => json!({ "type": "openBrowserChat" }),
        "automations" => json!({ "type": "openAutomationsPage" }),
        "searchByText" => {
            json!({ "actionId": "openFindPrompts", "type": "runGhostexHotkeyAction" })
        }
        "extensions" => json!({ "actionId": "openExtensions", "type": "runGhostexHotkeyAction" }),
        "openCurrentProjectInFinder" => json!({ "type": "openCurrentProjectInFinder" }),
        "setupGhostex" => json!({ "type": "openWorkspaceWelcome" }),
        _ => json!({ "type": "openBrowserPane", "url": GHOSTEX_CHANGELOG_URL }),
    }
}

/// One entry of the three populations (`PaletteCommand`).
#[derive(Clone, Debug)]
pub(crate) enum PaletteCommand {
    Hotkey {
        definition: &'static HotkeyDefinition,
        hotkey: String,
        search_text: String,
    },
    AppModal {
        command_id: &'static str,
        modal: &'static str,
        search_text: &'static str,
        title: &'static str,
    },
    SidebarMessage {
        command_id: &'static str,
        search_text: &'static str,
        title: &'static str,
    },
    OpenTarget {
        command_id: String,
        target_id: String,
        search_text: String,
        title: String,
    },
    /// "Open Lazygit": an enabled extension that opens without a place on screen to open from.
    Extension {
        extension_id: String,
        search_text: String,
        title: String,
    },
    /// New Window, Check for Updates, Restart and the two Quits, where the host is the desktop app.
    AppLifecycle(&'static AppLifecycleAction),
    Project {
        command: Value,
        hotkey: String,
        search_text: String,
    },
}

impl PaletteCommand {
    fn search_text(&self) -> &str {
        match self {
            Self::Hotkey { search_text, .. }
            | Self::OpenTarget { search_text, .. }
            | Self::Extension { search_text, .. }
            | Self::Project { search_text, .. } => search_text,
            Self::AppModal { search_text, .. } | Self::SidebarMessage { search_text, .. } => {
                search_text
            }
            Self::AppLifecycle(action) => action.search_text,
        }
    }

    /// `commandRowKey`.
    pub(crate) fn key(&self) -> String {
        match self {
            Self::Hotkey { definition, .. } => format!("hotkey:{}", definition.id),
            Self::AppModal { command_id, .. } => format!("appModal:{command_id}"),
            Self::SidebarMessage { command_id, .. } => format!("sidebarMessage:{command_id}"),
            Self::OpenTarget { command_id, .. } => format!("openTarget:{command_id}"),
            Self::Extension { extension_id, .. } => format!("extension:{extension_id}"),
            Self::AppLifecycle(action) => format!("appLifecycle:{}", action.id),
            Self::Project { command, .. } => {
                format!("project:{}", text(&command["commandId"]))
            }
        }
    }
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

/// `settings?.[key] ?? DEFAULT_ghostex_SETTINGS[key]) === true`; every default here is `false`.
fn flag(settings: &Value, key: &str) -> bool {
    match &settings[key] {
        Value::Null => false,
        value => *value == Value::Bool(true),
    }
}

/// Built-in extension `id` is on in the HUD's settings (`built_in_extensions`, the one rule every
/// surface reads); its Quick Access rows are left out while it is off.
fn built_in_on(data: &QuickAccessData, id: &str) -> bool {
    built_in_extensions::enabled_in_value(Some(data.settings()), id)
}

/// `isViewScopeVisible` for an official view: project override, then space, then default.
fn view_scope_visible(data: &QuickAccessData, official_extension_id: &str) -> bool {
    let key = format!("official:{official_extension_id}");
    let scope = &data.settings()["viewScopes"][key.as_str()];
    if scope.is_null() {
        return true;
    }
    let project_id = data.hud["activeProjectId"]
        .as_str()
        .filter(|id| !id.is_empty());
    if let Some(project_id) = project_id {
        if let Some(state) = scope["projects"][project_id].as_str() {
            return state == "shown";
        }
    }
    let mut space_states: Vec<&str> = Vec::new();
    for space in data.hud["activeProjectSpaceRefs"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let space_key = format!("{}:{}", text(&space["sectionKey"]), text(&space["spaceId"]));
        if let Some(state) = scope["spaces"][space_key.as_str()].as_str() {
            space_states.push(state);
        }
    }
    if space_states.contains(&"hidden") {
        return false;
    }
    if space_states.contains(&"shown") {
        return true;
    }
    scope["default"].as_str().unwrap_or("shown") == "shown"
}

/// `hiddenWorkareaCommandIds`.
fn hidden_workarea_command_ids(data: &QuickAccessData) -> BTreeSet<&'static str> {
    let settings = data.settings();
    let mut hidden = BTreeSet::new();
    if flag(settings, "browserViewTabHidden") || !view_scope_visible(data, "browser") {
        hidden.insert("switchGitHubView");
        hidden.insert("openBrowserPane");
        hidden.insert("quickBrowserTab");
    }
    if flag(settings, "codeViewTabHidden") || !view_scope_visible(data, "code") {
        hidden.insert("switchSourceView");
    }
    if flag(settings, "docsViewTabHidden") || !view_scope_visible(data, "docs") {
        hidden.insert("switchManageView");
    }
    if flag(settings, "kanbanViewTabHidden") || !view_scope_visible(data, "kanban") {
        hidden.insert("switchKanbanView");
    }
    if flag(settings, "terminalViewTabHidden") || !view_scope_visible(data, "terminal") {
        hidden.insert("switchTerminalView");
    }
    hidden
}

/// `openTargetCommands`: nothing while the HUD carries no settings.
fn open_target_commands(data: &QuickAccessData) -> Vec<PaletteCommand> {
    if data.settings().is_null() {
        return Vec::new();
    }
    data.open_targets
        .iter()
        .map(|target| PaletteCommand::OpenTarget {
            command_id: format!("openTarget:{}", target.id),
            target_id: target.id.clone(),
            search_text: if target.custom {
                format!(
                    "Open In {} current project workspace custom target",
                    target.label
                )
            } else {
                format!(
                    "Open In {} current project workspace editor target",
                    target.label
                )
            },
            title: format!("Open In: {}", target.label),
        })
        .collect()
}

/// CDXC:Extensions 2026-10-27 DECISION:
/// User: "i enabled lazygit extension and i dont see a way to actually use it pls fix". A terminal-pane extension has no view tab, so its only door was a row two menus deep (header ⋯ > Extensions). Every enabled extension that opens without a place on screen to open from (terminal panes, views, modals) is an "Open <title>" row in Quick Access's Commands, which the host lists already filtered by the extension's view scope.
fn extension_commands(data: &QuickAccessData) -> Vec<PaletteCommand> {
    data.extensions
        .iter()
        .map(|extension| PaletteCommand::Extension {
            extension_id: extension.id.clone(),
            search_text: format!(
                "Open {} extension {} {}",
                extension.title, extension.kind, extension.description
            ),
            title: format!("Open {}", extension.title),
        })
        .collect()
}

/// A saved Action's name, empty when it has none.
fn command_name(command: &Value) -> &str {
    command["name"].as_str().map(js_trim).unwrap_or("")
}

fn is_browser(command: &Value) -> bool {
    command["actionType"] == "browser"
}

/// `isConfigured`.
pub(crate) fn is_configured(command: &Value) -> bool {
    let target = if is_browser(command) {
        &command["url"]
    } else {
        &command["command"]
    };
    target.as_str().is_some_and(|value| !value.is_empty())
}

fn command_title(command: &Value) -> String {
    let name = command_name(command);
    if !name.is_empty() {
        return name.to_string();
    }
    if is_browser(command) {
        "Untitled Webpage".to_string()
    } else {
        "Untitled Action".to_string()
    }
}

/// `commandTarget`: the first line of the trimmed command or URL.
fn command_target(command: &Value) -> Option<String> {
    let target = if is_browser(command) {
        command["url"].as_str()
    } else {
        command["command"].as_str()
    };
    let target = js_trim(target?);
    if target.is_empty() {
        return None;
    }
    let first = target.split('\n').next().unwrap_or("");
    (!first.is_empty()).then(|| first.to_string())
}

fn command_description(command: &Value) -> String {
    let type_label = if is_browser(command) {
        "Browser"
    } else {
        "Terminal"
    };
    match command_target(command) {
        Some(target) => format!("{type_label} - {target}"),
        None => format!("{type_label} - Not configured"),
    }
}

/// The three populations, in the order the React palette lists them.
pub(crate) struct Populations {
    pub(crate) built_in: Vec<PaletteCommand>,
    pub(crate) pane_actions: Vec<PaletteCommand>,
    pub(crate) project_actions: Vec<PaletteCommand>,
}

impl Populations {
    pub(crate) fn all(&self) -> impl Iterator<Item = &PaletteCommand> {
        self.built_in
            .iter()
            .chain(&self.pane_actions)
            .chain(&self.project_actions)
    }
}

/// `quickAccessCommandPopulations`.
pub(crate) fn populations(data: &QuickAccessData) -> Populations {
    let settings = data.settings();
    let hotkeys: BTreeMap<&'static str, String> =
        normalize_hotkey_settings(&settings["hotkeys"], data.platform());
    let hidden = hidden_workarea_command_ids(data);
    let to_hotkey = |definition: &'static HotkeyDefinition| {
        let hotkey = normalize_hotkey_text(
            hotkeys
                .get(definition.id)
                .map(String::as_str)
                .unwrap_or(definition.default_key),
        );
        PaletteCommand::Hotkey {
            definition,
            search_text: format!("{} {} {}", definition.title, definition.description, hotkey),
            hotkey,
        }
    };
    let mut built_in: Vec<PaletteCommand> = hotkey_definitions()
        .iter()
        .filter(|definition| {
            definition.id != "openCommandPalette"
                && definition.id != "openSessionSearchPalette"
                && definition.id != "openProjectSearchPalette"
                && definition.id != "openQuickAccessFiles"
                && definition.id != "openExtensions"
                // The same command as Open Commands Panel, under its second key.
                && definition.id != "openCommandsPanelSecondKey"
                && definition.kind() != "runActionSlot"
                && definition.kind() != "chatAction"
                // Paste, find, zoom, close and the like act on whatever has the keyboard, which a
                // Quick Access row is not (CDXC:Hotkeys 2026-10-27 DECISION in the hotkey catalog).
                && definition.kind() != "shellAction"
                // Unfocus Terminal only means something while a terminal has the keyboard.
                && ghostex_settings_catalog::hotkey_has_normal_binding(definition.id)
                && !PANE_ACTION_COMMAND_IDS.contains(&definition.id)
                && !hidden.contains(definition.id)
                // A hotkey of a built-in extension that is off (Session Note while Session notes is).
                && built_in_extensions::hotkey_shown_with(definition.id, |key| {
                    data.settings().get(key).and_then(Value::as_bool)
                })
        })
        .map(to_hotkey)
        .collect();
    let actions_on = built_in_on(data, built_in_extensions::ACTIONS);
    let open_in_on = built_in_on(data, built_in_extensions::OPEN_IN);
    built_in.extend(
        APP_MODAL_COMMANDS
            .iter()
            .filter(|(command_id, ..)| match *command_id {
                "actions" => actions_on,
                "openTargets" => open_in_on,
                _ => true,
            })
            .map(
                |(command_id, modal, search_text, title)| PaletteCommand::AppModal {
                    command_id,
                    modal,
                    search_text,
                    title,
                },
            ),
    );
    built_in.extend(
        SIDEBAR_MESSAGE_COMMANDS
            .iter()
            .filter(|(command_id, _, _)| !hidden.contains(command_id))
            .map(
                |(command_id, search_text, title)| PaletteCommand::SidebarMessage {
                    command_id,
                    search_text,
                    title,
                },
            ),
    );
    if open_in_on {
        built_in.extend(open_target_commands(data));
    }
    built_in.extend(extension_commands(data));
    if data.app_lifecycle {
        built_in.extend(
            APP_LIFECYCLE_ACTIONS
                .iter()
                .map(PaletteCommand::AppLifecycle),
        );
    }
    let pane_actions = PANE_ACTION_COMMAND_IDS
        .iter()
        .filter(|id| !hidden.contains(*id))
        .filter_map(|id| {
            hotkey_definitions()
                .iter()
                .find(|definition| definition.id == *id)
        })
        .map(to_hotkey)
        .collect();
    let project_actions = data.hud["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|_| actions_on)
        .enumerate()
        .filter_map(|(index, command)| {
            let slot_number = index + 1;
            let hotkey = if (1..=5).contains(&slot_number) {
                normalize_hotkey_text(
                    hotkeys
                        .get(format!("runActionSlot{slot_number}").as_str())
                        .map(String::as_str)
                        .unwrap_or(""),
                )
            } else {
                String::new()
            };
            // `isRunnableOrConfigurableCommand`: a name, or an `icon` key at all (`!== undefined`,
            // so even a null one counts).
            if command_name(command).is_empty() && command.get("icon").is_none() {
                return None;
            }
            Some(PaletteCommand::Project {
                search_text: format!(
                    "{} {} {} action {}",
                    command_title(command),
                    command_description(command),
                    hotkey,
                    slot_number
                ),
                command: command.clone(),
                hotkey,
            })
        })
        .collect();
    Populations {
        built_in,
        pane_actions,
        project_actions,
    }
}

fn command_row(command: &PaletteCommand, data: &QuickAccessData) -> QuickAccessRow {
    let platform = data.platform();
    let (title, icon, hotkey) = match command {
        PaletteCommand::Project {
            command, hotkey, ..
        } => (
            command_title(command),
            command["icon"]
                .as_str()
                .unwrap_or(DEFAULT_SIDEBAR_COMMAND_ICON)
                .to_string(),
            hotkey.clone(),
        ),
        PaletteCommand::Hotkey {
            definition, hotkey, ..
        } => (
            definition.title.to_string(),
            hotkey_action_icon(&definition.action).to_string(),
            hotkey.clone(),
        ),
        PaletteCommand::AppModal { modal, title, .. } => (
            (*title).to_string(),
            app_modal_icon(modal).to_string(),
            String::new(),
        ),
        PaletteCommand::SidebarMessage {
            command_id, title, ..
        } => (
            (*title).to_string(),
            sidebar_message_icon(command_id).to_string(),
            String::new(),
        ),
        PaletteCommand::OpenTarget { title, .. } => {
            (title.clone(), "external-link".to_string(), String::new())
        }
        PaletteCommand::Extension { title, .. } => {
            (title.clone(), "puzzle".to_string(), String::new())
        }
        PaletteCommand::AppLifecycle(action) => (
            action.title.to_string(),
            action.icon.to_string(),
            String::new(),
        ),
    };
    QuickAccessRow::Command {
        key: command.key(),
        title,
        icon: QuickAccessIcon::asset(&icon, None),
        hotkey: if hotkey.is_empty() {
            String::new()
        } else {
            format_hotkey_label(&hotkey, platform)
        },
    }
}

/// `BuiltInCommandIcon` for a hotkey-backed row: the icon of its action.
fn hotkey_action_icon(action: &J) -> &'static str {
    let text = |key: &str| action.get(key).and_then(J::as_str).unwrap_or_default();
    match text("kind") {
        "createSession" | "createAgentSession" => "plus",
        "openCommandsPanel" => "terminal-2",
        "openSettings" => "settings",
        "openHotkeys" => "keyboard",
        "toggleSidebarCollapsed" => "layout-sidebar",
        "toggleViewPanel" => "layout-sidebar-right-expand",
        "expandViewPanel" | "expandViewPanelFully" => "arrows-diagonal",
        "renameActiveSession" => "edit",
        "focusedPaneAction" => focused_pane_icon(text("focusedPaneAction")),
        "focusAdjacentGroup" | "cyclePaneTab" => {
            if action
                .get("direction")
                .and_then(J::as_f64)
                .unwrap_or_default()
                < 0.0
            {
                "chevron-left"
            } else {
                "chevron-right"
            }
        }
        "focusDirection" => match text("direction") {
            "up" => "chevron-up",
            "right" => "arrow-right",
            "down" => "chevron-down",
            _ => "arrow-left",
        },
        "splitFocusedPane" => "arrows-diagonal-2",
        "setViewMode" => "layout-dashboard",
        _ => "keyboard",
    }
}

/// `FocusedPaneCommandIcon`.
fn focused_pane_icon(action: &str) -> &'static str {
    match action {
        "openBrowserPane" => "browser",
        "rotatePanesClockwise" => "rotate-clockwise",
        "mergeAllTabs" => "window-maximize",
        "delayedSend" | "closeAfterDone" => "clock",
        "forkSession" => "git-fork",
        "reloadSession" => "refresh",
        "sleepFocusedSession" => "moon",
        "wakeFocusedSession" => "player-play",
        "closeFocusedSession" => "x",
        "popOutPane" => "external-link",
        _ => "layout-sidebar-right-expand",
    }
}

/// `AppModalCommandIcon`.
fn app_modal_icon(modal: &str) -> &'static str {
    match modal {
        "previousSessions" => "history",
        "agentsHub" | "configureAgents" => "settings-automation",
        "configureActions" => "list-details",
        "openTargets" => "external-link",
        "addProject" => "folder-plus",
        _ => "keyboard",
    }
}

/// `SidebarMessageCommandIcon`.
fn sidebar_message_icon(command_id: &str) -> &'static str {
    match command_id {
        "searchByText" => "search",
        "quickBrowserTab" => "browser",
        "automations" => "settings-automation",
        "extensions" => "puzzle",
        "openCurrentProjectInFinder" => "folder-open",
        "setupGhostex" => "checklist",
        _ => "brand-github",
    }
}

/// The Commands list: one ranked "Results" group while searching, and the three hairline-separated
/// sections at rest.
pub(crate) fn command_groups(query: &str, data: &QuickAccessData) -> Vec<QuickAccessGroup> {
    let populations = populations(data);
    let trimmed = js_trim(query);
    if !trimmed.is_empty() {
        let all: Vec<&PaletteCommand> = populations.all().collect();
        let results = filter_palette_items(&all, trimmed, |command| command.search_text());
        if results.is_empty() {
            return Vec::new();
        }
        return vec![QuickAccessGroup {
            key: "results".to_string(),
            heading: "Results".to_string(),
            separated: false,
            rows: results
                .into_iter()
                .map(|command| command_row(command, data))
                .collect(),
        }];
    }
    let mut groups: Vec<QuickAccessGroup> = Vec::new();
    for (key, heading, commands) in [
        ("ghostex", "Ghostex", &populations.built_in),
        ("paneActions", "Pane Actions", &populations.pane_actions),
        (
            "projectActions",
            "Project Actions",
            &populations.project_actions,
        ),
    ] {
        if commands.is_empty() {
            continue;
        }
        groups.push(QuickAccessGroup {
            key: key.to_string(),
            heading: heading.to_string(),
            separated: !groups.is_empty(),
            rows: commands
                .iter()
                .map(|command| command_row(command, data))
                .collect(),
        });
    }
    groups
}

/// What running a Commands row asks the host to do, in order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommandRun {
    /// Close Quick Access.
    Close,
    /// A modal-host `sidebarCommand` with this message.
    Post(Value),
    /// A modal-host `open` message.
    OpenModal(Value),
}

/// `runCommandRow`: the React palette's exact routing. Reopen a Session re-targets Quick Access,
/// Delayed Send keeps the window open while the native owner replaces it, and everything else
/// closes first.
pub(crate) fn run_command_row(key: &str, data: &QuickAccessData) -> Vec<CommandRun> {
    let populations = populations(data);
    let Some(command) = populations.all().find(|command| command.key() == key) else {
        return Vec::new();
    };
    match command {
        PaletteCommand::AppModal { modal, .. } => {
            if *modal == "previousSessions" {
                // `openQuickAccess('recentSessions')`.
                return vec![CommandRun::OpenModal(json!({
                    "modal": "previousSessions",
                    "type": "open",
                }))];
            }
            vec![
                CommandRun::Close,
                CommandRun::OpenModal(json!({ "modal": modal, "type": "open" })),
            ]
        }
        PaletteCommand::SidebarMessage { command_id, .. } => {
            vec![
                CommandRun::Close,
                CommandRun::Post(sidebar_message(command_id)),
            ]
        }
        PaletteCommand::AppLifecycle(action) => vec![
            CommandRun::Close,
            CommandRun::Post(json!({ "action": action.id, "type": APP_LIFECYCLE_MESSAGE_TYPE })),
        ],
        PaletteCommand::OpenTarget { target_id, .. } => vec![
            CommandRun::Close,
            CommandRun::Post(
                json!({ "targetId": target_id, "type": "openCurrentProjectInTarget" }),
            ),
        ],
        PaletteCommand::Extension { extension_id, .. } => vec![
            CommandRun::Close,
            CommandRun::Post(json!({ "extensionId": extension_id, "type": "launchExtension" })),
        ],
        PaletteCommand::Project { command, .. } => {
            if !is_configured(command) {
                return vec![
                    CommandRun::Close,
                    CommandRun::OpenModal(json!({
                        "initialTab": "actions",
                        "modal": "settings",
                        "type": "open",
                    })),
                ];
            }
            let command_id = text(&command["commandId"]).to_string();
            let run_state = data.command_run_states.get(&command_id);
            // `getSidebarCommandRunModeForClick`.
            let debug = command["actionType"] == "terminal"
                && command["closeTerminalOnExit"].as_bool() == Some(true)
                && run_state.is_some_and(|state| {
                    state.status == "error" && state.active_run_ids.is_empty()
                });
            let mut message = json!({ "commandId": command_id, "type": "runSidebarCommand" });
            if debug {
                message["runMode"] = json!("debug");
            }
            vec![CommandRun::Close, CommandRun::Post(message)]
        }
        PaletteCommand::Hotkey { definition, .. } => {
            let post = CommandRun::Post(json!({
                "actionId": definition.id,
                "type": "runGhostexHotkeyAction",
            }));
            /*
            CDXC:AppModal 2026-09-20 WHY:
            Delayed Send replaces Quick Access inside the same native window, so closing before
            posting would race the replacement open and dismiss the timer dialog. The React palette
            skips its own close for exactly this one action.
            */
            if definition.id == "delayedSend" {
                vec![post]
            } else {
                vec![CommandRun::Close, post]
            }
        }
    }
}

/// `filterCommandPaletteItems`: exact, prefix, word-start, substring, then subsequence, with equal
/// scores sorted by the search text and then by list position.
pub(crate) fn filter_palette_items<'a, T>(
    items: &[&'a T],
    query: &str,
    search_text: impl Fn(&T) -> &str,
) -> Vec<&'a T> {
    let normalized_query = normalize_fuzzy(js_trim(query));
    if normalized_query.is_empty() {
        return items.to_vec();
    }
    let mut scored: Vec<(i64, &str, usize, &'a T)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let text = search_text(item);
            score_fuzzy(&normalized_query, text).map(|score| (score, text, index, *item))
        })
        .collect();
    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| locale_compare(left.1, right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    scored.into_iter().map(|(_, _, _, item)| item).collect()
}

fn normalize_fuzzy(value: &str) -> String {
    js_lower(
        &value
            .chars()
            .filter(|character| !is_format_character(*character))
            .collect::<String>(),
    )
}

/// `scoreCommandPaletteFuzzyMatch`.
fn score_fuzzy(normalized_query: &str, raw_candidate: &str) -> Option<i64> {
    let candidate = normalize_fuzzy(raw_candidate);
    let candidate_length = candidate.chars().count() as i64;
    if candidate == normalized_query {
        return Some(100_000);
    }
    if candidate.starts_with(normalized_query) {
        return Some(90_000 - candidate_length);
    }
    if let Some(index) = candidate.find(normalized_query) {
        let preceding = candidate[..index].chars().next_back();
        let is_word_start = preceding.is_none_or(|character| !is_letter_or_number(character));
        return Some(if is_word_start { 80_000 } else { 70_000 } - candidate_length);
    }
    let query: Vec<char> = normalized_query.chars().collect();
    let characters: Vec<char> = candidate.chars().collect();
    let mut query_index = 0;
    let mut score: i64 = 0;
    let mut run: i64 = 0;
    let mut previous_match: i64 = -2;
    for (index, character) in characters.iter().enumerate() {
        if query_index >= query.len() || *character != query[query_index] {
            continue;
        }
        let mut bonus = 1;
        if index as i64 == previous_match + 1 {
            run += 1;
            bonus += run * 3;
        } else {
            run = 0;
        }
        if index == 0 {
            bonus += 12;
        } else if !is_letter_or_number(characters[index - 1]) {
            bonus += 8;
        }
        score += bonus;
        previous_match = index as i64;
        query_index += 1;
        if query_index == query.len() {
            return Some(score);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quick_access::data::QuickAccessExtension;

    fn data_with_lazygit() -> QuickAccessData {
        QuickAccessData {
            extensions: vec![QuickAccessExtension {
                id: "lazygit".to_string(),
                title: "Lazygit".to_string(),
                description: "Manage the active project's Git repository.".to_string(),
                kind: "terminal".to_string(),
            }],
            ..QuickAccessData::default()
        }
    }

    #[test]
    fn an_enabled_extension_is_an_open_row_found_by_its_name() {
        let data = data_with_lazygit();
        let groups = command_groups("lazygit", &data);
        let rows: Vec<&QuickAccessRow> = groups.iter().flat_map(|group| &group.rows).collect();
        let QuickAccessRow::Command { key, title, .. } = rows.first().expect("a row") else {
            panic!("a command row");
        };
        assert_eq!(key, "extension:lazygit");
        assert_eq!(title, "Open Lazygit");
        let at_rest = command_groups("", &data);
        assert!(at_rest.iter().flat_map(|group| &group.rows).any(
            |row| matches!(row, QuickAccessRow::Command { key, .. } if key == "extension:lazygit")
        ));
    }

    #[test]
    fn running_the_row_closes_quick_access_and_asks_the_host_to_launch_it() {
        let data = data_with_lazygit();
        assert_eq!(
            run_command_row("extension:lazygit", &data),
            vec![
                CommandRun::Close,
                CommandRun::Post(json!({ "extensionId": "lazygit", "type": "launchExtension" })),
            ]
        );
    }

    fn has_row(data: &QuickAccessData, wanted: &str) -> bool {
        command_groups("", data)
            .iter()
            .flat_map(|group| &group.rows)
            .any(|row| matches!(row, QuickAccessRow::Command { key, .. } if key == wanted))
    }

    #[test]
    fn session_note_is_a_row_only_while_session_notes_is_on() {
        let mut data = QuickAccessData::default();
        assert!(!has_row(&data, "hotkey:sessionNote"), "off by default");
        assert!(has_row(&data, "hotkey:stashPrompt"));
        data.hud = json!({ "settings": { "sessionNotesHidden": false } });
        assert!(has_row(&data, "hotkey:sessionNote"));
        data.hud = json!({ "settings": { "sessionNotesHidden": true } });
        assert!(!has_row(&data, "hotkey:sessionNote"));
    }

    #[test]
    fn no_extension_rows_without_enabled_extensions() {
        let data = QuickAccessData::default();
        assert!(command_groups("", &data)
            .iter()
            .flat_map(|group| &group.rows)
            .all(|row| !matches!(row, QuickAccessRow::Command { key, .. } if key.starts_with("extension:"))));
    }
}
