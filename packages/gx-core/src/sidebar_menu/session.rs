//! A session row's context menu and its hover buttons.
//!
//! Ported row for row from the sidebar page's session menu (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/session-menu.ts`; see git history).
//!
//! CDXC:ContextMenus 2026-09-19 DECISION (2026-11-08 update):
//! User, 2026-11-08: "this pin is fugly please fix. we should use the other look for the pin we have in the app already (filled / unfilled)". Every pin in the app is now the one Tabler pin pair: Pin shows the outline (`pin-outline`) and Unpin the filled pin (`pin-filled`), in this menu, its hover button and the bulk menus. This supersedes the 2026-09-19 upright pushpin and crossed-out pushpin. Every sidebar menu label is Title Case ("Close Inactive",
//! "Pin Selected"), except disabled status sentences.

use crate::sidebar_view::ordering::session_is_snoozed;
use crate::sidebar_view::tags::{enabled_visible_tag_sections, tag_presentation, TagCatalog};
use crate::sidebar_view::view::SessionRow;
use crate::sidebar_view::SidebarSettings;

use super::capabilities::{can_sleep, SessionCapabilities};
use super::commands::{message, MenuCommand};
use super::group::MenuGroup;
use super::hover::{hover_strip, HoverAction, HoverStrip};
use super::item::MenuItem;
use super::layout::{self, SessionMenuSpot};
use super::text::transcript_agent;

/// The four snooze presets and their labels.
const SNOOZE_PRESETS: [(&str, &str); 4] = [
    ("oneHour", "1 Hour"),
    ("threeHours", "3 Hours"),
    ("tomorrow", "Tomorrow"),
    ("nextWeek", "Next Week"),
];

/// The Postpone By presets.
const POSTPONE_PRESETS: [(&str, i64); 5] = [
    ("10 Minutes", 600_000),
    ("30 Minutes", 1_800_000),
    ("1 Hour", 3_600_000),
    ("2 Hours", 7_200_000),
    ("5 Hours", 18_000_000),
];

/// What a row offers: its context menu and the buttons on either side of the hover chevron.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionActions {
    pub menu: Vec<MenuItem>,
    pub hover_before: Vec<MenuItem>,
    pub hover_after: Vec<MenuItem>,
    pub hover_chevron: bool,
}

/// Everything a row's menu reads besides the row itself.
pub struct SessionMenuInput<'a> {
    pub row: &'a SessionRow,
    pub group: &'a MenuGroup<'a>,
    pub settings: &'a SidebarSettings,
    pub catalog: &'a TagCatalog,
    /// The rows drawn below this one in its group, for Sleep Below and Close Below. Empty for the
    /// per-publish build, which never offers them.
    pub below: &'a [&'a SessionRow],
    /// An orchestrator row with at least one thread its Sleep Inactive Threads would sleep.
    pub has_inactive_threads: bool,
    pub now_ms: u64,
    /// Where Move to Group can send the row. `None` for the per-publish build, which never offers
    /// it, and for a row outside a project and its user-made groups.
    pub group_moves: Option<&'a SessionGroupMoves>,
}

/// The groups a session row can move between: its project's user-made groups and the project's
/// own list, which Remove from Group moves it back to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionGroupMoves {
    pub project_list_id: String,
    pub groups: Vec<SessionGroupTarget>,
    /// For a chat in No project: the projects Move to project can take it to (this computer's
    /// own projects, without Ghostex's), as `(project id, name)`. Empty for every other row.
    pub projects: Vec<(String, String)>,
}

/// One of the project's user-made groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionGroupTarget {
    pub group_id: String,
    pub title: String,
    /// The group holding the row now.
    pub is_current: bool,
}

/// The rows the hover strip and the context menu both pick from.
#[derive(Default)]
struct ActionRows {
    close: Option<MenuItem>,
    rename: Option<MenuItem>,
    note: Option<MenuItem>,
    sleep: Option<MenuItem>,
    pin: Option<MenuItem>,
    park: Option<MenuItem>,
    tag: Option<MenuItem>,
    snooze: Option<MenuItem>,
    close_after_done: Option<MenuItem>,
}

impl ActionRows {
    fn get(&self, action: HoverAction) -> Option<&MenuItem> {
        match action {
            HoverAction::Close => self.close.as_ref(),
            HoverAction::Rename => self.rename.as_ref(),
            HoverAction::Note => self.note.as_ref(),
            HoverAction::Sleep => self.sleep.as_ref(),
            HoverAction::Pin => self.pin.as_ref(),
            HoverAction::Park => self.park.as_ref(),
            HoverAction::Tag => self.tag.as_ref(),
            HoverAction::Snooze => self.snooze.as_ref(),
            HoverAction::CloseAfterDone => self.close_after_done.as_ref(),
        }
    }
}

/// The lazy placeholder a row publishes instead of its menu.
///
/// CDXC:ContextMenus 2026-09-20 WHY:
/// Building every row's menu, its Copy Details text and its Below actions on every publish is what
/// made pane switching slow enough to notice (the TypeScript measured 30 to 40 ms per publish over
/// two hundred rows). A row publishes this one item; the host answers `sessionMenu` with the full
/// menu for the one row the user opened.
fn lazy_menu(session_id: &str, action: Option<HoverAction>) -> Vec<MenuItem> {
    let owner = format!("session:{session_id}");
    vec![MenuItem {
        label: Some("Loading…".to_string()),
        // CDXC:ContextMenus 2026-11-20 DECISION: User: "fix these context menu items not having an icon". The placeholder shows the spinner the app draws for every loading state, so the menu's rows all carry an icon.
        icon: Some("loader2".to_string()),
        disabled: true,
        menu_owner: Some(owner.clone()),
        on_open: Some(MenuCommand::session_menu(
            session_id,
            action.map(HoverAction::as_str),
            &owner,
        )),
        ..MenuItem::default()
    }]
}

