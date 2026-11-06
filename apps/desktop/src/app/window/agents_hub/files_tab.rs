//! The four file tabs (Skills, MDs, Hooks, Configs & MCPs): the searchable grouped list on the
//! left and the selected file's editor on the right (`.agents-hub-layout` in
//! packages/core-ui/styles/agents-hub.css (deleted 2026-11-06)).
use super::super::native_modal_kit::{MODAL_MONO_FONT, hsla, rgba_of};
use super::editor::editor_body;
use super::model::*;
use super::palette::HubPalette;
use super::widgets::*;
use super::window::{AgentsHubModalCommand, GpuiAgentsHubModalWindow};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::scroll::Scrollbar;
use gpui_component::tooltip::Tooltip;
use gpui_component::{h_flex, v_flex};

/// `grid-template-columns: minmax(18rem, 23rem) minmax(0, 1fr)`: the list takes its 23rem cap
/// at every window size the Hub opens in.
pub(crate) const LIST_PANE_WIDTH: f32 = 368.0;

/// The two-pane frame every tab draws in.
pub(crate) fn hub_layout(hp: &HubPalette) -> gpui::Div {
    h_flex()
        .relative()
        .flex_1()
        .size_full()
        .min_w_0()
        .min_h_0()
        .items_stretch()
        .bg(hsla(hp.panel))
        .border_1()
        .border_color(hsla(hp.line))
        .rounded(px(12.0))
        .overflow_hidden()
}

