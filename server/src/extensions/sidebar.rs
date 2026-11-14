//! Sidebar extensions on the registry: the context a panel call needs, the `extensionPanels` map
//! clients draw, and starting a sidebar extension's own server while it is on.

use std::path::PathBuf;

use ghostex_gx_protocol::extension_panel::{
    ExtensionPanelEntry, ExtensionPanels, ExtensionSessionPanel, ExtensionSidePanel,
    ExtensionSidebarMode, ExtensionSidebarPanel, ExtensionSidebarSection, SidePanelScope,
};
use serde_json::Value;

use super::{
    read_manifest, read_store, validate_extension_id, ExtensionError, ExtensionKind,
    ExtensionLaunchContext, ExtensionManifest, ExtensionRegistry, ExtensionResult,
    ExtensionStoreEntry, PanelStore, PanelSurface,
};

/// Where gxserver posts a tap on an `extension` action on the extension's own server.
pub(crate) const PANEL_ACTION_SERVER_PATH: &str = "/ghostex/panel-action";

/// What `/ext/<id>/…` serves from a sidebar extension's folder for its web blocks.
const SIDEBAR_PAGE_FILE_TYPES: &[&str] = &[
    "html", "htm", "js", "mjs", "css", "svg", "png", "jpg", "jpeg", "gif", "webp", "woff", "woff2",
];

/// An installed sidebar extension, as a panel call sees it.
pub(crate) struct SidebarExtension {
    pub(crate) manifest: ExtensionManifest,
    pub(crate) payload_dir: PathBuf,
    pub(crate) state: ExtensionStoreEntry,
}

impl ExtensionRegistry {
    pub(crate) fn panels(&self) -> &PanelStore {
        &self.panels
    }

    /// The extension whose own API token this request carries, for any endpoint (the scoped
    /// `/api/ext/v1` routes check what it may do themselves).
    pub(crate) fn extension_for_token(&self, headers: &axum::http::HeaderMap) -> Option<String> {
        self.runtime.extension_for_token(headers)
    }

    /// An installed sidebar extension, installed through Ghostex (a payload put into the
    /// extensions folder some other way has no panels: CDXC:Extensions 2026-11-08 in registry.rs).
    pub(crate) fn sidebar_extension(&self, id: &str) -> ExtensionResult<SidebarExtension> {
        validate_extension_id(id)?;
        let payload_dir = self.installed_dir().join(id);
        let manifest = read_manifest(&payload_dir, Some(id)).map_err(|error| {
            if payload_dir.exists() {
                error
            } else {
                ExtensionError::not_found(format!("Extension {id:?} is not installed."))
            }
        })?;
        if manifest.kind != Some(ExtensionKind::Sidebar) {
            return Err(ExtensionError::bad_request(format!(
                "Extension {id:?} is not a sidebar extension."
            )));
        }
        let state = read_store(&self.store_file)?.get(id).cloned().ok_or_else(|| {
            ExtensionError::not_found(format!(
                "Extension {id:?} wasn't installed through Ghostex. Install it in Settings > Extensions."
            ))
        })?;
        Ok(SidebarExtension {
            manifest,
            payload_dir,
            state,
        })
    }

    /// Every enabled sidebar extension's panels, as the `extensionPanels` snapshot field.
    ///
    /// Read without the registry's gate: snapshots are built inside the presentation sequencer,
    /// and an install holds the gate through its download, which would stall every sidebar.
    pub(crate) fn extension_panels(&self) -> Value {
        let mut panels = ExtensionPanels::default();
        let Ok(installed) = self.scan_installed() else {
            return serde_json::to_value(panels).unwrap_or(Value::Null);
        };
        for extension in installed {
            if extension.manifest.kind != Some(ExtensionKind::Sidebar) || !extension.state.enabled {
                continue;
            }
            let payload_dir = self.installed_dir().join(&extension.id);
            if let Some(entry) = self.panel_entry(
                &extension.id,
                &extension.manifest,
                &extension.state,
                &payload_dir,
            ) {
                panels.0.insert(extension.id.clone(), entry);
            }
        }
        serde_json::to_value(panels).unwrap_or(Value::Null)
    }

