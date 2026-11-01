use gpui::{App, EXACT_FONT_FAMILY_PREFIX, SharedString};
use std::sync::{Arc, Mutex};

/// CDXC:DesignSystem 2026-09-26 DECISION:
/// User: Windows should render with the same quality, type sizes and spacing as macOS.
pub(crate) const UI_FONT: &str = "Inter";

/// The Files view's type sizes at 100%: its editor text, its line numbers and its file list names.
/// The Changes view draws its diff with the same three, so both views read at one size. Draw the
/// text and line numbers with `files_text_size()` and `files_line_number_size()`, which apply the
/// Files zoom.
pub(crate) const FILES_TEXT_SIZE: f32 = 14.0;
pub(crate) const FILES_LINE_NUMBER_SIZE: f32 = 12.0;
/// `.manage-file-name`: 15.55px at weight 300.
pub(crate) const FILES_NAME_SIZE: f32 = 15.55;

/// Settings > Interface > Default files zoom (%).
pub(crate) const FILES_ZOOM_SETTING: &str = "filesZoomPercent";
/// The range Settings offers for it, the same as Default chat zoom's.
const FILES_ZOOM_RANGE: (f32, f32) = (70.0, 200.0);

thread_local! {
    /// The zoom the Files and Changes views paint at, set by `sync_files_zoom` as they draw.
    static FILES_ZOOM_PERCENT: std::cell::Cell<f32> = const { std::cell::Cell::new(100.0) };
    /// A zoom chord's value the debounced save has not written yet (`native_docs/text_zoom.rs`);
    /// it wins over the saved setting so the next press steps from what is on screen.
    static PENDING_FILES_ZOOM_PERCENT: std::cell::Cell<Option<f32>> =
        const { std::cell::Cell::new(None) };
}

/// Reads the Files zoom (a chord's pending value, else the saved `filesZoomPercent`) into the
/// value the sizes below scale by, and returns it. The Files and Changes views call it at the top
/// of their draw, so every size read during that draw is one lock-free lookup.
pub(crate) fn sync_files_zoom(settings: &serde_json::Map<String, serde_json::Value>) -> f32 {
    let percent = PENDING_FILES_ZOOM_PERCENT
        .with(std::cell::Cell::get)
        .unwrap_or_else(|| {
            settings
                .get(FILES_ZOOM_SETTING)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(100.0) as f32
        })
        .clamp(FILES_ZOOM_RANGE.0, FILES_ZOOM_RANGE.1);
    FILES_ZOOM_PERCENT.with(|cell| cell.set(percent));
    percent
}

pub(crate) fn set_pending_files_zoom_percent(percent: Option<f32>) {
    PENDING_FILES_ZOOM_PERCENT.with(|cell| cell.set(percent));
}

pub(crate) fn take_pending_files_zoom_percent() -> Option<f32> {
    PENDING_FILES_ZOOM_PERCENT.with(std::cell::Cell::take)
}

/// The Files zoom as a factor (1.0 at 100%), for sizes measured from the text: row heights,
/// gutters, the reading column.
pub(crate) fn files_zoom_scale() -> f32 {
    FILES_ZOOM_PERCENT.with(std::cell::Cell::get) / 100.0
}

pub(crate) fn files_text_size() -> f32 {
    FILES_TEXT_SIZE * files_zoom_scale()
}

pub(crate) fn files_line_number_size() -> f32 {
    FILES_LINE_NUMBER_SIZE * files_zoom_scale()
}

/// CDXC:DesignSystem 2026-09-26 WHY:
/// The bundled static faces identify as Inter, not Inter Variable. Register them before the first window so chrome and Docs use the same metrics without relying on fonts installed on the host.
pub(crate) fn register(cx: &App) {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        let fonts = vec![
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-Light.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-Regular.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-Medium.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-SemiBold.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-Bold.ttf")
                .as_slice()
                .into(),
            include_bytes!("../../../.dependencies/app-fonts/inter/Inter-Italic.ttf")
                .as_slice()
                .into(),
        ];
        if let Err(error) = cx.text_system().add_fonts(fonts) {
            eprintln!("[ghostex-gpui] Could not register UI fonts: {error}");
        }
    });
}

/// The setting that names the app font. It keeps its older name, from when it only reached the
/// chat, so a font someone already saved stays saved.
pub(crate) const APP_FONT_SETTING: &str = "sessionChatFontFamily";

