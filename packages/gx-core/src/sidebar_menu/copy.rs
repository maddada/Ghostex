//! A session menu's Copy ID row and its Copy submenu: Copy Details, Copy Link, and every link or
//! name of the session's work.

use crate::sidebar_view::view::SessionRow;

use super::clipboard::{session_details_text, session_link_text, session_mention_text};
use super::commands::{message, MenuCommand};
use super::group::MenuGroup;
use super::item::MenuItem;

/// The session menu's own Copy ID row, or `None` while the machine's daemon id is not known yet,
/// since a mention without the ref reaches nobody.
///
/// CDXC:ContextMenus 2026-11-12 DECISION:
/// User: "i want a new Copy ID that let's me just copy the id and name of an agent so i can mention it in other chats" (2026-10-28), then (2026-11-12) "move Copy ID to sit directly above Tag As": Copy ID leaves the Copy submenu and is a row of the session menu itself, right above Tag As. It copies `[<session title>](<Global Ref>)` (`session_mention` in gx-protocol), which the chats draw as the session's agent pill; dropping a session row on a chat's composer types the same text. The phone's long-press menu matches.
pub(crate) fn copy_id_row(row: &SessionRow, group: &MenuGroup<'_>) -> Option<MenuItem> {
    session_mention_text(row, &group.details())
        .filter(|text| !text.trim().is_empty())
        .map(|text| {
            MenuItem::row(
                "Copy ID",
                "hash",
                MenuCommand::command(message::copy_text(&text)),
            )
        })
}

