//! A session row's context menu and its hover buttons.
//!
//! Ported row for row from the sidebar page's session menu (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/session-menu.ts`; see git history).
//!
//! CDXC:ContextMenus 2026-09-19 DECISION:
//! The user asked for a different pin icon in the sidebar context menu. Pin uses the upright
//! pushpin that pairs with Unpin's crossed-out pushpin (the React menu's IconPinned); the card
//! hover button keeps the diagonal pin. Every sidebar menu label is Title Case ("Close Inactive",
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
use super::text::{js_trim, transcript_agent};

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
    let mut park = MenuItem::row(
        if parked { "Unpark" } else { "Park" },
        "archive",
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
    let rows = ActionRows {
        close: Some(
            MenuItem::row(
                "Close",
                "x",
                MenuCommand::command(message::close_session(id)),
            )
            .with_danger(),
        ),
        rename: caps.can_rename_session.then(|| {
            MenuItem::row(
                "Rename",
                "pencil",
                MenuCommand::session_action(id, "rename"),
            )
        }),
        note: (settings.session_notes_enabled && caps.can_open_session_note)
            .then(|| MenuItem::row("Note", "note", MenuCommand::session_action(id, "note"))),
        sleep: caps.can_sleep_session.then(|| {
            MenuItem::row(
                if sleeping { "Wake" } else { "Sleep" },
                if sleeping { "player-play" } else { "moon" },
                MenuCommand::command(message::set_session_sleeping(id, !sleeping)),
            )
        }),
        pin: caps.can_pin_session.then(|| {
            MenuItem::row(
                if row.is_pinned { "Unpin" } else { "Pin" },
                if row.is_pinned { "pinned-off" } else { "pin" },
                MenuCommand::command(message::set_session_pinned(id, !row.is_pinned)),
            )
        }),
        park: (!caps.is_browser_session && settings.enable_session_parking).then_some(park),
        tag: (caps.can_tag_session && !tags.is_empty())
            .then(|| MenuItem::submenu("Tag As", "tag", tags.clone())),
        snooze: (!caps.is_browser_session).then(|| {
            if snoozed {
                MenuItem::row(
                    "Unsnooze",
                    "alarm",
                    MenuCommand::command(message::unsnooze_session(id)),
                )
            } else {
                MenuItem::submenu("Snooze", "alarm", snooze_presets)
            }
        }),
        close_after_done: caps.can_close_after_done.then(|| {
            MenuItem::row(
                "Close After Done",
                "clock",
                MenuCommand::command(message::toggle_close_after_done(id)),
            )
        }),
    };
    (rows, caps)
}

