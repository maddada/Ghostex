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
use super::clipboard::session_details_text;
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
    pub now_ms: u64,
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
    let caps = SessionCapabilities::resolve(
        row,
        input.group.is_remote,
        settings.show_session_command_copy_actions,
        settings.show_session_details_copy_action,
        input.group.workspace_focus_bridge,
    );
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
        note: caps
            .can_open_session_note
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
    // An action the hover strip offers is repeated at the top of the menu, newest first, with the
    // two that would be duplicated by their own rows left out.
    let mirror: Vec<HoverAction> = if settings.show_session_card_hover_buttons_in_context_menu {
        if caps.is_browser_session {
            vec![HoverAction::Sleep]
        } else {
            enabled
                .iter()
                .rev()
                .copied()
                .filter(|action| {
                    *action != HoverAction::Close && *action != HoverAction::CloseAfterDone
                })
                .collect()
        }
    } else {
        Vec::new()
    };
    const PRIMARY_ORDER: [HoverAction; 7] = [
        HoverAction::Rename,
        HoverAction::Sleep,
        HoverAction::Pin,
        HoverAction::Park,
        HoverAction::Snooze,
        HoverAction::Note,
        HoverAction::Tag,
    ];
    let mut menu: Vec<MenuItem> = mirror
        .into_iter()
        .chain(
            PRIMARY_ORDER
                .into_iter()
                .filter(|action| !enabled.contains(action)),
        )
        .filter_map(|action| match (action, rows.get(action)) {
            (HoverAction::Pin, Some(item)) if !row.is_pinned => Some(MenuItem {
                icon: Some("pinned".to_string()),
                ..item.clone()
            }),
            (_, item) => item.cloned(),
        })
        .collect();

    let mut advanced: Vec<MenuItem> = vec![MenuItem::heading("Session")];
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
    if caps.can_fork_session {
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
    if !caps.is_browser_session
        && !group.is_stale
        && matches!(account_provider, Some("claude") | Some("codex"))
    {
        advanced.push(MenuItem {
            label: Some("Switch Account".to_string()),
            icon: Some("users-group".to_string()),
            keep_open: true,
            command: Some(MenuCommand::session_accounts_load(id)),
            ..MenuItem::default()
        });
    }
    if caps.can_export_transcript {
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
    if caps.can_split_session_right {
        advanced.push(MenuItem::row(
            "Split Right",
            "layout-columns",
            MenuCommand::command(message::split_session_right(id)),
        ));
    }
    if group.can_create_session_group {
        advanced.push(MenuItem::row(
            "Move to New Group",
            "layout-sidebar-right-expand",
            MenuCommand::command(message::create_group_from_session(id)),
        ));
    }
    if group.can_focus_mode {
        advanced.push(MenuItem::row(
            "Focus",
            "focus-2",
            MenuCommand::command(message::focus_session_mode(id)),
        ));
    }
    let mut copy: Vec<MenuItem> = Vec::new();
    if caps.can_copy_session_details {
        copy.push(MenuItem::row(
            "Copy Details",
            "copy",
            MenuCommand::command(message::copy_session_details(
                id,
                &session_details_text(row, &group.details()),
            )),
        ));
    }
    if caps.can_copy_resume_command {
        copy.push(MenuItem::row(
            "Copy Resume",
            "copy",
            MenuCommand::command(message::copy_resume_command(id)),
        ));
    }
    if caps.can_copy_attach_command {
        copy.push(MenuItem::row(
            "Copy Attach Command",
            "copy",
            MenuCommand::command(message::copy_attach_command(id)),
        ));
    }
    if !copy.is_empty() {
        advanced.push(MenuItem::separator());
        advanced.push(MenuItem::heading("Copy"));
        advanced.extend(copy);
    }
    if !input.below.is_empty() {
        advanced.push(MenuItem::separator());
        advanced.push(MenuItem::heading("Below"));
        let sleepable: Vec<String> = input
            .below
            .iter()
            .filter(|row| can_sleep(row))
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
                MenuCommand::command(message::close_sessions(&below_ids)),
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
    if advanced.len() > 1 {
        menu.push(MenuItem::separator());
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
