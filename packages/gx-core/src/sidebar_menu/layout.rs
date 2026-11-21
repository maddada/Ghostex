//! The one session menu: which rows a session's menu shows, in what order, under what name and
//! icon, wherever it opens.
//!
//! CDXC:ContextMenus 2026-11-20 DECISION:
//! User: "I'd like the session context menus to be SMALL in the different spots I see them", with
//! consistency "between the chat box and the sidebar", then picked "Apply as shown" (Q4292) on the
//! drawn proposal. Every session menu (the sidebar row, the chat box ⋯, the phone's long-press,
//! chat ⋯ and terminal ⋯) is this one list: a quick row of tiles (Rename first, then the spot's
//! own), the spot's own rows, the shared rows (Coordinator, Mark Thread Done, Copy, Tag As), one
//! More submenu (the shared rows, then the spot's own rare ones) and Close. One name per action, in
//! Title Case; Advanced, Agent and Project became More. Focus, View 1st Message and Generate Title
//! left the menus for the command palette. This supersedes the ChatGPT order (2026-10-01), the
//! quick row with Sleep under Advanced (2026-11-15, Q3420/Q3421) and the chat box's layout A
//! (2026-11-12, native_chat/actions.rs). The phone reads this list through its chat core
//! (`gx_session_menu_layout` in packages/gx-chat-wasm), so its menus cannot drift from it.
//!
//! CDXC:ContextMenus 2026-11-20 DECISION:
//! User: the project right-click menu is "way too long" (17 rows on main), then picked layout A
//! (Q4313) on the drawn proposals, with "New Browser Tab" named "New Tab" and "Rename" named
//! "Rename Project". The project menu follows the session menu's rules from this same list: a tile
//! row (Rename Project, Add Worktree, New Terminal, New Tab), Copy ›, Organise › (Project Group,
//! Spaces, Move to Workspace), Hide, one More › with everything else (Advanced › Autopilot became
//! More › Autopilot), then Close Inactive and Close Project. Every action stays; a worktree's own
//! short menu is unchanged. `project_menu_layout` is it, for the sidebar (desktop and web) and the
//! phone's project menu.
//!
//! CDXC:ContextMenus 2026-11-20 SEE-ALSO: session.rs (the sidebar fills this list),
//! apps/desktop/src/app/native_chat/actions.rs (the chat box ⋯), apps/mobile/app/src/session-menu/
//! (the phone's three menus), packages/gx-chat-wasm/src/lib.rs (the phone's door to it), project.rs
//! (the sidebar's project menu fills `project_menu_layout`), apps/mobile/app/src/screens/
//! sessions-screen/sidebar-menus.ts `buildProjectMenu` (the phone's).

use serde_json::{json, Map, Value};

use super::item::MenuItem;

/// Where a session's menu opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionMenuSpot {
    /// The sidebar row's right-click (desktop and web).
    Sidebar,
    /// The chat box's ⋯ (desktop and web).
    ChatBox,
    /// The phone's long-press on a session row.
    PhoneRow,
    /// The phone chat's ⋯.
    PhoneChat,
    /// The phone terminal view's ⋯.
    PhoneTerminal,
}

impl SessionMenuSpot {
    pub const ALL: [SessionMenuSpot; 5] = [
        SessionMenuSpot::Sidebar,
        SessionMenuSpot::ChatBox,
        SessionMenuSpot::PhoneRow,
        SessionMenuSpot::PhoneChat,
        SessionMenuSpot::PhoneTerminal,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SessionMenuSpot::Sidebar => "sidebar",
            SessionMenuSpot::ChatBox => "chatBox",
            SessionMenuSpot::PhoneRow => "phoneRow",
            SessionMenuSpot::PhoneChat => "phoneChat",
            SessionMenuSpot::PhoneTerminal => "phoneTerminal",
        }
    }
}

