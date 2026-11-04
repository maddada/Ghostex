//! The Code view's extra VS Code windows: File > New Window, or a folder or workspace opened in a
//! new window, becomes another window tab inside the Code view, with its own page on the same
//! code-server as the project's own Code window.
//!
//! CDXC:CodeEditor 2026-10-24 WHY:
//! Several projects and workspaces stay open in VS Code at once, each in its own window, shown as macOS window tabs (`window.nativeTabs`), and VS Code's File > New Window was blocked here. Those windows become tabs inside the VS Code view, in a row above the page, rather than Browser tabs with an address bar or extra tabs in the side panel's strip, behind an advanced setting (`codeServerNewWindowsAsTabs`). The side panel keeps one VS Code tab. Each window keeps its own page while another is on screen, belongs to the project it was opened in, and closes with the VS Code tab. Only windows on the VS Code view's own code-server origin become window tabs (what happens to every other request: `code_window_popup_open_handler`).
//!
//! CDXC:CodeEditor 2026-10-24 WHY:
//! A Code window's page gets no page-metadata handler: with one, the browser layer takes Cmd+R (reload) and Cmd+Y (history) before VS Code sees them (`on_pre_key_event` in cef/shell/lifecycle.rs). Its address and title come from its own script over the page bridge instead.
//! SEE-ALSO: app/model/code_windows.rs (the windows), app/render/code_window_tabs.rs (their tabs).

use gpui::SharedString;

use crate::app::model::*;
use crate::*;

/// Every Code page while the setting is on: a window VS Code opens on its own origin gets a stand-in
/// window object, because CEF cancels the popup once it became a tab, `window.open` returns null,
/// and VS Code's workspace provider (`return !!window.open(url)`) would show "The browser blocked
/// opening a new tab or window" over the editor.
pub(crate) const CODE_WINDOW_OPEN_SHIM: &str = r#"(() => {
  if (window.top !== window || window.__ghostexCodeWindowsAsTabs) return;
  window.__ghostexCodeWindowsAsTabs = true;
  const nativeOpen = window.open;
  window.open = function (url, target, features) {
    const opened = nativeOpen.call(window, url, target, features);
    if (opened || !url) return opened;
    let sameOrigin = false;
    try { sameOrigin = new URL(String(url), location.href).origin === location.origin; } catch (_) {}
    return sameOrigin ? { closed: true, opener: null, close() {}, focus() {}, blur() {}, postMessage() {} } : opened;
  };
})();"#;

/// An extra window's page also reports where it is and what VS Code calls it, and hands
/// `window.close()` (File > Close Window) to the app, which closes its tab.
///
/// CDXC:CodeEditor 2026-10-25 WHY:
/// The script runs in every document of the page, the `about:blank` one a new page starts with included, which reported its window's address as "blank" (measured). The page's own address was then rebuilt from that, became a URL that no longer parses, and every later report left it wrong, so the page counted as stale and was replaced by a new one, again and again while the window was on screen. Only the window's own http(s) documents report. It also runs in every frame (`Page.addScriptToEvaluateOnNewDocument`), so a webview (a Markdown preview, an extension's panel) would report its iframe's address as the window's and its `window.close()` would close the tab: only the top document reports.
const CODE_WINDOW_PAGE_SCRIPT: &str = r#"(() => {
  if (window.top !== window || window.__ghostexCodeWindowPage || !/^https?:$/.test(location.protocol)) return;
  window.__ghostexCodeWindowPage = true;
  const post = (message) => { try { window.__ghostexPageBridge(JSON.stringify(message)); } catch (_) {} };
  let last = '';
  const report = () => {
    const state = { type: 'codeWindow', location: location.pathname + location.search, title: String(document.title || '').slice(0, 300) };
    const key = state.location + '\n' + state.title;
    if (key === last) return;
    last = key;
    post(state);
  };
  window.close = () => post({ type: 'codeWindowClose' });
  const watch = () => {
    report();
    new MutationObserver(report).observe(document.head || document.documentElement, { subtree: true, childList: true, characterData: true });
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', watch, { once: true });
  else watch();
})();"#;

