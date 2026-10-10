//! What the onboarding reads and writes: the detected agents, the settings it shows, the choices
//! that live only for the length of the flow, and the helpers the panels share
//! (packages/core-ui/onboarding/onboarding-state.ts (deleted 2026-10-01), contract.ts, apps/desktop/views/onboarding-host-adapter.ts (deleted 2026-10-01)).
use serde_json::{Map, Value};

/// README section listing every supported agent CLI; the Install guide popup and the finished screen open it.
pub(crate) const INSTALL_GUIDE_URL: &str =
    "https://github.com/maddada/Ghostex#supports-all-of-the-popular-agent-clis";

/// CDXC:Onboarding 2026-09-28 DECISION:
/// User: "We need to implement installing claude/codex/cursor/grok through the setup flow if not installed, and from the agents page in settings." These four always have a row on the Agents panel; a missing one shows an Install button that runs the same gxserver CLI job Settings > Agents uses, with each agent's official installer (PowerShell on Windows). Every other catalog agent installs from the Install guide popup.
pub(crate) const PRIMARY_AGENTS: [(&str, &str); 4] = [
    ("claude", "Claude Code"),
    ("codex", "Codex CLI"),
    ("cursor", "Cursor Agent"),
    ("grok", "Grok Build"),
];

/// The workspace views the Workspace panel toggles, each `<view>ViewTabHidden` in settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ViewKey {
    Browser,
    Docs,
    Code,
    Kanban,
    Automate,
}

pub(crate) const VIEW_KEYS: [ViewKey; 5] = [
    ViewKey::Browser,
    ViewKey::Docs,
    ViewKey::Code,
    ViewKey::Kanban,
    ViewKey::Automate,
];

impl ViewKey {
    pub(crate) fn hidden_key(self) -> &'static str {
        match self {
            ViewKey::Browser => "browserViewTabHidden",
            ViewKey::Docs => "docsViewTabHidden",
            ViewKey::Code => "codeViewTabHidden",
            ViewKey::Kanban => "kanbanViewTabHidden",
            ViewKey::Automate => "automateViewTabHidden",
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            ViewKey::Browser => 0,
            ViewKey::Docs => 1,
            ViewKey::Code => 2,
            ViewKey::Kanban => 3,
            ViewKey::Automate => 4,
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            ViewKey::Browser => "Browser",
            ViewKey::Docs => "Files",
            ViewKey::Code => "Code",
            ViewKey::Kanban => "Kanban",
            ViewKey::Automate => "Automate",
        }
    }
}

/// One agent the host's detection reported (`OnboardingDetectedAgent`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DetectedAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
    pub(crate) account_label: Option<String>,
    pub(crate) installed: bool,
    pub(crate) hooks_installed: bool,
    pub(crate) detail: Option<String>,
    /// The hook status row said `updateRequired`.
    pub(crate) update_required: bool,
}

/// A sidebar catalog agent: id, display name and logo (`DEFAULT_SIDEBAR_AGENTS`).
#[derive(Clone, Debug)]
pub(crate) struct CatalogAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
    pub(crate) icon: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComputerUseState {
    Off,
    Installing,
    /// Cua Driver is installed but the Accessibility / Screen Recording grants are still missing.
    Permissions,
    On,
}

fn display_name_override(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "claude" => Some("Claude Code"),
        "codex" => Some("Codex CLI"),
        _ => None,
    }
}

fn account_label(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "claude" => Some("uses your Claude account"),
        "codex" => Some("uses your ChatGPT account"),
        "cursor" => Some("uses your Cursor account"),
        _ => None,
    }
}