/// Every family the app asks for when it draws ordinary, non-monospace text, its implicit default
/// included. Each surface keeps its own look (Inter in the chrome, DM Sans in the chat, the system
/// face in dialogs) until an app font is picked, and then all of them draw in it.
///
/// CDXC:DesignSystem 2026-10-31 DECISION:
/// User: "add list of the fonts so person can pick one of the installed fonts for the app" and "Font should apply everywhere (except for monospace, monospace should remain as is font)". The pick reaches every surface through one substitution in gpui's text system (`TextSystem::set_font_family_substitution`), not through the call sites: text with no family of its own inherits gpui's default, and measuring asks for the same fonts as drawing, so a list of call sites would keep missing some. No pick installs no substitution, so the default look is untouched. A new proportional family goes in this list, and `every_named_family_is_classified` fails until it does.
/// SEE-ALSO: .dependencies/zed/crates/gpui/src/text_system.rs.
pub(crate) const PROPORTIONAL_FAMILIES: &[&str] = &[
    ".SystemUIFont",
    ".AppleSystemUIFont",
    ".ZedSans",
    "Zed Plex Sans",
    "IBM Plex Sans",
    "Segoe UI",
    UI_FONT,
    "DM Sans",
    "DM Sans 14pt",
    "Manrope",
    "Arial",
    "Helvetica",
];

/// The monospace families the app asks for by name, which an app font never replaces.
#[cfg(test)]
const MONOSPACE_FAMILIES: &[&str] = &[
    ".AppleSystemUIFontMonospaced",
    ".ZedMono",
    "Consolas",
    "IBM Plex Mono",
    "IBM Plex Mono Medium",
    "JetBrains Mono",
    "JetBrainsMono Nerd Font",
    "Lilex",
    "Menlo",
    "SF Mono",
    "monospace",
];

/// The setting that names the monospace font. It keeps its older name, from when it only reached
/// the terminal, so a family someone already typed stays saved.
pub(crate) const MONOSPACE_FONT_SETTING: &str = "terminalFontFamily";

/// The monospace setting's default, which keeps every surface on its own font.
pub(crate) const MONOSPACE_DEFAULT_FAMILY: &str = "JetBrains Mono";

/// The name the default's bundled faces register under.
const MONOSPACE_DEFAULT_FACE: &str = "JetBrainsMono Nerd Font";

/// The row of the monospace list that leaves the family to Ghostex's terminal config.
pub(crate) const TERMINAL_CONFIG_LABEL: &str = "From terminal config";

/// Every family a monospace pick replaces, so terminals, the Files view, diffs, code blocks, inline
/// code and tool output all follow one setting.
///
/// CDXC:DesignSystem 2026-10-31 DECISION:
/// GitHub #375, user: "We need ability to pick the font from dropdown for terminal code etc (monospace text)", one setting for all of them unless there is a real reason not to. It reaches them the way the app font does, through the text system's substitution table rather than the call sites, and the default installs nothing, so today's fonts (JetBrains Mono in terminals and chat, the system monospace in diffs and dialogs) stay as they are. Left out on purpose: onboarding's IBM Plex Mono (its designed look) and the small Menlo and SF Mono badges drawn to fixed sizes.
/// SEE-ALSO: .dependencies/zed/crates/gpui/src/text_system.rs.
const REPLACED_MONOSPACE_FAMILIES: &[&str] = &[
    MONOSPACE_DEFAULT_FACE,
    MONOSPACE_DEFAULT_FAMILY,
    ".AppleSystemUIFontMonospaced",
    ".ZedMono",
    "Consolas",
    "Lilex",
    "monospace",
];

/// The family `value` of the app font setting names, if it names one: blank is the app's default,
/// and a CSS stack from an older save counts as its first entry.
pub(crate) fn picked_family(value: &str) -> Option<&str> {
    let family = value
        .split(',')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches(|character| character == '"' || character == '\'');
    match family {
        "" | "system-ui" | "ui-sans-serif" | "sans-serif" | "-apple-system" => None,
        family => Some(family),
    }
}