/// Where a project's menu opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectMenuSpot {
    /// The project header's right-click and ⋯ (desktop and web).
    Sidebar,
    /// The phone's project header menu.
    Phone,
}

impl ProjectMenuSpot {
    pub const ALL: [ProjectMenuSpot; 2] = [ProjectMenuSpot::Sidebar, ProjectMenuSpot::Phone];

    pub fn as_str(self) -> &'static str {
        match self {
            ProjectMenuSpot::Sidebar => "project",
            ProjectMenuSpot::Phone => "phoneProject",
        }
    }
}

/// One row of the list: its one name and icon (and the reverse a toggle reads: Unpin, Wake…).
#[derive(Clone, Copy, Debug)]
pub struct SessionMenuEntry {
    pub id: &'static str,
    /// Empty for a host slot: rows the spot draws itself (the chat box's folded toolbar controls,
    /// its Context window and options), whose words depend on the moment.
    pub label: &'static str,
    pub alt_label: Option<&'static str>,
    pub icon: &'static str,
    pub alt_icon: Option<&'static str>,
    /// The hotkey catalog action whose key the row shows.
    pub hotkey: Option<&'static str>,
    pub danger: bool,
}

const fn row(id: &'static str, label: &'static str, icon: &'static str) -> SessionMenuEntry {
    SessionMenuEntry {
        id,
        label,
        alt_label: None,
        icon,
        alt_icon: None,
        hotkey: None,
        danger: false,
    }
}

const fn toggle(
    id: &'static str,
    label: &'static str,
    icon: &'static str,
    alt_label: &'static str,
    alt_icon: &'static str,
) -> SessionMenuEntry {
    SessionMenuEntry {
        alt_label: Some(alt_label),
        alt_icon: Some(alt_icon),
        ..row(id, label, icon)
    }
}

const fn keyed(mut entry: SessionMenuEntry, hotkey: &'static str) -> SessionMenuEntry {
    entry.hotkey = Some(hotkey);
    entry
}

const fn danger(mut entry: SessionMenuEntry) -> SessionMenuEntry {
    entry.danger = true;
    entry
}

const fn host(id: &'static str) -> SessionMenuEntry {
    row(id, "", "")
}