/// The longest a switch keeps the outgoing page in front when the incoming page's view never
/// reports a new surface, and the one wait on Windows and Linux, which cannot tell.
///
/// CDXC:CodeEditor 2026-10-24 WHY:
/// Measured at ~98 fps on an M-series Mac. The page cannot say when it is on screen: it keeps drawing while hidden, so a "two animation frames later" signal from its own script came after 0–16 ms and still left empty frames. A fixed 100 ms hold left none (26 switches) but the new window then took 100 ms to appear. Watching the view's surface identity instead (`watch_code_window_handoff`) brought the new window on screen in the next frame, about 8 ms after the tab changed, with no empty frame in 6 of 6 switches; this hold is only the cap.
const CODE_WINDOW_HANDOFF_HOLD: std::time::Duration = std::time::Duration::from_millis(100);

/// How often a switch looks for the incoming page's new surface (macOS).
const CODE_WINDOW_HANDOFF_POLL: std::time::Duration = std::time::Duration::from_millis(4);

/// Once raised, the incoming page covers the outgoing one, which stays behind it this long before
/// it is hidden.
///
/// CDXC:CodeEditor 2026-10-24 WHY:
/// Hiding the outgoing page straight away, with the incoming one still behind it, showed one empty frame on 2 of 4 switches (measured): the page behind it was not on screen yet in that frame. Raising the incoming page first and hiding the outgoing one two frames later showed none.
const CODE_WINDOW_HANDOFF_RAISED: std::time::Duration = std::time::Duration::from_millis(34);