/// The row's hover buttons and the placeholder menu, for the per-publish build.
pub fn session_hover_actions(input: &SessionMenuInput<'_>) -> SessionActions {
    build(input, false)
}

/// The row's whole context menu, for the row the user opened.
pub fn session_menu(input: &SessionMenuInput<'_>) -> Vec<MenuItem> {
    build(input, true).menu
}

/// The submenu behind one hover button, for the button the user opened.
pub fn session_hover_submenu(input: &SessionMenuInput<'_>, action: HoverAction) -> Vec<MenuItem> {
    let strip = hover_strip(&input.settings.session_card_hover_buttons);
    if !strip.includes(action) {
        return Vec::new();
    }
    let (rows, _) = action_rows(input, true);
    rows.get(action)
        .and_then(|item| item.children.clone())
        .unwrap_or_default()
}

/// The submenu of one of the row's actions (Snooze's presets, Park's tags) whether or not the
/// user's hover strip shows that action: the Unified sidebar's own Snooze button opens it.
pub fn session_action_submenu(input: &SessionMenuInput<'_>, action: HoverAction) -> Vec<MenuItem> {
    let (rows, _) = action_rows(input, true);
    rows.get(action)
        .and_then(|item| item.children.clone())
        .unwrap_or_default()
}

/// CDXC:Sessions 2026-09-12 DECISION:
/// User: hover buttons are icons only; the tooltip names the action. A button on a pinned, parked or snoozed row shows the reverse action (Unpin, Unpark, Unsnooze) so one slot toggles the state, which is why the strip reuses the menu's own action rows.
fn build(input: &SessionMenuInput<'_>, include_menu: bool) -> SessionActions {
    let (rows, caps) = action_rows(input, include_menu);
    let strip = hover_strip(&input.settings.session_card_hover_buttons);
    let hover = |actions: &[HoverAction]| -> Vec<MenuItem> {
        actions
            .iter()
            .filter_map(|action| rows.get(*action).cloned())
            .collect()
    };
    let actions = SessionActions {
        menu: Vec::new(),
        hover_before: hover(&strip.before),
        hover_after: hover(&strip.after),
        hover_chevron: strip.chevron,
    };
    if !include_menu {
        return SessionActions {
            menu: lazy_menu(&input.row.sidebar_session_id, None),
            ..actions
        };
    }
    SessionActions {
        menu: full_menu(input, &rows, &caps, &strip),
        ..actions
    }
}

/// The per-action rows, plus the capabilities they were resolved from.
fn action_rows(
    input: &SessionMenuInput<'_>,
    include_menu: bool,
) -> (ActionRows, SessionCapabilities) {
    let row = input.row;
    let settings = input.settings;
    let id = row.sidebar_session_id.as_str();
    let caps = SessionCapabilities::resolve(row, input.group.is_remote);
    let tags = tag_items(input, include_menu);
    let parked = row.is_parked;
    let mut park = layout_row(
        "park",
        parked,
        MenuCommand::command(message::set_session_parked(id, !parked)),
    );
    if !parked && settings.show_tag_menu_when_parking && caps.can_tag_session && !tags.is_empty() {
        park.children = Some(if !include_menu {
            lazy_menu(id, Some(HoverAction::Park))
        } else {
            let mut children = vec![
                MenuItem::row(
                    "No Tag Change",
                    "tag-off",
                    MenuCommand::command(message::set_session_parked(id, true)),
                ),
                MenuItem::separator(),
            ];
            children.extend(tags.iter().map(|item| park_with_tag(item, id, row)));
            children
        });
        park.command = None;
    }
    let snooze_presets: Vec<MenuItem> = if !include_menu {
        lazy_menu(id, Some(HoverAction::Snooze))
    } else {
        SNOOZE_PRESETS
            .iter()
            .map(|(preset, label)| {
                if settings.show_tag_menu_when_parking && caps.can_tag_session && !tags.is_empty() {
                    let mut children = vec![
                        MenuItem::row("No Tag Change", "alarm", MenuCommand::snooze(id, preset)),
                        MenuItem::separator(),
                    ];
                    children.extend(
                        tags.iter()
                            .map(|item| snooze_with_tag(item, id, preset, row)),
                    );
                    MenuItem {
                        label: Some((*label).to_string()),
                        children: Some(children),
                        ..MenuItem::default()
                    }
                } else {
                    MenuItem {
                        label: Some((*label).to_string()),
                        command: Some(MenuCommand::snooze(id, preset)),
                        ..MenuItem::default()
                    }
                }
            })
            .collect()
    };
    let sleeping = row.lifecycle_state == "sleeping";
    // The same function the section rule and the action surface read, not a second copy of its
    // comparison: the menu that offers Unsnooze and the section that draws the row must change at
    // the identical millisecond.
    let snoozed = session_is_snoozed(row.timing.snoozed_until_ms, input.now_ms);
    // A coordinator's thread always shows under its coordinator, so it offers none of the three
    // shelves; a row still on one (sent by an older gxserver) keeps its way back (Unpin, Unpark,
    // Unsnooze). CDXC:Coordinators 2026-11-25 in server/src/coordinators/thread_shelves.rs.
    let thread = row.stays_under_coordinator();
    let rows = ActionRows {
        close: Some(layout_row(
            "close",
            false,
            MenuCommand::command(message::close_session(id)),
        )),
        rename: caps
            .can_rename_session
            .then(|| layout_row("rename", false, MenuCommand::session_action(id, "rename"))),
        note: (settings.session_notes_enabled && caps.can_open_session_note)
            .then(|| layout_row("note", false, MenuCommand::session_action(id, "note"))),
        sleep: caps.can_sleep_session.then(|| {
            layout_row(
                "sleep",
                sleeping,
                MenuCommand::command(message::set_session_sleeping(id, !sleeping)),
            )
        }),
        pin: (caps.can_pin_session && (!thread || row.is_pinned)).then(|| {
            layout_row(
                "pin",
                row.is_pinned,
                MenuCommand::command(message::set_session_pinned(id, !row.is_pinned)),
            )
        }),
        park: (!caps.is_browser_session && settings.enable_session_parking && (!thread || parked))
            .then_some(park),
        tag: (caps.can_tag_session && !tags.is_empty())
            .then(|| layout_submenu("tagAs", tags.clone())),
        snooze: (!caps.is_browser_session && (!thread || snoozed)).then(|| {
            if snoozed {
                layout_row(
                    "snooze",
                    true,
                    MenuCommand::command(message::unsnooze_session(id)),
                )
            } else {
                layout_submenu("snooze", snooze_presets)
            }
        }),
        close_after_done: caps.can_close_after_done.then(|| {
            layout_row(
                "closeAfterDone",
                false,
                MenuCommand::command(message::toggle_close_after_done(id)),
            )
        }),
    };
    (rows, caps)
}

