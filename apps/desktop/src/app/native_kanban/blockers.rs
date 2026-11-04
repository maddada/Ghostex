//! A ticket's blockers: which cards it waits on (bd's `dependencies[]`) and which cards wait on it,
//! the refusals bd itself makes (a card blocking itself, a loop of blocks), and the local edit the
//! board shows while bd's `dep add` / `dep remove` runs.

use std::collections::{HashSet, VecDeque};

use super::model::{BLOCKS_DEPENDENCY, BeadsDependency, BeadsIssue, BoardTicket};

/// Which list a card picked in the ticket panel's blocker search joins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BlockerDirection {
    /// The picked card blocks this one.
    #[default]
    BlockedBy,
    /// This card blocks the picked one.
    Blocks,
}

/// One link in a ticket's blocker lists: the other card's id and bd's dependency type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockerLink {
    pub(crate) other_id: String,
    pub(crate) kind: String,
}

/// One waits-on edge: `waiter` cannot start until `blocker` is done. The ticket panel's New ticket
/// uses an empty id for the card it has not created yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockerEdge {
    pub(crate) waiter: String,
    pub(crate) blocker: String,
}

/// The cards `card_id` waits on, every kind bd records, in bd's order.
pub(crate) fn blocked_by_links(issues: &[BeadsIssue], card_id: &str) -> Vec<BlockerLink> {
    issues
        .iter()
        .find(|issue| issue.id == card_id)
        .map(|issue| {
            issue
                .dependencies
                .iter()
                .map(|dependency| BlockerLink {
                    other_id: dependency.depends_on_id.clone(),
                    kind: dependency.kind.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The cards that wait on `card_id`, in board order.
pub(crate) fn blocks_links(issues: &[BeadsIssue], card_id: &str) -> Vec<BlockerLink> {
    issues
        .iter()
        .flat_map(|issue| {
            issue
                .dependencies
                .iter()
                .filter(|dependency| dependency.depends_on_id == card_id)
                .map(|dependency| BlockerLink {
                    other_id: issue.id.clone(),
                    kind: dependency.kind.clone(),
                })
        })
        .collect()
}

/// Whether `from` already waits on `to`, directly or through other cards, over `blocks` links
/// (the only kind bd's cycle check follows) plus the `extra` edges not in bd yet.
fn waits_on(issues: &[BeadsIssue], extra: &[BlockerEdge], from: &str, to: &str) -> bool {
    let blockers_of = |id: &str| -> Vec<String> {
        let saved = issues
            .iter()
            .filter(|issue| issue.id == id)
            .flat_map(|issue| issue.dependencies.iter())
            .filter(|dependency| dependency.kind == BLOCKS_DEPENDENCY)
            .map(|dependency| dependency.depends_on_id.clone());
        let staged = extra
            .iter()
            .filter(|edge| edge.waiter == id)
            .map(|edge| edge.blocker.clone());
        saved.chain(staged).collect()
    };
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([from.to_string()]);
    while let Some(id) = queue.pop_front() {
        if id == to {
            return true;
        }
        if seen.insert(id.clone()) {
            queue.extend(blockers_of(&id));
        }
    }
    false
}

/// CDXC:ProjectBoard 2026-10-31 WHY: bd refuses a self-dependency and a `blocks` link that closes a loop (`bd dep add` exits 1 with "cannot add self-dependency" or "adding dependency would create a cycle"); the panel refuses the same two before calling bd, so a New ticket's links (which bd only sees after the create) are held to the same rule, and says it in plain words. `name` turns an id into what the panel shows for it.
pub(crate) fn blocker_refusal(
    issues: &[BeadsIssue],
    extra: &[BlockerEdge],
    edge: &BlockerEdge,
    name: impl Fn(&str) -> String,
) -> Option<String> {
    if edge.waiter == edge.blocker {
        return Some("A card can't block itself.".to_string());
    }
    waits_on(issues, extra, &edge.blocker, &edge.waiter).then(|| {
        format!(
            "{} already waits on {}, so this would make a loop.",
            name(&edge.blocker),
            name(&edge.waiter)
        )
    })
}

/// bd's own refusal (the board was stale, or another client added a link meanwhile), in the
/// panel's words.
pub(crate) fn plain_blocker_error(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("self-dependency") || lower.contains("depend on itself") {
        "A card can't block itself.".to_string()
    } else if lower.contains("cycle") {
        "That link would make a loop of cards waiting on each other.".to_string()
    } else {
        error.to_string()
    }
}

/// The board's copy of a `dep add` before bd answers: the link, and the counts the lane cards draw.
pub(crate) fn add_local_blocker(issues: &mut [BeadsIssue], edge: &BlockerEdge) -> bool {
    let Some(waiter) = issues.iter_mut().find(|issue| issue.id == edge.waiter) else {
        return false;
    };
    if waiter
        .dependencies
        .iter()
        .any(|dependency| dependency.depends_on_id == edge.blocker)
    {
        return false;
    }
    waiter.dependencies.push(BeadsDependency {
        depends_on_id: edge.blocker.clone(),
        kind: BLOCKS_DEPENDENCY.to_string(),
    });
    waiter.dependency_count = waiter.dependency_count.map(|count| count + 1);
    if let Some(blocker) = issues.iter_mut().find(|issue| issue.id == edge.blocker) {
        blocker.dependent_count = blocker.dependent_count.map(|count| count + 1);
    }
    true
}

/// The board's copy of a `dep remove`; returns the link removed so a refusal can put it back.
pub(crate) fn remove_local_blocker(
    issues: &mut [BeadsIssue],
    edge: &BlockerEdge,
) -> Option<BeadsDependency> {
    let waiter = issues.iter_mut().find(|issue| issue.id == edge.waiter)?;
    let index = waiter
        .dependencies
        .iter()
        .position(|dependency| dependency.depends_on_id == edge.blocker)?;
    let removed = waiter.dependencies.remove(index);
    waiter.dependency_count = waiter.dependency_count.map(|count| count.saturating_sub(1));
    if let Some(blocker) = issues.iter_mut().find(|issue| issue.id == edge.blocker) {
        blocker.dependent_count = blocker.dependent_count.map(|count| count.saturating_sub(1));
    }
    Some(removed)
}

/// Puts back a link `remove_local_blocker` took out, in its old kind.
pub(crate) fn restore_local_blocker(
    issues: &mut [BeadsIssue],
    edge: &BlockerEdge,
    dependency: BeadsDependency,
) {
    let Some(waiter) = issues.iter_mut().find(|issue| issue.id == edge.waiter) else {
        return;
    };
    if waiter
        .dependencies
        .iter()
        .any(|existing| existing.depends_on_id == edge.blocker)
    {
        return;
    }
    waiter.dependencies.push(dependency);
    waiter.dependency_count = waiter.dependency_count.map(|count| count + 1);
    if let Some(blocker) = issues.iter_mut().find(|issue| issue.id == edge.blocker) {
        blocker.dependent_count = blocker.dependent_count.map(|count| count + 1);
    }
}

/// The blocker search's results: cards whose id, board id or title holds `query`, leaving out
/// this card and the ones already in the list being added to.
pub(crate) fn blocker_candidates<'a>(
    tickets: &'a [BoardTicket],
    query: &str,
    card_id: Option<&str>,
    linked: &[String],
    limit: usize,
) -> Vec<&'a BoardTicket> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    tickets
        .iter()
        .filter(|ticket| Some(ticket.issue.id.as_str()) != card_id)
        .filter(|ticket| !linked.contains(&ticket.issue.id))
        .filter(|ticket| {
            ticket.display_id.to_lowercase().contains(&query)
                || ticket.issue.id.to_lowercase().contains(&query)
                || ticket.issue.title.to_lowercase().contains(&query)
        })
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(id: &str, blocked_by: &[&str]) -> BeadsIssue {
        BeadsIssue {
            id: id.to_string(),
            title: format!("Card {id}"),
            dependency_count: Some(blocked_by.len() as u64),
            dependent_count: Some(0),
            dependencies: blocked_by
                .iter()
                .map(|blocker| BeadsDependency {
                    depends_on_id: blocker.to_string(),
                    kind: BLOCKS_DEPENDENCY.to_string(),
                })
                .collect(),
            ..BeadsIssue::default()
        }
    }

    fn edge(waiter: &str, blocker: &str) -> BlockerEdge {
        BlockerEdge {
            waiter: waiter.to_string(),
            blocker: blocker.to_string(),
        }
    }

    fn name(id: &str) -> String {
        if id.is_empty() {
            "This card".to_string()
        } else {
            id.to_uppercase()
        }
    }

    fn ids(links: &[BlockerLink]) -> Vec<&str> {
        links.iter().map(|link| link.other_id.as_str()).collect()
    }

    #[test]
    fn reads_both_directions_from_bd_dependencies() {
        let issues = vec![issue("a", &[]), issue("b", &["a"]), issue("c", &["a", "b"])];
        assert_eq!(ids(&blocked_by_links(&issues, "c")), ["a", "b"]);
        assert_eq!(ids(&blocks_links(&issues, "a")), ["b", "c"]);
        assert!(blocked_by_links(&issues, "a").is_empty());
        assert!(blocks_links(&issues, "c").is_empty());
    }

    #[test]
    fn links_an_agent_set_with_bd_show_in_both_lists() {
        // `bd list --all --json` (bd 1.2.2) after `bd dep add p-b p-a` and a parent-child link.
        let listed = serde_json::json!([
            { "id": "p-a", "title": "Card A", "dependent_count": 1 },
            {
                "id": "p-b",
                "title": "Card B",
                "dependency_count": 2,
                "dependencies": [
                    { "issue_id": "p-b", "depends_on_id": "p-a", "type": "blocks" },
                    { "issue_id": "p-b", "depends_on_id": "p-epic", "type": "parent-child" }
                ]
            }
        ]);
        let issues = listed
            .as_array()
            .unwrap()
            .iter()
            .filter_map(BeadsIssue::from_value)
            .collect::<Vec<_>>();
        assert_eq!(
            blocked_by_links(&issues, "p-b"),
            [
                BlockerLink {
                    other_id: "p-a".to_string(),
                    kind: "blocks".to_string()
                },
                BlockerLink {
                    other_id: "p-epic".to_string(),
                    kind: "parent-child".to_string()
                },
            ]
        );
        assert_eq!(ids(&blocks_links(&issues, "p-a")), ["p-b"]);
    }

    #[test]
    fn adding_a_blocker_links_it_and_moves_both_counts() {
        let mut issues = vec![issue("a", &[]), issue("b", &[])];
        assert!(add_local_blocker(&mut issues, &edge("b", "a")));
        assert_eq!(ids(&blocked_by_links(&issues, "b")), ["a"]);
        assert_eq!(ids(&blocks_links(&issues, "a")), ["b"]);
        assert_eq!(issues[1].dependency_count, Some(1));
        assert_eq!(issues[0].dependent_count, Some(1));
        // A second add of the same link changes nothing, as `bd dep add` does.
        assert!(!add_local_blocker(&mut issues, &edge("b", "a")));
        assert_eq!(issues[1].dependencies.len(), 1);
        assert_eq!(issues[1].dependency_count, Some(1));
    }

    #[test]
    fn removing_a_blocker_unlinks_it_and_restore_puts_it_back() {
        let mut issues = vec![issue("a", &[]), issue("b", &["a"])];
        issues[0].dependent_count = Some(1);
        let removed = remove_local_blocker(&mut issues, &edge("b", "a")).expect("linked");
        assert!(blocked_by_links(&issues, "b").is_empty());
        assert!(blocks_links(&issues, "a").is_empty());
        assert_eq!(issues[1].dependency_count, Some(0));
        assert_eq!(issues[0].dependent_count, Some(0));
        assert!(remove_local_blocker(&mut issues, &edge("b", "a")).is_none());
        restore_local_blocker(&mut issues, &edge("b", "a"), removed);
        assert_eq!(ids(&blocked_by_links(&issues, "b")), ["a"]);
        assert_eq!(issues[1].dependency_count, Some(1));
        assert_eq!(issues[0].dependent_count, Some(1));
    }

    #[test]
    fn refuses_a_card_blocking_itself() {
        let issues = vec![issue("a", &[])];
        assert_eq!(
            blocker_refusal(&issues, &[], &edge("a", "a"), name).as_deref(),
            Some("A card can't block itself.")
        );
    }

    #[test]
    fn refuses_a_direct_and_an_indirect_loop() {
        // c waits on b, b waits on a.
        let issues = vec![issue("a", &[]), issue("b", &["a"]), issue("c", &["b"])];
        assert_eq!(
            blocker_refusal(&issues, &[], &edge("a", "b"), name).as_deref(),
            Some("B already waits on A, so this would make a loop.")
        );
        assert_eq!(
            blocker_refusal(&issues, &[], &edge("a", "c"), name).as_deref(),
            Some("C already waits on A, so this would make a loop.")
        );
        // The same direction again, or an unrelated pair, is fine.
        assert_eq!(blocker_refusal(&issues, &[], &edge("c", "a"), name), None);
        let more = vec![issue("a", &[]), issue("b", &["a"]), issue("d", &[])];
        assert_eq!(blocker_refusal(&more, &[], &edge("a", "d"), name), None);
    }

    #[test]
    fn only_blocks_links_count_towards_a_loop() {
        // b is a child of a, not blocked by it: bd lets a wait on b.
        let mut issues = vec![issue("a", &[]), issue("b", &[])];
        issues[1].dependencies.push(BeadsDependency {
            depends_on_id: "a".to_string(),
            kind: "parent-child".to_string(),
        });
        assert_eq!(blocker_refusal(&issues, &[], &edge("a", "b"), name), None);
    }

    #[test]
    fn a_new_cards_staged_links_are_checked_together() {
        // The new card (empty id) is to wait on b, and c already waits on b's blocker a.
        let issues = vec![issue("a", &[]), issue("b", &["a"]), issue("c", &[])];
        let staged = vec![edge("", "b")];
        // b waits on a; making a wait on the new card closes new -> b -> a -> new.
        assert_eq!(
            blocker_refusal(&issues, &staged, &edge("a", ""), name).as_deref(),
            Some("This card already waits on A, so this would make a loop.")
        );
        assert_eq!(
            blocker_refusal(&issues, &staged, &edge("c", ""), name),
            None
        );
    }

    #[test]
    fn bd_refusals_read_as_plain_words() {
        assert_eq!(
            plain_blocker_error("cannot add self-dependency: p-1 cannot depend on itself"),
            "A card can't block itself."
        );
        assert_eq!(
            plain_blocker_error("adding dependency would create a cycle"),
            "That link would make a loop of cards waiting on each other."
        );
        assert_eq!(
            plain_blocker_error("issue p-9 not found"),
            "issue p-9 not found"
        );
    }

    #[test]
    fn search_finds_by_id_or_title_and_skips_this_card_and_linked_ones() {
        let issues = vec![issue("p-1", &[]), issue("p-2", &[]), issue("p-3", &[])];
        let tickets = issues
            .into_iter()
            .map(|issue| BoardTicket {
                display_id: issue.id.to_uppercase(),
                board_status: "todo".to_string(),
                issue,
            })
            .collect::<Vec<_>>();
        let found = |query: &str, linked: &[String]| {
            blocker_candidates(&tickets, query, Some("p-1"), linked, 6)
                .into_iter()
                .map(|ticket| ticket.issue.id.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(found("card", &[]), ["p-2", "p-3"]);
        assert_eq!(found("P-3", &[]), ["p-3"]);
        assert_eq!(found("card", &["p-2".to_string()]), ["p-3"]);
        assert!(found("  ", &[]).is_empty());
    }
}
