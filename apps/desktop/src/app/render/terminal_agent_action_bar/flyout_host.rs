// The ⋯ menu's flyouts (Switch Account's accounts or agents, and Skills): where each is drawn and
// in which skin.
//
// CDXC:AppModal 2026-10-29 DECISION:
// The user answered "Yes, as a separate task" to moving the terminal bar's account and skills
// flyouts into blurred windows like the other menus. Under window glass (where the frosted hosts
// run: macOS and Windows) a flyout draws in a frosted host window of its own
// (`FrostedHostKind::TerminalBarFlyout`) with the popover's fill (`popup_window_surface`), the
// frosted menus' faint outline and ink-wash hover, and the frosted surfaces' 1.25x blur. The bar's
// own window keeps an invisible copy at the same place and size, so the flyout's placement, its
// occlusion of the terminal under it and the ⋯ anchor's click-away bounds stay exactly as before.
// Glass off, and on Linux, the flyout draws beside the menu in the bar's window as before.
// Then the user approved moving the ⋯ menu itself the same way, so under glass the menu and its
// flyouts match: the menu draws in a frosted host of its own (`FrostedHostKind::TerminalBarMenu`)
// with the same fill, outline, hover and blur, over an invisible copy that keeps its place above
// the ⋯ button, its width, its click-away bounds and the flyouts' placement beside it. A host whose
// menu stopped being drawn (its pane went away while it was open) is taken down at the next render
// of that window (`drop_unseen_terminal_bar_hosts`).
// SEE-ALSO: window/frosted_host.rs, helpers/window_glass.rs `frosted_dialog_fill`.

use std::rc::Rc;

use gpui::AnyElement;
use gpui::AnyWindowHandle;
use gpui::BoxShadow;
use gpui::Hsla;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::ParentElement as _;
use gpui::Rgba;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use serde_json::Value;

use super::palette::*;
use super::{
    TERMINAL_AGENT_BAR_ACCOUNT_SUBMENU_GAP, TERMINAL_AGENT_BAR_FLYOUT_WINDOW_MARGIN,
    TerminalAgentBarSurface, TerminalBarPopupBounds,
};
use crate::app::helpers::{
    popup_window_surface, titlebar_popup_menu_border_color, titlebar_popup_menu_hover_color,
};
use crate::app::model::*;
use crate::app::window::frosted_host::{
    FrostedContent, FrostedHostKind, TERMINAL_BAR_FLYOUT_HOST_RADIUS, frosted_hosting_active,
    hide_frosted_host, hide_frosted_host_over, show_frosted_host,
};
use crate::*;

/// What the frosted host needs to draw the open flyout over the bar's window.
#[derive(Clone)]
pub(super) struct TerminalBarHostRequest {
    app: gpui::Entity<GhostexGpuiApp>,
    surface: TerminalAgentBarSurface,
    session_id: TerminalSessionId,
    suffix: String,
}

/// Where a flyout is drawn.
pub(super) enum FlyoutPlace<'a> {
    /// Beside the ⋯ menu in the bar's own window. `host` is set under window glass: the copy here
    /// is then invisible and only holds the flyout's place, and the frosted host draws it.
    Inline {
        menu_width: f32,
        painted_menu: gpui::Bounds<gpui::Pixels>,
        window_width: gpui::Pixels,
        bounds: &'a TerminalBarPopupBounds,
        host: Option<TerminalBarHostRequest>,
    },
    /// Filling a frosted host window over the bar's window `parent`.
    Hosted { parent: AnyWindowHandle },
}

