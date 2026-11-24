/*
CDXC:Telemetry 2026-11-20:
The daily `settings.snapshot`: what an install looks like, sent once a day with
the heartbeat (same `analytics.state.v1` clock, same gate).

User: "add PostHog tracking ... for settings like transparency, which Linux
distro is used, and other useful settings; how many accounts people have and
which subscription; what kind of sidebar people use and their sidebar settings;
all these and more." This event answers those questions, and only with values
that cannot name anyone:

- A select setting is sent as one of the choices Settings itself lists for it
  (`setting_choices`, read from the settings catalog's own rows), so a value a
  user typed, or one Settings does not offer, is left out rather than sent.
- A switch is sent as true/false.
- A number (a tint, a zoom, a width) is sent as a bucket, never the raw value.
- A wallpaper, a video or theme colours are sent only as "set or not", never a
  path or a colour.
- Linux distro and desktop come from closed tables (`taxonomy::LINUX_*`), with
  `other` for anything else.
- Accounts are counts per provider and the plan each provider reports, mapped
  onto `taxonomy::CLAUDE_PLANS` / `CODEX_PLANS`; never an email, name, org or id.
- Extensions: official Store ids with their version, and a count of the rest.
- Boosts: on or off, and how many of the user's own Boosts and edited prompts
  there are; never their text.
- Small counts (accounts, workspaces, Boosts, extensions) go as
  `taxonomy::COUNT_BUCKETS` ranges, never the number.

Every key below is checked again by `taxonomy::validate` through
`property_spec`, so this table IS the whitelist: a key not listed here cannot
be sent on this event.
*/

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::OnceLock,
};

use ghostex_settings_catalog::{J, Platform, availability};
use serde_json::Value;

use super::{
    extensions::ExtensionRef,
    queue::capture,
    taxonomy::{self, PropertySpec, PropertyValue},
};

/// One bucketed number: the label of the first upper bound the value is at or
/// below, else the last label.
pub struct Buckets {
    pub uppers: &'static [f64],
    pub labels: &'static [&'static str],
}

/// Tints, opacities and strengths that run 0 to 100.
const PERCENT: Buckets = Buckets {
    uppers: &[0.0, 25.0, 50.0, 75.0, 90.0, 99.0],
    labels: &["0", "1-25", "26-50", "51-75", "76-90", "91-99", "100"],
};

/// Zoom and scale percentages, where 100 is the default.
const ZOOM: Buckets = Buckets {
    uppers: &[89.0, 99.0, 100.0, 110.0, 125.0],
    labels: &["<90", "90-99", "100", "101-110", "111-125", ">125"],
};

/// Blur radii.
const BLUR: Buckets = Buckets {
    uppers: &[0.0, 5.0, 10.0, 20.0, 40.0],
    labels: &["0", "1-5", "6-10", "11-20", "21-40", ">40"],
};

/// The sidebar's width in points.
const WIDTH: Buckets = Buckets {
    uppers: &[219.0, 259.0, 299.0, 359.0],
    labels: &["<220", "220-259", "260-299", "300-359", "360+"],
};

impl Buckets {
    fn label(&self, value: f64) -> &'static str {
        self.uppers
            .iter()
            .position(|upper| value <= *upper)
            .map(|index| self.labels[index])
            .unwrap_or(self.labels[self.labels.len() - 1])
    }
}