/// The concrete families a monospace setting names, in order: one Ghostty family, or a list of
/// them, without the generic names that stand for no font.
fn monospace_entries(value: &str) -> Vec<&str> {
    value
        .split(',')
        .map(|entry| {
            entry
                .trim()
                .trim_matches(|character| character == '"' || character == '\'')
        })
        .filter(|entry| !entry.is_empty() && !matches!(*entry, "monospace" | "ui-monospace"))
        .collect()
}

fn is_installed(family: &str, installed: &[(String, bool)]) -> bool {
    installed
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case(family))
}

fn is_monospace_default(family: &str) -> bool {
    family.eq_ignore_ascii_case(MONOSPACE_DEFAULT_FAMILY)
        || family.eq_ignore_ascii_case(MONOSPACE_DEFAULT_FACE)
}

/// The family the setting's `value` draws in: its first entry on this computer. `installed` is
/// `None` until the fonts have been read, which trusts the first entry.
fn first_available_entry<'a>(
    value: &'a str,
    installed: Option<&[(String, bool)]>,
) -> Option<&'a str> {
    let entries = monospace_entries(value);
    match installed {
        Some(installed) => entries
            .into_iter()
            .find(|entry| is_monospace_default(entry) || is_installed(entry, installed)),
        None => entries.first().copied(),
    }
}

/// The family a monospace setting's `value` replaces the app's monospace fonts with. `None` leaves
/// them as they are: the default, a blank value (the terminal's own config decides), or a family
/// that is not on this computer.
pub(crate) fn monospace_replacement(
    value: &str,
    installed: Option<&[(String, bool)]>,
) -> Option<String> {
    first_available_entry(value, installed)
        .filter(|family| !is_monospace_default(family))
        .map(str::to_string)
}

/// The family a terminal draws for the Ghostty `font-family` value `value`. One name stays as it
/// is, installed or not; of a list, it is the first entry on this computer, because the terminal
/// draws one family and a whole list names none.
pub(crate) fn terminal_family(value: &str, installed: Option<&[(String, bool)]>) -> String {
    if !value.contains(',') {
        return value.to_string();
    }
    first_available_entry(value, installed)
        .or_else(|| monospace_entries(value).first().copied())
        .unwrap_or(value)
        .to_string()
}

/// What the text system draws in place of what: the picked app font over the proportional families
/// and the picked monospace font over the monospace ones.
fn font_substitutions(
    app: Option<&str>,
    monospace: Option<&str>,
    theme_family: &SharedString,
    theme_mono_family: &SharedString,
) -> Vec<(SharedString, SharedString)> {
    let mut table = Vec::new();
    if let Some(app) = app {
        let to = SharedString::from(app.to_string());
        table.extend(
            PROPORTIONAL_FAMILIES
                .iter()
                .map(|family| SharedString::new_static(family))
                .chain([theme_family.clone()])
                .map(|from| (from, to.clone())),
        );
    }
    if let Some(monospace) = monospace {
        let to = SharedString::from(monospace.to_string());
        table.extend(
            REPLACED_MONOSPACE_FAMILIES
                .iter()
                .map(|family| SharedString::new_static(family))
                .chain([theme_mono_family.clone()])
                .map(|from| (from, to.clone())),
        );
    }
    table
}

/// Applies the saved app and monospace fonts to the whole process, once for each change.
pub(crate) fn apply_fonts(settings: &serde_json::Map<String, serde_json::Value>, cx: &mut App) {
    static APPLIED: Mutex<Option<AppliedFonts>> = Mutex::new(None);
    let app = settings
        .get(APP_FONT_SETTING)
        .and_then(serde_json::Value::as_str)
        .and_then(picked_family);
    let installed = installed_families();
    let monospace = monospace_replacement(
        settings
            .get(MONOSPACE_FONT_SETTING)
            .and_then(serde_json::Value::as_str)
            .unwrap_or(MONOSPACE_DEFAULT_FAMILY),
        installed.as_ref().map(|families| families.as_slice()),
    );
    let mut applied = APPLIED.lock().unwrap_or_else(|poison| poison.into_inner());
    let theme = gpui_component::Theme::global(cx);
    let (theme_family, theme_mono_family) =
        (theme.font_family.clone(), theme.mono_font_family.clone());
    let state = applied.get_or_insert_with(|| AppliedFonts {
        app: None,
        monospace: None,
        theme_family,
        theme_mono_family,
    });
    if state.app.as_deref() == app && state.monospace == monospace {
        return;
    }
    state.app = app.map(str::to_string);
    state.monospace = monospace;
    let table = font_substitutions(
        app,
        state.monospace.as_deref(),
        &state.theme_family,
        &state.theme_mono_family,
    );
    let theme_family = state.theme_family.clone();
    drop(applied);
    cx.text_system().set_font_family_substitutions(table);
    // gpui-component re-resolves `.SystemUIFont` when the appearance changes and can name the
    // picked family on its theme, which must not outlive the pick.
    gpui_component::Theme::global_mut(cx).font_family = theme_family;
    cx.refresh_windows();
}

