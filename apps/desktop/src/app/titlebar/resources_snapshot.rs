// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the native resources snapshot sampling used by the titlebar resources popover.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

use std::collections::HashSet;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn gpui_native_resources_snapshot(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> GpuiNativeResourcesSnapshot {
        /*
        GPUI owns this process snapshot directly. The native popup samples only
        when opened, so Tips/Resources no longer create a CEF browser, wait for
        React readiness, or run hidden web polling after dismissal.
        */
        let processes = gpui_read_native_resource_processes();
        let servers = gpui_read_native_resource_servers();
        self.gpui_native_resources_snapshot_from_samples(processes, servers, cx)
    }

    pub(crate) fn gpui_native_resources_snapshot_from_samples(
        &self,
        processes: Vec<GpuiNativeResourceProcess>,
        servers: Vec<GpuiNativeResourceServer>,
        cx: &mut gpui::Context<Self>,
    ) -> GpuiNativeResourcesSnapshot {
        let children_by_parent = gpui_native_resource_children_by_parent(&processes);
        let mut claimed_pids = HashSet::new();
        let mut session_rows = Vec::new();
        let mut inactive_terminal_sleep_count = 0;
        let mut sleep_all_session_count = 0;

        let protected_browser_tab_ids = if self.active_mode == TitlebarMode::Browser
            && self
                .project_editor_shell
                .is_mode_awake(TitlebarMode::Browser)
        {
            self.browser_tabs.rendered_active_loaded_tab_ids()
        } else {
            HashSet::new()
        };
        sleep_all_session_count += self.browser_surfaces.len();
        inactive_terminal_sleep_count += self
            .browser_surfaces
            .keys()
            .filter(|tab_id| !protected_browser_tab_ids.contains(tab_id))
            .count();

        for session in &self.agents_workspace.terminal_sessions {
            let title = self.agents_workspace_tab_display_title(session.id);
            let mapped_key =
                self.local_workspace_session_mappings
                    .iter()
                    .find_map(|(key, shell_session_id)| {
                        (*shell_session_id == session.id).then_some(key)
                    });
            let session_id = mapped_key
                .map(|key| gpui_combined_presentation_session_id(&key.project_id, &key.session_id))
                .unwrap_or_else(|| gpui_agents_session_external_id(session.id));
            let match_tokens = [
                session.zmx_session_name.as_deref(),
                Some(session_id.as_str()),
                Some(title.as_str()),
            ];
            let seeds = processes
                .iter()
                .filter(|process| {
                    match_tokens.iter().flatten().any(|token| {
                        let token = token.trim();
                        token.chars().count() >= 4 && process.command.contains(token)
                    })
                })
                .cloned()
                .collect::<Vec<_>>();
            let tree = gpui_collect_native_resource_process_tree(&seeds, &children_by_parent);
            if tree.is_empty()
                && (session.presentation_state == TerminalSessionPresentationState::Sleeping
                    || session.presentation_state
                        == TerminalSessionPresentationState::StartupFailed
                    || session.zmx_session_name.is_none())
            {
                continue;
            }
            claimed_pids.extend(tree.iter().map(|process| process.pid));
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&tree);
            sleep_all_session_count += 1;
            if session.presentation_state != TerminalSessionPresentationState::Sleeping
                && session.activity == AgentTerminalActivity::Idle
                && !session.delayed_send_active
            {
                inactive_terminal_sleep_count += 1;
            }
            session_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::Session,
                agent_icon: session.agent_icon,
                children: gpui_native_resource_child_rows(&tree, seeds.first().map(|row| row.pid)),
                cpu,
                detail: match seeds.first() {
                    Some(process) => format!(
                        "{} terminal pid {}",
                        gpui_native_resource_process_name(process),
                        process.system_pid
                    ),
                    None => "Active, not loaded".to_string(),
                },
                icon_path: "titlebar/terminal-2.svg",
                label: title,
                memory_mb,
                pids: tree.iter().map(|process| process.system_pid).collect(),
                session_id: Some(session_id),
                url: None,
            });
        }

        let mut browser_rows = Vec::new();
        for tab in &self.browser_tabs.tabs {
            if tab.state != BrowserTabState::Loaded {
                continue;
            }
            let Some(surface) = self.browser_surfaces.get(&tab.id) else {
                continue;
            };
            let browser_id = surface.read(cx).browser_identifier().to_string();
            let browser_processes = processes
                .iter()
                .filter(|process| {
                    !claimed_pids.contains(&process.pid)
                        && gpui_native_resource_is_ghostex_browser_process(process)
                        && (process
                            .command
                            .contains(&format!("--client-id={browser_id}"))
                            || process
                                .command
                                .contains(&format!("--renderer-client-id={browser_id}")))
                })
                .cloned()
                .collect::<Vec<_>>();
            if browser_processes.is_empty() {
                continue;
            }
            claimed_pids.extend(browser_processes.iter().map(|process| process.pid));
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&browser_processes);
            session_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::Browser(tab.id),
                agent_icon: None,
                children: gpui_native_resource_child_rows(&browser_processes, None),
                cpu,
                detail: tab.url.clone(),
                icon_path: BROWSER_ICON_WORLD,
                label: tab.display_title(),
                memory_mb,
                pids: browser_processes
                    .iter()
                    .map(|process| process.system_pid)
                    .collect(),
                session_id: None,
                url: Some(tab.url.clone()),
            });
        }

        let browser_runtime_processes = processes
            .iter()
            .filter(|process| {
                !claimed_pids.contains(&process.pid)
                    && gpui_native_resource_is_ghostex_browser_process(process)
            })
            .cloned()
            .collect::<Vec<_>>();
        if !browser_runtime_processes.is_empty() {
            claimed_pids.extend(browser_runtime_processes.iter().map(|process| process.pid));
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&browser_runtime_processes);
            browser_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::None,
                agent_icon: None,
                children: gpui_native_resource_child_rows(&browser_runtime_processes, None),
                cpu,
                detail: "Shared GPU, network, and storage helpers".to_string(),
                icon_path: BROWSER_ICON_WORLD,
                label: "Browser runtime".to_string(),
                memory_mb,
                pids: browser_runtime_processes
                    .iter()
                    .map(|process| process.system_pid)
                    .collect(),
                session_id: None,
                url: None,
            });
        }

        /*
        CDXC:GPUITitlebarResources 2026-08-19-12:10:
        Dev Servers rows describe one listening *process*, not one listening
        socket, and never root at the app's own executables. The Ghostex shell
        listens on the CEF remote-debugging port, so rooting a row there walked
        the whole app process tree and reported every CEF helper as a single
        dev server; a process holding several ports repeated its whole tree in
        one row per port. Both inflated the row and the section total far past
        the app total. Keep the listener process plus its own descendants, stop
        at any other listener and at app executables, and fold a process's
        extra ports into its one row.
        */
        let listener_pids = servers
            .iter()
            .map(|server| server.pid)
            .collect::<HashSet<_>>();
        let mut grouped_servers: Vec<(GpuiNativeResourceServer, Vec<u16>)> = Vec::new();
        for server in servers {
            let Some(process) = processes.iter().find(|process| process.pid == server.pid) else {
                continue;
            };
            if gpui_native_resource_is_app_shell_process(process) {
                continue;
            }
            if !claimed_pids.contains(&server.pid)
                && !gpui_native_resource_is_ghostex_owned_process(process)
            {
                continue;
            }
            match grouped_servers
                .iter_mut()
                .find(|(existing, _)| existing.pid == server.pid)
            {
                Some((existing, extra_ports)) => {
                    if server.port < existing.port {
                        extra_ports.push(existing.port);
                        *existing = server;
                    } else {
                        extra_ports.push(server.port);
                    }
                }
                None => grouped_servers.push((server, Vec::new())),
            }
        }

        grouped_servers.sort_by_key(|(server, _)| (server.port, server.pid));

        let mut server_rows = Vec::new();
        for (server, mut extra_ports) in grouped_servers {
            let Some(process) = processes.iter().find(|process| process.pid == server.pid) else {
                continue;
            };
            let owning_session = session_rows.iter().find(|row| {
                matches!(row.action, GpuiNativeResourceAction::Session)
                    && row.pids.contains(&server.pid)
            });
            let tree = gpui_collect_native_resource_process_tree_bounded(
                std::slice::from_ref(process),
                &children_by_parent,
                &|candidate| {
                    listener_pids.contains(&candidate.pid)
                        || gpui_native_resource_is_app_shell_process(candidate)
                },
            );
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&tree);
            extra_ports.sort_unstable();
            extra_ports.dedup();
            let mut detail = format!(
                "{} pid {}",
                gpui_native_resource_process_name(process),
                process.system_pid
            );
            if !extra_ports.is_empty() {
                detail.push_str(&format!(
                    " • also :{}",
                    extra_ports
                        .iter()
                        .map(|port| port.to_string())
                        .collect::<Vec<_>>()
                        .join(", :")
                ));
            }
            server_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::Server,
                agent_icon: owning_session.and_then(|row| row.agent_icon),
                children: gpui_native_resource_child_rows(&tree, Some(server.pid)),
                cpu,
                detail,
                icon_path: BROWSER_ICON_WORLD,
                label: server.label,
                memory_mb,
                pids: tree.iter().map(|process| process.system_pid).collect(),
                session_id: owning_session.and_then(|row| row.session_id.clone()),
                url: Some(server.url),
            });
        }


        let mut code_rows = Vec::new();
        if let Some(process) = processes
            .iter()
            .find(|process| process.command.contains("code-server"))
        {
            let tree = gpui_collect_native_resource_process_tree(
                std::slice::from_ref(process),
                &children_by_parent,
            );
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&tree);
            code_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::Code,
                agent_icon: None,
                children: gpui_native_resource_child_rows(&tree, Some(process.pid)),
                cpu,
                detail: format!("pid {}", process.system_pid),
                icon_path: TITLEBAR_ICON_CODE,
                label: "Code".to_string(),
                memory_mb,
                pids: tree.iter().map(|process| process.system_pid).collect(),
                session_id: None,
                url: None,
            });
        }

        let orphan_roots = processes
            .iter()
            .filter(|process| {
                !claimed_pids.contains(&process.pid)
                    && gpui_native_resource_is_ghostex_owned_process(process)
                    && gpui_native_resource_is_user_runtime_process(process)
            })
            .filter(|process| {
                !processes.iter().any(|parent| {
                    parent.pid == process.ppid
                        && !claimed_pids.contains(&parent.pid)
                        && gpui_native_resource_is_ghostex_owned_process(parent)
                        && gpui_native_resource_is_user_runtime_process(parent)
                })
            })
            .take(16)
            .cloned()
            .collect::<Vec<_>>();
        let mut orphan_rows = Vec::new();
        for root in orphan_roots {
            let tree = gpui_collect_native_resource_process_tree(
                std::slice::from_ref(&root),
                &children_by_parent,
            )
            .into_iter()
            .filter(|process| !claimed_pids.contains(&process.pid))
            .collect::<Vec<_>>();
            claimed_pids.extend(tree.iter().map(|process| process.pid));
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&tree);
            orphan_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::Orphan,
                agent_icon: None,
                children: gpui_native_resource_child_rows(&tree, Some(root.pid)),
                cpu,
                detail: format!("pid {}", root.system_pid),
                icon_path: TITLEBAR_ICON_BOX,
                label: gpui_native_resource_process_name(&root),
                memory_mb,
                pids: tree.iter().map(|process| process.system_pid).collect(),
                session_id: None,
                url: None,
            });
        }

        let app_roots = processes
            .iter()
            .filter(|process| {
                gpui_native_resource_is_app_bundle_process(process)
                    || (cfg!(target_os = "windows")
                        && gpui_native_resource_is_ghostex_owned_process(process))
            })
            .cloned()
            .collect::<Vec<_>>();
        let app_tree = gpui_collect_native_resource_process_tree(&app_roots, &children_by_parent);
        let (total_cpu, total_memory_mb) = gpui_sum_native_resource_processes(&app_tree);
        GpuiNativeResourcesSnapshot {
            browser_rows,
            code_rows,
            inactive_terminal_sleep_count,
            orphan_rows,
            persistent_session_mode: gpui_titlebar_session_persistence_provider_from_settings(
                shared_settings::shared_sidebar_settings_snapshot().object(),
            ) != "off",
            project_label: self
                .latest_sidebar_project_snapshot
                .as_ref()
                .map(|snapshot| snapshot.display_name.clone())
                .unwrap_or_else(|| "Ghostex".to_string()),
            server_rows,
            session_rows,
            sleep_all_session_count,
            total_cpu,
            total_memory_mb,
        }
    }

}