/// How one settings key is reported.
pub enum Kind {
    /// A select: its value, if Settings lists it as a choice.
    Choice,
    /// A switch.
    Toggle,
    /// A number, bucketed.
    Bucket(&'static Buckets),
    /// Whether the setting holds anything: a picture, a video, theme colours.
    Present,
}

/// One reported setting: the property name PostHog shows and the settings key it is read from.
pub struct Field {
    pub property: &'static str,
    pub setting: &'static str,
    pub kind: Kind,
}

const fn field(property: &'static str, setting: &'static str, kind: Kind) -> Field {
    Field {
        property,
        setting,
        kind,
    }
}

/// The settings the snapshot reports. Keep it to settings a product question
/// needs; every row here is one more property on every install's daily event.
pub const FIELDS: &[Field] = &[
    // Look.
    field("appearance", "sidebarTheme", Kind::Choice),
    field("dark_theme_preset", "darkThemePreset", Kind::Choice),
    field("light_theme_preset", "lightThemePreset", Kind::Choice),
    field("theme_colours_set", "themeRoles", Kind::Present),
    field("transparency", "windowGlass", Kind::Choice),
    field("glass_source", "windowGlassSource", Kind::Choice),
    field("glass_blur", "windowGlassBlurRadius", Kind::Bucket(&BLUR)),
    field(
        "glass_menu_blur",
        "windowGlassMenuBlurRadius",
        Kind::Bucket(&BLUR),
    ),
    field(
        "glass_sidebar_tint_dark",
        "windowGlassSidebarOpacityDark",
        Kind::Bucket(&PERCENT),
    ),
    field(
        "glass_work_area_tint_dark",
        "windowGlassWorkAreaTintDark",
        Kind::Bucket(&PERCENT),
    ),
    field(
        "glass_sidebar_tint_light",
        "windowGlassSidebarOpacityLight",
        Kind::Bucket(&PERCENT),
    ),
    field(
        "glass_work_area_tint_light",
        "windowGlassWorkAreaTintLight",
        Kind::Bucket(&PERCENT),
    ),
    field(
        "glass_live_style_dark",
        "windowGlassLiveStyleDark",
        Kind::Choice,
    ),
    field(
        "glass_live_style_light",
        "windowGlassLiveStyleLight",
        Kind::Choice,
    ),
    field(
        "glass_picture_dark_set",
        "windowGlassImageDark",
        Kind::Present,
    ),
    field(
        "glass_picture_light_set",
        "windowGlassImageLight",
        Kind::Present,
    ),
    field(
        "glass_video_dark_set",
        "windowGlassVideoDark",
        Kind::Present,
    ),
    field(
        "glass_video_light_set",
        "windowGlassVideoLight",
        Kind::Present,
    ),
    field("chat_theme", "sessionChatTheme", Kind::Choice),
    field("terminal_theme", "terminalColorScheme", Kind::Choice),
    field(
        "terminal_palette_dark",
        "terminalGhosttyTheme",
        Kind::Choice,
    ),
    field(
        "interface_scale",
        "interfaceScalePercent",
        Kind::Bucket(&ZOOM),
    ),
    field("chat_zoom", "sessionChatZoomPercent", Kind::Bucket(&ZOOM)),
    field(
        "sidebar_zoom",
        "agentManagerZoomPercent",
        Kind::Bucket(&ZOOM),
    ),
    field("animations", "panelAnimationSpeed", Kind::Choice),
    // Sidebar.
    field("sidebar_view", "sidebarView", Kind::Choice),
    field("sidebar_layout", "sidebarLayout", Kind::Choice),
    field("sidebar_preset", "sidebarSettingsPreset", Kind::Choice),
    field(
        "sidebar_width",
        "sidebarDefaultWidthPx",
        Kind::Bucket(&WIDTH),
    ),
    field("sidebar_session_links", "sidebarSessionLinks", Kind::Choice),
    field("sidebar_spaces", "sidebarSpacesEnabled", Kind::Toggle),
    field(
        "sidebar_separate_by_machine",
        "sidebarSeparateByMachine",
        Kind::Toggle,
    ),
    field(
        "sidebar_merge_repository_copies",
        "sidebarMergeRepositoryCopies",
        Kind::Toggle,
    ),
    field(
        "sidebar_hide_projects_without_sessions",
        "sidebarHideProjectsWithoutSessions",
        Kind::Toggle,
    ),
    field(
        "unified_group_by_project",
        "unifiedGroupByProject",
        Kind::Toggle,
    ),
    field(
        "unified_moving_section",
        "unifiedWorkingSection",
        Kind::Toggle,
    ),
    field("unified_session_order", "unifiedSessionOrder", Kind::Choice),
    field("pinned_session_order", "pinnedSessionOrder", Kind::Choice),
    field(
        "group_working_sessions",
        "groupWorkingSessions",
        Kind::Toggle,
    ),
    field("session_parking", "enableSessionParking", Kind::Toggle),
    field(
        "session_tooltips",
        "showSidebarSessionTooltips",
        Kind::Toggle,
    ),
    field("card_project_icons", "showProjectIcons", Kind::Toggle),
    field(
        "card_hide_last_active_time",
        "hideLastActiveTimeOnSessionCards",
        Kind::Toggle,
    ),
    field(
        "card_hide_agent_icon",
        "hideSessionAgentIconUntilHover",
        Kind::Toggle,
    ),
    field(
        "card_hide_project_git_stats",
        "hideProjectHeaderDiffStats",
        Kind::Toggle,
    ),
    field("workspaces_hidden", "workspacesHidden", Kind::Toggle),
    // Agents, chat and other notable switches.
    field(
        "chat_detailed_view",
        "sessionChatDetailedView",
        Kind::Toggle,
    ),
    field("lean_codex_workers", "leanCodexWorkers", Kind::Toggle),
    field("agent_accept_all", "agentAcceptAllEnabled", Kind::Toggle),
    field("agent_hooks", "useAgentHooks", Kind::Toggle),
    field("marked_handovers", "markedHandovers", Kind::Choice),
    field("chief_hidden", "chiefHidden", Kind::Toggle),
    field("voice_agent_hidden", "voiceAgentHidden", Kind::Toggle),
    field("floating_capture", "ghostexCaptureEnabled", Kind::Toggle),
    field(
        "keep_awake_while_working",
        "keepAwakeWhileWorkingSessions",
        Kind::Toggle,
    ),
    field("terminal_shaders", "terminalShadersEnabled", Kind::Toggle),
    field("remote_tailscale", "remoteTailscaleEnabled", Kind::Toggle),
    field(
        "windows_terminal_backend",
        "windowsTerminalBackend",
        Kind::Choice,
    ),
    field("updates_check_ghostex", "updatesCheckGhostex", Kind::Choice),
];

/// The keys this event carries that are not settings.
fn other_property_spec(key: &str) -> Option<PropertySpec> {
    Some(match key {
        "linux_distro" => PropertySpec::Enum(taxonomy::LINUX_DISTROS),
        "linux_distro_version" => PropertySpec::Version,
        "linux_desktop" => PropertySpec::Enum(taxonomy::LINUX_DESKTOPS),
        "linux_session" => PropertySpec::Enum(taxonomy::LINUX_SESSION_TYPES),
        "linux_wsl" | "boosts_on" => PropertySpec::Bool,
        "remote_machine_count" => PropertySpec::Number,
        "workspace_count"
        | "custom_boost_count"
        | "edited_boost_count"
        | "custom_extensions_installed"
        | "claude_accounts"
        | "codex_accounts"
        | "cursor_accounts"
        | "gemini_accounts"
        | "copilot_accounts" => PropertySpec::Enum(taxonomy::COUNT_BUCKETS),
        "claude_plans" => PropertySpec::EnumList(taxonomy::CLAUDE_PLANS),
        "codex_plans" => PropertySpec::EnumList(taxonomy::CODEX_PLANS),
        "claude_top_plan" => PropertySpec::Enum(taxonomy::CLAUDE_PLANS),
        "codex_top_plan" => PropertySpec::Enum(taxonomy::CODEX_PLANS),
        "antigravity_mode" | "claude_mode" | "codex_mode" => {
            PropertySpec::Enum(taxonomy::SESSION_MODES)
        }
        "store_extensions_installed" => PropertySpec::StoreExtensionVersionList,
        _ => return None,
    })
}

/// What `settings.snapshot` may carry under `key` (the taxonomy asks here).
pub(super) fn property_spec(key: &str) -> Option<PropertySpec> {
    if let Some(field) = FIELDS.iter().find(|field| field.property == key) {
        return Some(match field.kind {
            Kind::Choice => PropertySpec::SettingChoice(field.setting),
            Kind::Toggle | Kind::Present => PropertySpec::Bool,
            Kind::Bucket(buckets) => PropertySpec::Enum(buckets.labels),
        });
    }
    other_property_spec(key)
}

/// The snapshot keys also written to the person (`$set`), so any event can be
/// broken down by them.
pub const PERSON_KEYS: &[&str] = &[
    "appearance",
    "transparency",
    "sidebar_view",
    "sidebar_layout",
    "linux_distro",
    "linux_desktop",
    "claude_top_plan",
    "codex_top_plan",
    "antigravity_mode",
    "claude_mode",
    "codex_mode",
];

/*
The choices Settings lists for each `Kind::Choice` key, read once from the
settings catalog's own rows (General, Theme and the other pages) for this
platform. The catalog builds its rows with owned strings, so the values are
leaked once into `&'static str`: a fixed, small set for the life of the process,
which lets `PropertyValue::Enum` keep its "only a static string" rule.
*/
pub(super) fn setting_choices(setting: &str) -> &'static [&'static str] {
    static CHOICES: OnceLock<HashMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    CHOICES
        .get_or_init(|| {
            let platform = Platform::current();
            let wanted: BTreeSet<&'static str> = FIELDS
                .iter()
                .filter(|field| matches!(field.kind, Kind::Choice))
                .map(|field| field.setting)
                .collect();
            let mut choices: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
            let general = ghostex_settings_catalog::general_sections(platform);
            let pages = ghostex_settings_catalog::extra_pages(platform);
            let rows = general
                .iter()
                .chain(pages.iter().flat_map(|page| page.sections.iter()))
                .flat_map(|section| section.settings.iter());
            for row in rows {
                if !wanted.contains(row.key) || row.options.is_empty() {
                    continue;
                }
                let entry = choices.entry(row.key).or_default();
                for option in &row.options {
                    if !entry.iter().any(|known| *known == option.value) {
                        entry.push(Box::leak(option.value.clone().into_boxed_str()));
                    }
                }
            }
            choices
        })
        .get(setting)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// A setting's value from the settings file, or Settings' default for this
/// platform when the file has none.
fn setting_value(settings: Option<&Value>, key: &str) -> Option<Value> {
    if let Some(value) = settings.and_then(|settings| settings.get(key)) {
        if !value.is_null() {
            return Some(value.clone());
        }
    }
    match availability::default_value_on(Platform::current(), key)? {
        J::Bool(value) => Some(Value::Bool(*value)),
        J::Num(value) => serde_json::Number::from_f64(*value).map(Value::Number),
        J::Str(value) => Some(Value::String((*value).to_string())),
        // Only whether a list holds anything is ever read (`Kind::Present`).
        J::Arr(items) => Some(Value::Bool(!items.is_empty())),
        J::Obj(_) | J::Null => None,
    }
}

fn has_content(value: &Value) -> bool {
    match value {
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(entries) => !entries.is_empty(),
        Value::Bool(value) => *value,
        Value::Number(_) => true,
        Value::Null => false,
    }
}

/// The settings half of the snapshot. A value Settings would not show (an
/// unknown choice, a non-number) is left out, never guessed.
pub fn settings_properties(settings: Option<&Value>) -> Vec<(&'static str, PropertyValue)> {
    let mut properties = Vec::with_capacity(FIELDS.len());
    for field in FIELDS {
        let value = setting_value(settings, field.setting);
        let property = match field.kind {
            Kind::Choice => value
                .as_ref()
                .and_then(Value::as_str)
                .and_then(|text| {
                    setting_choices(field.setting)
                        .iter()
                        .copied()
                        .find(|choice| *choice == text)
                })
                .map(PropertyValue::Enum),
            Kind::Toggle => value
                .as_ref()
                .and_then(Value::as_bool)
                .map(PropertyValue::Bool),
            Kind::Bucket(buckets) => value
                .as_ref()
                .and_then(|value| {
                    value
                        .as_f64()
                        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
                })
                .map(|number| PropertyValue::Enum(buckets.label(number))),
            Kind::Present => Some(PropertyValue::Bool(value.as_ref().is_some_and(has_content))),
        };
        if let Some(property) = property {
            properties.push((field.property, property));
        }
    }
    properties
}

/// The Linux machine gxserver runs on. `None` off Linux.
#[derive(Clone, Debug, Default)]
pub struct LinuxReport {
    pub distro: &'static str,
    pub distro_version: Option<String>,
    pub desktop: Option<&'static str>,
    pub session: Option<&'static str>,
    pub wsl: bool,
}

/// `ID=` and `VERSION_ID=` of an `os-release` document.
fn os_release_fields(text: &str) -> (Option<String>, Option<String>) {
    let mut id = None;
    let mut version = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        match key.trim() {
            "ID" => id = Some(value),
            "VERSION_ID" => version = Some(value),
            _ => {}
        }
    }
    (id, version)
}

