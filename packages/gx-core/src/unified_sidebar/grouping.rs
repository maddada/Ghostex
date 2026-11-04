//! Group by Project: every machine's sessions of one repository under one header, with that
//! project's own shelves inside it, in the order the user dragged the headers into.

use std::collections::BTreeMap;

use super::build::{UnifiedFilterOption, UnifiedRow, UnifiedSidebarView};

/// One project's header and what sits under it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedGroup {
    /// [`super::RepoIdentity::key`], which is what the saved order and the fold state hold.
    pub repo_key: String,
    pub label: String,
    /// Which of its machines' name the filter's menu prints for it.
    pub detail: Option<String>,
    pub pinned: Vec<UnifiedRow>,
    pub active: Vec<UnifiedRow>,
    pub working: Vec<UnifiedRow>,
    pub snoozed: Vec<UnifiedRow>,
    pub parked: Vec<UnifiedRow>,
}

impl UnifiedGroup {
    pub fn count(&self) -> usize {
        self.pinned.len()
            + self.active.len()
            + self.working.len()
            + self.snoozed.len()
            + self.parked.len()
    }
}

/// The repositories in the order a list of them is drawn: the saved order first, then any the
/// order has not met, A to Z.
///
/// CDXC:Sidebar 2026-10-31 DECISION:
/// User (Q850, "drag"): the project groups keep the order the user drags them into, saved per repository, so a repository's place follows it across machines. A repository the order has not met lands after the ordered ones, A to Z, so a new project never pushes an arranged one down. A key whose repository is gone stays in the saved order, so a machine that was offline for a day finds its projects where they were.
pub fn ordered_repo_keys<'a>(options: &'a [UnifiedFilterOption], order: &[String]) -> Vec<&'a str> {
    let place: BTreeMap<&str, usize> = order
        .iter()
        .enumerate()
        .rev()
        .map(|(index, key)| (key.as_str(), index))
        .collect();
    let mut keys: Vec<&UnifiedFilterOption> = options.iter().collect();
    keys.sort_by(|left, right| {
        let place_of = |option: &UnifiedFilterOption| place.get(option.key.as_str()).copied();
        match (place_of(left), place_of(right)) {
            (Some(left), Some(right)) => left.cmp(&right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left
                .label
                .to_lowercase()
                .cmp(&right.label.to_lowercase())
                .then_with(|| left.key.cmp(&right.key)),
        }
    });
    keys.into_iter().map(|option| option.key.as_str()).collect()
}

/// Puts a list's rows under their projects' headers.
///
/// CDXC:Sidebar 2026-10-31 DECISION:
/// User (Q849, "headers shelves inside"): with Group by Project on, every machine's sessions of one repository sit under one header, and Working, Snoozed and Parked sit inside each project instead of at the bottom of the list, so a project keeps everything of its own in one place. Drafts stay above every group, since a draft is what you were about to do. Pinned sessions lead their own project's cards. Cards inside a group keep the "came back to you" order. A project with no sessions, or none the filter or search kept, draws no header.
pub fn group_unified_sidebar(view: &UnifiedSidebarView, order: &[String]) -> Vec<UnifiedGroup> {
    let mut groups: BTreeMap<String, UnifiedGroup> = BTreeMap::new();
    let mut place = |rows: &[UnifiedRow], shelf: fn(&mut UnifiedGroup) -> &mut Vec<UnifiedRow>| {
        for row in rows {
            let group = groups.entry(row.repo.key.clone()).or_default();
            shelf(group).push(row.clone());
        }
    };
    place(&view.pinned, |group| &mut group.pinned);
    place(&view.active, |group| &mut group.active);
    place(&view.working, |group| &mut group.working);
    place(&view.snoozed, |group| &mut group.snoozed);
    place(&view.parked, |group| &mut group.parked);
    ordered_repo_keys(&view.options.repos, order)
        .into_iter()
        .filter_map(|key| {
            let mut group = groups.remove(key)?;
            let option = view.options.repos.iter().find(|option| option.key == key)?;
            group.repo_key = key.to_string();
            group.label.clone_from(&option.label);
            group.detail.clone_from(&option.detail);
            Some(group)
        })
        .collect()
}

/// The saved order after a header is dropped before (or after) another.
///
/// `all` is every repository the options list, including the ones the filter or Hide keep off
/// screen right now. The result is the whole order written out, so those keep their place relative
/// to their neighbours instead of being pushed to the end, and a saved key no machine lists right
/// now stays where it was.
pub fn move_unified_project(
    order: &[String],
    all: &[UnifiedFilterOption],
    moved: &str,
    target: &str,
    after: bool,
) -> Vec<String> {
    let mut keys: Vec<String> = order.to_vec();
    for key in ordered_repo_keys(all, order) {
        if !keys.iter().any(|known| known == key) {
            keys.push(key.to_string());
        }
    }
    if moved == target || !keys.iter().any(|key| key == target) {
        return keys;
    }
    keys.retain(|key| key != moved);
    let at = keys
        .iter()
        .position(|key| key == target)
        .map_or(keys.len(), |index| index + usize::from(after));
    keys.insert(at, moved.to_string());
    keys
}
