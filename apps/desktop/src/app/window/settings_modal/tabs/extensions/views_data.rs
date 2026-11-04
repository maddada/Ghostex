//! The view data of the Extensions page: view scopes (ghostex-settings/view-scopes.ts), the view
//! order (ghostex-settings/titlebar-view-order.ts), and the custom views and templates
//! (ghostex-settings/custom-views.ts, project-views.ts).
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::store::SettingsValues;
use super::data::{
    FilterSubject, InstalledExtension, OfficialExtension, SourceFilter, official_extensions,
    preference_string, string_list, text,
};
use serde_json::{Map, Value, json};

// ---- view scopes ---------------------------------------------------------------------------

/// `GhostexViewScope`: a Default of shown (`true`) or hidden plus per-project and per-space
/// overrides, in the order they were written.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ViewScope {
    pub(crate) default_shown: bool,
    pub(crate) projects: Vec<(String, bool)>,
    pub(crate) spaces: Vec<(String, bool)>,
}

impl Default for ViewScope {
    /// `DEFAULT_GHOSTEX_VIEW_SCOPE`.
    fn default() -> Self {
        Self {
            default_shown: true,
            projects: Vec::new(),
            spaces: Vec::new(),
        }
    }
}

fn state_value(shown: bool) -> Value {
    json!(if shown { "shown" } else { "hidden" })
}

fn scope_state(value: &Value) -> Option<bool> {
    match value.as_str() {
        Some("shown") => Some(true),
        Some("hidden") => Some(false),
        _ => None,
    }
}

const UNSAFE_KEYS: [&str; 3] = ["__proto__", "constructor", "prototype"];

fn normalize_overrides(value: Option<&Value>) -> Vec<(String, bool)> {
    let Some(object) = value.and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut overrides: Vec<(String, bool)> = Vec::new();
    for (raw_key, raw_state) in object {
        let key: String = raw_key.trim().chars().take(512).collect();
        let Some(state) = scope_state(raw_state) else {
            continue;
        };
        if key.is_empty() || UNSAFE_KEYS.contains(&key.as_str()) {
            continue;
        }
        if let Some(existing) = overrides.iter_mut().find(|(existing, _)| *existing == key) {
            existing.1 = state;
        } else {
            overrides.push((key, state));
        }
    }
    overrides
}