/// An `os-release` `ID` as a `LINUX_DISTROS` member (`other` when it is not one).
pub fn linux_distro(id: &str) -> &'static str {
    let id = id.trim().to_ascii_lowercase();
    taxonomy::LINUX_DISTROS
        .iter()
        .copied()
        .find(|known| *known == id)
        .unwrap_or("other")
}

/*
CDXC:Telemetry 2026-11-20 (privacy review of 591004b37):
`VERSION_ID` is sent only for a distro in `LINUX_DISTROS` and only as digits and
dots (`24.04`, `41`, `2024.1`), at most 16 characters. The general version clamp
let through `AcmeCorp-Internal-2`, so a company's own build could name the
company; an `other` distro sends no version at all.
*/
pub fn linux_distro_version(distro: &str, raw: &str) -> Option<String> {
    let raw = raw.trim();
    let well_formed = !raw.is_empty()
        && raw.len() <= 16
        && raw.starts_with(|character: char| character.is_ascii_digit())
        && raw
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.');
    (distro != "other" && taxonomy::LINUX_DISTROS.contains(&distro) && well_formed)
        .then(|| raw.to_string())
}

/// `XDG_CURRENT_DESKTOP` (a `:`-separated list such as `ubuntu:GNOME`) as a
/// `LINUX_DESKTOPS` member: the first entry that is one, else `other`.
pub fn linux_desktop(raw: &str) -> Option<&'static str> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let found = raw.split(':').find_map(|entry| {
        let entry = entry.trim().to_ascii_lowercase();
        let entry = entry.strip_prefix("x-").unwrap_or(&entry).to_string();
        let entry = match entry.as_str() {
            "plasma" | "kde-plasma" => "kde".to_string(),
            "pop" | "ubuntu" => return None,
            _ => entry,
        };
        taxonomy::LINUX_DESKTOPS
            .iter()
            .copied()
            .find(|known| *known == entry && *known != "other")
    });
    Some(found.unwrap_or("other"))
}

