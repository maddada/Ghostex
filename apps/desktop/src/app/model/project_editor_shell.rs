// C1 wave-3 re-cluster: the project-editor shell model, lifecycle, auto-sleep policy, project view-state, and their shell-state persistence, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectEditorLifecycleState {
    Awake,
    Sleeping,
}


impl ProjectEditorLifecycleState {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "awake" => Some(Self::Awake),
            "sleeping" => Some(Self::Sleeping),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Awake => "awake",
            Self::Sleeping => "sleeping",
        }
    }
}


#[derive(Clone, Copy)]
pub(crate) struct ProjectEditorModeLifecycle {
    pub(crate) state: ProjectEditorLifecycleState,
    pub(crate) recency: u64,
}


#[derive(Default)]
pub(crate) struct ProjectEditorAutoSleepEpochs {
    pub(crate) source: u64,
    pub(crate) browser: u64,
    pub(crate) kanban: u64,
    pub(crate) automate: u64,
    pub(crate) manage: u64,
}


impl ProjectEditorAutoSleepEpochs {
    pub(crate) fn epoch(&self, mode: TitlebarMode) -> Option<u64> {
        match mode {
            TitlebarMode::Source => Some(self.source),
            TitlebarMode::Browser => Some(self.browser),
            TitlebarMode::Kanban => Some(self.kanban),
            TitlebarMode::Automate => Some(self.automate),
            TitlebarMode::Manage => Some(self.manage),
            TitlebarMode::Agents => None,
        }
    }

    pub(crate) fn bump(&mut self, mode: TitlebarMode) -> Option<u64> {
        let epoch = match mode {
            TitlebarMode::Source => &mut self.source,
            TitlebarMode::Browser => &mut self.browser,
            TitlebarMode::Kanban => &mut self.kanban,
            TitlebarMode::Automate => &mut self.automate,
            TitlebarMode::Manage => &mut self.manage,
            TitlebarMode::Agents => return None,
        };
        *epoch = epoch.wrapping_add(1);
        Some(*epoch)
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectEditorAutoSleepPolicySnapshot {
    pub(crate) source: Option<Duration>,
    pub(crate) browser: Option<Duration>,
    pub(crate) kanban: Option<Duration>,
    pub(crate) automate: Option<Duration>,
    pub(crate) manage: Option<Duration>,
}


impl ProjectEditorAutoSleepPolicySnapshot {
    pub(crate) fn read_current() -> Self {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        Self::from_shared_settings(&settings)
    }

    pub(crate) fn from_shared_settings(settings: &shared_settings::SharedSidebarSettingsSnapshot) -> Self {
        Self {
            source: project_editor_auto_sleep_duration(TitlebarMode::Source, settings),
            browser: project_editor_auto_sleep_duration(TitlebarMode::Browser, settings),
            kanban: project_editor_auto_sleep_duration(TitlebarMode::Kanban, settings),
            automate: project_editor_auto_sleep_duration(TitlebarMode::Automate, settings),
            manage: project_editor_auto_sleep_duration(TitlebarMode::Manage, settings),
        }
    }

    pub(crate) fn duration_for_mode(self, mode: TitlebarMode) -> Option<Duration> {
        match mode {
            TitlebarMode::Source => self.source,
            TitlebarMode::Browser => self.browser,
            TitlebarMode::Kanban => self.kanban,
            TitlebarMode::Automate => self.automate,
            TitlebarMode::Manage => self.manage,
            TitlebarMode::Agents => None,
        }
    }
}


/*
CDXC:GPUIProjectViewMemory 2026-08-07:
The workarea a project was last shown in — and how that project's companion
pane was arranged — is project-owned state, exactly like its Agents split
topology. Keyed by the same canonical workspace project key, so a remote
project's view memory is machine-scoped and never collides with a same-named
local project. Companion slot occupants are the project's own shell session
ids, the identical vocabulary the parked workspace model already stores, so
they are meaningful only alongside that model and are validated against it on
restore.
*/
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct GpuiProjectViewState {
    pub(crate) active_mode: TitlebarMode,
    pub(crate) companion_visible: bool,
    pub(crate) companion_split_enabled: bool,
    pub(crate) companion_width_ratio: f32,
    pub(crate) companion_split_ratio: f32,
    pub(crate) companion_top_session_id: Option<TerminalSessionId>,
    pub(crate) companion_bottom_session_id: Option<TerminalSessionId>,
    pub(crate) companion_focused_slot: ProjectEditorCompanionTerminalSlot,
}


pub(crate) struct ProjectEditorShellModel {
    pub(crate) left_companion_visible: bool,
    pub(crate) left_companion_width_ratio: f32,
    pub(crate) left_companion_split_enabled: bool,
    pub(crate) left_companion_split_ratio: f32,
    pub(crate) source_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) browser_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) kanban_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) automate_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) manage_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) next_lifecycle_recency: u64,
}