impl ViewScope {
    /// `normalizeGhostexViewScope` (with the allow-list migration).
    pub(crate) fn from_value(value: &Value) -> Self {
        let Some(object) = value.as_object() else {
            return Self::default();
        };
        if scope_state(object.get("default").unwrap_or(&Value::Null)).is_none()
            && object.get("availability").is_some_and(Value::is_string)
        {
            let text_of = |value: &Value| -> String {
                value
                    .as_str()
                    .map(|text| text.trim().chars().take(512).collect())
                    .unwrap_or_default()
            };
            return match object["availability"].as_str() {
                Some("selected") => Self {
                    default_shown: false,
                    projects: object
                        .get("projectIds")
                        .and_then(Value::as_array)
                        .map(|ids| {
                            ids.iter()
                                .map(text_of)
                                .filter(|id| !id.is_empty() && !UNSAFE_KEYS.contains(&id.as_str()))
                                .map(|id| (id, true))
                                .collect()
                        })
                        .unwrap_or_default(),
                    spaces: Vec::new(),
                },
                Some("spaces") => Self {
                    default_shown: false,
                    projects: Vec::new(),
                    spaces: object
                        .get("spaceRefs")
                        .and_then(Value::as_array)
                        .map(|refs| {
                            refs.iter()
                                .filter_map(|entry| {
                                    let section = text_of(entry.get("sectionKey")?);
                                    let space = text_of(entry.get("spaceId")?);
                                    (!section.is_empty() && !space.is_empty())
                                        .then(|| (format!("{section}:{space}"), true))
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                },
                _ => Self::default(),
            };
        }
        Self {
            default_shown: scope_state(object.get("default").unwrap_or(&Value::Null))
                .unwrap_or(true),
            projects: normalize_overrides(object.get("projects")),
            spaces: normalize_overrides(object.get("spaces")),
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        let overrides = |entries: &[(String, bool)]| {
            Value::Object(
                entries
                    .iter()
                    .map(|(key, shown)| (key.clone(), state_value(*shown)))
                    .collect(),
            )
        };
        json!({
            "default": state_value(self.default_shown),
            "projects": overrides(&self.projects),
            "spaces": overrides(&self.spaces),
        })
    }

    /// `isDefaultGhostexViewScope`.
    pub(crate) fn is_default(&self) -> bool {
        self.default_shown && self.projects.is_empty() && self.spaces.is_empty()
    }

    /// `prunedGhostexViewScope`.
    pub(crate) fn pruned(&self) -> Self {
        if self
            .spaces
            .iter()
            .any(|(_, shown)| *shown != self.default_shown)
        {
            return self.clone();
        }
        Self {
            default_shown: self.default_shown,
            projects: self
                .projects
                .iter()
                .filter(|(_, shown)| *shown != self.default_shown)
                .cloned()
                .collect(),
            spaces: Vec::new(),
        }
    }
}

/// `ghostexViewScope(scopes, key)`.
pub(crate) fn view_scope(scopes: &Value, key: &str) -> ViewScope {
    match scopes.get(key) {
        Some(value) => ViewScope::from_value(value),
        None => ViewScope::default(),
    }
}

/// `normalizeGhostexViewScopes`.
fn normalize_view_scopes(value: &Map<String, Value>) -> Map<String, Value> {
    let mut scopes = Map::new();
    for (key, entry) in value {
        if key.is_empty() || key.chars().count() > 256 || UNSAFE_KEYS.contains(&key.as_str()) {
            continue;
        }
        let scope = ViewScope::from_value(entry);
        if scope.is_default() {
            continue;
        }
        scopes.insert(key.clone(), scope.to_value());
    }
    scopes
}

/// `setGhostexViewScope(scopes, key, scope)`.
pub(crate) fn set_view_scope(scopes: &Value, key: &str, scope: &ViewScope) -> Value {
    let mut next = scopes.as_object().cloned().unwrap_or_default();
    next.insert(key.to_string(), scope.pruned().to_value());
    Value::Object(normalize_view_scopes(&next))
}

/// `officialViewScopeKey`.
pub(crate) fn official_view_scope_key(id: &str) -> String {
    format!("official:{id}")
}

/// `extensionViewScopeKey`.
pub(crate) fn extension_view_scope_key(id: &str) -> String {
    format!("extension:{id}")
}

/// `ProjectViewProject`: a project the sidebar shows.
#[derive(Clone, Debug)]
pub(crate) struct ScopeProject {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) project_id: String,
}

/// `ProjectViewSpace`.
#[derive(Clone, Debug)]
pub(crate) struct ScopeSpace {
    pub(crate) section_key: String,
    pub(crate) space_id: String,
    pub(crate) name: String,
}

impl ScopeSpace {
    /// `viewScopeSpaceKey`.
    pub(crate) fn key(&self) -> String {
        format!("{}:{}", self.section_key, self.space_id)
    }
}

/// `hud.projectViewProjects` and `hud.projectViewSpaces` of the hydrate.
pub(crate) fn scope_projects_and_spaces(
    hud: Option<&Value>,
) -> (Vec<ScopeProject>, Vec<ScopeSpace>) {
    let projects = hud
        .and_then(|hud| hud.get("projectViewProjects"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| ScopeProject {
                    name: text(item, "name"),
                    path: text(item, "path"),
                    project_id: text(item, "projectId"),
                })
                .collect()
        })
        .unwrap_or_default();
    let spaces = hud
        .and_then(|hud| hud.get("projectViewSpaces"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| ScopeSpace {
                    section_key: text(item, "sectionKey"),
                    space_id: text(item, "spaceId"),
                    name: text(item, "name"),
                })
                .collect()
        })
        .unwrap_or_default();
    (projects, spaces)
}

/// `parseViewScopeSpaceKey`: whether a stored key names a space at all.
pub(crate) fn parse_view_scope_space_key(key: &str) -> Option<(String, String)> {
    let separator = key.rfind(':')?;
    if separator == 0 || separator == key.len() - 1 {
        return None;
    }
    Some((
        key[..separator].to_string(),
        key[separator + 1..].to_string(),
    ))
}

/// `viewScopeDescription`: the one-line summary on a narrowed card.
pub(crate) fn view_scope_description(
    scope: &ViewScope,
    projects: &[ScopeProject],
    spaces: &[ScopeSpace],
) -> String {
    let mut shown: Vec<String> = Vec::new();
    let mut hidden: Vec<String> = Vec::new();
    for (project_id, state) in &scope.projects {
        let name = projects
            .iter()
            .find(|project| project.project_id == *project_id)
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "Unavailable project".to_string());
        if *state { &mut shown } else { &mut hidden }.push(name);
    }
    for (space_key, state) in &scope.spaces {
        let name = spaces
            .iter()
            .find(|space| space.key() == *space_key)
            .map(|space| space.name.clone())
            .unwrap_or_else(|| "Unavailable space".to_string());
        if *state { &mut shown } else { &mut hidden }.push(name);
    }
    if scope.default_shown {
        return if hidden.is_empty() {
            "All projects".to_string()
        } else {
            format!("Everywhere except {}", hidden.join(", "))
        };
    }
    if shown.is_empty() {
        return "Hidden everywhere".to_string();
    }
    if hidden.is_empty() {
        format!("Only {}", shown.join(", "))
    } else {
        format!("Only {}, except {}", shown.join(", "), hidden.join(", "))
    }
}

// ---- view order ----------------------------------------------------------------------------

/// `TitlebarViewOrderItem`.
#[derive(Clone, Debug)]
pub(crate) struct ViewOrderItem {
    pub(crate) id: String,
    pub(crate) title: String,
    /// `Built-in`, `Extension` or `Custom view`.
    pub(crate) source: &'static str,
    pub(crate) visible: bool,
}

/// `titlebarViewOrderItems(settings, installed)`.
///
/// CDXC:Titlebar 2026-09-09 DECISION:
/// User: one Settings popup controls the order of built-in, extension, and custom views together.
/// SEE-ALSO: apps/desktop/src/app/workarea.rs applies these mode slugs to the native titlebar list before numbered shortcuts resolve it.
pub(crate) fn view_order_items(
    values: &SettingsValues,
    installed: &[InstalledExtension],
) -> Vec<ViewOrderItem> {
    let item = |id: &str, title: &str, visible: bool| ViewOrderItem {
        id: id.to_string(),
        title: title.to_string(),
        source: "Built-in",
        visible,
    };
    let mut items = vec![
        item("agents", "Agents", true),
        item("source", "VS Code", !values.bool("codeViewTabHidden")),
        item("browser", "Browser", !values.bool("browserViewTabHidden")),
        item("kanban", "Kanban", !values.bool("kanbanViewTabHidden")),
        item(
            "automate",
            "Automate",
            !values.bool("automateViewTabHidden"),
        ),
        item("manage", "Files", !values.bool("docsViewTabHidden")),
        item("changes", "Changes", !values.bool("changesViewTabHidden")),
        item(
            "terminal",
            "Terminal",
            !values.bool("terminalViewTabHidden"),
        ),
        item(
            "extension:storybook",
            "Storybook",
            !values.bool("storybookViewTabHidden"),
        ),
    ];
    let website_ids: Vec<&OfficialExtension> = official_extensions()
        .iter()
        .filter(|extension| extension.category == "project-websites")
        .collect();
    for provider in &website_ids {
        items.push(ViewOrderItem {
            id: format!("extension:{}", provider.id),
            title: provider.title.clone(),
            source: "Built-in",
            visible: !values.bool(&provider.settings_key),
        });
    }
    let custom_views = custom_views(values);
    let mut extension_items: Vec<ViewOrderItem> = installed
        .iter()
        .filter(|extension| {
            let id = extension.id();
            id != "storybook"
                && !website_ids.iter().any(|provider| provider.id == id)
                && extension
                    .placements()
                    .iter()
                    .any(|placement| placement == "view")
                && extension.state_placement().as_deref() == Some("view")
                && !custom_views.iter().any(|view| view.id() == id)
        })
        .map(|extension| ViewOrderItem {
            id: format!("extension:{}", extension.id()),
            title: extension.title(),
            source: "Extension",
            visible: extension.enabled(),
        })
        .collect();
    extension_items.sort_by(|left, right| left.title.cmp(&right.title));
    items.extend(extension_items);
    items.extend(custom_views.iter().map(|view| ViewOrderItem {
        id: format!("extension:{}", view.id()),
        title: view.name(),
        source: "Custom view",
        visible: view.enabled(),
    }));
    let order = titlebar_view_order(values);
    let order: Vec<&str> = order.iter().map(String::as_str).collect();
    let position = |id: &str| titlebar_view_order_rank(&order, id);
    // `sort` with `Infinity` for unknown ids is stable: unplaced items keep their order at the end.
    items.sort_by(
        |left, right| match (position(&left.id), position(&right.id)) {
            (Some(left), Some(right)) => left.cmp(&right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        },
    );
    items
}

/// Where a view with `slug` sorts in a saved `titlebarViewOrder`: its own place, or for the Changes
/// view in an order saved before it existed, right after Files. `None` sorts last.
///
/// CDXC:Changes 2026-10-25 DECISION: User wants the Changes view on by default and, in the default order of views, right after Files.
pub(crate) fn titlebar_view_order_rank(order: &[&str], slug: &str) -> Option<usize> {
    if let Some(index) = order.iter().position(|id| *id == slug) {
        return Some(index * 2);
    }
    (slug == "changes")
        .then(|| order.iter().position(|id| *id == "manage"))
        .flatten()
        .map(|index| index * 2 + 1)
}

/// `settings.titlebarViewOrder`.
pub(crate) fn titlebar_view_order(values: &SettingsValues) -> Vec<String> {
    string_list(Some(&values.value("titlebarViewOrder")))
}

/// `moveId(ids, initialIndex, index)`.
pub(crate) fn move_id(ids: &[String], from: usize, to: usize) -> Vec<String> {
    let mut next = ids.to_vec();
    if from >= next.len() {
        return next;
    }
    let id = next.remove(from);
    next.insert(to.min(next.len()), id);
    next
}

/// The saved order after a move: the moved ids, then the stored ids they did not include.
pub(crate) fn merged_view_order(ids: Vec<String>, stored: &[String]) -> Value {
    let mut order = ids;
    let known: Vec<String> = order.clone();
    order.extend(stored.iter().filter(|id| !known.contains(id)).cloned());
    json!(order)
}

// ---- custom views and templates --------------------------------------------------------------

/// `CUSTOM_VIEW_ID_PREFIX`.
pub(crate) const CUSTOM_VIEW_ID_PREFIX: &str = "custom-view-";

/// A `GhostexCustomView` or `ProjectViewTemplate` as the settings store it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CustomView {
    pub(crate) raw: Map<String, Value>,
}

impl CustomView {
    pub(crate) fn id(&self) -> String {
        self.raw
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    pub(crate) fn name(&self) -> String {
        self.raw
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    pub(crate) fn url(&self) -> String {
        self.raw
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    pub(crate) fn enabled(&self) -> bool {
        self.raw.get("enabled").and_then(Value::as_bool) != Some(false)
    }

    pub(crate) fn source(&self) -> Option<&Map<String, Value>> {
        self.raw.get("source").and_then(Value::as_object)
    }

    /// `view.source ?? DEFAULT_PROJECT_VIEW_SOURCE`.
    pub(crate) fn source_or_default(&self) -> Map<String, Value> {
        self.source()
            .cloned()
            .unwrap_or_else(default_project_view_source)
    }

    pub(crate) fn source_text(&self, key: &str) -> String {
        self.source_or_default()
            .get(key)
            .map(|value| preference_string(Some(value)))
            .unwrap_or_default()
    }

    pub(crate) fn availability(&self) -> String {
        self.raw
            .get("availability")
            .and_then(Value::as_str)
            .unwrap_or("all")
            .to_string()
    }

    pub(crate) fn space_refs(&self) -> Vec<(String, String)> {
        self.raw
            .get("spaceRefs")
            .and_then(Value::as_array)
            .map(|refs| {
                refs.iter()
                    .map(|entry| (text(entry, "sectionKey"), text(entry, "spaceId")))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `customViewKindLabel`.
    pub(crate) fn kind_label(&self) -> &'static str {
        match self
            .source()
            .and_then(|source| source.get("kind"))
            .and_then(Value::as_str)
        {
            Some("report") => "HTML report",
            Some("dev-server") => "Dev server",
            _ => "Website",
        }
    }

    /// `projectViewDescription`.
    pub(crate) fn description(&self) -> String {
        let Some(source) = self.source() else {
            return self.url();
        };
        let field = |key: &str| {
            source
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match field("kind").as_str() {
            "report" => {
                let directory = field("reportDirectory");
                format!(
                    "HTML report · {}",
                    if directory.is_empty() {
                        "Choose an output directory".to_string()
                    } else {
                        directory
                    }
                )
            }
            "dev-server" => {
                if field("discovery") == "storybook" {
                    "Dev server · Detect project Storybook script".to_string()
                } else {
                    let command = field("command");
                    format!(
                        "Dev server · {}",
                        if command.is_empty() {
                            "Set a project command".to_string()
                        } else {
                            command
                        }
                    )
                }
            }
            _ => {
                let destination = field("destination");
                if destination == "project" {
                    "Website · URL supplied by each project".to_string()
                } else if let Some(rest) = destination.strip_prefix("github-") {
                    format!("Website · Current repository {rest}")
                } else {
                    let url = self.url();
                    if url.is_empty() {
                        "Website · Set a URL".to_string()
                    } else {
                        url
                    }
                }
            }
        }
    }

    /// `customViewFilterSubject`.
    pub(crate) fn filter_subject(&self) -> FilterSubject {
        FilterSubject {
            categories: Vec::new(),
            search_text: vec![self.description(), self.url()],
            source: SourceFilter::Custom,
            title: self.name(),
            types: vec!["view".to_string()],
        }
    }
}

/// `settings.customViews`.
pub(crate) fn custom_views(values: &SettingsValues) -> Vec<CustomView> {
    values
        .value("customViews")
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_object)
                .map(|raw| CustomView { raw: raw.clone() })
                .collect()
        })
        .unwrap_or_default()
}

/// `settings.customViewTemplates`.
pub(crate) fn custom_view_templates(values: &SettingsValues) -> Vec<CustomView> {
    values
        .value("customViewTemplates")
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_object)
                .map(|raw| CustomView { raw: raw.clone() })
                .collect()
        })
        .unwrap_or_default()
}

/// `DEFAULT_PROJECT_VIEW_SOURCE`.
pub(crate) fn default_project_view_source() -> Map<String, Value> {
    settings_catalog()
        .module_value(module::PROJECT_VIEWS, "DEFAULT_PROJECT_VIEW_SOURCE")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// `BUILTIN_PROJECT_VIEW_TEMPLATES`.
pub(crate) fn builtin_templates() -> Vec<CustomView> {
    settings_catalog()
        .module_value(module::PROJECT_VIEWS, "BUILTIN_PROJECT_VIEW_TEMPLATES")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_object)
                .map(|raw| CustomView { raw: raw.clone() })
                .collect()
        })
        .unwrap_or_default()
}

