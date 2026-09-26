//! The menu bar status item and the pet overlay: which sessions count as working, waiting for the
//! user or available, per project, and what the pet shows.
//!
//! The rules are the old app runtime's (apps/desktop/sidebar/gxserver-runtime/helpers/status-indicators.ts),
//! over the same rows: every machine's sidebar groups (this computer's, then each saved remote
//! machine's, live or last seen), unfiltered by Space, tag or collapse, each in its display order.
//! The host keeps one [`SidebarViewModel`] per machine with neutral list state for that
//! (`neutral_indicator_inputs`) and hands over each one's `built_groups`, which include the Chats
//! group the drawn list leaves out.
//!
//! CDXC:StatusPet 2026-06-26-04:38:
//! GPUI status indicators and the pet overlay consume the same saved shared Settings object as the
//! sidebar. Publish only bounded counts, booleans, pet id, and sidebar-projected project/session
//! ids/titles. Project icon parity may carry only an already-normalized image data URL for
//! notification attachments.
//!
//! CDXC:StatusPet 2026-06-27-20:11:
//! The standalone GPUI floating session indicator was removed. Keep producing status counts and
//! projects for the menu bar and the pet badge, but do not include floating visibility or floating
//! size settings in the status payload.
//!
//! CDXC:StatusPet 2026-09-25 WHY:
//! The app runtime port (family F2) moves the derivation here, fed from the store, so the status
//! item and the pet no longer wait on the runtime's projection.

use serde_json::{Map, Value};

use crate::core::Core;
use crate::keys::{MachineId, ProjectKey};
use crate::sidebar_view::{SessionRow, SidebarInputs, SidebarViewModel};

/// `GPUI_STATUS_INDICATOR_MAX_CANDIDATES`.
const MAX_CANDIDATES: usize = 96;
/// `GPUI_STATUS_INDICATOR_MAX_PROJECTS`.
const MAX_PROJECTS: usize = 32;
/// `GPUI_STATUS_INDICATOR_MAX_SESSIONS_PER_PROJECT`.
const MAX_SESSIONS_PER_PROJECT: usize = 16;
/// `GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS`.
const TITLE_MAX_CHARS: usize = 120;

pub const SESSION_STATUS_INDICATORS_MESSAGE_TYPE: &str =
    "ghostex.gpui.sidebar.sessionStatusIndicators";
pub const PET_OVERLAY_STATE_MESSAGE_TYPE: &str = "ghostex.gpui.sidebar.petOverlayState";

/// `PET_IDS` (packages/shared/pets.ts); the first is `DEFAULT_PET_ID`.
const PET_IDS: [&str; 9] = [
    "boo",
    "codex",
    "dewey",
    "fireball",
    "rocky",
    "seedy",
    "stacky",
    "bsod",
    "null-signal",
];

/// One row the status item may count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndicatorCandidate {
    pub has_running_zmx_backing: bool,
    pub icon_data_url: Option<String>,
    pub last_interaction_at: Option<String>,
    pub order: usize,
    pub project_id: String,
    pub project_title: String,
    pub session_id: String,
    pub status: &'static str,
    pub title: String,
}

/// The list state the indicators read a machine's rows through: that machine selected, and no
/// Space, tag filter, collapse or hide applied, because the runtime counted its groups before any
/// of those.
pub fn neutral_indicator_inputs(list_inputs: &SidebarInputs, machine: &MachineId) -> SidebarInputs {
    let mut inputs = list_inputs.clone();
    inputs.ui = crate::sidebar_view::SidebarUiState {
        selected_machine_id: machine
            .remote_id()
            .unwrap_or(crate::sidebar_view::LOCAL_MACHINE_ID)
            .to_string(),
        ..Default::default()
    };
    inputs.settings.sidebar_spaces_enabled = false;
    inputs
}

/// `boundedGpuiStatusIndicatorTitle`: trimmed-empty reads as the fallback, then at most 120
/// UTF-16 units.
fn bounded_title(value: Option<&str>, fallback: &str) -> String {
    let value = value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback);
    let mut out = String::new();
    let mut units = 0;
    for character in value.chars() {
        units += character.len_utf16();
        if units > TITLE_MAX_CHARS {
            break;
        }
        out.push(character);
    }
    out
}

fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// `getGpuiPetOverlaySessionTitle`. The last resort is the session's number, its place in the
/// group before the display sort.
fn session_title(row: &SessionRow, session_number: usize) -> String {
    let number = session_number.to_string();
    let facts = &row.menu_facts;
    let title = non_blank(facts.raw_display_title.as_deref())
        .or_else(|| non_blank(facts.primary_title.as_deref()))
        .or_else(|| non_blank(facts.terminal_title.as_deref()))
        .or_else(|| non_blank(Some(row.alias.as_str())))
        .or(Some(number.as_str()));
    bounded_title(title, "Untitled session")
}