/// Every row any session menu can show, under its one name.
pub const ENTRIES: &[SessionMenuEntry] = &[
    // Shared by every spot.
    keyed(row("rename", "Rename", "pencil"), "renameActiveSession"),
    row("coordinator", "Coordinator", "coordinator-crown-points"),
    toggle(
        "markThreadDone",
        "Mark Thread Done",
        "circle-check",
        "Reopen Thread",
        "rotate-clockwise",
    ),
    row("copy", "Copy", "copy"),
    row("tagAs", "Tag As", "tag"),
    keyed(
        toggle("sleep", "Sleep", "moon", "Wake", "player-play"),
        "sleepFocusedSession",
    ),
    keyed(row("fork", "Fork", "git-fork"), "forkSession"),
    keyed(row("fullReload", "Full Reload", "refresh"), "reloadSession"),
    row("switchAccount", "Switch Account", "switch-horizontal"),
    keyed(
        row("delayedSend", "Delayed Send", "clock-check"),
        "delayedSend",
    ),
    keyed(row("note", "Note", "note"), "sessionNote"),
    row("closeAfterDone", "Close After Done", "clock"),
    row("export", "Handoff / Export", "file-export"),
    row("linkTo", "Link to", "link"),
    row("branches", "Branches", "git-branch"),
    row("makeCoordinator", "Make Coordinator", "users-group"),
    row("postponeBy", "Postpone By", "clock"),
    keyed(danger(row("close", "Close", "x")), "closeFocusedSession"),
    // A spot's own.
    toggle("pin", "Pin", "pin-outline", "Unpin", "pin-filled"),
    keyed(row("popout", "Popout", "external-link"), "popOutPane"),
    keyed(
        row("voiceCall", "Voice Call", "headset"),
        "voiceCallAboutSession",
    ),
    keyed(
        row("filesLinks", "Files & Links", "link"),
        "toggleChatFilesLinks",
    ),
    row("sideChat", "Side Chat", "message-circle"),
    row("view", "View", "eye"),
    row("stopReading", "Stop Reading", "player-stop"),
    row("searchConversation", "Search Conversation", "search"),
    keyed(row("promptEditor", "Prompt Editor", "edit"), "promptEditor"),
    toggle("snooze", "Snooze", "alarm", "Unsnooze", "alarm"),
    toggle("park", "Park", "archive", "Unpark", "archive"),
    row("box", "Box", "world"),
    row("claudeAi", "claude.ai", "external-link"),
    row(
        "moveToGroup",
        "Move to Group",
        "layout-sidebar-right-expand",
    ),
    row("sleepBelow", "Sleep Below", "moon"),
    danger(row("closeBelow", "Close Below", "x")),
    keyed(
        row("stashPrompt", "Stash Prompt", "stack-push"),
        "stashPrompt",
    ),
    row("skills", "Skills", "sparkles"),
    row("links", "Links", "link"),
    row("changes", "Changes", "file-diff"),
    row("beads", "Beads", "checklist"),
    row("projectActions", "Actions…", "player-play"),
    row("catchUp", "Catch Me Up", "history"),
    row("files", "Files", "folder"),
    row("newTerminal", "New Terminal", "terminal-2"),
    row("openTerminal", "Open Terminal", "terminal-2"),
    row("attachFile", "Send & Attach File", "paperclip"),
    row("terminalSettings", "Settings", "settings"),
    danger(row("disconnect", "Disconnect", "plug-x")),
    danger(row("killSession", "Kill Session", "x")),
    // The project menu's (`project_menu_layout`). New Terminal, Copy, More and Full Reload are the
    // session menu's rows above, under the same names.
    row("renameProject", "Rename Project", "pencil"),
    row("addWorktree", "Add Worktree", "git-branch"),
    keyed(row("newTab", "New Tab", "world"), "openBrowserPane"),
    row("copyPath", "Copy Path", "copy"),
    row("copyRemoteUrl", "Copy Remote URL", "link"),
    row("organise", "Organise", "folders"),
    row("projectGroup", "Project Group", "folder"),
    row("spaces", "Spaces", "stack"),
    row("moveToWorkspace", "Move to Workspace", "switch-horizontal"),
    toggle("hide", "Hide", "eye-off", "Unhide", "eye"),
    row("history", "History", "history"),
    row("openFolder", "Open Folder", "folder-open"),
    row("openIn", "Open in", "external-link"),
    row("cloneOn", "Clone on…", "copy-plus"),
    toggle(
        "sleepInactive",
        "Sleep Inactive",
        "moon",
        "Wake",
        "player-play",
    ),
    row("autopilot", "Autopilot", "plane"),
    danger(row("closeInactive", "Close Inactive", "x")),
    danger(row("closeProject", "Close Project", "x")),
    danger(row("deleteWorktree", "Delete Worktree", "trash")),
    row("moveProjectUp", "Move Project Up", "arrow-up"),
    row("moveProjectDown", "Move Project Down", "arrow-down"),
    row("refreshSessions", "Refresh Sessions", "refresh"),
    row("projectDetails", "Details", "info-circle"),
    // Rows a spot draws itself in a fixed place.
    host("chatToolbar"),
    host("chatOptions"),
    host("terminalLeading"),
    // The project's Create Linear Ticket… or Create GitHub Issue…, and its pinned Actions.
    host("createTicket"),
    host("pinnedActions"),
];

/// The More submenu's own row.
pub const MORE: SessionMenuEntry = row("more", "More", "dots");

pub fn entry(id: &str) -> Option<&'static SessionMenuEntry> {
    ENTRIES.iter().find(|entry| entry.id == id)
}