/// `normalizeCustomViewUrl`: a complete HTTP or HTTPS URL, as `URL.toString()` writes it.
pub(crate) fn normalize_custom_view_url(candidate: &str) -> Option<String> {
    let value = candidate.trim();
    let url = url::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return None;
    }
    Some(url.to_string())
}

fn clamp_text(value: Option<&Value>, max: usize) -> String {
    value
        .and_then(Value::as_str)
        .map(|text| text.trim().chars().take(max).collect())
        .unwrap_or_default()
}

/// `normalizeProjectViewOptions(value)`.
fn normalize_project_view_options(value: &Map<String, Value>) -> Map<String, Value> {
    let mut options = Map::new();
    let Some(raw) = value.get("source").and_then(Value::as_object) else {
        return options;
    };
    let kind = raw.get("kind").and_then(Value::as_str).unwrap_or("website");
    let destination = raw
        .get("destination")
        .and_then(Value::as_str)
        .unwrap_or("fixed");
    let discovery = if raw.get("discovery").and_then(Value::as_str) == Some("storybook") {
        "storybook"
    } else {
        "command"
    };
    let cwd = clamp_text(raw.get("cwd"), 8192);
    let entry = clamp_text(raw.get("entry"), 8192);
    let timeout = raw
        .get("timeoutSeconds")
        .and_then(Value::as_f64)
        .filter(|seconds| seconds.is_finite())
        .map(|seconds| seconds.round().clamp(5.0, 600.0) as i64)
        .unwrap_or(60);
    options.insert(
        "source".into(),
        json!({
            "kind": kind,
            "destination": destination,
            "discovery": discovery,
            "command": clamp_text(raw.get("command"), 8192),
            "cwd": if cwd.is_empty() { ".".to_string() } else { cwd },
            "readinessUrl": clamp_text(raw.get("readinessUrl"), 8192),
            "reportDirectory": clamp_text(raw.get("reportDirectory"), 8192),
            "entry": if entry.is_empty() { "index.html".to_string() } else { entry },
            "timeoutSeconds": timeout,
        }),
    );
    let availability = value
        .get("availability")
        .and_then(Value::as_str)
        .unwrap_or("matching");
    options.insert("availability".into(), json!(availability));
    let mut project_ids: Vec<String> = Vec::new();
    for id in value
        .get("projectIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !project_ids.iter().any(|existing| existing == id) {
            project_ids.push(id.to_string());
        }
    }
    options.insert("projectIds".into(), json!(project_ids));
    let space_refs: Vec<Value> = value
        .get("spaceRefs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let section = clamp_text(entry.get("sectionKey"), 256);
            let space = clamp_text(entry.get("spaceId"), 256);
            (!section.is_empty() && !space.is_empty())
                .then(|| json!({ "sectionKey": section, "spaceId": space }))
        })
        .collect();
    options.insert("spaceRefs".into(), Value::Array(space_refs));
    let mut bindings = Map::new();
    if let Some(raw_bindings) = value.get("projectBindings").and_then(Value::as_object) {
        for (id, binding) in raw_bindings {
            let Some(binding) = binding.as_object() else {
                continue;
            };
            if UNSAFE_KEYS.contains(&id.as_str()) {
                continue;
            }
            let mut next = Map::new();
            for key in ["url", "repositoryUrl", "command", "cwd", "readinessUrl"] {
                next.insert(key.into(), json!(clamp_text(binding.get(key), 8192)));
            }
            if let Some(start) = binding.get("startOnProjectOpen").and_then(Value::as_bool) {
                next.insert("startOnProjectOpen".into(), json!(start));
            }
            next.insert(
                "inherit".into(),
                json!(binding.get("inherit").and_then(Value::as_bool) != Some(false)),
            );
            bindings.insert(id.clone(), Value::Object(next));
        }
    }
    options.insert("projectBindings".into(), Value::Object(bindings));
    let template_id = clamp_text(value.get("templateId"), 128);
    if !template_id.is_empty() {
        options.insert("templateId".into(), json!(template_id));
    }
    options
}

