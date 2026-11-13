use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, Hsla, Image, IntoElement, ParentElement, Pixels, Styled, Window, div,
    img, px,
};

use super::{appearance::SidebarAppearance, model::NativeSidebarSession};
use crate::app::{consts::*, helpers::*};

#[derive(Clone)]
pub(crate) enum RowDragIdentity {
    Project {
        image: Option<Arc<Image>>,
        show_icon: bool,
    },
    Collection {
        color: Hsla,
        background: Hsla,
    },
    Session {
        session: Arc<NativeSidebarSession>,
    },
}

#[derive(Clone)]
pub(crate) struct RowDragPreview {
    pub(crate) identity: RowDragIdentity,
    pub(crate) appearance: SidebarAppearance,
    pub(crate) width: Pixels,
    pub(crate) pointer_x: Pixels,
}

/// CDXC:Sidebar 2026-10-08 DECISION:
/// User: "when i drag a project header it's showing below my cursor which is wrong" (2026-10-06) and "when i drag a session card in the sidebar it's not aligned with my cursor pls fix". Every dragged row sits under the pointer exactly where it was grabbed; a project or collection row keeps its column, a session follows the pointer both ways so it can be carried onto a pane. This supersedes the 2026-10-01 rule that hung a dragged session 26px below and 14px right of the pointer to keep the drop line clear: the session's fill is see-through instead, so the line beside the pointer still shows through it.
/// The see-through look is `drag_ghost`'s, which every other drag ghost in the app shares.

impl RowDragPreview {
    pub(crate) fn render(&self, title: &str, window: &Window) -> AnyElement {
        let appearance = &self.appearance;
        let scale = appearance.scale;
        let session_backing = drag_ghost_fill();
        let row = div()
            .flex()
            .items_center()
            .w(self.width)
            .min_w_0()
            .font_family(crate::ui_fonts::UI_FONT)
            .font_weight(FontWeight::LIGHT)
            .text_size(px(DRAG_GHOST_TEXT_SIZE * scale))
            .text_color(appearance.foreground);
        let row = match &self.identity {
            // The header box's own insets, so the icon and name stay where they were grabbed.
            RowDragIdentity::Project { image, show_icon } => row
                .h(px(30.0 * scale))
                .pl(px((5.0
                    + super::project_header::PROJECT_HEADER_CHEVRON_GUTTER)
                    * scale))
                .pr(px(6.0 * scale))
                .gap(px(10.0 * scale))
                .rounded(px(5.0 * scale))
                .bg(appearance.hover)
                .when(*show_icon, |row| {
                    row.child(match image {
                        Some(image) => img(image.clone())
                            .size(px(16.0 * scale))
                            .flex_shrink_0()
                            .into_any_element(),
                        None => titlebar_svg_icon(
                            TITLEBAR_ICON_FOLDER_OPEN,
                            16.0 * scale,
                            appearance.muted,
                        )
                        .into_any_element(),
                    })
                }),
            RowDragIdentity::Collection { color, background } => row
                .h(px(30.0 * scale))
                .px(px(8.0 * scale))
                .gap(px(5.0 * scale))
                .border_l_2()
                .border_color(*color)
                .bg(*background),
            // A work-mode card with links is two lines tall; the preview keeps that height, with the
            // icon and title centred on line 1 where they were grabbed.
            RowDragIdentity::Session { session } => row
                .h(px(
                    super::session_list::session_card_height(session, false) * scale
                ))
                .pb(px((super::session_list::session_card_height(
                    session, false,
                ) - super::session_list::SESSION_HEIGHT)
                    * scale))
                // The card's own insets, so the icon and title stay where they were grabbed.
                .pl(px((5.0
                    + super::threads::thread_depth(session)
                        * super::threads::THREAD_INDENT)
                    * scale))
                .pr(px(6.0 * scale))
                .gap(px(6.0 * scale))
                .rounded(px(DRAG_GHOST_RADIUS * scale))
                .bg(session_backing)
                .opacity(DRAG_GHOST_OPACITY)
                .when(session.is_focused, |row| {
                    row.bg(session_backing.blend(appearance.session_selected))
                })
                .when(session.is_visible && !session.is_focused, |row| {
                    row.bg(session_backing.blend(appearance.visible))
                })
                .when(session.is_visible || session.is_focused, |row| {
                    row.text_color(chrome_color(0xd8d8d8, 0x292929))
                })
                .child(
                    div()
                        .size(px(15.0 * scale))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(super::icons::session_drag_icon(session, appearance)),
                ),
        };
        // GPUI draws the preview with its origin at the pointer minus the grab offset, so a row with
        // no padding around it sits exactly where it was grabbed.
        let lock_x = !matches!(self.identity, RowDragIdentity::Session { .. });
        div()
            .relative()
            .when(lock_x, |wrapper| {
                wrapper.left(self.pointer_x - window.mouse_position().x)
            })
            .child(row.child(div().flex_1().min_w_0().truncate().child(title.to_owned())))
            .into_any_element()
    }
}
