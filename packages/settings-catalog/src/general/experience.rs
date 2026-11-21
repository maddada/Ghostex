use crate::json::opt;
use crate::rows::{row, section, Section};

/// The Experience page's rows: the whole app's preset, then one per area. None has a settings key:
/// each writes the settings its preset names (`crate::experience`).
pub(crate) fn experience() -> Section {
    let presets = [
        opt("Chill", "chill"),
        opt("Regular", "regular"),
        opt("Advanced", "advanced"),
        opt("Custom", "custom"),
    ];
    section(
        "experience",
        "Experience",
        vec![
            row("experiencePreset", "Experience", "How many buttons and details Ghostex shows. Chill: a simple chat box and a calm chat. Regular: pick the model, the mode and the effort as you chat. Advanced: for people who use terminals more. Pick one to try it in the preview first; nothing changes until Apply. Each area below follows it; change one and it says Custom.").options(&presets),
            row("experienceChatBox", "Chat box", "Chill: the field, the model, the effort, •••, attach, the mic and Send. Regular adds the mode and Maximize chat box; the terminal is in •••. Advanced adds the terminal button and the context ring.").options(&presets),
            row("experienceChat", "Chat", "Chill: Quiet View keeps the running turn as one line with its time and names only reports. Regular and Advanced: Quiet View with the last turn open.").options(&presets),
            row("experienceSessionsList", "Sessions list", "Chill is the Sidebar's Minimal preset (no project icons, times or git numbers), Regular its Recommended, Advanced its Detailed (with last-active times).").options(&presets),
        ],
    )
}