impl GpuiAgentsHubModalWindow {
    pub(crate) fn render_files_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hp = self.hp;
        let slot = self.active_tab.file_slot().unwrap_or(0);
        let rows = self.render_group_rows(&hp, slot, self.groups(slot), cx);
        // The group rows are the scroll area's own children, so `list_scroll` can bring one of
        // them into view (a Hub opened on one skill).
        let list = div()
            .id("agents-hub-list")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.list_scroll);
        let list = match rows {
            Some(rows) => list.px(px(8.0)).pb(px(10.0)).children(rows),
            None => list.child(empty_note(
                &hp,
                if self.catalog.is_none() {
                    "Loading agent files..."
                } else {
                    "No matching files."
                },
            )),
        };
        let search = div()
            .flex_shrink_0()
            .w_full()
            .p(px(10.0))
            .child(hub_search_input(&hp, &self.search, window, cx));
        let list_pane = v_flex()
            .flex_shrink_0()
            .w(px(LIST_PANE_WIDTH))
            .h_full()
            .min_h_0()
            .border_r_1()
            .border_color(hsla(hp.line))
            .child(search)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(list)
                    .child(Scrollbar::vertical(&self.list_scroll)),
            );
        let right = self.render_file_pane(&hp, cx);
        hub_layout(&hp)
            .child(list_pane)
            .child(right)
            .into_any_element()
    }

    /// One row per group that matches the search, or `None` when none does.
    fn render_group_rows(
        &self,
        hp: &HubPalette,
        slot: usize,
        groups: &[AgentsHubGroup],
        cx: &mut Context<Self>,
    ) -> Option<Vec<AnyElement>> {
        let filtered = filtered_groups(groups, &self.query);
        if filtered.is_empty() {
            return None;
        }
        let active_file_id = self.selected_file_ids[slot].clone();
        Some(
            filtered
                .into_iter()
                .enumerate()
                .map(|(index, group)| self.render_group(hp, index, group, &active_file_id, cx))
                .collect(),
        )
    }

    /*
    CDXC:AgentLauncher 2026-08-24 (round 3):
    Every tab opens collapsed, and a collapsed group is one compact row in every expandable tab:
    only an explicit click expands a group, which then shows its file count, path, description,
    the profiles that use it and its files.
    */
    fn render_group(
        &self,
        hp: &HubPalette,
        index: usize,
        group: &AgentsHubGroup,
        active_file_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.expanded_ids.contains(&group.id);
        let active_group = group.files.iter().any(|file| file.id == active_file_id);
        let group_id = group.id.clone();
        let primary = group
            .files
            .first()
            .map(|file| file.id.clone())
            .unwrap_or_default();
        let hover = hp.wash(0.05);
        let count = group.files.len();
        let title_row = h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(px(6.0))
            .child(icon(
                if expanded {
                    ICON_CHEVRON_DOWN
                } else {
                    ICON_CHEVRON_RIGHT
                },
                14.0,
                if active_group {
                    hp.foreground
                } else {
                    hp.muted
                },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.0))
                    .line_height(px(16.9))
                    .text_color(hsla(hp.foreground))
                    .child(SharedString::from(group.name.clone())),
            )
            .when(group.variant_count > 1, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .px(px(6.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(hsla(hp.line))
                        .text_size(px(11.0))
                        .line_height(px(16.0))
                        .text_color(hsla(hp.muted))
                        .child(format!("{} versions", group.variant_count)),
                )
            })
            .when(expanded, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0))
                        .text_color(hsla(hp.muted))
                        .child(format!(
                            "{count} {}",
                            if count == 1 { "file" } else { "files" }
                        )),
                )
            });
        let main = v_flex()
            .id(("agents-hub-group-main", index))
            .w_full()
            .min_w_0()
            .gap(px(4.0))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.toggle_group(group_id.clone(), primary.clone(), window, cx);
            }))
            .child(title_row)
            .when(expanded, |this| {
                this.child(small_line(hp, &group.path, hp.muted))
                    .child(small_line(
                        hp,
                        &group.description,
                        rgba_of(hp.muted, hp.muted.a * 0.78),
                    ))
            });
        v_flex()
            .id(("agents-hub-group", index))
            .w_full()
            .min_w_0()
            .gap(px(6.0))
            .px(px(8.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .overflow_hidden()
            .when(active_group, |this| this.bg(hsla(hp.wash(0.09))))
            .when(!active_group, |this| {
                this.hover(move |this| this.bg(hsla(hover)))
            })
            .child(main)
            .when(expanded, |this| {
                this.child(self.render_profile_row(hp, index, &group.profiles, cx))
                    .child(self.render_file_list(hp, index, group, active_file_id, cx))
            })
            .into_any_element()
    }

    /// CDXC:AgentLauncher 2026-05-15-15:41:
    /// Profile chip tooltips keep the profile label, the instruction file path, the optional resolved target path and the folder-opening action as organized sections, so dense path content stays scannable.
    ///
    /// CDXC:AgentLauncher 2026-06-04-13:39:
    /// Filesystem actions use OS-agnostic "Open File/Folder Location" language.
    fn render_profile_row(
        &self,
        hp: &HubPalette,
        group_index: usize,
        profiles: &[AgentsHubProfile],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chips = profiles.iter().enumerate().map(|(index, profile)| {
            let badge = agent_profile_badge(&profile.profile_path);
            let path = profile.profile_path.clone();
            let tooltip_profile = profile.clone();
            let tip_hp = *hp;
            let rest = hp.wash(0.05);
            let hover = hp.wash(0.12);
            div()
                .id(("agents-hub-profile", group_index * 1000 + index))
                .relative()
                .flex_shrink_0()
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .border_1()
                .border_color(hsla(hp.line))
                .bg(hsla(rest))
                .hover(move |this| this.bg(hsla(hover)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    this.send(AgentsHubModalCommand::OpenPath { path: path.clone() }, cx);
                }))
                .tooltip(move |window, cx| {
                    let profile = tooltip_profile.clone();
                    Tooltip::element(move |_, _| profile_tooltip(&tip_hp, &profile))
                        .build(window, cx)
                })
                .children(agent_logo(&profile.agent_icon, 14.0, hp))
                .children(badge.map(|badge| {
                    div()
                        .absolute()
                        .right(px(-3.0))
                        .bottom(px(-3.0))
                        .size(px(12.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .border_1()
                        .border_color(hsla(hp.raised))
                        .bg(hsla(hp.foreground))
                        .text_color(hsla(hp.page))
                        .text_size(px(8.0))
                        .line_height(px(8.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(badge)
                }))
        });
        h_flex()
            .w_full()
            .flex_wrap()
            .gap(px(6.0))
            .children(chips)
            .into_any_element()
    }

    fn render_file_list(
        &self,
        hp: &HubPalette,
        group_index: usize,
        group: &AgentsHubGroup,
        active_file_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = group.files.iter().enumerate().map(|(index, file)| {
            let active = file.id == active_file_id;
            let file_id = file.id.clone();
            let hover_fill = hp.wash(0.06);
            let foreground = hp.foreground;
            h_flex()
                .id(("agents-hub-file", group_index * 1000 + index))
                .w_full()
                .min_w_0()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(5.0))
                .rounded(px(6.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(hsla(if active { hp.foreground } else { hp.muted }))
                .when(active, |this| this.bg(hsla(hp.wash(0.12))))
                .when(!active, |this| {
                    this.hover(move |this| this.bg(hsla(hover_fill)).text_color(hsla(foreground)))
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.select_file(file_id.clone(), window, cx);
                }))
                .child(icon(
                    ICON_FILE,
                    14.0,
                    if active { hp.foreground } else { hp.muted },
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(SharedString::from(file.name.clone())),
                )
        });
        // CDXC:AgentLauncher 2026-05-13-02:33
        // Nested file rows stay inside the card edge: the tree indent is taken from the width instead of overflowing under the divider.
        v_flex()
            .min_w_0()
            .ml(px(14.0))
            .pl(px(8.0))
            .gap(px(2.0))
            .border_l_1()
            .border_color(hsla(hp.line))
            .children(rows)
            .into_any_element()
    }

    fn render_file_pane(&self, hp: &HubPalette, cx: &mut Context<Self>) -> AnyElement {
        let frame = v_flex().flex_1().min_w_0().min_h_0().h_full();
        let Some(file) = self.active_file() else {
            return frame
                .child(empty_note(
                    hp,
                    if self.catalog.is_some() {
                        "No files found."
                    } else {
                        "Loading agent files..."
                    },
                ))
                .into_any_element();
        };
        let content = self.active_file_content(&file);
        let editor_ready = content.is_some()
            && self
                .editor
                .as_ref()
                .is_some_and(|editor| editor.path == file.path);
        if !editor_ready {
            let message = self
                .content_errors
                .get(&file.path)
                .cloned()
                .unwrap_or_else(|| "Loading file...".to_string());
            return frame
                .child(file_toolbar(hp, &file, None))
                .child(hairline(hp))
                .child(empty_note(hp, message))
                .into_any_element();
        }
        let actions = self.render_editor_actions(hp, &file, cx);
        let editor = self
            .editor
            .as_ref()
            .map(|editor| editor.state.clone())
            .expect("editor_ready");
        frame
            .child(file_toolbar(hp, &file, Some(actions)))
            .child(hairline(hp))
            .child(editor_body(hp, &editor))
            .into_any_element()
    }

    /// CDXC:AgentLauncher 2026-06-04-20:08:
    /// Editor toolbar actions are compact icon-only buttons with hover tooltips, and Refresh sits immediately before Save so disk changes can be reloaded without closing the Hub. Open File/Folder Location sits beside the built-in editor action so users can choose filesystem or in-app navigation.
    fn render_editor_actions(
        &self,
        hp: &HubPalette,
        file: &AgentsHubFile,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dirty = self.editor.as_ref().is_some_and(|editor| editor.dirty);
        let copied = self.path_copied();
        let copy_path = file.path.clone();
        let folder_path = file.path.clone();
        let editor_path = file.path.clone();
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.0))
            .child(toolbar_button(
                hp,
                "agents-hub-copy-path",
                if copied { ICON_CHECK } else { ICON_COPY },
                if copied {
                    "Path copied"
                } else {
                    "Copy file path"
                },
                false,
                move |this: &mut Self, _window, cx| this.copy_active_path(copy_path.clone(), cx),
                cx,
            ))
            .child(toolbar_button(
                hp,
                "agents-hub-open-folder",
                ICON_FOLDER_OPEN,
                "Open containing folder",
                false,
                move |this: &mut Self, _window, cx| {
                    this.send(
                        AgentsHubModalCommand::OpenPath {
                            path: folder_path.clone(),
                        },
                        cx,
                    )
                },
                cx,
            ))
            .child(toolbar_button(
                hp,
                "agents-hub-open-editor",
                ICON_EDIT,
                "Open in built-in editor",
                false,
                move |this: &mut Self, _window, cx| {
                    this.send(
                        AgentsHubModalCommand::OpenInBuiltInEditor {
                            file_path: editor_path.clone(),
                        },
                        cx,
                    )
                },
                cx,
            ))
            .child(toolbar_button(
                hp,
                "agents-hub-refresh",
                ICON_REFRESH,
                "Refresh contents from disk",
                false,
                |this: &mut Self, _window, cx| this.refresh_catalog(cx),
                cx,
            ))
            .child(toolbar_button(
                hp,
                "agents-hub-save",
                ICON_SAVE,
                if dirty {
                    "Save changes"
                } else {
                    "No changes to save"
                },
                !dirty,
                |this: &mut Self, _window, cx| this.save_active_file(cx),
                cx,
            ))
            .child(wrap_button(hp, self.word_wrap, cx))
            .into_any_element()
    }
}

/// `.agents-hub-path` / `.agents-hub-description`: one 11px line, ellipsized.
fn small_line(hp: &HubPalette, text: &str, color: gpui::Rgba) -> AnyElement {
    let _ = hp;
    div()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(11.0))
        .line_height(px(15.4))
        .text_color(hsla(color))
        .child(SharedString::from(text.to_string()))
        .into_any_element()
}

/// `.agents-hub-editor-toolbar`: the file name over its path, and the actions on the right.
fn file_toolbar(hp: &HubPalette, file: &AgentsHubFile, actions: Option<AnyElement>) -> AnyElement {
    h_flex()
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .px(px(12.0))
        .py(px(10.0))
        .min_h(px(53.3))
        .child(
            v_flex()
                .min_w_0()
                .gap(px(1.0))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(13.0))
                        .line_height(px(16.9))
                        .text_color(hsla(hp.foreground))
                        .child(SharedString::from(file.name.clone())),
                )
                .child(small_line(hp, &file.path, hp.muted)),
        )
        .children(actions)
        .into_any_element()
}