/// `normalizeGhostexCustomViews(candidate)`.
///
/// CDXC:Extensions 2026-09-03 DECISION: Users can add any number of custom titlebar views, arrange them in their preferred order, and turn individual views off without deleting their name and HTTP or HTTPS URL.
/// CDXC:Extensions 2026-09-03 SEE-ALSO: The editor, native titlebar projection, and isolated CEF workarea must keep this ordered, enabled, name-and-URL contract aligned. See apps/desktop/src/app/helpers/titlebar/mode_switcher.rs and apps/desktop/src/app/workarea.rs.
pub(crate) fn normalize_custom_views(views: &[CustomView]) -> Value {
    let mut ids: Vec<String> = Vec::new();
    let mut normalized: Vec<Value> = Vec::new();
    for view in views {
        let entry = &view.raw;
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        let options = normalize_project_view_options(entry);
        let url = entry
            .get("url")
            .and_then(Value::as_str)
            .and_then(normalize_custom_view_url)
            .unwrap_or_default();
        let source = options.get("source");
        let needs_url = source.is_none_or(|source| {
            source["kind"].as_str() == Some("website")
                && source["destination"].as_str() == Some("fixed")
        });
        let valid_id = id.starts_with(CUSTOM_VIEW_ID_PREFIX)
            && id.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            });
        if !valid_id || ids.contains(&id) || name.is_empty() || (needs_url && url.is_empty()) {
            continue;
        }
        ids.push(id.clone());
        let mut next = options;
        next.insert(
            "enabled".into(),
            json!(entry.get("enabled").and_then(Value::as_bool) != Some(false)),
        );
        next.insert("id".into(), json!(id));
        next.insert("name".into(), json!(name));
        next.insert("url".into(), json!(url));
        normalized.push(Value::Object(next));
    }
    Value::Array(normalized)
}

/// A fresh lower-case uuid (`crypto.randomUUID()`).
pub(crate) fn random_uuid() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for chunk in bytes.chunks_mut(8) {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        chunk.copy_from_slice(&hasher.finish().to_le_bytes()[..chunk.len()]);
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// `extensionStaticAssetUrl(id, path)` as the gxserver path the host resolves.
pub(crate) fn extension_static_asset_path(id: &str, path: &str) -> String {
    let encoded: Vec<String> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(encode_uri_component)
        .collect();
    format!("/ext/{}/{}", encode_uri_component(id), encoded.join("/"))
}

/// `encodeURIComponent`.
pub(crate) fn encode_uri_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        let character = byte as char;
        if character.is_ascii_alphanumeric() || "-_.!~*'()".contains(character) {
            encoded.push(character);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// `new URL(path, catalogUrl).toString()`.
pub(crate) fn catalog_asset_url(catalog_url: &str, path: &str) -> Option<String> {
    let base = url::Url::parse(catalog_url).ok()?;
    base.join(path).ok().map(|url| url.to_string())
}