/// `buildOnboardingDetectedAgents`: the host's `agentHookStatus` rows in the onboarding's shape.
pub(crate) fn detected_agents_from_hook_status(
    payload: &Value,
    catalog: &[CatalogAgent],
) -> Vec<DetectedAgent> {
    let Some(rows) = payload.get("agents").and_then(Value::as_array) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let agent_id = row.get("agentId").and_then(Value::as_str)?.to_string();
            let detail = row
                .get("detail")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|detail| !detail.is_empty())
                .map(str::to_string);
            let name = display_name_override(&agent_id)
                .map(str::to_string)
                .or_else(|| {
                    catalog
                        .iter()
                        .find(|agent| agent.agent_id == agent_id)
                        .map(|agent| agent.name.clone())
                })
                .unwrap_or_else(|| agent_id.clone());
            Some(DetectedAgent {
                account_label: account_label(&agent_id).map(str::to_string),
                installed: row.get("cliInstalled").and_then(Value::as_bool) == Some(true),
                hooks_installed: row.get("hookInstalled").and_then(Value::as_bool) == Some(true),
                update_required: row.get("status").and_then(Value::as_str)
                    == Some("updateRequired"),
                agent_id,
                name,
                detail,
            })
        })
        .collect()
}

/// `deriveOnboardingComputerUseState`: installed and granted, installed and waiting for a grant,
/// installing because the user asked, or off.
pub(crate) fn computer_use_state(
    cli_status: Option<&Value>,
    install_requested: bool,
) -> ComputerUseState {
    let flag = |key: &str| {
        cli_status
            .and_then(|status| status.get(key))
            .and_then(Value::as_bool)
    };
    let installed =
        flag("cuaDriverInstalled") == Some(true) && flag("computerUseSkillInstalled") == Some(true);
    if installed {
        let missing = flag("cuaDriverAccessibilityPermissionGranted") == Some(false)
            || flag("cuaDriverScreenRecordingPermissionGranted") == Some(false);
        return if missing {
            ComputerUseState::Permissions
        } else {
            ComputerUseState::On
        };
    }
    if install_requested {
        ComputerUseState::Installing
    } else {
        ComputerUseState::Off
    }
}

/// The settings the onboarding shows, normalized the way `normalizeghostexSettings` reads them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OnboardingSettings {
    pub(crate) view_hidden: [bool; 5],
    pub(crate) notify: bool,
    /// Floating Capture (`ghostexCaptureEnabled`).
    pub(crate) floating_capture: bool,
    pub(crate) preferred_interface: String,
    pub(crate) default_prompt_agent_id: String,
    pub(crate) sidebar_theme: String,
    pub(crate) dark_theme_preset: String,
    pub(crate) light_theme_preset: String,
    pub(crate) sidebar_contrast: f64,
    pub(crate) work_area_contrast: f64,
    pub(crate) window_glass: String,
    pub(crate) glass_sidebar_dark: f64,
    pub(crate) glass_work_area_dark: f64,
    pub(crate) glass_sidebar_light: f64,
    pub(crate) glass_work_area_light: f64,
}

pub(crate) const DARK_PRESETS: [(&str, &str); 16] = [
    ("gray", "Graphite"),
    ("black", "Black"),
    ("slate", "Slate"),
    ("midnight", "Midnight"),
    ("blue", "Blue"),
    ("indigo", "Indigo"),
    ("teal", "Teal"),
    ("green", "Green"),
    ("forest", "Forest"),
    ("olive", "Olive"),
    ("amber", "Amber"),
    ("orange", "Orange"),
    ("red", "Red"),
    ("rose", "Rose"),
    ("pink", "Pink"),
    ("purple", "Purple"),
];
pub(crate) const LIGHT_PRESETS: [(&str, &str); 16] = [
    ("gray", "Graphite"),
    ("white", "White"),
    ("slate", "Slate"),
    ("midnight", "Midnight"),
    ("blue", "Blue"),
    ("indigo", "Indigo"),
    ("teal", "Teal"),
    ("green", "Green"),
    ("forest", "Forest"),
    ("olive", "Olive"),
    ("amber", "Amber"),
    ("orange", "Orange"),
    ("red", "Red"),
    ("rose", "Rose"),
    ("pink", "Pink"),
    ("purple", "Purple"),
];

