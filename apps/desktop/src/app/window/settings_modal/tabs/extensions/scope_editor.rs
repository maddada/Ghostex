//! `ViewScopeEditor` and `ScopeMultiSelect` (settings-modal/project-views/scope-editor.tsx (deleted 2026-10-01),
//! scope-multi-select.tsx (deleted 2026-10-01) and scope-editor.css): "Show <view> [Everywhere | Only in selected
//! places]", then "Except in" / "Show in" with the picked spaces and projects, then "But keep in" /
//! "But not in" once a space is picked, and a sentence stating the result, with Reset, Cancel and
//! Save. Picks are chips in a searchable multi-select.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    DropdownAlign, DropdownRow, DropdownState, SizedButtonSize, SizedButtonVariant, icon,
    searchable_dropdown, settings_icon, settings_sized_button, stock_segmented, toggle_dropdown,
};
use super::super::super::palette::SettingsPalette;
use super::data::{
    ScopeProject, ScopeSpace, ViewScope, parse_view_scope_space_key, scope_projects_and_spaces,
    set_view_scope,
};
use super::{ExtensionsTab, ScopeEditorState};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::{h_flex, v_flex};

/// `ScopeOption`.
#[derive(Clone, Debug)]
pub(crate) struct ScopeOption {
    /// `space:<key>` or `project:<id>`.
    pub(crate) id: String,
    pub(crate) is_space: bool,
    pub(crate) label: String,
    pub(crate) hint: Option<String>,
}

fn space_option_id(key: &str) -> String {
    format!("space:{key}")
}

fn project_option_id(id: &str) -> String {
    format!("project:{id}")
}

/// `scopeSelection(scope)`: the picks that differ from the Default, and the projects kept at it.
pub(crate) struct Selection {
    pub(crate) keep: Vec<String>,
    pub(crate) targets: Vec<String>,
}

pub(crate) fn scope_selection(scope: &ViewScope) -> Selection {
    let opposite = !scope.default_shown;
    Selection {
        keep: scope
            .projects
            .iter()
            .filter(|(_, shown)| *shown == scope.default_shown)
            .map(|(id, _)| project_option_id(id))
            .collect(),
        targets: scope
            .spaces
            .iter()
            .filter(|(_, shown)| *shown == opposite)
            .map(|(key, _)| space_option_id(key))
            .chain(
                scope
                    .projects
                    .iter()
                    .filter(|(_, shown)| *shown == opposite)
                    .map(|(id, _)| project_option_id(id)),
            )
            .collect(),
    }
}

/// `withSelection(scope, optionId, state)`: `None` is `'inherit'`.
fn with_selection(scope: &ViewScope, option_id: &str, state: Option<bool>) -> ViewScope {
    let mut next = scope.clone();
    let (overrides, key) = if let Some(key) = option_id.strip_prefix("space:") {
        (&mut next.spaces, key)
    } else {
        (
            &mut next.projects,
            option_id.strip_prefix("project:").unwrap_or(option_id),
        )
    };
    match state {
        None => overrides.retain(|(existing, _)| existing != key),
        Some(shown) => {
            if let Some(existing) = overrides.iter_mut().find(|(existing, _)| existing == key) {
                existing.1 = shown;
            } else {
                overrides.push((key.to_string(), shown));
            }
        }
    }
    next
}

/// `withFlippedDefault(scope)`: the same picks mean the opposite.
fn with_flipped_default(scope: &ViewScope) -> ViewScope {
    ViewScope {
        default_shown: !scope.default_shown,
        projects: scope
            .projects
            .iter()
            .map(|(key, shown)| (key.clone(), !shown))
            .collect(),
        spaces: scope
            .spaces
            .iter()
            .filter(|(_, shown)| *shown != scope.default_shown)
            .map(|(key, shown)| (key.clone(), !shown))
            .collect(),
    }
}

/// `listNames`: "a, b and c".
fn list_names(names: &[String]) -> String {
    match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        count => format!("{} and {}", names[..count - 1].join(", "), names[count - 1]),
    }
}