/// One place in a menu's layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutNode {
    /// The quick row of tiles.
    Tiles(Vec<&'static str>),
    Row(&'static str),
    Separator,
    /// The More submenu.
    More(Vec<LayoutNode>),
    /// A submenu named by its entry (the project menu's Copy and Organise). A submenu left with
    /// one row shows that row in its place.
    Submenu(&'static str, Vec<LayoutNode>),
}

const SEP: &str = "-";
const SHARED_TILES: &[&str] = &["rename"];
const SHARED_ROWS: &[&str] = &["coordinator", "markThreadDone", "copy", "tagAs"];
const SHARED_MORE: &[&str] = &[
    "sleep",
    "fork",
    "fullReload",
    "switchAccount",
    "delayedSend",
    "note",
    SEP,
    "closeAfterDone",
    "export",
    "linkTo",
    "branches",
    "makeCoordinator",
    "postponeBy",
];
const SHARED_AFTER: &[&str] = &["close"];

/// A spot's own rows: tiles after Rename, rows above the shared ones, rows at the end of More.
struct SpotRows {
    tiles: &'static [&'static str],
    rows: &'static [&'static str],
    more: &'static [&'static str],
}

fn spot_rows(spot: SessionMenuSpot) -> SpotRows {
    match spot {
        SessionMenuSpot::Sidebar => SpotRows {
            tiles: &["pin", "popout"],
            rows: &["snooze", "park", "box", "claudeAi"],
            more: &["moveToGroup", "sleepBelow", "closeBelow"],
        },
        SessionMenuSpot::ChatBox => SpotRows {
            tiles: &["filesLinks", "sideChat"],
            rows: &["chatToolbar", "view", "stopReading"],
            more: &["stashPrompt", "skills", "chatOptions"],
        },
        SessionMenuSpot::PhoneRow => SpotRows {
            tiles: &["pin", "voiceCall"],
            rows: &["snooze", "park"],
            more: &[
                "openTerminal",
                "links",
                "changes",
                "beads",
                "projectActions",
                "sleepBelow",
                "closeBelow",
            ],
        },
        SessionMenuSpot::PhoneChat => SpotRows {
            tiles: &["filesLinks", "sideChat"],
            rows: &["chatToolbar", "view", "stopReading", "searchConversation"],
            more: &[
                "voiceCall",
                "promptEditor",
                "catchUp",
                "files",
                "changes",
                "newTerminal",
                "stashPrompt",
                "skills",
                "chatOptions",
                "killSession",
            ],
        },
        SessionMenuSpot::PhoneTerminal => SpotRows {
            tiles: &["searchConversation", "promptEditor"],
            rows: &["terminalLeading"],
            more: &[
                "files",
                "changes",
                "beads",
                "skills",
                SEP,
                "attachFile",
                "newTerminal",
                "terminalSettings",
                "disconnect",
                "killSession",
            ],
        },
    }
}

fn nodes(ids: &[&'static str]) -> Vec<LayoutNode> {
    ids.iter()
        .map(|id| {
            if *id == SEP {
                LayoutNode::Separator
            } else {
                LayoutNode::Row(id)
            }
        })
        .collect()
}

/// The menu a spot shows: its tiles, its own rows, a line, the shared rows, More, a line, Close.
pub fn session_menu_layout(spot: SessionMenuSpot) -> Vec<LayoutNode> {
    let own = spot_rows(spot);
    let mut tiles: Vec<&'static str> = SHARED_TILES.to_vec();
    tiles.extend_from_slice(own.tiles);
    let mut more = nodes(SHARED_MORE);
    more.push(LayoutNode::Separator);
    more.extend(nodes(own.more));
    let mut layout = vec![LayoutNode::Tiles(tiles)];
    layout.extend(nodes(own.rows));
    layout.push(LayoutNode::Separator);
    layout.extend(nodes(SHARED_ROWS));
    layout.push(LayoutNode::More(more));
    layout.push(LayoutNode::Separator);
    layout.extend(nodes(SHARED_AFTER));
    layout
}

/// Drops a line that would open, end or double up a list.
pub fn tidy_separators<T>(items: Vec<T>, is_separator: impl Fn(&T) -> bool) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    for item in items {
        if is_separator(&item) && out.last().is_none_or(&is_separator) {
            continue;
        }
        out.push(item);
    }
    while out.last().is_some_and(&is_separator) {
        out.pop();
    }
    out
}

