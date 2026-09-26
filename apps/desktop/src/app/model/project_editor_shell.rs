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
            TitlebarMode::Agents | TitlebarMode::Terminal | TitlebarMode::Extension(_) => None,
        }
    }

    pub(crate) fn bump(&mut self, mode: TitlebarMode) -> Option<u64> {
        let epoch = match mode {
            TitlebarMode::Source => &mut self.source,
            TitlebarMode::Browser => &mut self.browser,
            TitlebarMode::Kanban => &mut self.kanban,
            TitlebarMode::Automate => &mut self.automate,
            TitlebarMode::Manage => &mut self.manage,
            TitlebarMode::Agents | TitlebarMode::Terminal | TitlebarMode::Extension(_) => {
                return None;
            }
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

    pub(crate) fn from_shared_settings(
        settings: &shared_settings::SharedSidebarSettingsSnapshot,
    ) -> Self {
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
            TitlebarMode::Agents | TitlebarMode::Terminal | TitlebarMode::Extension(_) => None,
        }
    }
}

/*
CDXC:Navigation 2026-08-07:
The workarea a project was last shown in — and how wide its Agents column is
beside that view — is project-owned state, exactly like its Agents split
topology. Keyed by the same canonical workspace project key, so a remote
project's view memory is machine-scoped and never collides with a same-named
local project.
*/
#[derive(Clone, PartialEq)]
pub(crate) struct GpuiProjectViewState {
    pub(crate) active_mode: TitlebarMode,
    /// CDXC:Workarea 2026-09-20 WHY:
    /// The tab strip in the order the user left it. It is project-owned for the same reason the
    /// active view is: two projects keep different sets of tabs open, and a project switch must put
    /// back exactly the strip that project had. `active_mode` is always one of these, or `Agents`
    /// when the list is empty and the panel is closed.
    pub(crate) open_views: Vec<TitlebarMode>,
    pub(crate) view_strip_layout: GpuiViewStripLayout,
    /// The last view this project had open, kept even while the panel is closed so reopening it
    /// comes back to the same view.
    pub(crate) last_view_mode: Option<TitlebarMode>,
    pub(crate) workarea_split_ratio: f32,
    /// CDXC:Workarea 2026-09-26 DECISION:
    /// User: going between Spaces must not sleep the side panel view; "it should stay active", just the currently active view. A view's awake flag is app-wide, so while another project is on screen the idle timer and the awake cap would count this project's active view as a hidden tab and sleep it. A project left with its active view awake keeps that view out of both (`view_modes_kept_awake_by_parked_projects`) until the user is back or sleeps it on purpose (Sleep Space clears this). Supersedes the 2026-09-25 rule that let it sleep and woke it on the way back in. Runtime only, never persisted: after a launch every view starts asleep (CDXC:Browser 2026-09-19).
    pub(crate) active_view_awake: bool,
}

pub(crate) struct ProjectEditorShellModel {
    /// CDXC:Workarea 2026-09-20 WHY:
    /// The share of the workarea the Agents column keeps while a view is open. It is the live value
    /// for the active project only; every project keeps its own in `GpuiProjectViewState`, which is
    /// captured on the way out of a project and restored on the way in.
    pub(crate) workarea_split_ratio: f32,
    pub(crate) source_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) browser_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) kanban_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) automate_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) manage_lifecycle: ProjectEditorModeLifecycle,
    pub(crate) next_lifecycle_recency: u64,
    extension_lifecycles: HashMap<ExtensionId, ProjectEditorModeLifecycle>,
}