/// `scopeSentence`.
fn scope_sentence(
    title: &str,
    scope: &ViewScope,
    selection: &Selection,
    label_for: impl Fn(&str) -> String,
) -> String {
    let names =
        |ids: &[String]| list_names(&ids.iter().map(|id| label_for(id)).collect::<Vec<_>>());
    let keep = if selection.keep.is_empty() {
        String::new()
    } else {
        names(&selection.keep)
    };
    if scope.default_shown {
        if selection.targets.is_empty() {
            return format!("{title} shows in every project.");
        }
        return format!(
            "{title} shows everywhere except {}{}.",
            names(&selection.targets),
            if keep.is_empty() {
                String::new()
            } else {
                format!(", but stays in {keep}")
            }
        );
    }
    if selection.targets.is_empty() {
        return format!("{title} is hidden everywhere. Pick where it should show.");
    }
    format!(
        "{title} shows only in {}{}.",
        names(&selection.targets),
        if keep.is_empty() {
            String::new()
        } else {
            format!(", but not in {keep}")
        }
    )
}

/// The editor's options: spaces, then projects, each with the stored entries that no longer
/// exist kept at the end of their kind.
fn scope_options(
    scope: &ViewScope,
    projects: &[ScopeProject],
    spaces: &[ScopeSpace],
) -> (Vec<ScopeOption>, Vec<ScopeOption>) {
    let mut space_options: Vec<ScopeOption> = spaces
        .iter()
        .map(|space| ScopeOption {
            id: space_option_id(&space.key()),
            is_space: true,
            label: space.name.clone(),
            hint: None,
        })
        .collect();
    for (key, _) in &scope.spaces {
        // With Spaces off the HUD lists no Space, and a saved Space rule is kept
        // but not offered (settings-catalog spaces.rs).
        if spaces.is_empty()
            || spaces.iter().any(|space| space.key() == *key)
            || parse_view_scope_space_key(key).is_none()
        {
            continue;
        }
        space_options.push(ScopeOption {
            id: space_option_id(key),
            is_space: true,
            label: "Unavailable space".into(),
            hint: Some("No longer in the sidebar".into()),
        });
    }
    let mut project_options: Vec<ScopeOption> = projects
        .iter()
        .map(|project| ScopeOption {
            id: project_option_id(&project.project_id),
            is_space: false,
            label: project.name.clone(),
            hint: (!project.path.is_empty()).then(|| project.path.clone()),
        })
        .collect();
    for (id, _) in &scope.projects {
        if projects.iter().any(|project| project.project_id == *id) {
            continue;
        }
        project_options.push(ScopeOption {
            id: project_option_id(id),
            is_space: false,
            label: "Unavailable project".into(),
            hint: Some("No longer in the sidebar".into()),
        });
    }
    (space_options, project_options)
}

fn scope_targets(page: &mut ExtensionsTab) -> &mut DropdownState {
    &mut page.scope_targets
}

fn scope_keep(page: &mut ExtensionsTab) -> &mut DropdownState {
    &mut page.scope_keep
}

impl ExtensionsTab {
    fn update_scope_draft(
        &mut self,
        cx: &mut Context<Self>,
        apply: impl FnOnce(&ViewScope) -> ViewScope,
    ) {
        if let Some(editor) = self.scope_editor.as_mut() {
            editor.draft = apply(&editor.draft);
            cx.notify();
        }
    }

    /// Opens the "Except in" / "Show in" picker (`targets`) or the "But keep in" one.
    pub(crate) fn toggle_scope_dropdown(
        &mut self,
        targets: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if targets {
            self.scope_keep.open = false;
            toggle_dropdown(
                self,
                scope_targets,
                "Search projects and spaces",
                Some(0),
                window,
                cx,
            );
        } else {
            self.scope_targets.open = false;
            toggle_dropdown(self, scope_keep, "Search projects", Some(0), window, cx);
        }
    }

