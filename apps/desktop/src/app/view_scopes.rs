use crate::*;
use serde_json::Value;
use std::collections::HashMap;

/*
CDXC:Extensions 2026-09-20 DECISION:
User (ruling 3A): the per-view scope is a set of OVERRIDES, not an allow-list. Each view carries a
`default` of shown or hidden plus per-project and per-space overrides, resolved project, then space,
then default, so "hide this view here" is one override instead of a list of every other project.
This supersedes the 2026-09-18 "Available in" picker ('all' | 'selected' | 'spaces'), whose stored
shape is still read here and converted, because a settings file written before the rewrite is exactly
expressible in the new model.

The scope is a plain settings map (`viewScopes`), so it resolves synchronously while the work area
header renders. A view with NO entry is shown everywhere, which is why an absent key returns true
rather than falling back to a default entry: the setting only ever stores the views the user narrowed.

Space membership itself is owned by the daemon's collections and spaces documents, which this process
cannot read; the sidebar HUD carries the active project's resolved spaces instead.
SEE-ALSO: packages/shared/ghostex-settings/view-scopes.ts owns the same rule, the same precedence and
the same allow-list migration for React and the settings schema, and
packages/gx-core/src/hud/scopes.rs resolves the HUD field.
*/

pub(crate) fn official_view_scope_key(official_extension_id: &str) -> String {
    format!("official:{official_extension_id}")
}

pub(crate) fn extension_view_scope_key(extension_id: &str) -> String {
    format!("extension:{extension_id}")
}

/// The official descriptor id behind a built-in workarea tab, matching `GHOSTEX_OFFICIAL_EXTENSIONS`.
/// The titlebar's own slugs (`source`, `manage`) are deliberately not used as scope keys.
pub(crate) fn titlebar_mode_official_extension_id(mode: TitlebarMode) -> Option<&'static str> {
    match mode {
        mode if mode.website_provider().is_some() => {
            mode.website_provider().map(|provider| provider.id.as_str())
        }
        mode if mode.is_storybook() => Some("storybook"),
        TitlebarMode::Source => Some("code"),
        TitlebarMode::Browser => Some("browser"),
        TitlebarMode::Kanban => Some("kanban"),
        TitlebarMode::Automate => Some("automate"),
        TitlebarMode::Manage => Some("docs"),
        TitlebarMode::Terminal => Some("terminal"),
        // CDXC:Workarea 2026-09-20 DECISION:
        // User: the Ghostex pages are app-wide, so they are available in every project regardless of
        // scope. No scope key means nothing to hide them with, which is exactly that rule.
        TitlebarMode::Agents | TitlebarMode::Extension(_) => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewScopeState {
    Shown,
    Hidden,
}

fn view_scope_state(value: Option<&Value>) -> Option<ViewScopeState> {
    match value.and_then(Value::as_str) {
        Some("shown") => Some(ViewScopeState::Shown),
        Some("hidden") => Some(ViewScopeState::Hidden),
        _ => None,
    }
}

/// A space override is keyed by section and space together, because each gxserver section mints its
/// space ids independently.
pub(crate) fn view_scope_space_key(section_key: &str, space_id: &str) -> String {
    format!("{section_key}:{space_id}")
}

/// The section and space a `sectionKey:spaceId` override key names. A remote section key contains a
/// colon of its own, so the space id is everything after the LAST one.
pub(crate) fn parse_view_scope_space_key(key: &str) -> Option<(&str, &str)> {
    key.rsplit_once(':')
}

/// Which of a view scope's two override maps a menu row writes into.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewScopeOverrideTarget {
    Project,
    Space,
}

struct ViewScope {
    default_state: ViewScopeState,
    projects: HashMap<String, ViewScopeState>,
    spaces: HashMap<String, ViewScopeState>,
}

fn view_scope_overrides(value: Option<&Value>) -> HashMap<String, ViewScopeState> {
    value
        .and_then(Value::as_object)
        .map(|overrides| {
            overrides
                .iter()
                .filter_map(|(key, state)| Some((key.clone(), view_scope_state(Some(state))?)))
                .collect()
        })
        .unwrap_or_default()
}

impl ViewScope {
    fn from_json(value: &Value) -> Self {
        if view_scope_state(value.get("default")).is_none()
            && let Some(availability) = value.get("availability").and_then(Value::as_str)
        {
            return Self::from_allow_list(value, availability);
        }
        Self {
            default_state: view_scope_state(value.get("default")).unwrap_or(ViewScopeState::Shown),
            projects: view_scope_overrides(value.get("projects")),
            spaces: view_scope_overrides(value.get("spaces")),
        }
    }

