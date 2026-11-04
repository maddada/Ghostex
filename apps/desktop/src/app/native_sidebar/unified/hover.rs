//! What a hovered Unified card shows where its status was: its hover strip on a card, Unsnooze or
//! Unpark on a shelf's one-line row.
//!
//! CDXC:Sidebar 2026-10-30 DECISION:
//! User: a snoozed row offers Unsnooze and a parked row Unpark where its time was.

use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    px,
};
use gpui_component::h_flex;
use serde_json::{Value, json};

use ghostex_gx_core::unified_sidebar::UnifiedShelf;

use super::model::UnifiedCard;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::actions::NativeSidebarAction;
use crate::app::native_sidebar::appearance::SidebarAppearance;

/// A session command as the session menu's rows send it.
fn session_command(message: Value) -> Value {
    json!({"type": "command", "message": message})
}

impl GhostexGpuiApp {
    /// One muted button: an icon, and a word when it has one.
    fn unified_hover_button(
        id: String,
        icon: &'static str,
        word: Option<&'static str>,
        label: &'static str,
        appearance: &SidebarAppearance,
    ) -> gpui::Stateful<gpui::Div> {
        let scale = appearance.scale;
        let hover_color = appearance.foreground;
        h_flex()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .flex_shrink_0()
            .h(px(20.0 * scale))
            .px(px(6.0 * scale))
            .gap(px(4.0 * scale))
            .rounded(px(5.0 * scale))
            .cursor_pointer()
            .text_size(px(12.0 * scale))
            .text_color(appearance.muted)
            .hover(move |button| button.text_color(hover_color).bg(appearance.hover))
            .child(
                gpui::svg()
                    .path(icon)
                    .size(px(13.0 * scale))
                    .flex_shrink_0()
                    .text_color(appearance.muted),
            )
            .children(word)
    }

    /// A hovered card's own strip: `unifiedSessionCardHoverButtons`, through the same buttons as a
    /// Project sidebar row.
    ///
    /// CDXC:Sidebar 2026-10-31 DECISION:
    /// User (Q846): "if i expand my strip then it's expanded for all session cards so snooze/park can be 1 click". The chevron opens the strip on every card at once and stays open across restarts (`UnifiedSavedState::strip_expanded`), where the Project sidebar keeps it per project. This replaces the fixed Snooze and Park buttons; those two are in the default strip, right of its chevron.
    pub(super) fn render_unified_card_strip(
        &self,
        card: &UnifiedCard,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let expanded = self.gx_store.unified_sidebar.saved.strip_expanded;
        if crate::app::native_sidebar::hover_actions::session_hover_items(&card.session, expanded)
            .is_empty()
        {
            return None;
        }
        Some(self.render_native_session_hover_strip(
            &card.session,
            expanded,
            json!({"type": "unifiedHoverExpand"}),
            appearance,
            cx,
        ))
    }

    /// Unsnooze on a snoozed row, Unpark on a parked one.
    pub(super) fn render_unified_shelf_row_action(
        &self,
        card: &UnifiedCard,
        shelf: UnifiedShelf,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let id = card.session.session_id.clone();
        let (icon, word, label, message) = match shelf {
            UnifiedShelf::Snoozed => (
                "titlebar/alarm.svg",
                "Unsnooze",
                "Unsnooze session",
                json!({"type": "unsnoozeSession", "sessionId": id}),
            ),
            UnifiedShelf::Parked => (
                "titlebar/arrow-back-up.svg",
                "Unpark",
                "Unpark session",
                json!({"type": "setSessionParked", "sessionId": id, "parked": false}),
            ),
            _ => return None,
        };
        Some(
            Self::unified_hover_button(
                format!("native-unified-{word}-{id}"),
                icon,
                Some(word),
                label,
                appearance,
            )
            .on_click(cx.listener(move |app, _, window, cx| {
                cx.stop_propagation();
                app.handle_native_sidebar_action(
                    &NativeSidebarAction {
                        command: session_command(message.clone()),
                    },
                    window,
                    cx,
                );
            }))
            .into_any_element(),
        )
    }
}