impl FlyoutPlace<'_> {
    /// The bar's window when the flyout draws in a frosted host.
    pub(super) fn hosted_parent(&self) -> Option<AnyWindowHandle> {
        match self {
            Self::Hosted { parent } => Some(*parent),
            Self::Inline { .. } => None,
        }
    }

    /// The row hover: the bar menu's solid hover, the frosted menus' ink wash in a host.
    pub(super) fn hover(&self) -> Hsla {
        match self {
            Self::Hosted { .. } => titlebar_popup_menu_hover_color(),
            Self::Inline { .. } => terminal_agent_bar_hover_background().into(),
        }
    }

    /// A separator line in the flyout.
    pub(super) fn separator(&self) -> AnyElement {
        div()
            .h(px(1.0))
            .mx(px(4.0))
            .my(px(5.0))
            .bg(match self {
                Self::Hosted { .. } => titlebar_popup_menu_border_color(),
                Self::Inline { .. } => terminal_agent_bar_menu_separator_color().into(),
            })
            .into_any_element()
    }

    /// The flyout's panel, `width` wide: beside the menu, opening left when the window has no room
    /// on the right, or filling its frosted host window.
    pub(super) fn frame(&self, id: String, width: f32) -> gpui::Stateful<gpui::Div> {
        let panel = div()
            .id(id)
            .font_family(crate::ui_fonts::UI_FONT)
            .flex()
            .flex_col()
            .p(px(5.0))
            .rounded(px(TERMINAL_BAR_FLYOUT_HOST_RADIUS))
            .border_1()
            .occlude();
        match self {
            Self::Hosted { .. } => panel
                .size_full()
                .border_color(titlebar_popup_menu_border_color())
                .bg(popup_window_surface(
                    terminal_agent_bar_menu_background().into(),
                )),
            Self::Inline {
                menu_width,
                painted_menu,
                window_width,
                bounds,
                host,
            } => {
                let offset = px(menu_width + TERMINAL_AGENT_BAR_ACCOUNT_SUBMENU_GAP);
                // Nothing is measured on the frame the menu first opens, and the flyout cannot be
                // open yet, so it starts on the right.
                let opens_left = painted_menu.size.width > px(0.0)
                    && painted_menu.right() + px(TERMINAL_AGENT_BAR_ACCOUNT_SUBMENU_GAP + width)
                        > *window_width - px(TERMINAL_AGENT_BAR_FLYOUT_WINDOW_MARGIN);
                panel
                    .absolute()
                    .when(opens_left, |this| this.right(offset))
                    .when(!opens_left, |this| this.left(offset))
                    .bottom_0()
                    .w(px(width))
                    .border_color(terminal_agent_bar_menu_border())
                    .bg(terminal_agent_bar_menu_background())
                    .shadow(vec![
                        BoxShadow::new(
                            px(0.0),
                            px(10.0),
                            Rgba {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 0.45,
                            }
                            .into(),
                        )
                        .blur_radius(px(22.0)),
                    ])
                    .child(super::terminal_agent_bar_popup_bounds_probe(bounds))
                    .when_some(host.clone(), |this, host| {
                        this.opacity(0.0).child(host_probe(
                            FrostedHostKind::TerminalBarFlyout,
                            host,
                            render_hosted_flyout,
                        ))
                    })
            }
        }
    }
}

/// Draws a ⋯ menu surface in its frosted host over the bar's window `parent`.
type HostedRender = fn(
    &GhostexGpuiApp,
    &TerminalBarHostRequest,
    AnyWindowHandle,
    &mut gpui::Context<GhostexGpuiApp>,
) -> Option<AnyElement>;