    /// The pre-2026-09-20 allow-list, read in place: `selected` is "hidden except these projects",
    /// `spaces` is "hidden except these spaces", and `all` carries nothing at all.
    fn from_allow_list(value: &Value, availability: &str) -> Self {
        let shown = |key: &str, id: fn(&Value) -> Option<String>| {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| Some((id(entry)?, ViewScopeState::Shown)))
                        .collect()
                })
                .unwrap_or_default()
        };
        match availability {
            "selected" => Self {
                default_state: ViewScopeState::Hidden,
                projects: shown("projectIds", |entry| entry.as_str().map(ToOwned::to_owned)),
                spaces: HashMap::new(),
            },
            "spaces" => Self {
                default_state: ViewScopeState::Hidden,
                projects: HashMap::new(),
                spaces: shown("spaceRefs", |entry| {
                    Some(view_scope_space_key(
                        entry.get("sectionKey").and_then(Value::as_str)?,
                        entry.get("spaceId").and_then(Value::as_str)?,
                    ))
                }),
            },
            _ => Self {
                default_state: ViewScopeState::Shown,
                projects: HashMap::new(),
                spaces: HashMap::new(),
            },
        }
    }

    /// Project, then space, then default: the most specific rule wins, and where two of the project's
    /// spaces disagree, hidden wins.
    fn resolve(&self, project_id: Option<&str>, space_keys: &[String]) -> ViewScopeState {
        if let Some(state) = project_id.and_then(|id| self.projects.get(id)) {
            return *state;
        }
        let mut shown = false;
        for key in space_keys {
            match self.spaces.get(key) {
                Some(ViewScopeState::Hidden) => return ViewScopeState::Hidden,
                Some(ViewScopeState::Shown) => shown = true,
                None => {}
            }
        }
        if shown {
            ViewScopeState::Shown
        } else {
            self.default_state
        }
    }
}

impl ViewScopeState {
    fn slug(self) -> &'static str {
        match self {
            Self::Shown => "shown",
            Self::Hidden => "hidden",
        }
    }
}

impl ViewScope {
    fn to_json(&self) -> Value {
        let overrides = |map: &HashMap<String, ViewScopeState>| {
            Value::Object(
                map.iter()
                    .map(|(key, state)| (key.clone(), Value::String(state.slug().to_string())))
                    .collect(),
            )
        };
        serde_json::json!({
            "default": self.default_state.slug(),
            "projects": overrides(&self.projects),
            "spaces": overrides(&self.spaces),
        })
    }

    /// The same minimisation the TypeScript writer applies: overrides that provably cannot change
    /// anything go, so ticking a menu row off and on again leaves no residue. The moment one space
    /// override differs from the default they are all kept, because a project's `hidden` is then the
    /// only thing that can beat its space's `shown`.
    fn pruned(mut self) -> Self {
        if self
            .spaces
            .values()
            .any(|state| *state != self.default_state)
        {
            return self;
        }
        self.spaces.clear();
        self.projects
            .retain(|_, state| *state != self.default_state);
        self
    }

    fn says_nothing(&self) -> bool {
        self.default_state == ViewScopeState::Shown
            && self.projects.is_empty()
            && self.spaces.is_empty()
    }
}