    fn panel_entry(
        &self,
        id: &str,
        manifest: &ExtensionManifest,
        state: &ExtensionStoreEntry,
        payload_dir: &std::path::Path,
    ) -> Option<ExtensionPanelEntry> {
        let icon_url = |icon: &str| format!("/ext/{id}/{icon}");
        let sidebar = match &manifest.sidebar {
            Some(sidebar) => {
                let record = self
                    .panels
                    .read(manifest, payload_dir, &PanelSurface::Sidebar)
                    .ok()?;
                // CDXC:Extensions 2026-11-14 WHY:
                // The coordinator's phase 2 brief: Settings > Extensions turns a sidebar extension's mode and its sections on or off. They are left out here, where every client reads them, so the desktop and the phone agree.
                Some(ExtensionSidebarPanel {
                    mode: sidebar
                        .mode
                        .as_ref()
                        .filter(|_| !state.sidebar_mode_hidden)
                        .map(|mode| ExtensionSidebarMode {
                            title: mode.title.clone(),
                            icon: Some(icon_url(mode.icon.as_deref().unwrap_or(&manifest.icon))),
                        }),
                    sections: sidebar
                        .sections
                        .iter()
                        .filter(|_| !state.sidebar_sections_hidden)
                        .map(|section| ExtensionSidebarSection {
                            block: section.block.clone(),
                            place: section.place.clone(),
                        })
                        .collect(),
                    revision: record.revision,
                    document: record.document,
                })
            }
            None => None,
        };
        let side_panel = match &manifest.side_panel {
            Some(side_panel) => {
                let scope = side_panel.scope.unwrap_or(SidePanelScope::Global);
                // A per-session side panel's base document is its template (revision 0), which
                // a session shows until a document is written for it; those ride beside it.
                let base = match scope {
                    SidePanelScope::Global => self
                        .panels
                        .read(manifest, payload_dir, &PanelSurface::SidePanel)
                        .ok()?,
                    SidePanelScope::Session => self
                        .panels
                        .template(
                            manifest,
                            payload_dir,
                            &PanelSurface::SessionSidePanel {
                                project_id: "template".into(),
                                session_id: "template".into(),
                            },
                        )
                        .ok()?,
                };
                let sessions = match scope {
                    SidePanelScope::Global => Default::default(),
                    SidePanelScope::Session => self
                        .panels
                        .session_side_panels(manifest, payload_dir)
                        .into_iter()
                        .map(|(session, record)| {
                            (
                                session,
                                ExtensionSessionPanel {
                                    revision: record.revision,
                                    document: record.document,
                                },
                            )
                        })
                        .collect(),
                };
                Some(ExtensionSidePanel {
                    title: side_panel.title.clone(),
                    icon: Some(icon_url(
                        side_panel.icon.as_deref().unwrap_or(&manifest.icon),
                    )),
                    scope,
                    revision: base.revision,
                    document: base.document,
                    sessions,
                })
            }
            None => None,
        };
        Some(ExtensionPanelEntry {
            title: manifest.title.clone(),
            icon: Some(icon_url(&manifest.icon)),
            granted_permissions: state
                .granted_permissions
                .iter()
                .map(|permission| permission.as_str().to_string())
                .collect(),
            sidebar,
            side_panel,
            page_base: Some(format!("{}/ext/{id}/", self.runtime.api_url())),
        })
    }

