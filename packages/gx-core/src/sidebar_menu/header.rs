//! The buttons on a project header row, and the agent launcher behind the last one.
//!
//! Ported from the TypeScript sidebar page's project actions and agent launcher (frozen in the
//! deleted `tooling/gx-core/sidebar-page-frozen/project-actions.ts` and `agent-launcher.ts`; see
//! git history).

use crate::sidebar_accounts::AccountsState;
use crate::sidebar_view::SidebarSettings;

use super::agent_logos::colored_agent_logo;
use super::commands::{message, MenuCommand};
use super::group::MenuGroup;
use super::host::MenuHost;
use super::item::{MenuItem, MenuSplit};
use super::text::transcript_agent;

/// The project's pinned Actions (the ones shown on the project row), as header rows.
fn pinned_actions(
    group_id: &str,
    project: &crate::sidebar_view::view::ProjectContextView,
    settings: &SidebarSettings,
    host: &MenuHost,
) -> Vec<MenuItem> {
    let mut pinned = Vec::new();
    let project_commands = host
        .project_commands
        .get(project.project_id.as_str())
        .map(Vec::as_slice)
        .unwrap_or_default();
    for (scope, commands) in [
        ("global", host.global_commands.as_slice()),
        ("project", project_commands),
    ]
    .into_iter()
    .filter(|_| settings.actions_enabled)
    {
        for command in commands
            .iter()
            .filter(|command| command.show_on_project_row)
        {
            let label = command.name.trim();
            pinned.push(MenuItem::row(
                if label.is_empty() {
                    "Run Action"
                } else {
                    label
                },
                command.icon.as_deref().unwrap_or("bolt"),
                MenuCommand::command(message::run_sidebar_command(
                    &command.command_id,
                    scope,
                    group_id,
                )),
            ));
        }
    }
    pinned
}

/// `createNativeProjectHeaderActions`.
///
/// CDXC:AgentLauncher 2026-09-18 DECISION:
/// User: remove the gap between the last-used agent button and the Select agent button in the
/// project header. Both halves render as the one split button the React header shows.
///
/// CDXC:Bots 2026-09-27 DECISION:
/// User: a bot row shows its pinned Actions, Edit SOUL, Edit config and one "+", because a bot is not a repo: no worktree, PR, history, browser or terminal button, and no agent split with its picker.
/// Edit SOUL and Edit config always open that bot's own `SOUL.md` and `config.yaml`, in Ghostex's built-in Code view under the bot.
/// They are fixed buttons on every bot row on this computer, not editable or deletable Actions; a remote computer's bot has none, because its file would open on this computer.
/// Supersedes the 2026-09-26 decision (pinned Actions and "+" only, with seeded `code <file>` Actions).
pub fn project_header_actions(
    group: &MenuGroup<'_>,
    settings: &SidebarSettings,
    host: &MenuHost,
) -> Vec<MenuItem> {
    let group_id = group.group_id;
    let Some(project) = group.project else {
        return vec![MenuItem::row(
            "Create a Terminal",
            "plus",
            MenuCommand::command(message::create_session_in_group(group_id)),
        )];
    };
    let mut pinned = pinned_actions(group_id, project, settings, host);
    if project.bot_profile.is_some() {
        let edit_files: &[_] = if group.is_remote {
            &[]
        } else {
            &[
                ("Edit SOUL", "brain", "SOUL.md"),
                ("Edit config", "settings", "config.yaml"),
            ]
        };
        for &(label, icon, file) in edit_files {
            let file_path = std::path::Path::new(&project.path).join(file);
            pinned.push(MenuItem::row(
                label,
                icon,
                MenuCommand::command(message::open_bot_file(
                    group_id,
                    &project.path,
                    &file_path.to_string_lossy(),
                )),
            ));
        }
        pinned.push(MenuItem::row(
            &format!("New {} session", group.title),
            "plus",
            MenuCommand::project_action(group_id, "bot", None),
        ));
        return pinned;
    }
    // CDXC:Sidebar 2026-10-20 DECISION:
    // User: "add to the left of the 'select agent' combined button a button to start a terminal in that project ... i want it back at top level for each project header". New Terminal is a header button again, right before the agent split, and stays in the ⋯ menu too. This supersedes the 2026-10-08 decision below for that one button; the other moved buttons stay in the ⋯ menu.
    let mut actions = vec![MenuItem::row(
        "New Terminal",
        "terminal-2",
        MenuCommand::command(message::create_project_terminal(group_id)),
    )
    .with_hotkey("createSession")];
    let primary = host.primary_agent_at(&group.agent_place);
    let primary_icon = primary.and_then(|agent| agent.icon.as_deref());
    actions.push(MenuItem {
        label: Some(format!(
            "Create {}",
            primary.map_or("Agent", |agent| agent.name.as_str())
        )),
        icon: Some("sparkles".to_string()),
        agent_icon: primary_icon.map(str::to_string),
        image_data_url: primary_icon
            .and_then(colored_agent_logo)
            .map(str::to_string),
        command: Some(MenuCommand::project_action(
            group_id,
            "agent",
            primary.map(|agent| agent.agent_id.as_str()),
        )),
        split: Some(MenuSplit::Start),
        hotkey: Some("createAgentSession".to_string()),
        ..MenuItem::default()
    });
    actions.push(MenuItem {
        label: Some("Select Agent".to_string()),
        icon: Some("chevron-down".to_string()),
        children: Some(agent_launcher_items(group_id, &group.agent_place, host)),
        split: Some(MenuSplit::End),
        ..MenuItem::default()
    });
    actions
}