impl ProjectEditorShellModel {
    pub(crate) fn shell_default() -> Self {
        /*
        CDXC:CodeEditor 2026-06-22-05:49:
        Source, Browser, Kanban, Automate, and Docs are project-editor workspace modes in the GPUI parity shell. They replace the normal Agents workspace while active, keep command-pane wrapping outside the editor area, and reserve an in-memory left companion pane with a real divider region while project routing stays deferred.

        CDXC:CodeEditor 2026-06-22-06:24:
        The project-editor companion should start near the macOS default ratio intent instead of a small fixed pixel width. Store the shell default as a 32% editor-area ratio and apply practical companion/editor minimums at render and resize time.

        CDXC:CodeEditor 2026-06-22-08:29:
        Source, Browser, Kanban, Automate, and Docs need independent shell-level awake/sleeping state while their real surfaces remain runtime-owned. Persist only enum-like lifecycle values and recency counters; runtime auto-sleep epochs live on the GPUI app so timer tokens never enter shell state and no source content, paths, raw page titles, command text, tokens, or secrets are stored.

        CDXC:Docs 2026-06-23-14:08:
        Docs sleep/wake must preserve the selected project/workarea runtime identity while hiding or restoring only shell-owned surface state. Sleeping, waking, and load-failed Docs states must not clear or synthesize CEF/file-bridge readiness, perform file I/O, persist project facts, or create fallback surfaces.

        CDXC:Docs 2026-06-23-14:48:
        Docs sleep/wake must preserve companion layout and command-pane shell state at the same shell boundary as Source and Kanban. Lifecycle toggles may not synthesize readiness, mount CEF or file bridges, reset shell-owned layout, persist private project/workarea facts, or create fallback surfaces.

        CDXC:Automations 2026-07-04-23:18:
        Automate participates in project-editor shell lifecycle, focus, persistence, companion layout, and the direct workarea CEF slot. Shell lifecycle may wake or sleep the mode only; it must not synthesize surface ids, issue fallback URLs, persist private page facts, or mount hidden CEF views outside the active workarea gate.

        CDXC:ProjectBoard 2026-06-24-08:09:
        Kanban sleep/wake/lifecycle must preserve the explicit project/board runtime identity plus separate CEF bridge state, including load-failed, without becoming runtime CEF instantiation, runtime URL issuance, hidden mounts, placeholder replacement, fallback probes, logging/persistence/private payloads, or WKWebView/WebKit/non-CEF paths.

        CDXC:ProjectBoard 2026-06-28-17:09:
        Kanban runtime CEF creation is owned by the direct runtime URL/CefSurface gate, not by shell lifecycle. Do not widen readiness or sidebar bridge routing into URL/path/CEF payloads, hidden mounts, placeholder replacement, fallback probes, logging, persistence, private payloads, or WKWebView/WebKit paths.

        CDXC:CodeEditor 2026-06-22-08:15:
        The optional project-editor companion has explicit shell-owned hide and restore controls before real Source, Browser, Kanban, Automate, and Docs companion content exists. Hiding only toggles companion visibility and focus; it preserves the stored width ratio plus Browser tab/surface identity, placeholder editor identity, command-pane state, and terminal placeholder state.
        */
        Self {
            workarea_split_ratio: WORKAREA_SPLIT_DEFAULT_RATIO,
            source_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            browser_lifecycle: ProjectEditorModeLifecycle {
                state: ProjectEditorLifecycleState::Awake,
                recency: 1,
            },
            kanban_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            automate_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            manage_lifecycle: ProjectEditorModeLifecycle::sleeping(),
            next_lifecycle_recency: 2,
            extension_lifecycles: HashMap::new(),
        }
    }

    pub(crate) fn lifecycle(&self, mode: TitlebarMode) -> Option<ProjectEditorModeLifecycle> {
        match mode {
            TitlebarMode::Source => Some(self.source_lifecycle),
            TitlebarMode::Browser => Some(self.browser_lifecycle),
            TitlebarMode::Kanban => Some(self.kanban_lifecycle),
            TitlebarMode::Automate => Some(self.automate_lifecycle),
            TitlebarMode::Manage => Some(self.manage_lifecycle),
            TitlebarMode::Extension(id) => {
                Some(self.extension_lifecycles.get(&id).copied().unwrap_or(
                    ProjectEditorModeLifecycle {
                        state: ProjectEditorLifecycleState::Awake,
                        recency: u64::MAX,
                    },
                ))
            }
            TitlebarMode::Agents | TitlebarMode::Terminal => None,
        }
    }

    pub(crate) fn lifecycle_mut(
        &mut self,
        mode: TitlebarMode,
    ) -> Option<&mut ProjectEditorModeLifecycle> {
        match mode {
            TitlebarMode::Source => Some(&mut self.source_lifecycle),
            TitlebarMode::Browser => Some(&mut self.browser_lifecycle),
            TitlebarMode::Kanban => Some(&mut self.kanban_lifecycle),
            TitlebarMode::Automate => Some(&mut self.automate_lifecycle),
            TitlebarMode::Manage => Some(&mut self.manage_lifecycle),
            TitlebarMode::Extension(id) => Some(self.extension_lifecycles.entry(id).or_insert(
                ProjectEditorModeLifecycle {
                    state: ProjectEditorLifecycleState::Awake,
                    recency: u64::MAX,
                },
            )),
            TitlebarMode::Agents | TitlebarMode::Terminal => None,
        }
    }

    /// Every mode this model holds a lifecycle record for: the five built-ins plus whatever
    /// extension views have been woken in this session.
    pub(crate) fn lifecycle_modes(&self) -> Vec<TitlebarMode> {
        let mut modes = project_editor_modes().to_vec();
        modes.extend(
            self.extension_lifecycles
                .keys()
                .copied()
                .map(TitlebarMode::Extension),
        );
        modes
    }