/// `themePresetLabel`.
pub(crate) fn theme_preset_label(dark: bool, preset: &str) -> String {
    let presets = if dark { &DARK_PRESETS } else { &LIGHT_PRESETS };
    if preset == "custom" {
        return "Custom".to_string();
    }
    presets
        .iter()
        .find(|(value, _)| *value == preset)
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| preset.to_string())
}

/// `LIGHT_PRESET_FOR_DARK` / `DARK_PRESET_FOR_LIGHT`: the same colour in the other appearance.
pub(crate) fn matching_preset(from_dark: bool, preset: &str) -> Option<&'static str> {
    let (from, to) = if from_dark {
        (&DARK_PRESETS, &LIGHT_PRESETS)
    } else {
        (&LIGHT_PRESETS, &DARK_PRESETS)
    };
    let index = from.iter().position(|(value, _)| *value == preset)?;
    Some(to[index].0)
}

fn read_bool(object: &Map<String, Value>, key: &str, fallback: bool) -> bool {
    object.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

fn read_number(object: &Map<String, Value>, key: &str) -> Option<f64> {
    object
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

/// `readThemeContrastPoints`.
pub(crate) fn theme_contrast_points(object: &Map<String, Value>, key: &str) -> f64 {
    if let Some(points) = read_number(object, key) {
        return ((points * 2.0).round() / 2.0).clamp(-12.0, 4.0);
    }
    match read_number(object, "themeContrast")
        .map_or(0, |value| (value.round() as i64).clamp(-2, 2))
    {
        -2 => -8.0,
        -1 => -4.0,
        1 => 2.0,
        2 => 4.0,
        _ => 0.0,
    }
}

impl OnboardingSettings {
    /// Reads a settings object with the TypeScript defaults. The theme presets fall back to Graphite
    /// here; the app passes presets already migrated by its own theme reader.
    pub(crate) fn from_object(object: &Map<String, Value>) -> Self {
        let text = |key: &str| object.get(key).and_then(Value::as_str).map(str::trim);
        let sidebar_theme = match text("sidebarTheme") {
            None => "system".to_string(),
            Some(value @ ("plain-light" | "system")) => value.to_string(),
            Some(_) => "dark-2".to_string(),
        };
        let preset = |key: &str, presets: &[(&str, &str); 16]| {
            text(key)
                .filter(|value| *value == "custom" || presets.iter().any(|(id, _)| id == value))
                .unwrap_or("gray")
                .to_string()
        };
        let sidebar = |key: &str, fallback: f64| {
            read_number(object, key)
                .map(|value| value.clamp(0.0, 100.0).round())
                .unwrap_or(fallback)
        };
        let glass_sidebar_dark = sidebar("windowGlassSidebarOpacityDark", 94.0);
        let glass_sidebar_light = sidebar("windowGlassSidebarOpacityLight", 97.0);
        let work_area = |key: &str, legacy: &str, sidebar_percent: f64, fallback: f64| {
            read_number(object, key)
                .or_else(|| {
                    read_number(object, legacy).map(|extra| {
                        let sidebar = sidebar_percent / 100.0;
                        let extra = extra.clamp(0.0, 100.0) / 100.0;
                        (1.0 - (1.0 - sidebar) * (1.0 - extra)) * 100.0
                    })
                })
                .map(|value| value.clamp(0.0, 100.0).round())
                .unwrap_or(fallback)
        };
        Self {
            view_hidden: VIEW_KEYS.map(|key| read_bool(object, key.hidden_key(), false)),
            notify: read_bool(object, "showMacOSAttentionNotifications", true),
            floating_capture: read_bool(object, "ghostexCaptureEnabled", false),
            preferred_interface: match text("preferredAgentInterface") {
                Some("terminal") => "terminal".to_string(),
                _ => "chat".to_string(),
            },
            default_prompt_agent_id: text("defaultPromptAgentId")
                .filter(|value| !value.is_empty())
                .unwrap_or("codex")
                .chars()
                .take(120)
                .collect(),
            sidebar_theme,
            dark_theme_preset: preset("darkThemePreset", &DARK_PRESETS),
            light_theme_preset: preset("lightThemePreset", &LIGHT_PRESETS),
            sidebar_contrast: theme_contrast_points(object, "themeSidebarContrast"),
            work_area_contrast: theme_contrast_points(object, "themeWorkAreaContrast"),
            window_glass: match text("windowGlass").or_else(|| {
                ghostex_settings_catalog::availability::default_value_on(
                    ghostex_settings_catalog::Platform::current(),
                    "windowGlass",
                )
                .and_then(ghostex_settings_catalog::J::as_str)
            }) {
                Some(value @ ("frosted" | "opaque")) => value.to_string(),
                _ => "auto".to_string(),
            },
            glass_sidebar_dark,
            glass_work_area_dark: work_area(
                "windowGlassWorkAreaTintDark",
                "windowGlassMainOpacityDark",
                glass_sidebar_dark,
                91.0,
            ),
            glass_sidebar_light,
            glass_work_area_light: work_area(
                "windowGlassWorkAreaTintLight",
                "windowGlassMainOpacityLight",
                glass_sidebar_light,
                93.0,
            ),
        }
    }

    /// Applies a patch the onboarding just saved, so the page shows it before the host echoes it back.
    pub(crate) fn apply_patch(&mut self, patch: &Map<String, Value>) {
        for (index, key) in VIEW_KEYS.iter().enumerate() {
            if let Some(value) = patch.get(key.hidden_key()).and_then(Value::as_bool) {
                self.view_hidden[index] = value;
            }
        }
        let text = |key: &str| patch.get(key).and_then(Value::as_str).map(str::to_string);
        let number = |key: &str| patch.get(key).and_then(Value::as_f64);
        if let Some(value) = patch
            .get("showMacOSAttentionNotifications")
            .and_then(Value::as_bool)
        {
            self.notify = value;
        }
        if let Some(value) = patch.get("ghostexCaptureEnabled").and_then(Value::as_bool) {
            self.floating_capture = value;
        }
        if let Some(value) = text("preferredAgentInterface") {
            self.preferred_interface = value;
        }
        if let Some(value) = text("defaultPromptAgentId") {
            self.default_prompt_agent_id = value;
        }
        if let Some(value) = text("sidebarTheme") {
            self.sidebar_theme = value;
        }
        if let Some(value) = text("darkThemePreset") {
            self.dark_theme_preset = value;
        }
        if let Some(value) = text("lightThemePreset") {
            self.light_theme_preset = value;
        }
        if let Some(value) = number("themeSidebarContrast") {
            self.sidebar_contrast = value;
        }
        if let Some(value) = number("themeWorkAreaContrast") {
            self.work_area_contrast = value;
        }
        if let Some(value) = text("windowGlass") {
            self.window_glass = value;
        }
        if let Some(value) = number("windowGlassSidebarOpacityDark") {
            self.glass_sidebar_dark = value;
        }
        if let Some(value) = number("windowGlassWorkAreaTintDark") {
            self.glass_work_area_dark = value;
        }
        if let Some(value) = number("windowGlassSidebarOpacityLight") {
            self.glass_sidebar_light = value;
        }
        if let Some(value) = number("windowGlassWorkAreaTintLight") {
            self.glass_work_area_light = value;
        }
    }

    pub(crate) fn is_view_on(&self, key: ViewKey) -> bool {
        !self.view_hidden[key.index()]
    }
}

/// `withViewsOn`: the settings patch that turns the given views on or off.
pub(crate) fn views_patch(changes: &[(ViewKey, bool)]) -> Map<String, Value> {
    let mut patch = Map::new();
    for (key, on) in changes {
        patch.insert(key.hidden_key().to_string(), Value::Bool(!on));
    }
    patch
}

/// Choices that live only for the length of the flow (everything else is read from and written to settings).
#[derive(Clone, Debug)]
pub(crate) struct FlowState {
    /// The "Ghostex integration" switch on the Agents panel.
    pub(crate) integration_on: bool,
    /// Set once "Connect & continue" asked the host to install the agent helper.
    pub(crate) hooks_requested: bool,
    /// "Open after onboarding" in the Install guide popup.
    pub(crate) install_queued: bool,
    /// Agent id or `terminal` for the first session; unset until the user picks one.
    pub(crate) start_with: Option<String>,
    /// CDXC:Onboarding 2026-09-11 WHY:
    /// "Pair this computer" used to open Settings -> Remote at once, which replaces the onboarding window and ends the flow before the Get started panel. The click now only queues the phone step and `finish` opens Remote settings right before closing.
    pub(crate) phone_queued: bool,
    /// The folder handed to the first-launch create; shown on the finished screen.
    pub(crate) finished_path: Option<String>,
    /// Shows the "You're set" screen in place of the last panel; any page change clears it.
    pub(crate) finished: bool,
}

impl Default for FlowState {
    fn default() -> Self {
        Self {
            integration_on: true,
            hooks_requested: false,
            install_queued: false,
            start_with: None,
            phone_queued: false,
            finished_path: None,
            finished: false,
        }
    }
}

pub(crate) fn installed_agents(agents: &[DetectedAgent]) -> Vec<&DetectedAgent> {
    agents.iter().filter(|agent| agent.installed).collect()
}

/// The default prompt agent when it is installed, otherwise the first installed agent.
pub(crate) fn default_agent_id(
    settings: &OnboardingSettings,
    agents: &[DetectedAgent],
) -> Option<String> {
    let installed = installed_agents(agents);
    let configured = &settings.default_prompt_agent_id;
    if installed.iter().any(|agent| &agent.agent_id == configured) {
        return Some(configured.clone());
    }
    installed.first().map(|agent| agent.agent_id.clone())
}

pub(crate) fn agent_display_name(agents: &[DetectedAgent], agent_id: Option<&str>) -> String {
    match agent_id {
        None => "No agent".to_string(),
        Some("terminal") => "Terminal".to_string(),
        Some(agent_id) => agents
            .iter()
            .find(|agent| agent.agent_id == agent_id)
            .map(|agent| agent.name.clone())
            .unwrap_or_else(|| agent_id.to_string()),
    }
}

pub(crate) fn all_installed_have_hooks(agents: &[DetectedAgent]) -> bool {
    let installed = installed_agents(agents);
    !installed.is_empty() && installed.iter().all(|agent| agent.hooks_installed)
}

/// Display name for any catalog agent: the host's detection name when it reported one, else the catalog.
pub(crate) fn catalog_agent_name(
    agents: &[DetectedAgent],
    catalog: &[CatalogAgent],
    agent_id: &str,
) -> String {
    agents
        .iter()
        .find(|agent| agent.agent_id == agent_id)
        .map(|agent| agent.name.clone())
        .or_else(|| {
            PRIMARY_AGENTS
                .iter()
                .find(|(id, _)| *id == agent_id)
                .map(|(_, name)| name.to_string())
        })
        .or_else(|| {
            catalog
                .iter()
                .find(|agent| agent.agent_id == agent_id)
                .map(|agent| agent.name.clone())
        })
        .unwrap_or_else(|| agent_id.to_string())
}

/// One row of packages/shared/agent-cli-catalog.json, the fields the Install guide reads.
#[derive(Clone, Debug)]
pub(crate) struct CliCatalogEntry {
    pub(crate) agent_id: String,
    pub(crate) docs_url: String,
    /// `agentCliCatalogInstallCommand`: the command shown when gxserver cannot list the real methods.
    pub(crate) install_command: Option<String>,
}

/// The catalog gxserver installs from (`AGENT_CLI_CATALOG`), in file order.
pub(crate) fn cli_catalog() -> Vec<CliCatalogEntry> {
    let entries: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../../../packages/shared/agent-cli-catalog.json"
    ))
    .unwrap_or_default();
    entries
        .iter()
        .filter_map(|entry| {
            let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_string);
            let list = |key: &str| {
                entry
                    .get(key)
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            };
            let install_command = if let Some(package) = text("npmPackage") {
                let manager = list("packageManagers")
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "npm".to_string());
                let verb = if manager == "pnpm" { "add" } else { "install" };
                let flags = list("npmFlags");
                let flags = if manager == "npm" && !flags.is_empty() {
                    format!("{} ", flags.join(" "))
                } else {
                    String::new()
                };
                Some(format!("{manager} {verb} -g {flags}{package}@latest"))
            } else if let Some(native) = entry
                .get("native")
                .and_then(|native| native.get("install"))
                .and_then(Value::as_str)
            {
                Some(native.to_string())
            } else {
                text("brewFormula").map(|formula| {
                    let cask = entry.get("brewCask").and_then(Value::as_bool) == Some(true);
                    format!(
                        "brew install {}{formula}",
                        if cask { "--cask " } else { "" }
                    )
                })
            };
            Some(CliCatalogEntry {
                agent_id: text("agentId")?,
                docs_url: text("docsUrl").unwrap_or_default(),
                install_command,
            })
        })
        .collect()
}

