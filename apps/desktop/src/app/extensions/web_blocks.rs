//! A sidebar extension's `web` blocks on the desktop: each is a CEF page from the extension's own
//! folder (`/ext/<id>/<path>`, served by gxserver), in its own exact frame inside the block, never
//! an overlay over the sidebar.
//!
//! A page is made only once its block is fully on screen, shows only while it is (GPUI cannot clip
//! a native child view, so a block scrolled half out of the list shows its placeholder instead),
//! and is closed by Auto Sleep once it has been hidden for the Project Auto Sleep time (the sweep in
//! app/web_page_sleep.rs). Which blocks are on screen is learned every frame: the root's first
//! child clears the set ([`GhostexGpuiApp::extension_web_blocks_frame_start`]), each block's marker
//! adds itself when painted whole, and the root's last child hides every page that was not
//! ([`GhostexGpuiApp::extension_web_blocks_frame_end`]).
//!
//! The page talks through the extension bridge (cef/sidebar_bridge_manifest.rs): `page.session`
//! hands it the token its extension's pages call `/api/ext/v1` with, so `ghostex.sidebar.*`,
//! `ghostex.projects()`, `ghostex.sessions()` and `ghostex.on(…)` are the extension API's own calls
//! with the permissions the user granted.
//!
//! CDXC:Extensions 2026-11-14 DECISION:
//! Q23 (native blocks plus a web block), phase 4's brief: "On the desktop it's a CEF page from the extension's own files. It's created only while it's on screen and closed by Auto Sleep, with no CEF page alive while it's hidden", and the page is "its own exact frame, not a transparent overlay over the sidebar". The phone and the GPUI web build show an Open on computer row in its place.
//! SEE-ALSO: native_sidebar/extension_panel/web.rs (the block), server/src/extensions/sidebar.rs (`sidebar_page_file`, `sidebar_page_token`).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::*;

/// Which page a block shows: one per block of each extension's surface (and session).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WebBlockKey {
    pub(crate) extension_id: String,
    pub(crate) surface: &'static str,
    pub(crate) session: Option<String>,
    pub(crate) block_id: String,
}

impl WebBlockKey {
    fn live_key(&self) -> (String, &'static str, String) {
        (
            self.extension_id.clone(),
            self.surface,
            self.block_id.clone(),
        )
    }
}

struct WebBlockPage {
    surface: Entity<CefSurface>,
    url: String,
}

/// What this frame painted: the blocks fully on screen, and the ones among them with no page yet.
#[derive(Default)]
struct WebBlockFrame {
    window: Option<gpui::AnyWindowHandle>,
    painted: HashSet<WebBlockKey>,
    wanted: Vec<(WebBlockKey, String)>,
    /// The pages shown now.
    shown: HashSet<WebBlockKey>,
}

#[derive(Default)]
pub(crate) struct ExtensionWebBlocks {
    pages: HashMap<WebBlockKey, WebBlockPage>,
    frame: Rc<RefCell<WebBlockFrame>>,
    creating: HashSet<WebBlockKey>,
    failed: HashMap<WebBlockKey, String>,
}

/// A page's frame may sit this far past the clip and still count as whole.
const CLIP_TOLERANCE: f32 = 0.5;