/// An invisible child of a placeholder (the ⋯ menu's or a flyout's) that shows `kind`'s frosted
/// host over the placeholder's painted frame, in the bar's window coordinates.
fn host_probe(
    kind: FrostedHostKind,
    request: TerminalBarHostRequest,
    render: HostedRender,
) -> AnyElement {
    gpui::canvas(
        move |painted, window, cx| {
            let parent = gpui::Window::window_handle(window);
            let app = request.app.clone();
            let key = Some(app.entity_id());
            let content: FrostedContent = Rc::new(move |_, cx| {
                let request = request.clone();
                request.app.clone().update(cx, |app, cx| {
                    render(app, &request, parent, cx).unwrap_or_else(|| div().into_any_element())
                })
            });
            show_frosted_host(kind, parent, painted, key, content, Some(app), cx);
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
    .into_any_element()
}

fn render_hosted_flyout(
    app: &GhostexGpuiApp,
    request: &TerminalBarHostRequest,
    parent: AnyWindowHandle,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) -> Option<AnyElement> {
    app.render_terminal_agent_bar_flyout(
        request.surface,
        request.session_id,
        &FlyoutPlace::Hosted { parent },
        &request.suffix,
        cx,
    )
}

fn render_hosted_menu(
    app: &GhostexGpuiApp,
    request: &TerminalBarHostRequest,
    parent: AnyWindowHandle,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) -> Option<AnyElement> {
    if app.agents_terminal_action_bar_menu_session != Some(request.session_id) {
        return None;
    }
    Some(
        div()
            .size_full()
            .font_family(crate::ui_fonts::UI_FONT)
            .flex()
            .flex_col()
            .p(px(5.0))
            .rounded(px(TERMINAL_BAR_FLYOUT_HOST_RADIUS))
            .border_1()
            .border_color(titlebar_popup_menu_border_color())
            .bg(popup_window_surface(
                terminal_agent_bar_menu_background().into(),
            ))
            .occlude()
            .children(app.render_terminal_agent_bar_menu_rows(
                request.surface,
                request.session_id,
                &request.suffix,
                Some(parent),
                cx,
            ))
            .into_any_element(),
    )
}

/// Under window glass, the request that makes the ⋯ menu's placeholder show the menu's frosted
/// host; `None` (and the host hidden) otherwise.
pub(super) fn terminal_bar_menu_host_probe(
    surface: TerminalAgentBarSurface,
    session_id: TerminalSessionId,
    suffix: &str,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) -> Option<AnyElement> {
    if !frosted_hosting_active() {
        hide_frosted_host(FrostedHostKind::TerminalBarMenu, cx);
        return None;
    }
    // Noted as the menu is built, not when it prepaints: a frame that composites the pane's layer
    // skips its prepaint, and the menu must not count as gone then.
    let drawer = cx.entity_id();
    MENU_HOSTED_BY.with(|drawn| drawn.borrow_mut().push(drawer));
    Some(host_probe(
        FrostedHostKind::TerminalBarMenu,
        TerminalBarHostRequest {
            app: cx.entity(),
            surface,
            session_id,
            suffix: suffix.to_string(),
        },
        render_hosted_menu,
    ))
}

thread_local! {
    /// The app entities (one per workspace window) whose ⋯ menu showed its frosted host since
    /// that entity's last render began.
    static MENU_HOSTED_BY: std::cell::RefCell<Vec<gpui::EntityId>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

/// Called as a workspace window's render begins: takes down the ⋯ menu's and its flyout's frosted
/// hosts over that window when its menu did not show them in the previous render, because the
/// menu's pane went away or stopped showing the bar (a view or session switch from the keyboard)
/// while it was open. Drawn inline the menu simply vanished with its pane; a host window would
/// stay on screen.
pub(crate) fn drop_unseen_terminal_bar_hosts(
    window: &gpui::Window,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    let drawer = cx.entity_id();
    let seen = MENU_HOSTED_BY.with(|drawn| {
        let mut drawn = drawn.borrow_mut();
        let seen = drawn.contains(&drawer);
        drawn.retain(|id| *id != drawer);
        seen
    });
    if !seen {
        let parent = gpui::Window::window_handle(window);
        hide_frosted_host_over(FrostedHostKind::TerminalBarMenu, parent, cx);
        hide_frosted_host_over(FrostedHostKind::TerminalBarFlyout, parent, cx);
    }
}

/// Hides the flyout's frosted host, if one is up.
pub(super) fn hide_terminal_bar_flyout_host(cx: &mut gpui::App) {
    hide_frosted_host(FrostedHostKind::TerminalBarFlyout, cx);
}

/// Hides the ⋯ menu's and its flyout's frosted hosts, if any is up.
pub(super) fn hide_terminal_bar_hosts(cx: &mut gpui::App) {
    hide_frosted_host(FrostedHostKind::TerminalBarMenu, cx);
    hide_terminal_bar_flyout_host(cx);
}

impl GhostexGpuiApp {
    /// The ⋯ menu's open flyout, beside the menu in the bar's window. Under window glass it is
    /// an invisible placeholder there and the frosted host draws it; otherwise the host is hidden.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_terminal_agent_bar_inline_flyout(
        &self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        menu_width: f32,
        painted_menu: gpui::Bounds<gpui::Pixels>,
        window_width: gpui::Pixels,
        bounds: &TerminalBarPopupBounds,
        suffix: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let host = frosted_hosting_active().then(|| TerminalBarHostRequest {
            app: cx.entity(),
            surface,
            session_id,
            suffix: suffix.to_string(),
        });
        let hosted = host.is_some();
        let place = FlyoutPlace::Inline {
            menu_width,
            painted_menu,
            window_width,
            bounds,
            host,
        };
        let flyout = self.render_terminal_agent_bar_flyout(surface, session_id, &place, suffix, cx);
        if flyout.is_none() || !hosted {
            hide_terminal_bar_flyout_host(cx);
        }
        flyout
    }

    /// The ⋯ menu's open flyout at `place`: Switch Account's accounts (Claude and Codex) or
    /// agents (the daemon's switchable-agent rows), or Skills.
    pub(super) fn render_terminal_agent_bar_flyout(
        &self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        place: &FlyoutPlace,
        suffix: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if self.agents_terminal_action_bar_menu_session != Some(session_id) {
            return None;
        }
        if self.agents_terminal_action_bar_account_submenu_open {
            if self.terminal_agent_bar_account_target(session_id).is_some() {
                let (_, rows): (String, Vec<Value>) =
                    self.agents_terminal_action_bar_account_page.clone()?;
                return Some(self.render_terminal_agent_bar_account_page(&rows, place, suffix, cx));
            }
            let switchable_agents = self
                .agents_sidebar_session_for_terminal(session_id)
                .map(|session| session.switchable_agents.clone())
                .unwrap_or_default();
            if !switchable_agents.is_empty() {
                return Some(self.render_terminal_agent_bar_account_submenu(
                    session_id,
                    &switchable_agents,
                    place,
                    suffix,
                    cx,
                ));
            }
        }
        let rows = self.agents_terminal_action_bar_skills.as_deref()?;
        Some(
            self.render_terminal_agent_bar_skills_flyout(
                surface, session_id, rows, place, suffix, cx,
            ),
        )
    }
}

/// Runs a flyout row's action with the bar's window: the window it was clicked in, or, for a
/// flyout drawn in a frosted host, the bar's own window, once the click's handling is over.
pub(super) fn run_in_terminal_bar_window(
    app: &mut GhostexGpuiApp,
    hosted_parent: Option<AnyWindowHandle>,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GhostexGpuiApp>,
    run: impl FnOnce(&mut GhostexGpuiApp, &mut gpui::Window, &mut gpui::Context<GhostexGpuiApp>)
    + 'static,
) {
    let Some(parent) = hosted_parent else {
        run(app, window, cx);
        return;
    };
    let entity = cx.entity();
    cx.defer(move |cx| {
        let _ = parent.update(cx, |_, window, cx| {
            entity.update(cx, |app, cx| run(app, window, cx));
        });
    });
}
