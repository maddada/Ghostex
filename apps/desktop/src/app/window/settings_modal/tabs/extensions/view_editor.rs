//! `CustomViewEditor` and `ProjectViewTemplates` (settings-modal/project-views/editor.tsx (deleted 2026-10-01) and
//! templates.tsx): the custom view form laid out as setting rows inside a list card (Name,
//! Source, Available in, Spaces, Destination and URL, or the dev server and report fields,
//! Startup timeout, Enabled), its notice and error, Save as template / Cancel / Save; and the
//! template picker with its search field.
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    CONTROL_LANE_WIDTH, FieldStates, RowSpec, SizedButtonSize, SizedButtonVariant,
    checkbox_control, icon, select_field, setting_row, settings_icon, settings_list_item,
    settings_segmented, settings_sized_button, settings_square_button, settings_text_input,
    settings_textarea, switch_control,
};
use super::super::super::palette::SettingsPalette;
use super::ExtensionsTab;
use super::data::{
    CUSTOM_VIEW_ID_PREFIX, CustomView, builtin_templates, custom_view_templates, custom_views,
    default_project_view_source, normalize_custom_view_url, random_uuid, scope_projects_and_spaces,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, Focusable as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};

/// `CustomViewEditorState`.
#[derive(Clone, Debug)]
pub(crate) struct ViewEditorState {
    pub(crate) draft: Map<String, Value>,
    pub(crate) id: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) notice: Option<String>,
}

impl ViewEditorState {
    /// `{ draft: { ...view }, id: view.id }`.
    pub(crate) fn edit(view: &CustomView) -> Self {
        Self {
            draft: view.raw.clone(),
            id: Some(view.id()),
            error: None,
            notice: None,
        }
    }

    fn view(&self) -> CustomView {
        CustomView {
            raw: self.draft.clone(),
        }
    }
}

fn text_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => {
            super::super::super::catalog::js_number_string(number.as_f64().unwrap_or_default())
        }
        _ => String::new(),
    }
}

impl ExtensionsTab {
    /// `update(patch)`: merges into the draft and clears the error.
    fn update_view_draft(
        &mut self,
        cx: &mut Context<Self>,
        apply: impl FnOnce(&mut Map<String, Value>),
    ) {
        if let Some(editor) = self.view_editor.as_mut() {
            apply(&mut editor.draft);
            editor.error = None;
            cx.notify();
        }
    }

    /// `updateSource(patch)`.
    fn update_view_source(&mut self, key: &'static str, value: Value, cx: &mut Context<Self>) {
        self.update_view_draft(cx, move |draft| {
            let mut source = draft
                .get("source")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_else(default_project_view_source);
            source.insert(key.to_string(), value);
            draft.insert("source".into(), Value::Object(source));
        });
    }