/// `.agents-hub-editor-action-button`: a 32px outline icon button on the pane (8px radius, the
/// Hub hairline, muted glyph), the raised-hover fill on hover, 40% opacity when disabled, and
/// its label as a tooltip even while disabled.
#[allow(clippy::too_many_arguments)]
fn toolbar_button(
    hp: &HubPalette,
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
    disabled: bool,
    on_click: impl Fn(
        &mut GpuiAgentsHubModalWindow,
        &mut Window,
        &mut Context<GpuiAgentsHubModalWindow>,
    ) + 'static,
    cx: &mut Context<GpuiAgentsHubModalWindow>,
) -> AnyElement {
    let hover = hp.raised_hover;
    div()
        .id(id)
        .flex_shrink_0()
        .size(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(hsla(hp.line))
        .when(disabled, |this| this.opacity(0.4))
        .when(!disabled, |this| {
            this.hover(move |this| this.bg(hsla(hover)))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_click(this, window, cx);
                }))
        })
        .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
        .child(icon(glyph, 16.0, hp.muted))
        .into_any_element()
}

/// The wrap toggle: [`toolbar_button`]'s shape with the raised fill while wrapping is on, and the
/// Files view's icon and tooltips.
fn wrap_button(
    hp: &HubPalette,
    word_wrap: bool,
    cx: &mut Context<GpuiAgentsHubModalWindow>,
) -> AnyElement {
    let hover = hp.raised_hover;
    div()
        .id("agents-hub-word-wrap")
        .flex_shrink_0()
        .size(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(hsla(hp.line))
        .when(word_wrap, |this| this.bg(hsla(hover)))
        .hover(move |this| this.bg(hsla(hover)))
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.toggle_word_wrap(window, cx);
        }))
        .tooltip(move |window, cx| {
            Tooltip::new(if word_wrap {
                "Turn off line wrapping"
            } else {
                "Wrap long lines"
            })
            .build(window, cx)
        })
        .child(icon(
            ICON_TEXT_WRAP,
            16.0,
            if word_wrap { hp.foreground } else { hp.muted },
        ))
        .into_any_element()
}