/// `XDG_SESSION_TYPE` as a `LINUX_SESSION_TYPES` member.
pub fn linux_session(raw: &str) -> Option<&'static str> {
    let raw = raw.trim().to_ascii_lowercase();
    if raw.is_empty() {
        return None;
    }
    Some(
        taxonomy::LINUX_SESSION_TYPES
            .iter()
            .copied()
            .find(|known| *known == raw)
            .unwrap_or("other"),
    )
}

pub fn read_linux_report() -> Option<LinuxReport> {
    if std::env::consts::OS != "linux" {
        return None;
    }
    let (id, version) = std::fs::read_to_string("/etc/os-release")
        .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
        .map(|text| os_release_fields(&text))
        .unwrap_or((None, None));
    let env = |name: &str| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    };
    let desktop = env("XDG_CURRENT_DESKTOP")
        .or_else(|| env("XDG_SESSION_DESKTOP"))
        .and_then(|raw| linux_desktop(&raw));
    let session = env("XDG_SESSION_TYPE")
        .and_then(|raw| linux_session(&raw))
        .or_else(|| env("WAYLAND_DISPLAY").map(|_| "wayland"));
    let wsl = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|text| text.to_ascii_lowercase().contains("microsoft"))
        .unwrap_or(false);
    let distro = id.as_deref().map(linux_distro).unwrap_or("other");
    Some(LinuxReport {
        distro,
        distro_version: version.and_then(|version| linux_distro_version(distro, &version)),
        desktop,
        session,
        wsl,
    })
}

