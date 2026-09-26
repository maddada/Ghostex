//! Inter, the Docs page's typeface, registered once for the native view.

use gpui::App;

/// The family native Docs draws its chrome and live Markdown in.
pub(crate) const DOCS_FONT: &str = "Inter";
/// Source mode and code: the app's registered monospace family.
pub(crate) const DOCS_MONO: &str = crate::app::native_chat::fonts::CHAT_MONO;

/// CDXC:Docs 2026-09-24 WHY:
/// The Docs page drew in Inter Variable, and the native view has to read the same, but GPUI matches static faces by weight, so the static Light/Regular/Medium/SemiBold/Bold/Italic TTFs from the rsms/inter release (OFL, `.dependencies/app-fonts/inter/`) are registered instead of the variable font.
pub(crate) fn register(cx: &App) {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        let fonts = vec![
            include_bytes!("../../../../../.dependencies/app-fonts/inter/Inter-Light.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/inter/Inter-Regular.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/inter/Inter-Medium.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/inter/Inter-SemiBold.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/inter/Inter-Bold.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../../../.dependencies/app-fonts/inter/Inter-Italic.ttf")
                .as_slice()
                .into(),
        ];
        if let Err(error) = cx.text_system().add_fonts(fonts) {
            eprintln!("[ghostex-gpui] Could not register the Docs font: {error}");
        }
    });
}
