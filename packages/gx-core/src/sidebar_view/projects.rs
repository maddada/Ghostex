//! Which projects the list shows, in which order, and what rides on each of them: the parked
//! ones, the icon, the worktree metadata, and the manual order.
//!
//! Ported from `createGpuiPresentationProjectProjectionMetadata` in the deleted
//! `gxserver-runtime/helpers/presentation-projection.ts` (see git history).
//!
//! SEE-ALSO: packages/shared/project-worktree-order.ts (deleted 2026-10-01),
//! packages/shared/gxserver-presentation-sidebar-projection.ts (deleted 2026-10-01)
//! (`orderGxserverPresentationSidebarProjects`).

use std::collections::{BTreeMap, BTreeSet};

use ghostex_gx_protocol::PresentationProject;
use serde_json::Value;

use crate::presentation_store::MachinePresentation;

use super::text::js_trim;
use super::view::WorktreeView;
use crate::project_docs::{
    order_projects_with_worktrees as order_items_with_worktrees, ProjectOrderItem,
};

/// What the projection knows about one project beside its daemon row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectOverlay {
    pub(crate) icon_data_url: Option<String>,
    /// The Hermes profile a bot project stands for (`default` for `HERMES_HOME` itself).
    pub(crate) bot_profile: Option<String>,
    pub(crate) order_index: Option<usize>,
    pub(crate) worktree: Option<WorktreeView>,
}

/// Every project fact the list needs, rebuilt when a project, a domain row, or the manual order
/// changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectMeta {
    pub(crate) overlays: BTreeMap<String, ProjectOverlay>,
    pub(crate) hidden_project_ids: BTreeSet<String>,
    /// Visible projects in sidebar order, worktrees under their parents.
    pub(crate) project_order: Vec<String>,
    /// Visible bot projects in sidebar order, which Bots mode lists instead of `project_order`.
    pub(crate) bot_order: Vec<String>,
    /// GBots' projects (`gbotsBot`), which only Main Hub's GBots section draws (gbots.rs).
    pub(crate) gbot_order: Vec<String>,
    /// How many projects the Projects settings page would list; the empty state reads it.
    pub(crate) project_settings_count: usize,
}

impl ProjectMeta {
    pub(crate) fn overlay(&self, project_id: &str) -> Option<&ProjectOverlay> {
        self.overlays.get(project_id)
    }

    /// Every project that gets a project group, projects first, then bots, then GBots. Every kind
    /// is built in either mode; `assemble` draws the kind the mode lists, the way it applies a
    /// Space, and draws GBots under Main Hub.
    pub(crate) fn grouped_project_ids(&self) -> impl Iterator<Item = &String> {
        self.project_order
            .iter()
            .chain(&self.bot_order)
            .chain(&self.gbot_order)
    }

    pub(crate) fn is_gbot(&self, project_id: &str) -> bool {
        self.gbot_order.iter().any(|id| id == project_id)
    }

    pub(crate) fn bot_profile(&self, project_id: &str) -> Option<&str> {
        self.overlay(project_id)?.bot_profile.as_deref()
    }
}

fn string_field(value: Option<&Value>, key: &str) -> Option<String> {
    let text = value?.get(key)?.as_str()?;
    let trimmed = js_trim(text);
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn boolean_field(value: Option<&Value>, key: &str) -> Option<bool> {
    value?.get(key)?.as_bool()
}

/// `normalizeGpuiProjectPath`.
fn normalize_project_path(value: Option<&str>) -> Option<String> {
    let trimmed = js_trim(value?);
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.trim_end_matches('/').to_string())
}

/// `normalizeGpuiPathForProjectComparison`.
fn path_for_comparison(value: &str) -> String {
    let trimmed = js_trim(value);
    let stripped = trimmed.trim_end_matches('/');
    if stripped.is_empty() {
        trimmed.to_string()
    } else {
        stripped.to_string()
    }
}

/// The icon a project's sidebar row draws, as a data URL: the image set for the project, else the
/// one gxserver found in its folder. Settings > Projects' Project dropdown draws the same icon.
pub fn project_sidebar_icon_data_url(
    machine: &MachinePresentation,
    project_id: &str,
) -> Option<String> {
    machine
        .domain_project(project_id)
        .and_then(project_icon_data_url)
        .or_else(|| {
            machine
                .loaded()?
                .project(project_id)?
                .discovered_icon_data_url
                .clone()
        })
}