/// One theme colour square's inputs: the preset's chrome at each Colourfulness step (Subtle ...
/// Vivid) and its accent.
#[derive(Clone, Debug)]
pub(crate) struct ThemeSwatchPreset {
    pub(crate) value: String,
    /// The chrome at each Colourfulness position (`COLOURFULNESS_LAST_POSITION + 1` entries).
    pub(crate) chrome: Vec<u32>,
    pub(crate) accent: u32,
}

/// The chrome colours the look card draws its squares from, and the settings patch each
/// Colourfulness position writes. Built by the host from the same theme math the window paints with.
#[derive(Clone, Debug, Default)]
pub(crate) struct ThemeTable {
    pub(crate) dark: Vec<ThemeSwatchPreset>,
    pub(crate) light: Vec<ThemeSwatchPreset>,
    pub(crate) colourfulness_patches: Vec<Map<String, Value>>,
}

/// `COLOURFULNESS_CHOICES`: the named points; the slider moves in half points between them
/// (CDXC:Theming in apps/desktop/src/app/window/settings_modal/tabs/theme/colours.rs).
pub(crate) const COLOURFULNESS: [(&str, f64); 5] = [
    ("Subtle", 4.0),
    ("Soft", 0.0),
    ("Balanced", -4.0),
    ("Rich", -8.0),
    ("Vivid", -12.0),
];