/// The Tag As options, or the one placeholder that stands in for them on a publish.
fn tag_items(input: &SessionMenuInput<'_>, include_menu: bool) -> Vec<MenuItem> {
    // Session Tags off: no Tag As, no hover Tag button, no tag prompt when parking or snoozing.
    if !input.settings.session_tags_enabled {
        return Vec::new();
    }
    let row = input.row;
    let id = row.sidebar_session_id.as_str();
    let tag = row.effective_tag.as_deref();
    let include: Vec<&str> = tag.into_iter().collect();
    let sections =
        enabled_visible_tag_sections(&input.settings.tag_list_items, input.catalog, &include);
    if !include_menu {
        return if !sections.is_empty() || !input.group.is_remote {
            lazy_menu(id, Some(HoverAction::Tag))
        } else {
            Vec::new()
        };
    }
    let mut items: Vec<MenuItem> = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            items.push(MenuItem::separator());
        }
        for (value, label) in &section.options {
            let presentation = tag_presentation(Some(value), input.catalog);
            let checked = tag == Some(value.as_str());
            let mut item = MenuItem::row(
                label,
                presentation
                    .as_ref()
                    .map_or("tag", |presentation| presentation.icon.as_str()),
                MenuCommand::command(message::set_session_tag(
                    id,
                    if checked { None } else { Some(value) },
                )),
            );
            if let Some(presentation) = &presentation {
                item.icon_color = Some(presentation.icon_color.clone());
            }
            items.push(item.with_checked(checked));
        }
    }
    if !input.group.is_remote {
        items.push(MenuItem::separator());
        items.push(MenuItem::row(
            "New Tag…",
            "plus",
            MenuCommand::sidebar_action("newTag"),
        ));
    }
    items
}

/// Add to Favorites / Remove from Favorites, in Tag As's place while Session Tags is off.
///
/// CDXC:SessionTags 2026-11-27 DECISION:
/// The coordinator, relaying the user: "Favorites keep working ... don't remove favouriting itself".
/// Favorite is a tag value (`sessionTag: favorite`) and Tag As was the only way to set it, so this
/// row sets and clears that same value; a favourite keeps its star on its card.
fn favorite_rows(id: &str, row: &SessionRow, caps: SessionCapabilities) -> Vec<MenuItem> {
    if !caps.can_tag_session {
        return Vec::new();
    }
    let favorite = row.effective_tag.as_deref() == Some(crate::sidebar_view::tags::FAVORITE_TAG);
    let (label, icon, tag) = if favorite {
        ("Remove from Favorites", "star-filled", None)
    } else {
        (
            "Add to Favorites",
            "star",
            Some(crate::sidebar_view::tags::FAVORITE_TAG),
        )
    };
    vec![MenuItem::row(
        label,
        icon,
        MenuCommand::command(message::set_session_tag(id, tag)),
    )]
}

/// A Tag As row turned into "set this tag, then park". A row without a `command` (a separator) and
/// a row whose command is not a runtime message (New Tag…) are kept as they are.
fn park_with_tag(item: &MenuItem, session_id: &str, row: &SessionRow) -> MenuItem {
    let Some(tag_message) = runtime_message(item) else {
        return item.clone();
    };
    let message = match set_session_tag_value(&tag_message) {
        // `sessionTag ?? tag ?? null`: the row for the tag the session already has clears it, so
        // parking through it must keep the tag rather than take it off.
        Some(None) => message::set_session_tag(session_id, row.effective_tag.as_deref()),
        _ => tag_message,
    };
    MenuItem {
        command: Some(MenuCommand::batch(
            vec![message, message::set_session_parked(session_id, true)],
            false,
        )),
        ..item.clone()
    }
}