struct AppliedFonts {
    app: Option<String>,
    monospace: Option<String>,
    /// gpui-component's own UI family, resolved once before any pick was applied. On Linux it is
    /// whichever installed family `.SystemUIFont` landed on.
    theme_family: SharedString,
    /// gpui-component's monospace family, which fenced code reads through its theme.
    theme_mono_family: SharedString,
}

/// A family name that draws in exactly that family whatever the app font is: how a row of the
/// font list shows its own face.
pub(crate) fn exact_family(name: &str) -> SharedString {
    format!("{EXACT_FONT_FAMILY_PREFIX}{name}").into()
}

/// Whether a family looks monospaced from its name, for what the platform cannot answer without
/// loading every installed font (`TextSystem::monospace_font_names`: Windows, Linux and fonts the
/// app bundles). A family this misses there stays reachable by typing its name in the monospace
/// list's search field.
pub(crate) fn looks_monospaced(name: &str) -> bool {
    let lower = name.to_lowercase();
    let words = lower.split([' ', '-', '_']).collect::<Vec<_>>();
    [
        "mono",
        "monospace",
        "monospaced",
        "code",
        "terminal",
        "fixed",
        "typewriter",
    ]
    .iter()
    .any(|word| words.contains(word))
        || lower.ends_with("mono")
        || lower.ends_with("code")
        || [
            "andale",
            "consolas",
            "courier",
            "inconsolata",
            "menlo",
            "monaco",
            "sfmono",
            "lilex",
            "iosevka",
            "fira code",
            "cascadia",
        ]
        .iter()
        .any(|name| lower.contains(name))
}

/// Whether a family draws its own name legibly. Symbol fonts would show a row of pictures.
pub(crate) fn draws_its_own_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    ![
        "dingbats",
        "wingdings",
        "webdings",
        "symbol",
        "ornaments",
        "emoji",
        "icons",
        "glyphs",
        "braille",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

/// A family's face in the font list, drawn in itself whatever font is picked.
pub(crate) fn monospace_default_face() -> SharedString {
    exact_family(MONOSPACE_DEFAULT_FACE)
}

/// One row of the font list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FontRow {
    /// The app's own fonts; always first, so it is easy to get back to.
    AppDefault,
    /// The monospace list's first row: today's font.
    MonospaceDefault,
    /// A blank monospace setting: Ghostex's terminal config, or the platform default, decides.
    TerminalConfig,
    /// A saved monospace value the list has no row for that still draws: a Ghostty list, or a
    /// family that is installed but not monospaced. It stays saved exactly as typed.
    Custom(String),
    /// What is typed in the monospace list's search field, to save as it is.
    UseTyped(String),
    /// The saved family when it is not on this computer. It stays saved and listed, and the app
    /// draws in its default fonts meanwhile.
    NotInstalled(String),
    Family {
        name: String,
        monospaced: bool,
    },
}

impl FontRow {
    /// What choosing this row saves.
    pub(crate) fn value(&self) -> &str {
        match self {
            FontRow::AppDefault | FontRow::TerminalConfig => "",
            FontRow::MonospaceDefault => MONOSPACE_DEFAULT_FAMILY,
            FontRow::NotInstalled(name)
            | FontRow::Custom(name)
            | FontRow::UseTyped(name)
            | FontRow::Family { name, .. } => name,
        }
    }
}

/// The families `names` a person can pick from: hidden faces out, ordinary fonts first and the
/// monospaced ones after them, each group alphabetical. `monospaced` is what the platform reports.
pub(crate) fn pickable_families(names: Vec<String>, monospaced: &[String]) -> Vec<(String, bool)> {
    let mut families: Vec<(String, bool)> = names
        .into_iter()
        .filter(|name| !name.is_empty() && !name.starts_with(['.', '@']))
        .map(|name| {
            let monospaced = monospaced.contains(&name) || looks_monospaced(&name);
            (name, monospaced)
        })
        .collect();
    families.sort_by_cached_key(|(name, monospaced)| (*monospaced, name.to_lowercase()));
    families.dedup_by(|left, right| left.0 == right.0);
    families
}

