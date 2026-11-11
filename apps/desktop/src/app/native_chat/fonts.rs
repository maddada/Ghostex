use gpui::App;

/// The transcript's monospace family: tool arguments and previews, command
/// lines, and the code inside an expanded tool body. React resolved the same
/// text through `var(--font-mono, …)` (`.ghostex-chat-work-preview` and
/// `.ghostex-chat-tool-body` in styles/chat.css, both deleted on 2026-09-25); this is the one family GPUI
/// has registered for it, so a call site must never spell it out again.
pub(crate) const CHAT_MONO: &str = super::chat_text_look::CHAT_MONO_FONT;

pub(crate) fn register(cx: &App) {
    // CDXC:SessionChat 2026-09-18 WHY: A host outside the desktop app (Chat Lab, since deleted) never reached the startup call that registers the vendored mono faces, so its tool previews and command lines fell back to the proportional family. Registering from the view itself means every host of the transcript has CHAT_MONO.
    crate::terminal_gpui_engine::register_gpui_terminal_engine_fonts(cx);
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        // CDXC:SessionChat 2026-09-17 WHY: The variable font declares family "DM Sans 9pt", while chat requests "DM Sans". GPUI also matches concrete weight/style faces, so register the generated static family for the same bold and italic text as React.
        let fonts = crate::assets::DM_SANS_UPRIGHT_FACES
            .iter()
            .chain(&[
                include_bytes!(
                    "../../../../../.dependencies/app-fonts/dm-sans/static/400-italic.ttf"
                )
                .as_slice(),
                include_bytes!(
                    "../../../../../.dependencies/app-fonts/dm-sans/static/500-italic.ttf"
                )
                .as_slice(),
                include_bytes!(
                    "../../../../../.dependencies/app-fonts/dm-sans/static/600-italic.ttf"
                )
                .as_slice(),
                include_bytes!(
                    "../../../../../.dependencies/app-fonts/dm-sans/static/700-italic.ttf"
                )
                .as_slice(),
            ])
            .map(|face| std::borrow::Cow::Borrowed(*face))
            .collect();
        if let Err(error) = cx.text_system().add_fonts(fonts) {
            eprintln!("Could not register chat fonts: {error}");
        }
    });
}

thread_local! {
    /// Label widths by font, weight and size (in pixels; both as bits) and text.
    static LABEL_WIDTHS: std::cell::RefCell<std::collections::HashMap<(String, u32, u32, String), f32>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Past this many kept widths the cache starts over, so labels that came and went do not pile up.
const LABEL_WIDTHS_KEPT: usize = 4096;

/// How wide `label` is, in pixels, in `font` at `weight` and `size` pixels on one line.
///
/// CDXC:SessionChat 2026-11-10 WHY:
/// Measured on the user's running app: 3.5% of the main thread's busy time was the work-summary pills and the chat box's pills measuring their labels, each time with a fresh text system, so every label was shaped again on every frame. A label's width depends only on its text, its font, its weight and its size, which are the key here, so a theme, font or zoom change measures anew. The coordinator dock and its work item rows (work_cards.rs `dock_text_width`, merged from main 2026-11-11) measure through here too.
pub(crate) fn label_width(
    label: &str,
    font: &str,
    weight: gpui::FontWeight,
    size: f32,
    cx: &App,
) -> f32 {
    if label.is_empty() {
        return 0.0;
    }
    let key = (
        font.to_owned(),
        weight.0.to_bits(),
        size.to_bits(),
        label.to_owned(),
    );
    if let Some(width) = LABEL_WIDTHS.with_borrow(|widths| widths.get(&key).copied()) {
        return width;
    }
    let system = gpui::WindowTextSystem::new(cx.text_system().clone());
    let run = gpui::TextRun {
        len: label.len(),
        font: gpui::Font {
            weight,
            ..gpui::font(font.to_owned())
        },
        ..Default::default()
    };
    let width = system
        .shape_line(label.to_owned().into(), gpui::px(size), &[run], None)
        .width
        .as_f32();
    LABEL_WIDTHS.with_borrow_mut(|widths| {
        if widths.len() >= LABEL_WIDTHS_KEPT {
            widths.clear();
        }
        widths.insert(key, width);
    });
    width
}
