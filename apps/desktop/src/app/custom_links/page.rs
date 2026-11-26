//! The page a link opens in: the built-in view the takeover shows in the sessions column's place
//! (claude_web/takeover.rs, `WebPlace::Link`), what the page reports back (its address, loading,
//! Back and Forward, its icon), and the address bar above it.
//!
//! CDXC:Browser 2026-11-22 DECISION:
//! User: the link view's address bar has "the same full toolbar as the side panel Browser" (Back, Forward, Reload, Home, the lock and address, then the device view, the element picker, History, Profile, the annotate tool and the ⋯ menu), reusing the Browser's toolbar and its actions, with Close kept. The link page is still not a Browser tab: it belongs to no project, so it cannot live in a project's browser model (`browser_tabs`, swapped and parked with the project). So the Browser's own `render_browser_toolbar` is drawn for `CUSTOM_LINK_PAGE_PANE`, a pane id no Browser pane has, and each place the toolbar reads a pane or runs an action hands that id to the link page: its address, loading and history from what the page reports, its surface, Home as the link's own address, History opening a visit here, Profile choosing which Browser profile the page opens in (or the links' own, `CUSTOM_LINK_BROWSER_PROFILE`, kept in `custom-link-page.json`), the Page Size frame and user agent (this run only), and the ⋯ menu's Open in External Browser. The address field is the link page's own (it takes `localhost:8317`, `normalize_custom_link_url`); Escape puts the page's own address back. Supersedes the 2026-11-22 WHY that drew a reduced bar of its own.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, Entity, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::Sizable as _;
use gpui_component::Size as ComponentSize;
use gpui_component::WindowExt as _;
use gpui_component::h_flex;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use serde_json::json;

use super::store::*;
use crate::app::claude_web::WebPlace;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

/// The pane id the Browser toolbar is drawn for on the link page: no Browser pane has it, and the
/// toolbar's buttons, popups and actions that read a pane hand this one to the link page
/// (`run_custom_link_toolbar_action` and the `CUSTOM_LINK_PAGE_PANE` branches beside them).
pub(crate) const CUSTOM_LINK_PAGE_PANE: BrowserPaneId = BrowserPaneId(u64::MAX);

