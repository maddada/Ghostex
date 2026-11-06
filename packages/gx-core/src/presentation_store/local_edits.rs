//! Local-first edits: hides and optimistic patches. None of them touches the revision.

use std::borrow::Cow;

use ghostex_gx_protocol::{LifecycleState, PresentationSession};

use super::store::{MachinePresentation, PresentationStore};
use crate::change::{ChangeSummary, IgnoredReason};
use crate::keys::{ProjectKey, SessionKey};
use crate::overlay::{PatchVerdict, SessionPatch, StoredPatch};

impl PresentationStore {
    /// Hides a session locally (a local-first close). Returns whether it was visible before.
    pub fn hide_session(&mut self, key: &SessionKey) -> ChangeSummary {
        let Some(entry) = self.machines.get_mut(&key.machine) else {
            return ChangeSummary::ignored(IgnoredReason::UnknownTarget);
        };
        let was_visible = entry
            .effective_session(&key.project_id, &key.session_id)
            .is_some();
        entry
            .overlays
            .hidden_sessions
            .insert(&key.project_id, &key.session_id, ());
        let mut summary = ChangeSummary::default();
        if was_visible {
            summary.note_session_removed(key.clone());
            summary.note_session_order_changed(key.project_key());
        }
        summary
    }

    /// Takes a local hide back (the close request failed).
    pub fn unhide_session(&mut self, key: &SessionKey) -> ChangeSummary {
        let Some(entry) = self.machines.get_mut(&key.machine) else {
            return ChangeSummary::ignored(IgnoredReason::UnknownTarget);
        };
        let mut summary = ChangeSummary::default();
        if entry
            .overlays
            .hidden_sessions
            .remove(&key.project_id, &key.session_id)
            .is_some()
            && entry
                .effective_session(&key.project_id, &key.session_id)
                .is_some()
        {
            summary.note_session_changed(key.clone());
            summary.note_session_order_changed(key.project_key());
        }
        summary
    }

    /// Hides a project locally (close to recent) until the daemon's removal arrives.
    pub fn hide_project(&mut self, key: &ProjectKey) -> ChangeSummary {
        let Some(entry) = self.machines.get_mut(&key.machine) else {
            return ChangeSummary::ignored(IgnoredReason::UnknownTarget);
        };
        let mut summary = ChangeSummary::default();
        let exists = entry
            .loaded()
            .is_some_and(|loaded| loaded.project(&key.project_id).is_some());
        if entry
            .overlays
            .hidden_projects
            .insert(key.project_id.clone())
            && exists
        {
            summary.note_project_removed(key.clone());
            summary.note_project_order_changed(key.machine.clone());
        }
        summary
    }

    pub fn unhide_project(&mut self, key: &ProjectKey) -> ChangeSummary {
        let Some(entry) = self.machines.get_mut(&key.machine) else {
            return ChangeSummary::ignored(IgnoredReason::UnknownTarget);
        };
        let mut summary = ChangeSummary::default();
        let exists = entry
            .loaded()
            .is_some_and(|loaded| loaded.project(&key.project_id).is_some());
        if entry.overlays.hidden_projects.remove(&key.project_id) && exists {
            summary.note_project_changed(key.clone());
            summary.note_project_order_changed(key.machine.clone());
        }
        summary
    }

