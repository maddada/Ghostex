//! Group sessions by tags (`groupSessionsByTags`): the Unified list's active cards in groups, one
//! per enabled and visible entry of the Session Tags list, in that list's order. The six status
//! entries (sidebar_view/status_tags.rs) hold the sessions whose state they name; a hand tag or No
//! tag holds the sessions carrying it; a card no enabled entry applies to stays in an unheaded list
//! below the groups.
//!
//! CDXC:Sidebar 2026-11-14 DECISION:
//! User: the Unified list can group its sessions by status across all projects, then by project: Needs you, Question, Error, Finished (not yet read), Working, Idle, in that default order, with set-off headers that count top-level cards; the order is the Session Tags list's, changed in Settings on a computer or on the phone; a coordinator stands where its most urgent thread stands; groups are open and empty ones are not shown; Drafts and Pinned stay above the groups and Snoozed and Parked below, also with Group by project on, where each group holds its projects' headings instead; no Moving section while grouping is on (build.rs `unified_shelf`). The maintainer asked for a new switch that reuses the tags system and leaves people who use tags as they are with no change, so it is off by default.
//! SEE-ALSO: apps/mobile/app/src/contract/unified/ (the phone's port of this rule), apps/desktop/src/app/native_sidebar/unified/status_groups.rs (the drawing), packages/settings-catalog/src/general/sidebar.rs (the switch).

use ghostex_gx_protocol::presentation::AttentionKind;

use crate::keys::MachineId;
use crate::presentation_store::PresentationStore;
use crate::sidebar_view::status_tags::StatusGroup;
use crate::sidebar_view::tags::{
    tag_list_item_label, tag_presentation, TagCatalog, TagListItemKind, UNTAGGED_TAG_FILTER,
};
use crate::sidebar_view::{SessionRow, SidebarSettings, TagPresentation};

use super::build::{unified_status, UnifiedFilterOption, UnifiedRow, UnifiedSettings, UnifiedStatus};
use super::grouping::ordered_repo_keys;

/// The working assumptions of which group a session stands in, kept in one place so a correction
/// changes this block (and the phone's matching one), not the drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupingPolicy {
    /// A failure outranks a plain wait for input on the same session: Error, not Needs you.
    pub plain_wait_after_failure: bool,
    /// An attention without a known kind (an older computer sends none) is a wait for input:
    /// Needs you, not Finished.
    pub unknown_attention_is_input_wait: bool,
    /// A finished turn whose handover has a Blocked or Needs you section is a wait for input:
    /// Needs you (after an approval, a question and a failure), not Finished (not yet read).
    pub finished_handover_is_input_wait: bool,
}

/// CDXC:Sidebar 2026-11-14 WHY: the two cases the approved states do not settle, answered the way the existing code and the phone's folded-display triage already answer them: `unified_status` checks a failure before an attention, and the triage counts an attention that is not `finished` as waiting on the user. The user can reverse each; the phone's port holds the same block.
///
/// CDXC:Sidebar 2026-11-14 DECISION:
/// User: a finished turn whose handover ends with a Blocked or Needs you section stands in Needs you, exactly like a plain wait for input (an approval stays Needs you, a question Question, a failure Error, a background job does not make it Working); a finished turn without such a handover stays in Finished (not yet read).
pub const GROUPING_POLICY: GroupingPolicy = GroupingPolicy {
    plain_wait_after_failure: true,
    unknown_attention_is_input_wait: true,
    finished_handover_is_input_wait: true,
};

/// The settings the list is built with: no Moving section while grouping is on (build.rs
/// `unified_shelf`), the stored `unifiedWorkingSection` left as it is.
pub fn unified_grouping_settings(settings: UnifiedSettings, group_by_tags: bool) -> UnifiedSettings {
    UnifiedSettings {
        working_section: settings.working_section && !group_by_tags,
        ..settings
    }
}

/// The group a session's own state puts it in: `unified_status`, the card's status word, with
/// its attention split by kind. No second status rule.
pub fn session_status_group(row: &SessionRow) -> StatusGroup {
    status_group_with(row, GROUPING_POLICY)
}

fn status_group_with(row: &SessionRow, policy: GroupingPolicy) -> StatusGroup {
    let waits_for_input = row.activity == "attention"
        && match &row.attention_kind {
            Some(AttentionKind::Finished) => {
                policy.finished_handover_is_input_wait && row.attention_handover_needs_you
            }
            Some(AttentionKind::NeedsInput) => true,
            Some(AttentionKind::Other(_)) | None => policy.unknown_attention_is_input_wait,
        };
    match unified_status(row) {
        Some(UnifiedStatus::Approval) => StatusGroup::NeedsYou,
        Some(UnifiedStatus::Question) => StatusGroup::Question,
        Some(UnifiedStatus::Failed) if waits_for_input && !policy.plain_wait_after_failure => {
            StatusGroup::NeedsYou
        }
        Some(UnifiedStatus::Failed) => StatusGroup::Error,
        Some(UnifiedStatus::Working | UnifiedStatus::Waiting) => StatusGroup::Working,
        Some(UnifiedStatus::Done) if waits_for_input => StatusGroup::NeedsYou,
        Some(UnifiedStatus::Done) => StatusGroup::Finished,
        None => StatusGroup::Idle,
    }
}

