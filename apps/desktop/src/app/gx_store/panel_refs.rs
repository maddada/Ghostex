//! A sidebar extension's `project` and `session` items (native_sidebar/extension_panel/refs.rs):
//! their hover text and right-click menu, from this computer's whole Projects list rather than the
//! list on screen (gx-core `unfiltered_local_view`), so they don't depend on the Space shown, or
//! on the extension's mode, which draws no list at all. The GPUI web build compiles this file
//! too (its symlink in apps/gpui-web/src/app/gx_store/).

use std::collections::BTreeMap;

use ghostex_gx_core::extension_panel::{panel_ref_hover_texts, unfiltered_local_view};
use ghostex_gx_core::{MachineId, ProjectKey, SidebarMenus, menu_to_json};
use serde_json::Value;

use crate::GhostexGpuiApp;

/// The items' hover texts, rebuilt when the store moves or the minute does (the session texts
/// carry relative times).
#[derive(Clone, Debug, Default)]
pub(crate) struct PanelRefHoverCache {
    key: (i64, u64),
    projects: BTreeMap<String, String>,
    sessions: BTreeMap<String, String>,
}

impl GhostexGpuiApp {
    fn gx_store_panel_ref_hover_key(&self) -> (i64, u64) {
        let revision = self
            .gx_store
            .core
            .presentation()
            .loaded_live(&MachineId::Local)
            .map(|loaded| loaded.revision)
            .unwrap_or_default();
        (revision, super::host::now_ms() / 60_000)
    }

    /// The hover text of a project item (by project id) or a session item (by sidebar session id),
    /// as its row in the Projects list shows it.
    pub(crate) fn gx_store_panel_ref_hover(
        &self,
        project_id: Option<&str>,
        sidebar_session_id: Option<&str>,
    ) -> Option<String> {
        let key = self.gx_store_panel_ref_hover_key();
        let mut cache = self.native_sidebar.extension_ref_hover.borrow_mut();
        if cache.as_ref().is_none_or(|cache| cache.key != key) {
            let view = unfiltered_local_view(
                &self.gx_store.core,
                &self.gx_store.sidebar_list.last_inputs,
                super::host::now_ms(),
            );
            let (projects, sessions) = panel_ref_hover_texts(&view);
            *cache = Some(PanelRefHoverCache {
                key,
                projects,
                sessions,
            });
        }
        let cache = cache.as_ref()?;
        match (project_id, sidebar_session_id) {
            (Some(project_id), _) => cache.projects.get(&project_group_id(project_id)).cloned(),
            (None, Some(session)) => cache.sessions.get(session).cloned(),
            (None, None) => None,
        }
    }

    /// The project's right-click menu, as its header in the Projects list has it.
    pub(crate) fn gx_store_panel_project_menu(&mut self, project_id: &str) -> Option<Value> {
        let group_id = project_group_id(project_id);
        self.gx_store_panel_menu(|menus, view| {
            let group = view
                .groups
                .iter()
                .find(|group| group.core.group_id == group_id)?;
            Some(menu_to_json(&menus.project_menu(group)))
        })
    }

    /// The session's right-click menu, as its card in the Projects list has it.
    pub(crate) fn gx_store_panel_session_menu(
        &mut self,
        sidebar_session_id: &str,
    ) -> Option<Value> {
        self.gx_store_panel_menu(|menus, _| {
            menus
                .row_menu(sidebar_session_id)
                .map(|menu| menu_to_json(&menu))
        })
    }

    fn gx_store_panel_menu(
        &mut self,
        build: impl FnOnce(&SidebarMenus<'_>, &ghostex_gx_core::SidebarView) -> Option<Value>,
    ) -> Option<Value> {
        let host = self.gx_store_menu_host();
        let now_ms = super::host::now_ms();
        let store = &self.gx_store;
        let view = unfiltered_local_view(&store.core, &store.sidebar_list.last_inputs, now_ms);
        let menus = SidebarMenus::new(
            &store.core,
            &view,
            &store.sidebar_list.last_inputs,
            &host,
            now_ms,
        );
        build(&menus, &view).filter(|menu| menu.as_array().is_some_and(|items| !items.is_empty()))
    }
}

fn project_group_id(project_id: &str) -> String {
    ProjectKey {
        machine: MachineId::Local,
        project_id: project_id.to_string(),
    }
    .to_sidebar_group_id()
}
