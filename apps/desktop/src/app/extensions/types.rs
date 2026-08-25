use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum GpuiExtensionPermission {
    Exec,
    Cli,
    Ssh,
    Network,
    Clipboard,
}

impl GpuiExtensionPermission {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "exec" => Self::Exec,
            "cli" => Self::Cli,
            "ssh" => Self::Ssh,
            "network" => Self::Network,
            "clipboard" => Self::Clipboard,
            _ => return None,
        })
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Exec => "exec",
            Self::Cli => "cli",
            Self::Ssh => "ssh",
            Self::Network => "network",
            Self::Clipboard => "clipboard",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiExtensionPlacement {
    View,
    ChatBar,
    Popup,
    Modal,
}

impl GpuiExtensionPlacement {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "view" => Self::View,
            "chat-bar" => Self::ChatBar,
            "popup" => Self::Popup,
            "modal" => Self::Modal,
            _ => return None,
        })
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::ChatBar => "chat-bar",
            Self::Popup => "popup",
            Self::Modal => "modal",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiInstalledExtension {
    pub(crate) id: String,
    pub(crate) declared_permissions: HashSet<GpuiExtensionPermission>,
    pub(crate) granted_permissions: HashSet<GpuiExtensionPermission>,
    pub(crate) placements: Vec<GpuiExtensionPlacement>,
    pub(crate) placement: Option<GpuiExtensionPlacement>,
    pub(crate) preferences: BTreeMap<String, serde_json::Value>,
    pub(crate) storage: BTreeMap<String, serde_json::Value>,
    pub(crate) runtime_url: Option<String>,
    pub(crate) enabled: bool,
}

impl GpuiInstalledExtension {
    pub(crate) fn bridge_surface_spec(&self) -> Option<crate::cef::ExtensionBridgeSurfaceSpec> {
        if !self.enabled || self.placements.is_empty() {
            return None;
        }
        let url = self.runtime_url.as_deref()?;
        let rest = url.strip_prefix("http://")?;
        let origin_end = rest.find('/').unwrap_or(rest.len());
        let authority = &rest[..origin_end];
        if authority.is_empty() {
            return None;
        }
        let origin = format!("http://{authority}");
        let static_prefix = format!("/ext/{}/", self.id);
        let path_prefix = if rest.get(origin_end..)?.starts_with(&static_prefix) {
            static_prefix
        } else {
            "/".to_string()
        };
        crate::cef::ExtensionBridgeSurfaceSpec::new(self.id.clone(), origin, path_prefix).ok()
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiExtensionsSnapshot {
    pub(crate) installed: HashMap<String, GpuiInstalledExtension>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiExtensionProjectMetadata {
    pub(crate) project_id: String,
    pub(crate) name: String,
    pub(crate) path: Option<String>,
    pub(crate) is_worktree: bool,
    pub(crate) worktree_branch: Option<String>,
    pub(crate) worktree_name: Option<String>,
    pub(crate) parent_project_name: Option<String>,
}

#[derive(Clone)]
pub(crate) struct GpuiExtensionSurfaceContext {
    pub(crate) placement: GpuiExtensionPlacement,
    pub(crate) start_session: Option<serde_json::Value>,
}

pub(crate) type GpuiExtensionBridgeResponder = std::rc::Rc<dyn Fn(serde_json::Value)>;
pub(crate) type GpuiExtensionCloseHandler = std::rc::Rc<dyn Fn()>;