/// `hasRunningZmxBackingForGpuiIdleIndicator`: a terminal row on zmx whose provider session exists.
fn has_running_zmx_backing(core: &Core, row: &SessionRow) -> bool {
    if row.session_kind.as_deref() != Some("terminal") {
        return false;
    }
    let facts = &row.menu_facts;
    if facts.session_persistence_provider.as_deref() != Some("zmx")
        || non_blank(facts.session_persistence_name.as_deref()).is_none()
    {
        return false;
    }
    let Some(key) = &row.key else {
        return false;
    };
    let Some(session) = core
        .presentation()
        .machine(&key.machine)
        .and_then(|machine| machine.effective_session(&key.project_id, &key.session_id))
    else {
        return false;
    };
    let published = session.provider_session_state.as_str();
    let state = if !published.is_empty() {
        published
    } else {
        match session.lifecycle_state.as_str() {
            "running" => "exists",
            _ => "missing",
        }
    };
    state == "exists"
}

/// `createGpuiSessionStatusIndicatorCandidatesFromSidebarGroups`, over each machine's groups in
/// the order the runtime built them.
pub fn indicator_candidates(
    core: &Core,
    models: &[(MachineId, &SidebarViewModel)],
) -> Vec<IndicatorCandidate> {
    let mut candidates = Vec::new();
    for (machine, model) in models {
        for (core_group, unsorted) in model.built_groups() {
            if candidates.len() >= MAX_CANDIDATES {
                return candidates;
            }
            let group_project = core_group.project_context.as_ref().map(|context| {
                ProjectKey {
                    machine: machine.clone(),
                    project_id: context.project_id.clone(),
                }
                .to_workspace_project_id()
            });
            let icon_data_url = core_group
                .project_context
                .as_ref()
                .and_then(|context| context.icon_data_url.clone())
                .filter(|url| {
                    url.starts_with("data:image/png;base64,")
                        || url.starts_with("data:image/svg+xml;base64,")
                });
            for session in &core_group.sessions {
                if candidates.len() >= MAX_CANDIDATES {
                    return candidates;
                }
                let row = &session.row;
                let session_number = unsorted
                    .iter()
                    .position(|candidate| candidate.sidebar_session_id == row.sidebar_session_id)
                    .unwrap_or_default()
                    + 1;
                // `parseGxserverPresentationProjectSessionId` reads only this computer's ids, so a
                // remote row outside a project group counts nowhere, as there.
                let row_project = row
                    .key
                    .as_ref()
                    .filter(|key| key.machine.is_local() && !row.is_browser)
                    .map(|key| key.project_id.clone());
                let Some(project_id) = group_project.clone().or(row_project) else {
                    continue;
                };
                let project_title = bounded_title(Some(core_group.title.as_str()), &project_id);
                candidates.push(IndicatorCandidate {
                    has_running_zmx_backing: has_running_zmx_backing(core, row),
                    icon_data_url: icon_data_url.clone(),
                    last_interaction_at: row.last_interaction_at.clone(),
                    order: candidates.len(),
                    project_id,
                    project_title,
                    session_id: row.sidebar_session_id.clone(),
                    status: match row.activity.as_str() {
                        "attention" => "attention",
                        "working" => "working",
                        _ => "available",
                    },
                    title: session_title(row, session_number),
                });
            }
        }
    }
    candidates
}

/// `shouldCountGpuiSessionStatusIndicatorCandidate`.
fn counts(candidate: &IndicatorCandidate) -> bool {
    candidate.status != "available" || candidate.has_running_zmx_backing
}

fn status_counts(candidates: &[IndicatorCandidate]) -> (u64, u64, u64) {
    let (mut attention, mut available, mut working) = (0, 0, 0);
    for candidate in candidates.iter().filter(|candidate| counts(candidate)) {
        match candidate.status {
            "attention" => attention += 1,
            "working" => working += 1,
            _ => available += 1,
        }
    }
    (attention, available, working)
}