/// The project header buttons that live in the project's ⋯ menu: Add Worktree or Create PR, History, New Browser Tab, Create Terminal and the pinned Actions.
///
/// CDXC:Sidebar 2026-10-08 DECISION:
/// User (for the phone, applied to the desktop header too): move most per-project buttons into the 3-dots menu, keeping only the agent picker and New agent (New Terminal came back to the header on 2026-10-20, see `project_header_actions`). A header that shows six buttons on hover pushed the project name out of a 200px sidebar. A bot row keeps its own few buttons.
/// SEE-ALSO: `SidebarMenus::header_actions` puts the ⋯ button in front of the agent split; `project_menu` starts with these rows.
pub fn project_header_menu_rows(
    group: &MenuGroup<'_>,
    settings: &SidebarSettings,
    host: &MenuHost,
) -> Vec<MenuItem> {
    let group_id = group.group_id;
    let Some(project) = group.project else {
        return Vec::new();
    };
    if project.bot_profile.is_some() {
        return Vec::new();
    }
    let mut actions = vec![
        if project.worktree.is_some() {
            MenuItem::row(
                "Create PR",
                "git-pull-request",
                MenuCommand::command(message::run_sidebar_git_action(group_id, "pr")),
            )
        } else {
            MenuItem::row(
                "Add Worktree",
                "git-branch",
                MenuCommand::project_action(group_id, "worktree", None),
            )
        },
        MenuItem::row(
            "History",
            "history",
            MenuCommand::project_action(group_id, "history", None),
        ),
    ];
    if !settings.browser_view_tab_hidden {
        actions.push(
            MenuItem::row(
                "New Browser Tab",
                "world",
                MenuCommand::command(message::open_browser_pane_in_group(group_id)),
            )
            .with_hotkey("openBrowserPane"),
        );
    }
    actions.push(
        MenuItem::row(
            "Create Terminal",
            "terminal-2",
            MenuCommand::command(message::create_project_terminal(group_id)),
        )
        .with_hotkey("createSession"),
    );
    // CDXC:WorkMode 2026-10-09 DECISION:
    // User: "We need a button to create a linear ticket to start work in the ... dropdown in the project header" (also from the Work page). Only a project of this computer with a Linear key has it (with GitHub as the tracker, one where `gh` is signed in); New session itself stays on main with no worktree.
    // CDXC:WorkMode 2026-10-09 DECISION:
    // User: the workspace's primary tracker is "Linear Tickets & Projects or Github Issues & Projects"; with GitHub the same item and dialog create a GitHub issue in the project's repo instead.
    let create_ticket = if project.work_github {
        Some(("Create GitHub Issue…", "brand-github"))
    } else if project.work_linear && !project.work_tracker_github {
        Some(("Create Linear Ticket…", "brand-linear"))
    } else {
        None
    };
    if let Some((label, icon)) = create_ticket.filter(|_| !group.is_remote && !group.is_stale) {
        actions.insert(
            0,
            MenuItem::row(
                label,
                icon,
                MenuCommand::project_action(group_id, "createLinearTicket", None),
            ),
        );
    }
    actions.append(&mut pinned_actions(group_id, project, settings, host));
    actions
}

/// `nativeAgentLauncherItems(groupId)`, the state before any account list has been read. The
/// ticked row is the agent the group's `place` starts.
pub fn agent_launcher_items(
    group_id: &str,
    place: &super::AgentChoicePlace,
    host: &MenuHost,
) -> Vec<MenuItem> {
    agent_launcher_items_with_accounts(group_id, place, host, None)
}