impl ProjectEditorShellModel {
    pub(crate) fn shell_default() -> Self {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-05:49:
        Source, Browser, Kanban, Automate, and Docs are project-editor workspace modes in the GPUI parity shell. They replace the normal Agents workspace while active, keep command-pane wrapping outside the editor area, and reserve an in-memory left companion pane with a real divider region while project routing stays deferred.

        CDXC:GPUIProjectEditor 2026-06-22-06:24:
        The project-editor companion should start near the macOS default ratio intent instead of a small fixed pixel width. Store the shell default as a 32% editor-area ratio and apply practical companion/editor minimums at render and resize time.

        CDXC:GPUIProjectEditorLifecycle 2026-06-22-08:29:
        Source, Browser, Kanban, Automate, and Docs need independent shell-level awake/sleeping state while their real surfaces remain runtime-owned. Persist only enum-like lifecycle values and recency counters; runtime auto-sleep epochs live on the GPUI app so timer tokens never enter shell state and no source content, paths, raw page titles, command text, tokens, or secrets are stored.

        CDXC:GPUIManageLifecycle 2026-06-23-14:08:
        Docs sleep/wake must preserve the selected project/workarea runtime identity while hiding or restoring only shell-owned surface state. Sleeping, waking, and load-failed Docs states must not clear or synthesize CEF/file-bridge readiness, perform file I/O, persist project facts, or create fallback surfaces.

        CDXC:GPUIManageLifecycle 2026-06-23-14:48:
        Docs sleep/wake must preserve companion layout and command-pane shell state at the same shell boundary as Source and Kanban. Lifecycle toggles may not synthesize readiness, mount CEF or file bridges, reset shell-owned layout, persist private project/workarea facts, or create fallback surfaces.

        CDXC:GPUIAutomateWorkarea 2026-07-04-23:18:
        Automate participates in project-editor shell lifecycle, focus, persistence, companion layout, and the direct workarea CEF slot. Shell lifecycle may wake or sleep the mode only; it must not synthesize surface ids, issue fallback URLs, persist private page facts, or mount hidden CEF views outside the active workarea gate.

        CDXC:GPUIKanbanLifecycle 2026-06-24-08:09:
        Kanban sleep/wake/lifecycle must preserve the explicit project/board runtime identity plus separate CEF bridge state, including load-failed, without becoming runtime CEF instantiation, runtime URL issuance, hidden mounts, placeholder replacement, fallback probes, logging/persistence/private payloads, or WKWebView/WebKit/non-CEF paths.

        CDXC:GPUIKanbanLifecycle 2026-06-28-17:09:
        Kanban runtime CEF creation is owned by the direct runtime URL/CefSurface gate, not by shell lifecycle. Do not widen readiness or sidebar bridge routing into URL/path/CEF payloads, hidden mounts, placeholder replacement, fallback probes, logging, persistence, private payloads, or WKWebView/WebKit paths.

        CDXC:GPUIProjectEditor 2026-06-22-08:15:
        The optional project-editor companion has explicit shell-owned hide and restore controls before real Source, Browser, Kanban, Automate, and Docs companion content exists. Hiding only toggles companion visibility and focus; it preserves the stored width ratio plus Browser tab/surface identity, placeholder editor identity, command-pane state, and terminal placeholder state.
        */
        Self {
            left_companion_visible: true,
            left_companion_width_ratio: PROJECT_EDITOR_COMPANION_WIDTH_RATIO,
            left_companion_split_enabled: true,
            left_companion_split_ratio: PROJECT_EDITOR_COMPANION_SPLIT_RATIO,
            source_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            browser_lifecycle: ProjectEditorModeLifecycle {
                state: ProjectEditorLifecycleState::Awake,
                recency: 1,
            },
            kanban_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            automate_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            manage_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            next_lifecycle_recency: 2,
        }
    }

    pub(crate) fn lifecycle(&self, mode: TitlebarMode) -> Option<ProjectEditorModeLifecycle> {
        match mode {
            TitlebarMode::Source => Some(self.source_lifecycle),
            TitlebarMode::Browser => Some(self.browser_lifecycle),
            TitlebarMode::Kanban => Some(self.kanban_lifecycle),
            TitlebarMode::Automate => Some(self.automate_lifecycle),
            TitlebarMode::Manage => Some(self.manage_lifecycle),
            TitlebarMode::Agents => None,
        }
    }