/// `createGpuiSessionStatusIndicatorsPayload`.
pub fn status_indicators_payload(candidates: &[IndicatorCandidate], hide_menu_bar: bool) -> Value {
    let (attention, available, working) = status_counts(candidates);
    let mut projects: Vec<Map<String, Value>> = Vec::new();
    let mut project_ids: Vec<&str> = Vec::new();
    for candidate in candidates.iter().filter(|candidate| counts(candidate)) {
        let index = match project_ids
            .iter()
            .position(|id| *id == candidate.project_id)
        {
            Some(index) => index,
            None => {
                if projects.len() >= MAX_PROJECTS {
                    continue;
                }
                let mut project = Map::new();
                if let Some(icon) = &candidate.icon_data_url {
                    project.insert("iconDataUrl".into(), Value::from(icon.as_str()));
                }
                project.insert(
                    "projectId".into(),
                    Value::from(candidate.project_id.as_str()),
                );
                project.insert("sessions".into(), Value::Array(Vec::new()));
                project.insert(
                    "title".into(),
                    Value::from(candidate.project_title.as_str()),
                );
                projects.push(project);
                project_ids.push(candidate.project_id.as_str());
                projects.len() - 1
            }
        };
        let Some(Value::Array(sessions)) = projects[index].get_mut("sessions") else {
            continue;
        };
        if sessions.len() >= MAX_SESSIONS_PER_PROJECT {
            continue;
        }
        let mut session = Map::new();
        if let Some(last) = &candidate.last_interaction_at {
            session.insert("lastActiveAt".into(), Value::from(last.as_str()));
        }
        session.insert(
            "sessionId".into(),
            Value::from(candidate.session_id.as_str()),
        );
        session.insert("sidebarOrder".into(), Value::from(candidate.order));
        session.insert("status".into(), Value::from(candidate.status));
        session.insert("title".into(), Value::from(candidate.title.as_str()));
        sessions.push(Value::Object(session));
    }
    let mut payload = Map::new();
    payload.insert("attentionCount".into(), Value::from(attention));
    payload.insert("availableCount".into(), Value::from(available));
    payload.insert("hideMenuBarIndicators".into(), Value::Bool(hide_menu_bar));
    payload.insert(
        "projects".into(),
        Value::Array(projects.into_iter().map(Value::Object).collect()),
    );
    payload.insert(
        "type".into(),
        Value::from(SESSION_STATUS_INDICATORS_MESSAGE_TYPE),
    );
    payload.insert("version".into(), Value::from(1));
    payload.insert("workingCount".into(), Value::from(working));
    Value::Object(payload)
}

/// `getGpuiIndicatorTimestamp`: 0 when there is none or it does not parse.
fn timestamp(value: Option<&str>) -> i64 {
    value
        .and_then(crate::sidebar_view::text::parse_iso_ms)
        .unwrap_or(0)
}

/// `createGpuiPetOverlayStatePayload`.
pub fn pet_overlay_payload(
    candidates: &[IndicatorCandidate],
    enabled: bool,
    selected_pet_id: Option<&str>,
) -> Value {
    let actionable: Vec<&IndicatorCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.status == "attention" || candidate.status == "working")
        .collect();
    let shown: Vec<&IndicatorCandidate> = if actionable.is_empty() {
        // `compareGpuiSessionStatusIndicatorCandidates`: newest interaction first, then order.
        let mut all: Vec<&IndicatorCandidate> = candidates.iter().collect();
        all.sort_by(|left, right| {
            timestamp(right.last_interaction_at.as_deref())
                .cmp(&timestamp(left.last_interaction_at.as_deref()))
                .then(left.order.cmp(&right.order))
        });
        all.into_iter().take(2).collect()
    } else {
        // `compareGpuiPetOverlayActivityCandidates`: attention before working, then order.
        let priority = |status: &str| match status {
            "attention" => 2,
            "working" => 1,
            _ => 0,
        };
        let mut shown = actionable;
        shown.sort_by(|left, right| {
            priority(right.status)
                .cmp(&priority(left.status))
                .then(left.order.cmp(&right.order))
        });
        shown.into_iter().take(3).collect()
    };
    let (attention, available, working) = status_counts(candidates);
    let mut status_items = Vec::new();
    let item = |count: u64, status: &str| {
        let mut item = Map::new();
        item.insert("count".into(), Value::from(count));
        item.insert("status".into(), Value::from(status));
        Value::Object(item)
    };
    if attention > 0 || working > 0 {
        if attention > 0 {
            status_items.push(item(attention, "attention"));
        }
        if working > 0 {
            status_items.push(item(working, "working"));
        }
    } else if available > 0 {
        status_items.push(item(available, "available"));
    }
    let pet_id = selected_pet_id
        .filter(|id| PET_IDS.contains(id))
        .unwrap_or(PET_IDS[0]);
    let mut payload = Map::new();
    payload.insert(
        "activities".into(),
        Value::Array(
            shown
                .into_iter()
                .map(|candidate| {
                    let mut activity = Map::new();
                    activity.insert("id".into(), Value::from(candidate.session_id.as_str()));
                    activity.insert(
                        "projectId".into(),
                        Value::from(candidate.project_id.as_str()),
                    );
                    activity.insert("state".into(), Value::from(candidate.status));
                    activity.insert("title".into(), Value::from(candidate.title.as_str()));
                    Value::Object(activity)
                })
                .collect(),
        ),
    );
    payload.insert("enabled".into(), Value::Bool(enabled));
    payload.insert("selectedPetId".into(), Value::from(pet_id));
    payload.insert("statusItems".into(), Value::Array(status_items));
    payload.insert("type".into(), Value::from(PET_OVERLAY_STATE_MESSAGE_TYPE));
    payload.insert("version".into(), Value::from(1));
    Value::Object(payload)
}