/// A Tag As row turned into "snooze with this tag".
fn snooze_with_tag(item: &MenuItem, session_id: &str, preset: &str, row: &SessionRow) -> MenuItem {
    let Some(tag_message) = runtime_message(item) else {
        return item.clone();
    };
    let Some(tag) = set_session_tag_value(&tag_message) else {
        return item.clone();
    };
    let tag = match tag {
        Some(tag) => Some(tag),
        None => row.effective_tag.clone(),
    };
    MenuItem {
        command: Some(MenuCommand::snooze_with_tag(
            session_id,
            preset,
            tag.as_deref(),
        )),
        ..item.clone()
    }
}

/// The `{ type: 'command', message }` payload of a row, if it has one.
fn runtime_message(item: &MenuItem) -> Option<serde_json::Value> {
    let command = item.command.as_ref()?.as_json();
    if command.get("type")?.as_str()? != "command" {
        return None;
    }
    command.get("message").cloned()
}

/// `Some(tag)` when the message is a `setSessionTag`; the inner `None` is the clearing row.
fn set_session_tag_value(message: &serde_json::Value) -> Option<Option<String>> {
    if message.get("type").and_then(serde_json::Value::as_str) != Some("setSessionTag") {
        return None;
    }
    Some(
        message
            .get("sessionTag")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    )
}

/// Move to project, on a chat in No project: a page of this computer's projects.
///
/// CDXC:Projects 2026-11-27 DECISION:
/// User (Q5262 B, Q5284 A): a chat in No project moves to an existing project from its menu, at
/// once with no confirm. gxserver's `/api/moveSessionToProject` (server/src/no_project/move_chat.rs)
/// resumes a Claude or Codex chat in the project's folder and starts any other agent there from a
/// handover of the chat; the desktop then opens the moved chat (native_sidebar/move_to_project.rs).
/// Terminals are not offered: a shell has no conversation to carry.
fn move_to_project_item(
    id: &str,
    row: &SessionRow,
    moves: Option<&SessionGroupMoves>,
    stale: bool,
) -> Option<MenuItem> {
    let moves = moves.filter(|moves| !stale && !moves.projects.is_empty())?;
    if row.session_kind.as_deref() == Some("terminal") {
        return None;
    }
    let children = moves
        .projects
        .iter()
        .map(|(project_id, name)| {
            MenuItem::row(
                name,
                "folder",
                MenuCommand::command(message::move_session_to_project(id, project_id)),
            )
        })
        .collect();
    Some(MenuItem::submenu("Move to project", "folders", children).with_page())
}

/// Move to New Group, or the Move to Group page once the project has a group to move to or
/// the row is in one.
///
/// CDXC:ContextMenus 2026-11-05 DECISION:
/// User (Q1311): a group of a project's sessions is a "sessions group": its rows read Move to New
/// Group (renamed from Move to New Sessions Group, 2026-11-18: too long) and New Sessions Group, and "New Group" names a group of projects (Add to Group).
/// A sessions group never makes a folder; a new folder is Add Project's New project folder.
///
/// CDXC:ContextMenus 2026-10-23 WHY:
/// Move to New Group was the only way into a user-made group and nothing led back out: the drag
/// that does it was refused on the desktop, and Close Group closes the sessions it holds. The page
/// mirrors a project's Add to Group (New Group, the groups, Remove from Group) and sends
/// the drag's own `moveSessionToGroup`, so the menu and a drop make the same edit.
fn move_to_group_item(id: &str, moves: Option<&SessionGroupMoves>, stale: bool) -> MenuItem {
    let new_group = |label: &str| {
        MenuItem::row(
            label,
            "layout-sidebar-right-expand",
            MenuCommand::command(message::create_group_from_session(id)),
        )
    };
    let Some(moves) = moves.filter(|moves| !stale && !moves.groups.is_empty()) else {
        return new_group("Move to New Group");
    };
    let mut children = vec![new_group("New Sessions Group")];
    for group in &moves.groups {
        children.push(MenuItem {
            label: Some(group.title.clone()),
            icon: Some("folder".to_string()),
            checked: group.is_current,
            command: (!group.is_current)
                .then(|| MenuCommand::command(message::move_session_to_group(id, &group.group_id))),
            ..MenuItem::default()
        });
    }
    if moves.groups.iter().any(|group| group.is_current) {
        children.push(MenuItem::separator());
        children.push(MenuItem::row(
            "Remove from Group",
            "x",
            MenuCommand::command(message::move_session_to_group(id, &moves.project_list_id)),
        ));
    }
    MenuItem::submenu("Move to Group", "layout-sidebar-right-expand", children).with_page()
}

