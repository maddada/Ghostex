//! What the Quick Access window paints: the snapshot, a row's actions menu, and the close request.
//! Field names and shapes are packages/shared/native-quick-access.ts, so the window's reader
//! (apps/desktop/src/app/window/quick_access/model.rs) takes these unchanged.

use serde::Serialize;

/// A row glyph: a bundled `titlebar/<name>.svg`, a data URL, or an empty slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum QuickAccessIcon {
    Asset {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<String>,
    },
    Image {
        url: String,
    },
    None,
}

impl QuickAccessIcon {
    /// `assetIcon(name, color)`: the color key is absent, not empty, when there is none.
    pub(crate) fn asset(name: &str, color: Option<&str>) -> Self {
        Self::Asset {
            name: name.to_string(),
            color: color.filter(|color| !color.is_empty()).map(str::to_string),
        }
    }

    /// `imageIcon(url)`: nothing for an empty or missing URL.
    pub(crate) fn image(url: Option<&str>) -> Option<Self> {
        url.filter(|url| !url.is_empty()).map(|url| Self::Image {
            url: url.to_string(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAccessTab {
    pub id: &'static str,
    pub label: &'static str,
    pub hotkey: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum QuickAccessRow {
    #[serde(rename_all = "camelCase")]
    Command {
        key: String,
        title: String,
        icon: QuickAccessIcon,
        hotkey: String,
    },
    #[serde(rename_all = "camelCase")]
    Project {
        key: String,
        title: String,
        icon: QuickAccessIcon,
        tooltip: String,
        session_count: u64,
        is_open: bool,
        is_hidden: bool,
    },
    #[serde(rename_all = "camelCase")]
    Session {
        key: String,
        title: String,
        icon: QuickAccessIcon,
        project_label: String,
        file_size: String,
        file_size_loading: bool,
        time: String,
        in_sidebar: bool,
        sleeping: bool,
        can_activate: bool,
    },
    #[serde(rename_all = "camelCase")]
    Prompt {
        key: String,
        title: String,
        tooltip: String,
        project_name: String,
        project_icon: QuickAccessIcon,
        session_title: String,
        tags: Vec<QuickAccessPromptChip>,
        time: String,
        is_favorite: bool,
    },
}

impl QuickAccessRow {
    pub fn key(&self) -> &str {
        match self {
            Self::Command { key, .. }
            | Self::Project { key, .. }
            | Self::Session { key, .. }
            | Self::Prompt { key, .. } => key,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct QuickAccessPromptChip {
    pub label: String,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAccessGroup {
    pub key: String,
    pub heading: String,
    pub separated: bool,
    pub rows: Vec<QuickAccessRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAccessOption {
    pub value: String,
    pub label: String,
    pub detail: String,
    pub color: String,
    pub icon: QuickAccessIcon,
    pub disabled: bool,
    pub selected: bool,
    pub separated: bool,
}

impl QuickAccessOption {
    /// An option with the defaults every picker in this module uses.
    pub(crate) fn plain(value: &str, label: &str, selected: bool) -> Self {
        Self {
            value: value.to_string(),
            label: label.to_string(),
            detail: String::new(),
            color: String::new(),
            icon: QuickAccessIcon::None,
            disabled: false,
            selected,
            separated: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAccessSelect {
    pub label: String,
    pub detail: String,
    pub color: String,
    pub options: Vec<QuickAccessOption>,
    pub searchable: bool,
    pub search_placeholder: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct QuickAccessSegment {
    pub value: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum QuickAccessToolbar {
    None,
    #[serde(rename_all = "camelCase")]
    Sessions {
        scope: String,
        scopes: Vec<QuickAccessSegment>,
        scope_hotkey: String,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAccessPromptEditor {
    pub heading: String,
    pub content: String,
    pub projects: QuickAccessSelect,
    pub tags: QuickAccessSelect,
    pub is_favorite: bool,
    pub error: String,
    pub saving: bool,
    pub submit_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAccessTagComposer {
    pub name: String,
    pub color: String,
    pub colors: Vec<String>,
    pub error: String,
    pub anchor: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAccessSnapshot {
    pub kind: &'static str,
    pub version: u32,
    pub revision: u64,
    pub tab: &'static str,
    pub tabs: Vec<QuickAccessTab>,
    pub placeholder: &'static str,
    pub query: String,
    pub query_revision: u64,
    pub loading: bool,
    pub loading_label: String,
    pub empty: String,
    pub groups: Vec<QuickAccessGroup>,
    pub selected_key: String,
    pub selection_seq: u64,
    pub toolbar: QuickAccessToolbar,
    pub primary_action: String,
    pub action_hotkeys: Vec<String>,
    pub hint: String,
    pub editor: Option<QuickAccessPromptEditor>,
    pub tag_composer: Option<QuickAccessTagComposer>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAccessMenuItem {
    pub id: String,
    pub label: String,
    pub icon: QuickAccessIcon,
    pub hotkey: String,
    pub danger: bool,
    pub disabled: bool,
    pub separator: bool,
}

/// One message to the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickAccessUpdate {
    Snapshot(Box<QuickAccessSnapshot>),
    Menu(Vec<QuickAccessMenuItem>),
    Close,
}

impl QuickAccessUpdate {
    /// The JSON the window reads, exactly as `postNativeQuickAccessSnapshot` carried it.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Snapshot(snapshot) => serde_json::to_value(snapshot).unwrap_or_default(),
            Self::Menu(items) => {
                serde_json::json!({ "kind": "menu", "version": 1, "items": items })
            }
            Self::Close => serde_json::json!({ "kind": "close", "version": 1 }),
        }
    }
}
