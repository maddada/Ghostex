//! The Projects page (packages/core-ui/settings-modal/tabs/projects.tsx (deleted 2026-10-01)), in
//! two panels (regions.rs): the Global settings fold (New projects, Files with the Docs folders
//! list, Global Defaults: global_settings.rs), then the project chosen in the Project dropdown
//! (picker.rs) with its sections: Project settings (worktree command, ticket key, Beads directory,
//! Docs directory, each saved with its own button), Issues Workflow (its Linear key:
//! work_mode.rs), Cloud box (cloud_box.rs) and its custom view settings (views.rs).
//!
//! CDXC:Projects 2026-06-19-12:11 SEE-ALSO: the page edits selected-project metadata only and never offers project deletion.
mod cloud_box;
mod cloud_box_setup;
mod global_settings;
mod picker;
mod regions;
mod views;
mod work_mode;

use super::super::super::native_modal_kit::*;
use super::super::fields::{
    ButtonVariant, FieldStates, ROW_GAP, ROW_MIN_HEIGHT, ROW_PADDING_X, ROW_PADDING_Y,
    SearchableList, SettingsPage, icon, settings_button, settings_icon, settings_section,
    settings_text_input, settings_textarea, tooltip_text,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::search::{TabSearch, should_show_section};
use super::super::store::{SettingsStore, post_store_message};
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, ElementId, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, px,
};
use gpui_component::input::InputState;
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::collections::HashMap;
use views::{ViewDraft, initial_draft, normalize_custom_view_url, saved_views, source_views};

/// `.settings-tab-scroll { padding-top: 16px }` above the first section.
const TAB_SCROLL_PADDING: f32 = 16.0;

/// Creates the Projects page view.
pub(crate) fn projects_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| ProjectsTab::new(store.clone(), window, cx))
        .into()
}