/// `gpuiPresentationProjectIconDataUrl`: the user's attached image, else the stored data URL. A
/// typed image icon whose data URL does not validate is no icon at all, and the legacy field is
/// read instead (`normalizeWorkspaceProjectIcon` returns nothing for it).
fn project_icon_data_url(project: &Value) -> Option<String> {
    let identity_icon = project.get("identityIcon")?;
    let icon = identity_icon.get("icon");
    if icon
        .and_then(|icon| icon.get("kind"))
        .and_then(Value::as_str)
        == Some("image")
    {
        if let Some(data_url) = icon
            .and_then(|icon| icon.get("dataUrl"))
            .and_then(Value::as_str)
            .filter(|value| is_project_icon_data_url(value))
        {
            return Some(data_url.to_string());
        }
    }
    identity_icon
        .get("iconDataUrl")
        .and_then(Value::as_str)
        .filter(|value| is_project_icon_data_url(value))
        .map(str::to_string)
}

/// `normalizeWorkspaceProjectIconDataUrl`: `^data:image\/(?:png|svg\+xml);base64,`.
fn is_project_icon_data_url(value: &str) -> bool {
    value.starts_with("data:image/png;base64,") || value.starts_with("data:image/svg+xml;base64,")
}

/// `normalizeGpuiSidebarWorktreeMetadata`: every field is required.
fn normalize_worktree(worktree: Option<&Value>) -> Option<WorktreeView> {
    Some(WorktreeView {
        branch: string_field(worktree, "branch")?,
        name: string_field(worktree, "name")?,
        parent_project_id: string_field(worktree, "parentProjectId")?,
        parent_project_name: string_field(worktree, "parentProjectName")?,
        parent_project_path: string_field(worktree, "parentProjectPath")?,
    })
}

/// One candidate parent project for `resolveGpuiProjectWorktreeParentMetadata`.
struct ParentCandidate<'a> {
    project_id: &'a str,
    name: Option<&'a str>,
    path: Option<&'a str>,
    is_worktree: bool,
}