fn profile_tooltip(hp: &HubPalette, profile: &AgentsHubProfile) -> AnyElement {
    let (title, text, arrow, rule) = if hp.light {
        (
            gpui::rgb(0x262626),
            rgba_of(gpui::rgb(0x262626), 0.76),
            rgba_of(gpui::rgb(0x262626), 0.55),
            super::super::native_modal_kit::modal_rgba(0x000000, 0.08),
        )
    } else {
        (
            gpui::rgb(0xffffff),
            rgba_of(gpui::rgb(0xffffff), 0.76),
            rgba_of(gpui::rgb(0xffffff), 0.55),
            super::super::native_modal_kit::modal_rgba(0xffffff, 0.08),
        )
    };
    let path_line = |value: &str, color: gpui::Rgba| {
        div()
            .min_w_0()
            .font_family(MODAL_MONO_FONT)
            .text_size(px(11.0))
            .line_height(px(15.4))
            .text_color(hsla(color))
            .child(SharedString::from(value.to_string()))
    };
    v_flex()
        .min_w(px(280.0))
        .max_w(px(480.0))
        .gap(px(10.0))
        .child(
            v_flex()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(16.9))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(title))
                        .child(SharedString::from(profile.label.clone())),
                )
                .child(path_line(&profile.file_path, text)),
        )
        .children(profile.target_path.as_ref().map(|target| {
            h_flex()
                .items_start()
                .gap(px(8.0))
                .pt(px(10.0))
                .border_t_1()
                .border_color(hsla(rule))
                .child(path_line("->", arrow).flex_shrink_0())
                .child(path_line(target, text).flex_1())
        }))
        .child(
            div()
                .pt(px(10.0))
                .border_t_1()
                .border_color(hsla(rule))
                .text_size(px(11.0))
                .line_height(px(14.3))
                .text_color(hsla(text))
                .child("Click to open file/folder location"),
        )
        .into_any_element()
}