/// A coordinator's rows (Make main coordinator and the Coordinator submenu), a lead link's Unlink
/// and a lead's Stop leading: the menu's `coordinator` place. A thread's Mark Thread Done or Reopen
/// Thread is `thread_rows`.
///
/// CDXC:Coordinators 2026-10-13 DECISION:
/// User: "Mark done / Reopen on threads, Goal & instructions and Memory editing on coordinators on both the phone and the desktop" (2026-10-01), ported onto the Orchestrator menus ("port all of it, rebased onto main"). Goal & Instructions and Memory open editors that save through gxserver; Mark Thread Done does what `ghostex orchestrator resolve` does (marks the thread done and closes its session) and is disabled while the thread works or waits on an answer, which resolve refuses; Reopen Thread shows instead on a done thread still in the list. A machine that is not connected gets none of them. The phone's menus fill the same places (layout.rs).
fn coordinator_rows(row: &SessionRow, stale: bool, has_inactive_threads: bool) -> Vec<MenuItem> {
    let id = row.sidebar_session_id.as_str();
    let mut rows = Vec::new();
    if row.is_draft {
        return rows;
    }
    if stale {
        // A coordinator whose computer is not connected has no Coordinator submenu, so its Sleep
        // Inactive Threads stays a row of its own, as before.
        if row.is_coordinator {
            rows.push(
                MenuItem::row(
                    "Sleep Inactive Threads",
                    "moon",
                    MenuCommand::command(message::sleep_inactive_coordinator_threads(id)),
                )
                .with_disabled(!has_inactive_threads),
            );
        }
        return rows;
    }
    if row.is_coordinator {
        // CDXC:Coordinators 2026-11-10 SEE-ALSO: the user's decision in
        // server/src/coordinators/main_coordinator.rs ("i should be able to give it to another
        // coordinator on that machine by right clicking on it"). Hidden on the main one and on Chief
        // of Staff, which never carries the label (gxserver refuses a GBot's chat too); the phone's long-press menu
        // (use-sessions-screen-menus.tsx) offers the same.
        if !row.is_main_coordinator && !row.is_chief {
            rows.push(MenuItem::row(
                "Make Main Coordinator",
                "coordinator-crown-points",
                MenuCommand::coordinator_session(id, "makeMain"),
            ));
        }
        // CDXC:Coordinators 2026-11-16 DECISION:
        // User (Android screenshot of the long-press menu of a coordinator): "the context menu for
        // coordinator is too long. Also view playbook didn't have icon. Pls put all these under 1
        // menu", then confirmed the desktop gets the same. Goal & Instructions, Memory, View
        // Playbook, Playbook Overrides and Sleep Inactive Threads are one "Coordinator" submenu
        // (the coordinator's crown icon) where the loose rows were; Make Main Coordinator stays a
        // row of its own above it. View Playbook's icon (`book`) is drawn on the phone too.
        let mut children = vec![
            MenuItem::row(
                "Goal & Instructions…",
                "checklist",
                MenuCommand::coordinator_session(id, "goal"),
            ),
            MenuItem::row(
                "Memory…",
                "brain",
                MenuCommand::coordinator_session(id, "memory"),
            ),
            // CDXC:Coordinators 2026-11-09 WHY: the user, after View playbook had shipped inside the
            // New Coordinator dialog, Settings and the overrides editor: "i still don't know how a
            // user can see the coordinator playbook". The playbook this coordinator reads (Ghostex's,
            // the reply format, then the user's overrides) opens straight from its own menu.
            MenuItem::row(
                "View Playbook",
                "book",
                MenuCommand::coordinator_session(id, "viewPlaybook"),
            ),
            // CDXC:Coordinators 2026-11-07: the user's playbook overrides (for every coordinator on
            // this one's computer) open from here too; see server/src/coordinators/playbook.rs.
            MenuItem::row(
                "Playbook Overrides…",
                "pencil",
                MenuCommand::coordinator_session(id, "playbookOverrides"),
            ),
        ];
        // CDXC:Coordinators 2026-10-13 SEE-ALSO: the user's decision on `coordinator_thread_keys` in sidebar_actions/bulk.rs ("right click on orchestrator and sleep", "Should just be sleep inactive not park"). Disabled, like the project menu's Sleep Inactive, when no thread can be slept.
        children.push(
            MenuItem::row(
                "Sleep Inactive Threads",
                "moon",
                MenuCommand::command(message::sleep_inactive_coordinator_threads(id)),
            )
            .with_disabled(!has_inactive_threads),
        );
        rows.push(layout_submenu("coordinator", children));
    } else if row.lead_linked {
        // CDXC:LeadLinks 2026-11-20 SEE-ALSO: server/src/agents/lead_links/mod.rs (the user: "Unlink
        // is fine to add but I don't want linking sessions to be a context menu item at all"). A
        // linked session is an ordinary session: Unlink only, never Mark Thread Done.
        rows.push(MenuItem::row(
            "Unlink from Lead",
            "unlink",
            MenuCommand::coordinator_session(id, "unlinkLead"),
        ));
    }
    if row.is_lead && !row.is_coordinator && row.lead_link_count > 0 {
        let count = row.lead_link_count;
        rows.push(MenuItem::row(
            &if count == 1 {
                "Stop Leading 1 Session".to_string()
            } else {
                format!("Stop Leading {count} Sessions")
            },
            "unlink",
            MenuCommand::coordinator_session(id, "stopLeading"),
        ));
    }
    rows
}

/// A thread's Mark Thread Done, or Reopen Thread once it is done.
fn thread_rows(row: &SessionRow, stale: bool) -> Vec<MenuItem> {
    if stale || row.is_draft || row.is_coordinator || row.lead_linked {
        return Vec::new();
    }
    if row.coordinator_parent.is_none() {
        return Vec::new();
    }
    let id = row.sidebar_session_id.as_str();
    let state = row.thread_state.as_deref();
    if state == Some("done") {
        vec![layout_row(
            "markThreadDone",
            true,
            MenuCommand::coordinator_session(id, "reopen"),
        )]
    } else {
        vec![layout_row(
            "markThreadDone",
            false,
            MenuCommand::coordinator_session(id, "markDone"),
        )
        .with_disabled(matches!(state, Some("working" | "waiting")))]
    }
}