/// Accounts per provider, with the plans their providers report.
#[derive(Clone, Debug, Default)]
pub struct AccountsReport {
    /// `claude`, `codex`, `cursor`, `gemini`, `copilot` -> how many.
    pub counts: BTreeMap<&'static str, usize>,
    pub claude_plans: Vec<&'static str>,
    pub codex_plans: Vec<&'static str>,
}

/// Build an anonymous `AccountsReport` from a discovered accounts snapshot plus
/// usage-only provider counts. Returns `None` if `discovered` has not yet been
/// populated (`fetched_at.is_none()`).
pub(crate) fn accounts_report_from_discovered(
    paths: &crate::paths::GxserverPaths,
    discovered: &crate::accounts::model::Snapshot,
) -> Option<AccountsReport> {
    discovered.fetched_at?;
    let mut report = AccountsReport::default();
    for account in &discovered.accounts {
        *report.counts.entry(account.provider.id()).or_default() += 1;
        let plans = match account.provider {
            crate::accounts::model::Provider::Claude => &mut report.claude_plans,
            crate::accounts::model::Provider::Codex => &mut report.codex_plans,
        };
        if let Some(plan) = account.plan {
            if !plans.contains(&plan) {
                plans.push(plan);
            }
        }
    }
    report.claude_plans.sort_unstable();
    report.codex_plans.sort_unstable();
    if let Some(counts) = crate::accounts::usage_only::provider_counts(paths) {
        report.counts.extend(counts);
    }
    Some(report)
}

