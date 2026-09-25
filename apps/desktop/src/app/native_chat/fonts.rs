use gpui::App;

/// The transcript's monospace family: tool arguments and previews, command
/// lines, and the code inside an expanded tool body. React resolved the same
/// text through `var(--font-mono, …)` (`.ghostex-chat-work-preview` and
/// `.ghostex-chat-tool-body` in styles/chat.css); this is the one family GPUI
/// has registered for it, so a call site must never spell it out again.
pub(crate) const CHAT_MONO: &str = "JetBrainsMono Nerd Font";

pub(crate) fn register(cx: &App) {
    // CDXC:SessionChat 2026-09-18 WHY: A host outside the desktop app (Chat Lab, since deleted) never reached the startup call that registers the vendored mono faces, so its tool previews and command lines fell back to the proportional family. Registering from the view itself means every host of the transcript has CHAT_MONO.
    crate::terminal_gpui_engine::register_gpui_terminal_engine_fonts(cx);
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        // CDXC:SessionChat 2026-09-17 WHY: The variable font declares family "DM Sans 9pt", while chat requests "DM Sans". GPUI also matches concrete weight/style faces, so register the generated static family for the same bold and italic text as React.
        let fonts = vec![
            include_bytes!("../../../../../.dependencies/app-fonts/dm-sans/static/400.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/dm-sans/static/500.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/dm-sans/static/600.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/dm-sans/static/700.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/dm-sans/static/400-italic.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/dm-sans/static/500-italic.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/dm-sans/static/600-italic.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/dm-sans/static/700-italic.ttf")
                .as_slice()
                .into(),
        ];
        if let Err(error) = cx.text_system().add_fonts(fonts) {
            eprintln!("Could not register chat fonts: {error}");
        }
    });
}
