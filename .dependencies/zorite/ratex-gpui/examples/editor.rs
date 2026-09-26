//! Run the structural math editor in a window:
//!
//!   cargo run -p ratex-gpui --example editor
//!
//! Type to build a formula — `/` makes a fraction, `^`/`_` a super/subscript, `(` `)` a
//! delimiter pair, space exits the current structure, ←/→ move, Backspace deletes.

use gpui::*;
use ratex_gpui::MathEditor;

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(900.0), px(520.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(MathEditor::new);
                let handle = view.read(cx).focus_handle();
                window.focus(&handle, cx);
                view
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