/// The Profile menu's "Links' own profile" row (`CUSTOM_LINK_BROWSER_PROFILE`).
pub(crate) const CUSTOM_LINK_OWN_PROFILE: u64 = u64::MAX;

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
    device_view: BrowserDeviceView,
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
    pub(crate) fn custom_link_surface(&self) -> Option<Entity<CefSurface>> {
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

    /// The bar above the link page: the side panel Browser's own toolbar
    /// (titlebar/icon_button_and_browser_toolbar.rs) for the link page's pane id, with Close.
    pub(crate) fn render_custom_link_bar(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        self.render_browser_toolbar(CUSTOM_LINK_PAGE_PANE, cx)
            .into_any_element()
    }

    /// What the toolbar shows of the page: its address, loading, Back and Forward.
    pub(crate) fn custom_link_toolbar_page(&self) -> (String, bool, bool, bool) {
        with_page(|page| {
            (
                page.address.clone(),
                page.loading,
                page.can_go_back,
                page.can_go_forward,
            )
        })
    }

    /// The page's phone or tablet preview (the toolbar's Page Size menu), for this run of the app.
    pub(crate) fn custom_link_device_view(&self) -> BrowserDeviceView {
        with_page(|page| page.device_view)
    }

    pub(crate) fn set_custom_link_device_view(
        &mut self,
        device_view: BrowserDeviceView,
        cx: &mut gpui::Context<Self>,
    ) {
        let previous = with_page(|page| std::mem::replace(&mut page.device_view, device_view));
        if previous.user_agent() != device_view.user_agent()
            && let Some(surface) = self.custom_link_surface()
        {
            surface.update(cx, |surface, _| {
                surface.set_mobile_user_agent(device_view.user_agent())
            });
        }
        cx.notify();
    }

    /// The page in its phone or tablet frame while one is picked, as a Browser pane frames it.
    pub(crate) fn custom_link_framed_page(&self, page: AnyElement) -> AnyElement {
        let device_view = self.custom_link_device_view();
        match device_view.frame_size() {
            Some(frame_size) => {
                crate::app::render::browser_body_and_tabs::render_browser_device_frame(
                    CUSTOM_LINK_PAGE_PANE,
                    device_view,
                    frame_size,
                    page,
                )
            }
            None => page,
        }
    }

    /// A toolbar button pressed on the link page: the Browser pane's action, on the link's page.
    pub(crate) fn run_custom_link_toolbar_action(
        &mut self,
        action: BrowserToolbarAction,
        trigger_bounds: Option<gpui::Bounds<gpui::Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let surface = self.custom_link_surface();
        match action {
            BrowserToolbarAction::Back
            | BrowserToolbarAction::Forward
            | BrowserToolbarAction::Reload
            | BrowserToolbarAction::StopLoading
            | BrowserToolbarAction::ResetZoom => match surface {
                Some(surface) => surface.update(cx, |surface, _| match action {
                    BrowserToolbarAction::Back => surface.go_back(),
                    BrowserToolbarAction::Forward => surface.go_forward(),
                    BrowserToolbarAction::Reload => surface.reload(),
                    BrowserToolbarAction::StopLoading => surface.stop_load(),
                    _ => surface.reset_zoom(),
                }),
                None if matches!(action, BrowserToolbarAction::Reload) => {
                    self.reload_titlebar_view(WebPlace::Link.mode(), window, cx)
                }
                None => {}
            },
            // Home is the link's own address.
            BrowserToolbarAction::Home => {
                if let Some(link) = custom_link_page_link() {
                    self.load_custom_link_page(&link.url, cx);
                    self.set_custom_link_address_text(&link.url, window, cx);
                }
            }
            BrowserToolbarAction::FeedbackTool => {
                let (address, ..) = self.custom_link_toolbar_page();
                if browser_feedback_tool_unavailable_url(&address) {
                    return;
                }
                let injected = surface.is_some_and(|surface| {
                    let script = browser_agentation_feedback_injection_script();
                    surface.update(cx, |surface, _| {
                        surface.inject_feedback_tool_script(&script)
                    })
                });
                if !injected {
                    window.push_notification(
                        gpui_component::notification::Notification::warning(format!(
                            "{} feedback is not ready on this page.",
                            BROWSER_FEEDBACK_TOOL_AGENTATION_LABEL
                        )),
                        cx,
                    );
                }
            }
            BrowserToolbarAction::HistoryMenu => {
                self.show_browser_history_popup(CUSTOM_LINK_PAGE_PANE, window, cx)
            }
            BrowserToolbarAction::ProfileMenu => {
                if let Some(bounds) = trigger_bounds {
                    self.show_custom_link_profile_menu(bounds, window, cx);
                }
            }
            BrowserToolbarAction::ResetMediaPermissions => {}
            BrowserToolbarAction::OpenExternally => self.open_custom_link_page_externally(),
        }
        cx.notify();
    }

    /// The Profile button's menu: the links' own profile, every Browser profile, New Profile….
    fn show_custom_link_profile_menu(
        &mut self,
        trigger_bounds: gpui::Bounds<gpui::Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let selected = custom_link_page_profile();
        let mut menu = crate::app::context_menu::GpuiContextMenu::new().menu_with_check(
            "Links' own profile",
            selected.is_none(),
            Box::new(SelectBrowserProfile {
                pane_id: CUSTOM_LINK_PAGE_PANE.0,
                profile_id: CUSTOM_LINK_OWN_PROFILE,
            }),
        );
        for profile_id in self.browser_profiles.profile_ids() {
            menu = menu.menu_with_check(
                profile_id.display_label(),
                selected == Some(profile_id),
                Box::new(SelectBrowserProfile {
                    pane_id: CUSTOM_LINK_PAGE_PANE.0,
                    profile_id: profile_id.0,
                }),
            );
        }
        menu.separator()
            .menu(
                "New Profile...",
                Box::new(CreateBrowserProfile {
                    pane_id: CUSTOM_LINK_PAGE_PANE.0,
                }),
            )
            .toggle_below(trigger_bounds, window, cx);
    }

    /// A profile picked from that menu (`CUSTOM_LINK_OWN_PROFILE`: the links' own): the page is
    /// made again in it, on the address it showed.
    pub(crate) fn select_custom_link_profile(
        &mut self,
        profile_id: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        let profile =
            (profile_id != CUSTOM_LINK_OWN_PROFILE).then_some(BrowserProfileId(profile_id));
        if profile.is_some_and(|profile| !self.browser_profiles.contains_profile(profile))
            || profile == custom_link_page_profile()
        {
            return;
        }
        set_custom_link_page_profile(profile);
        let (address, ..) = self.custom_link_toolbar_page();
        self.remove_project_workarea_runtime_cef_surface(slot(), cx);
        self.parked_project_workarea_surfaces
            .retain(|parked| parked.slot_key != slot());
        with_page(|page| page.address = address);
        cx.notify();
    }

    /// New Profile… on the link page: a new Browser profile, which the page then opens in.
    pub(crate) fn create_custom_link_profile(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(profile_id) = self.browser_profiles.create_generated_profile() else {
            window.push_notification(
                gpui_component::notification::Notification::warning(
                    "Browser profile limit reached.",
                ),
                cx,
            );
            return;
        };
        self.persist_shell_layout_state();
        self.select_custom_link_profile(profile_id.0, cx);
    }

    /// The CEF profile the link page opens in.
    pub(crate) fn custom_link_cef_profile(&self) -> String {
        match custom_link_page_profile() {
            Some(profile) if self.browser_profiles.contains_profile(profile) => {
                self.browser_tab_cef_profile(profile)
            }
            _ => CUSTOM_LINK_BROWSER_PROFILE.to_string(),
        }
    }

    /// A visit picked in the History window opened from the link page's toolbar: it opens there.
    pub(crate) fn open_custom_link_history_visit(
        &mut self,
        url: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.close_gpui_browser_history_modal(cx);
        self.load_custom_link_page(&url, cx);
        self.set_custom_link_address_text(&url, window, cx);
        cx.notify();
    }

    /// The toolbar's Open Externally and the ⋯ menu's Open in External Browser on the link page.
    pub(crate) fn open_custom_link_page_externally(&mut self) {
        let (address, ..) = self.custom_link_toolbar_page();
        if !address.is_empty() {
            let _ = gpui_open_external_http_url(&address);
        }
    }

    /// The address field: the page's icon (or the lock or globe), then the address, as the
    /// Browser's (`render_browser_address_field`), on the link page's own text field.
    pub(crate) fn render_custom_link_address_field(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let page = with_page(|page| page.clone());
        // The page's icon, fetched into the Browser's per-site cache so the menu row and a rail
        // pin show it too (custom_links/store.rs); the lock or globe until there is one.
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
        });
        h_flex()
            .id("ghostex-gpui-link-address")
            .flex_1()
            .min_w(px(BROWSER_ADDRESS_MINIMUM_WIDTH))
            .h(px(BROWSER_ADDRESS_HEIGHT))
            .items_center()
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    this.focus_custom_link_address(window, cx);
                }),
            )
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key.as_str() == "escape" {
                    cx.stop_propagation();
                    this.cancel_custom_link_address_edit(window, cx);
                }
            }))
            .child(site_icon)
            .child(
                div()
                    .ml(px(8.0))
                    .flex_1()
                    .min_w_0()
                    .h(px(BROWSER_ADDRESS_HEIGHT))
                    .overflow_hidden()
                    .children(address),
            )
            .into_any_element()
    }

    /// Close, after the toolbar's ⋯: the link page goes and the sessions come back.
    pub(crate) fn render_custom_link_close_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id("ghostex-gpui-link-close")
            .role(gpui::Role::Button)
            .aria_label("Close")
            .flex()
            .flex_shrink_0()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(BROWSER_TOOLBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .rounded(px(TITLEBAR_BUTTON_RADIUS))
            .cursor_default()
            .hover(|button| button.bg(titlebar_button_hover_color()))
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_X,
                BROWSER_TOOLBAR_BUTTON_ICON_SIZE,
                titlebar_icon_color(),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.turn_claude_off(cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                titlebar_tooltip("Close", window, cx)
            })
            .into_any_element()
    }
}
