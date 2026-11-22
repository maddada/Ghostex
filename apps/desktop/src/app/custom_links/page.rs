//! The page a link opens in: the built-in view the takeover shows in the sessions column's place
//! (claude_web/takeover.rs, `WebPlace::Link`), what the page reports back (its address, loading,
//! Back and Forward, its icon), and the address bar above it.
//!
//! CDXC:Browser 2026-11-22 WHY:
//! The link page is not a Browser tab: it belongs to no project, so it cannot live in a project's browser model (`browser_tabs`), and its bar is drawn here in the side panel Browser's look (render_browser_toolbar: Back, Forward, Reload, the lock or globe, the address field) rather than reusing that toolbar, which is built around a project's Browser pane. The bar is always shown, as the user asked; the address field takes any address `normalize_custom_link_url` reads (`localhost:8317` included) and Escape puts the page's own address back.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{AnyElement, Entity, FontWeight, IntoElement, MouseDownEvent, Styled as _, Window, px};
use gpui_component::Sizable as _;
use gpui_component::Size as ComponentSize;
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::json;

use super::store::*;
use crate::app::claude_web::WebPlace;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

/// What the open page last reported.
#[derive(Clone, Debug, Default)]
struct PageState {
    address: String,
    loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
    favicon_url: Option<String>,
    /// The address field has the keyboard: the page's own address waits until it is let go.
    editing: bool,
}

thread_local! {
    static PAGE: RefCell<PageState> = RefCell::default();
    static ADDRESS_INPUT: RefCell<Option<(Entity<InputState>, gpui::Subscription)>> =
        const { RefCell::new(None) };
}

fn with_page<T>(f: impl FnOnce(&mut PageState) -> T) -> T {
    PAGE.with(|page| f(&mut page.borrow_mut()))
}

fn address_input() -> Option<Entity<InputState>> {
    ADDRESS_INPUT.with(|input| input.borrow().as_ref().map(|(input, _)| input.clone()))
}

fn slot() -> ProjectWorkareaCefSurfaceSlotKey {
    ProjectWorkareaCefSurfaceSlotKey::Extension(WebPlace::Link.view_id())
}

/// The link page as the built-in view list carries it, opening on the picked link. Off while no
/// link is picked, so nothing routes to an empty page.
pub(crate) fn custom_link_web_view() -> GpuiCustomView {
    let link = custom_link_page_link();
    GpuiCustomView {
        id: WebPlace::Link.view_id(),
        title: link
            .as_ref()
            .map_or_else(|| "Link".to_string(), |link| link.label()),
        enabled: link.is_some(),
        url: link.map(|link| link.url).unwrap_or_default(),
        definition: json!({"id": LINK_WEB_VIEW_ID, "name": "Link", "availability": "all"}),
    }
}

impl GhostexGpuiApp {
    fn custom_link_surface(&self) -> Option<Entity<CefSurface>> {
        self.project_workarea_runtime_cef_surfaces
            .get(&slot())
            .map(|entry| entry.surface.clone())
    }

    /// Points the page at `url`: the open page navigates there; one put away is dropped, so it is
    /// made again on the new address when it next shows.
    pub(crate) fn load_custom_link_page(&mut self, url: &str, cx: &mut gpui::Context<Self>) {
        with_page(|page| {
            page.address = url.to_string();
            page.editing = false;
            page.favicon_url = None;
        });
        if let Some(surface) = self.custom_link_surface() {
            surface.update(cx, |surface, _| surface.load_url(url));
        } else {
            self.parked_project_workarea_surfaces
                .retain(|parked| parked.slot_key != slot());
        }
    }

    /// Closes the page of a removed link.
    pub(crate) fn forget_custom_link_page(&mut self, cx: &mut gpui::Context<Self>) {
        self.remove_project_workarea_runtime_cef_surface(slot(), cx);
        self.parked_project_workarea_surfaces
            .retain(|parked| parked.slot_key != slot());
        with_page(|page| *page = PageState::default());
    }