/// The rows the list shows for what is typed in its search field. `installed` is `None` until the
/// fonts have been read, so a saved family is not called missing before anyone has looked.
pub(crate) fn font_rows(
    installed: Option<&[(String, bool)]>,
    saved: &str,
    query: &str,
) -> Vec<FontRow> {
    let query = query.trim().to_lowercase();
    let matches = |name: &str| query.is_empty() || name.to_lowercase().contains(&query);
    let mut rows = Vec::new();
    if matches("App default") {
        rows.push(FontRow::AppDefault);
    }
    if let (Some(installed), Some(saved)) = (installed, picked_family(saved))
        && matches(saved)
        && !installed
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(saved))
    {
        rows.push(FontRow::NotInstalled(saved.to_string()));
    }
    rows.extend(
        installed
            .unwrap_or_default()
            .iter()
            .filter(|(name, _)| matches(name))
            .map(|(name, monospaced)| FontRow::Family {
                name: name.clone(),
                monospaced: *monospaced,
            }),
    );
    rows
}

/// The rows the monospace list shows: only monospaced families, after the default and the terminal
/// config, with a saved value it has no row for and whatever is typed that matches nothing exactly.
pub(crate) fn monospace_font_rows(
    installed: Option<&[(String, bool)]>,
    saved: &str,
    query: &str,
) -> Vec<FontRow> {
    let typed = query.trim();
    let query = typed.to_lowercase();
    let matches = |name: &str| query.is_empty() || name.to_lowercase().contains(&query);
    let saved = saved.trim();
    let listed = |name: &str| {
        monospaced_row(name)
            && installed
                .unwrap_or_default()
                .iter()
                .any(|(family, monospaced)| *monospaced && family.eq_ignore_ascii_case(name))
    };
    let mut rows = Vec::new();
    if matches(MONOSPACE_DEFAULT_FAMILY) {
        rows.push(FontRow::MonospaceDefault);
    }
    if matches(TERMINAL_CONFIG_LABEL) {
        rows.push(FontRow::TerminalConfig);
    }
    if let Some(installed) = installed
        && !saved.is_empty()
        && !is_monospace_default(saved)
        && !listed(saved)
        && matches(saved)
    {
        let entries = monospace_entries(saved);
        rows.push(
            if !entries.is_empty()
                && !entries
                    .iter()
                    .any(|entry| is_monospace_default(entry) || is_installed(entry, installed))
            {
                FontRow::NotInstalled(saved.to_string())
            } else {
                FontRow::Custom(saved.to_string())
            },
        );
    }
    rows.extend(
        installed
            .unwrap_or_default()
            .iter()
            .filter(|(name, monospaced)| *monospaced && monospaced_row(name) && matches(name))
            .map(|(name, monospaced)| FontRow::Family {
                name: name.clone(),
                monospaced: *monospaced,
            }),
    );
    if !typed.is_empty()
        && !rows
            .iter()
            .any(|row| row.value().eq_ignore_ascii_case(typed))
    {
        rows.push(FontRow::UseTyped(typed.to_string()));
    }
    rows
}

/// Whether a monospaced family gets its own row: not the default, which has one already, and not a
/// symbol font.
fn monospaced_row(name: &str) -> bool {
    !is_monospace_default(name) && draws_its_own_name(name)
}

/// What the monospace list's closed control reads for the saved `value`: as typed, list and all.
pub(crate) fn monospace_label(value: &str, installed: Option<&[(String, bool)]>) -> String {
    let value = value.trim();
    if value.is_empty() {
        return TERMINAL_CONFIG_LABEL.to_string();
    }
    let entries = monospace_entries(value);
    match installed {
        Some(installed)
            if !entries.is_empty()
                && !entries
                    .iter()
                    .any(|entry| is_monospace_default(entry) || is_installed(entry, installed)) =>
        {
            format!("{value} (not installed)")
        }
        _ => value.to_string(),
    }
}

static INSTALLED_FAMILIES: Mutex<Option<Arc<Vec<(String, bool)>>>> = Mutex::new(None);