    pub(crate) fn lifecycle_mut(&mut self, mode: TitlebarMode) -> Option<&mut ProjectEditorModeLifecycle> {
        match mode {
            TitlebarMode::Source => Some(&mut self.source_lifecycle),
            TitlebarMode::Browser => Some(&mut self.browser_lifecycle),
            TitlebarMode::Kanban => Some(&mut self.kanban_lifecycle),
            TitlebarMode::Automate => Some(&mut self.automate_lifecycle),
            TitlebarMode::Manage => Some(&mut self.manage_lifecycle),
            TitlebarMode::Agents => None,
        }
    }

    pub(crate) fn is_mode_awake(&self, mode: TitlebarMode) -> bool {
        self.lifecycle(mode)
            .is_some_and(|lifecycle| lifecycle.state == ProjectEditorLifecycleState::Awake)
    }

    pub(crate) fn mark_mode_awake(&mut self, mode: TitlebarMode) -> bool {
        if !mode.is_project_editor_mode() {
            return false;
        }

        let recency = self.next_lifecycle_recency.max(1);
        self.next_lifecycle_recency = recency.saturating_add(1);
        let Some(lifecycle) = self.lifecycle_mut(mode) else {
            return false;
        };
        lifecycle.state = ProjectEditorLifecycleState::Awake;
        lifecycle.recency = recency;
        self.enforce_awake_mode_cap(mode);
        true
    }