/// The Copy submenu of one row. Copy Details is always there; the other rows only when the session
/// has something for them (a branch from the git probe or work mode, the work-mode links).
///
/// CDXC:ContextMenus 2026-10-09 DECISION:
/// User: Copy Details moves into a Copy submenu next to Copy Branch, Copy Linear ID, Copy PR Link and the like, for normal and work-mode sessions; it stays always available so anyone can hand a session to another agent. Supersedes the 2026-09-26 decision that kept Copy Details under Advanced.
///
/// CDXC:ContextMenus 2026-10-31 DECISION:
/// User (GitHub issue #373): a "Copy link" item next to Copy Details and Copy ID, named Copy Link to match the other Copy rows, and it copies the https page that works where people paste it (`session_page_url` in gx-protocol). Left out when the machine's daemon id is not known yet, since a link without the ref reaches nobody. The phone's Copy submenu matches.
pub(crate) fn copy_submenu(id: &str, row: &SessionRow, group: &MenuGroup<'_>) -> MenuItem {
    let mut children = vec![MenuItem::row(
        "Copy Details",
        "copy",
        MenuCommand::command(message::copy_session_details(
            id,
            &session_details_text(row, &group.details()),
        )),
    )];
    let mut copy_row = |label: &str, icon: &str, text: Option<String>| {
        if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
            children.push(MenuItem::row(
                label,
                icon,
                MenuCommand::command(message::copy_text(&text)),
            ));
        }
    };
    copy_row(
        "Copy Link",
        "link",
        session_link_text(row, &group.details()),
    );
    copy_row("Copy Branch", "git-branch", row.menu_facts.branch.clone());
    let work = row.work.as_ref();
    let linear_issues = work
        .map(|work| work.linear_issues.as_slice())
        .unwrap_or(&[]);
    // Several issues shipped in one PR copy as one comma-joined line, which is what a PR title or
    // a Linear search takes.
    copy_row(
        if linear_issues.len() > 1 {
            "Copy Linear IDs"
        } else {
            "Copy Linear ID"
        },
        "hash",
        (!linear_issues.is_empty()).then(|| {
            linear_issues
                .iter()
                .map(|issue| issue.identifier.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }),
    );
    copy_row(
        "Copy Linear Link",
        "link",
        joined_lines(
            linear_issues
                .iter()
                .filter_map(|issue| issue.url.as_deref()),
        ),
    );
    copy_row(
        "Copy PR Link",
        "git-pull-request",
        work.and_then(|work| work.pull_request.as_ref())
            .and_then(|pr| pr.url.clone()),
    );
    copy_row(
        "Copy Issue Link",
        "circle-dot",
        joined_lines(
            work.map(|work| work.github_issues.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter_map(|issue| issue.url.as_deref()),
        ),
    );
    copy_row(
        "Copy Linear Project Link",
        "box",
        work.and_then(|work| work.linear_project.as_ref())
            .and_then(|project| project.url.clone()),
    );
    // The claude.ai conversation a Claude cloud session's row is linked to.
    copy_row(
        "Copy claude.ai Link",
        "link",
        row.claude_cloud
            .as_ref()
            .and_then(|cloud| cloud.url.clone()),
    );
    MenuItem::submenu("Copy", "copy", children)
}

fn joined_lines<'a>(values: impl Iterator<Item = &'a str>) -> Option<String> {
    let values: Vec<&str> = values.filter(|value| !value.trim().is_empty()).collect();
    (!values.is_empty()).then(|| values.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{MachineId, SessionKey};
    use crate::sidebar_menu::clipboard::MORE_DETAILS_LINE;

    fn row() -> SessionRow {
        SessionRow {
            key: Some(SessionKey {
                machine: MachineId::Local,
                project_id: "P3lv0".to_string(),
                session_id: "G7hgi".to_string(),
            }),
            alias: "Model and effort inventory for agent CLIs".to_string(),
            agent_icon: Some("claude".to_string()),
            ..SessionRow::default()
        }
    }

    fn group(server_id: Option<&str>) -> MenuGroup<'static> {
        MenuGroup {
            group_id: "P3lv0",
            storage_id: "P3lv0",
            title: "Ghostex",
            is_remote: false,
            remote_machine_name: None,
            is_stale: false,
            can_create_session_group: false,
            can_focus_mode: false,
            project: None,
            can_remove_project: false,
            git_remote_origin_url: None,
            server_id: server_id.map(str::to_string),
            agent_place: Default::default(),
        }
    }

    /// The submenu's rows as (label, copied text).
    fn copied(group: &MenuGroup<'_>) -> Vec<(String, String)> {
        copy_submenu("G7hgi", &row(), group)
            .children
            .unwrap_or_default()
            .into_iter()
            .map(|child| {
                let message = child
                    .command
                    .as_ref()
                    .map(MenuCommand::to_json)
                    .unwrap_or_default();
                let message = &message["message"];
                let text = message["text"]
                    .as_str()
                    .or_else(|| message["detailsText"].as_str())
                    .unwrap_or_default()
                    .to_string();
                (child.label.unwrap_or_default(), text)
            })
            .collect()
    }

    #[test]
    fn copy_id_copies_a_mention() {
        let item = copy_id_row(&row(), &group(Some("S90"))).expect("a Copy ID row");
        assert_eq!(item.label.as_deref(), Some("Copy ID"));
        let message = item
            .command
            .as_ref()
            .map(MenuCommand::to_json)
            .unwrap_or_default();
        assert_eq!(
            message["message"]["text"],
            "[Model and effort inventory for agent CLIs](S90:P3lv0:G7hgi)"
        );
        assert!(copy_id_row(&row(), &group(None)).is_none());
    }

    #[test]
    fn copy_link_copies_the_page_under_copy_details() {
        let rows = copied(&group(Some("S90")));
        assert_eq!(rows[0].0, "Copy Details");
        assert_eq!(
            rows[1],
            (
                "Copy Link".to_string(),
                "https://ghostex.dev/open/session#S90:P3lv0:G7hgi".to_string()
            )
        );
    }

    #[test]
    fn copy_link_waits_for_the_daemon_id() {
        let labels: Vec<String> = copied(&group(None)).into_iter().map(|row| row.0).collect();
        assert_eq!(labels, vec!["Copy Details".to_string()]);
    }

    #[test]
    fn copy_details_ends_by_pointing_at_ghostex_agents() {
        let rows = copied(&group(Some("S90")));
        let details = &rows[0].1;
        assert_eq!(details.lines().last(), Some(MORE_DETAILS_LINE));
        assert!(details.contains("Global Ref: S90:P3lv0:G7hgi"));
        assert!(!details.contains('$'), "no skill reference: {details}");
        assert!(MORE_DETAILS_LINE.contains("`ghostex agents`"));
    }
}