/// The saved setting, read once at launch; later changes arrive through the settings refresh
/// (`apply_code_windows_setting`).
pub(crate) fn code_windows_as_tabs_enabled() -> bool {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("codeServerNewWindowsAsTabs")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

impl GhostexGpuiApp {
    /// The setting is on and the project is local: only then does Code open extra windows (remote
    /// projects keep one Code window for now).
    pub(crate) fn code_windows_as_tabs_active(&self) -> bool {
        let remote_project = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .and_then(|project| gpui_remote_project_reference_from_project_id(&project.0))
            .is_some();
        self.code_windows_setting_on && !remote_project
    }

    /// The page the Code view shows: the project's own Code window, or the extra window picked in
    /// the strip.
    pub(crate) fn active_code_editor_slot(&self) -> ProjectWorkareaCefSurfaceSlotKey {
        self.code_windows
            .active
            .map(ProjectWorkareaCefSurfaceSlotKey::CodeWindow)
            .unwrap_or(ProjectWorkareaCefSurfaceSlotKey::Source)
    }

    /// The slot a view's page lives in right now: for Code, the window on screen.
    pub(crate) fn shown_workarea_slot(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> ProjectWorkareaCefSurfaceSlotKey {
        if slot_key.is_code_editor() {
            self.active_code_editor_slot()
        } else {
            slot_key
        }
    }

    /// The page of an extra window: the Code view's runtime origin with the window's own address.
    pub(crate) fn code_window_runtime_url(
        &self,
        code_view_url: &ProjectWorkareaRealRuntimeUrl,
        id: CodeWindowId,
    ) -> Option<ProjectWorkareaRealRuntimeUrl> {
        code_window_url_at(code_view_url, self.code_windows.window(id)?.location())
    }

    /// Where every Code page's requests for a new tab or window go: popups (`window.open`,
    /// `_blank`) and link gestures (middle-click, Cmd/Ctrl-click, "Open link in new tab"; the CEF
    /// handlers consume them once this route has them). A VS Code window on the Code view's own
    /// origin becomes a window tab while `codeServerNewWindowsAsTabs` is on and is dropped
    /// otherwise, so the page stays as it is; any other web page opens where a terminal link
    /// would: a remote project's loopback address on its machine, else the embedded Browser (a new
    /// tab, in the background for middle-click and Cmd/Ctrl-click) or the system browser, as Open
    /// links in says. Only http(s) opens: other schemes from a Code page belong to its own process
    /// (`blob:`, `data:`), and app links are handed to the OS before this
    /// (`dispatch_external_app_popup`).
    ///
    /// CDXC:CodeEditor 2026-10-25 DECISION:
    /// User (Q567): keep the new link behaviour in the VS Code view: Cmd/Ctrl-click and middle-click on a web link open it instead of nothing, even with Open VS Code windows as tabs off, and it follows Settings > Open links in (`webLinkOpenTarget`) and the project's remote machine, the same way terminal links do (`open_gpui_engine_terminal_action_url`). A VS Code window on the view's own origin becomes a window tab while that setting is on and leaves the page as it is while it is off; native sign-in popups stay off for VS Code.
    pub(crate) fn code_window_popup_open_handler(
        &self,
        code_view_url: &str,
        cx: &mut gpui::Context<Self>,
    ) -> cef::BrowserPopupOpenHandler {
        let open_in_browser = self.browser_popup_open_handler(None, None, cx);
        let is_web = |url: &url::Url| matches!(url.scheme(), "http" | "https");
        // Only an http(s) source can identify Code's own origin.
        let origin = url::Url::parse(code_view_url)
            .ok()
            .filter(is_web)
            .map(|url| url.origin());
        let windows_as_tabs = self.code_windows_as_tabs_active();
        let project_id = self.agents_workspace_project_id.clone();
        let remote_project = project_id
            .as_deref()
            .and_then(gpui_remote_project_reference_from_project_id);
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        std::rc::Rc::new(
            move |requested_url: String, placement: cef::BrowserPopupPlacement| {
                let Some(target) = url::Url::parse(&requested_url).ok().filter(is_web) else {
                    return;
                };
                let location = origin
                    .as_ref()
                    .and_then(|origin| code_window_location(&target, origin));
                let Some(location) = location else {
                    // The view's own origin (`/proxy/<port>/`) is this computer's, even for a
                    // remote project, whose editor reaches this computer through a tunnel.
                    let own_origin = origin
                        .as_ref()
                        .is_some_and(|origin| target.origin() == *origin);
                    if !own_origin
                        && let Some(remote_project) = remote_project.clone()
                        && gpui_remote_link_is_loopback(&requested_url)
                    {
                        let app = app.clone();
                        let mut async_cx = async_cx.clone();
                        foreground
                            .spawn(async move {
                                let _ = app.update_in(&mut async_cx, |this, _window, cx| {
                                    this.open_remote_machine_loopback_link(
                                        &requested_url,
                                        remote_project,
                                        cx,
                                    );
                                });
                            })
                            .detach();
                        return;
                    }
                    if !shared_settings::shared_sidebar_settings_snapshot().web_links_open_in_app()
                    {
                        let _ = gpui_open_terminal_action_url(&requested_url);
                        return;
                    }
                    open_in_browser(requested_url, placement);
                    return;
                };
                if !windows_as_tabs {
                    return;
                }
                let project_id = project_id.clone();
                let app = app.clone();
                let mut async_cx = async_cx.clone();
                foreground
                    .spawn(async move {
                        let _ = app.update_in(&mut async_cx, |this, window, cx| {
                            // A page of a project that was left asks nothing of the one on screen.
                            if this.agents_workspace_project_id != project_id {
                                return;
                            }
                            let select = matches!(placement, cef::BrowserPopupPlacement::Selected);
                            this.open_code_window(location, select, window, cx);
                        });
                    })
                    .detach();
            },
        )
    }

    /// Adds a Code window at `location` as the last tab, and shows it when `select` is set.
    pub(crate) fn open_code_window(
        &mut self,
        location: String,
        select: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.code_windows_as_tabs_active() {
            return;
        }
        if let Some(existing) = self.code_window_with_target(&location, None) {
            if select {
                self.select_code_window(existing, window, cx);
            }
            return;
        }
        let id = CodeWindowId(self.next_code_window_id);
        self.next_code_window_id += 1;
        self.code_windows
            .windows
            .push(CodeWindow::new(id, location));
        if select {
            self.select_code_window(Some(id), window, cx);
        } else {
            cx.notify();
        }
    }

    /// The window that already has `location`'s folder or workspace open, other than `except`:
    /// `Some(None)` is the project's own window.
    ///
    /// CDXC:CodeEditor 2026-10-25 WHY:
    /// A file sent to Code (`ghostex edit`, a chat link, Open file changes in VS Code) goes to the workbench rooted at the project's folder that registered last (`getCandidatesForWorkspaceFolder` in .dependencies/code-server/src/node/vscodeSocket.ts), so a second window on that folder took the file out of sight while the project's own window was shown. VS Code itself never opens a folder twice: it brings the window that has it to the front, and so does this.
    fn code_window_with_target(
        &self,
        location: &str,
        except: Option<CodeWindowId>,
    ) -> Option<Option<CodeWindowId>> {
        let target = code_window_target(location)?;
        let own = self
            .project_workarea_runtime_url_for_slot(ProjectWorkareaCefSurfaceSlotKey::Source)
            .and_then(|url| url::Url::parse(&url.value).ok())
            .and_then(|url| {
                url.query()
                    .and_then(|query| code_window_target(&format!("/?{query}")))
            });
        if own.as_ref() == Some(&target) {
            return Some(None);
        }
        self.code_windows
            .windows
            .iter()
            .find(|window| {
                Some(window.id) != except
                    && code_window_target(window.location()).as_ref() == Some(&target)
            })
            .map(|window| Some(window.id))
    }

    /// The `+` after the window tabs: a new empty window, as VS Code's File > New Window opens.
    ///
    /// CDXC:CodeEditor 2026-10-24 WHY:
    /// No VS Code Window row in the side panel's `+` menu: the `+` inside the row does it, and its tooltip says what it does.
    pub(crate) fn open_new_code_window(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.open_code_window("/?ew=true".to_string(), true, window, cx);
    }

    /// Scrolls the window tab row to the shown window's tab: the project's own is the first, each
    /// extra window follows in `code_windows.windows` order.
    pub(crate) fn reveal_shown_code_window_tab(&self) {
        let position = match self.code_windows.active {
            None => 0,
            Some(id) => match self.code_windows.windows.iter().position(|w| w.id == id) {
                Some(index) => index + 1,
                None => return,
            },
        };
        self.code_window_tab_scroll_handle.scroll_to_item(position);
    }

    /// Shows a Code window (`None` is the project's own), opening the Code view first if needed.
    pub(crate) fn select_code_window(
        &mut self,
        id: Option<CodeWindowId>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Source) {
            return;
        }
        let showing =
            (self.active_mode == TitlebarMode::Source).then(|| self.active_code_editor_slot());
        if self.active_mode != TitlebarMode::Source {
            self.open_view_tab(TitlebarMode::Source, window, cx);
        }
        self.code_windows.active = id.filter(|id| self.code_windows.window(*id).is_some());
        self.reveal_shown_code_window_tab();
        let incoming = self.active_code_editor_slot();
        if let Some(outgoing) = showing {
            self.begin_code_window_handoff(outgoing, incoming, cx);
        }
        self.show_active_code_window(window, cx);
        self.watch_code_window_handoff(cx);
    }

    /// CDXC:CodeEditor 2026-10-24 WHY:
    /// When switching windows, the outgoing page stays in front until the incoming one has drawn, at no extra CPU or GPU cost. Measured before: the outgoing page was hidden at once and the incoming one only reached the screen tens of milliseconds later, so every switch showed the panel's empty background for 25–30 ms. Now the two pages overlap on purpose for a few frames, the way a browser keeps the old tab's frame until the new one has drawn: the outgoing page is ordered to the front, the incoming one is shown behind it and raised as soon as its view holds a new surface, then the outgoing one is hidden. Only the visible page and the one it replaces are ever live; the others stay hidden as before.
    fn begin_code_window_handoff(
        &mut self,
        outgoing: ProjectWorkareaCefSurfaceSlotKey,
        incoming: ProjectWorkareaCefSurfaceSlotKey,
        cx: &mut gpui::Context<Self>,
    ) {
        self.code_window_handoff = None;
        if outgoing == incoming {
            return;
        }
        let surfaces = &self.project_workarea_runtime_cef_surfaces;
        let (Some(from), Some(to)) = (surfaces.get(&outgoing), surfaces.get(&incoming)) else {
            return;
        };
        if !from.page_ready() || !to.page_ready() {
            return;
        }
        let from = from.surface.clone();
        from.update(cx, |surface, _| surface.order_front());
        self.next_code_window_handoff_generation += 1;
        self.code_window_handoff = Some(CodeWindowHandoff {
            outgoing,
            incoming,
            generation: self.next_code_window_handoff_generation,
        });
    }

    /// Runs once the incoming page is shown behind the outgoing one: as soon as its view holds a
    /// new surface it is raised, and the outgoing page goes a moment later. Without a new surface
    /// within `CODE_WINDOW_HANDOFF_HOLD` the hold decides; where the platform cannot tell (Windows,
    /// Linux) it simply waits out the hold once.
    fn watch_code_window_handoff(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(handoff) = self.code_window_handoff else {
            return;
        };
        let generation = handoff.generation;
        let Some(surface) = self
            .project_workarea_runtime_cef_surfaces
            .get(&handoff.incoming)
            .map(|owned| owned.surface.clone())
        else {
            return;
        };
        let shown_with = surface.read(cx).content_identity();
        // The watcher must not keep a page alive that its window closed meanwhile.
        let surface = surface.downgrade();
        let started = std::time::Instant::now();
        cx.spawn(async move |this, cx| {
            if cfg!(target_os = "macos") {
                loop {
                    cx.background_executor()
                        .timer(CODE_WINDOW_HANDOFF_POLL)
                        .await;
                    // `None`: a later switch, or the page went, took over this one.
                    let Ok(Some(ready)) = this.update(cx, |this, cx| {
                        if !this.code_window_handoff_is(generation) {
                            return None;
                        }
                        let identity = surface
                            .read_with(cx, |surface, _| surface.content_identity())
                            .ok()?;
                        Some(
                            (identity != 0 && identity != shown_with)
                                || started.elapsed() >= CODE_WINDOW_HANDOFF_HOLD,
                        )
                    }) else {
                        return;
                    };
                    if ready {
                        break;
                    }
                }
            } else {
                cx.background_executor()
                    .timer(CODE_WINDOW_HANDOFF_HOLD)
                    .await;
            }
            let raised = this
                .update(cx, |this, cx| {
                    this.raise_code_window_handoff(generation, cx)
                })
                .unwrap_or(false);
            if !raised {
                return;
            }
            cx.background_executor()
                .timer(CODE_WINDOW_HANDOFF_RAISED)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_code_window_handoff(generation, cx)
            });
        })
        .detach();
    }

    fn code_window_handoff_is(&self, generation: u64) -> bool {
        self.code_window_handoff
            .is_some_and(|handoff| handoff.generation == generation)
    }

    /// The incoming page goes in front of the outgoing one before that is hidden; false when a
    /// later switch took over.
    fn raise_code_window_handoff(&mut self, generation: u64, cx: &mut gpui::Context<Self>) -> bool {
        let Some(handoff) = self
            .code_window_handoff
            .filter(|h| h.generation == generation)
        else {
            return false;
        };
        if let Some(owned) = self
            .project_workarea_runtime_cef_surfaces
            .get(&handoff.incoming)
        {
            owned.surface.update(cx, |surface, _| surface.order_front());
        }
        true
    }

    fn finish_code_window_handoff(&mut self, generation: u64, cx: &mut gpui::Context<Self>) {
        if self.code_window_handoff_is(generation) {
            self.code_window_handoff = None;
            self.update_project_workarea_runtime_cef_surface_visibility(cx);
            cx.notify();
        }
    }

    /// The outgoing page of a switch still in progress stays on screen.
    pub(crate) fn code_window_handoff_keeps_visible(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> bool {
        self.code_window_handoff.is_some_and(|handoff| {
            handoff.outgoing == slot_key
                && self.active_mode == TitlebarMode::Source
                && handoff.incoming == self.active_code_editor_slot()
        })
    }

    fn show_active_code_window(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        self.update_project_workarea_runtime_cef_surface_visibility(cx);
        self.focus_project_editor_surface_for_keyboard(TitlebarMode::Source, window, cx);
        cx.notify();
    }

    pub(crate) fn close_code_window(
        &mut self,
        id: CodeWindowId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let was_showing =
            self.active_mode == TitlebarMode::Source && self.code_windows.active == Some(id);
        if self.remove_code_window(id, cx) && was_showing {
            self.focus_project_editor_surface_for_keyboard(TitlebarMode::Source, window, cx);
        }
    }

    /// Closes the window and shows the one that takes over; false when it was already gone.
    fn remove_code_window(&mut self, id: CodeWindowId, cx: &mut gpui::Context<Self>) -> bool {
        if !self.code_windows.remove(id) {
            return false;
        }
        self.forget_code_window(id, cx);
        self.reveal_shown_code_window_tab();
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        self.update_project_workarea_runtime_cef_surface_visibility(cx);
        cx.notify();
        true
    }

    /// Closes a window's page wherever it is: among the pages on screen, or parked.
    ///
    /// CDXC:CodeEditor 2026-10-25 WHY:
    /// Where a window's page lives does not follow which project is on screen. A page parked when its project was left can stay parked after the project is back, when the return showed another window (`ghostex edit` on a project file opens the project's own Code window), so a window of the project on screen can still have its page parked. Closing it only among the pages on screen left that page running until the keep-alive window ran out.
    fn forget_code_window(&mut self, id: CodeWindowId, cx: &mut gpui::Context<Self>) {
        self.remove_project_workarea_runtime_cef_surface(
            ProjectWorkareaCefSurfaceSlotKey::CodeWindow(id),
            cx,
        );
        self.release_parked_code_window_page(id);
    }

    /// The page already went to `location` itself, so it stays the page for its new address
    /// instead of being rebuilt when its window is shown, wherever it is: on screen or parked.
    fn follow_code_window_page(&mut self, id: CodeWindowId, location: &str) {
        let slot = ProjectWorkareaCefSurfaceSlotKey::CodeWindow(id);
        if let Some(owned) = self.project_workarea_runtime_cef_surfaces.get_mut(&slot) {
            if let Some(runtime_url) = code_window_url_at(&owned.runtime_url, location) {
                owned.runtime_url = runtime_url;
            }
        } else if let Some(parked) = self.parked_code_window_page_mut(id)
            && let Some(runtime_url) = code_window_url_at(&parked.owned.runtime_url, location)
        {
            parked.owned.runtime_url = runtime_url;
        }
    }

    /// Turning `codeServerNewWindowsAsTabs` on or off: off closes every project's extra windows and
    /// shows each project's own Code window again; either way the Code pages are built again for
    /// the new window rule (`code_window_popup_open_handler`), while code-server and its terminals
    /// keep running.
    ///
    /// CDXC:CodeEditor 2026-10-25 WHY:
    /// The setting used to sit among the code-server launch options, so changing it restarted the editor and its terminals, and turning it off only hid the window tabs: a window that was on screen, or saved as a left project's selection, stayed on screen with no row to get back to the project's own window. It is a page option, so only the pages are rebuilt.
    pub(crate) fn apply_code_windows_setting(&mut self, on: bool, cx: &mut gpui::Context<Self>) {
        if !on {
            self.code_windows = CodeWindows::default();
            for state in self.project_view_states_by_project.values_mut() {
                state.code_windows = CodeWindows::default();
            }
        }
        self.remove_code_editor_surfaces(cx);
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        self.update_project_workarea_runtime_cef_surface_visibility(cx);
        cx.notify();
    }

    /// Releases the Code view's pages, its own window's and every extra window's, parked ones
    /// included, as stopping or losing the editor does: they all belong to the server that went.
    pub(crate) fn remove_code_editor_surfaces(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        self.code_window_handoff = None;
        let before = self.project_workarea_runtime_cef_surfaces.len();
        let parked_removed = self.release_parked_code_editor_pages();
        self.project_workarea_runtime_cef_surfaces
            .retain(|slot, owned| {
                if !slot.is_code_editor() {
                    return true;
                }
                owned
                    .surface
                    .update(cx, |surface, _| surface.set_visible(false));
                false
            });
        let removed = parked_removed || self.project_workarea_runtime_cef_surfaces.len() != before;
        if removed {
            self.update_project_workarea_runtime_cef_surface_visibility(cx);
        }
        removed
    }

    /// Runs once a Code page was created: the stand-in `window.open` for every Code page, and for
    /// an extra window the script that reports its address and title.
    pub(crate) fn prepare_code_editor_page(
        &mut self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        surface: &Entity<CefSurface>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.code_windows_as_tabs_active() {
            return;
        }
        let ProjectWorkareaCefSurfaceSlotKey::CodeWindow(id) = slot_key else {
            surface.read(cx).add_page_script(CODE_WINDOW_OPEN_SHIM);
            return;
        };
        let script = format!("{CODE_WINDOW_OPEN_SHIM}\n{CODE_WINDOW_PAGE_SCRIPT}");
        let Some(mut bridge) = CefSurface::install_page_bridge(surface, &script, cx) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            while let Some(message) = bridge.next().await {
                let Ok(message) = serde_json::from_str::<serde_json::Value>(&message) else {
                    continue;
                };
                let alive = this
                    .update(cx, |this, cx| {
                        this.receive_code_window_page_message(id, &message, cx);
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    /// Closes window `id` of the project on screen from its own page (File > Close Window, or it
    /// opened a folder another window has). When `id` was the one on screen, `next` is shown
    /// instead (`Some(None)` the project's own), or with `None` the window before it.
    ///
    /// CDXC:CodeEditor 2026-10-25 WHY:
    /// The page that had the keys is gone and showing the next one does not focus it, so without this the keys went nowhere until a click, unlike the tab's close control. The message arrives with no GPUI window, so the handoff runs on the next render; a window that did not have the keys leaves them where they are.
    fn hand_code_window_to(
        &mut self,
        id: CodeWindowId,
        next: Option<Option<CodeWindowId>>,
        cx: &mut gpui::Context<Self>,
    ) {
        let showing = self.code_windows.active == Some(id);
        let had_keys = showing
            && self.active_mode == TitlebarMode::Source
            && self.shell_focus == ShellFocusTarget::ProjectEditorSurface(TitlebarMode::Source);
        if showing && let Some(next) = next {
            self.code_windows.active = next;
        }
        if self.remove_code_window(id, cx) && had_keys {
            self.focus_shell_target(
                ShellFocusTarget::ProjectEditorSurface(TitlebarMode::Source),
                cx,
            );
            cx.notify();
        }
    }

    /// The windows `id` belongs to: the project on screen, or a project that was left (its saved
    /// view state), whose parked page can still report.
    fn code_windows_owning_mut(&mut self, id: CodeWindowId) -> Option<&mut CodeWindows> {
        if self.code_windows.window(id).is_some() {
            return Some(&mut self.code_windows);
        }
        self.project_view_states_by_project
            .values_mut()
            .map(|state| &mut state.code_windows)
            .find(|windows| windows.window(id).is_some())
    }

    /// A page's messages go to the window it was made for, whichever project is on screen.
    fn receive_code_window_page_message(
        &mut self,
        id: CodeWindowId,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let current_project = self.code_windows.window(id).is_some();
        match message["type"].as_str() {
            Some("codeWindow") => {
                // A window's address is a path on the Code view's own origin.
                let location = message["location"]
                    .as_str()
                    .filter(|location| location.starts_with('/'));
                let title = message["title"]
                    .as_str()
                    .map(str::trim)
                    .filter(|title| !title.is_empty());
                let Some(window) = self
                    .code_windows_owning_mut(id)
                    .and_then(|windows| windows.window_mut(id))
                else {
                    return;
                };
                let moved = location.is_some_and(|location| window.set_location(location));
                let retitled = window.title.as_deref() != title;
                if retitled {
                    window.title = title.map(|title| SharedString::from(title.to_owned()));
                }
                if let Some(location) = location.filter(|_| moved) {
                    // A window that opened a folder another window has: that window takes over.
                    if current_project
                        && let Some(existing) = self.code_window_with_target(location, Some(id))
                    {
                        self.hand_code_window_to(id, Some(existing), cx);
                        return;
                    }
                    self.follow_code_window_page(id, location);
                }
                // Another project's window changes nothing on screen.
                if current_project && (moved || retitled) {
                    cx.notify();
                }
            }
            // File > Close Window.
            Some("codeWindowClose") if current_project => {
                self.hand_code_window_to(id, None, cx);
            }
            Some("codeWindowClose") => {
                if let Some(windows) = self.code_windows_owning_mut(id) {
                    windows.remove(id);
                }
                self.forget_code_window(id, cx);
                cx.notify();
            }
            _ => {}
        }
    }
}

/// `runtime_url`'s origin with a Code window's address.
fn code_window_url_at(
    runtime_url: &ProjectWorkareaRealRuntimeUrl,
    location: &str,
) -> Option<ProjectWorkareaRealRuntimeUrl> {
    let origin = url::Url::parse(&runtime_url.value)
        .ok()?
        .origin()
        .ascii_serialization();
    ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(format!("{origin}{location}"))
}
