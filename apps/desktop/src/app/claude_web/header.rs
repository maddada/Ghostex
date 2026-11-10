//! The work area header while Claude is on: Claude is its own place, so the header drops what
//! names or acts on the project and session from before Claude, and with the sidebar hidden it
//! draws the sidebar's own top row where the sidebar draws it (claude_web/takeover.rs).

use gpui::AnyElement;
use gpui::IntoElement;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt as _;
use gpui_component::h_flex;

use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::*;

/// A control of the work area header, for whether it stays while Claude is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeaderControl {
    /// The header's leading half: Back and Forward through the sessions visited, and the project →
    /// session breadcrumb with the project icon.
    Breadcrumb,
    /// The app's Update button.
    Update,
    /// Titlebar-pinned extension buttons, which act on the project.
    PinnedExtensions,
    /// Prompt Editor or Exit Focus, for the focused session.
    PromptEditorOrExitFocus,
    /// The Claude and Grok Bot buttons, left of Voice.
    WebBots,
    /// The Voice button.
    Voice,
    /// Start (the project's Actions).
    Actions,
    /// Open In for the project.
    OpenIn,
    /// Commit and the Git menu.
    Commit,
    /// A Claude cloud session's button.
    ClaudeCloud,
    /// The context ring (Session details).
    SessionDetails,
    /// The trailing ⋯ menu (Ask Ghostex, Tips, Resources, Dev servers, Extensions).
    MoreMenu,
    /// The Commands and side panel toggles.
    PanelToggles,
}

impl HeaderControl {
    /// CDXC:Extensions 2026-10-29 WHY:
    /// Claude is its own place (the user's "consider claude a new space+project"), so what names or acts on the previous project or session leaves the header while Claude is on, and what is the app's own stays: the Update button, Voice, the ⋯ menu, and the panel toggles the user opens the Browser beside Claude with. The header shows no title: the Claude button right there shows as on and the account bar under the header names the account, so a "Claude" label would only repeat them.
    pub(crate) fn shown_with_claude(self) -> bool {
        match self {
            Self::Update | Self::WebBots | Self::Voice | Self::MoreMenu | Self::PanelToggles => {
                true
            }
            Self::Breadcrumb
            | Self::PinnedExtensions
            | Self::PromptEditorOrExitFocus
            | Self::Actions
            | Self::OpenIn
            | Self::Commit
            | Self::ClaudeCloud
            | Self::SessionDetails => false,
        }
    }
}

impl GhostexGpuiApp {
    /// Whether the header draws `control` now.
    pub(crate) fn header_control_shown(&self, control: HeaderControl) -> bool {
        !self.claude_on() || control.shown_with_claude()
    }

    /// The sidebar's top row stands in for the hidden sidebar: Claude is on and nothing else draws
    /// that row (the peek draws its own in its window).
    fn claude_header_draws_sidebar_row(&self) -> bool {
        self.claude_on() && self.sidebar_collapsed
    }

    /// The header's leading half while Claude is on, in place of the breadcrumb.
    ///
    /// CDXC:Extensions 2026-10-29 WHY:
    /// The header decision of 2026-10-29 is in claude_web/takeover.rs. With the sidebar hidden the header draws the sidebar's own top row (`render_native_sidebar_row`, as the docked sidebar owns the window corner) in a box as wide as the sidebar, starting where the sidebar starts, with the sidebar's appearance and scale, so every button keeps its x, look and behaviour on every OS (on Windows and Linux there are no traffic lights to clear, exactly as in the sidebar). Drawing a copy of the buttons instead would drift from the sidebar's row with every change to it.
    pub(crate) fn render_claude_header_leading(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let row = self
            .claude_header_draws_sidebar_row()
            .then(|| self.native_sidebar.snapshot.as_ref())
            .flatten()
            .map(|snapshot| {
                let appearance = SidebarAppearance::from_hud(&snapshot.hud, window);
                self.render_native_sidebar_row(&appearance, false, 0, true, cx)
            });
        let sidebar_width = self.sidebar_width;
        let view = cx.entity();
        h_flex()
            .h_full()
            .flex_1()
            .min_w_0()
            .items_start()
            .when_some(row, |this, row| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .w(px(sidebar_width))
                        .child(row)
                        // The sidebar's menu opens from this row too; it measures against the
                        // sidebar's frame, which is this box down the window's height.
                        .on_prepaint(move |bounds, window, cx| {
                            let frame = gpui::Bounds {
                                origin: bounds.origin,
                                size: gpui::size(px(sidebar_width), window.viewport_size().height),
                            };
                            view.update(cx, |app, _| {
                                if app.floating_reveal.panel.is_none() {
                                    app.native_sidebar.bounds = frame;
                                }
                            });
                        }),
                )
            })
            .when(
                (self.update_available || self.update_downloading)
                    && self.header_control_shown(HeaderControl::Update),
                |this| {
                    this.child(
                        div()
                            .h(px(TITLEBAR_CONTROL_HEIGHT))
                            .flex()
                            .items_center()
                            .child(self.render_titlebar_update_button(cx)),
                    )
                },
            )
            .into_any_element()
    }

    /// The sidebar's menu, opened from the header's copy of its top row while the sidebar is
    /// hidden. The sidebar draws it in its own frame; here it is drawn at that frame's origin
    /// (`native_sidebar.bounds`, kept by `render_claude_header_leading`).
    pub(crate) fn render_claude_sidebar_menu(
        &self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if !self.claude_header_draws_sidebar_row() || self.floating_reveal.panel.is_some() {
            return None;
        }
        let frame = self.native_sidebar.bounds;
        let menu = self.render_native_sidebar_menu(window, cx)?;
        Some(
            div()
                .absolute()
                .left(frame.origin.x)
                .top(frame.origin.y)
                .size_0()
                .child(menu)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_drops_what_belongs_to_the_previous_project_and_session() {
        for control in [
            HeaderControl::Breadcrumb,
            HeaderControl::PinnedExtensions,
            HeaderControl::PromptEditorOrExitFocus,
            HeaderControl::Actions,
            HeaderControl::OpenIn,
            HeaderControl::Commit,
            HeaderControl::ClaudeCloud,
            HeaderControl::SessionDetails,
        ] {
            assert!(!control.shown_with_claude(), "{control:?}");
        }
    }

    #[test]
    fn the_header_keeps_the_apps_own_controls_and_the_panel_toggles() {
        for control in [
            HeaderControl::Update,
            HeaderControl::WebBots,
            HeaderControl::Voice,
            HeaderControl::MoreMenu,
            HeaderControl::PanelToggles,
        ] {
            assert!(control.shown_with_claude(), "{control:?}");
        }
    }
}