    pub(crate) fn enforce_awake_mode_cap(&mut self, active_mode: TitlebarMode) {
        let mut awake_modes = project_editor_modes()
            .iter()
            .filter_map(|mode| {
                let lifecycle = self.lifecycle(*mode)?;
                if lifecycle.state == ProjectEditorLifecycleState::Awake {
                    Some((*mode, lifecycle.recency))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if awake_modes.len() <= PROJECT_EDITOR_AWAKE_MODE_CAP {
            return;
        }

        awake_modes.sort_by_key(|(mode, recency)| (*recency, mode.project_editor_order()));
        let mut modes_to_sleep = awake_modes.len() - PROJECT_EDITOR_AWAKE_MODE_CAP;
        for (mode, _) in awake_modes {
            if modes_to_sleep == 0 {
                break;
            }
            if mode == active_mode {
                continue;
            }
            if let Some(lifecycle) = self.lifecycle_mut(mode) {
                lifecycle.state = ProjectEditorLifecycleState::Sleeping;
                modes_to_sleep -= 1;
            }
        }
    }

    pub(crate) fn mark_mode_sleeping(&mut self, mode: TitlebarMode) -> bool {
        let Some(lifecycle) = self.lifecycle_mut(mode) else {
            return false;
        };
        if lifecycle.state == ProjectEditorLifecycleState::Sleeping {
            return false;
        }

        lifecycle.state = ProjectEditorLifecycleState::Sleeping;
        true
    }

    pub(crate) fn set_left_companion_width_ratio(&mut self, ratio: f32, content_span: f32) -> bool {
        let next_ratio = project_editor_companion_width_ratio_for_span(ratio, content_span);
        if (self.left_companion_width_ratio - next_ratio).abs() < 0.001 {
            return false;
        }

        self.left_companion_width_ratio = next_ratio;
        true
    }

    pub(crate) fn reset_left_companion_width_ratio(&mut self, content_span: Option<f32>) -> bool {
        let next_ratio = content_span
            .map(|content_span| {
                project_editor_companion_width_ratio_for_span(
                    PROJECT_EDITOR_COMPANION_WIDTH_RATIO,
                    content_span,
                )
            })
            .unwrap_or(PROJECT_EDITOR_COMPANION_WIDTH_RATIO);
        if (self.left_companion_width_ratio - next_ratio).abs() < 0.001 {
            return false;
        }

        self.left_companion_width_ratio = next_ratio;
        true
    }

    pub(crate) fn set_left_companion_split_ratio(&mut self, ratio: f32, content_span: f32) -> bool {
        let Some((minimum, maximum)) = split_drag_ratio_bounds_from_minimums(
            PANE_RESIZE_MINIMUM_HEIGHT,
            PANE_RESIZE_MINIMUM_HEIGHT,
            content_span,
        ) else {
            return false;
        };
        let next_ratio = ratio.clamp(minimum, maximum);
        if (self.left_companion_split_ratio - next_ratio).abs() < 0.001 {
            return false;
        }

        self.left_companion_split_ratio = next_ratio;
        true
    }

    pub(crate) fn reset_left_companion_split_ratio(&mut self, content_span: Option<f32>) -> bool {
        let next_ratio = content_span
            .and_then(|content_span| {
                split_drag_ratio_bounds_from_minimums(
                    PANE_RESIZE_MINIMUM_HEIGHT,
                    PANE_RESIZE_MINIMUM_HEIGHT,
                    content_span,
                )
                .map(|(minimum, maximum)| {
                    PROJECT_EDITOR_COMPANION_SPLIT_RATIO.clamp(minimum, maximum)
                })
            })
            .unwrap_or(PROJECT_EDITOR_COMPANION_SPLIT_RATIO);
        if (self.left_companion_split_ratio - next_ratio).abs() < 0.001 {
            return false;
        }

        self.left_companion_split_ratio = next_ratio;
        true
    }

    pub(crate) fn hide_left_companion(&mut self) -> bool {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-14:42:
        Companion close/hide needs a pure model transition for regression coverage: it may only hide the companion pane and must preserve stored width, mode lifecycle/recency, Browser tab identity, command-pane state, and terminal placeholder state for later restore.
        */
        if !self.left_companion_visible {
            return false;
        }

        self.left_companion_visible = false;
        true
    }

    pub(crate) fn restore_left_companion(&mut self) -> bool {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-14:42:
        Companion restore reuses the current shell-owned width and lifecycle state instead of recreating placeholder surfaces or resetting the project-editor layout.
        */
        if self.left_companion_visible {
            return false;
        }

        self.left_companion_visible = true;
        true
    }
}


impl ProjectEditorModeLifecycle {
    pub(crate) fn sleeping() -> Self {
        Self {
            state: ProjectEditorLifecycleState::Sleeping,
            recency: 0,
        }
    }
}


pub(crate) fn project_editor_modes() -> [TitlebarMode; 5] {
    [
        TitlebarMode::Source,
        TitlebarMode::Browser,
        TitlebarMode::Kanban,
        TitlebarMode::Automate,
        TitlebarMode::Manage,
    ]
}


pub(crate) fn project_view_state_to_shell_state_json(state: &GpuiProjectViewState) -> serde_json::Value {
    serde_json::json!({
        "activeMode": state.active_mode.element_slug(),
        "companionVisible": state.companion_visible,
        "companionSplitEnabled": state.companion_split_enabled,
        "companionWidthRatio": json_number_f32(project_editor_companion_width_ratio(
            state.companion_width_ratio,
        )),
        "companionSplitRatio": json_number_f32(state.companion_split_ratio.clamp(0.1, 0.9)),
        "companionTopSessionId": state.companion_top_session_id.map(|session_id| session_id.0),
        "companionBottomSessionId": state
            .companion_bottom_session_id
            .map(|session_id| session_id.0),
        "companionFocusedSlot": match state.companion_focused_slot {
            ProjectEditorCompanionTerminalSlot::Top => "top",
            ProjectEditorCompanionTerminalSlot::Bottom => "bottom",
        },
    })
}


pub(crate) fn project_view_state_from_shell_state(value: &serde_json::Value) -> Option<GpuiProjectViewState> {
    let object = value.as_object()?;
    let active_mode = object
        .get("activeMode")
        .and_then(serde_json::Value::as_str)
        .and_then(TitlebarMode::from_slug)?;
    Some(GpuiProjectViewState {
        active_mode,
        companion_visible: json_bool_field(object, "companionVisible").unwrap_or(true),
        companion_split_enabled: json_bool_field(object, "companionSplitEnabled").unwrap_or(false),
        companion_width_ratio: json_f32_field(object, "companionWidthRatio")
            .map(project_editor_companion_width_ratio)
            .unwrap_or(PROJECT_EDITOR_COMPANION_WIDTH_RATIO),
        companion_split_ratio: json_f32_field(object, "companionSplitRatio")
            .map(|ratio| ratio.clamp(0.1, 0.9))
            .unwrap_or(PROJECT_EDITOR_COMPANION_SPLIT_RATIO),
        companion_top_session_id: json_u64_field(object, "companionTopSessionId")
            .map(TerminalSessionId),
        companion_bottom_session_id: json_u64_field(object, "companionBottomSessionId")
            .map(TerminalSessionId),
        companion_focused_slot: match object
            .get("companionFocusedSlot")
            .and_then(serde_json::Value::as_str)
        {
            Some("bottom") => ProjectEditorCompanionTerminalSlot::Bottom,
            _ => ProjectEditorCompanionTerminalSlot::Top,
        },
    })
}


pub(crate) fn project_editor_shell_to_shell_state_json(model: &ProjectEditorShellModel) -> serde_json::Value {
    serde_json::json!({
        "leftCompanionVisible": model.left_companion_visible,
        "leftCompanionWidthRatio": json_number_f32(project_editor_companion_width_ratio(
            model.left_companion_width_ratio,
        )),
        "leftCompanionSplitEnabled": model.left_companion_split_enabled,
        "leftCompanionSplitRatio": json_number_f32(
            model.left_companion_split_ratio.clamp(0.1, 0.9),
        ),
        "modeLifecycle": project_editor_lifecycle_to_shell_state_json(model),
        "nextLifecycleRecency": model.next_lifecycle_recency,
    })
}


pub(crate) fn project_editor_shell_from_shell_state(
    value: &serde_json::Value,
    active_mode: TitlebarMode,
) -> Option<ProjectEditorShellModel> {
    let object = value.as_object()?;
    let mut model = ProjectEditorShellModel {
        left_companion_visible: json_bool_field(object, "leftCompanionVisible").unwrap_or(true),
        left_companion_width_ratio: json_f32_field(object, "leftCompanionWidthRatio")
            .map(project_editor_companion_width_ratio)
            .unwrap_or(PROJECT_EDITOR_COMPANION_WIDTH_RATIO),
        left_companion_split_enabled: json_bool_field(object, "leftCompanionSplitEnabled")
            .unwrap_or(true),
        left_companion_split_ratio: json_f32_field(object, "leftCompanionSplitRatio")
            .map(|ratio| ratio.clamp(0.1, 0.9))
            .unwrap_or(PROJECT_EDITOR_COMPANION_SPLIT_RATIO),
        ..ProjectEditorShellModel::shell_default()
    };

    if let Some(entries) = object
        .get("modeLifecycle")
        .and_then(project_editor_lifecycle_from_shell_state)
    {
        for (mode, lifecycle) in entries {
            if let Some(target) = model.lifecycle_mut(mode) {
                *target = lifecycle;
            }
        }
    }

    let max_recency = project_editor_modes()
        .iter()
        .filter_map(|mode| model.lifecycle(*mode).map(|lifecycle| lifecycle.recency))
        .max()
        .unwrap_or(0);
    model.next_lifecycle_recency = json_u64_field(object, "nextLifecycleRecency")
        .unwrap_or(model.next_lifecycle_recency)
        .max(max_recency.saturating_add(1))
        .max(1);
    model.enforce_awake_mode_cap(active_mode);
    Some(model)
}


pub(crate) fn project_editor_lifecycle_to_shell_state_json(
    model: &ProjectEditorShellModel,
) -> serde_json::Value {
    serde_json::Value::Array(
        project_editor_modes()
            .iter()
            .filter_map(|mode| {
                let lifecycle = model.lifecycle(*mode)?;
                Some(serde_json::json!({
                    "mode": mode.element_slug(),
                    "state": lifecycle.state.element_slug(),
                    "recency": lifecycle.recency,
                }))
            })
            .collect(),
    )
}


pub(crate) fn project_editor_lifecycle_from_shell_state(
    value: &serde_json::Value,
) -> Option<Vec<(TitlebarMode, ProjectEditorModeLifecycle)>> {
    Some(
        value
            .as_array()?
            .iter()
            .filter_map(|entry| {
                let object = entry.as_object()?;
                let mode = json_string_field(object, "mode").and_then(TitlebarMode::from_slug)?;
                if !mode.is_project_editor_mode() {
                    return None;
                }
                let state = json_string_field(object, "state")
                    .and_then(ProjectEditorLifecycleState::from_slug)
                    .unwrap_or(ProjectEditorLifecycleState::Sleeping);
                Some((
                    mode,
                    ProjectEditorModeLifecycle {
                        state,
                        recency: json_u64_field(object, "recency").unwrap_or(0),
                    },
                ))
            })
            .collect(),
    )
}


pub(crate) fn project_editor_companion_width_ratio(ratio: f32) -> f32 {
    ratio.clamp(0.10, 0.85)
}


pub(crate) fn project_editor_companion_width_ratio_for_span(ratio: f32, content_span: f32) -> f32 {
    let content_span = content_span.max(1.0);
    let companion_min_ratio = (PROJECT_EDITOR_COMPANION_MIN_WIDTH / content_span).clamp(0.10, 0.85);
    let editor_max_ratio = ((content_span - WORKSPACE_MIN_WIDTH) / content_span).clamp(0.10, 0.85);
    let ratio = project_editor_companion_width_ratio(ratio);

    if companion_min_ratio <= editor_max_ratio {
        ratio.clamp(companion_min_ratio, editor_max_ratio)
    } else {
        companion_min_ratio
    }
}