    /// `renderEditor(key)`: the scope editor under the card it belongs to.
    pub(crate) fn render_scope_editor_for(
        &mut self,
        p: &SettingsPalette,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self
            .scope_editor
            .clone()
            .filter(|editor| editor.key == key)?;
        let center = self.center_scope_editor.clone();
        let body = self.render_scope_editor(p, &editor, window, cx);
        let handle = self.store.update(cx, |store, _| {
            store.scroll_handle(super::super::super::model::SettingsTabId::Extensions)
        });
        Some(
            div()
                .w_full()
                .on_children_prepainted(move |bounds, window, _cx| {
                    if !center.get() {
                        return;
                    }
                    let Some(editor) = bounds.first() else {
                        return;
                    };
                    center.set(false);
                    // `scrollIntoView({ block: 'center' })`.
                    let viewport = handle.bounds();
                    let offset = handle.offset();
                    let content_top = editor.origin.y - viewport.origin.y - offset.y;
                    let target = content_top - (viewport.size.height - editor.size.height) / 2.0;
                    let target = target.max(px(0.0)).min(handle.max_offset().y);
                    handle.set_offset(gpui::point(offset.x, -target));
                    window.refresh();
                })
                .child(body)
                .into_any_element(),
        )
    }

    /// CDXC:Extensions 2026-09-24 DECISION:
    /// User (ruling 3A, 2026-09-20): a view's scope is a Default of shown or hidden plus per-project and per-space overrides. User (2026-09-24): replace the grids of toggle cards with a multi-select dropdown and "make the UX for this part great". The editor reads as one rule: "Show <view> [Everywhere | Only in selected places]", then "Except in" / "Show in" with the picked spaces and projects (the overrides that differ from the Default), then, once a space is picked, "But keep in" / "But not in" for projects that override their space, and a plain sentence stating the result.
    fn render_scope_editor(
        &mut self,
        p: &SettingsPalette,
        editor: &ScopeEditorState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scope = &editor.draft;
        let title = editor.title.clone();
        let everywhere = scope.default_shown;
        let mut selection = scope_selection(scope);
        let (projects, spaces) = scope_projects_and_spaces(self.store.read(cx).hud());
        if spaces.is_empty() {
            selection.targets.retain(|id| !id.starts_with("space:"));
        }
        let (space_options, project_options) = scope_options(scope, &projects, &spaces);
        let all_options: Vec<ScopeOption> = space_options
            .iter()
            .chain(project_options.iter())
            .cloned()
            .collect();
        let label_for = {
            let all = all_options.clone();
            move |id: &str| -> String {
                match all.iter().find(|option| option.id == id) {
                    Some(option) if option.is_space => format!("the {} space", option.label),
                    Some(option) => option.label.clone(),
                    None => id.to_string(),
                }
            }
        };
        let show_keep = selection.targets.iter().any(|id| id.starts_with("space:"))
            || !selection.keep.is_empty();
        let is_default = scope.is_default();
        let reset = settings_sized_button(
            p,
            "scope-editor-reset",
            "Reset",
            Some("modals/settings/arrow-back-up.svg"),
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Xs,
            is_default,
            None,
            |page: &mut Self, _window, cx| {
                page.update_scope_draft(cx, |_| ViewScope::default());
            },
            cx,
        );
        let mode = stock_segmented(
            p,
            "scope-editor-mode",
            &[
                ("shown".to_string(), "Everywhere".to_string()),
                ("hidden".to_string(), "Only in selected places".to_string()),
            ],
            Some(if everywhere { "shown" } else { "hidden" }),
            false,
            false,
            |page: &mut Self, next, _window, cx| {
                page.update_scope_draft(cx, |draft| {
                    if (next == "shown") == draft.default_shown {
                        draft.clone()
                    } else {
                        with_flipped_default(draft)
                    }
                });
            },
            cx,
        );
        let targets_field = if all_options.is_empty() {
            div()
                .text_size(px(13.0))
                .line_height(px(18.57))
                .text_color(hsla(p.muted))
                .child("No projects or spaces in the sidebar yet.")
                .into_any_element()
        } else {
            self.render_multi_select(
                p,
                true,
                &all_options,
                &selection.targets,
                if everywhere {
                    "Choose projects or spaces to hide it in"
                } else {
                    "Choose projects or spaces"
                },
                window,
                cx,
            )
        };
        let mut rows: Vec<AnyElement> = vec![
            rule_row(p, None, format!("Show {title}"), false, mode),
            rule_row(
                p,
                None,
                if everywhere { "Except in" } else { "Show in" }.to_string(),
                false,
                targets_field,
            ),
        ];
        if show_keep {
            let keep_options: Vec<ScopeOption> = project_options
                .iter()
                .filter(|option| {
                    !selection.targets.contains(&option.id) || selection.keep.contains(&option.id)
                })
                .cloned()
                .collect();
            let keep_field = self.render_multi_select(
                p,
                false,
                &keep_options,
                &selection.keep,
                if everywhere {
                    "Optional: projects that still show it"
                } else {
                    "Optional: projects that still hide it"
                },
                window,
                cx,
            );
            rows.push(rule_row(
                p,
                Some("modals/settings/corner-down-right.svg"),
                if everywhere {
                    "But keep in"
                } else {
                    "But not in"
                }
                .to_string(),
                true,
                keep_field,
            ));
        }
        let sentence = scope_sentence(&title, scope, &selection, &label_for);
        let cancel = settings_sized_button(
            p,
            "scope-editor-cancel",
            "Cancel",
            Some(icon::X),
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Default,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.scope_editor = None;
                page.scope_targets.open = false;
                page.scope_keep.open = false;
                cx.notify();
            },
            cx,
        );
        let save = settings_sized_button(
            p,
            "scope-editor-save",
            "Save",
            Some(super::super::super::fields::CHECK_ICON),
            None,
            SizedButtonVariant::Default,
            SizedButtonSize::Default,
            false,
            None,
            |page: &mut Self, _window, cx| page.save_scope(cx),
            cx,
        );
        v_flex()
            .id("view-scope-editor")
            .w_full()
            .gap(px(8.0))
            .px(px(16.0))
            .py(px(12.0))
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            .border_color(hsla(p.ring))
            .bg(hsla(p.raised))
            .child(
                h_flex()
                    .w_full()
                    .mb(px(4.0))
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child(format!("Where {title} is shown")),
                    )
                    .child(reset),
            )
            .children(rows)
            .child(
                h_flex()
                    .w_full()
                    .mt(px(4.0))
                    .pt(px(10.0))
                    .border_t_1()
                    .border_color(hsla(p.hairline))
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w(px(224.0))
                            .items_start()
                            .gap(px(6.0))
                            .text_size(px(13.0))
                            .line_height(px(18.2))
                            .text_color(hsla(p.foreground_alpha(0.8)))
                            .child(div().mt(px(2.0)).flex_shrink_0().child(settings_icon(
                                icon::INFO_CIRCLE,
                                14.0,
                                p.muted,
                            )))
                            .child(div().flex_1().min_w_0().child(sentence)),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .ml_auto()
                            .gap(px(8.0))
                            .child(cancel)
                            .child(save),
                    ),
            )
            .into_any_element()
    }

    /// Saves the draft (`setGhostexViewScope`) and closes the editor.
    fn save_scope(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.scope_editor.take() else {
            return;
        };
        self.scope_targets.open = false;
        self.scope_keep.open = false;
        let scopes = self.store.read(cx).values().value("viewScopes");
        let next = set_view_scope(&scopes, &editor.key, &editor.draft);
        let store = self.store.clone();
        store.update(cx, |store, cx| store.update_setting("viewScopes", next, cx));
        cx.notify();
    }

    /// CDXC:Extensions 2026-09-24 DECISION:
    /// User: "I really hate seeing all the toggles like this, please make it just a dropdown with multi select." Projects and spaces are picked from one searchable multi-select: the picks show as chips with a remove button, and the list shows spaces first, then projects, with a check on each pick. It replaces the 2026-09-18 toggle cards (three to a row, one switch per project and per space).
    /// `ScopeMultiSelect`: the chips, the trigger and, while open, the searchable list.
    #[allow(clippy::too_many_arguments)]
    fn render_multi_select(
        &mut self,
        p: &SettingsPalette,
        targets: bool,
        options: &[ScopeOption],
        selected: &[String],
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = if targets {
            self.scope_targets.open
        } else {
            self.scope_keep.open
        };
        let chips: Vec<ScopeOption> = selected
            .iter()
            .filter_map(|id| options.iter().find(|option| option.id == *id).cloned())
            .collect();
        let chip_elements = chips.iter().map(|option| {
            let id = option.id.clone();
            let chip_fill = p.foreground_alpha(0.09);
            let chip_border = p.foreground_alpha(0.10);
            let remove_hover = p.foreground_alpha(0.12);
            h_flex()
                .flex_shrink_0()
                .max_w(px(224.0))
                .h(px(24.0))
                .pl(px(7.0))
                .pr(px(2.0))
                .gap(px(5.0))
                .items_center()
                .rounded(px(6.0))
                .border_1()
                .border_color(hsla(chip_border))
                .bg(hsla(chip_fill))
                .text_size(px(13.0))
                .text_color(hsla(p.foreground))
                .child(settings_icon(
                    if option.is_space {
                        "modals/settings/stack-2.svg"
                    } else {
                        "modals/settings/folder.svg"
                    },
                    14.0,
                    p.muted,
                ))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(option.label.clone()),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("scope-chip-remove-{id}")))
                        .flex_shrink_0()
                        .size(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(move |this| this.bg(hsla(remove_hover)))
                        .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| {
                            page.toggle_scope_option(targets, &id, cx);
                        }))
                        .child(settings_icon(icon::X, 12.0, p.muted)),
                )
                .into_any_element()
        });
        let trigger_bounds = if targets {
            self.scope_targets.trigger_bounds.clone()
        } else {
            self.scope_keep.trigger_bounds.clone()
        };
        let trigger = h_flex()
            .id(if targets {
                "scope-targets-trigger"
            } else {
                "scope-keep-trigger"
            })
            .flex_1()
            .min_w(px(96.0))
            .h(px(24.0))
            .pl(px(6.0))
            .pr(px(4.0))
            .gap(px(8.0))
            .items_center()
            .justify_between()
            .text_size(px(13.0))
            .line_height(px(18.57))
            .text_color(hsla(p.muted))
            .cursor_pointer()
            .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| {
                page.toggle_scope_dropdown(targets, window, cx);
            }))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(if chips.is_empty() {
                        placeholder
                    } else {
                        "Add…"
                    }),
            )
            .child(settings_icon(icon::SELECTOR, 14.0, p.muted).flex_shrink_0());
        let hover_border = p.foreground_alpha(0.22);
        let ring = p.ring;
        let field = h_flex()
            .on_children_prepainted(move |bounds, _window, _cx| {
                // The popover anchors to the trigger, the field's last child.
                trigger_bounds.set(bounds.last().copied());
            })
            .id(if targets {
                "scope-targets-field"
            } else {
                "scope-keep-field"
            })
            .w_full()
            .min_w_0()
            .min_h(px(32.0))
            .p(px(3.0))
            .gap(px(4.0))
            .flex_wrap()
            .items_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(if open { ring } else { p.hairline }))
            // Under glass the field is the page's input wash, never the theme chrome (near-black).
            .bg(hsla(if p.glass {
                p.input_background()
            } else {
                p.surface
            }))
            .when(open, |this| {
                this.shadow(vec![gpui::BoxShadow {
                    color: hsla(css_fade(ring, 0.2)),
                    offset: gpui::point(px(0.0), px(0.0)),
                    blur_radius: px(0.0),
                    spread_radius: px(3.0),
                    inset: false,
                }])
            })
            .when(!open, |this| {
                this.hover(move |this| this.border_color(hsla(hover_border)))
            })
            .children(chip_elements)
            .child(trigger);
        let menu = self.render_scope_menu(p, targets, options, selected, window, cx);
        div()
            .w_full()
            .min_w_0()
            .child(field)
            .children(menu)
            .into_any_element()
    }

    fn toggle_scope_option(&mut self, targets: bool, id: &str, cx: &mut Context<Self>) {
        let id = id.to_string();
        self.update_scope_draft(cx, move |draft| {
            let selection = scope_selection(draft);
            if targets {
                let state = if selection.targets.contains(&id) {
                    None
                } else {
                    Some(!draft.default_shown)
                };
                with_selection(draft, &id, state)
            } else {
                let state = if selection.keep.contains(&id) {
                    None
                } else {
                    Some(draft.default_shown)
                };
                with_selection(draft, &id, state)
            }
        });
    }

    /// The multi-select's dropdown: Spaces then Projects, a check on each pick, and the
    /// "N selected" footer with Clear.
    #[allow(clippy::too_many_arguments)]
    fn render_scope_menu(
        &mut self,
        p: &SettingsPalette,
        targets: bool,
        options: &[ScopeOption],
        selected: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = if targets {
            &self.scope_targets
        } else {
            &self.scope_keep
        };
        if !state.open {
            return None;
        }
        let query = state.query(cx).trim().to_lowercase();
        let shown: Vec<ScopeOption> = options
            .iter()
            .filter(|option| {
                format!(
                    "{} {}",
                    option.label,
                    option.hint.clone().unwrap_or_default()
                )
                .to_lowercase()
                .contains(&query)
            })
            .cloned()
            .collect();
        let mut ordered: Vec<ScopeOption> = shown
            .iter()
            .filter(|option| option.is_space)
            .cloned()
            .collect();
        ordered.extend(shown.iter().filter(|option| !option.is_space).cloned());
        let rows: Vec<DropdownRow> = ordered
            .iter()
            .map(|option| DropdownRow {
                label: option.label.clone().into(),
                hint: option.hint.clone().map(Into::into),
                icon: Some(if option.is_space {
                    "modals/settings/stack-2.svg"
                } else {
                    "modals/settings/folder.svg"
                }),
                checked: selected.contains(&option.id),
                group: Some(
                    if option.is_space {
                        "Spaces"
                    } else {
                        "Projects"
                    }
                    .into(),
                ),
            })
            .collect();
        let count = selected.len();
        let selected_ids = selected.to_vec();
        let clear_hover = p.foreground_alpha(0.08);
        let footer = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .child(if count > 0 {
                format!("{count} selected")
            } else {
                "Nothing selected".to_string()
            })
            .when(count > 0, |this| {
                this.child(
                    div()
                        .id("scope-menu-clear")
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .text_size(px(12.0))
                        .text_color(hsla(p.foreground))
                        .cursor_pointer()
                        .hover(move |this| this.bg(hsla(clear_hover)))
                        .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| {
                            let ids = selected_ids.clone();
                            page.update_scope_draft(cx, move |draft| {
                                ids.iter().fold(draft.clone(), |draft, id| {
                                    with_selection(&draft, id, None)
                                })
                            });
                        }))
                        .child("Clear"),
                )
            })
            .into_any_element();
        let ids: Vec<String> = ordered.iter().map(|option| option.id.clone()).collect();
        let state = if targets {
            &self.scope_targets
        } else {
            &self.scope_keep
        };
        searchable_dropdown(
            p,
            if targets {
                "scope-targets-menu"
            } else {
                "scope-keep-menu"
            },
            state,
            if targets { scope_targets } else { scope_keep },
            &rows,
            "No projects or spaces match.",
            DropdownAlign::Start,
            Some(380.0_f32.min(f32::from(window.viewport_size().width) - 32.0)),
            true,
            Some(footer),
            move |page: &mut Self, index, _window, cx| {
                if let Some(id) = ids.get(index) {
                    page.toggle_scope_option(targets, id, cx);
                }
            },
            window,
            cx,
        )
    }
}

/// `.view-scope-rule`: an 8rem label column and the control.
fn rule_row(
    p: &SettingsPalette,
    glyph: Option<&'static str>,
    label: String,
    keep: bool,
    control: AnyElement,
) -> AnyElement {
    let color = if keep {
        p.muted
    } else {
        p.foreground_alpha(0.8)
    };
    h_flex()
        .w_full()
        .items_center()
        .gap(px(12.0))
        .child(
            h_flex()
                .w(px(128.0))
                .flex_shrink_0()
                .min_w_0()
                .items_center()
                .gap(px(5.0))
                .text_size(px(13.0))
                .line_height(px(18.57))
                .text_color(hsla(color))
                .children(glyph.map(|glyph| settings_icon(glyph, 14.0, color)))
                .child(label),
        )
        .child(div().flex_1().min_w_0().flex().child(control))
        .into_any_element()
}
