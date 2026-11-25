use super::state::NativeChatView;
use gpui::{Context, Pixels, Point, Window};
use serde_json::Value;

impl NativeChatView {
    /// The right-click menu on any reference, in the composer or in the transcript.
    ///
    /// CDXC:SessionChat 2026-09-18 SEE-ALSO:
    /// The rows come from the core (`packages/gx-chat-core/src/composer/reference_menu.rs`), ported
    /// from React's `session-chat-reference-menu-items.tsx`, which React offered on a composer pill
    /// and on a transcript link alike.
    pub(super) fn show_reference_menu(
        &mut self,
        href: String,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut rows: Vec<Value> = self
            .runtime
            .as_ref()
            .and_then(|runtime| {
                runtime.query_for_gesture(
                    "referenceMenu",
                    vec![Value::String(href)],
                    std::time::Duration::from_millis(250),
                )
            })
            .and_then(|rows| rows.as_array().cloned())
            .unwrap_or_default();
        self.keep_available_file_rows(&mut rows);
        if rows.is_empty() {
            return false;
        }
        self.show_chat_menu_at(rows, at, 240.0, window, cx);
        true
    }

    pub(super) fn show_composer_reference_menu(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.input.clone() else {
            return false;
        };
        let Some(range) = input.read(cx).inline_replacement_at(event.position) else {
            return false;
        };
        let Some(reference) = self
            .composer_references
            .iter()
            .find(|reference| reference.range == range)
            .cloned()
        else {
            return false;
        };
        // A press inside the composer already cancelled any pending pill open; keep it cancelled so
        // the menu cannot be followed by a view opening behind it.
        self.composer_reference_click += 1;
        self.composer_reference_task = None;
        // A Boost pill's own menu: Expand, Edit Boost and Remove (boosts.rs).
        if reference.kind == "boost"
            && let Some(id) = super::boosts::boost_id(&reference.path)
        {
            let rows = self.boost_pill_menu_rows(&reference.range, &id);
            self.show_chat_menu_at(rows, event.position, 200.0, window, cx);
            return true;
        }
        self.show_reference_menu(reference.path, event.position, window, cx)
    }
}
