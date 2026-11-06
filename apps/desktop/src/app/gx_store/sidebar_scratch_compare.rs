//! Comparing the list the cache kept with the same list built from nothing.
//!
//! CDXC:Sidebar 2026-09-20 WHY:
//! A difference here is never a difference with the old projection: both sides are this port's own
//! code reading the same store at the same instant, so one of them is a cache that was not
//! invalidated. That is a different question from the shadow comparison and it deserves its own
//! record, because the counter alone cannot say which field of which row the cache held on to. It
//! caught exactly one such bug in the live app before it had a record at all (a browser row kept a
//! pre-catalog tooltip because its cache was reused on the strength of an unchanged tab list), and
//! the only reason that took a production counter to find is that this record did not exist.
//!
//! Ids and field names only, like every other record here: never a title, a path or a tooltip.

use ghostex_gx_core::{SessionView, SidebarView};

/// Most ids one record names per list.
const MAX_IDS_PER_RECORD: usize = 24;

/// Where the kept list and the fresh one disagree.
#[derive(Clone, Debug, Default)]
pub(super) struct ScratchDifference {
    pub(super) only_incremental_groups: Vec<String>,
    pub(super) only_scratch_groups: Vec<String>,
    pub(super) group_order_differs: bool,
    /// Per group and per row: the id, and the names of the fields that differ. Kept apart rather
    /// than joined into a sentence, because the log redacts a string over 120 characters and a
    /// joined field list reaches that easily.
    pub(super) groups: Vec<(String, Vec<String>)>,
    pub(super) rows: Vec<(String, Vec<String>)>,
    pub(super) only_incremental_rows: Vec<String>,
    pub(super) only_scratch_rows: Vec<String>,
    /// The list itself, when the difference is above the groups.
    pub(super) top_level: Vec<String>,
}

impl ScratchDifference {
    fn differs(&self) -> bool {
        !self.only_incremental_groups.is_empty()
            || !self.only_scratch_groups.is_empty()
            || self.group_order_differs
            || !self.groups.is_empty()
            || !self.rows.is_empty()
            || !self.only_incremental_rows.is_empty()
            || !self.only_scratch_rows.is_empty()
            || !self.top_level.is_empty()
    }

    fn bound(&mut self) {
        self.only_incremental_groups.truncate(MAX_IDS_PER_RECORD);
        self.only_scratch_groups.truncate(MAX_IDS_PER_RECORD);
        self.groups.truncate(MAX_IDS_PER_RECORD);
        self.rows.truncate(MAX_IDS_PER_RECORD);
        self.only_incremental_rows.truncate(MAX_IDS_PER_RECORD);
        self.only_scratch_rows.truncate(MAX_IDS_PER_RECORD);
        self.top_level.truncate(MAX_IDS_PER_RECORD);
    }
}

/// Names where the two lists disagree. `None` when they are the same list.
pub(super) fn compare_views(
    incremental: &SidebarView,
    scratch: &SidebarView,
) -> Option<ScratchDifference> {
    let mut difference = ScratchDifference::default();
    let mut top = |name: &str, same: bool| {
        if !same {
            difference.top_level.push(name.to_string());
        }
    };
    top("ready", incremental.ready == scratch.ready);
    top("supported", incremental.supported == scratch.supported);
    top(
        "scrollScope",
        incremental.scroll_scope == scratch.scroll_scope,
    );
    top("machine", incremental.machine == scratch.machine);
    top(
        "spacesEnabled",
        incremental.spaces_enabled == scratch.spaces_enabled,
    );
    top("spaces", incremental.spaces == scratch.spaces);
    top(
        "botsMode",
        incremental.bots_enabled == scratch.bots_enabled
            && incremental.bots_source == scratch.bots_source
            && incremental.bots_mode == scratch.bots_mode
            && incremental.automations_row == scratch.automations_row
            && incremental.automations_today == scratch.automations_today,
    );
    top(
        "collections",
        incremental.collections == scratch.collections,
    );
    top("order", incremental.order == scratch.order);
    top("emptyState", incremental.empty_state == scratch.empty_state);
    top(
        "machineNotice",
        incremental.machine_notice == scratch.machine_notice,
    );

    for group in &incremental.groups {
        let Some(fresh) = scratch.group(&group.core.group_id) else {
            difference
                .only_incremental_groups
                .push(group.core.group_id.clone());
            continue;
        };
        let mut fields: Vec<&str> = Vec::new();
        let kept = &group.core;
        let built = &fresh.core;
        let mut note = |name: &'static str, same: bool| {
            if !same {
                fields.push(name);
            }
        };
        note("title", kept.title == built.title);
        note("titleTooltip", kept.title_tooltip == built.title_tooltip);
        note("storageId", kept.storage_id == built.storage_id);
        note("isActive", kept.is_active == built.is_active);
        note("collapsed", kept.collapsed == built.collapsed);
        note(
            "hoverActionsExpanded",
            kept.hover_actions_expanded == built.hover_actions_expanded,
        );
        note("summary", kept.summary == built.summary);
        note("sections", kept.sections == built.sections);
        note(
            "projectContext",
            kept.project_context == built.project_context,
        );
        note(
            "collectionColor",
            group.collection_color == fresh.collection_color,
        );
        note("collectionId", group.collection_id == fresh.collection_id);
        note(
            "sessionOrder",
            kept.sessions
                .iter()
                .map(|session| session.row.sidebar_session_id.as_str())
                .eq(built
                    .sessions
                    .iter()
                    .map(|session| session.row.sidebar_session_id.as_str())),
        );
        if !fields.is_empty() {
            difference.groups.push((
                kept.group_id.clone(),
                fields.into_iter().map(str::to_string).collect(),
            ));
        }
        compare_rows(
            kept.sessions.as_slice(),
            built.sessions.as_slice(),
            &mut difference,
        );
    }
    for group in &scratch.groups {
        if incremental.group(&group.core.group_id).is_none() {
            difference
                .only_scratch_groups
                .push(group.core.group_id.clone());
        }
    }
    difference.group_order_differs = difference.only_incremental_groups.is_empty()
        && difference.only_scratch_groups.is_empty()
        && incremental
            .groups
            .iter()
            .map(|group| group.core.group_id.as_str())
            .ne(scratch
                .groups
                .iter()
                .map(|group| group.core.group_id.as_str()));
    difference.bound();
    difference.differs().then_some(difference)
}

