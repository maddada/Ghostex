//! `ProjectViewSettings` (packages/core-ui/settings-modal/project-views/project-settings.tsx (deleted 2026-10-01)): one
//! section per custom view with a `source`, holding this project's binding for it (the website
//! URL or repository override, the command, working directory and ready URL overrides, Start
//! when project opens, Use for worktrees, and Available in this project for a view shown only in
//! selected projects), saved into `customViews[..].projectBindings[projectId]` with Save view
//! settings.
use super::super::super::fields::{
    ButtonVariant, CONTROL_LANE_WIDTH, FieldStates, card_inset, settings_button, settings_section,
    settings_text_input, switch_control,
};
use super::super::super::palette::SettingsPalette;
use super::{ProjectsTab, plain_row};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    px,
};
use gpui_component::h_flex;
use serde_json::{Map, Value, json};

/// The draft of one view's binding for the selected project (the editor's `useState`).
#[derive(Clone, Debug, Default)]
pub(super) struct ViewDraft {
    pub(super) binding: Map<String, Value>,
    pub(super) selected: bool,
    pub(super) error: String,
}

/// A custom view with a source, as the settings store it.
pub(super) struct SourceView {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) url: String,
    pub(super) availability: String,
    pub(super) kind: String,
    pub(super) destination: String,
    pub(super) discovery: String,
    pub(super) command: String,
    pub(super) cwd: String,
    pub(super) readiness_url: String,
    pub(super) value: Value,
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The views with a `source` (`views.filter((view) => view.source)`).
pub(super) fn source_views(custom_views: &Value) -> Vec<SourceView> {
    custom_views
        .as_array()
        .map(|views| {
            views
                .iter()
                .filter_map(|view| {
                    let source = view.get("source").filter(|source| source.is_object())?;
                    Some(SourceView {
                        id: text(view, "id"),
                        name: text(view, "name"),
                        url: text(view, "url"),
                        availability: text(view, "availability"),
                        kind: text(source, "kind"),
                        destination: text(source, "destination"),
                        discovery: text(source, "discovery"),
                        command: text(source, "command"),
                        cwd: text(source, "cwd"),
                        readiness_url: text(source, "readinessUrl"),
                        value: view.clone(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The draft a view starts from for `project_id`.
pub(super) fn initial_draft(view: &SourceView, project_id: &str) -> ViewDraft {
    ViewDraft {
        binding: view
            .value
            .get("projectBindings")
            .and_then(|bindings| bindings.get(project_id))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
        selected: view
            .value
            .get("projectIds")
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(project_id))),
        error: String::new(),
    }
}

/// `normalizeCustomViewUrl`: a complete HTTP or HTTPS URL with a host.
pub(super) fn normalize_custom_view_url(candidate: &str) -> Option<String> {
    let url = url::Url::parse(candidate.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return None;
    }
    Some(url.to_string())
}

fn binding_text(binding: &Map<String, Value>, key: &str) -> String {
    binding
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `binding.x || fallback` with the parent project's inherited binding in between.
fn inherited_text(inherited: Option<&Map<String, Value>>, key: &str) -> String {
    inherited
        .map(|binding| binding_text(binding, key))
        .unwrap_or_default()
}

fn draft_mut<'a>(tab: &'a mut ProjectsTab, view_id: &str) -> &'a mut ViewDraft {
    tab.view_drafts.entry(view_id.to_string()).or_default()
}

/// A binding text field (`TextField` in the control lane).
#[allow(clippy::too_many_arguments)]
fn binding_field(
    tab: &mut ProjectsTab,
    p: &SettingsPalette,
    view_id: &str,
    key: &'static str,
    label: &str,
    description: Option<&str>,
    placeholder: &str,
    window: &mut Window,
    cx: &mut Context<ProjectsTab>,
) -> AnyElement {
    let value = binding_text(&draft_mut(tab, view_id).binding, key);
    let id = SharedString::from(format!("project-view-{view_id}-{key}"));
    let owned_view = view_id.to_string();
    let state = FieldStates::text_state(
        tab,
        &id,
        &value,
        Some(placeholder),
        move |tab: &mut ProjectsTab, next, _window, cx| {
            let draft = draft_mut(tab, &owned_view);
            if binding_text(&draft.binding, key) != next {
                draft.binding.insert(key.to_string(), json!(next));
                draft.error.clear();
                cx.notify();
            }
        },
        window,
        cx,
    );
    tab.sync_placeholder(&id, &state, placeholder, window, cx);
    let control = div()
        .w(px(CONTROL_LANE_WIDTH))
        .max_w_full()
        .flex()
        .child(settings_text_input(p, &state, None, false, window, cx))
        .into_any_element();
    plain_row(p, &id, label, description, None, false, control, cx)
}

/// A binding switch.
#[allow(clippy::too_many_arguments)]
fn binding_switch(
    p: &SettingsPalette,
    view_id: &str,
    id: &str,
    label: &str,
    description: Option<&str>,
    checked: bool,
    on_change: impl Fn(&mut ViewDraft, bool) + 'static,
    cx: &mut Context<ProjectsTab>,
) -> AnyElement {
    let owned_view = view_id.to_string();
    let control = switch_control(
        p,
        SharedString::from(format!("project-view-{view_id}-{id}-switch")),
        label.to_string(),
        checked,
        false,
        None,
        move |tab: &mut ProjectsTab, next, _window, cx| {
            let draft = draft_mut(tab, &owned_view);
            on_change(draft, next);
            draft.error.clear();
            cx.notify();
        },
        cx,
    );
    plain_row(
        p,
        &SharedString::from(format!("project-view-{view_id}-{id}")),
        label,
        description,
        None,
        false,
        control,
        cx,
    )
}

/// One `ProjectViewBindingEditor` section.
pub(super) fn view_section(
    tab: &mut ProjectsTab,
    p: &SettingsPalette,
    view: &SourceView,
    project_id: &str,
    parent_binding: Option<Map<String, Value>>,
    window: &mut Window,
    cx: &mut Context<ProjectsTab>,
) -> Option<AnyElement> {
    let view_id = view.id.clone();
    // `parent?.inherit !== false ? parent : undefined`.
    let inherited = parent_binding.filter(|binding| binding.get("inherit") != Some(&json!(false)));
    let inherited = inherited.as_ref();
    let draft = draft_mut(tab, &view_id).clone();
    let mut rows: Vec<AnyElement> = Vec::new();
    if view.availability == "selected" {
        rows.push(binding_switch(
            p,
            &view_id,
            "available",
            "Available in this project",
            None,
            draft.selected,
            |draft, next| draft.selected = next,
            cx,
        ));
    }
    if view.kind == "website" {
        if view.destination == "project" || view.destination == "fixed" {
            let placeholder = [inherited_text(inherited, "url"), view.url.clone()]
                .into_iter()
                .find(|text| !text.is_empty())
                .unwrap_or_else(|| "https://example.com".to_string());
            rows.push(binding_field(
                tab,
                p,
                &view_id,
                "url",
                if view.name == "Linear" {
                    "Linear URL"
                } else {
                    "Project URL"
                },
                Some("Paste the exact project, workspace, or filtered page to open."),
                &placeholder,
                window,
                cx,
            ));
        } else {
            let placeholder = Some(inherited_text(inherited, "repositoryUrl"))
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "Use the detected GitHub repository".to_string());
            rows.push(binding_field(
                tab,
                p,
                &view_id,
                "repositoryUrl",
                "Repository URL override",
                None,
                &placeholder,
                window,
                cx,
            ));
        }
    } else {
        let command_placeholder = Some(inherited_text(inherited, "command"))
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| {
                if view.discovery == "storybook" {
                    "Detect the Storybook script".to_string()
                } else {
                    view.command.clone()
                }
            });
        rows.push(binding_field(
            tab,
            p,
            &view_id,
            "command",
            "Command override",
            None,
            &command_placeholder,
            window,
            cx,
        ));
        let cwd_placeholder = Some(inherited_text(inherited, "cwd"))
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| view.cwd.clone());
        rows.push(binding_field(
            tab,
            p,
            &view_id,
            "cwd",
            "Working directory override",
            Some("Relative to the checkout. Set this to select one package in a monorepo."),
            &cwd_placeholder,
            window,
            cx,
        ));
        if view.kind == "dev-server" {
            let ready_placeholder = [
                inherited_text(inherited, "readinessUrl"),
                view.readiness_url.clone(),
            ]
            .into_iter()
            .find(|text| !text.is_empty())
            .unwrap_or_else(|| "Detected from the Storybook script".to_string());
            rows.push(binding_field(
                tab,
                p,
                &view_id,
                "readinessUrl",
                "Ready URL override",
                Some("Use the port from this project\u{2019}s command, or {port} in both fields."),
                &ready_placeholder,
                window,
                cx,
            ));
            let start = draft
                .binding
                .get("startOnProjectOpen")
                .and_then(Value::as_bool)
                .or_else(|| {
                    inherited
                        .and_then(|binding| binding.get("startOnProjectOpen"))
                        .and_then(Value::as_bool)
                })
                .unwrap_or(false);
            rows.push(binding_switch(
                p,
                &view_id,
                "startup",
                "Start when project opens",
                Some("Otherwise, starts when you open this view."),
                start,
                |draft, next| {
                    draft
                        .binding
                        .insert("startOnProjectOpen".to_string(), json!(next));
                },
                cx,
            ));
        }
    }
    rows.push(binding_switch(
        p,
        &view_id,
        "inherit",
        "Use for worktrees",
        Some("Inherit these values; commands still run inside each worktree."),
        draft.binding.get("inherit") != Some(&json!(false)),
        |draft, next| {
            draft.binding.insert("inherit".to_string(), json!(next));
        },
        cx,
    ));
    if !draft.error.is_empty() {
        rows.push(card_inset(
            div()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(super::super::super::super::native_modal_kit::hsla(
                    p.destructive,
                ))
                .child(draft.error.clone()),
        ));
    }
    let clear_view = view_id.clone();
    let save_view = view_id.clone();
    let save_project = project_id.to_string();
    rows.push(card_inset(
        h_flex()
            .w_full()
            .justify_end()
            .gap(px(8.0))
            .child(settings_button(
                p,
                SharedString::from(format!("project-view-{view_id}-clear")),
                "Clear",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |tab: &mut ProjectsTab, window, cx| {
                    let draft = draft_mut(tab, &clear_view);
                    draft.binding.clear();
                    draft.error.clear();
                    for key in ["url", "repositoryUrl", "command", "cwd", "readinessUrl"] {
                        let id = format!("project-view-{clear_view}-{key}");
                        tab.set_input_text(&id, "", window, cx);
                    }
                    cx.notify();
                },
                cx,
            ))
            .child(settings_button(
                p,
                SharedString::from(format!("project-view-{view_id}-save")),
                "Save view settings",
                None,
                ButtonVariant::Default,
                false,
                None,
                move |tab: &mut ProjectsTab, _window, cx| {
                    tab.save_view(&save_view, &save_project, cx);
                },
                cx,
            )),
    ));
    settings_section(
        p,
        format!("{} view", view.name),
        Some(
            "Settings for this project. View definitions and templates are managed in Extensions."
                .into(),
        ),
        None,
        rows,
    )
    .map(IntoElement::into_any_element)
}

/// The `customViews` value with this project's binding and availability written into `view_id`.
pub(super) fn saved_views(
    custom_views: &Value,
    view_id: &str,
    project_id: &str,
    draft: &ViewDraft,
) -> Value {
    let views = custom_views.as_array().cloned().unwrap_or_default();
    Value::Array(
        views
            .into_iter()
            .map(|mut view| {
                if view.get("id").and_then(Value::as_str) != Some(view_id) {
                    return view;
                }
                let Some(object) = view.as_object_mut() else {
                    return view;
                };
                let mut bindings = object
                    .get("projectBindings")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                bindings.insert(project_id.to_string(), Value::Object(draft.binding.clone()));
                object.insert("projectBindings".to_string(), Value::Object(bindings));
                let ids: Vec<Value> = object
                    .get("projectIds")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let next_ids: Option<Vec<Value>> = if draft.selected {
                    let mut next = Vec::new();
                    for id in ids.into_iter().chain(std::iter::once(json!(project_id))) {
                        if !next.contains(&id) {
                            next.push(id);
                        }
                    }
                    Some(next)
                } else if object.contains_key("projectIds") {
                    Some(
                        ids.into_iter()
                            .filter(|id| id.as_str() != Some(project_id))
                            .collect(),
                    )
                } else {
                    None
                };
                match next_ids {
                    Some(ids) => {
                        object.insert("projectIds".to_string(), Value::Array(ids));
                    }
                    None => {
                        object.remove("projectIds");
                    }
                }
                view
            })
            .collect(),
    )
}
