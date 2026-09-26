//! The resource origin Docs files load their images, stylesheets and scripts from, shared by the
//! native Docs view and the browser areas it opens HTML files and drawings in.

use std::sync::Arc;

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    /// The current project's Docs resource scope: the project root and mounted Docs folders for a
    /// local project, the project's gxserver for a remote one. `None` without a project.
    pub(crate) fn manage_docs_resource_scope(&self) -> Option<cef::ManageDocsResourceScope> {
        let snapshot = self.latest_sidebar_project_snapshot.as_ref()?;
        let active_project_id = snapshot.active_project_id.as_ref()?.0.as_str();
        if let Some(reference) = gpui_remote_project_reference_from_project_id(active_project_id) {
            /*
            CDXC:Docs 2026-08-06:
            A remote project path belongs to its gxserver and must never be
            installed as a local CEF resource scope. Keep the synthetic
            Docs origin, but back it with a fixed project-id resource loader
            through the authenticated tunnel so authored remote HTML can
            load sibling CSS, JavaScript, images, and module imports too.
            */
            let target =
                self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str());
            let project_id = reference.project_id;
            let docs_folders =
                gpui_manage_additional_docs_folders_text(&self.sidebar_runtime_settings_snapshot);
            Some(cef::ManageDocsResourceScope::new_remote(Arc::new(
                move |relative_path| {
                    read_remote_manage_docs_resource(
                        target.as_ref(),
                        project_id.as_str(),
                        relative_path,
                        docs_folders.as_str(),
                    )
                },
            )))
        } else {
            /*
            CDXC:Docs 2026-08-09:
            Docs resources resolve against the same configurable roots the
            Docs bridge uses, through the one shared resolver. The lookup is
            deferred into the scope's resolver because reading the project's
            Docs directory talks to the daemon, which must never run on the
            main thread while a CEF surface is being created.

            CDXC:Docs 2026-08-09: both mounts come out of that
            same deferred lookup — the project root serving docs/ and the
            configured Docs folders, and the mounted Docs directory serving
            its whole tree behind its reserved path segment.
            */
            let project_root = snapshot.in_memory_project_path.clone()?;
            let project_id = active_project_id.to_string();
            let dynamic_project_id = project_id.clone();
            let docs_folders =
                gpui_manage_additional_docs_folders_text(&self.sidebar_runtime_settings_snapshot);
            let global_docs_directory =
                gpui_global_docs_directory_text(&self.sidebar_runtime_settings_snapshot);
            Some(cef::ManageDocsResourceScope::new(
                Arc::new(move || {
                    let roots = manage_docs_root(
                        Some(project_id.as_str()),
                        Some(project_root.as_path()),
                        global_docs_directory.as_str(),
                        None,
                        None,
                    )
                    .ok()?;
                    let mut mounts = vec![cef::ManageDocsResourceRoot {
                        allowed_relative_roots: manage_docs_project_scan_root_relative_paths(
                            roots.project.as_path(),
                            docs_folders.as_str(),
                        ),
                        mount_segment: String::new(),
                        path: roots.project,
                    }];
                    if let Some(path) = roots.extra.and_then(|mount| mount.location.ok()) {
                        mounts.push(cef::ManageDocsResourceRoot {
                            // The whole tree, matching what the mount lists.
                            allowed_relative_roots: vec![String::new()],
                            mount_segment: MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT.to_string(),
                            path,
                        });
                    }
                    Some(mounts)
                }),
                Arc::new(move |relative_path| {
                    let (id, _) = manage_chat_file_address(relative_path)?;
                    let authorization =
                        resolve_manage_chat_file(&dynamic_project_id, relative_path)?;
                    Some(cef::ManageDocsResourceRoot {
                        allowed_relative_roots: vec![String::new()],
                        mount_segment: format!("{MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT}/{id}"),
                        path: authorization.root,
                    })
                }),
            ))
        }
    }
}