/// The project menu a spot shows (CDXC:ContextMenus 2026-11-20, Q4313 A): its tiles, Copy ›,
/// Organise ›, Hide, More ›, then the two Close rows. The phone has no Rename Project, Add
/// Worktree, New Tab, Hide or Close Project of its own, and adds its own rows to More.
pub fn project_menu_layout(spot: ProjectMenuSpot) -> Vec<LayoutNode> {
    use LayoutNode::{More, Row, Separator, Submenu, Tiles};
    match spot {
        ProjectMenuSpot::Sidebar => vec![
            Tiles(vec![
                "renameProject",
                "addWorktree",
                "newTerminal",
                "newTab",
            ]),
            Submenu("copy", nodes(&["copyPath", "copyRemoteUrl"])),
            Submenu(
                "organise",
                nodes(&["projectGroup", "spaces", "moveToWorkspace"]),
            ),
            Row("hide"),
            More(nodes(&[
                "history",
                "openFolder",
                "openIn",
                "createTicket",
                "pinnedActions",
                "cloneOn",
                SEP,
                "sleepInactive",
                "fullReload",
                "autopilot",
            ])),
            Separator,
            Row("closeInactive"),
            Row("closeProject"),
            Row("deleteWorktree"),
        ],
        ProjectMenuSpot::Phone => vec![
            Tiles(vec!["newTerminal", "projectActions"]),
            Submenu("copy", nodes(&["copyPath"])),
            Submenu("organise", nodes(&["projectGroup"])),
            Row("files"),
            More(nodes(&[
                "projectDetails",
                "moveProjectUp",
                "moveProjectDown",
                "refreshSessions",
                SEP,
                "sleepInactive",
                "fullReload",
            ])),
            Separator,
            Row("closeInactive"),
        ],
    }
}

/// A menu built from `layout`: `fill` answers each row id with the rows shown for it right now
/// (none, one, or a block such as a coordinator's). Tiles become one quick row, More and a
/// Submenu one submenu each (a Submenu left with one row shows that row instead); any of them is
/// left out when empty, and stray lines are dropped.
pub fn assemble_menu(
    layout: &[LayoutNode],
    fill: &mut dyn FnMut(&str) -> Vec<MenuItem>,
) -> Vec<MenuItem> {
    fn rows_of(
        children: &[LayoutNode],
        fill: &mut dyn FnMut(&str) -> Vec<MenuItem>,
    ) -> Vec<MenuItem> {
        let rows: Vec<MenuItem> = children
            .iter()
            .flat_map(|child| match child {
                LayoutNode::Row(id) => fill(id),
                _ => vec![MenuItem::separator()],
            })
            .collect();
        tidy_separators(rows, |item| item.separator)
    }
    let mut items = Vec::new();
    for node in layout {
        match node {
            LayoutNode::Row(id) => items.extend(fill(id)),
            LayoutNode::Separator => items.push(MenuItem::separator()),
            LayoutNode::Tiles(ids) => {
                let tiles: Vec<MenuItem> = ids.iter().flat_map(|id| fill(id)).collect();
                if !tiles.is_empty() {
                    items.push(MenuItem::quick_row(tiles));
                }
            }
            LayoutNode::More(children) => {
                let rows = rows_of(children, fill);
                if !rows.is_empty() {
                    items.push(MenuItem::submenu(MORE.label, MORE.icon, rows));
                }
            }
            LayoutNode::Submenu(id, children) => {
                let mut rows = rows_of(children, fill);
                if rows.len() == 1 {
                    items.append(&mut rows);
                } else if !rows.is_empty() {
                    let head = entry(id).copied().unwrap_or(MORE);
                    items.push(MenuItem::submenu(head.label, head.icon, rows));
                }
            }
        }
    }
    tidy_separators(items, |item| item.separator)
}