/// A row named and drawn as the one session menu names it (layout.rs); `alt` takes the reverse a
/// toggle reads (Unpin, Wake, Reopen Thread).
fn layout_row(id: &str, alt: bool, command: MenuCommand) -> MenuItem {
    let entry = layout::entry(id).unwrap_or_else(|| panic!("no session menu entry {id}"));
    let label = if alt {
        entry.alt_label.unwrap_or(entry.label)
    } else {
        entry.label
    };
    let icon = if alt {
        entry.alt_icon.unwrap_or(entry.icon)
    } else {
        entry.icon
    };
    let mut item = MenuItem::row(label, icon, command);
    if let Some(hotkey) = entry.hotkey {
        item = item.with_hotkey(hotkey);
    }
    if entry.danger {
        item = item.with_danger();
    }
    item
}

fn layout_submenu(id: &str, children: Vec<MenuItem>) -> MenuItem {
    let entry = layout::entry(id).unwrap_or_else(|| panic!("no session menu entry {id}"));
    MenuItem::submenu(entry.label, entry.icon, children)
}

/// A box session's Open Box Web App, Open Box Screen, Stop Box and Destroy Box…, in one submenu.
/// Only this computer's rows: `/api/agentbox` answers for the gxserver the app talks to, and a
/// remote row's box lives on that machine.
fn box_rows(id: &str, row: &SessionRow, group: &MenuGroup<'_>) -> Vec<MenuItem> {
    if row.agentbox.is_none() || group.is_remote || group.is_stale {
        return Vec::new();
    }
    vec![layout_submenu(
        "box",
        vec![
            MenuItem::row(
                "Open Box Web App",
                "world",
                MenuCommand::agentbox_session(id, "openWeb"),
            ),
            MenuItem::row(
                "Open Box Screen",
                "device-desktop",
                MenuCommand::agentbox_session(id, "openScreen"),
            ),
            MenuItem::row(
                "Stop Box",
                "player-stop",
                MenuCommand::agentbox_session(id, "stop"),
            ),
            MenuItem::row(
                "Destroy Box…",
                "trash",
                MenuCommand::agentbox_session(id, "destroy"),
            )
            .with_danger(),
        ],
    )]
}

/// A Claude cloud session's own rows, in one submenu. Only this computer's rows: the session's
/// link and the worktree it comes back to belong to the gxserver that started it.
/// CDXC:ClaudeCloud 2026-10-23 DECISION: User: Bring to this computer goes "into a new worktree; the new local row takes over and the cloud row parks", and the row's menu also opens the session on claude.ai.
fn claude_cloud_rows(id: &str, row: &SessionRow, group: &MenuGroup<'_>) -> Vec<MenuItem> {
    let Some(cloud) = row
        .claude_cloud
        .as_ref()
        .filter(|_| !group.is_remote && !group.is_stale)
    else {
        return Vec::new();
    };
    let has_page = cloud.url.is_some();
    let mut children = vec![
        MenuItem::row(
            "Open on claude.ai",
            "external-link",
            MenuCommand::claude_cloud_session(id, "open"),
        )
        .with_disabled(!has_page),
        MenuItem::row(
            "Bring to This Computer",
            "device-laptop",
            MenuCommand::claude_cloud_session(id, "bringBack"),
        )
        .with_disabled(!has_page || cloud.state == "moved"),
    ];
    // Offered while the session's account is out of usage (CDXC:ClaudeCloud in
    // server/src/claude_cloud/accounts.rs).
    if cloud.limit_notice.is_some() && cloud.state == "running" {
        children.push(MenuItem::row(
            "Start a Copy on Another Account…",
            "copy",
            MenuCommand::claude_cloud_session(id, "copy"),
        ));
    }
    vec![layout_submenu("claudeAi", children)]
}

/// The Postpone By presets, Edit Delayed Send and Disable Delayed Send, while a delayed send is
/// armed.
fn postpone_rows(id: &str, row: &SessionRow, caps: &SessionCapabilities) -> Vec<MenuItem> {
    let armed = row
        .delayed_send
        .as_ref()
        .is_some_and(|delayed| delayed.deadline_at.is_some());
    if !caps.can_delayed_send || !armed {
        return Vec::new();
    }
    let mut children: Vec<MenuItem> = POSTPONE_PRESETS
        .iter()
        .map(|(label, delay_ms)| {
            MenuItem::row(
                label,
                "clock",
                MenuCommand::command(message::postpone_delayed_send(id, *delay_ms)),
            )
        })
        .collect();
    children.push(MenuItem::separator());
    children.push(MenuItem::row(
        "Edit Delayed Send",
        "pencil",
        MenuCommand::session_action(id, "delayedSend"),
    ));
    children.push(MenuItem::row(
        "Disable Delayed Send",
        "x",
        MenuCommand::command(message::cancel_delayed_send(id)),
    ));
    vec![layout_submenu("postponeBy", children)]
}

/// Switch Account, for a Claude or Codex session (or a cloud one before it starts on claude.ai).
fn switch_account_rows(
    id: &str,
    row: &SessionRow,
    group: &MenuGroup<'_>,
    caps: &SessionCapabilities,
    in_box: bool,
) -> Vec<MenuItem> {
    let account_provider = transcript_agent(
        row.menu_facts.agent_name.as_deref(),
        row.agent_icon.as_deref(),
    );
    // A Claude cloud session moves to another account until it starts on claude.ai
    // (CDXC:ClaudeCloud in server/src/claude_cloud/account_pick.rs).
    let cloud_account_open = row.claude_cloud.as_ref().is_some_and(|cloud| {
        !group.is_remote && matches!(cloud.state.as_str(), "draft" | "failed")
    });
    if caps.is_browser_session
        || group.is_stale
        || !((!in_box && matches!(account_provider, Some("claude") | Some("codex")))
            || cloud_account_open)
    {
        return Vec::new();
    }
    let entry = layout::entry("switchAccount").expect("switchAccount entry");
    vec![MenuItem {
        label: Some(entry.label.to_string()),
        icon: Some(entry.icon.to_string()),
        keep_open: true,
        command: Some(MenuCommand::session_accounts_load(id)),
        ..MenuItem::default()
    }]
}

