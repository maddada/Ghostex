use crate::*;

/// CDXC:Workarea 2026-09-12 DECISION:
/// User: pane visibility is remembered for the whole app, never per project, so switching projects cannot toggle the sidebar on its own.
/// There are two layouts: Agents, and Wide for every other view (Browser, Code, Docs, Kanban, Automate, extensions), so Browser, Code and Docs never reshuffle between each other.
/// The companion and the Commands pane always follow the layout of the view. Whether the sessions sidebar does too is the advanced `sidebarVisibilityMemory` setting; its default keeps one sidebar state everywhere while the per-view model is evaluated.
/// This supersedes the 2026-09-09 decision to remember the panes per project and view. A project keeps only its last view and its companion contents.
/// SEE-ALSO: packages/shared/ghostex-settings/types.ts, apps/desktop/src/app/view_pane_state.rs, apps/desktop/native/macos/GpuiSidebarReveal.m.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiViewPaneLayoutKind {
    Agents,
    Wide,
}

impl GpuiViewPaneLayoutKind {
    pub(crate) fn for_mode(mode: TitlebarMode) -> Self {
        if mode == TitlebarMode::Agents {
            Self::Agents
        } else {
            Self::Wide
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarVisibilityMemory {
    Shared,
    PerView,
}

impl GpuiSidebarVisibilityMemory {
    pub(crate) fn from_shared_settings(
        settings: &shared_settings::SharedSidebarSettingsSnapshot,
    ) -> Self {
        match settings.sidebar_visibility_memory() {
            shared_settings::SharedSidebarVisibilityMemory::Shared => Self::Shared,
            shared_settings::SharedSidebarVisibilityMemory::PerView => Self::PerView,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct GpuiViewPaneState {
    pub(crate) sidebar_collapsed: bool,
    pub(crate) companion_visible: bool,
    pub(crate) command_mode: CommandPaneMode,
    pub(crate) command_last_expanded_mode: CommandPaneMode,
}

impl GpuiViewPaneState {
    pub(crate) fn default_for_kind(kind: GpuiViewPaneLayoutKind) -> Self {
        Self {
            sidebar_collapsed: kind == GpuiViewPaneLayoutKind::Wide,
            companion_visible: false,
            command_mode: CommandPaneMode::Collapsed,
            command_last_expanded_mode: CommandPaneMode::Pinned,
        }
    }

    fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "sidebarCollapsed": self.sidebar_collapsed,
            "companionVisible": self.companion_visible,
            "commandMode": self.command_mode.element_slug(),
            "commandLastExpandedMode": self.command_last_expanded_mode.element_slug(),
        })
    }

    fn from_json(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        let command_last_expanded_mode =
            CommandPaneMode::from_slug(object.get("commandLastExpandedMode")?.as_str()?)?;
        if command_last_expanded_mode == CommandPaneMode::Collapsed {
            return None;
        }
        Some(Self {
            sidebar_collapsed: json_bool_field(object, "sidebarCollapsed")?,
            companion_visible: json_bool_field(object, "companionVisible")?,
            command_mode: CommandPaneMode::from_slug(object.get("commandMode")?.as_str()?)?,
            command_last_expanded_mode,
        })
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct GpuiViewPaneLayouts {
    pub(crate) agents: GpuiViewPaneState,
    pub(crate) wide: GpuiViewPaneState,
    /// The single sidebar state used while `sidebarVisibilityMemory` is "shared".
    pub(crate) shared_sidebar_collapsed: bool,
}

impl GpuiViewPaneLayouts {
    pub(crate) fn shell_default() -> Self {
        Self {
            agents: GpuiViewPaneState::default_for_kind(GpuiViewPaneLayoutKind::Agents),
            wide: GpuiViewPaneState::default_for_kind(GpuiViewPaneLayoutKind::Wide),
            shared_sidebar_collapsed: false,
        }
    }

    /// First launch after the per-project pane memory was retired: the layout of
    /// the restored view starts from the companion and Commands values that were
    /// live when the app last quit, so the update itself moves nothing.
    pub(crate) fn seeded_from_restored_shell(
        active_mode: TitlebarMode,
        project_editor_shell: &ProjectEditorShellModel,
        command_pane: &CommandPaneModel,
    ) -> Self {
        let mut layouts = Self::shell_default();
        let panes = layouts.get_mut(GpuiViewPaneLayoutKind::for_mode(active_mode));
        if active_mode.is_project_editor_mode() {
            panes.companion_visible = project_editor_shell.left_companion_visible;
        }
        panes.command_mode = command_pane.mode;
        if command_pane.last_expanded_mode != CommandPaneMode::Collapsed {
            panes.command_last_expanded_mode = command_pane.last_expanded_mode;
        }
        layouts
    }

    pub(crate) fn get(&self, kind: GpuiViewPaneLayoutKind) -> GpuiViewPaneState {
        match kind {
            GpuiViewPaneLayoutKind::Agents => self.agents,
            GpuiViewPaneLayoutKind::Wide => self.wide,
        }
    }

    pub(crate) fn get_mut(&mut self, kind: GpuiViewPaneLayoutKind) -> &mut GpuiViewPaneState {
        match kind {
            GpuiViewPaneLayoutKind::Agents => &mut self.agents,
            GpuiViewPaneLayoutKind::Wide => &mut self.wide,
        }
    }

    pub(crate) fn to_shell_state_json(&self) -> serde_json::Value {
        serde_json::json!({
            "agents": self.agents.to_json(),
            "wide": self.wide.to_json(),
            "sharedSidebarCollapsed": self.shared_sidebar_collapsed,
        })
    }

    pub(crate) fn from_shell_state(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        Some(Self {
            agents: GpuiViewPaneState::from_json(object.get("agents")?)?,
            wide: GpuiViewPaneState::from_json(object.get("wide")?)?,
            shared_sidebar_collapsed: json_bool_field(object, "sharedSidebarCollapsed")?,
        })
    }
}
