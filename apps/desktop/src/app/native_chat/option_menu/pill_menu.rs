//! A working-row pill's floating menu (`chat_pills.rs`): the rows the Threads, Tasks, Subagents or
//! Background panel used to show above the composer, drawn in a chat menu window over the
//! transcript. The Files and links menu (files_links.rs) is drawn the same way.

use super::window::ChatOptionMenuPanel;
use crate::app::native_chat::state::NativeChatView;
use gpui::{
    AnyElement, Context, Entity, IntoElement, ParentElement as _, Styled as _, Window, div, px,
};
use serde_json::{Value, json};
use std::sync::Arc;

impl ChatOptionMenuPanel {
    /// The chat draws the body with its own listeners, so a row's click reaches the chat as it did
    /// when the panel lived in the chat's window.
    pub(super) fn render_pill_menu(
        &self,
        pill: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = pill["id"].as_str().unwrap_or_default().to_owned();
        let height = pill["height"].as_f64().unwrap_or(0.0) as f32;
        let (appearance, chat) = {
            let menu = self.menu.read(cx);
            let strip_menu = menu.pill().is_some_and(|pill| pill.starts_with("chat-pill-"));
            (
                body_appearance(menu.appearance.clone(), strip_menu),
                menu.chat.clone(),
            )
        };
        let body = chat
            .update(cx, |chat, cx| {
                chat.pill_menu_body(&id, &appearance, window, cx)
            })
            .ok()
            .flatten();
        div()
            .flex_shrink_0()
            .h(px(height * appearance.scale))
            .min_h_0()
            .flex()
            .flex_col()
            .children(body)
            .into_any_element()
    }

    /// The chat changed under an open pill menu: redraw it, follow its rows' height, and close it once
    /// its panel is gone (the subagents finished) or a row opened the subagent viewer over the pane.
    pub(super) fn pill_menu_changed(
        &mut self,
        chat: &Entity<NativeChatView>,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self
            .rows
            .first()
            .and_then(|row| row["pillMenu"]["id"].as_str())
            .map(str::to_owned)
        else {
            return;
        };
        let chat = chat.read(cx);
        let snapshot = &chat.snapshot;
        let height = chat.pill_menu_height(&id);
        let Some(height) = height.filter(|_| !snapshot["subagent"].is_object()) else {
            self.menu.update(cx, |menu, cx| menu.close(None, cx));
            return;
        };
        cx.notify();
        // Files and links only grows while it is open: filtering shrinks its list on every
        // keystroke, and where a window cannot move in place (the web build) a shrink
        // reopens the menu, which took the filter's focus and sent the next key to the chat box.
        let current = self.heights.first().copied().unwrap_or(0.0);
        let height = if id == "filesLinks" {
            height.max(current)
        } else {
            height
        };
        if current == height {
            return;
        }
        let rows = vec![json!({"pillMenu": {"id": id, "height": height}})];
        self.heights = vec![height];
        self.rows = Arc::new(rows.clone());
        let depth = self.depth;
        self.menu.update(cx, |menu, cx| {
            let outer = px((menu.metrics().chrome() + height) * menu.appearance.scale);
            menu.refit_panel_height(depth, outer, rows, cx);
        });
    }
}

/// The look a pill menu's body is drawn in. A menu the chat box's pill strip opened sits on the
/// sidebar dropdowns' surface, which follows the app's theme (render.rs `strip_menu`), so its body
/// takes that theme too; any other menu keeps the chat's.
/// CDXC:Boosts 2026-11-20 WHY: with the chat in light and the app in dark, the Boost menu stayed
/// dark (the app's surface) while its rows took the light chat's dark ink, so every title was
/// unreadable. The body now reads its colours from the surface it sits on; the chat's scale, font
/// and window glass carry over.
fn body_appearance(
    appearance: crate::app::native_chat::appearance::ChatAppearance,
    strip_menu: bool,
) -> crate::app::native_chat::appearance::ChatAppearance {
    let light = crate::CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed);
    if !strip_menu || light == appearance.light {
        return appearance;
    }
    let mut body = crate::app::native_chat::appearance::ChatAppearance::for_variant(light);
    body.scale = appearance.scale;
    body.font = appearance.font;
    body.window_glass = appearance.window_glass;
    body
}