/// The installed families from the last read, once one has finished.
pub(crate) fn installed_families() -> Option<Arc<Vec<(String, bool)>>> {
    INSTALLED_FAMILIES
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

/// Reads the installed families off the UI thread: asking the platform walks every font on the
/// computer. Runs at launch so the first open of Settings has them, and again when the list opens
/// so a font installed since then appears. The returned task resolves when the read is stored.
pub(crate) fn read_installed_families(cx: &App) -> gpui::Task<()> {
    let text_system = cx.text_system().clone();
    cx.background_executor().spawn(async move {
        let families = Arc::new(pickable_families(
            text_system.all_font_names(),
            &text_system.monospace_font_names(),
        ));
        *INSTALLED_FAMILIES
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(families);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn families(names: &[&str]) -> Vec<(String, bool)> {
        pickable_families(names.iter().map(|name| name.to_string()).collect(), &[])
    }

    #[test]
    fn blank_and_generic_names_are_the_app_default() {
        for value in ["", "   ", "system-ui", "sans-serif", "-apple-system, Arial"] {
            assert_eq!(picked_family(value), None, "{value:?}");
        }
    }

    #[test]
    fn a_saved_name_is_trimmed_and_a_stack_counts_as_its_first_entry() {
        assert_eq!(picked_family("  Georgia "), Some("Georgia"));
        assert_eq!(picked_family("'Fira Sans', sans-serif"), Some("Fira Sans"));
    }

    #[test]
    fn the_app_default_comes_first() {
        let installed = families(&["Zapfino", "Arial"]);
        let rows = font_rows(Some(&installed), "Arial", "");
        assert_eq!(rows[0], FontRow::AppDefault);
        assert_eq!(rows[0].value(), "");
        assert_eq!(rows[1].value(), "Arial");
    }

    #[test]
    fn a_saved_font_that_is_not_installed_stays_listed() {
        let installed = families(&["Arial"]);
        let rows = font_rows(Some(&installed), "Gone Font", "");
        assert_eq!(rows[1], FontRow::NotInstalled("Gone Font".to_string()));
        assert_eq!(rows[1].value(), "Gone Font");
    }

    #[test]
    fn a_saved_font_is_not_called_missing_before_the_fonts_are_read() {
        assert_eq!(font_rows(None, "Arial", ""), vec![FontRow::AppDefault]);
    }

    #[test]
    fn a_saved_font_that_is_installed_is_listed_once() {
        let installed = families(&["Arial"]);
        let rows = font_rows(Some(&installed), "arial", "");
        assert_eq!(rows.len(), 2);
        assert!(matches!(rows[1], FontRow::Family { .. }));
    }

    #[test]
    fn hidden_faces_are_left_out_and_monospaced_fonts_come_last() {
        let names: Vec<String> = families(&[
            "Menlo",
            ".SF NS",
            "@MS Gothic",
            "zapfino",
            "Arial",
            "JetBrains Mono",
        ])
        .into_iter()
        .map(|(name, _)| name)
        .collect();
        assert_eq!(names, ["Arial", "zapfino", "JetBrains Mono", "Menlo"]);
    }

    #[test]
    fn monospaced_fonts_stay_pickable_and_searchable() {
        let installed = families(&["Arial", "Menlo"]);
        let rows = font_rows(Some(&installed), "", "men");
        assert_eq!(
            rows,
            vec![FontRow::Family {
                name: "Menlo".to_string(),
                monospaced: true,
            }]
        );
    }

    #[test]
    fn searching_filters_the_list_without_case() {
        let installed = families(&["Arial", "Georgia"]);
        let rows = font_rows(Some(&installed), "", "GEO");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value(), "Georgia");
    }

    #[test]
    fn symbol_fonts_do_not_draw_their_own_names() {
        for name in ["Wingdings 2", "Zapf Dingbats", "Apple Color Emoji"] {
            assert!(!draws_its_own_name(name), "{name}");
        }
        assert!(draws_its_own_name("Georgia"));
    }

    #[test]
    fn an_app_font_never_replaces_a_monospace_family() {
        for family in MONOSPACE_FAMILIES {
            assert!(!PROPORTIONAL_FAMILIES.contains(family), "{family}");
        }
    }

    #[test]
    fn a_monospace_pick_only_replaces_monospace_families() {
        for family in REPLACED_MONOSPACE_FAMILIES {
            assert!(MONOSPACE_FAMILIES.contains(family), "{family}");
        }
    }

    #[test]
    fn the_two_picks_substitute_independently() {
        let theme = SharedString::new_static("Theme Sans");
        let mono = SharedString::new_static("Theme Mono");
        let target = |table: &[(SharedString, SharedString)], from: &str| {
            table
                .iter()
                .find(|(family, _)| family.as_ref() == from)
                .map(|(_, to)| to.to_string())
        };
        let both = font_substitutions(Some("Georgia"), Some("Menlo"), &theme, &mono);
        assert_eq!(target(&both, "Inter").as_deref(), Some("Georgia"));
        assert_eq!(target(&both, "Theme Sans").as_deref(), Some("Georgia"));
        assert_eq!(
            target(&both, MONOSPACE_DEFAULT_FACE).as_deref(),
            Some("Menlo")
        );
        assert_eq!(target(&both, "Theme Mono").as_deref(), Some("Menlo"));
        let only_app = font_substitutions(Some("Georgia"), None, &theme, &mono);
        assert_eq!(target(&only_app, MONOSPACE_DEFAULT_FACE), None);
        let only_mono = font_substitutions(None, Some("Menlo"), &theme, &mono);
        assert_eq!(target(&only_mono, "Inter"), None);
        assert!(font_substitutions(None, None, &theme, &mono).is_empty());
    }

    fn mono_installed() -> Vec<(String, bool)> {
        pickable_families(
            [
                "Arial",
                "Menlo",
                "Hack",
                "Fira Code",
                "Symbols Nerd Font Mono",
            ]
            .iter()
            .map(|name| name.to_string())
            .collect(),
            // The platform says Hack is monospaced; its name does not.
            &["Hack".to_string()],
        )
    }

    fn values(rows: &[FontRow]) -> Vec<&str> {
        rows.iter().map(FontRow::value).collect()
    }

    #[test]
    fn the_default_and_a_blank_value_replace_nothing() {
        let installed = mono_installed();
        for value in [
            "JetBrains Mono",
            "JetBrainsMono Nerd Font",
            "",
            "  ",
            "monospace",
        ] {
            assert_eq!(
                monospace_replacement(value, Some(&installed)),
                None,
                "{value:?}"
            );
        }
    }

    #[test]
    fn a_picked_family_replaces_the_monospace_fonts() {
        let installed = mono_installed();
        assert_eq!(
            monospace_replacement("Menlo", Some(&installed)).as_deref(),
            Some("Menlo")
        );
    }

    #[test]
    fn a_family_that_is_not_installed_replaces_nothing_until_it_is() {
        let installed = mono_installed();
        assert_eq!(monospace_replacement("Gone Mono", Some(&installed)), None);
        assert_eq!(
            monospace_replacement("Gone Mono", None).as_deref(),
            Some("Gone Mono")
        );
    }

    #[test]
    fn a_typed_list_uses_its_first_installed_entry() {
        let installed = mono_installed();
        let list = "\"Gone Mono\", 'Fira Code', Menlo, monospace";
        assert_eq!(
            monospace_replacement(list, Some(&installed)).as_deref(),
            Some("Fira Code")
        );
        assert_eq!(terminal_family(list, Some(&installed)), "Fira Code");
    }

    #[test]
    fn a_terminal_keeps_a_single_name_as_it_is() {
        let installed = mono_installed();
        for value in ["Menlo", "Gone Mono", "JetBrains Mono"] {
            assert_eq!(terminal_family(value, Some(&installed)), value);
        }
    }

    #[test]
    fn a_terminal_with_nothing_installed_from_a_list_asks_for_its_first_entry() {
        let installed = mono_installed();
        assert_eq!(terminal_family("Gone, Also Gone", Some(&installed)), "Gone");
    }

    #[test]
    fn the_monospace_list_only_offers_monospaced_fonts_after_the_default() {
        let installed = mono_installed();
        let rows = monospace_font_rows(Some(&installed), "JetBrains Mono", "");
        assert_eq!(rows[0], FontRow::MonospaceDefault);
        assert_eq!(rows[1], FontRow::TerminalConfig);
        assert_eq!(values(&rows[2..]), ["Fira Code", "Hack", "Menlo"]);
    }

    #[test]
    fn the_platforms_answer_lists_a_font_whose_name_does_not_say_mono() {
        let installed = mono_installed();
        let rows = monospace_font_rows(Some(&installed), "", "hack");
        assert_eq!(values(&rows), ["Hack"]);
    }

    #[test]
    fn a_blank_value_selects_the_terminal_config_row() {
        assert_eq!(FontRow::TerminalConfig.value(), "");
        assert_eq!(monospace_label("", None), TERMINAL_CONFIG_LABEL);
    }

    #[test]
    fn a_saved_list_stays_listed_exactly_as_typed() {
        let installed = mono_installed();
        let list = "Fira Code, Menlo";
        let rows = monospace_font_rows(Some(&installed), list, "");
        assert_eq!(rows[2], FontRow::Custom(list.to_string()));
        assert_eq!(rows[2].value(), list);
        assert_eq!(monospace_label(list, Some(&installed)), list);
    }

    #[test]
    fn a_saved_font_that_is_installed_but_not_monospaced_stays_listed() {
        let installed = mono_installed();
        let rows = monospace_font_rows(Some(&installed), "Arial", "");
        assert_eq!(rows[2], FontRow::Custom("Arial".to_string()));
        assert_eq!(monospace_label("Arial", Some(&installed)), "Arial");
    }

    #[test]
    fn a_saved_font_that_is_missing_says_so() {
        let installed = mono_installed();
        let rows = monospace_font_rows(Some(&installed), "Gone Mono", "");
        assert_eq!(rows[2], FontRow::NotInstalled("Gone Mono".to_string()));
        assert_eq!(
            monospace_label("Gone Mono", Some(&installed)),
            "Gone Mono (not installed)"
        );
        // Not called missing before anyone has looked.
        assert_eq!(monospace_label("Gone Mono", None), "Gone Mono");
    }

    #[test]
    fn a_saved_font_the_list_has_is_listed_once() {
        let installed = mono_installed();
        let rows = monospace_font_rows(Some(&installed), "menlo", "");
        assert_eq!(rows.iter().filter(|row| row.value() == "Menlo").count(), 1);
        assert!(!rows.iter().any(|row| matches!(row, FontRow::Custom(_))));
    }

    #[test]
    fn typing_a_name_the_list_lacks_offers_to_use_it() {
        let installed = mono_installed();
        let rows = monospace_font_rows(Some(&installed), "", " Iosevka Term ");
        assert_eq!(rows, vec![FontRow::UseTyped("Iosevka Term".to_string())]);
        assert_eq!(rows[0].value(), "Iosevka Term");
    }

    #[test]
    fn typing_a_listed_name_does_not_offer_it_twice() {
        let installed = mono_installed();
        let rows = monospace_font_rows(Some(&installed), "", "menlo");
        assert_eq!(values(&rows), ["Menlo"]);
    }

    /// Pins the routing: a family named anywhere in the app's drawing code is either replaced by
    /// the app font or deliberately monospace, so a new one cannot slip past the pick unnoticed.
    #[test]
    fn every_named_family_is_classified() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut unknown = Vec::new();
        let mut files = vec![source];
        while let Some(path) = files.pop() {
            if path.is_dir() {
                for entry in std::fs::read_dir(&path).unwrap() {
                    files.push(entry.unwrap().path());
                }
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for line in text
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
            {
                for name in named_families(line) {
                    if !PROPORTIONAL_FAMILIES.contains(&name.as_str())
                        && !MONOSPACE_FAMILIES.contains(&name.as_str())
                    {
                        unknown.push(format!("{}: {name}", path.display()));
                    }
                }
            }
        }
        assert!(
            unknown.is_empty(),
            "unclassified font families: {unknown:#?}"
        );
    }

    /// The string literals on a line that sets a font family.
    fn named_families(line: &str) -> Vec<String> {
        const SETTERS: [&str; 5] = [
            "font_family(\"",
            "font(\"",
            "font_family: \"",
            "_FONT: &str = \"",
            "_FONT_FAMILY: &str = \"",
        ];
        SETTERS
            .iter()
            .filter_map(|setter| line.split_once(setter))
            .filter_map(|(_, rest)| rest.split_once('"'))
            .map(|(name, _)| name.to_string())
            // `ICON_FONT` and its kind name an icon file, not a family.
            .filter(|name| !name.contains('/'))
            .collect()
    }
}