/// Make Coordinator, on a session of an agent that can coordinate.
///
/// CDXC:Coordinators 2026-10-04 SEE-ALSO: promote_session_to_coordinator in server/src/coordinators/promote.rs (the user's decision that Make Coordinator lives here and never disturbs the running session); gxserver refuses the same sessions this row is hidden on, by the same agent list (`COORDINATOR_AGENT_FAMILIES` in gx-protocol). The phone's menus offer it too.
/// CDXC:Coordinators 2026-11-07 FIX: a reporter on Discord asked "why orchestrator cannot be promoted to a session in a remote machine from the desktop app?" (they had found it only in the CLI), and the user answered "I'll add that command too looks like it's not showing in ui". A connected computer's row offers Make Coordinator too: the desktop sends `/api/promoteCoordinator` to the gxserver that runs the session (gx_store/create/coordinator.rs), which installs the orchestrator skill on that computer and queues the playbook there.
fn make_coordinator_rows(
    id: &str,
    row: &SessionRow,
    group: &MenuGroup<'_>,
    caps: &SessionCapabilities,
    in_box: bool,
) -> Vec<MenuItem> {
    let offered = !caps.is_browser_session
        && !group.is_stale
        && !in_box
        && !row.is_draft
        && !row.is_coordinator
        && row.coordinator_parent.is_none()
        && row
            .agent_icon
            .as_deref()
            .or(row.menu_facts.agent_name.as_deref())
            .map(|agent| crate::default_agent_id_for_icon(Some(agent)).unwrap_or(agent))
            .is_some_and(ghostex_gx_protocol::coordinator_agent_family_supported);
    if !offered {
        return Vec::new();
    }
    vec![layout_row(
        "makeCoordinator",
        false,
        MenuCommand::session_action(id, "makeCoordinator"),
    )]
}

/// Sleep Below and Close Below, for the rows drawn under this one in its group.
fn below_rows(id_rows: &[&SessionRow], which: &str) -> Vec<MenuItem> {
    if id_rows.is_empty() {
        return Vec::new();
    }
    if which == "sleepBelow" {
        let sleepable: Vec<String> = id_rows
            .iter()
            // CDXC:SessionSleep 2026-10-01 DECISION: User: "please don't sleep terminal sessions that have shell running or a question status", and Sleep Below is a bulk sweep too, so it skips rows with the grey shell-running dot or an unanswered question.
            .filter(|row| {
                can_sleep(row)
                    && !row.has_background_work
                    && !row.has_question()
                    && !row.awaiting_approval
            })
            .map(|row| row.sidebar_session_id.clone())
            .collect();
        if sleepable.is_empty() {
            return Vec::new();
        }
        return vec![layout_row(
            "sleepBelow",
            false,
            MenuCommand::command(message::sleep_sessions_below(&sleepable)),
        )];
    }
    let below_ids: Vec<String> = id_rows
        .iter()
        .map(|row| row.sidebar_session_id.clone())
        .collect();
    vec![layout_row(
        "closeBelow",
        false,
        MenuCommand::command(message::close_sessions(&below_ids, "closeBelow")),
    )]
}

/// What fills each place of the one session menu (layout.rs) for one row.
struct Places<'a, 'b> {
    input: &'a SessionMenuInput<'b>,
    rows: &'a ActionRows,
    caps: &'a SessionCapabilities,
    /// The row's hover strip, in the sidebar: a row the strip already shows stays out of the menu
    /// unless the strip's actions are mirrored into it (Show each thing once). `None` where the
    /// menu has no strip beside it (the chat box ⋯), which shows every row.
    strip: Option<(Vec<HoverAction>, Vec<HoverAction>)>,
}

impl Places<'_, '_> {
    fn new<'a, 'b>(
        input: &'a SessionMenuInput<'b>,
        rows: &'a ActionRows,
        caps: &'a SessionCapabilities,
        strip: Option<&HoverStrip>,
    ) -> Places<'a, 'b> {
        let strip = strip.map(|strip| {
            let enabled = strip.enabled();
            // An action the hover strip offers is repeated in the menu too, with the two that
            // would be duplicated by their own rows left out.
            let mirror: Vec<HoverAction> = if input
                .settings
                .show_session_card_hover_buttons_in_context_menu
            {
                if caps.is_browser_session {
                    vec![HoverAction::Sleep]
                } else {
                    enabled
                        .iter()
                        .copied()
                        .filter(|action| {
                            *action != HoverAction::Close && *action != HoverAction::CloseAfterDone
                        })
                        .collect()
                }
            } else {
                Vec::new()
            };
            (enabled, mirror)
        });
        Places {
            input,
            rows,
            caps,
            strip,
        }
    }

    fn in_strip(&self, action: HoverAction) -> bool {
        self.strip
            .as_ref()
            .is_some_and(|(enabled, _)| enabled.contains(&action))
    }