/// A spot's menu built from its layout: `fill` answers each row id with the rows the spot shows
/// for it right now (none, one, or a block such as a coordinator's).
pub fn assemble_session_menu(
    spot: SessionMenuSpot,
    mut fill: impl FnMut(&str) -> Vec<MenuItem>,
) -> Vec<MenuItem> {
    assemble_menu(&session_menu_layout(spot), &mut fill)
}

/// A spot's project menu built from its layout, as [`assemble_session_menu`].
pub fn assemble_project_menu(
    spot: ProjectMenuSpot,
    mut fill: impl FnMut(&str) -> Vec<MenuItem>,
) -> Vec<MenuItem> {
    assemble_menu(&project_menu_layout(spot), &mut fill)
}

/// A row from the list: its one name and icon (the reverse when `alt`), its hotkey and colour.
pub(crate) fn entry_row(id: &str, alt: bool, command: super::commands::MenuCommand) -> MenuItem {
    let Some(entry) = entry(id) else {
        return MenuItem::row(id, "", command);
    };
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

fn node_json(node: &LayoutNode) -> Value {
    match node {
        LayoutNode::Tiles(ids) => json!({ "tiles": ids }),
        LayoutNode::Row(id) => json!({ "row": id }),
        LayoutNode::Separator => json!({ "separator": true }),
        LayoutNode::More(children) => {
            json!({ "more": children.iter().map(node_json).collect::<Vec<_>>() })
        }
        LayoutNode::Submenu(id, children) => json!({
            "submenu": id,
            "rows": children.iter().map(node_json).collect::<Vec<_>>(),
        }),
    }
}

fn entry_json(entry: &SessionMenuEntry) -> Value {
    let mut object = Map::new();
    object.insert("label".into(), json!(entry.label));
    object.insert("icon".into(), json!(entry.icon));
    if let Some(label) = entry.alt_label {
        object.insert("altLabel".into(), json!(label));
    }
    if let Some(icon) = entry.alt_icon {
        object.insert("altIcon".into(), json!(icon));
    }
    if let Some(hotkey) = entry.hotkey {
        object.insert("hotkey".into(), json!(hotkey));
    }
    if entry.danger {
        object.insert("danger".into(), json!(true));
    }
    if entry.label.is_empty() {
        object.insert("host".into(), json!(true));
    }
    Value::Object(object)
}

/// The whole list as JSON, for the phone: `{ entries: {id: {label, icon, altLabel?, altIcon?,
/// hotkey?, danger?, host?}}, more: {label, icon}, spots: {phoneRow: [node…], …, project: […],
/// phoneProject: […]} }`, where a node is `{tiles: [id…]}`, `{row: id}`, `{separator: true}`,
/// `{more: [node…]}` or `{submenu: id, rows: [node…]}`.
pub fn session_menu_layout_json() -> Value {
    let entries: Map<String, Value> = ENTRIES
        .iter()
        .map(|entry| (entry.id.to_string(), entry_json(entry)))
        .collect();
    let mut spots: Map<String, Value> = SessionMenuSpot::ALL
        .iter()
        .map(|spot| {
            (
                spot.as_str().to_string(),
                Value::Array(session_menu_layout(*spot).iter().map(node_json).collect()),
            )
        })
        .collect();
    for spot in ProjectMenuSpot::ALL {
        spots.insert(
            spot.as_str().to_string(),
            Value::Array(project_menu_layout(spot).iter().map(node_json).collect()),
        );
    }
    json!({
        "entries": entries,
        "more": { "label": MORE.label, "icon": MORE.icon },
        "spots": spots,
    })
}

#[cfg(test)]
mod project_layout_tests {
    use super::*;
    use crate::sidebar_menu::commands::MenuCommand;

    fn labels(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| {
                if item.separator {
                    "-".to_string()
                } else if let Some(tiles) = &item.quick {
                    format!("[{}]", labels(tiles).join(", "))
                } else if let Some(children) = &item.children {
                    format!(
                        "{} › ({})",
                        item.label.as_deref().unwrap_or(""),
                        labels(children).join(", ")
                    )
                } else {
                    item.label.clone().unwrap_or_default()
                }
            })
            .collect()
    }

    fn filled(spot: ProjectMenuSpot, shown: &[&str]) -> Vec<String> {
        labels(&assemble_project_menu(spot, |id| {
            if shown.contains(&id) {
                vec![entry_row(
                    id,
                    false,
                    MenuCommand::command(serde_json::json!({})),
                )]
            } else {
                Vec::new()
            }
        }))
    }

    /// Q4313 A: tiles, Copy ›, Organise ›, Hide, More ›, then the two Close rows.
    #[test]
    fn the_sidebar_project_menu_is_layout_a() {
        let every: Vec<&str> = ENTRIES.iter().map(|entry| entry.id).collect();
        assert_eq!(
            filled(ProjectMenuSpot::Sidebar, &every),
            [
                "[Rename Project, Add Worktree, New Terminal, New Tab]",
                "Copy › (Copy Path, Copy Remote URL)",
                "Organise › (Project Group, Spaces, Move to Workspace)",
                "Hide",
                "More › (History, Open Folder, Open in, , , Clone on…, -, Sleep Inactive, Full Reload, Autopilot)",
                "-",
                "Close Inactive",
                "Close Project",
                "Delete Worktree",
            ]
        );
    }

    /// A submenu left with one row shows that row; an empty one and its stray lines go.
    #[test]
    fn a_submenu_of_one_row_shows_the_row() {
        assert_eq!(
            filled(
                ProjectMenuSpot::Sidebar,
                &[
                    "newTerminal",
                    "copyPath",
                    "projectGroup",
                    "hide",
                    "openFolder",
                    "closeProject"
                ]
            ),
            [
                "[New Terminal]",
                "Copy Path",
                "Project Group",
                "Hide",
                "More › (Open Folder)",
                "-",
                "Close Project",
            ]
        );
    }

    #[test]
    fn the_phone_project_menu_folds_its_own_rows_into_more() {
        let every: Vec<&str> = ENTRIES.iter().map(|entry| entry.id).collect();
        assert_eq!(
            filled(ProjectMenuSpot::Phone, &every),
            [
                "[New Terminal, Actions…]",
                "Copy Path",
                "Project Group",
                "Files",
                "More › (Details, Move Project Up, Move Project Down, Refresh Sessions, -, Sleep Inactive, Full Reload)",
                "-",
                "Close Inactive",
            ]
        );
    }

    #[test]
    fn every_project_place_has_an_entry() {
        for spot in ProjectMenuSpot::ALL {
            fn walk(nodes: &[LayoutNode]) {
                for node in nodes {
                    match node {
                        LayoutNode::Row(id) => assert!(entry(id).is_some(), "{id}"),
                        LayoutNode::Tiles(ids) => ids
                            .iter()
                            .for_each(|id| assert!(entry(id).is_some(), "{id}")),
                        LayoutNode::More(children) => walk(children),
                        LayoutNode::Submenu(id, children) => {
                            assert!(entry(id).is_some(), "{id}");
                            walk(children);
                        }
                        LayoutNode::Separator => {}
                    }
                }
            }
            walk(&project_menu_layout(spot));
        }
    }
}