impl GhostexGpuiApp {
    /// The spaces the ACTIVE project resolves into, as `sectionKey:spaceId` override keys, with group
    /// and worktree-parent inheritance already applied by the store's HUD (gx-core
    /// `hud/scopes.rs`, `active_project_space_refs`). A project only the built-in Other space holds resolves into no space at all.
    pub(crate) fn active_project_space_keys(&self) -> Vec<String> {
        self.native_sidebar
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.hud.get("activeProjectSpaceRefs"))
            .and_then(Value::as_array)
            .map(|refs| {
                refs.iter()
                    .filter_map(|reference| {
                        Some(view_scope_space_key(
                            reference.get("sectionKey").and_then(Value::as_str)?,
                            reference.get("spaceId").and_then(Value::as_str)?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// False when the user hid this view in the active project, in one of its spaces, or everywhere.
    pub(crate) fn view_scope_allows(&self, key: &str) -> bool {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let Some(scope) = settings
            .object()
            .get("viewScopes")
            .and_then(|map| map.get(key))
        else {
            return true;
        };
        let project_id = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .map(|id| id.0.as_str());
        ViewScope::from_json(scope).resolve(project_id, &self.active_project_space_keys())
            == ViewScopeState::Shown
    }

    /// The scope key for a view tab, which is what a right-click menu writes an override under.
    /// Custom views keep their own availability rule, so they have no scope key.
    pub(crate) fn titlebar_mode_view_scope_key(&self, mode: TitlebarMode) -> Option<String> {
        match mode {
            mode if mode.website_provider().is_some() => mode
                .website_provider()
                .map(|provider| official_view_scope_key(&provider.id)),
            mode if mode.is_storybook() => Some(official_view_scope_key("storybook")),
            TitlebarMode::Extension(id) => {
                (gpui_custom_view(id).is_none()).then(|| extension_view_scope_key(id.as_str()))
            }
            _ => titlebar_mode_official_extension_id(mode).map(official_view_scope_key),
        }
    }

    /// CDXC:Extensions 2026-09-20 SEE-ALSO:
    /// One override, written the way `setViewScopeOverride` writes it in
    /// packages/shared/ghostex-settings/view-scopes.ts: same map shape, same `inherit` meaning, same
    /// pruning, same "a scope that says nothing is not stored at all" normalisation. Rust owns a
    /// writer of its own because the view tab's right-click menu is native, and it already owns the
    /// reader beside it for the same reason; the two implementations must stay word for word.
    pub(crate) fn set_view_scope_override(
        &mut self,
        key: &str,
        target: ViewScopeOverrideTarget,
        override_key: &str,
        state: Option<ViewScopeState>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut settings = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        let scopes = settings
            .entry("viewScopes".to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if !scopes.is_object() {
            *scopes = Value::Object(serde_json::Map::new());
        }
        let scopes = scopes
            .as_object_mut()
            .expect("view scopes map must be an object");
        let mut scope = scopes
            .get(key)
            .map(ViewScope::from_json)
            .unwrap_or_else(|| ViewScope {
                default_state: ViewScopeState::Shown,
                projects: HashMap::new(),
                spaces: HashMap::new(),
            });
        let overrides = match target {
            ViewScopeOverrideTarget::Project => &mut scope.projects,
            ViewScopeOverrideTarget::Space => &mut scope.spaces,
        };
        match state {
            Some(state) => {
                overrides.insert(override_key.to_string(), state);
            }
            None => {
                overrides.remove(override_key);
            }
        }
        let scope = scope.pruned();
        if scope.says_nothing() {
            scopes.remove(key);
        } else {
            scopes.insert(key.to_string(), scope.to_json());
        }
        let _ = shared_settings::write_shared_sidebar_settings_object(settings);
        self.refresh_gpui_plugins_modal(cx);
        cx.notify();
    }

    /// The effective state of a view for the active project, which is what a checkable menu row
    /// shows and what unticking it has to reverse.
    pub(crate) fn view_scope_state_for_active_project(&self, key: &str) -> ViewScopeState {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let Some(scope) = settings
            .object()
            .get("viewScopes")
            .and_then(|map| map.get(key))
        else {
            return ViewScopeState::Shown;
        };
        ViewScope::from_json(scope).resolve(
            self.active_project_id_for_view_scope().as_deref(),
            &self.active_project_space_keys(),
        )
    }

    pub(crate) fn active_project_id_for_view_scope(&self) -> Option<String> {
        self.latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .map(|id| id.0.clone())
    }

    /// The active project's spaces with the names the sidebar shows, so the tab menu can say "Show in
    /// space ShortPoint". The keys come from `activeProjectSpaceRefs`, which carries membership but
    /// no name; the names come from `projectViewSpaces`, the same HUD list the Settings scope editor
    /// labels its cards from, so the two surfaces cannot disagree about what a space is called.
    pub(crate) fn active_project_space_labels(&self) -> Vec<(String, String)> {
        let keys = self.active_project_space_keys();
        if keys.is_empty() {
            return Vec::new();
        }
        let names = self
            .native_sidebar
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.hud.get("projectViewSpaces"))
            .and_then(Value::as_array)
            .map(|spaces| {
                spaces
                    .iter()
                    .filter_map(|space| {
                        let key = view_scope_space_key(
                            space.get("sectionKey").and_then(Value::as_str)?,
                            space.get("spaceId").and_then(Value::as_str)?,
                        );
                        Some((key, space.get("name").and_then(Value::as_str)?.to_string()))
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        keys.into_iter()
            .map(|key| {
                let name = names.get(&key).cloned().unwrap_or_else(|| {
                    parse_view_scope_space_key(&key)
                        .map(|(_, space_id)| space_id.to_string())
                        .unwrap_or_else(|| key.clone())
                });
                (key, name)
            })
            .collect()
    }

    pub(crate) fn official_view_scope_allows(&self, official_extension_id: &str) -> bool {
        self.view_scope_allows(&official_view_scope_key(official_extension_id))
    }

    /// The scope gate for one workarea view. Custom views keep their own richer availability rule in
    /// `custom_project_view_visible`, so they are not scoped twice.
    pub(crate) fn titlebar_mode_view_scope_allows(&self, mode: TitlebarMode) -> bool {
        match mode {
            mode if mode.website_provider().is_some() => {
                self.official_view_scope_allows(&mode.website_provider().unwrap().id)
            }
            mode if mode.is_storybook() => self.official_view_scope_allows("storybook"),
            TitlebarMode::Extension(id) => {
                gpui_custom_view(id).is_some()
                    || self.view_scope_allows(&extension_view_scope_key(id.as_str()))
            }
            _ => match titlebar_mode_official_extension_id(mode) {
                Some(official) => self.official_view_scope_allows(official),
                None => true,
            },
        }
    }
}