    fn shown(&self, action: HoverAction) -> bool {
        match &self.strip {
            None => true,
            Some((enabled, mirror)) => mirror.contains(&action) || !enabled.contains(&action),
        }
    }

    fn action(&self, action: HoverAction) -> Vec<MenuItem> {
        self.rows
            .get(action)
            .filter(|_| self.shown(action))
            .cloned()
            .into_iter()
            .collect()
    }

    fn fill(&self, place: &str) -> Vec<MenuItem> {
        let input = self.input;
        let (row, group, caps) = (input.row, input.group, self.caps);
        let id = row.sidebar_session_id.as_str();
        // A box session has no transcript on this computer to fork, export or move to another
        // account. A Claude cloud session's conversation lives on claude.ai, so none of these
        // apply to it either.
        let in_box = row.agentbox.is_some() || row.claude_cloud.is_some();
        match place {
            "rename" => self.action(HoverAction::Rename),
            "pin" => self.action(HoverAction::Pin),
            "snooze" => self.action(HoverAction::Snooze),
            "park" => self.action(HoverAction::Park),
            "box" => box_rows(id, row, group),
            "cleanUpWorktree" | "keepWorktree" => worktree_cleanup_rows(id, row, place),
            "claudeAi" => claude_cloud_rows(id, row, group),
            "coordinator" => coordinator_rows(row, group.is_stale, input.has_inactive_threads),
            "markThreadDone" => thread_rows(row, group.is_stale),
            "copy" => vec![super::copy::copy_submenu(id, row, group)],
            "tagAs" if !input.settings.session_tags_enabled => favorite_rows(id, row, *caps),
            "tagAs" => self.action(HoverAction::Tag),
            "sleep" => self.action(HoverAction::Sleep),
            "fork" if caps.can_fork_session && !in_box => vec![layout_row(
                "fork",
                false,
                MenuCommand::command(message::fork_session(id)),
            )],
            "fullReload" if caps.can_full_reload_session => vec![layout_row(
                "fullReload",
                false,
                MenuCommand::command(message::full_reload_session(id)),
            )],
            "switchAccount" => switch_account_rows(id, row, group, caps, in_box),
            "delayedSend" if caps.can_delayed_send => vec![layout_row(
                "delayedSend",
                false,
                MenuCommand::session_action(id, "delayedSend"),
            )],
            "note" => self.action(HoverAction::Note),
            "closeAfterDone"
                if !self.in_strip(HoverAction::CloseAfterDone)
                    || input
                        .settings
                        .show_session_card_hover_buttons_in_context_menu =>
            {
                self.rows.close_after_done.clone().into_iter().collect()
            }
            "export" if caps.can_export_transcript && !in_box => vec![layout_row(
                "export",
                false,
                MenuCommand::command(message::export_session_transcript(id)),
            )],
            "linkTo" => super::link_menu::link_submenu(id, row, group)
                .into_iter()
                .collect(),
            "makeCoordinator" => make_coordinator_rows(id, row, group, caps, in_box),
            "postponeBy" => postpone_rows(id, row, caps),
            "moveToGroup" if group.can_create_session_group => {
                let mut rows: Vec<MenuItem> =
                    move_to_project_item(id, row, input.group_moves, group.is_stale)
                        .into_iter()
                        .collect();
                rows.push(move_to_group_item(id, input.group_moves, group.is_stale));
                rows
            }
            "sleepBelow" | "closeBelow" => below_rows(input.below, place),
            "close" if !self.in_strip(HoverAction::Close) => {
                self.rows.close.clone().into_iter().collect()
            }
            // Popout is the desktop's own tile (gx_store/sidebar_menus.rs), Branches lives in the
            // chat's core, and the chat box's and the phone's own rows are theirs to fill.
            _ => Vec::new(),
        }
    }
}

/// Clean Up Worktree or Keep Worktree while gxserver offers to clean up the row's merged PR
/// (CDXC:WorkMode 2026-11-25 in layout.rs); the same answer the row's merged badge sends.
fn worktree_cleanup_rows(id: &str, row: &SessionRow, place: &str) -> Vec<MenuItem> {
    if !row.work.as_ref().is_some_and(|work| work.offer_cleanup) {
        return Vec::new();
    }
    let answer = if place == "cleanUpWorktree" {
        "cleanUp"
    } else {
        "keep"
    };
    vec![layout_row(
        place,
        false,
        MenuCommand::answer_work_cleanup(id, answer),
    )]
}

/// The row's whole menu: the one session menu (layout.rs) as the sidebar fills it.
fn full_menu(
    input: &SessionMenuInput<'_>,
    rows: &ActionRows,
    caps: &SessionCapabilities,
    strip: &HoverStrip,
) -> Vec<MenuItem> {
    let places = Places::new(input, rows, caps, Some(strip));
    layout::assemble_session_menu(SessionMenuSpot::Sidebar, |place| places.fill(place))
}

/// The rows the sidebar would put in each place of the one session menu, for a spot with no hover
/// strip beside it (the chat box ⋯) to fill its shared places with the sidebar's own rows, so
/// both run the same commands. Places with nothing to show are left out.
pub fn session_menu_places(input: &SessionMenuInput<'_>) -> Vec<(&'static str, Vec<MenuItem>)> {
    let (rows, caps) = action_rows(input, true);
    let places = Places::new(input, &rows, &caps, None);
    layout::ENTRIES
        .iter()
        .map(|entry| (entry.id, places.fill(entry.id)))
        .filter(|(_, items)| !items.is_empty())
        .collect()
}