fn compare_rows(kept: &[SessionView], built: &[SessionView], difference: &mut ScratchDifference) {
    for session in kept {
        let id = session.row.sidebar_session_id.as_str();
        let Some(fresh) = built
            .iter()
            .find(|candidate| candidate.row.sidebar_session_id == id)
        else {
            difference.only_incremental_rows.push(id.to_string());
            continue;
        };
        if session == fresh {
            continue;
        }
        let row = &session.row;
        let other = &fresh.row;
        let mut fields: Vec<&str> = Vec::new();
        let mut note = |name: &'static str, same: bool| {
            if !same {
                fields.push(name);
            }
        };
        note("isFocused", session.is_focused == fresh.is_focused);
        note("isVisible", session.is_visible == fresh.is_visible);
        note(
            "isMultiSelected",
            session.is_multi_selected == fresh.is_multi_selected,
        );
        note("displayTitle", row.display_title == other.display_title);
        note("alias", row.alias == other.alias);
        note("titleTooltip", row.title_tooltip == other.title_tooltip);
        note("activity", row.activity == other.activity);
        note(
            "lifecycleState",
            row.lifecycle_state == other.lifecycle_state,
        );
        note("agentIcon", row.agent_icon == other.agent_icon);
        note("sessionKind", row.session_kind == other.session_kind);
        note(
            "faviconDataUrl",
            row.favicon_data_url == other.favicon_data_url,
        );
        note("effectiveTag", row.effective_tag == other.effective_tag);
        note("sessionTag", row.session_tag == other.session_tag);
        note(
            "tagPresentation",
            row.tag_presentation == other.tag_presentation,
        );
        note("isPinned", row.is_pinned == other.is_pinned);
        note("isParked", row.is_parked == other.is_parked);
        note("isDraft", row.is_draft == other.is_draft);
        note("isFavorite", row.is_favorite == other.is_favorite);
        note(
            "pendingQuestionCount",
            row.pending_question_count == other.pending_question_count,
        );
        note(
            "awaitingAnswer",
            row.awaiting_answer == other.awaiting_answer,
        );
        note(
            "lastInteractionAt",
            row.last_interaction_at == other.last_interaction_at,
        );
        note("timing", row.timing == other.timing);
        note("delayedSend", row.delayed_send == other.delayed_send);
        note(
            "closeAfterDone",
            row.close_after_done == other.close_after_done,
        );
        // The two are unequal and none of the named fields explains it, which is worth saying
        // plainly rather than reporting a row with no fields.
        if fields.is_empty() {
            fields.push("otherRowField");
        }
        difference.rows.push((
            id.to_string(),
            fields.into_iter().map(str::to_string).collect(),
        ));
    }
    for session in built {
        let id = session.row.sidebar_session_id.as_str();
        if !kept
            .iter()
            .any(|candidate| candidate.row.sidebar_session_id == id)
        {
            difference.only_scratch_rows.push(id.to_string());
        }
    }
}