/// `SidebarProjectSettingsItem`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProjectItem {
    pub(crate) project_id: String,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) worktree_command: String,
    pub(crate) beads_display_key: String,
    pub(crate) beads_directory: String,
    pub(crate) docs_directory: String,
    pub(crate) worktree_parent_project_id: Option<String>,
    /// The icon its sidebar row draws, as a data URL (picker.rs).
    pub(crate) icon_data_url: Option<String>,
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn projects_from_hud(hud: Option<&Value>) -> Vec<ProjectItem> {
    hud.and_then(|hud| hud.get("projectSettingsProjects"))
        .and_then(Value::as_array)
        .map(|projects| {
            projects
                .iter()
                .filter_map(|project| {
                    let project_id = text(project, "projectId");
                    (!project_id.is_empty()).then(|| ProjectItem {
                        project_id,
                        name: text(project, "name"),
                        path: text(project, "path"),
                        worktree_command: text(project, "worktreeCommand"),
                        beads_display_key: text(project, "beadsDisplayKey"),
                        beads_directory: text(project, "beadsDirectory"),
                        docs_directory: text(project, "docsDirectory"),
                        worktree_parent_project_id: project
                            .get("worktreeParentProjectId")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        icon_data_url: project
                            .get("iconDataUrl")
                            .and_then(Value::as_str)
                            .filter(|url| !url.is_empty())
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The selected project's editable fields (`useState` seeded from the saved project).
#[derive(Clone, Debug, Default, PartialEq)]
struct ProjectDrafts {
    command: String,
    beads_display_key: String,
    beads_directory: String,
    docs_directory: String,
}

/// What the selected project's drafts are seeded from: its id and its saved values. Nothing else
/// about the project (its name, path or icon) reseeds them, so an icon that arrives later keeps
/// what the user typed.
type DraftSeed = (String, ProjectDrafts);

fn draft_seed(project: Option<&ProjectItem>) -> Option<DraftSeed> {
    project.map(|project| {
        (
            project.project_id.clone(),
            ProjectDrafts::from_project(project),
        )
    })
}

impl ProjectDrafts {
    fn from_project(project: &ProjectItem) -> Self {
        Self {
            command: project.worktree_command.clone(),
            beads_display_key: project.beads_display_key.clone(),
            beads_directory: project.beads_directory.clone(),
            docs_directory: project.docs_directory.clone(),
        }
    }
}

/// `value.toUpperCase().replace(/[^A-Z0-9]/gu, '')` limited to `maxLength={3}`.
fn ticket_key(value: &str) -> String {
    value
        .to_uppercase()
        .chars()
        .filter(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        .take(3)
        .collect()
}

/// `inheritedPlaceholder(projectValue, globalValue, fallback)`.
fn inherited_placeholder(project_value: &str, global_value: &str, fallback: &str) -> String {
    if project_value.trim().is_empty() && !global_value.trim().is_empty() {
        global_value.to_string()
    } else {
        fallback.to_string()
    }
}

/// `InheritedSettingBadge`: a quiet uppercase pill after the field name.
///
/// CDXC:Projects 2026-08-02 SEE-ALSO: the badge marks a field taking its value from Global Defaults and must not compete with the label (`.settings-inherited-badge` in styles/modals.css (deleted 2026-11-06)).
fn inherited_badge(p: &SettingsPalette, id: &SharedString) -> AnyElement {
    div()
        .id(ElementId::Name(format!("{id}-inherited").into()))
        .flex_shrink_0()
        .px(px(6.0))
        .rounded_full()
        .border_1()
        .border_color(hsla(css_fade(p.foreground, 0.25)))
        .text_size(px(11.0))
        .line_height(px(15.4))
        .text_color(hsla(p.foreground))
        .opacity(0.65)
        .whitespace_nowrap()
        .tooltip(tooltip_text("Using the Global Default set above"))
        .child("INHERITED")
        .into_any_element()
}

/// `SettingRow` without a setting key: the label line (with the Inherited badge when given and
/// the info icon revealed on hover when there is a description) and the control beside it, or
/// under it for a wide row.
#[allow(clippy::too_many_arguments)]
pub(super) fn plain_row(
    p: &SettingsPalette,
    id: &SharedString,
    label: &str,
    description: Option<&str>,
    addon: Option<AnyElement>,
    wide: bool,
    control: AnyElement,
    _cx: &mut Context<ProjectsTab>,
) -> AnyElement {
    let group: SharedString = format!("projects-row-{id}").into();
    let mut label_line = h_flex()
        .items_center()
        .gap(px(4.0))
        .min_w_0()
        .child(
            div()
                .min_w_0()
                .text_size(px(14.0))
                .line_height(px(18.9))
                .text_color(hsla(p.foreground))
                .child(label.to_string()),
        )
        .children(addon);
    if let Some(description) = description {
        label_line = label_line.child(
            div()
                .id(ElementId::Name(format!("{id}-info").into()))
                .flex_shrink_0()
                .size(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .opacity(0.0)
                .group_hover(group.clone(), |this| this.opacity(1.0))
                .tooltip(tooltip_text(description.to_string()))
                .child(settings_icon(icon::INFO_CIRCLE, 15.0, p.muted)),
        );
    }
    let row = div()
        .id(ElementId::Name(format!("{id}-row").into()))
        .group(group)
        .w_full()
        .min_h(px(ROW_MIN_HEIGHT))
        .px(px(ROW_PADDING_X))
        .py(px(ROW_PADDING_Y))
        .flex();
    if wide {
        row.flex_col()
            .gap(px(10.0))
            .child(label_line)
            .child(div().w_full().min_w_0().flex().child(control))
            .into_any_element()
    } else {
        row.items_center()
            .justify_between()
            .gap(px(ROW_GAP))
            .child(div().flex_1().min_w_0().child(label_line))
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(gpui::relative(0.6))
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(control),
            )
            .into_any_element()
    }
}

pub(crate) struct ProjectsTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    selected_project_id: String,
    pub(crate) picker: SearchableList,
    drafts: ProjectDrafts,
    /// The project and saved values the drafts were seeded from; a change reseeds them.
    seeded_from: Option<DraftSeed>,
    /// The view drafts of the selected project, by view id (reset when the project changes).
    view_drafts: HashMap<String, ViewDraft>,
    view_drafts_project: String,
    /// The placeholder each input shows now (they follow the Global Defaults).
    placeholders: HashMap<SharedString, String>,
    preview_applied: bool,
    /// The selected project's Work mode switch and Linear key (work_mode.rs).
    work: work_mode::ProjectWorkModeState,
    /// The selected project's Cloud box section (cloud_box.rs).
    cloud_box: cloud_box::ProjectCloudBoxState,
    /// Whether the Global settings fold is open (regions.rs); it starts closed.
    global_open: bool,
}

impl ProjectsTab {
    pub(crate) fn new(
        store: Entity<SettingsStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let picker = SearchableList::new("Search project paths", window, cx);
        // The opening window's project (or the one the opener named); the first project when the
        // page does not list it.
        let selected_project_id = store
            .read(cx)
            .request()
            .initial_project_id
            .clone()
            .unwrap_or_default();
        Self {
            store,
            fields: FieldStates::default(),
            selected_project_id,
            picker,
            drafts: ProjectDrafts::default(),
            seeded_from: None,
            view_drafts: HashMap::new(),
            view_drafts_project: String::new(),
            placeholders: HashMap::new(),
            preview_applied: false,
            work: work_mode::ProjectWorkModeState::default(),
            cloud_box: Default::default(),
            global_open: false,
        }
    }

    fn post(&self, message: Value, cx: &mut App) {
        post_store_message(&self.store, message, cx);
    }

    fn save_setting(&mut self, key: &'static str, value: String, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            if store.string(key) != value {
                store.update_setting(key, json!(value), cx);
            }
        });
    }

    /// `selectProject(projectId)`.
    pub(crate) fn select_project(
        &mut self,
        project_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_project_id = project_id.to_string();
        self.picker.close(window, cx);
        self.picker
            .search
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    /// Replaces an input's buffer (Clear, the ticket key filter), focused or not.
    pub(crate) fn set_input_text(
        &mut self,
        id: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.fields.texts.get(id) {
            let input = state.input.clone();
            if input.read(cx).value().as_ref() != text {
                let text = text.to_string();
                input.update(cx, |input, cx| input.set_value(text, window, cx));
            }
        }
        if let Some(state) = self.fields.textareas.get(id) {
            let input = state.input.clone();
            if input.read(cx).value().as_ref() != text {
                let text = text.to_string();
                input.update(cx, |input, cx| input.set_value(text, window, cx));
            }
        }
    }

    /// Shows `placeholder` in the input `id` (the inherited Global Default or the fallback).
    pub(crate) fn sync_placeholder(
        &mut self,
        id: &SharedString,
        state: &Entity<InputState>,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.placeholders.get(id).map(String::as_str) != Some(placeholder) {
            self.placeholders
                .insert(id.clone(), placeholder.to_string());
            let placeholder = placeholder.to_string();
            state.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx)
            });
        }
    }

    /// `ProjectViewBindingEditor`'s save: the URLs must be complete HTTP(S) URLs.
    pub(crate) fn save_view(&mut self, view_id: &str, project_id: &str, cx: &mut Context<Self>) {
        let Some(draft) = self.view_drafts.get(view_id).cloned() else {
            return;
        };
        let invalid = ["url", "repositoryUrl"].iter().any(|key| {
            draft
                .binding
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|url| !url.is_empty() && normalize_custom_view_url(url).is_none())
        });
        if invalid {
            if let Some(draft) = self.view_drafts.get_mut(view_id) {
                draft.error = "Enter a complete HTTP or HTTPS URL.".to_string();
            }
            cx.notify();
            return;
        }
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            let views = saved_views(&store.value("customViews"), view_id, project_id, &draft);
            store.update_setting("customViews", views, cx);
        });
    }

    /// The preview binary's page states.
    fn apply_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_applied {
            return;
        }
        self.preview_applied = true;
        let Some(state) = self.store.read(cx).request().preview_state.clone() else {
            return;
        };
        match state.as_str() {
            "projects-picker" | "projects-picker-search" => {
                self.picker.open(true, window, cx);
                if state == "projects-picker-search" {
                    self.picker
                        .search
                        .update(cx, |input, cx| input.set_value("infra", window, cx));
                }
                let store = self.store.clone();
                store.update(cx, |store, cx| {
                    store.scroll_to_section(SettingsTabId::Projects, "projectSettings", cx)
                });
            }
            "projects-fold-open" => {
                self.global_open = true;
                cx.notify();
            }
            "projects-dropdown" => self.picker.open(true, window, cx),
            "projects-picked" => self.select_project("project-web", window, cx),
            "projects-inherited"
            | "projects-views"
            | "projects-work-mode"
            | "projects-work-mode-own" => {
                let store = self.store.clone();
                store.update(cx, |store, cx| {
                    store.scroll_to_section(SettingsTabId::Projects, "projectSettings", cx)
                });
            }
            _ => {}
        }
    }

    /// A full-width input bound to a setting (`updateDraft` on every change).
    #[allow(clippy::too_many_arguments)]
    fn setting_input(
        &mut self,
        p: &SettingsPalette,
        key: &'static str,
        value: &str,
        placeholder: &str,
        width: Option<f32>,
        normalize: Option<fn(&str) -> String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = SharedString::from(key);
        let state = FieldStates::text_state(
            self,
            &id,
            value,
            Some(placeholder),
            move |tab: &mut ProjectsTab, text, window, cx| {
                let next = normalize
                    .map(|normalize| normalize(&text))
                    .unwrap_or(text.clone());
                if next != text {
                    tab.set_input_text(key, &next, window, cx);
                }
                tab.save_setting(key, next, cx);
            },
            window,
            cx,
        );
        settings_text_input(p, &state, width, false, window, cx)
    }

    /// A project draft input (`SettingsInput` bound to local state).
    #[allow(clippy::too_many_arguments)]
    fn draft_input(
        &mut self,
        p: &SettingsPalette,
        id: &'static str,
        value: &str,
        placeholder: &str,
        width: Option<f32>,
        set: fn(&mut ProjectDrafts, String),
        normalize: Option<fn(&str) -> String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sid = SharedString::from(id);
        let state = FieldStates::text_state(
            self,
            &sid,
            value,
            Some(placeholder),
            move |tab: &mut ProjectsTab, text, window, cx| {
                let next = normalize
                    .map(|normalize| normalize(&text))
                    .unwrap_or(text.clone());
                if next != text {
                    tab.set_input_text(id, &next, window, cx);
                }
                set(&mut tab.drafts, next);
                cx.notify();
            },
            window,
            cx,
        );
        self.sync_placeholder(&sid, &state, placeholder, window, cx);
        settings_text_input(p, &state, width, false, window, cx)
    }

    /// A Clear / Save pair (`settings-management-actions`).
    fn clear_save(
        &mut self,
        p: &SettingsPalette,
        id: &'static str,
        save_label: &'static str,
        on_clear: fn(&mut ProjectsTab, &mut Window, &mut Context<ProjectsTab>),
        on_save: fn(&mut ProjectsTab, &mut Context<ProjectsTab>),
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        vec![
            settings_button(
                p,
                SharedString::from(format!("{id}-clear")),
                "Clear",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |tab: &mut ProjectsTab, window, cx| on_clear(tab, window, cx),
                cx,
            ),
            settings_button(
                p,
                SharedString::from(format!("{id}-save")),
                save_label,
                None,
                ButtonVariant::Default,
                false,
                None,
                move |tab: &mut ProjectsTab, _window, cx| on_save(tab, cx),
                cx,
            ),
        ]
    }

    fn selected_project_id(&self) -> String {
        self.selected_project_id.clone()
    }
}