    /// A file of an enabled sidebar extension's own web pages (`web` blocks), for
    /// `GET /ext/<id>/<path>`: a page or what it loads, inside the extension's folder. Not a
    /// sidebar extension: `None`, and the static-extension rules apply instead (serve.rs).
    ///
    /// CDXC:Extensions 2026-11-14 WHY:
    /// A sidebar extension usually runs a command server, so it has no static folder to serve its web blocks from; its pages and their scripts, styles, fonts and pictures come from its own folder instead, by file type, so its manifest, panel and data files are not served (its scripts are, which is what a page needs).
    pub(crate) fn sidebar_page_file(
        &self,
        id: &str,
        relative: &std::path::Path,
    ) -> Option<ExtensionResult<PathBuf>> {
        let extension = self.sidebar_extension(id).ok()?;
        if !extension.state.enabled {
            return Some(Err(ExtensionError::not_found(format!(
                "Extension {id:?} is disabled."
            ))));
        }
        let allowed = relative
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .is_some_and(|extension| SIDEBAR_PAGE_FILE_TYPES.contains(&extension.as_str()));
        if !allowed {
            return Some(Err(ExtensionError::bad_request(
                "Only an extension's web pages and what they load are served.",
            )));
        }
        let root = match std::fs::canonicalize(&extension.payload_dir) {
            Ok(root) => root,
            Err(_) => {
                return Some(Err(ExtensionError::not_found(
                    "The extension has no folder.",
                )))
            }
        };
        Some(match std::fs::canonicalize(root.join(relative)) {
            Ok(path) if path.starts_with(&root) && path.is_file() => Ok(path),
            Ok(_) => Err(ExtensionError::bad_request(
                "The file is outside the extension folder.",
            )),
            Err(_) => Err(ExtensionError::not_found("No such file.")),
        })
    }

    /// The token a sidebar extension's web pages call `/api/ext/v1` with (ext_api.rs).
    pub(crate) fn sidebar_page_token(&self, id: &str) -> ExtensionResult<String> {
        let extension = self.sidebar_extension(id)?;
        if !extension.state.enabled {
            return Err(ExtensionError {
                code: "conflict",
                message: format!("{id} is turned off."),
            });
        }
        Ok(self.runtime.page_token(id))
    }

    /// The extension whose web page holds this request's page token.
    pub(crate) fn extension_for_page_token(
        &self,
        headers: &axum::http::HeaderMap,
    ) -> Option<String> {
        self.runtime.extension_for_page_token(headers)
    }

    /// Starts the server of every enabled sidebar extension that has a command server, each on
    /// its own thread (a start waits for the server's readiness probe).
    ///
    /// CDXC:Extensions 2026-11-14 WHY:
    /// A view's server starts when its page opens, but a sidebar panel is on screen whenever the sidebar is, and the panel's outside data comes from the extension's server through its token (ext_api.rs). So a sidebar extension's server runs while the extension is on: at gxserver start, after an install and when it is turned on; turning it off stops it as before.
    pub(crate) fn start_sidebar_servers(&self) {
        let Ok(installed) = self.list() else {
            return;
        };
        for extension in installed {
            self.start_sidebar_server(&extension.id, &extension.manifest, &extension.state);
        }
    }

    /// Hands a tap on an `extension` action to the extension's own server as well as to its event
    /// stream: `POST <server>/ghostex/panel-action` with the event as the body and the
    /// extension's own token, so the server knows it is gxserver calling. Best effort, off the
    /// caller's thread: a server that is not running or does not answer misses only this copy.
    pub(crate) fn post_panel_action(&self, id: &str, event: Value) {
        let Some((url, token)) = self.runtime.server_and_token(id) else {
            return;
        };
        std::thread::spawn(move || {
            let _ = crate::http_agent::agent()
                .post(format!(
                    "{}{PANEL_ACTION_SERVER_PATH}",
                    url.trim_end_matches('/')
                ))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .config()
                .timeout_global(Some(std::time::Duration::from_secs(5)))
                .build()
                .send(event.to_string().as_str());
        });
    }

    pub(crate) fn start_sidebar_server(
        &self,
        id: &str,
        manifest: &ExtensionManifest,
        state: &ExtensionStoreEntry,
    ) {
        let runs_command = manifest
            .server
            .as_ref()
            .is_some_and(|server| server.command.is_some());
        if manifest.kind != Some(ExtensionKind::Sidebar) || !state.enabled || !runs_command {
            return;
        }
        let registry = self.clone();
        let id = id.to_string();
        std::thread::spawn(move || {
            if let Err(error) = registry.start(&id, ExtensionLaunchContext::default()) {
                eprintln!("Could not start sidebar extension {id}: {error}");
            }
        });
    }
}
