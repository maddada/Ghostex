//! The Quick Access wire snapshot, the Rust half of packages/shared/native-quick-access.ts.
//! Every field is already resolved for display: the controller owns filtering, sorting, grouping and
//! formatting, and this window only paints what arrives here.
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum QuickAccessTabId {
    #[default]
    Commands,
    RecentProjects,
    RecentSessions,
    SavedPrompts,
}

impl QuickAccessTabId {
    pub(crate) fn modal_kind(self) -> crate::GpuiAppModalKind {
        match self {
            Self::Commands => crate::GpuiAppModalKind::CommandPalette,
            Self::RecentProjects => crate::GpuiAppModalKind::RecentProjects,
            Self::RecentSessions => crate::GpuiAppModalKind::PreviousSessions,
            Self::SavedPrompts => crate::GpuiAppModalKind::StashedPrompts,
        }
    }

    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::Commands => "commands",
            Self::RecentProjects => "recentProjects",
            Self::RecentSessions => "recentSessions",
            Self::SavedPrompts => "savedPrompts",
        }
    }

    pub(crate) fn from_modal_kind(kind: crate::GpuiAppModalKind) -> Option<Self> {
        match kind {
            crate::GpuiAppModalKind::CommandPalette => Some(Self::Commands),
            crate::GpuiAppModalKind::RecentProjects => Some(Self::RecentProjects),
            crate::GpuiAppModalKind::PreviousSessions => Some(Self::RecentSessions),
            crate::GpuiAppModalKind::StashedPrompts => Some(Self::SavedPrompts),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickAccessTab {
    pub(crate) id: QuickAccessTabId,
    pub(crate) label: String,
    pub(crate) hotkey: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum QuickAccessIcon {
    Asset {
        name: String,
        #[serde(default)]
        color: Option<String>,
    },
    Image {
        url: String,
    },
    #[default]
    None,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum QuickAccessRow {
    #[serde(rename_all = "camelCase")]
    Command {
        key: String,
        title: String,
        #[serde(default)]
        icon: QuickAccessIcon,
        #[serde(default)]
        hotkey: String,
    },
    #[serde(rename_all = "camelCase")]
    Project {
        key: String,
        title: String,
        #[serde(default)]
        icon: QuickAccessIcon,
        #[serde(default)]
        tooltip: String,
        #[serde(default)]
        session_count: u64,
        #[serde(default)]
        is_open: bool,
        #[serde(default)]
        is_hidden: bool,
    },
    #[serde(rename_all = "camelCase")]
    Session {
        key: String,
        title: String,
        #[serde(default)]
        icon: QuickAccessIcon,
        #[serde(default)]
        project_label: String,
        #[serde(default)]
        file_size: String,
        #[serde(default)]
        file_size_loading: bool,
        #[serde(default)]
        time: String,
        #[serde(default)]
        in_sidebar: bool,
        #[serde(default)]
        sleeping: bool,
        #[serde(default)]
        can_activate: bool,
    },
    #[serde(rename_all = "camelCase")]
    Prompt {
        key: String,
        title: String,
        #[serde(default)]
        tooltip: String,
        #[serde(default)]
        project_name: String,
        #[serde(default)]
        project_icon: QuickAccessIcon,
        #[serde(default)]
        session_title: String,
        #[serde(default)]
        tags: Vec<QuickAccessPromptChip>,
        #[serde(default)]
        time: String,
        #[serde(default)]
        is_favorite: bool,
    },
}

impl QuickAccessRow {
    pub(crate) fn key(&self) -> &str {
        match self {
            Self::Command { key, .. }
            | Self::Project { key, .. }
            | Self::Session { key, .. }
            | Self::Prompt { key, .. } => key,
        }
    }

    /// Rows that Enter or a click cannot activate keep their hover fill but do nothing.
    pub(crate) fn is_activatable(&self) -> bool {
        match self {
            Self::Session { can_activate, .. } => *can_activate,
            _ => true,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickAccessPromptChip {
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) color: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickAccessGroup {
    #[serde(default)]
    pub(crate) heading: String,
    #[serde(default)]
    pub(crate) separated: bool,
    pub(crate) rows: Vec<QuickAccessRow>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickAccessOption {
    pub(crate) value: String,
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) detail: String,
    #[serde(default)]
    pub(crate) color: String,
    #[serde(default)]
    pub(crate) icon: QuickAccessIcon,
    #[serde(default)]
    pub(crate) disabled: bool,
    #[serde(default)]
    pub(crate) selected: bool,
    #[serde(default)]
    pub(crate) separated: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickAccessSelect {
    #[serde(default)]
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) detail: String,
    #[serde(default)]
    pub(crate) color: String,
    #[serde(default)]
    pub(crate) options: Vec<QuickAccessOption>,
    #[serde(default)]
    pub(crate) searchable: bool,
    #[serde(default)]
    pub(crate) search_placeholder: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickAccessSegment {
    pub(crate) value: String,
    pub(crate) label: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum QuickAccessToolbar {
    #[default]
    None,
    #[serde(rename_all = "camelCase")]
    Sessions {
        scope: String,
        scopes: Vec<QuickAccessSegment>,
        #[serde(default)]
        scope_hotkey: String,
        #[serde(default)]
        tag_filter_active: bool,
        tags: QuickAccessSelect,
        projects: QuickAccessSelect,
    },
    #[serde(rename_all = "camelCase")]
    Prompts {
        view: String,
        views: Vec<QuickAccessSegment>,
        projects: QuickAccessSelect,
        tags: QuickAccessSelect,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickAccessPromptEditor {
    pub(crate) heading: String,
    #[serde(default)]
    pub(crate) content: String,
    pub(crate) projects: QuickAccessSelect,
    pub(crate) tags: QuickAccessSelect,
    #[serde(default)]
    pub(crate) is_favorite: bool,
    #[serde(default)]
    pub(crate) error: String,
    #[serde(default)]
    pub(crate) saving: bool,
    pub(crate) submit_label: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickAccessTagComposer {
    #[serde(default)]
    pub(crate) name: String,
    pub(crate) color: String,
    pub(crate) colors: Vec<String>,
    #[serde(default)]
    pub(crate) error: String,
    pub(crate) anchor: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickAccessSnapshot {
    pub(crate) version: u32,
    pub(crate) tab: QuickAccessTabId,
    pub(crate) tabs: Vec<QuickAccessTab>,
    #[serde(default)]
    pub(crate) placeholder: String,
    #[serde(default)]
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) query_revision: u64,
    #[serde(default)]
    pub(crate) loading: bool,
    #[serde(default)]
    pub(crate) loading_label: String,
    #[serde(default)]
    pub(crate) empty: String,
    #[serde(default)]
    pub(crate) groups: Vec<QuickAccessGroup>,
    #[serde(default)]
    pub(crate) selected_key: String,
    #[serde(default)]
    pub(crate) selection_seq: u64,
    #[serde(default)]
    pub(crate) toolbar: QuickAccessToolbar,
    #[serde(default)]
    pub(crate) primary_action: String,
    #[serde(default)]
    pub(crate) action_hotkeys: Vec<String>,
    #[serde(default)]
    pub(crate) hint: String,
    #[serde(default)]
    pub(crate) editor: Option<QuickAccessPromptEditor>,
    #[serde(default)]
    pub(crate) tag_composer: Option<QuickAccessTagComposer>,
}

impl QuickAccessSnapshot {
    pub(crate) fn rows(&self) -> impl Iterator<Item = &QuickAccessRow> {
        self.groups.iter().flat_map(|group| group.rows.iter())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuickAccessMenuItem {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) icon: QuickAccessIcon,
    #[serde(default)]
    pub(crate) hotkey: String,
    #[serde(default)]
    pub(crate) danger: bool,
    #[serde(default)]
    pub(crate) disabled: bool,
    #[serde(default)]
    pub(crate) separator: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum QuickAccessUpdate {
    Snapshot(QuickAccessSnapshot),
    #[serde(rename_all = "camelCase")]
    Menu {
        items: Vec<QuickAccessMenuItem>,
    },
    Close,
}