/// The last Colourfulness position: 0 is Subtle (+4), 32 is Vivid (-12), half a point apart.
pub(crate) const COLOURFULNESS_LAST_POSITION: usize = 32;

/// The contrast points one Colourfulness position sets.
pub(crate) fn colourfulness_points(position: usize) -> f64 {
    4.0 - position.min(COLOURFULNESS_LAST_POSITION) as f64 * 0.5
}

/// The name of the named point nearest to `points`.
pub(crate) fn colourfulness_name(points: f64) -> &'static str {
    let mut best = 0;
    for (index, (_, value)) in COLOURFULNESS.iter().enumerate() {
        if (value - points).abs() < (COLOURFULNESS[best].1 - points).abs() {
            best = index;
        }
    }
    COLOURFULNESS[best].0
}

/// `colourfulnessStepIndex`: the one position both areas share, or `None` once they are set apart or between positions.
pub(crate) fn colourfulness_step(settings: &OnboardingSettings) -> Option<usize> {
    if settings.sidebar_contrast != settings.work_area_contrast {
        return None;
    }
    let position = (4.0 - settings.sidebar_contrast) / 0.5;
    (position.fract() == 0.0 && (0.0..=COLOURFULNESS_LAST_POSITION as f64).contains(&position))
        .then_some(position as usize)
}