/// `resolveGpuiProjectWorktreeParentMetadata`: point the worktree at the project that really owns
/// its parent path, when one is registered.
fn resolve_worktree_parent(
    worktree: Option<WorktreeView>,
    candidates: &[ParentCandidate<'_>],
) -> Option<WorktreeView> {
    let worktree = worktree?;
    let parent_path = path_for_comparison(&worktree.parent_project_path);
    let canonical = candidates.iter().find(|candidate| {
        if candidate.project_id == worktree.parent_project_id {
            return false;
        }
        let Some(path) = candidate.path.filter(|path| !path.is_empty()) else {
            return false;
        };
        path_for_comparison(path) == parent_path && !candidate.is_worktree
    });
    let Some(canonical) = canonical else {
        return Some(worktree);
    };
    let name = canonical
        .name
        .map(js_trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    let path = canonical
        .path
        .map(js_trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string);
    Some(WorktreeView {
        parent_project_id: canonical.project_id.to_string(),
        parent_project_name: name.unwrap_or(worktree.parent_project_name),
        parent_project_path: path.unwrap_or(worktree.parent_project_path),
        ..worktree
    })
}

/// Builds every project fact of one machine.
///
/// `parked_project_ids` are the projects the daemon keeps as Recent Projects. They are normally
/// out of the presentation already, but the list is the authoritative one, so a project that is
/// still in a snapshot while it is being parked is hidden here too.
pub(crate) fn build_project_meta(
    machine: &MachinePresentation,
    project_order: &[String],
    parked_project_ids: &BTreeSet<String>,
) -> ProjectMeta {
    let mut meta = ProjectMeta {
        hidden_project_ids: parked_project_ids.clone(),
        ..ProjectMeta::default()
    };
    let Some(loaded) = machine.loaded() else {
        return meta;
    };
    let order_index: BTreeMap<&str, usize> = project_order
        .iter()
        .enumerate()
        .map(|(index, project_id)| (project_id.as_str(), index))
        .collect();
    let domain_projects: Vec<(&String, &Value)> = machine.domain_projects.iter().collect();
    let mut candidates: Vec<ParentCandidate<'_>> = loaded
        .projects()
        .iter()
        .map(|project| ParentCandidate {
            project_id: project.project_id.as_str(),
            name: Some(project.title.as_str()),
            path: project.path.as_deref(),
            is_worktree: string_field(project.worktree.as_ref(), "parentProjectId").is_some(),
        })
        .collect();
    candidates.extend(
        domain_projects
            .iter()
            .map(|(project_id, project)| ParentCandidate {
                project_id: project_id.as_str(),
                name: project.get("name").and_then(Value::as_str),
                path: project.get("path").and_then(Value::as_str),
                is_worktree: string_field(project.get("worktree"), "parentProjectId").is_some(),
            }),
    );

    for (project_id, project) in &domain_projects {
        let worktree =
            resolve_worktree_parent(normalize_worktree(project.get("worktree")), &candidates);
        if boolean_field(Some(project), "isRecentProject") == Some(true) {
            meta.hidden_project_ids.insert((*project_id).clone());
        }
        merge_overlay(
            &mut meta.overlays,
            project_id,
            ProjectOverlayPatch {
                icon_data_url: project_icon_data_url(project),
                order_index: order_index.get(project_id.as_str()).copied(),
                worktree,
                ..ProjectOverlayPatch::default()
            },
        );
    }
    for project in loaded.projects() {
        // CDXC:GBots 2026-11-09 WHY:
        // A GBot's project is its folder, never one of the user's projects: while GBots are on it is drawn as the bot's row under Chief of Staff (gbots.rs), and while they are off (`gbotsProject` without `gbotsBot`) nothing of it shows.
        if project.gbots_project && project.gbots_bot.is_none() {
            meta.hidden_project_ids.insert(project.project_id.clone());
        }
        let index = order_index.get(project.project_id.as_str()).copied();
        let worktree =
            resolve_worktree_parent(normalize_worktree(project.worktree.as_ref()), &candidates);
        // CDXC:Bots 2026-09-26 WHY:
        // A bot is an ordinary gxserver project flagged in `launchSettings` (`isBot`, `botProfile`), so no schema migration and no `systemKind` value is needed.
        // The presentation row carries the profile (`botProfile`), which every client holds from its first frame.
        let bot_profile = project.bot_profile.clone();
        if index.is_some() || worktree.is_some() || bot_profile.is_some() {
            merge_overlay(
                &mut meta.overlays,
                &project.project_id,
                ProjectOverlayPatch {
                    order_index: index,
                    worktree,
                    bot_profile,
                    ..ProjectOverlayPatch::default()
                },
            );
        }
    }

    let visible: Vec<&PresentationProject> = loaded
        .projects()
        .iter()
        .filter(|project| {
            !meta.hidden_project_ids.contains(&project.project_id)
                && !machine.is_project_hidden(&project.project_id)
        })
        .collect();
    let (gbots, visible): (Vec<&PresentationProject>, Vec<&PresentationProject>) = visible
        .iter()
        .copied()
        .partition(|project| project.gbots_bot.is_some());
    let (bots, projects): (Vec<&PresentationProject>, Vec<&PresentationProject>) = visible
        .iter()
        .copied()
        .partition(|project| meta.bot_profile(&project.project_id).is_some());
    meta.gbot_order = super::gbots::order_gbot_projects(gbots);
    let project_order = order_sidebar_projects(projects.into_iter(), &meta);
    let bot_order = order_sidebar_projects(bots.into_iter(), &meta);
    meta.project_order = project_order;
    meta.bot_order = bot_order;
    meta.project_settings_count = project_settings_count(machine);
    meta
}

/// Merges one patch into a project's overlay, creating it only when the patch says something
/// (`mergeGpuiPresentationProjectOverlay`).
fn merge_overlay(
    overlays: &mut BTreeMap<String, ProjectOverlay>,
    project_id: &str,
    patch: ProjectOverlayPatch,
) {
    if !overlays.contains_key(project_id) && patch.is_empty() {
        return;
    }
    let overlay = overlays.entry(project_id.to_string()).or_default();
    if let Some(icon_data_url) = patch.icon_data_url {
        overlay.icon_data_url = Some(icon_data_url);
    }
    if let Some(bot_profile) = patch.bot_profile {
        overlay.bot_profile = Some(bot_profile);
    }
    if let Some(order_index) = patch.order_index {
        overlay.order_index = Some(order_index);
    }
    if let Some(worktree) = patch.worktree {
        overlay.worktree = Some(worktree);
    }
}

#[derive(Default)]
struct ProjectOverlayPatch {
    icon_data_url: Option<String>,
    bot_profile: Option<String>,
    order_index: Option<usize>,
    worktree: Option<WorktreeView>,
}

impl ProjectOverlayPatch {
    fn is_empty(&self) -> bool {
        self.icon_data_url.is_none()
            && self.bot_profile.is_none()
            && self.order_index.is_none()
            && self.worktree.is_none()
    }
}

/// `orderGxserverPresentationSidebarProjects`: the manual order when there is one, else the
/// daemon's sort key, then worktrees under their parent projects.
///
/// CDXC:StateSync 2026-09-20 SEE-ALSO:
/// packages/shared/gxserver-presentation-sidebar-projection.ts (deleted 2026-10-01) compares these keys with `localeCompare`, which in the desktop's QuickJS was NFC normalization plus a code-point comparison, so the byte order used here is the same order for every string a daemon sends today. It is NOT the same in V8, whose `localeCompare` collates through ICU, so the web build and any other V8 consumer of this crate (M9) needs the difference decided on purpose rather than rediscovered; a string that is not in NFC already differs even on the desktop.
fn order_sidebar_projects<'a>(
    projects: impl Iterator<Item = &'a PresentationProject>,
    meta: &ProjectMeta,
) -> Vec<String> {
    let mut ordered: Vec<&PresentationProject> = projects.collect();
    ordered.sort_by(|left, right| {
        let left_index = meta.overlay(&left.project_id).and_then(|o| o.order_index);
        let right_index = meta.overlay(&right.project_id).and_then(|o| o.order_index);
        if left_index.is_some() || right_index.is_some() {
            return left_index
                .unwrap_or(usize::MAX)
                .cmp(&right_index.unwrap_or(usize::MAX));
        }
        left.sort_key
            .cmp(&right.sort_key)
            .then_with(|| {
                right
                    .updated_at
                    .as_deref()
                    .unwrap_or("")
                    .cmp(left.updated_at.as_deref().unwrap_or(""))
            })
            .then_with(|| left.project_id.cmp(&right.project_id))
    });
    let ids: Vec<String> = ordered
        .iter()
        .map(|project| project.project_id.clone())
        .collect();
    order_projects_with_worktrees(&ids, meta)
}

/// `orderProjectsWithWorktrees` over ids, built into the items the shared rule takes.
///
/// CDXC:Worktrees 2026-09-21 WHY:
/// The rule itself is `crate::project_docs::order_projects_with_worktrees`, shared with the project
/// drag. It used to be written twice, once here over ids and once (from M5 piece 7d) over the
/// items a drag needs, and the two must agree exactly: the list nests a worktree under its parent
/// and a drop has to land it in the same place. One row per project here, so `order_id` is the
/// project id.
fn order_projects_with_worktrees(ids: &[String], meta: &ProjectMeta) -> Vec<String> {
    let items: Vec<ProjectOrderItem> = ids
        .iter()
        .map(|project_id| {
            let overlay = meta.overlay(project_id);
            ProjectOrderItem {
                order_id: project_id.clone(),
                project_id: project_id.clone(),
                parent_project_id: overlay
                    .and_then(|overlay| overlay.worktree.as_ref())
                    .map(|worktree| worktree.parent_project_id.clone()),
            }
        })
        .collect();
    order_items_with_worktrees(&items)
        .into_iter()
        .map(|item| item.project_id)
        .collect()
}

/// `createGpuiProjectSettingsProjects(...).length`.
fn project_settings_count(machine: &MachinePresentation) -> usize {
    if !machine.domain_projects.is_empty() {
        return machine
            .domain_projects
            .values()
            .filter(|project| {
                normalize_project_path(project.get("path").and_then(Value::as_str)).is_some()
                    && boolean_field(Some(project), "isRecentProject") != Some(true)
            })
            .count();
    }
    machine
        .loaded()
        .map(|loaded| {
            loaded
                .projects()
                .iter()
                .filter(|project| normalize_project_path(project.path.as_deref()).is_some())
                .count()
        })
        .unwrap_or_default()
}