    pub(crate) fn is_mode_awake(&self, mode: TitlebarMode) -> bool {
        self.lifecycle(mode)
            .is_some_and(|lifecycle| lifecycle.state == ProjectEditorLifecycleState::Awake)
    }

    /// `kept_awake` are the views the cap may not put to sleep besides `mode` itself.
    pub(crate) fn mark_mode_awake(
        &mut self,
        mode: TitlebarMode,
        kept_awake: &[TitlebarMode],
    ) -> bool {
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
        self.enforce_awake_mode_cap(mode, kept_awake);
        true
    }

    /// CDXC:Workarea 2026-09-20 WHY:
    /// The cap used to look at the five built-in modes only, which was enough while exactly one view
    /// could be on screen. A tab strip can hold extension views too, and an extension page is the
    /// same live CEF child view as Kanban's, so the cap counts them: without this six tabs really
    /// would mean six renderer processes.
    pub(crate) fn enforce_awake_mode_cap(
        &mut self,
        active_mode: TitlebarMode,
        kept_awake: &[TitlebarMode],
    ) {
        let mut awake_modes = self
            .lifecycle_modes()
            .into_iter()
            .filter_map(|mode| {
                let lifecycle = self.lifecycle(mode)?;
                if lifecycle.state == ProjectEditorLifecycleState::Awake {
                    Some((mode, lifecycle.recency))
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
            if mode == active_mode || kept_awake.contains(&mode) {
                continue;
            }
            if let Some(lifecycle) = self.lifecycle_mut(mode) {
                lifecycle.state = ProjectEditorLifecycleState::Sleeping;
                modes_to_sleep -= 1;
            }
        }
    }

    /// CDXC:Browser 2026-09-19 DECISION:
    /// User: after the app loads, web panes sleep until clicked, even when a browser tab or view was open last time, so launch never pays for a web page nobody asked for yet.
    /// Only the awake flag changes: recency, tabs, and companion layout are kept, and the normal sleeping placeholder wakes the view on click. A restored custom view is included because an unknown extension id otherwise reads as awake.
    pub(crate) fn sleep_all_modes_for_launch(&mut self, active_mode: TitlebarMode) {
        for mode in project_editor_modes() {
            self.mark_mode_sleeping(mode);
        }
        if matches!(active_mode, TitlebarMode::Extension(_)) {
            self.mark_mode_sleeping(active_mode);
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

    /// Move the divider between the Agents column and the open view, clamped so neither side goes
    /// under its own minimum for the current workarea width.
    pub(crate) fn set_workarea_split_ratio(&mut self, ratio: f32, content_span: f32) -> bool {
        let next_ratio = workarea_split_ratio_for_span(ratio, content_span);
        if (self.workarea_split_ratio - next_ratio).abs() < 0.001 {
            return false;
        }

        self.workarea_split_ratio = next_ratio;
        true
    }

    /// Double-clicking the divider puts it back at the default share. Without a known span the plain
    /// clamp applies: the span-aware one would read a zero span as an unsatisfiable minimum.
    pub(crate) fn reset_workarea_split_ratio(&mut self, content_span: Option<f32>) -> bool {
        let next_ratio = content_span
            .filter(|content_span| *content_span > 1.0)
            .map(|content_span| {
                workarea_split_ratio_for_span(WORKAREA_SPLIT_DEFAULT_RATIO, content_span)
            })
            .unwrap_or(WORKAREA_SPLIT_DEFAULT_RATIO);
        if (self.workarea_split_ratio - next_ratio).abs() < 0.001 {
            return false;
        }

        self.workarea_split_ratio = next_ratio;
        true
    }

    /// Put back a ratio remembered for a project, clamped for the current span when one is known.
    pub(crate) fn restore_workarea_split_ratio(
        &mut self,
        ratio: f32,
        content_span: Option<f32>,
    ) -> bool {
        let next_ratio = match content_span {
            Some(content_span) if content_span > 1.0 => {
                workarea_split_ratio_for_span(ratio, content_span)
            }
            _ => workarea_split_ratio(ratio),
        };
        if (self.workarea_split_ratio - next_ratio).abs() < 0.001 {
            return false;
        }

        self.workarea_split_ratio = next_ratio;
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

pub(crate) fn project_view_state_to_shell_state_json(
    state: &GpuiProjectViewState,
) -> serde_json::Value {
    serde_json::json!({
        "activeMode": state.active_mode.element_slug(),
        "openViews": state
            .open_views
            .iter()
            .map(|mode| serde_json::Value::String(mode.element_slug()))
            .collect::<Vec<_>>(),
        "viewStrip": state.view_strip_layout.to_shell_state_json(),
        "lastViewMode": state.last_view_mode.map(TitlebarMode::element_slug),
        "workareaSplitRatio": json_number_f32(workarea_split_ratio(state.workarea_split_ratio)),
    })
}

/// CDXC:Workarea 2026-09-20 DECISION:
/// User: "keep saved widths" - a user who had already sized the split keeps that number, and
/// `WORKAREA_SPLIT_DEFAULT_RATIO` applies only where nothing is saved. The companion's
/// `companionWidthRatio` (per project) and `leftCompanionWidthRatio` (app-wide) measured the same
/// thing this ratio does, the left column's share of the workarea, at the same scope and with the
/// same 0.10..0.85 clamp, so each old key is read back in place of its own successor and nothing is
/// rescaled. This supersedes the phase 3 decision to drop the old keys and start everyone at 0.44.
fn workarea_split_ratio_from_shell_state(
    object: &serde_json::Map<String, serde_json::Value>,
    legacy_key: &str,
) -> f32 {
    json_f32_field(object, "workareaSplitRatio")
        .or_else(|| json_f32_field(object, legacy_key))
        .map(workarea_split_ratio)
        .unwrap_or(WORKAREA_SPLIT_DEFAULT_RATIO)
}

pub(crate) fn project_view_state_from_shell_state(
    value: &serde_json::Value,
) -> Option<GpuiProjectViewState> {
    let object = value.as_object()?;
    let active_mode = object
        .get("activeMode")
        .and_then(serde_json::Value::as_str)
        .and_then(TitlebarMode::from_slug)?;
    Some(GpuiProjectViewState {
        active_mode,
        open_views: open_view_modes_from_shell_state(object.get("openViews"), active_mode),
        view_strip_layout: GpuiViewStripLayout::from_shell_state(object.get("viewStrip")),
        last_view_mode: object
            .get("lastViewMode")
            .and_then(serde_json::Value::as_str)
            .and_then(TitlebarMode::from_slug)
            .filter(|mode| *mode != TitlebarMode::Agents),
        workarea_split_ratio: workarea_split_ratio_from_shell_state(object, "companionWidthRatio"),
        active_view_awake: false,
    })
}

/// CDXC:Workarea 2026-09-20 WHY:
/// A state written before the tab strip existed holds one active view and no list, so it reads back
/// as a strip of exactly that view: the user sees the tab they already had, not an empty panel.
/// `Agents` never enters the list; it is the name for "the panel is closed".
pub(crate) fn open_view_modes_from_shell_state(
    value: Option<&serde_json::Value>,
    active_mode: TitlebarMode,
) -> Vec<TitlebarMode> {
    let Some(entries) = value.and_then(serde_json::Value::as_array) else {
        return Vec::from_iter((active_mode != TitlebarMode::Agents).then_some(active_mode));
    };
    let mut modes = Vec::new();
    for entry in entries {
        let Some(mode) = entry.as_str().and_then(TitlebarMode::from_slug) else {
            continue;
        };
        if mode == TitlebarMode::Agents || modes.contains(&mode) {
            continue;
        }
        modes.push(mode);
    }
    if active_mode != TitlebarMode::Agents && !modes.contains(&active_mode) {
        modes.push(active_mode);
    }
    modes
}

pub(crate) fn project_editor_shell_to_shell_state_json(
    model: &ProjectEditorShellModel,
) -> serde_json::Value {
    serde_json::json!({
        "workareaSplitRatio": json_number_f32(workarea_split_ratio(model.workarea_split_ratio)),
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
        workarea_split_ratio: workarea_split_ratio_from_shell_state(
            object,
            "leftCompanionWidthRatio",
        ),
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
    model.enforce_awake_mode_cap(active_mode, &[]);
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

/// The Agents column never takes less than a tenth or more than 85% of the workarea, whatever the
/// window width, so a stored or dragged ratio can never hide either side outright.
pub(crate) fn workarea_split_ratio(ratio: f32) -> f32 {
    ratio.clamp(0.10, 0.85)
}

/// The same clamp for a known workarea width: the Agents column keeps a workspace pane's minimum and
/// the view panel keeps its own, and when the window is too narrow for both the Agents column wins,
/// because the view panel is the thing the user can close.
pub(crate) fn workarea_split_ratio_for_span(ratio: f32, content_span: f32) -> f32 {
    let content_span = content_span.max(1.0);
    let agents_min_ratio = (WORKAREA_AGENTS_COLUMN_MIN_WIDTH / content_span).clamp(0.10, 0.85);
    let agents_max_ratio =
        ((content_span - WORKAREA_VIEW_PANEL_MIN_WIDTH) / content_span).clamp(0.10, 0.85);
    let ratio = workarea_split_ratio(ratio);

    if agents_min_ratio <= agents_max_ratio {
        ratio.clamp(agents_min_ratio, agents_max_ratio)
    } else {
        agents_min_ratio
    }
}