/// `colourfulnessDisplayStep`: the Colourfulness position nearest to `points`.
pub(crate) fn colourfulness_display_step(points: f64) -> usize {
    if !points.is_finite() {
        return colourfulness_display_step(0.0);
    }
    ((4.0 - points) / 0.5)
        .round()
        .clamp(0.0, COLOURFULNESS_LAST_POSITION as f64) as usize
}

const TRANSPARENCY_WORK_AREA_GAP: f64 = 7.0;
const TRANSPARENCY_STRENGTH_DEFAULT: f64 = 20.0;
pub(crate) const TRANSPARENCY_STRENGTH_STEP: f64 = 5.0;

fn transparency_sidebar_tint(strength: f64, at_default: f64) -> f64 {
    if strength <= TRANSPARENCY_STRENGTH_DEFAULT {
        return 100.0 - ((100.0 - at_default) * strength) / TRANSPARENCY_STRENGTH_DEFAULT;
    }
    (at_default * (100.0 - strength)) / (100.0 - TRANSPARENCY_STRENGTH_DEFAULT)
}

fn transparency_work_area_gap(strength: f64) -> f64 {
    let ramp = (strength / TRANSPARENCY_STRENGTH_DEFAULT)
        .min((100.0 - strength) / TRANSPARENCY_STRENGTH_DEFAULT)
        .min(1.0);
    TRANSPARENCY_WORK_AREA_GAP * ramp.max(0.0)
}