impl super::HoldsUnsavedInput for ProjectsTab {
    /// The selected project's fields or a view binding were edited and not saved yet.
    fn holds_unsaved_input(&self, cx: &App) -> bool {
        if self
            .seeded_from
            .as_ref()
            .is_some_and(|(_, saved)| *saved != self.drafts)
        {
            return true;
        }
        if self.view_drafts.is_empty() {
            return false;
        }
        let views = source_views(&self.store.read(cx).value("customViews"));
        self.view_drafts.iter().any(|(view_id, draft)| {
            views.iter().any(|view| {
                view.id == *view_id
                    && initial_draft(view, &self.view_drafts_project).binding != draft.binding
            })
        })
    }
}

impl SettingsPage for ProjectsTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

/// `Empty` with nothing to show: "No projects".
fn no_projects(p: &SettingsPalette) -> AnyElement {
    v_flex()
        .w_full()
        .items_center()
        .justify_center()
        .p(px(48.0))
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(16.0))
                .line_height(px(24.9))
                .text_color(hsla(p.foreground))
                .child("No projects"),
        )
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(21.1))
                .text_color(hsla(p.muted))
                .child("Main projects will appear here."),
        )
        .into_any_element()
}

impl Render for ProjectsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, values, search, projects, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.values(),
                store.tab_search(SettingsTabId::Projects),
                projects_from_hud(store.hud()),
                matching,
            )
        };
        // `setSelectedProjectId(projects[0])` when the selection is gone.
        if !projects
            .iter()
            .any(|project| project.project_id == self.selected_project_id)
        {
            self.selected_project_id = projects
                .first()
                .map(|project| project.project_id.clone())
                .unwrap_or_default();
        }
        let selected = projects
            .iter()
            .find(|project| project.project_id == self.selected_project_id)
            .cloned();
        // Reseed the drafts when the project or its saved values change.
        let seed = draft_seed(selected.as_ref());
        if seed != self.seeded_from {
            self.drafts = seed
                .as_ref()
                .map(|(_, saved)| saved.clone())
                .unwrap_or_default();
            self.seeded_from = seed;
            let drafts = self.drafts.clone();
            self.set_input_text("project-worktree-command", &drafts.command, window, cx);
            self.set_input_text("project-ticket-key", &drafts.beads_display_key, window, cx);
            self.set_input_text(
                "project-beads-directory",
                &drafts.beads_directory,
                window,
                cx,
            );
            self.set_input_text("project-docs-directory", &drafts.docs_directory, window, cx);
        }
        self.apply_preview(window, cx);
        let global_command = values.string("globalWorktreeCommand");
        let global_key = values.string("globalBeadsDisplayKey");
        let global_beads = values.string("globalBeadsDirectory");
        let global_docs = values.string("globalDocsDirectory");

        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.has_matches() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Projects,
                &matching,
                move |tab, _window, cx| {
                    store.update(cx, |store, cx| store.set_active_tab(tab, cx));
                },
            )));
        }

        blocks.extend(self.page_regions(
            &p,
            &values,
            &search,
            &projects,
            selected,
            [&global_command, &global_key, &global_beads, &global_docs],
            window,
            cx,
        ));
        if let Some(first) = blocks.first_mut() {
            first.margin_top += TAB_SCROLL_PADDING;
        }
        div().size_full().child(settings_page(
            &self.store,
            SettingsTabId::Projects,
            &p,
            blocks,
            cx,
        ))
    }
}