/// The group of a card's whole tree: the most urgent of the card and every thread under it, by
/// the default order, whatever the user's arrangement of the list and whether the threads are
/// folded or drawn.
pub fn tree_status_group(card: &UnifiedRow) -> StatusGroup {
    card.rows_in_tree()
        .into_iter()
        .map(|row| session_status_group(&row.session.row))
        .min()
        .unwrap_or(StatusGroup::Idle)
}

/// What heads a group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnifiedGroupHead {
    Status(StatusGroup),
    /// A hand tag (`id` the tag) or No tag (`id` `untagged`), drawn with its own icon in the
    /// neutral colour.
    Tag {
        id: String,
        label: String,
        presentation: Option<TagPresentation>,
    },
}

/// One enabled and visible entry of the Session Tags list that can head a group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnifiedTagEntry {
    pub head: UnifiedGroupHead,
}

impl UnifiedTagEntry {
    fn applies(&self, card: &UnifiedRow, tree: &mut Option<StatusGroup>) -> bool {
        match &self.head {
            UnifiedGroupHead::Status(group) => *tree.get_or_insert_with(|| tree_status_group(card)) == *group,
            UnifiedGroupHead::Tag { id, .. } => match card.session.row.effective_tag.as_deref() {
                Some(tag) => tag == id,
                None => id == UNTAGGED_TAG_FILTER,
            },
        }
    }
}

/// The entries that can head a group, in the Session Tags list's order: enabled and visible, no
/// separators. Normalized with this computer's own tag catalog, as the Sort & Filter menu is;
/// labels read from every computer's.
pub fn unified_tag_entries(
    store: &PresentationStore,
    settings: &SidebarSettings,
) -> Vec<UnifiedTagEntry> {
    let local = store
        .machine(&MachineId::Local)
        .and_then(|entry| entry.side_state().custom_session_tags.as_ref());
    let labels = TagCatalog::merged(
        std::iter::once(local).chain(
            store
                .machines()
                .filter(|(machine, _)| !machine.is_local())
                .map(|(_, entry)| entry.side_state().custom_session_tags.as_ref()),
        ),
    );
    settings
        .tag_list(local)
        .into_iter()
        .filter(|item| item.enabled && item.visible)
        .filter_map(|item| {
            let head = match item.kind {
                TagListItemKind::Separator => return None,
                TagListItemKind::Status => UnifiedGroupHead::Status(StatusGroup::from_id(&item.id)?),
                TagListItemKind::Tag | TagListItemKind::Untagged => {
                    let id = if item.kind == TagListItemKind::Untagged {
                        UNTAGGED_TAG_FILTER.to_string()
                    } else {
                        item.id.clone()
                    };
                    UnifiedGroupHead::Tag {
                        label: tag_list_item_label(&item, &labels),
                        presentation: tag_presentation(Some(&id), &labels),
                        id,
                    }
                }
            };
            Some(UnifiedTagEntry { head })
        })
        .collect()
}

/// One group and its cards: the saved project order first, then A to Z, and within a project the
/// order the list already has (when each came back to the user).
#[derive(Clone, Debug)]
pub struct UnifiedTagGroup<'a> {
    pub head: UnifiedGroupHead,
    pub cards: Vec<&'a UnifiedRow>,
}

/// The active cards grouped, and the ones no entry applies to.
#[derive(Clone, Debug, Default)]
pub struct UnifiedByTags<'a> {
    /// Only groups that hold a card, in the entries' order.
    pub groups: Vec<UnifiedTagGroup<'a>>,
    /// Drawn unheaded below the groups, never hidden.
    pub fallback: Vec<&'a UnifiedRow>,
}

/// Puts `active` (the list's ordinary cards, already in came-back order) in the groups of
/// `entries`: the first entry that applies to a card's tree takes the whole tree.
pub fn group_unified_by_tags<'a>(
    active: &'a [UnifiedRow],
    entries: &[UnifiedTagEntry],
    repos: &[UnifiedFilterOption],
    order: &[String],
) -> UnifiedByTags<'a> {
    let ranked = ordered_repo_keys(repos, order);
    let rank = |row: &UnifiedRow| {
        ranked
            .iter()
            .position(|key| *key == row.repo.key)
            .unwrap_or(ranked.len())
    };
    let mut cards: Vec<&UnifiedRow> = active.iter().collect();
    cards.sort_by_key(|row| rank(row));
    let mut buckets: Vec<Vec<&UnifiedRow>> = vec![Vec::new(); entries.len()];
    let mut fallback = Vec::new();
    for card in cards {
        let mut tree = None;
        match entries.iter().position(|entry| entry.applies(card, &mut tree)) {
            Some(index) => buckets[index].push(card),
            None => fallback.push(card),
        }
    }
    UnifiedByTags {
        groups: entries
            .iter()
            .zip(buckets)
            .filter(|(_, cards)| !cards.is_empty())
            .map(|(entry, cards)| UnifiedTagGroup {
                head: entry.head.clone(),
                cards,
            })
            .collect(),
        fallback,
    }
}