impl GhostexGpuiApp {
    /// The root's first child: a new frame starts with no block painted.
    pub(crate) fn extension_web_blocks_frame_start(&self) -> impl IntoElement {
        let frame = self.extension_web_blocks.frame.clone();
        gpui::canvas(
            move |_, window, _| {
                let mut frame = frame.borrow_mut();
                frame.window = Some(gpui::Window::window_handle(window));
                frame.painted.clear();
                frame.wanted.clear();
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0()
    }

    /// The root's last child: hides every page whose block was not painted whole this frame, and
    /// makes the pages of blocks that came on screen.
    pub(crate) fn extension_web_blocks_frame_end(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let frame = self.extension_web_blocks.frame.clone();
        let surfaces: Vec<(WebBlockKey, Entity<CefSurface>)> = self
            .extension_web_blocks
            .pages
            .iter()
            .map(|(key, page)| (key.clone(), page.surface.clone()))
            .collect();
        let app = cx.entity().downgrade();
        gpui::canvas(
            move |_, window, cx| {
                let mut frame = frame.borrow_mut();
                let mut changed = false;
                for (key, surface) in &surfaces {
                    if !frame.painted.contains(key) && frame.shown.remove(key) {
                        surface.update(cx, |surface, _| surface.set_visible(false));
                        changed = true;
                    }
                }
                let wanted = std::mem::take(&mut frame.wanted);
                drop(frame);
                if changed {
                    window.refresh();
                }
                if !wanted.is_empty() {
                    cx.defer(move |cx| {
                        let _ =
                            app.update(cx, |app, cx| app.create_extension_web_blocks(wanted, cx));
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0()
    }

    /// A web block's frame: its page once made, else a quiet placeholder (with why, when the page
    /// could not be made).
    pub(crate) fn render_extension_web_block(
        &self,
        key: WebBlockKey,
        url: String,
        height: Pixels,
        appearance: &crate::app::native_sidebar::appearance::SidebarAppearance,
    ) -> AnyElement {
        let page = self
            .extension_web_blocks
            .pages
            .get(&key)
            .filter(|page| page.url == url)
            .map(|page| page.surface.clone());
        let failed = self.extension_web_blocks.failed.get(&key).cloned();
        let frame = self.extension_web_blocks.frame.clone();
        let marker_key = key.clone();
        let marker_page = page.clone();
        let marker_url = url.clone();
        let marker = gpui::canvas(
            move |bounds, window, cx| {
                let mut frame = frame.borrow_mut();
                // Only the main window holds the pages (their native views live there).
                if frame.window != Some(gpui::Window::window_handle(window)) {
                    return;
                }
                let mask = window.content_mask().bounds;
                let tolerance = px(CLIP_TOLERANCE);
                let whole = bounds.size.width > px(1.0)
                    && bounds.size.height > px(1.0)
                    && bounds.origin.x >= mask.origin.x - tolerance
                    && bounds.origin.y >= mask.origin.y - tolerance
                    && bounds.right() <= mask.right() + tolerance
                    && bounds.bottom() <= mask.bottom() + tolerance;
                if !whole {
                    return;
                }
                frame.painted.insert(marker_key.clone());
                match &marker_page {
                    Some(surface) => {
                        if frame.shown.insert(marker_key.clone()) {
                            surface.update(cx, |surface, _| surface.set_visible(true));
                        }
                    }
                    None => frame.wanted.push((marker_key.clone(), marker_url.clone())),
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let scale = appearance.scale;
        div()
            .relative()
            .w_full()
            .h(height)
            .rounded(px(6.0 * scale))
            .overflow_hidden()
            .bg(appearance.foreground.opacity(0.04))
            // The marker first, so the page's own paint below already knows whether to show.
            .child(marker)
            .when_some(page, |frame, surface| {
                frame.child(div().absolute().size_full().child(surface))
            })
            .when_some(failed, |frame, error| {
                frame.child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .px(px(8.0 * scale))
                        .text_size(px(11.5 * scale))
                        .text_color(appearance.muted)
                        .child(error),
                )
            })
            .into_any_element()
    }

    fn create_extension_web_blocks(
        &mut self,
        wanted: Vec<(WebBlockKey, String)>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !cef::context_initialized() {
            // Made on the first frame after the web runtime is up.
            if crate::app::helpers::web_runtime::web_runtime_available() {
                cef::request_runtime();
            } else {
                for (key, _) in wanted {
                    self.extension_web_blocks.failed.insert(
                        key,
                        "Install the web runtime in Settings to show this.".to_string(),
                    );
                }
                cx.notify();
            }
            return;
        }
        for (key, url) in wanted {
            if self.extension_web_blocks.creating.contains(&key)
                || self
                    .extension_web_blocks
                    .pages
                    .get(&key)
                    .is_some_and(|page| page.url == url)
            {
                continue;
            }
            let bridge = self
                .extensions_snapshot
                .installed
                .get(&key.extension_id)
                .and_then(|extension| extension.bridge_surface_spec_for_url(&url));
            let Some(bridge) = bridge else {
                self.extension_web_blocks
                    .failed
                    .insert(key, "This page can't be shown.".to_string());
                continue;
            };
            let handler = self.extension_web_block_bridge_handler(key.clone(), cx);
            let result = CefSurface::try_new_extension(
                format!(
                    "ghostex-gpui-extension-web-{}-{}-{}",
                    key.extension_id, key.surface, key.block_id
                ),
                self.parent_ns_view,
                url.clone(),
                format!("sidebar-extension-{}", key.extension_id),
                pane_prepaint_background_color(),
                false,
                gpui::transparent_black(),
                false,
                bridge,
                handler,
                None,
                cx,
            );
            match result {
                Ok(surface) => {
                    surface.update(cx, |surface, _| surface.show_when_placed());
                    self.extension_web_blocks.failed.remove(&key);
                    self.extension_web_blocks
                        .frame
                        .borrow_mut()
                        .shown
                        .remove(&key);
                    if let Some(old) = self
                        .extension_web_blocks
                        .pages
                        .insert(key, WebBlockPage { surface, url })
                    {
                        old.surface
                            .update(cx, |surface, _| surface.set_visible(false));
                    }
                }
                // A first try can fail while the runtime's profile starts; the next frame retries.
                Err(_) => {}
            }
        }
        cx.notify();
    }

    fn extension_web_block_bridge_handler(
        &self,
        key: WebBlockKey,
        cx: &mut gpui::Context<Self>,
    ) -> cef::ExtensionBridgeEventHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        Rc::new(move |event: cef::ExtensionBridgeEvent| {
            let app = app.clone();
            let key = key.clone();
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    let _ = app.update_in(&mut async_cx, |this, _window, cx| {
                        let response_app = cx.entity().downgrade();
                        let response_async_cx = cx.to_async();
                        let response_foreground = cx.foreground_executor().clone();
                        let responder: GpuiExtensionBridgeResponder = Rc::new(move |payload| {
                            let response_app = response_app.clone();
                            let key = key.clone();
                            let mut response_async_cx = response_async_cx.clone();
                            response_foreground
                                .spawn(async move {
                                    let _ = response_app.update_in(
                                        &mut response_async_cx,
                                        |this, _window, cx| {
                                            let Some(surface) = this
                                                .extension_web_blocks
                                                .pages
                                                .get(&key)
                                                .map(|page| page.surface.clone())
                                            else {
                                                return;
                                            };
                                            surface.update(cx, |surface, _| {
                                                surface.dispatch_extension_bridge_message(&payload);
                                            });
                                        },
                                    );
                                })
                                .detach();
                        });
                        this.handle_extension_bridge_event(
                            event,
                            this.extension_surface_context(GpuiExtensionPlacement::View),
                            responder,
                            None,
                            cx,
                        );
                    });
                })
                .detach();
        })
    }

    /// Auto Sleep for web blocks: closes the pages hidden for `limit`, and those whose block is
    /// gone (the panel changed, the extension was turned off). Whether anything closed.
    pub(crate) fn sleep_hidden_extension_web_blocks(
        &mut self,
        limit: Option<std::time::Duration>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let live = self.extension_web_block_keys();
        let due: Vec<WebBlockKey> = self
            .extension_web_blocks
            .pages
            .iter()
            .filter(|(key, page)| {
                !live.contains(&key.live_key())
                    || limit.is_some_and(|limit| {
                        page.surface
                            .read(cx)
                            .hidden_for()
                            .is_some_and(|hidden| hidden >= limit)
                    })
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in &due {
            if let Some(page) = self.extension_web_blocks.pages.remove(key) {
                page.surface
                    .update(cx, |surface, _| surface.set_visible(false));
            }
            self.extension_web_blocks
                .frame
                .borrow_mut()
                .shown
                .remove(key);
        }
        self.extension_web_blocks
            .failed
            .retain(|key, _| live.contains(&key.live_key()));
        !due.is_empty()
    }

    /// Every web block the panels have now. A per-session side panel shows its template's blocks
    /// for a session with no document of its own, so the session is not part of this.
    fn extension_web_block_keys(&self) -> HashSet<(String, &'static str, String)> {
        use ghostex_gx_protocol::extension_panel::{PanelBlockBody, PanelDocument};
        let mut keys = HashSet::new();
        let Some(panels) = self.native_extension_panels() else {
            return keys;
        };
        let mut add = |id: &str, surface: &'static str, document: &PanelDocument| {
            for block in &document.blocks {
                if matches!(block.body, PanelBlockBody::Web { .. }) {
                    keys.insert((id.to_string(), surface, block.id.clone()));
                }
            }
        };
        for (id, entry) in &panels.0 {
            if let Some(sidebar) = &entry.sidebar {
                add(id, "sidebar", &sidebar.document);
            }
            if let Some(side_panel) = &entry.side_panel {
                add(id, "sidePanel", &side_panel.document);
                for panel in side_panel.sessions.values() {
                    add(id, "sidePanel", &panel.document);
                }
            }
        }
        keys
    }
}