    /// A text row bound to the draft: `apply` stores the typed text.
    #[allow(clippy::too_many_arguments)]
    fn editor_text_row(
        &mut self,
        p: &SettingsPalette,
        id: &'static str,
        spec: RowSpec,
        value: &str,
        placeholder: Option<&str>,
        apply: fn(&mut ExtensionsTab, String, &mut Context<ExtensionsTab>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = FieldStates::text_state(
            self,
            &SharedString::from(id),
            value,
            placeholder,
            move |page: &mut Self, text, _window, cx| apply(page, text, cx),
            window,
            cx,
        );
        let control = div()
            .w(px(CONTROL_LANE_WIDTH))
            .max_w_full()
            .flex()
            .child(settings_text_input(p, &input, None, false, window, cx))
            .into_any_element();
        setting_row(p, id, spec, None, control, cx)
    }

    /// The editor inside its list card.
    /// CDXC:Settings 2026-09-09 DECISION:
    /// User: the custom view editor reuses the new settings look and shared controls. Keep Cancel bordered like the other editor actions.
    pub(crate) fn render_view_editor(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(editor) = self.view_editor.clone() else {
            return div().into_any_element();
        };
        let view = editor.view();
        let source = view.source_or_default();
        let kind = text_of(source.get("kind"));
        let kind = if kind.is_empty() {
            "website".to_string()
        } else {
            kind
        };
        let destination = text_of(source.get("destination"));
        let discovery = text_of(source.get("discovery"));
        let availability = view.availability();
        let mut rows: Vec<AnyElement> = Vec::new();
        rows.push(self.editor_text_row(
            p,
            "view-editor-name",
            RowSpec::new("Name"),
            &view.name(),
            Some("Tab name"),
            |page, text, cx| {
                page.update_view_draft(cx, |draft| {
                    draft.insert("name".into(), json!(text));
                })
            },
            window,
            cx,
        ));
        let source_options = [
            ("website", "Website"),
            ("dev-server", "Dev server"),
            ("report", "HTML report"),
        ]
        .map(|(value, label)| SettingOption {
            label: label.into(),
            value: value.into(),
        });
        let source_control = settings_segmented(
            p,
            "view-editor-source",
            &source_options,
            Some(kind.as_str()),
            |page: &mut Self, next, _window, cx| page.update_view_source("kind", json!(next), cx),
            cx,
        );
        rows.push(setting_row(
            p,
            "view-editor-source-row",
            RowSpec::new("Source"),
            None,
            source_control,
            cx,
        ));
        let availability_options = [
            ("all", "All projects"),
            ("matching", "Matching projects"),
            ("selected", "Selected projects"),
            ("spaces", "Selected spaces"),
        ]
        .map(|(value, label)| SettingOption {
            label: label.into(),
            value: value.into(),
        });
        rows.push(select_field(
            self,
            p,
            "view-editor-availability",
            RowSpec::new("Available in").description(
                "Selected projects are enabled individually in Settings → Projects. Matching projects show the view when its source can be resolved.",
            ),
            None,
            &availability_options,
            &availability,
            None,
            |page: &mut Self, next, _window, cx| {
                page.update_view_draft(cx, |draft| {
                    if !draft.contains_key("source") {
                        draft.insert("source".into(), Value::Object(default_project_view_source()));
                    }
                    draft.insert("availability".into(), json!(next));
                });
            },
            window,
            cx,
        ));
        if availability == "spaces" {
            rows.push(self.render_space_picker(p, &view, cx));
        }
        if kind == "website" {
            let destination_options = [
                ("fixed", "Fixed URL"),
                ("project", "Project URL"),
                ("github-issues", "GitHub Issues"),
                ("github-actions", "GitHub Actions"),
                ("github-pulls", "GitHub Pull Requests"),
            ]
            .map(|(value, label)| SettingOption {
                label: label.into(),
                value: value.into(),
            });
            rows.push(select_field(
                self,
                p,
                "view-editor-destination",
                RowSpec::new("Destination").description(
                    "Project URLs are supplied in Settings → Projects. GitHub destinations use the selected project’s repository, with an optional project override.",
                ),
                None,
                &destination_options,
                &destination,
                None,
                |page: &mut Self, next, _window, cx| {
                    page.update_view_source("destination", json!(next), cx)
                },
                window,
                cx,
            ));
            if destination == "fixed" {
                rows.push(self.editor_text_row(
                    p,
                    "view-editor-url",
                    RowSpec::new("URL"),
                    &view.url(),
                    Some("https://example.com"),
                    |page, text, cx| {
                        page.update_view_draft(cx, |draft| {
                            draft.insert("url".into(), json!(text));
                        })
                    },
                    window,
                    cx,
                ));
            }
        } else {
            if kind == "dev-server" {
                let discovery_options = [
                    ("command", "Configured command"),
                    ("storybook", "Detect Storybook script"),
                ]
                .map(|(value, label)| SettingOption {
                    label: label.into(),
                    value: value.into(),
                });
                rows.push(select_field(
                    self,
                    p,
                    "view-editor-discovery",
                    RowSpec::new("Command source").description(
                        "Detects package scripts and their package manager without running them. Select a package or override the command in Projects settings when several Storybooks exist.",
                    ),
                    None,
                    &discovery_options,
                    &discovery,
                    None,
                    |page: &mut Self, next, _window, cx| {
                        page.update_view_source("discovery", json!(next), cx)
                    },
                    window,
                    cx,
                ));
            }
            if kind == "report" || discovery == "command" {
                let command = text_of(source.get("command"));
                let placeholder = if kind == "report" {
                    "bun run coverage"
                } else {
                    "bun run dev --port {port}"
                };
                let area = FieldStates::textarea_state(
                    self,
                    &SharedString::from("view-editor-command"),
                    &command,
                    Some(placeholder),
                    (2, 8),
                    |page: &mut Self, text, _window, cx| {
                        page.update_view_source("command", json!(text), cx)
                    },
                    window,
                    cx,
                );
                let control = settings_textarea(p, &area, 64.0, false, false, window, cx);
                rows.push(setting_row(
                    p,
                    "view-editor-command-row",
                    RowSpec::new("Command")
                        .description(if kind == "report" {
                            "Optional. Generate the report before serving it."
                        } else {
                            "Runs on the project’s computer when you open the view."
                        })
                        .wide(),
                    None,
                    control,
                    cx,
                ));
                rows.push(
                    self.editor_text_row(
                        p,
                        "view-editor-cwd",
                        RowSpec::new("Working directory")
                            .description("Relative to each checkout. Use . for the project root."),
                        &text_of(source.get("cwd")),
                        None,
                        |page, text, cx| page.update_view_source("cwd", json!(text), cx),
                        window,
                        cx,
                    ),
                );
            }
            if kind == "report" {
                rows.push(self.editor_text_row(
                    p,
                    "view-editor-report-directory",
                    RowSpec::new("Report directory").description(
                        "Relative to the checkout. Assets are served from this directory.",
                    ),
                    &text_of(source.get("reportDirectory")),
                    Some("coverage"),
                    |page, text, cx| page.update_view_source("reportDirectory", json!(text), cx),
                    window,
                    cx,
                ));
                rows.push(self.editor_text_row(
                    p,
                    "view-editor-entry",
                    RowSpec::new("Entry page"),
                    &text_of(source.get("entry")),
                    Some("index.html"),
                    |page, text, cx| page.update_view_source("entry", json!(text), cx),
                    window,
                    cx,
                ));
            } else {
                rows.push(self.editor_text_row(
                    p,
                    "view-editor-ready-url",
                    RowSpec::new("Ready URL").description(
                        "Use {port} in both command and URL for an allocated port. Storybook can detect its configured port.",
                    ),
                    &text_of(source.get("readinessUrl")),
                    Some(if discovery == "storybook" {
                        "Detected from the Storybook script"
                    } else {
                        "http://127.0.0.1:{port}/"
                    }),
                    |page, text, cx| page.update_view_source("readinessUrl", json!(text), cx),
                    window,
                    cx,
                ));
            }
            rows.push(self.editor_text_row(
                p,
                "view-editor-timeout",
                RowSpec::new("Startup timeout").description(
                    "Starts when you open the view. Project settings can enable startup when the project opens. Switching views keeps the process running.",
                ),
                &text_of(source.get("timeoutSeconds")),
                None,
                |page, text, cx| {
                    // `Number(e.currentTarget.value)`: empty is 0, anything else unparsable is NaN.
                    let trimmed = text.trim();
                    let value = if trimmed.is_empty() {
                        json!(0)
                    } else {
                        trimmed
                            .parse::<f64>()
                            .ok()
                            .filter(|number| number.is_finite())
                            .map(|number| {
                                if number.fract() == 0.0 {
                                    json!(number as i64)
                                } else {
                                    json!(number)
                                }
                            })
                            .unwrap_or(Value::Null)
                    };
                    page.update_view_source("timeoutSeconds", value, cx)
                },
                window,
                cx,
            ));
        }
        rows.push(setting_row(
            p,
            "view-editor-enabled",
            RowSpec::new("Enabled"),
            None,
            switch_control(
                p,
                "view-editor-enabled-switch",
                "Enabled",
                view.enabled(),
                false,
                None,
                |page: &mut Self, next, _window, cx| {
                    page.update_view_draft(cx, |draft| {
                        draft.insert("enabled".into(), json!(next));
                    })
                },
                cx,
            ),
            cx,
        ));
        // The setting rows drop their own inset inside the panel (`.settings-list-panel > *`).
        let setting_rows = rows.len();
        if let Some(notice) = editor.notice.clone() {
            rows.push(
                div()
                    .w_full()
                    .py(px(10.0))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.muted))
                    .child(notice)
                    .into_any_element(),
            );
        }
        if let Some(error) = editor.error.clone() {
            rows.push(
                div()
                    .w_full()
                    .py(px(10.0))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.destructive))
                    .child(error)
                    .into_any_element(),
            );
        }
        let save_template = settings_sized_button(
            p,
            "view-editor-save-template",
            "Save as template",
            Some("modals/settings/template.svg"),
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Default,
            false,
            None,
            |page: &mut Self, _window, cx| page.save_view_template(cx),
            cx,
        );
        let cancel = settings_sized_button(
            p,
            "view-editor-cancel",
            "Cancel",
            Some(icon::X),
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Default,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.view_editor = None;
                cx.notify();
            },
            cx,
        );
        let save = settings_sized_button(
            p,
            "view-editor-save",
            if editor.id.is_some() {
                "Save changes"
            } else {
                "Add view"
            },
            Some(super::super::super::fields::CHECK_ICON),
            None,
            SizedButtonVariant::Default,
            SizedButtonSize::Default,
            false,
            None,
            |page: &mut Self, _window, cx| page.save_custom_view(cx),
            cx,
        );
        rows.push(
            h_flex()
                .w_full()
                .py(px(12.0))
                .gap(px(8.0))
                .child(save_template)
                .child(div().flex_1())
                .child(cancel)
                .child(save)
                .into_any_element(),
        );
        let hairline = hsla(p.hairline);
        let panel = v_flex()
            .w_full()
            .children(rows.into_iter().enumerate().map(|(index, row)| {
                div()
                    .w_full()
                    .when(index > 0, |this| this.border_t_1().border_color(hairline))
                    .child(if index < setting_rows {
                        div().mx(px(-20.0)).child(row).into_any_element()
                    } else {
                        row
                    })
            }));
        let name_input: Option<Entity<InputState>> = self
            .fields
            .texts
            .get("view-editor-name")
            .map(|state| state.input.clone());
        let focus = self.focus_view_editor.clone();
        let handle = self.store.update(cx, |store, _| {
            store.scroll_handle(super::super::super::model::SettingsTabId::Extensions)
        });
        div()
            .w_full()
            .on_children_prepainted(move |bounds, window, cx| {
                if !focus.get() {
                    return;
                }
                let Some(editor) = bounds.first() else {
                    return;
                };
                focus.set(false);
                // `scrollIntoView({ block: 'start' })`, then the first field takes focus.
                let top = super::super::super::store::scroll_top_for_child(&handle, *editor);
                let offset = handle.offset();
                handle.set_offset(gpui::point(offset.x, px(-top)));
                if let Some(input) = name_input.clone() {
                    input.update(cx, |input, cx| input.focus(window, cx));
                }
                window.refresh();
            })
            .child(
                div()
                    .w_full()
                    .px(px(20.0))
                    .py(px(14.0))
                    .rounded(px(MODAL_RADIUS_SECTION))
                    .border_1()
                    .border_color(hairline)
                    .bg(hsla(p.raised))
                    .child(panel),
            )
            .into_any_element()
    }

    /// The Spaces row: a checkbox per space (and per stored space that is gone).
    fn render_space_picker(
        &mut self,
        p: &SettingsPalette,
        view: &CustomView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (_, spaces) = scope_projects_and_spaces(self.store.read(cx).hud());
        let refs = view.space_refs();
        let mut options: Vec<(String, String, String)> = spaces
            .iter()
            .map(|space| {
                (
                    space.section_key.clone(),
                    space.space_id.clone(),
                    space.name.clone(),
                )
            })
            .collect();
        for (section, space) in &refs {
            if !spaces
                .iter()
                .any(|candidate| candidate.section_key == *section && candidate.space_id == *space)
            {
                options.push((section.clone(), space.clone(), "Unavailable space".into()));
            }
        }
        let control = if options.is_empty() {
            div()
                .text_size(px(14.0))
                .text_color(hsla(p.muted))
                .child("No spaces available. Create a space in the sidebar first.")
                .into_any_element()
        } else {
            h_flex()
                .w_full()
                .flex_wrap()
                .gap(px(16.0))
                .children(options.into_iter().map(|(section, space, name)| {
                    let checked = refs.iter().any(|(ref_section, ref_space)| {
                        *ref_section == section && *ref_space == space
                    });
                    checkbox_control(
                        p,
                        SharedString::from(format!("view-editor-space-{section}:{space}")),
                        checked,
                        Some(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(20.0))
                                .child(name)
                                .into_any_element(),
                        ),
                        8.0,
                        move |page: &mut Self, selected, _window, cx| {
                            let section = section.clone();
                            let space = space.clone();
                            page.update_view_draft(cx, move |draft| {
                                let mut refs: Vec<Value> = draft
                                    .get("spaceRefs")
                                    .and_then(Value::as_array)
                                    .cloned()
                                    .unwrap_or_default();
                                if selected {
                                    refs.push(json!({ "sectionKey": section, "spaceId": space }));
                                } else {
                                    refs.retain(|entry| {
                                        entry["sectionKey"] != section.as_str()
                                            || entry["spaceId"] != space.as_str()
                                    });
                                }
                                draft.insert("spaceRefs".into(), Value::Array(refs));
                            });
                        },
                        cx,
                    )
                }))
                .into_any_element()
        };
        setting_row(
            p,
            "view-editor-spaces",
            RowSpec::new("Spaces")
                .description(
                    "Show this view for any project in a selected space, including projects in its groups and their worktrees.",
                )
                .wide(),
            None,
            control,
            cx,
        )
    }

    /// `saveCustomView`.
    fn save_custom_view(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.view_editor.clone() else {
            return;
        };
        let view = editor.view();
        let name = view.name().trim().to_string();
        let source = view.source().cloned();
        let needs_url = source.as_ref().is_none_or(|source| {
            source.get("kind").and_then(Value::as_str) == Some("website")
                && source.get("destination").and_then(Value::as_str) == Some("fixed")
        });
        let url = normalize_custom_view_url(&view.url()).unwrap_or_default();
        let fail = |page: &mut Self, message: &str, cx: &mut Context<Self>| {
            if let Some(editor) = page.view_editor.as_mut() {
                editor.error = Some(message.to_string());
            }
            cx.notify();
        };
        if name.is_empty() || (needs_url && url.is_empty()) {
            fail(
                self,
                if name.is_empty() {
                    "Enter a name for the view tab."
                } else {
                    "Enter a complete HTTP or HTTPS URL."
                },
                cx,
            );
            return;
        }
        if view.availability() == "spaces" && view.space_refs().is_empty() {
            fail(self, "Choose at least one space.", cx);
            return;
        }
        if source.as_ref().is_some_and(|source| {
            source.get("kind").and_then(Value::as_str) == Some("report")
                && source
                    .get("reportDirectory")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
        }) {
            fail(self, "Enter the report directory.", cx);
            return;
        }
        let mut raw = editor.draft.clone();
        let id = editor
            .id
            .clone()
            .unwrap_or_else(|| format!("{CUSTOM_VIEW_ID_PREFIX}{}", random_uuid()));
        raw.insert("id".into(), json!(id));
        raw.insert("name".into(), json!(name));
        raw.insert("url".into(), json!(url));
        let saved = CustomView { raw };
        let values = self.store.read(cx).values();
        let mut views = custom_views(&values);
        match &editor.id {
            Some(existing) => {
                for view in &mut views {
                    if view.id() == *existing {
                        *view = saved.clone();
                    }
                }
            }
            None => views.push(saved),
        }
        self.save_custom_views(&views, cx);
        self.view_editor = None;
        cx.notify();
    }

    /// `saveViewTemplate`.
    fn save_view_template(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.view_editor.clone() else {
            return;
        };
        let view = editor.view();
        let name = view.name().trim().to_string();
        if name.is_empty() {
            if let Some(editor) = self.view_editor.as_mut() {
                editor.error = Some("Enter a name for the template.".into());
            }
            cx.notify();
            return;
        }
        let mut source = view.source_or_default();
        if source.get("kind").and_then(Value::as_str) == Some("website")
            && source.get("destination").and_then(Value::as_str) == Some("fixed")
        {
            source.insert("destination".into(), json!("project"));
        }
        let cwd = source
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let absolute = cwd.starts_with('/')
            || cwd.starts_with('\\')
            || cwd
                .as_bytes()
                .get(..2)
                .is_some_and(|head| head[0].is_ascii_alphabetic() && head[1] == b':');
        if absolute {
            source.insert("cwd".into(), json!("."));
        }
        let template = json!({
            "id": format!("personal-{}", random_uuid()),
            "name": name,
            "url": "",
            "source": source,
            "availability": "matching",
        });
        let values = self.store.read(cx).values();
        let mut templates: Vec<Value> = custom_view_templates(&values)
            .into_iter()
            .map(|template| Value::Object(template.raw))
            .collect();
        templates.push(template);
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.update_setting("customViewTemplates", Value::Array(templates), cx)
        });
        if let Some(editor) = self.view_editor.as_mut() {
            editor.notice = Some("Template saved. Choose Add view to use it.".into());
        }
        cx.notify();
    }

    /// Add view: the template picker.
    pub(crate) fn start_choosing_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.choosing_template = true;
        if self.template_query.is_none() {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search templates"));
            let subscription = cx.subscribe_in(
                &input,
                window,
                |_: &mut Self, _, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                },
            );
            self.fields.subscriptions.push(subscription);
            self.template_query = Some(input);
        }
        if let Some(input) = self.template_query.clone() {
            input.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
        }
        cx.notify();
    }

    /// `ProjectViewTemplates`.
    pub(crate) fn render_templates(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.template_query.is_none() {
            self.start_choosing_template(window, cx);
        }
        let query_input = self.template_query.clone().expect("template query");
        let query = query_input.read(cx).value().to_lowercase();
        let values = self.store.read(cx).values();
        let personal = custom_view_templates(&values);
        let mut all = builtin_templates();
        all.extend(personal.iter().cloned());
        let hairline = hsla(p.hairline);
        let rows: Vec<AnyElement> = all
            .into_iter()
            .filter(|template| {
                format!("{} {}", template.name(), template.description())
                    .to_lowercase()
                    .contains(&query)
            })
            .map(|template| {
                let id = template.id();
                let is_personal = personal.iter().any(|candidate| candidate.id() == id);
                let remove = is_personal.then(|| {
                    let id = id.clone();
                    settings_square_button(
                        p,
                        SharedString::from(format!("template-{id}-remove")),
                        icon::TRASH,
                        None,
                        SizedButtonVariant::Ghost,
                        28.0,
                        None,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| {
                            let values = page.store.read(cx).values();
                            let templates: Vec<Value> = custom_view_templates(&values)
                                .into_iter()
                                .filter(|template| template.id() != id)
                                .map(|template| Value::Object(template.raw))
                                .collect();
                            let store = page.store.clone();
                            store.update(cx, |store, cx| {
                                store.update_setting(
                                    "customViewTemplates",
                                    Value::Array(templates),
                                    cx,
                                )
                            });
                        },
                        cx,
                    )
                });
                let use_template = {
                    let template = template.clone();
                    settings_sized_button(
                        p,
                        SharedString::from(format!("template-{id}-use")),
                        "Use template",
                        None,
                        None,
                        SizedButtonVariant::Outline,
                        SizedButtonSize::Default,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| page.use_template(&template, cx),
                        cx,
                    )
                };
                let controls = h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(remove)
                    .child(use_template)
                    .into_any_element();
                div()
                    .w_full()
                    .child(settings_list_item(
                        p,
                        None,
                        Some(
                            settings_icon("modals/settings/world.svg", 16.0, p.muted)
                                .into_any_element(),
                        ),
                        template.name(),
                        Some(div().child(template.description()).into_any_element()),
                        Some(controls),
                    ))
                    .into_any_element()
            })
            .collect();
        let focused = query_input.read(cx).focus_handle(cx).is_focused(window);
        let search = div()
            .w_full()
            .h(px(32.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(if focused { p.focus_border } else { p.hairline }))
            .bg(hsla(p.input_background()))
            .child(
                div().flex_1().min_w_0().child(
                    gpui_component::input::Input::new(&query_input)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .w_full()
                        .px(px(0.0))
                        .py(px(0.0))
                        .text_size(px(14.0))
                        .placeholder_color(hsla(p.muted))
                        .text_color(hsla(p.foreground)),
                ),
            );
        let scratch = settings_sized_button(
            p,
            "templates-scratch",
            "Start from scratch",
            Some(icon::PLUS),
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Default,
            false,
            None,
            |page: &mut Self, _window, cx| {
                let mut raw = Map::new();
                raw.insert("id".into(), json!(""));
                raw.insert("name".into(), json!(""));
                raw.insert("url".into(), json!(""));
                raw.insert("availability".into(), json!("all"));
                raw.insert(
                    "source".into(),
                    Value::Object(default_project_view_source()),
                );
                page.use_template(&CustomView { raw }, cx);
            },
            cx,
        );
        let cancel = settings_sized_button(
            p,
            "templates-cancel",
            "Cancel",
            None,
            None,
            SizedButtonVariant::Ghost,
            SizedButtonSize::Default,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.choosing_template = false;
                cx.notify();
            },
            cx,
        );
        v_flex()
            .w_full()
            .py(px(12.0))
            .gap(px(12.0))
            .child(search)
            .child(
                v_flex()
                    .w_full()
                    .children(rows.into_iter().enumerate().map(|(index, row)| {
                        div()
                            .w_full()
                            .when(index > 0, |this| this.border_t_1().border_color(hairline))
                            .child(div().w_full().mx(px(-4.0)).child(row))
                    })),
            )
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap(px(8.0))
                    .child(scratch)
                    .child(cancel),
            )
            .into_any_element()
    }

    /// `onSelect(template)`: a new editor from the template.
    fn use_template(&mut self, template: &CustomView, cx: &mut Context<Self>) {
        self.choosing_template = false;
        let mut draft = template.raw.clone();
        let template_id = template.id();
        draft.insert("id".into(), json!(""));
        draft.insert("enabled".into(), json!(true));
        draft.insert("projectBindings".into(), json!({}));
        draft.insert("templateId".into(), json!(template_id));
        self.view_editor = Some(ViewEditorState {
            draft,
            id: None,
            error: None,
            notice: None,
        });
        cx.notify();
    }
}