    /// `reorderPresentationProjectSessions`: the local-first half of a manual session reorder.
    ///
    /// CDXC:Sessions 2026-09-21 WHY:
    /// gxserver owns the durable `sidebarOrder`, but `/api/updateSessionOrder` is awaited and the
    /// row has to move under the user's finger, so the client writes the same value into its own
    /// copy first. It is a direct edit of the daemon rows and NOT a [`SessionPatch`] overlay, for
    /// the reason the TypeScript writes it the same way: an overlay would expire (60 s, declared
    /// difference 24) and the order would silently snap back to the daemon's old one if the row
    /// were never re-sent, which is the oscillation this whole family exists to prevent. A row the
    /// daemon does send again replaces it whole, overlays included, which is the correction.
    ///
    /// The saved rows start at 1000 so a session created later, with sidebar order 0, leads the
    /// manual list (`CDXC:Sessions 2026-06-05-12:30`).
    pub fn reorder_project_sessions(
        &mut self,
        project: &ProjectKey,
        ordered_session_ids: &[String],
    ) -> ChangeSummary {
        let Some(entry) = self.machines.get_mut(&project.machine) else {
            return ChangeSummary::ignored(IgnoredReason::UnknownTarget);
        };
        let Some(loaded) = entry.loaded_mut() else {
            return ChangeSummary::ignored(IgnoredReason::UnknownTarget);
        };
        let mut touched_groups: Vec<String> = Vec::new();
        let mut changed = false;
        for (index, session_id) in ordered_session_ids.iter().enumerate() {
            let sidebar_order = ((index + 1) * 1_000) as f64;
            let Some(session) = loaded.session_mut(&project.project_id, session_id) else {
                continue;
            };
            if session.sidebar_order == Some(sidebar_order) {
                continue;
            }
            session.sidebar_order = Some(sidebar_order);
            session.sort_key = sort_key_with_sidebar_order(session, sidebar_order);
            changed = true;
            if !touched_groups.contains(&session.group_id) {
                touched_groups.push(session.group_id.clone());
            }
        }
        if !changed {
            return ChangeSummary::default();
        }
        for group_id in touched_groups {
            loaded.rebuild_group_session_ids(&project.project_id, &group_id);
        }
        let mut summary = ChangeSummary::default();
        summary.note_session_order_changed(project.clone());
        summary
    }

    /// The local-first half of a Unified sidebar pinned reorder: each session takes the
    /// `sidebarOrder` the drop gave it (the pin call carries the same value to its machine's
    /// gxserver), on whichever machine it is. A direct edit of the daemon rows, like
    /// [`Self::reorder_project_sessions`], so it is corrected by the next row the daemon sends.
    pub fn set_session_sidebar_orders(&mut self, orders: &[(SessionKey, i64)]) -> ChangeSummary {
        let mut summary = ChangeSummary::default();
        let mut touched: Vec<(ProjectKey, String)> = Vec::new();
        for (key, order) in orders {
            let Some(loaded) = self
                .machines
                .get_mut(&key.machine)
                .and_then(MachinePresentation::loaded_mut)
            else {
                continue;
            };
            let Some(session) = loaded.session_mut(&key.project_id, &key.session_id) else {
                continue;
            };
            let sidebar_order = *order as f64;
            if session.sidebar_order == Some(sidebar_order) {
                continue;
            }
            session.sidebar_order = Some(sidebar_order);
            session.sort_key = sort_key_with_sidebar_order(session, sidebar_order);
            // The row carries the order (`SessionRow::sidebar_order`), so it is built again.
            summary.note_session_changed(key.clone());
            let group = (key.project_key(), session.group_id.clone());
            if !touched.contains(&group) {
                touched.push(group);
            }
        }
        for (project, group_id) in touched {
            if let Some(loaded) = self
                .machines
                .get_mut(&project.machine)
                .and_then(MachinePresentation::loaded_mut)
            {
                loaded.rebuild_group_session_ids(&project.project_id, &group_id);
            }
            summary.note_session_order_changed(project);
        }
        summary
    }