    /// What the page tells the bar: its address, loading, Back and Forward, its icon.
    pub(crate) fn custom_link_page_metadata_handler(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> cef::BrowserPageMetadataHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        Rc::new(move |event: cef::BrowserPageMetadataEvent| {
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    let _ = app.update_in(&mut async_cx, |this, window, cx| {
                        this.handle_custom_link_page_event(event, window, cx);
                    });
                })
                .detach();
        })
    }

    fn handle_custom_link_page_event(
        &mut self,
        event: cef::BrowserPageMetadataEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match event {
            cef::BrowserPageMetadataEvent::AddressChanged(url) => {
                let editing = with_page(|page| {
                    page.address = url.clone();
                    page.editing
                });
                if !editing {
                    self.set_custom_link_address_text(&url, window, cx);
                }
            }
            cef::BrowserPageMetadataEvent::LoadingStateChanged {
                is_loading,
                can_go_back,
                can_go_forward,
            } => with_page(|page| {
                page.loading = is_loading;
                page.can_go_back = can_go_back;
                page.can_go_forward = can_go_forward;
            }),
            cef::BrowserPageMetadataEvent::FaviconUrlChanged(url) => {
                with_page(|page| page.favicon_url = url);
            }
            cef::BrowserPageMetadataEvent::CopyToClipboard(item) => {
                gpui_copy_to_clipboard(item, cx);
            }
            _ => return,
        }
        cx.notify();
    }

    fn set_custom_link_address_text(
        &self,
        text: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(input) = address_input()
            && input.read(cx).value().as_ref() != text
        {
            let text = text.to_string();
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }

    /// Makes the address field the first time the page shows (a text field needs the window).
    pub(crate) fn ensure_custom_link_address_input(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let address = with_page(|page| {
            if page.address.is_empty() {
                page.address = custom_link_page_link()
                    .map(|link| link.url)
                    .unwrap_or_default();
            }
            page.address.clone()
        });
        if address_input().is_some() {
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Enter an address")
                .default_value(&address)
        });
        let subscription = cx.subscribe(
            &input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::Focus => {
                    this.drop_pending_browser_keyboard_handoff();
                    this.reclaim_gpui_root_for_chrome_input_focus();
                    with_page(|page| page.editing = true);
                }
                InputEvent::Change => with_page(|page| page.editing = true),
                InputEvent::Blur => with_page(|page| page.editing = false),
                InputEvent::PressEnter { .. } => {
                    let typed = input.read(cx).value().to_string();
                    if let Some(url) =
                        ghostex_gx_core::custom_links::normalize_custom_link_url(&typed)
                    {
                        this.load_custom_link_page(&url, cx);
                        this.focus_shell_target(
                            ShellFocusTarget::ProjectEditorSurface(WebPlace::Link.mode()),
                            cx,
                        );
                        cx.notify();
                    }
                }
            },
        );
        ADDRESS_INPUT.with(|slot| *slot.borrow_mut() = Some((input, subscription)));
    }

    fn focus_custom_link_address(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(input) = address_input() else {
            return;
        };
        self.drop_pending_browser_keyboard_handoff();
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        cef::focus_gpui_root_view(self.parent_ns_view);
        with_page(|page| page.editing = true);
        input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Escape in the address field: the page's own address comes back and the page takes the keys.
    fn cancel_custom_link_address_edit(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let address = with_page(|page| {
            page.editing = false;
            page.address.clone()
        });
        self.set_custom_link_address_text(&address, window, cx);
        self.focus_shell_target(
            ShellFocusTarget::ProjectEditorSurface(WebPlace::Link.mode()),
            cx,
        );
        cx.notify();
    }

    /// The bar above the link page: Back, Forward, Reload, the address field, and Close, in the
    /// side panel Browser's toolbar look (helpers/titlebar/custom_link_bar_look.rs).
    pub(crate) fn render_custom_link_bar(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let page = with_page(|page| page.clone());
        let surface = self.custom_link_surface();
        let state = CustomLinkBarState {
            can_go_back: page.can_go_back
                || surface
                    .as_ref()
                    .is_some_and(|surface| surface.read(cx).can_go_back()),
            can_go_forward: page.can_go_forward
                || surface
                    .as_ref()
                    .is_some_and(|surface| surface.read(cx).can_go_forward()),
            loading: page.loading,
        };
        let loading = state.loading;
        // The page's icon, fetched into the Browser's per-site cache so the menu row shows it too
        // (custom_links/store.rs); the lock or globe until there is one.
        let favicon = page
            .favicon_url
            .as_deref()
            .and_then(browser_safe_http_favicon_parts)
            .map(|(_, mut source)| {
                source.cache_key = browser_favicon_cache_key(&page.address, None);
                source
            });
        let cached = browser_favicon_cache_lookup(&page.address, None);
        let site_icon = if favicon.is_some() || cached.is_some() {
            browser_favicon_element(14.0, cached.as_ref(), favicon.as_ref())
        } else {
            titlebar_svg_icon(
                browser_security_icon_path(&page.address),
                14.0,
                browser_toolbar_security_icon_color(),
            )
            .into_any_element()
        };
        let address = address_input().map(|input| {
            Input::new(&input)
                .with_size(ComponentSize::XSmall)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .w_full()
                .px(px(0.0))
                .py(px(0.0))
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .line_height(px(BROWSER_ADDRESS_HEIGHT))
                .text_color(browser_toolbar_text_color())
                .into_any_element()
        });
        let actions = CustomLinkBarActions {
            back: Box::new(cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                if let Some(surface) = this.custom_link_surface() {
                    surface.update(cx, |surface, _| surface.go_back());
                }
            })),
            forward: Box::new(cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                if let Some(surface) = this.custom_link_surface() {
                    surface.update(cx, |surface, _| surface.go_forward());
                }
            })),
            reload: Box::new(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                match this.custom_link_surface() {
                    Some(surface) if loading => {
                        surface.update(cx, |surface, _| surface.stop_load())
                    }
                    Some(surface) => surface.update(cx, |surface, _| surface.reload()),
                    None => this.reload_titlebar_view(WebPlace::Link.mode(), window, cx),
                }
            })),
            close: Box::new(cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                this.turn_claude_off(cx);
            })),
            address: Box::new(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                this.focus_custom_link_address(window, cx);
            })),
            address_key: Box::new(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key.as_str() == "escape" {
                    cx.stop_propagation();
                    this.cancel_custom_link_address_edit(window, cx);
                }
            })),
        };
        custom_link_bar(state, site_icon, address, actions)
    }
}