/// The best plan of `plans` by `rank` (best first).
fn top_plan(plans: &[&'static str], rank: &'static [&'static str]) -> Option<&'static str> {
    rank.iter().copied().find(|plan| plans.contains(plan))
}

/// Boosts on this computer: whether they are on, and how many the user made or edited.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoostsReport {
    pub on: bool,
    pub custom: usize,
    pub edited: usize,
}

/// Everything the snapshot reports that is not read from the settings file.
#[derive(Clone, Debug, Default)]
pub struct SnapshotFacts {
    pub linux: Option<LinuxReport>,
    pub remote_machine_count: usize,
    /// `None` when the workspaces could not be read: left out, not reported as 0.
    pub workspace_count: Option<usize>,
    pub boosts: BoostsReport,
    /// `None` when neither a live account snapshot nor a cached anonymous
    /// `AccountsReport` in `analytics.state.v1` is available yet.
    pub accounts: Option<AccountsReport>,
    /// Installed official Store extensions with their version.
    pub store_extensions: Vec<(String, String)>,
    pub custom_extensions_installed: usize,
}

pub fn snapshot_properties(
    settings: Option<&Value>,
    facts: &SnapshotFacts,
) -> Vec<(&'static str, PropertyValue)> {
    let mut properties = settings_properties(settings);
    if let Some(linux) = &facts.linux {
        properties.push(("linux_distro", PropertyValue::Enum(linux.distro)));
        if let Some(version) = &linux.distro_version {
            properties.push((
                "linux_distro_version",
                PropertyValue::Version(version.clone()),
            ));
        }
        if let Some(desktop) = linux.desktop {
            properties.push(("linux_desktop", PropertyValue::Enum(desktop)));
        }
        if let Some(session) = linux.session {
            properties.push(("linux_session", PropertyValue::Enum(session)));
        }
        properties.push(("linux_wsl", PropertyValue::Bool(linux.wsl)));
    }
    properties.push((
        "remote_machine_count",
        PropertyValue::Number(facts.remote_machine_count as f64),
    ));
    if let Some(count) = facts.workspace_count {
        properties.push((
            "workspace_count",
            PropertyValue::Enum(taxonomy::count_bucket(count)),
        ));
    }
    properties.push(("boosts_on", PropertyValue::Bool(facts.boosts.on)));
    properties.push((
        "custom_boost_count",
        PropertyValue::Enum(taxonomy::count_bucket(facts.boosts.custom)),
    ));
    properties.push((
        "edited_boost_count",
        PropertyValue::Enum(taxonomy::count_bucket(facts.boosts.edited)),
    ));
    if let Some(accounts) = &facts.accounts {
        for (provider, key) in [
            ("claude", "claude_accounts"),
            ("codex", "codex_accounts"),
            ("cursor", "cursor_accounts"),
            ("gemini", "gemini_accounts"),
            ("copilot", "copilot_accounts"),
        ] {
            let count = accounts.counts.get(provider).copied().unwrap_or(0);
            properties.push((key, PropertyValue::Enum(taxonomy::count_bucket(count))));
        }
        properties.push((
            "claude_plans",
            PropertyValue::EnumList(accounts.claude_plans.clone()),
        ));
        properties.push((
            "codex_plans",
            PropertyValue::EnumList(accounts.codex_plans.clone()),
        ));
        if let Some(plan) = top_plan(&accounts.claude_plans, taxonomy::CLAUDE_PLAN_RANK) {
            properties.push(("claude_top_plan", PropertyValue::Enum(plan)));
        }
        if let Some(plan) = top_plan(&accounts.codex_plans, taxonomy::CODEX_PLAN_RANK) {
            properties.push(("codex_top_plan", PropertyValue::Enum(plan)));
        }
    }
    properties.push((
        "store_extensions_installed",
        PropertyValue::ExtensionVersionList(
            /*
            CDXC:Telemetry 2026-11-20 (privacy review of 591004b37): an entry that would fail
            the check (an id the Store does not list, a version such as `1.4.2+build5`) is left
            out here, so one odd extension can never make the validator drop the whole day's
            snapshot.
            */
            facts
                .store_extensions
                .iter()
                .filter(|(id, version)| {
                    super::extensions::is_official_store_id(id)
                        && taxonomy::normalize_version_string(version).is_some()
                })
                .take(super::extensions::MAX_STORE_EXTENSION_LIST_LENGTH)
                .map(|(id, version)| (ExtensionRef::Store(id.clone()), version.clone()))
                .collect(),
        ),
    ));
    properties.push((
        "custom_extensions_installed",
        PropertyValue::Enum(taxonomy::count_bucket(facts.custom_extensions_installed)),
    ));
    let modes = super::capture::resolve_install_session_modes(settings);
    properties.push(("antigravity_mode", PropertyValue::Enum(modes.antigravity)));
    properties.push(("claude_mode", PropertyValue::Enum(modes.claude)));
    properties.push(("codex_mode", PropertyValue::Enum(modes.codex)));
    let person: Vec<(&'static str, PropertyValue)> = properties
        .iter()
        .filter(|(key, _)| PERSON_KEYS.contains(key))
        .cloned()
        .collect();
    properties.push((taxonomy::PERSON_SET_KEY, PropertyValue::PersonSet(person)));
    properties
}

pub fn emit(settings: Option<&Value>, facts: &SnapshotFacts) {
    capture(
        taxonomy::EVENT_SETTINGS_SNAPSHOT,
        &snapshot_properties(settings, facts),
    );
}

/*
A Claude login's plan, from the `plan` object `cswap list --json` reports
(`organizationType` such as `claude_max` or `claude_team`, and `tier` such as
`default_claude_max_20x`). `None` when cswap has not read one.
*/
pub fn claude_plan(plan: &Value) -> Option<&'static str> {
    let organization = plan.get("organizationType").and_then(Value::as_str)?;
    let tier = plan
        .get("tier")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    Some(match organization.trim().to_ascii_lowercase().as_str() {
        "claude_max" if tier.contains("20x") => "max_20x",
        "claude_max" if tier.contains("5x") => "max_5x",
        "claude_max" => "max",
        "claude_pro" => "pro",
        "claude_team" => "team",
        "claude_enterprise" => "enterprise",
        "claude_free" | "free" => "free",
        _ => "other",
    })
}

/// A Codex login's plan, from `xswap list --json` `plan` (ChatGPT's own plan
/// name: `free`, `plus`, `pro`, `team`, ...). `None` when xswap reports none.
pub fn codex_plan(plan: &Value) -> Option<&'static str> {
    let plan = plan.as_str()?.trim().to_ascii_lowercase();
    if plan.is_empty() {
        return None;
    }
    Some(
        taxonomy::CODEX_PLANS
            .iter()
            .copied()
            .find(|known| *known == plan)
            .unwrap_or("other"),
    )
}
