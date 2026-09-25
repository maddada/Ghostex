//! Whether a press that landed on a transcript row is a click the row should
//! act on, or the tail of a text selection.
//!
//! CDXC:SessionChat 2026-09-19 WHY:
//! React's row-level handlers asked the DOM event two questions before they
//! toggled anything (`session-chat-message-list/rows.tsx`,
//! `session-chat-file-change-card.tsx`): did the press land on a control of its
//! own (`event.target.closest('a, button, …')`), and did it leave a selection
//! behind (`getSelection()?.isCollapsed === false`). GPUI has no event target,
//! so the controls inside a row stop the press themselves and this answers the
//! second question for every row that turns its own text into a trigger.

use gpui::{App, ClickEvent, Window};
use gpui_component::WindowExt as _;

/// How far the pointer may travel between press and release and still read as a
/// click rather than a drag across the row's text. A selection that ends on
/// blank space leaves no selected text behind, so the distance is asked for as
/// well as the selection itself.
const DRAG_SLOP: f64 = 3.0;

/// True when this press should act on the row it landed on.
pub(super) fn acts_on_row(event: &ClickEvent, window: &mut Window, cx: &mut App) -> bool {
    if let ClickEvent::Mouse(click) = event {
        // The second press of a double click selects the word under it, and a
        // press that moved was a drag over the row's own text.
        if click.down.click_count > 1
            || (click.up.position - click.down.position).magnitude() > DRAG_SLOP
        {
            return false;
        }
    }
    window.selected_text(cx).trim().is_empty()
}
