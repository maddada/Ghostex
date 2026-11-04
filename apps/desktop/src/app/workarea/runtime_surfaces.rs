//! The CEF surfaces that back a project workarea view: creating one for a slot, deciding
//! whether it may be visible, pruning the ones whose gate is gone, and tearing one down.
//! Moved verbatim out of `app/workarea.rs` on 2026-09-20.

use crate::app::element::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;
use gpui::ClipboardItem;
use gpui::Entity;
use std::time::Instant;

impl GhostexGpuiApp {
    pub(crate) fn remove_project_workarea_runtime_cef_surface(
        &mut self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(owned_surface) = self.project_workarea_runtime_cef_surfaces.remove(&slot_key)
        else {
            return false;
        };
        owned_surface
            .surface
            .update(cx, |surface, _| surface.set_visible(false));
        self.update_project_workarea_runtime_cef_surface_visibility(cx);
        true
    }

    pub(crate) fn project_workarea_runtime_cef_surface_replacement_permitted(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> bool {
        /*
        CDXC:Workarea 2026-06-24-11:03:
        Runtime placeholder replacement now follows real navigable CEF URL authority, not the retired proof-only owner gates. Kanban, Automate, and Manage can replace placeholders only when the current explicit project snapshot can issue a first-party bundled CEF URL.

        CDXC:CodeEditor 2026-06-24-23:17:
        Source joins this same replacement edge only after the app-owned code-server runtime has reached the ready state for the current explicit sidebar project target. The URL may be used immediately for CefSurface creation but is not retained in shell state, logs, or proof JSON.
        */
        self.project_workarea_runtime_url_for_slot(slot_key)
            .is_some()
    }

    pub(crate) fn project_workarea_runtime_url_for_slot(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> Option<ProjectWorkareaRealRuntimeUrl> {
        if let ProjectWorkareaCefSurfaceSlotKey::Extension(id) = slot_key {
            if gpui_custom_view(id).is_none() {
                self.latest_sidebar_project_snapshot.as_ref()?;
            }
            return self.extension_view_runtime_url(id);
        }
        // The Work page is app-wide: it needs a work-mode project in the window, not a project
        // snapshot (app/work_view/).
        if slot_key == ProjectWorkareaCefSurfaceSlotKey::Work {
            return self
                .work_view_available()
                .then(|| self.work_view_page_url())
                .flatten();
        }
        let snapshot = self.latest_sidebar_project_snapshot.as_ref()?;
        match slot_key {
            ProjectWorkareaCefSurfaceSlotKey::Source => {
                let target = self.source_code_server_runtime_target(snapshot)?;
                self.source_code_server_runtime
                    .runtime_url_for_target(&target)
            }
            ProjectWorkareaCefSurfaceSlotKey::CodeWindow(id) => {
                let target = self.source_code_server_runtime_target(snapshot)?;
                let code_view_url = self
                    .source_code_server_runtime
                    .runtime_url_for_target(&target)?;
                self.code_window_runtime_url(&code_view_url, id)
            }
            // Kanban renders natively (app/native_kanban/), so its CEF slot never gets a URL
            // either: no surface is created, and one left from before is pruned.
            ProjectWorkareaCefSurfaceSlotKey::Kanban => None,
            // Automate renders natively (app/native_automate/), so its CEF slot never gets a URL:
            // no surface is created, and one left from before is pruned.
            ProjectWorkareaCefSurfaceSlotKey::Automate => None,
            // Files (app/native_docs/) draws the view itself; its slot only runs the embed page
            // as the browser area for an open HTML file, drawing or media file.
            ProjectWorkareaCefSurfaceSlotKey::Manage => self.native_docs_browser_area_url(snapshot),
            ProjectWorkareaCefSurfaceSlotKey::Work
            | ProjectWorkareaCefSurfaceSlotKey::Extension(_) => None,
        }
    }

    pub(crate) fn project_workarea_runtime_cef_surface_may_be_visible(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> bool {
        self.project_workarea_view_may_show_page(slot_key)
            // The Code view shows one of its windows at a time.
            && (!slot_key.is_code_editor() || slot_key == self.active_code_editor_slot())
    }

    /// Whether the view a page belongs to may show a page at all: it is the active, awake view,
    /// nothing covers it, and no tab drag is under way.
    ///
    /// CDXC:CodeEditor 2026-10-25 WHY:
    /// A switch between Code windows keeps the outgoing window's page on screen for a few frames (`code_window_handoff_keeps_visible`), but only in place of the selected-window rule below. It once bypassed this whole gate, so a tab drag that began mid-switch showed the old page over the drag.
    fn project_workarea_view_may_show_page(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> bool {
        let mode = slot_key.titlebar_mode();
        // Claude's page shows in the sessions column's place (app/claude_web/takeover.rs).
        self.view_page_on_screen(mode)
            // The Work page never sleeps as a view, like the GPUI pages beside it.
            && (self.project_editor_shell.is_mode_awake(mode)
                || slot_key == ProjectWorkareaCefSurfaceSlotKey::Work)
            && !(slot_key == ProjectWorkareaCefSurfaceSlotKey::Manage
                && self.native_docs_browser_area_covered())
            && !self.browser_tab_drag_active
            && !self.command_tab_drag_active
            && !self.workspace_tab_drag_active
    }

    pub(crate) fn project_workarea_runtime_cef_surface_should_be_visible(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> bool {
        self.project_workarea_runtime_cef_surface_replacement_permitted(slot_key)
            && self.project_workarea_runtime_cef_surface_may_be_visible(slot_key)
    }

    pub(crate) fn project_workarea_runtime_cef_surface_is_current(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> bool {
        let Some(runtime_url) = self.project_workarea_runtime_url_for_slot(slot_key) else {
            return false;
        };
        self.project_workarea_runtime_cef_surfaces
            .get(&slot_key)
            .is_some_and(|owned_surface| owned_surface.matches_runtime_url(&runtime_url))
    }

    pub(crate) fn project_workarea_runtime_cef_surface_for_render(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> Option<Entity<CefSurface>> {
        if !self.project_workarea_runtime_cef_surface_may_be_visible(slot_key) {
            return None;
        }

        let runtime_url = self.project_workarea_runtime_url_for_slot(slot_key)?;
        let owned_surface = self.project_workarea_runtime_cef_surfaces.get(&slot_key)?;
        owned_surface
            .matches_runtime_url(&runtime_url)
            .then(|| owned_surface.surface.clone())
    }

    #[allow(dead_code)]
    pub(crate) fn ensure_project_workarea_runtime_cef_surface(
        &mut self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        runtime_url: ProjectWorkareaRealRuntimeUrl,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<CefSurface>> {
        /*
        CDXC:Workarea 2026-06-24-10:12:
        Project-workarea CEF creation happens only at the visible replacement edge after the slot gate already permits placeholder replacement and a real runtime URL value has been supplied. This avoids hidden/offscreen preparatory mounts and keeps URL values out of app shell state while still using the existing CefSurface child-view wrapper for Source, Kanban, Automate, and Manage.
        */
        if !self.project_workarea_runtime_cef_surface_should_be_visible(slot_key) {
            return None;
        }
        if let Some(owned_surface) = self.project_workarea_runtime_cef_surfaces.get(&slot_key)
            && owned_surface.matches_runtime_url(&runtime_url)
        {
            return Some(owned_surface.surface.clone());
        }
        if let Some(stale_surface) = self.project_workarea_runtime_cef_surfaces.remove(&slot_key) {
            stale_surface
                .surface
                .update(cx, |surface, _| surface.set_visible(false));
            self.park_project_workarea_runtime_cef_surface(stale_surface, slot_key, true, cx);
        }
        if let Some(owned) =
            self.take_parked_project_workarea_runtime_cef_surface(&runtime_url, slot_key)
        {
            let surface = owned.surface.clone();
            self.project_workarea_runtime_cef_surfaces
                .insert(slot_key, owned);
            self.update_project_workarea_runtime_cef_surface_visibility(cx);
            return Some(surface);
        }

        let parent_ns_view = self.parent_ns_view;
        let surface_id = slot_key.cef_surface_id();
        let mut profile = slot_key.cef_profile_id();
        let website_view = slot_key.titlebar_mode().website_provider().is_some();
        if website_view {
            profile = self.browser_tab_cef_profile(self.browser_profiles.active_profile_id());
            if !self.prepare_workspace_browser_context(&profile, cx) {
                return None;
            }
        }
        if let ProjectWorkareaCefSurfaceSlotKey::Extension(id) = slot_key
            && !website_view
            && gpui_custom_view(id).is_some_and(|v| v.definition.get("source").is_some())
            && let Some(remote) = self
                .latest_sidebar_project_snapshot
                .as_ref()
                .and_then(|s| s.active_project_id.as_ref())
                .and_then(|p| gpui_remote_project_reference_from_project_id(&p.0))
        {
            profile = format!("{profile}-{}", remote.remote_machine_id);
            let tunnel = self.ensure_remote_browser_tunnel(&remote.remote_machine_id, false, cx)?;
            if !self.prepare_remote_browser_context(
                &remote.remote_machine_id,
                &profile,
                tunnel.port,
                cx,
            ) {
                return None;
            }
        }
        // The Files view showing a page an agent published: its own browser, without the
        // Files bridge (native_docs/published_page.rs).
        let published_page = slot_key == ProjectWorkareaCefSurfaceSlotKey::Manage
            && self.native_docs_active_page().is_some();
        let themed_page = !published_page
            && matches!(
                slot_key,
                ProjectWorkareaCefSurfaceSlotKey::Kanban
                    | ProjectWorkareaCefSurfaceSlotKey::Automate
                    | ProjectWorkareaCefSurfaceSlotKey::Manage
                    | ProjectWorkareaCefSurfaceSlotKey::Work
            );
        let mut url = runtime_url.clone().into_cef_url();
        if themed_page {
            let separator = if url.contains('?') { '&' } else { '?' };
            let theme = if CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed) {
                "light"
            } else {
                "dark"
            };
            url.push_str(&format!("{separator}appTheme={theme}"));
        }
        // The Source slot hosts the app-owned code-server runtime; its origin
        // is the one trusted clipboard origin (macOS trustedClipboardOrigin).
        let trusted_clipboard_origin = slot_key.is_code_editor().then(|| url.clone());
        let code_window_popup_handler = slot_key
            .is_code_editor()
            .then(|| self.code_window_popup_open_handler(&url, cx));
        let project_workarea_bridge_event_handler = (!published_page
            && !matches!(slot_key, ProjectWorkareaCefSurfaceSlotKey::Extension(_)))
        .then(|| self.project_workarea_bridge_event_handler(slot_key, cx));
        let surface_background =
            if CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed) {
                rgb(0xffffff).into()
            } else if slot_key.is_code_editor() || themed_page {
                source_view_background_color()
            } else {
                workspace_background_color()
            };
        let manage_docs_resource_scope =
            if slot_key == ProjectWorkareaCefSurfaceSlotKey::Manage && !published_page {
                Some(self.manage_docs_resource_scope()?)
            } else {
                None
            };
        let page_loaded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let page_load_end_handler = self.project_workarea_page_load_end_handler(&page_loaded, cx);
        let creation_result = match slot_key {
            ProjectWorkareaCefSurfaceSlotKey::Manage if published_page => self
                .native_docs_create_page_surface(
                    surface_id,
                    parent_ns_view,
                    url,
                    surface_background,
                    page_load_end_handler,
                    cx,
                ),
            ProjectWorkareaCefSurfaceSlotKey::Extension(id) => {
                if gpui_custom_view(id).is_some() {
                    // The built-in Claude view opens on the picked Claude account's own claude.ai
                    // sign-in, and waits while the account list loads (claude_web/accounts.rs).
                    let profile = if slot_key.titlebar_mode().is_claude_web() {
                        let account_profile = self.claude_web_profile(&profile, cx)?;
                        if !self.prepare_workspace_browser_context(&account_profile, cx) {
                            return None;
                        }
                        account_profile
                    } else {
                        profile
                    };
                    // CDXC:Browser 2026-09-22 DECISION:
                    // User: middle-click and Cmd-click in project website views open other tickets in new Browser tabs. Reuse the project-owned popup route, including its background placement and parked-project handling.
                    let popup_remote = if website_view {
                        None
                    } else {
                        self.active_project_id_for_view_scope()
                            .and_then(|project| {
                                gpui_remote_project_reference_from_project_id(&project)
                            })
                            .map(|remote| remote.remote_machine_id)
                    };
                    let popup_handler = self.browser_popup_open_handler(popup_remote, None, cx);
                    CefSurface::try_new(
                        surface_id,
                        parent_ns_view,
                        url,
                        profile,
                        pane_prepaint_background_color(),
                        true,
                        surface_background,
                        None,
                        true,
                        Some(popup_handler),
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(page_load_end_handler.clone()),
                        None,
                        cx,
                    )
                } else {
                    let extension = self.installed_extension_view(id)?;
                    let bridge_surface = extension
                        .bridge_surface_spec()
                        .filter(|surface| surface.matches_url(&url))
                        .or_else(|| extension.bridge_surface_spec_for_url(&url));
                    let Some(bridge_surface) = bridge_surface else {
                        return None;
                    };
                    CefSurface::try_new_extension(
                        surface_id,
                        parent_ns_view,
                        url,
                        profile,
                        pane_prepaint_background_color(),
                        false,
                        surface_background,
                        true,
                        bridge_surface,
                        self.extension_view_bridge_event_handler(slot_key, cx),
                        Some(page_load_end_handler.clone()),
                        cx,
                    )
                }
            }
            _ => CefSurface::try_new(
                surface_id,
                parent_ns_view,
                url,
                profile,
                pane_prepaint_background_color(),
                false,
                surface_background,
                trusted_clipboard_origin,
                true,
                None,
                code_window_popup_handler,
                None,
                None,
                project_workarea_bridge_event_handler,
                manage_docs_resource_scope,
                Some(page_load_end_handler),
                None,
                cx,
            ),
        };
        let surface = match creation_result {
            Ok(surface) => surface,
            Err(error) => {
                // Ensure-style reconcile: skip this pass, retried on the next
                // workarea sync (CDXC:CefRuntime 2026-07-11).
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.cefSurface.createFailed",
                    serde_json::json!({ "surface": "projectWorkarea", "error": error }),
                );
                return None;
            }
        };
        self.project_workarea_runtime_cef_surfaces.insert(
            slot_key,
            ProjectWorkareaRuntimeCefSurface {
                runtime_url,
                surface: surface.clone(),
                page_loaded,
                created_at: Instant::now(),
            },
        );
        self.release_held_page_replaced_by(slot_key);
        if slot_key.is_code_editor() {
            self.prepare_code_editor_page(slot_key, &surface, cx);
        }
        self.update_project_workarea_runtime_cef_surface_visibility(cx);
        // The skeleton hold ends on its own if the page never reports a load end.
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(PROJECT_WORKAREA_PAGE_SKELETON_MAX)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.update_project_workarea_runtime_cef_surface_visibility(cx);
                cx.notify();
            });
        })
        .detach();
        Some(surface)
    }

    /// CDXC:Workarea 2026-09-19 WHY:
    /// A freshly created page painted its prepaint colour, then whatever the page drew first, for as long as it took to load; the view's skeleton has to stay in front until the page is really there.
    /// GPUI cannot paint over a native child view, so the child stays hidden until its first main-frame load end (or the skeleton hold runs out) and the visibility reconcile shows it then.
    fn project_workarea_page_load_end_handler(
        &self,
        page_loaded: &std::sync::Arc<std::sync::atomic::AtomicBool>,
        cx: &mut gpui::Context<Self>,
    ) -> cef::PageLoadEndHandler {
        let page_loaded = page_loaded.clone();
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        std::rc::Rc::new(move || {
            page_loaded.store(true, std::sync::atomic::Ordering::Relaxed);
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    let _ = app.update(&mut async_cx, |this, cx| {
                        this.update_project_workarea_runtime_cef_surface_visibility(cx);
                        cx.notify();
                    });
                })
                .detach();
        })
    }

    pub(crate) fn ensure_project_workarea_runtime_cef_surfaces_for_current_context(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.ensure_custom_project_views(cx);
        /*
        CDXC:Workarea 2026-06-24-11:03:
        Workarea CEF surface materialization is active-workarea-only. The app creates Kanban/Automate/Manage CefSurface entities only when CEF is initialized, the workarea is selected and awake, and a real bundled runtime URL can be issued from the current explicit sidebar snapshot; it does not prewarm hidden surfaces or synthesize Source/code-server URLs.

        CDXC:CodeEditor 2026-06-24-23:17:
        Source uses the same active-workarea-only materialization edge, with one extra predecessor: ensure the shared code-server runtime is launching or ready before asking the URL gate for a Source CefSurface. Until the app-owned runtime reaches ready, this method leaves Source on its loading/error placeholder instead of creating an about:blank or dead localhost surface.
        */
        let mut changed = self.refresh_source_code_server_runtime_child(cx);
        // A file sent to this project's Code opens in its own window, so that is the one to show.
        if let Some(pending) = self.pending_source_file_open.as_ref()
            && self.code_windows.active.is_some()
            && self
                .latest_sidebar_project_snapshot
                .as_ref()
                .and_then(|snapshot| self.source_code_server_runtime_target(snapshot))
                .is_some_and(|target| target.project_path == pending.project_path)
        {
            self.code_windows.active = None;
            self.reveal_shown_code_window_tab();
            changed = true;
        }
        if !self.project_workarea_runtime_cef_surface_is_current(self.active_code_editor_slot()) {
            changed |= self.ensure_source_code_server_runtime_for_current_context(cx);
        }
        let mut slot_keys = ProjectWorkareaCefSurfaceSlotKey::project_placeholder_slots().to_vec();
        if let Some(id) = self.code_windows.active {
            slot_keys.push(ProjectWorkareaCefSurfaceSlotKey::CodeWindow(id));
        }
        if let TitlebarMode::Extension(id) = self.active_mode {
            slot_keys.push(ProjectWorkareaCefSurfaceSlotKey::Extension(id));
        }
        if self.claude_page_on_screen() {
            slot_keys.push(ProjectWorkareaCefSurfaceSlotKey::Extension(
                crate::app::claude_web::claude_web_view_id(),
            ));
        }
        if self.active_mode == TitlebarMode::Work {
            slot_keys.push(ProjectWorkareaCefSurfaceSlotKey::Work);
        }
        for slot_key in slot_keys {
            if !self.project_workarea_runtime_cef_surface_may_be_visible(slot_key) {
                continue;
            }
            let Some(runtime_url) = self.project_workarea_runtime_url_for_slot(slot_key) else {
                continue;
            };
            if self
                .project_workarea_runtime_cef_surfaces
                .get(&slot_key)
                .is_some_and(|owned_surface| owned_surface.matches_runtime_url(&runtime_url))
            {
                continue;
            }
            if self
                .ensure_project_workarea_runtime_cef_surface(slot_key, runtime_url, cx)
                .is_some()
            {
                changed = true;
            }
        }
        changed |= self.open_pending_source_file_if_ready(cx);
        changed |= self.deliver_pending_docs_file_open(cx);
        changed |= self.deliver_pending_docs_review_open(cx);
        changed
    }

    pub(crate) fn open_pending_source_file_if_ready(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(pending) = self.pending_source_file_open.as_ref() else {
            return false;
        };
        let Some(target) = self.source_code_server_runtime.target.as_ref() else {
            return false;
        };
        if self.source_code_server_runtime.state != SourceCodeServerRuntimeLaunchState::Ready
            || target.project_path != pending.project_path
            || pending
                .remote_target
                .as_ref()
                .is_some_and(|remote| remote != target)
            || (pending.remote_target.is_none()
                && !matches!(target.endpoint, SourceCodeServerRuntimeEndpoint::Local))
            || !self.project_workarea_runtime_cef_surface_is_current(
                ProjectWorkareaCefSurfaceSlotKey::Source,
            )
        {
            return false;
        }

        let Some(pending) = self.pending_source_file_open.take() else {
            return false;
        };
        let origin = pending.origin;
        let failed_file_path = pending.file_path.to_string_lossy().into_owned();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    if let Some(target) = &pending.remote_target {
                        return crate::app::session_chat_context_menu::open_remote_source_file(
                            target,
                            &pending.file_path,
                            pending.line,
                            pending.column,
                            pending.remote_working_directory.as_deref(),
                        );
                    }
                    source_code_server_open_file_in_existing_instance(
                        &pending.file_path,
                        pending.line,
                        pending.column,
                        &pending.project_path,
                    )
                })
                .await;
            if let Err(message) = result {
                let _ = this.update(cx, |this, cx| {
                    gpui_copy_to_clipboard(ClipboardItem::new_string(failed_file_path), cx);
                    let (id, title) = match origin {
                        PendingSourceFileOpenOrigin::AgentsHub => (
                            "gpui-agents-hub-source-open-failed",
                            "Could not open Agents Hub file",
                        ),
                        PendingSourceFileOpenOrigin::SessionChat => (
                            GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID,
                            "Could not open file in VS Code view",
                        ),
                    };
                    this.upsert_gpui_app_toast(
                        GpuiAppToast {
                            copy_text: None,
                            id: id.to_string(),
                            level: GpuiAppToastLevel::from_raw(Some("warning")),
                            title: title.to_string(),
                            description: Some(format!(
                                "{message} The file path was copied to your clipboard."
                            )),
                            loading: false,
                            persistent: false,
                            duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                            action: None,
                            epoch: 0,
                        },
                        cx,
                    );
                });
            }
        })
        .detach();
        true
    }

    pub(crate) fn prune_project_workarea_runtime_cef_surfaces_for_current_gates(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let surface_slot_keys = self
            .project_workarea_runtime_cef_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let mut pruned = false;
        for slot_key in surface_slot_keys {
            let current_runtime_url = self.project_workarea_runtime_url_for_slot(slot_key);
            let surface_is_current = current_runtime_url.as_ref().is_some_and(|runtime_url| {
                self.project_workarea_runtime_cef_surfaces
                    .get(&slot_key)
                    .is_some_and(|owned_surface| owned_surface.matches_runtime_url(runtime_url))
            });
            let work_page_kept = slot_key == ProjectWorkareaCefSurfaceSlotKey::Work
                && current_runtime_url.is_none()
                && self
                    .project_workarea_runtime_cef_surfaces
                    .get(&slot_key)
                    .is_some_and(|owned_surface| self.work_view_page_kept_hidden(owned_surface));
            if surface_is_current || work_page_kept {
                continue;
            }
            let was_visible = self.project_workarea_runtime_cef_surface_may_be_visible(slot_key);
            if let Some(owned_surface) =
                self.project_workarea_runtime_cef_surfaces.remove(&slot_key)
            {
                owned_surface
                    .surface
                    .update(cx, |surface, _| surface.set_visible(false));
                self.park_project_workarea_runtime_cef_surface(
                    owned_surface,
                    slot_key,
                    was_visible,
                    cx,
                );
                pruned = true;
            }
        }
        if pruned {
            self.update_project_workarea_runtime_cef_surface_visibility(cx);
        }
        pruned
    }

    pub(crate) fn update_project_workarea_runtime_cef_surface_visibility(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Workarea 2026-06-24-10:12:
        Source, Kanban, Automate, and Manage CEF visibility is derived from the active awake workarea plus the same replacement gate used by rendering. The loop only toggles already-owned CEF child views; it does not create missing surfaces, issue URLs, synthesize fallback pages, overlap project panes, or persist/log private runtime details.
        */
        // A page's URL is built only for a page that may show: a hidden Code window costs no URL
        // work on every pass.
        for (&slot_key, owned_surface) in &self.project_workarea_runtime_cef_surfaces {
            let surface_visible = self.project_workarea_view_may_show_page(slot_key)
                && (!slot_key.is_code_editor()
                    || slot_key == self.active_code_editor_slot()
                    || self.code_window_handoff_keeps_visible(slot_key))
                && owned_surface.page_ready()
                && self
                    .project_workarea_runtime_url_for_slot(slot_key)
                    .is_some_and(|runtime_url| owned_surface.matches_runtime_url(&runtime_url));
            owned_surface.surface.update(cx, |surface, _| {
                surface.set_visible(surface_visible);
            });
        }
    }
}