impl ProjectsTab {
    /// The picked project's sections a search leaves on screen: Project settings, Issues
    /// Workflow and Cloud box (each its own catalog section), then one per custom view.
    #[allow(clippy::too_many_arguments)]
    fn project_sections(
        &mut self,
        p: &SettingsPalette,
        projects: &[ProjectItem],
        project: &ProjectItem,
        globals: [&str; 4],
        custom_views: &Value,
        search: &TabSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let shows = |id: &str| should_show_section(&search.section(id), None);
        let mut sections = Vec::new();
        if shows("projectSettings") {
            sections.extend(self.project_settings_section(p, globals, window, cx));
        }
        if shows("issuesWorkflow") {
            let rows = self.work_mode_rows(p, projects, project, window, cx);
            sections.extend(
                settings_section(p, "Issues Workflow", None, None, rows)
                    .map(IntoElement::into_any_element),
            );
        }
        if shows("projectCloudBox") {
            sections.extend(self.cloud_box_section(p, project, window, cx));
        }
        // The custom views have no catalog rows: a search shows them only when it names the page.
        if !search.tab.is_searching || search.tab.group_title_matches {
            sections.extend(self.view_sections(p, project, custom_views, window, cx));
        }
        sections
    }

    /// The Project settings section: the project's own fields, each with Clear and Save.
    fn project_settings_section(
        &mut self,
        p: &SettingsPalette,
        globals: [&str; 4],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let [global_command, global_key, global_beads, global_docs] = globals;
        let drafts = self.drafts.clone();
        let mut rows: Vec<AnyElement> = Vec::new();

        // CDXC:Projects 2026-06-15-03:21 SEE-ALSO: the worktree command is the first editable project field; ticket key and Beads directory follow.
        let command_inherited =
            drafts.command.trim().is_empty() && !global_command.trim().is_empty();
        let command_id = SharedString::from("project-worktree-command");
        let command_state = FieldStates::textarea_state(
            self,
            &command_id,
            &drafts.command,
            Some("bun install"),
            (1, 20),
            |tab: &mut ProjectsTab, text, _window, cx| {
                tab.drafts.command = text;
                cx.notify();
            },
            window,
            cx,
        );
        let command_placeholder =
            inherited_placeholder(&drafts.command, global_command, "bun install");
        if self.placeholders.get(&command_id) != Some(&command_placeholder) {
            self.placeholders
                .insert(command_id.clone(), command_placeholder.clone());
            command_state.update(cx, |input, cx| {
                input.set_placeholder(command_placeholder, window, cx)
            });
        }
        let command_actions = self.clear_save(
            p,
            "project-worktree-command",
            "Save Command",
            |tab, window, cx| {
                tab.drafts.command.clear();
                tab.set_input_text("project-worktree-command", "", window, cx);
                cx.notify();
            },
            |tab, cx| {
                let project_id = tab.selected_project_id();
                if project_id.is_empty() {
                    return;
                }
                tab.post(
                    json!({
                        "command": tab.drafts.command,
                        "projectId": project_id,
                        "type": "setProjectWorktreeCommand",
                    }),
                    cx,
                );
            },
            cx,
        );
        rows.push(plain_row(
            p,
            &SharedString::from("project-worktree-command-row"),
            "Worktree command",
            Some("Runs in the new worktree folder before the project is added (Useful for .envs/installing dependencies/etc.)"),
            command_inherited.then(|| inherited_badge(p, &command_id)),
            true,
            v_flex()
                .w_full()
                .gap(px(12.0))
                .child(settings_textarea(p, &command_state, 96.0, false, false, window, cx))
                .child(h_flex().w_full().justify_end().gap(px(8.0)).children(command_actions))
                .into_any_element(),
            cx,
        ));

        // CDXC:ProjectBoard 2026-05-23-14:35 SEE-ALSO: the three-letter ticket key shown on the board lives here while Beads keeps hash ids.
        let key_inherited =
            drafts.beads_display_key.trim().is_empty() && !global_key.trim().is_empty();
        let key_input = self.draft_input(
            p,
            "project-ticket-key",
            &drafts.beads_display_key,
            &inherited_placeholder(&drafts.beads_display_key, global_key, "ZMX"),
            Some(96.0),
            |drafts, text| drafts.beads_display_key = text,
            Some(ticket_key),
            window,
            cx,
        );
        let key_actions = self.clear_save(
            p,
            "project-ticket-key",
            "Save Ticket Key",
            |tab, window, cx| {
                tab.drafts.beads_display_key.clear();
                tab.set_input_text("project-ticket-key", "", window, cx);
                cx.notify();
            },
            |tab, cx| {
                let project_id = tab.selected_project_id();
                if project_id.is_empty() {
                    return;
                }
                tab.post(
                    json!({
                        "displayKey": tab.drafts.beads_display_key,
                        "projectId": project_id,
                        "type": "setProjectBeadsDisplayKey",
                    }),
                    cx,
                );
            },
            cx,
        );
        rows.push(plain_row(
            p,
            &SharedString::from("project-ticket-key-row"),
            "Ticket key",
            Some("Three-letter prefix used for Linear-style ticket numbers on the Project board."),
            key_inherited.then(|| inherited_badge(p, &SharedString::from("project-ticket-key"))),
            false,
            h_flex()
                .items_center()
                .gap(px(8.0))
                .child(key_input)
                .children(key_actions)
                .into_any_element(),
            cx,
        ));

        // CDXC:ProjectBoard 2026-06-13 SEE-ALSO: the Project board launches its Beads workspace from this directory, the project root when blank.
        let beads_inherited =
            drafts.beads_directory.trim().is_empty() && !global_beads.trim().is_empty();
        let beads_input = self.draft_input(
            p,
            "project-beads-directory",
            &drafts.beads_directory,
            &inherited_placeholder(
                &drafts.beads_directory,
                global_beads,
                "/Users/you/code/my-repo",
            ),
            None,
            |drafts, text| drafts.beads_directory = text,
            None,
            window,
            cx,
        );
        let beads_actions = self.clear_save(
            p,
            "project-beads-directory",
            "Save Beads Directory",
            |tab, window, cx| {
                tab.drafts.beads_directory.clear();
                tab.set_input_text("project-beads-directory", "", window, cx);
                cx.notify();
            },
            |tab, cx| {
                let project_id = tab.selected_project_id();
                if project_id.is_empty() {
                    return;
                }
                tab.post(
                    json!({
                        "directory": tab.drafts.beads_directory,
                        "projectId": project_id,
                        "type": "setProjectBeadsDirectory",
                    }),
                    cx,
                );
            },
            cx,
        );
        rows.push(plain_row(
            p,
            &SharedString::from("project-beads-directory-row"),
            "Beads directory",
            Some("Path to this project's Beads workspace (.beads). Leave blank to use the Global Default or project root."),
            beads_inherited
                .then(|| inherited_badge(p, &SharedString::from("project-beads-directory"))),
            true,
            v_flex()
                .w_full()
                .gap(px(12.0))
                .child(beads_input)
                .child(h_flex().w_full().justify_end().gap(px(8.0)).children(beads_actions))
                .into_any_element(),
            cx,
        ));

        // CDXC:Docs 2026-08-09 SEE-ALSO: a project's `docsDirectory` only ever adds a tree beside its own docs; blank uses the Global Default.
        let docs_inherited =
            drafts.docs_directory.trim().is_empty() && !global_docs.trim().is_empty();
        let docs_input = self.draft_input(
            p,
            "project-docs-directory",
            &drafts.docs_directory,
            &inherited_placeholder(
                &drafts.docs_directory,
                global_docs,
                "/Users/you/Documents/vault",
            ),
            None,
            |drafts, text| drafts.docs_directory = text,
            None,
            window,
            cx,
        );
        let docs_actions = self.clear_save(
            p,
            "project-docs-directory",
            "Save Docs Directory",
            |tab, window, cx| {
                tab.drafts.docs_directory.clear();
                tab.set_input_text("project-docs-directory", "", window, cx);
                cx.notify();
            },
            |tab, cx| {
                let project_id = tab.selected_project_id();
                if project_id.is_empty() {
                    return;
                }
                tab.post(
                    json!({
                        "directory": tab.drafts.docs_directory,
                        "projectId": project_id,
                        "type": "setProjectDocsDirectory",
                    }),
                    cx,
                );
            },
            cx,
        );
        rows.push(plain_row(
            p,
            &SharedString::from("project-docs-directory-row"),
            "Docs directory",
            Some("Extra folder this project's Files view shows, in addition to the project's own docs. Leave blank to use the Global Default."),
            docs_inherited
                .then(|| inherited_badge(p, &SharedString::from("project-docs-directory"))),
            true,
            v_flex()
                .w_full()
                .gap(px(12.0))
                .child(docs_input)
                .child(h_flex().w_full().justify_end().gap(px(8.0)).children(docs_actions))
                .into_any_element(),
            cx,
        ));

        // No heading: the "Project" picker above names it (CDXC:Settings 2026-11-11 at fields/row.rs).
        settings_section(p, "", None, None, rows).map(IntoElement::into_any_element)
    }