    /// Overlays an optimistic patch on a session. The revision is untouched.
    ///
    /// Refused when the store does not hold the session (there is nothing to overlay). A patch the
    /// daemon row already agrees with is not stored: it would have nothing to do, and would later
    /// look like a pending overlay to a newer daemon value.
    pub fn patch_session(&mut self, key: &SessionKey, patch: SessionPatch) -> ChangeSummary {
        let Some(entry) = self.machines.get_mut(&key.machine) else {
            return ChangeSummary::ignored(IgnoredReason::UnknownTarget);
        };
        let Some(server) = entry
            .loaded()
            .and_then(|loaded| loaded.server_session(&key.project_id, &key.session_id))
        else {
            return ChangeSummary::ignored(IgnoredReason::UnknownTarget);
        };
        let before = entry
            .effective_session(&key.project_id, &key.session_id)
            .map(Cow::into_owned);
        let stored = StoredPatch::new(patch, server);
        if stored.verdict(server) == PatchVerdict::Pending {
            entry
                .overlays
                .session_patches
                .insert(&key.project_id, &key.session_id, stored);
        } else {
            entry
                .overlays
                .session_patches
                .remove(&key.project_id, &key.session_id);
        }
        Self::session_change_since(entry, key, before)
    }

    /// Drops a session's optimistic patch (the request it anticipated failed).
    pub fn clear_session_patch(&mut self, key: &SessionKey) -> ChangeSummary {
        let Some(entry) = self.machines.get_mut(&key.machine) else {
            return ChangeSummary::ignored(IgnoredReason::UnknownTarget);
        };
        let before = entry
            .effective_session(&key.project_id, &key.session_id)
            .map(Cow::into_owned);
        entry
            .overlays
            .session_patches
            .remove(&key.project_id, &key.session_id);
        Self::session_change_since(entry, key, before)
    }

    fn session_change_since(
        entry: &MachinePresentation,
        key: &SessionKey,
        before: Option<PresentationSession>,
    ) -> ChangeSummary {
        let after = entry.effective_session(&key.project_id, &key.session_id);
        let mut summary = ChangeSummary::default();
        if before.as_ref() != after.as_deref() {
            summary.note_session_changed(key.clone());
        }
        summary
    }

    /// Drops every patch whose expiry has passed.
    pub fn expire_patches(&mut self, now_ms: u64) -> ChangeSummary {
        let mut summary = ChangeSummary::default();
        for (machine, entry) in &mut self.machines {
            let expired: Vec<(String, String)> = entry
                .overlays
                .session_patches
                .iter()
                .filter(|(_, _, stored)| stored.patch.expires_at_ms <= now_ms)
                .map(|(project_id, session_id, _)| (project_id.to_string(), session_id.to_string()))
                .collect();
            for (project_id, session_id) in expired {
                entry
                    .overlays
                    .session_patches
                    .remove(&project_id, &session_id);
                summary.note_session_changed(SessionKey {
                    machine: machine.clone(),
                    project_id,
                    session_id,
                });
            }
        }
        summary
    }
}

/// `createPresentationSessionSortKeyWithSidebarOrder`. The manual order comes FIRST, before the
/// active and pin ranks, which is what lets a saved Manual Sorting order be exact; the ranks after
/// it are the same ones the daemon's own key carries, so a row the daemon re-sends lands in the
/// same place.
fn sort_key_with_sidebar_order(session: &PresentationSession, sidebar_order: f64) -> String {
    let active_rank = match session.lifecycle_state {
        LifecycleState::Running | LifecycleState::Sleeping => '0',
        _ => '1',
    };
    let pin_rank = if session.is_pinned {
        '0'
    } else if session.is_parked {
        '3'
    } else if session.session_tag.as_deref() == Some("favorite") || session.is_favorite {
        '1'
    } else {
        '2'
    };
    let timestamp = session
        .last_active_at
        .as_deref()
        .unwrap_or(session.updated_at.as_str());
    // `String(sidebarOrder).padStart(12, '0')`: JavaScript prints an integral Number without a
    // decimal point, and every value written here is `(index + 1) * 1000`, so the text is the
    // integer's.
    format!(
        "{:0>12}:{active_rank}:{pin_rank}:{timestamp}:{}",
        sidebar_order as i64, session.session_id
    )
}