/// `transparencyStrengthPatch`: the four glass tints one strength (0-100) sets.
pub(crate) fn transparency_strength_patch(strength: f64) -> Map<String, Value> {
    let value = strength.clamp(0.0, 100.0);
    let gap = transparency_work_area_gap(value);
    let tints = |at_default: f64| {
        let sidebar = transparency_sidebar_tint(value, at_default);
        (sidebar.round(), (sidebar - gap).round().max(0.0))
    };
    let (dark_sidebar, dark_work) = tints(88.0);
    let (light_sidebar, light_work) = tints(93.0);
    let mut patch = Map::new();
    patch.insert(
        "windowGlassSidebarOpacityDark".into(),
        js_number(dark_sidebar),
    );
    patch.insert("windowGlassWorkAreaTintDark".into(), js_number(dark_work));
    patch.insert(
        "windowGlassSidebarOpacityLight".into(),
        js_number(light_sidebar),
    );
    patch.insert("windowGlassWorkAreaTintLight".into(), js_number(light_work));
    patch
}

/// A whole number is written as a JSON integer, the way `JSON.stringify` writes it.
pub(crate) fn js_number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        Value::from(value as i64)
    } else {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

/// `transparencyStrengthFromSettings`: where the slider sits, and whether the tints are exactly one of its stops.
pub(crate) fn transparency_strength(settings: &OnboardingSettings) -> (Option<f64>, f64) {
    let sidebar = settings.glass_sidebar_dark;
    let at_default = 88.0;
    let raw = if sidebar >= at_default {
        ((100.0 - sidebar) * TRANSPARENCY_STRENGTH_DEFAULT) / (100.0 - at_default)
    } else {
        100.0 - (sidebar * (100.0 - TRANSPARENCY_STRENGTH_DEFAULT)) / at_default
    };
    let nearest =
        ((raw / TRANSPARENCY_STRENGTH_STEP).round() * TRANSPARENCY_STRENGTH_STEP).clamp(0.0, 100.0);
    let patch = transparency_strength_patch(nearest);
    let matches = |key: &str, value: f64| patch.get(key).and_then(Value::as_f64) == Some(value);
    let exact = (matches("windowGlassSidebarOpacityDark", settings.glass_sidebar_dark)
        && matches("windowGlassWorkAreaTintDark", settings.glass_work_area_dark)
        && matches(
            "windowGlassSidebarOpacityLight",
            settings.glass_sidebar_light,
        )
        && matches(
            "windowGlassWorkAreaTintLight",
            settings.glass_work_area_light,
        ))
    .then_some(nearest);
    (exact, nearest)
}