    /// One section per custom view, with this project's binding.
    fn view_sections(
        &mut self,
        p: &SettingsPalette,
        project: &ProjectItem,
        custom_views: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut sections = Vec::new();
        // `ProjectViewSettings`, keyed by `${projectId}:${view.id}`: a new project starts fresh drafts.
        let views = source_views(custom_views);
        if self.view_drafts_project != project.project_id {
            self.view_drafts_project = project.project_id.clone();
            self.view_drafts.clear();
            for view in &views {
                for key in ["url", "repositoryUrl", "command", "cwd", "readinessUrl"] {
                    let id = format!("project-view-{}-{key}", view.id);
                    let value = initial_draft(view, &project.project_id)
                        .binding
                        .get(key)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    self.set_input_text(&id, &value, window, cx);
                }
            }
        }
        let parent_bindings = |view: &views::SourceView| {
            project
                .worktree_parent_project_id
                .as_ref()
                .and_then(|parent| {
                    view.value
                        .get("projectBindings")
                        .and_then(|bindings| bindings.get(parent))
                        .and_then(Value::as_object)
                        .cloned()
                })
        };
        for view in &views {
            if !self.view_drafts.contains_key(&view.id) {
                self.view_drafts
                    .insert(view.id.clone(), initial_draft(view, &project.project_id));
            }
            let parent = parent_bindings(view);
            sections.extend(views::view_section(
                self,
                p,
                view,
                &project.project_id,
                parent,
                window,
                cx,
            ));
        }
        sections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: &str, command: &str, icon: Option<&str>) -> ProjectItem {
        ProjectItem {
            project_id: id.to_string(),
            name: "Ghostex".to_string(),
            path: "/Users/you/dev/ghostex".to_string(),
            worktree_command: command.to_string(),
            beads_display_key: "ZMX".to_string(),
            beads_directory: String::new(),
            docs_directory: String::new(),
            worktree_parent_project_id: None,
            icon_data_url: icon.map(str::to_string),
        }
    }

    #[test]
    fn only_the_project_and_its_saved_values_reseed_the_drafts() {
        let seeded = draft_seed(Some(&project("p1", "bun install", None)));
        let icon_arrived = project("p1", "bun install", Some("data:image/png;base64,AA=="));
        assert_eq!(draft_seed(Some(&icon_arrived)), seeded);
        let saved_elsewhere = project("p1", "pnpm install", None);
        assert_ne!(draft_seed(Some(&saved_elsewhere)), seeded);
        let other_project = project("p2", "bun install", None);
        assert_ne!(draft_seed(Some(&other_project)), seeded);
    }
}
