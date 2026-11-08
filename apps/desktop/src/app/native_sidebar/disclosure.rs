use super::model::NativeSidebarSnapshot;
use crate::GhostexGpuiApp;
use crate::fold_motion::{FoldMotion, fold_duration, render_fold};
use gpui::AnyElement;
use std::collections::HashMap;

const UNIFIED_PREFIX: &str = "unified:";

/// The disclosure key of a Unified sidebar project group.
pub(crate) fn unified_key(repo_key: &str) -> String {
    format!("{UNIFIED_PREFIX}{repo_key}")
}

#[derive(Default)]
pub(crate) struct SidebarDisclosures {
    items: HashMap<String, Disclosure>,
}
struct Disclosure {
    motion: FoldMotion,
    session_ids: Vec<String>,
}
impl SidebarDisclosures {
    pub(crate) fn sync(&mut self, snapshot: &NativeSidebarSnapshot, reduce_motion: bool) {
        let duration = fold_duration(
            snapshot.hud["settings"]["sidebarCollapseAnimationDurationMs"]
                .as_u64()
                .unwrap_or_default(),
            reduce_motion,
        );
        // CDXC:Sidebar 2026-09-20 WHY:
        // Keyed rather than a list scanned per held item: with the sidebar list built in Rust this
        // runs on every change the list draws, a focus step during a held hotkey included, and the
        // scan was one comparison per drawn heading per held heading.
        let mut next: HashMap<String, (bool, Vec<String>)> = HashMap::new();
        for group in &snapshot.groups {
            next.insert(
                format!("group:{}", group.group_id),
                (group.collapsed, Vec::new()),
            );
            for section in &group.sections {
                next.insert(
                    format!("section:{}:{}", group.group_id, section.id),
                    (section.collapsed, section.session_ids.clone()),
                );
            }
        }
        // Main Hub's GBots heading folds every bot (native_sidebar/gbots_rows.rs).
        for section in &snapshot.gbots {
            next.insert(
                format!("group:{}", section.storage_id),
                (section.collapsed, Vec::new()),
            );
        }
        for collection in &snapshot.collections {
            next.insert(
                format!("collection:{}", collection.collection_id),
                (collection.collapsed, Vec::new()),
            );
        }
        // The Unified sidebar's groups are tracked by `track_unified`, from its own snapshot.
        self.items
            .retain(|key, _| key.starts_with(UNIFIED_PREFIX) || next.contains_key(key));
        for (key, (collapsed, ids)) in next {
            if let Some(item) = self.items.get_mut(&key) {
                item.motion.fold_to(collapsed, duration);
                if !collapsed {
                    item.session_ids = ids;
                }
            } else {
                self.items.insert(
                    key,
                    Disclosure {
                        motion: FoldMotion::settled(collapsed),
                        session_ids: ids,
                    },
                );
            }
        }
    }
    /// The Unified sidebar's project groups (Group by Project), keyed by repository: a new group
    /// rests as it is drawn, so its height is measured before its first fold; a group whose fold
    /// changed folds to it; a group gone is forgotten.
    ///
    /// CDXC:Sidebar 2026-11-06 DECISION:
    /// User: the Unified sidebar's project groups animate open and shut when their header is clicked, instead of snapping, like the Project sidebar's projects. They fold through the same `FoldMotion` (fold_motion.rs): Collapse animation speed (`sidebarCollapseAnimationDurationMs`) on the same curve, the content clipped and faded while it moves, a click mid-flight reversing from where it is, and no motion under Reduce Motion.
    pub(crate) fn track_unified(
        &mut self,
        groups: impl IntoIterator<Item = (String, bool)>,
        duration: std::time::Duration,
    ) {
        let groups: HashMap<String, bool> = groups
            .into_iter()
            .map(|(repo_key, folded)| (unified_key(&repo_key), folded))
            .collect();
        self.items
            .retain(|key, _| !key.starts_with(UNIFIED_PREFIX) || groups.contains_key(key));
        for (key, folded) in groups {
            self.fold(key, folded, duration);
        }
    }

    /// Folds one keyed item toward `collapsed`, starting it resting the other way when it is new.
    pub(crate) fn fold(&mut self, key: String, collapsed: bool, duration: std::time::Duration) {
        match self.items.get_mut(&key) {
            Some(item) => item.motion.fold_to(collapsed, duration),
            None => {
                self.items.insert(
                    key,
                    Disclosure {
                        motion: FoldMotion::settled(collapsed),
                        session_ids: Vec::new(),
                    },
                );
            }
        }
    }

    pub(crate) fn present(&self, key: &str, collapsed: bool) -> bool {
        !collapsed
            || self
                .items
                .get(key)
                .is_some_and(|item| item.motion.animating())
    }
    pub(crate) fn section_ids<'a>(
        &'a self,
        key: &str,
        current: &'a [String],
        collapsed: bool,
    ) -> &'a [String] {
        if collapsed {
            self.items
                .get(key)
                .map(|item| item.session_ids.as_slice())
                .unwrap_or(current)
        } else {
            current
        }
    }
}
impl GhostexGpuiApp {
    /// The sidebar's fold duration: Collapse animation speed, or none under Reduce Motion.
    pub(crate) fn native_sidebar_fold_duration(&self) -> std::time::Duration {
        fold_duration(
            self.native_sidebar
                .snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot.hud["settings"]["sidebarCollapseAnimationDurationMs"].as_u64()
                })
                .unwrap_or_default(),
            self.gpui_reduce_motion_enabled,
        )
    }

    /// Keeps the Unified groups' folds in step with the snapshot just installed and the saved
    /// folds (`SidebarDisclosures::track_unified`).
    pub(crate) fn track_native_unified_folds(&mut self) {
        let Some(unified) = self.native_sidebar.unified.clone() else {
            return;
        };
        let duration = self.native_sidebar_fold_duration();
        let folded = &self.gx_store.unified_sidebar.saved.folded_groups;
        let groups: Vec<(String, bool)> = unified
            .groups
            .iter()
            .map(|group| (group.repo_key.clone(), folded.contains(&group.repo_key)))
            .collect();
        self.native_sidebar
            .disclosures
            .track_unified(groups, duration);
    }

    pub(crate) fn render_native_disclosure(
        &self,
        key: String,
        content: AnyElement,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let motion = self
            .native_sidebar
            .disclosures
            .items
            .get(&key)
            .map(|item| &item.motion);
        render_fold(
            cx.entity().clone(),
            motion,
            move |app: &mut GhostexGpuiApp| {
                app.native_sidebar
                    .disclosures
                    .items
                    .get_mut(&key)
                    .map(|item| &mut item.motion)
            },
            content,
        )
    }
}