/// `nativeAgentLauncherItems(groupId, data)`: with an account list read, each account button
/// carries how many registered accounts its provider has. The pages behind the buttons are
/// `sidebar_accounts/`.
pub fn agent_launcher_items_with_accounts(
    group_id: &str,
    place: &super::AgentChoicePlace,
    host: &MenuHost,
    accounts: Option<&AccountsState>,
) -> Vec<MenuItem> {
    let primary_id = host
        .primary_agent_at(place)
        .map(|agent| agent.agent_id.as_str());
    let mut items: Vec<MenuItem> = host
        .agents
        .iter()
        .map(|agent| {
            let icon = agent.icon.as_deref();
            MenuItem {
                primary: Some(agent.agent_id.as_str()) == primary_id,
                supports_chat: transcript_agent(Some(&agent.agent_id), icon).is_some(),
                label: Some(agent.name.clone()),
                agent_icon: icon.map(str::to_string),
                image_data_url: icon.and_then(colored_agent_logo).map(str::to_string),
                icon: icon.is_none().then(|| "code".to_string()),
                keep_open: true,
                command: Some(MenuCommand::agent_accounts(
                    group_id,
                    "launch",
                    Some(&agent.agent_id),
                )),
                secondary: agent.account_provider().map(|provider| {
                    super::item::MenuSecondary {
                        icon: "user".to_string(),
                        // The count arrives with the account list; until then the pill is blank.
                        label: accounts
                            .map(|accounts| {
                                accounts.registered_for(Some(provider)).count().to_string()
                            })
                            .unwrap_or_default(),
                        command: MenuCommand::agent_accounts(
                            group_id,
                            "accounts",
                            Some(&agent.agent_id),
                        ),
                    }
                }),
                ..MenuItem::default()
            }
        })
        .collect();
    if !items.is_empty() {
        items.push(MenuItem::separator());
    }
    items.extend(super::run_in_box::run_in_box_row(group_id, host));
    items.insert(0, new_coordinator_row(group_id));
    items.insert(1, MenuItem::separator());
    items.push(MenuItem::row(
        "Configure",
        "settings",
        MenuCommand::project_action(group_id, "agent", None),
    ));
    if let Some(first) = items.first_mut() {
        first.agent_launcher = true;
        first.menu_owner = Some(format!("group:{group_id}"));
        first.on_open = Some(MenuCommand::agent_accounts(group_id, "load", None));
    }
    items
}

/// The launcher's "New Coordinator…" row.
///
/// CDXC:Coordinators 2026-10-03 DECISION:
/// User: "Please move this to the top in both apps": "New Coordinator…" is the first row of a project's agent launcher, above the agents, on the desktop, the web build and the phone. A coordinator is started where agents are started, so the launcher offers it; the dialog behind it names the coordinator and picks its agent. Supersedes the 2026-09-30 placement after Run in a box.
/// SEE-ALSO: `agentMenuItems` in apps/mobile/app/src/screens/sessions-screen/use-sessions-screen-menus.tsx (the phone's agent menu keeps the same order).
fn new_coordinator_row(group_id: &str) -> MenuItem {
    MenuItem {
        info: Some(NEW_ORCHESTRATOR_BENEFITS.to_string()),
        ..MenuItem::row(
            "New Coordinator…",
            "users-group",
            MenuCommand::project_action(group_id, "coordinator", None),
        )
    }
}

/// The tooltip behind the (i) beside "New Orchestrator…".
///
/// CDXC:Coordinators 2026-10-17 DECISION:
/// User: "please add an i icon next to the "New Orchestrator..." menu item that can be hovered to explain the benefits of using it, in short bullet points. Say it's recommended, and it's the main way Ghostex is built, and you need to use a very good model for it (ex. Opus 5.5 high or xhigh), you just talk to 1 agent and work gets done, and it keeps improving as it saves more memories about your project". The phone shows the same text when its (i) is tapped.
/// SEE-ALSO: apps/desktop/src/app/native_sidebar/agent_launcher_menu.rs (the icon and tooltip), apps/mobile/app/src/screens/sessions-screen/use-sessions-screen-menus.tsx (the phone's copy).
pub const NEW_ORCHESTRATOR_BENEFITS: &str = "Recommended
• The main way Ghostex itself is built
• You talk to one agent and the work gets done
• It keeps improving as it saves memories about your project
• Needs a very good model, e.g. Opus 5.5 at high or xhigh";

/// `providerFor`: only Claude and Codex have an account switcher.
pub(crate) fn account_provider(agent_id: &str, icon: Option<&str>) -> Option<&'static str> {
    match icon.unwrap_or(agent_id) {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        _ => None,
    }
}