/// The Tag As options, or the one placeholder that stands in for them on a publish.
fn tag_items(input: &SessionMenuInput<'_>, include_menu: bool) -> Vec<MenuItem> {
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

/// Move to New Sessions Group, or the Move to Group page once the project has a group to move to or
/// the row is in one.
///
/// CDXC:ContextMenus 2026-11-05 DECISION:
/// User (Q1311): a group of a project's sessions is a "sessions group": its rows read Move to New
/// Sessions Group and New Sessions Group, and "New Group" names a group of projects (Add to Group).
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
        return new_group("Move to New Sessions Group");
    };
    let mut children = vec![new_group("New Sessions Group")];
    for group in &moves.groups {
        children.push(MenuItem {
            label: Some(group.title.clone()),
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

/// An orchestrator's Goal & Instructions and Memory editors, and a thread's Mark Thread Done or
/// Reopen Thread, after a separator. An orchestrator's block goes on with Sleep Inactive Threads.
///
/// CDXC:Coordinators 2026-10-13 DECISION:
/// User: "Mark done / Reopen on threads, Goal & instructions and Memory editing on coordinators on both the phone and the desktop" (2026-10-01), ported onto the Orchestrator menus ("port all of it, rebased onto main"). Goal & Instructions and Memory open editors that save through gxserver; Mark Thread Done does what `ghostex orchestrator resolve` does (marks the thread done and closes its session) and is disabled while the thread works or waits on an answer, which resolve refuses; Reopen Thread shows instead on a done thread still in the list. A machine that is not connected gets none of them. The phone's menu (`apps/mobile/app/src/screens/sessions-screen/sidebar-menus.ts`) must match.
fn coordinator_rows(row: &SessionRow, stale: bool) -> Vec<MenuItem> {
    if stale || row.is_draft {
        return Vec::new();
    }
    let id = row.sidebar_session_id.as_str();
    let mut rows = vec![MenuItem::separator()];
    if row.is_coordinator {
        rows.push(MenuItem::row(
            "Goal & Instructions…",
            "checklist",
            MenuCommand::coordinator_session(id, "goal"),
        ));
        rows.push(MenuItem::row(
            "Memory…",
            "brain",
            MenuCommand::coordinator_session(id, "memory"),
        ));
    } else if row.coordinator_parent.is_some() {
        let state = row.thread_state.as_deref();
        if state == Some("done") {
            rows.push(MenuItem::row(
                "Reopen Thread",
                "rotate-clockwise",
                MenuCommand::coordinator_session(id, "reopen"),
            ));
        } else {
            rows.push(
                MenuItem::row(
                    "Mark Thread Done",
                    "circle-check",
                    MenuCommand::coordinator_session(id, "markDone"),
                )
                .with_disabled(matches!(state, Some("working" | "waiting"))),
            );
        }
    }
    if rows.len() == 1 {
        return Vec::new();
    }
    rows
}

fn full_menu(
    input: &SessionMenuInput<'_>,
    rows: &ActionRows,
    caps: &SessionCapabilities,
    strip: &HoverStrip,
) -> Vec<MenuItem> {
    let row = input.row;
    let settings = input.settings;
    let group = input.group;
    let id = row.sidebar_session_id.as_str();
    let enabled = strip.enabled();
    // An action the hover strip offers is repeated in the menu too, with the two that would be
    // duplicated by their own rows left out.
    let mirror: Vec<HoverAction> = if settings.show_session_card_hover_buttons_in_context_menu {
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
    /*
    CDXC:ContextMenus 2026-10-01 DECISION:
    The user wants the session menu in ChatGPT's conversation-menu order (Rename, Pin, Mark as unread, Archive, then Section after a line), mirrored hover buttons included: "why is rename below those other buttons? match the order of ChatGPT". The rows run Rename, Pin, Snooze, Park, Sleep, Note, then Tag As after a separator, whether a row is there as a mirrored hover button or on its own; this supersedes the mirrored buttons leading the menu in the card's right-to-left order. Note shows here only as a mirrored hover button and otherwise sits first in Advanced. Close keeps its hover-strip rule, and Fork stays in Advanced (Copy Details moved into the Copy submenu on 2026-10-09, sidebar_menu/copy.rs). The phone's menu (`apps/mobile/app/src/screens/sessions-screen/sidebar-menus.ts`) must match.
    */
    const PRIMARY_ORDER: [HoverAction; 7] = [
        HoverAction::Rename,
        HoverAction::Pin,
        HoverAction::Snooze,
        HoverAction::Park,
        HoverAction::Sleep,
        HoverAction::Note,
        HoverAction::Tag,
    ];
    let mut menu: Vec<MenuItem> = Vec::new();
    for action in PRIMARY_ORDER {
        let shown = if mirror.contains(&action) {
            true
        } else {
            action != HoverAction::Note && !enabled.contains(&action)
        };
        let Some(item) = rows.get(action).filter(|_| shown) else {
            continue;
        };
        if action == HoverAction::Tag && !menu.is_empty() {
            menu.push(MenuItem::separator());
        }
        menu.push(if action == HoverAction::Pin && !row.is_pinned {
            MenuItem {
                icon: Some("pinned".to_string()),
                ..item.clone()
            }
        } else {
            item.clone()
        });
    }

    let mut coordinator_block = coordinator_rows(row, group.is_stale);
    let has_coordinator_block = !coordinator_block.is_empty();
    // Its separator only divides it from rows above, so a menu never opens on a line.
    if has_coordinator_block && menu.is_empty() {
        coordinator_block.remove(0);
    }
    menu.extend(coordinator_block);

    // CDXC:Coordinators 2026-10-13 SEE-ALSO: the user's decision on `coordinator_thread_keys` in sidebar_actions/bulk.rs ("right click on orchestrator and sleep", "Should just be sleep inactive not park"). Disabled, like the project menu's Sleep Inactive, when no thread can be slept.
    if row.is_coordinator && !row.is_draft {
        if !has_coordinator_block && !menu.is_empty() {
            menu.push(MenuItem::separator());
        }
        menu.push(
            MenuItem::row(
                "Sleep Inactive Threads",
                "moon",
                MenuCommand::command(message::sleep_inactive_coordinator_threads(id)),
            )
            .with_disabled(!input.has_inactive_threads),
        );
    }

    // A box session's own section. Only this computer's rows: `/api/agentbox` answers for the
    // gxserver the app talks to, and a remote row's box lives on that machine.
    if row.agentbox.is_some() && !group.is_remote && !group.is_stale {
        if !menu.is_empty() {
            menu.push(MenuItem::separator());
        }
        menu.push(MenuItem::row(
            "Open Box Web App",
            "world",
            MenuCommand::agentbox_session(id, "openWeb"),
        ));
        menu.push(MenuItem::row(
            "Open Box Screen",
            "device-desktop",
            MenuCommand::agentbox_session(id, "openScreen"),
        ));
        menu.push(MenuItem::row(
            "Stop Box",
            "player-stop",
            MenuCommand::agentbox_session(id, "stop"),
        ));
        menu.push(
            MenuItem::row(
                "Destroy Box…",
                "trash",
                MenuCommand::agentbox_session(id, "destroy"),
            )
            .with_danger(),
        );
    }

    // A Claude cloud session's own section. Only this computer's rows: the session's link and the
    // worktree it comes back to belong to the gxserver that started it.
    // CDXC:ClaudeCloud 2026-10-23 DECISION: User: Bring to this computer goes "into a new worktree; the new local row takes over and the cloud row parks", and the row's menu also opens the session on claude.ai.
    if let Some(cloud) = row
        .claude_cloud
        .as_ref()
        .filter(|_| !group.is_remote && !group.is_stale)
    {
        if !menu.is_empty() {
            menu.push(MenuItem::separator());
        }
        let has_page = cloud.url.is_some();
        menu.push(
            MenuItem::row(
                "Open on claude.ai",
                "external-link",
                MenuCommand::claude_cloud_session(id, "open"),
            )
            .with_disabled(!has_page),
        );
        menu.push(
            MenuItem::row(
                "Bring to This Computer",
                "device-laptop",
                MenuCommand::claude_cloud_session(id, "bringBack"),
            )
            .with_disabled(!has_page || cloud.state == "moved"),
        );
        // Offered while the session's account is out of usage (CDXC:ClaudeCloud in
        // server/src/claude_cloud/accounts.rs).
        if cloud.limit_notice.is_some() && cloud.state == "running" {
            menu.push(MenuItem::row(
                "Start a Copy on Another Account…",
                "copy",
                MenuCommand::claude_cloud_session(id, "copy"),
            ));
        }
    }

    let mut advanced: Vec<MenuItem> = vec![MenuItem::heading("Session")];
    if !enabled.contains(&HoverAction::Note) {
        if let Some(item) = &rows.note {
            advanced.push(item.clone());
        }
    }
    if caps.can_delayed_send {
        advanced.push(MenuItem::row(
            "Delayed Send",
            "clock",
            MenuCommand::session_action(id, "delayedSend"),
        ));
    }
    if caps.can_close_after_done
        && (!enabled.contains(&HoverAction::CloseAfterDone)
            || settings.show_session_card_hover_buttons_in_context_menu)
    {
        if let Some(item) = &rows.close_after_done {
            advanced.push(item.clone());
        }
    }
    // A box session has no transcript on this computer to fork, export or move to another account.
    // A Claude cloud session's conversation lives on claude.ai, so none of these apply to it either.
    let in_box = row.agentbox.is_some() || row.claude_cloud.is_some();
    if caps.can_fork_session && !in_box {
        advanced.push(MenuItem::row(
            "Fork",
            "git-fork",
            MenuCommand::command(message::fork_session(id)),
        ));
    }
    if caps.can_full_reload_session {
        advanced.push(MenuItem::row(
            "Full Reload",
            "refresh",
            MenuCommand::command(message::full_reload_session(id)),
        ));
    }
    let account_provider = transcript_agent(
        row.menu_facts.agent_name.as_deref(),
        row.agent_icon.as_deref(),
    );
    // A Claude cloud session moves to another account until it starts on claude.ai
    // (CDXC:ClaudeCloud in server/src/claude_cloud/account_pick.rs).
    let cloud_account_open = row.claude_cloud.as_ref().is_some_and(|cloud| {
        !group.is_remote && matches!(cloud.state.as_str(), "draft" | "failed")
    });
    if !caps.is_browser_session
        && !group.is_stale
        && ((!in_box && matches!(account_provider, Some("claude") | Some("codex")))
            || cloud_account_open)
    {
        advanced.push(MenuItem {
            label: Some("Switch Account".to_string()),
            icon: Some("users-group".to_string()),
            keep_open: true,
            command: Some(MenuCommand::session_accounts_load(id)),
            ..MenuItem::default()
        });
    }
    // CDXC:Coordinators 2026-10-04 SEE-ALSO: promote_session_to_coordinator in server/src/coordinators/promote.rs (the user's decision that Make Coordinator lives here and never disturbs the running session); gxserver refuses the same sessions this row is hidden on, by the same agent list (`COORDINATOR_AGENT_FAMILIES` in gx-protocol). The phone's session menu (apps/mobile/app/src/screens/sessions-screen/sidebar-menus.ts) offers it too.
    // CDXC:Coordinators 2026-11-07 FIX: a reporter on Discord asked "why orchestrator cannot be promoted to a session in a remote machine from the desktop app?" (they had found it only in the CLI), and the user answered "I'll add that command too looks like it's not showing in ui". A connected computer's row offers Make Coordinator too: the desktop sends `/api/promoteCoordinator` to the gxserver that runs the session (gx_store/create/coordinator.rs), which installs the orchestrator skill on that computer and queues the playbook there.
    if !caps.is_browser_session
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
            .is_some_and(ghostex_gx_protocol::coordinator_agent_family_supported)
    {
        advanced.push(MenuItem::row(
            "Make Coordinator",
            "users-group",
            MenuCommand::session_action(id, "makeCoordinator"),
        ));
    }
    if caps.can_export_transcript && !in_box {
        advanced.push(MenuItem::row(
            "Handoff / Export",
            "file-export",
            MenuCommand::command(message::export_session_transcript(id)),
        ));
    }
    if row
        .menu_facts
        .first_user_message
        .as_deref()
        .is_some_and(|message| !js_trim(message).is_empty())
    {
        advanced.push(MenuItem::row(
            "View 1st Message",
            "message-circle",
            MenuCommand::session_action(id, "firstMessage"),
        ));
    }
    if caps.can_generate_session_title {
        advanced.push(MenuItem::row(
            "Generate Title",
            "sparkles",
            MenuCommand::command(message::generate_session_title(
                id,
                row.menu_facts.first_user_message.as_deref().unwrap_or(""),
            )),
        ));
    }
    if group.can_create_session_group {
        advanced.push(move_to_group_item(id, input.group_moves, group.is_stale));
    }
    if group.can_focus_mode {
        advanced.push(MenuItem::row(
            "Focus",
            "focus-2",
            MenuCommand::command(message::focus_session_mode(id)),
        ));
    }
    if !input.below.is_empty() {
        advanced.push(MenuItem::separator());
        advanced.push(MenuItem::heading("Below"));
        let sleepable: Vec<String> = input
            .below
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
        if !sleepable.is_empty() {
            advanced.push(MenuItem::row(
                "Sleep Below",
                "moon",
                MenuCommand::command(message::sleep_sessions_below(&sleepable)),
            ));
        }
        let below_ids: Vec<String> = input
            .below
            .iter()
            .map(|row| row.sidebar_session_id.clone())
            .collect();
        advanced.push(
            MenuItem::row(
                "Close Below",
                "x",
                MenuCommand::command(message::close_sessions(&below_ids, "closeBelow")),
            )
            .with_danger(),
        );
    }

    if caps.can_delayed_send
        && row
            .delayed_send
            .as_ref()
            .is_some_and(|delayed| delayed.deadline_at.is_some())
    {
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
        menu.push(MenuItem::submenu("Postpone By", "clock", children));
    }
    if !menu.is_empty() {
        menu.push(MenuItem::separator());
    }
    if let Some(link) = super::link_menu::link_submenu(id, row, group) {
        menu.push(link);
    }
    menu.push(super::copy::copy_submenu(id, row, group));
    if advanced.len() > 1 {
        menu.push(MenuItem::submenu("Advanced", "dots", advanced));
    }
    if !enabled.contains(&HoverAction::Close) {
        menu.push(MenuItem::separator());
        if let Some(close) = &rows.close {
            menu.push(close.clone());
        }
    }
    menu
}
