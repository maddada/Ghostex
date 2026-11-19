//! The slashed computer after the name of a project whose folder is missing, and the popover it
//! opens under the header: what broke and the fixes for that one project.
//!
//! CDXC:Projects 2026-11-20 DECISION: User picked (Q4244) "the machine icon but with a slash
//! through it", "should be only on the project header, not on the actual sessions", fixed "folder
//! by folder by clicking on that icon", and never a pop-up. The mark is amber, kept apart from the
//! soft red a computer's icon turns while that computer is offline (computer_mark.rs), and grey
//! while the folder's drive is away. Its popover hangs under the header inside the sidebar (drawn
//! deferred, over the rows below, like the sidebar's menus): the cause and paths gx-core words
//! (`FolderProblemView`), then Use <the folder found>, Locate Folder… (this computer's projects:
//! it needs this computer's folder picker), Ask AI to fix and Remove project, which asks once. A
//! fix says so in the popover, which closes about 3 seconds later; the project's next snapshot has
//! no mark. A click outside or on the mark again closes it, and nothing opens it by itself.
//! SEE-ALSO: packages/gx-core/src/sidebar_view/folder_problem.rs (the words),
//! server/src/project_folder_problem.rs (the diagnosis), gx_store/create/folder_pick.rs (Locate's
//! relocation, which reports back here).

use super::{appearance::SidebarAppearance, model::NativeSidebarGroup};
use crate::app::gx_store::gx_rpc;
use crate::app::window::native_modal_kit::MODAL_MONO_FONT;
use crate::{GhostexGpuiApp, app::helpers::*};
use ghostex_gx_core::{FolderProblemView, ProjectKey};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;
use std::time::Duration;
use web_time::Instant;

/// The slashed laptop, the user's "machine icon but with a slash through it".
pub(crate) const FOLDER_MARK_ICON: &str = "titlebar/device-laptop-off.svg";
/// Locate Folder… (this computer's folder picker) and Ask AI to fix (the Help chat) are the
/// desktop's; the web build offers Use and Remove project.
const DESKTOP_FIXES: bool = cfg!(not(ghostex_web));
/// How long the popover shows a fix before it closes.
const DONE_FOR: Duration = Duration::from_secs(3);
/// A click on the mark right after a click outside closed the popover is that same click.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

/// The open popover: which project's, and what it shows besides the fixes.
#[derive(Clone, Debug, Default)]
pub(crate) struct FolderPopover {
    pub(crate) group_id: String,
    /// A fix is running.
    pub(crate) busy: bool,
    /// The fix that worked: the two lines the popover shows until it closes.
    pub(crate) done: Option<(String, String)>,
    pub(crate) error: Option<String>,
    /// Remove project was clicked once and asks again.
    pub(crate) confirm_remove: bool,
}

/// The mark's amber, apart from an offline computer's soft red (0xff9494).
fn mark_amber(appearance: &SidebarAppearance) -> Hsla {
    if appearance.light {
        rgb(0xb8650a).into()
    } else {
        rgb(0xe8a33d).into()
    }
}

fn popover_fill(appearance: &SidebarAppearance) -> Hsla {
    if appearance.light {
        rgb(0xffffff).into()
    } else {
        titlebar_background().blend(appearance.foreground.opacity(0.07))
    }
}

fn danger(appearance: &SidebarAppearance) -> Hsla {
    if appearance.light {
        rgb(0xb91c1c).into()
    } else {
        rgb(0xf87171).into()
    }
}

/// What Ask AI to fix stages in the Help chat.
fn ask_ai_prompt(title: &str, problem: &FolderProblemView) -> String {
    let mut prompt = format!(
        "Fix my missing project folder. The Ghostex project \"{}\" can't find its folder {}. Ghostex says: {} {}",
        title.trim(),
        problem.folder,
        problem.title,
        problem.line
    );
    for path in &problem.paths {
        prompt.push_str(&format!("\n- {}: {}", path.label, path.path));
    }
    prompt.push_str(
        "\nFind out what happened and fix it. Ask me before you change anything on disk or in Ghostex.",
    );
    prompt
}

impl GhostexGpuiApp {
    fn folder_popover_open_for(&self, group_id: &str) -> bool {
        self.native_sidebar
            .folder_popover
            .as_ref()
            .is_some_and(|popover| popover.group_id == group_id)
    }

    fn close_folder_popover(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(popover) = self.native_sidebar.folder_popover.take() {
            self.native_sidebar.folder_popover_closed = Some((popover.group_id, Instant::now()));
            cx.notify();
        }
    }

    fn toggle_folder_popover(&mut self, group_id: &str, cx: &mut gpui::Context<Self>) {
        if self.folder_popover_open_for(group_id) {
            self.close_folder_popover(cx);
            return;
        }
        let just_closed = self
            .native_sidebar
            .folder_popover_closed
            .as_ref()
            .is_some_and(|(closed, at)| closed == group_id && at.elapsed() < REOPEN_GUARD);
        if just_closed {
            return;
        }
        self.native_sidebar.folder_popover = Some(FolderPopover {
            group_id: group_id.to_string(),
            ..FolderPopover::default()
        });
        cx.notify();
    }

    /// This computer's project `project_id` wears the mark.
    #[cfg_attr(ghostex_web, allow(dead_code))]
    pub(crate) fn local_project_wears_folder_mark(&self, project_id: &str) -> bool {
        let group_id = ProjectKey::local(project_id).to_sidebar_group_id();
        self.native_sidebar
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot
                    .groups
                    .iter()
                    .any(|group| group.group_id == group_id && group.folder_problem.is_some())
            })
    }

    /// Opens the popover of the project `project_id` (this computer's) under its header. It shows
    /// once the header wears the mark, which a refused create's fresh snapshot brings when this
    /// computer had not seen the folder go yet. New Terminal and New Agent in such a project land
    /// here instead of a dialog.
    pub(crate) fn open_folder_popover_for_local_project(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let group_id = ProjectKey::local(project_id).to_sidebar_group_id();
        if !self.folder_popover_open_for(&group_id) {
            self.native_sidebar.folder_popover = Some(FolderPopover {
                group_id,
                ..FolderPopover::default()
            });
            cx.notify();
        }
        true
    }

    /// Locate Folder… or Use moved `project` (this computer's or a machine's) to `path`: the open
    /// popover says so and closes a moment later. Returns whether a popover took the answer.
    pub(crate) fn folder_popover_relocated(
        &mut self,
        project: &ProjectKey,
        path: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let group_id = project.to_sidebar_group_id();
        let title = self
            .native_sidebar
            .snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .groups
                    .iter()
                    .find(|group| group.group_id == group_id)
            })
            .map(|group| group.title.clone())
            .unwrap_or_default();
        let Some(popover) = self
            .native_sidebar
            .folder_popover
            .as_mut()
            .filter(|popover| popover.group_id == group_id)
        else {
            return false;
        };
        popover.busy = false;
        popover.error = None;
        popover.done = Some(FolderProblemView::fixed_words(&title, path));
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DONE_FOR).await;
            let _ = this.update(cx, |this, cx| {
                let still_done = this
                    .native_sidebar
                    .folder_popover
                    .as_ref()
                    .is_some_and(|popover| popover.group_id == group_id && popover.done.is_some());
                if still_done {
                    this.close_folder_popover(cx);
                }
            });
        })
        .detach();
        true
    }

    /// A relocation the popover started failed: it says why under its title.
    pub(crate) fn folder_popover_failed(
        &mut self,
        project: &ProjectKey,
        message: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let group_id = project.to_sidebar_group_id();
        let Some(popover) = self
            .native_sidebar
            .folder_popover
            .as_mut()
            .filter(|popover| popover.group_id == group_id)
        else {
            return false;
        };
        popover.busy = false;
        popover.error = Some(format!("Couldn't move the project: {message}"));
        cx.notify();
        true
    }

    /// Use <found>: `/api/relocateProject` on the project's own gxserver, which moves its
    /// sessions with it.
    fn folder_popover_use(&mut self, group_id: &str, path: String, cx: &mut gpui::Context<Self>) {
        let Some(project) = ProjectKey::parse_sidebar_group_id(group_id) else {
            return;
        };
        let remote = match project.machine.remote_id() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.folder_popover_failed(&project, "that computer isn't connected", cx);
                    return;
                }
            },
            None => None,
        };
        if let Some(popover) = self.native_sidebar.folder_popover.as_mut() {
            popover.busy = true;
            popover.error = None;
        }
        cx.notify();
        let project_id = project.project_id.clone();
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(
                remote,
                "/api/relocateProject",
                json!({ "path": path, "projectId": project_id }),
            )
            .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => {
                    // The relocation publishes the project and its moved sessions itself.
                    this.folder_popover_relocated(&project, &path, cx);
                }
                Err(error) => {
                    this.folder_popover_failed(&project, &error.message, cx);
                }
            });
        })
        .detach();
    }

    #[cfg(not(ghostex_web))]
    fn folder_popover_locate(&mut self, project_id: String, cx: &mut gpui::Context<Self>) {
        self.handle_gpui_pick_replacement_project_folder_message(project_id, cx);
    }

    #[cfg(ghostex_web)]
    fn folder_popover_locate(&mut self, _project_id: String, _cx: &mut gpui::Context<Self>) {}

    #[cfg(not(ghostex_web))]
    fn folder_popover_ask_ai(&mut self, prompt: String, cx: &mut gpui::Context<Self>) {
        self.ask_ghostex_help(prompt, cx);
    }

    #[cfg(ghostex_web)]
    fn folder_popover_ask_ai(&mut self, _prompt: String, _cx: &mut gpui::Context<Self>) {}

    fn folder_popover_remove(&mut self, group_id: &str, cx: &mut gpui::Context<Self>) {
        let Some(popover) = self.native_sidebar.folder_popover.as_mut() else {
            return;
        };
        if !popover.confirm_remove {
            popover.confirm_remove = true;
            cx.notify();
            return;
        }
        self.close_folder_popover(cx);
        self.dispatch_native_sidebar_command(
            json!({ "type": "removeWorkspaceProjectForGroup", "groupId": group_id }),
            cx,
        );
    }

    /// The slashed computer after the project's name, when its folder is missing.
    pub(super) fn render_project_folder_mark(
        &self,
        group: &NativeSidebarGroup,
        tooltip_span: super::tooltips::SidebarTooltipSpan,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let problem = group.folder_problem.as_ref()?;
        let scale = appearance.scale;
        let color = if problem.quiet {
            appearance.muted
        } else {
            mark_amber(appearance)
        };
        let open = self.folder_popover_open_for(&group.group_id);
        let group_id = group.group_id.clone();
        let tooltip = problem.tooltip.clone();
        let hover = appearance.session_hover;
        Some(
            div()
                .id(format!("native-project-folder-mark-{}", group.group_id))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .size(px(18.0 * scale))
                .rounded(px(4.0 * scale))
                .cursor_pointer()
                .role(gpui::Role::Button)
                .aria_label(tooltip.clone())
                .when(open, |mark| mark.bg(hover))
                .hover(move |mark| mark.bg(hover))
                .child(titlebar_svg_icon(FOLDER_MARK_ICON, 13.0 * scale, color))
                // The header's press starts a drag and its click folds the project; a click on the
                // mark does neither.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.toggle_folder_popover(&group_id, cx);
                }))
                .when(
                    !open
                        && self.native_sidebar.pointer_inside
                        && self.native_sidebar.menu.is_none()
                        && !cx.has_active_drag(),
                    |mark| {
                        use gpui_component::tooltip::ManagedTooltipExt as _;
                        mark.managed_discrete_tooltip_with_placement(
                            tooltip_span.placement(),
                            appearance.tooltip_delay,
                            move |window, cx| {
                                super::tooltips::sidebar_tooltip(
                                    tooltip.clone(),
                                    tooltip_span,
                                    scale,
                                    window,
                                    cx,
                                )
                            },
                        )
                    },
                )
                .into_any_element(),
        )
    }

    /// The popover under the header of `group`, while it is open.
    pub(super) fn render_project_folder_popover(
        &self,
        group: &NativeSidebarGroup,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let popover = self
            .native_sidebar
            .folder_popover
            .as_ref()
            .filter(|popover| popover.group_id == group.group_id)?;
        let scale = appearance.scale;
        let fg = appearance.foreground;
        let muted = appearance.muted;
        let line = fg.opacity(0.1);
        let card = v_flex()
            .id(format!("native-project-folder-popover-{}", group.group_id))
            .occlude()
            .w_full()
            .px(px(6.0 * scale))
            .pt(px(12.0 * scale))
            .pb(px(6.0 * scale))
            .gap(px(10.0 * scale))
            .rounded(px(10.0 * scale))
            .border_1()
            .border_color(fg.opacity(0.14))
            .bg(popover_fill(appearance))
            .shadow_lg()
            .text_size(px(12.5 * scale))
            .font_weight(FontWeight::NORMAL)
            .cursor_default()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|app, _, _, cx| app.close_folder_popover(cx)));
        if let Some((title, detail)) = &popover.done {
            return Some(
                self.folder_popover_frame(
                    card.child(
                        h_flex()
                            .items_start()
                            .gap(px(8.0 * scale))
                            .px(px(8.0 * scale))
                            .pb(px(6.0 * scale))
                            .child(titlebar_svg_icon(
                                "titlebar/circle-check.svg",
                                14.0 * scale,
                                if appearance.light {
                                    rgb(0x15803d).into()
                                } else {
                                    rgb(0x4ade80).into()
                                },
                            ))
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .child(div().text_color(fg).child(title.clone()))
                                    .child(div().text_color(muted).child(detail.clone())),
                            ),
                    ),
                    scale,
                ),
            );
        }
        let problem = group.folder_problem.as_ref()?;
        let local = ProjectKey::parse_sidebar_group_id(&group.group_id)
            .is_some_and(|project| project.machine.is_local());
        let busy = popover.busy;
        let mut card = card.child(
            v_flex()
                .px(px(8.0 * scale))
                .gap(px(3.0 * scale))
                .child(
                    div()
                        .text_size(px(13.5 * scale))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(fg)
                        .child(problem.title.clone()),
                )
                .child(div().text_color(muted).child(problem.line.clone()))
                .when_some(popover.error.clone(), |text, error| {
                    text.child(div().text_color(danger(appearance)).child(error))
                }),
        );
        if !problem.paths.is_empty() {
            card = card.child(
                v_flex()
                    .mx(px(8.0 * scale))
                    .rounded(px(8.0 * scale))
                    .border_1()
                    .border_color(line)
                    .children(problem.paths.iter().enumerate().map(|(index, path)| {
                        h_flex()
                            .items_start()
                            .gap(px(8.0 * scale))
                            .px(px(9.0 * scale))
                            .py(px(5.0 * scale))
                            .when(index > 0, |row| row.border_t_1().border_color(line))
                            .child(
                                div()
                                    .w(px(48.0 * scale))
                                    .flex_shrink_0()
                                    .text_size(px(11.5 * scale))
                                    .text_color(muted)
                                    .child(path.label),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family(MODAL_MONO_FONT)
                                    .text_size(px(11.5 * scale))
                                    .text_color(if path.gone { muted } else { fg })
                                    .when(path.gone, |text| text.line_through())
                                    .child(path.path.clone()),
                            )
                    })),
            );
        }
        let primary = match (&problem.use_folder, &problem.use_label) {
            (Some(folder), Some(label)) => Some((label.clone(), Some(folder.clone()))),
            _ if problem.locate_primary && local && DESKTOP_FIXES => {
                Some(("Locate Folder…".to_string(), None))
            }
            _ => None,
        };
        let primary_is_locate = primary.as_ref().is_some_and(|(_, folder)| folder.is_none());
        if let Some((label, folder)) = primary {
            let group_id = group.group_id.clone();
            let raw_id = ProjectKey::parse_sidebar_group_id(&group.group_id)
                .map(|project| project.project_id)
                .unwrap_or_default();
            card = card.child(
                div()
                    .id("folder-popover-primary")
                    .mx(px(8.0 * scale))
                    .h(px(30.0 * scale))
                    .px(px(10.0 * scale))
                    .rounded(px(7.0 * scale))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(fg)
                    .text_color(popover_fill(appearance))
                    .text_size(px(13.0 * scale))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .when(busy, |button| button.opacity(0.6))
                    .child(div().min_w_0().truncate().child(if busy {
                        "Moving…".to_string()
                    } else {
                        label
                    }))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        if app
                            .native_sidebar
                            .folder_popover
                            .as_ref()
                            .is_some_and(|popover| popover.busy)
                        {
                            return;
                        }
                        match &folder {
                            Some(folder) => app.folder_popover_use(&group_id, folder.clone(), cx),
                            None => app.folder_popover_locate(raw_id.clone(), cx),
                        }
                    })),
            );
        }
        let row = |id: &'static str, icon: &'static str, label: String, color: Hsla| {
            h_flex()
                .id(id)
                .h(px(30.0 * scale))
                .px(px(10.0 * scale))
                .gap(px(9.0 * scale))
                .rounded(px(6.0 * scale))
                .text_size(px(13.0 * scale))
                .text_color(color)
                .cursor_pointer()
                .hover(|row| row.bg(appearance.session_hover))
                .child(titlebar_svg_icon(icon, 14.0 * scale, color))
                .child(label)
        };
        let mut menu = v_flex();
        if local && DESKTOP_FIXES && !primary_is_locate {
            let raw_id = ProjectKey::parse_sidebar_group_id(&group.group_id)
                .map(|project| project.project_id)
                .unwrap_or_default();
            menu = menu.child(
                row(
                    "folder-popover-locate",
                    "modals/missing-project-folder/folder-search.svg",
                    problem.locate_label.to_string(),
                    fg,
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.folder_popover_locate(raw_id.clone(), cx);
                })),
            );
        }
        let prompt = ask_ai_prompt(&group.title, problem);
        menu = menu.when(DESKTOP_FIXES, |menu| {
            menu.child(
                row(
                    "folder-popover-ask-ai",
                    "titlebar/sparkles.svg",
                    "Ask AI to fix".to_string(),
                    fg,
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.close_folder_popover(cx);
                    app.folder_popover_ask_ai(prompt.clone(), cx);
                })),
            )
        });
        let remove_group = group.group_id.clone();
        menu = menu
            .child(
                div()
                    .h(px(1.0))
                    .mx(px(8.0 * scale))
                    .my(px(4.0 * scale))
                    .bg(line),
            )
            .child(
                row(
                    "folder-popover-remove",
                    "titlebar/trash.svg",
                    if popover.confirm_remove {
                        format!("Remove {} from Ghostex", group.title)
                    } else {
                        "Remove project".to_string()
                    },
                    danger(appearance),
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.folder_popover_remove(&remove_group, cx);
                })),
            );
        Some(self.folder_popover_frame(card.child(menu), scale))
    }

    /// The popover placed under the header, painted over the rows below it.
    fn folder_popover_frame(&self, card: gpui::Stateful<gpui::Div>, scale: f32) -> AnyElement {
        gpui::deferred(
            div()
                .absolute()
                .top(px(33.0 * scale))
                .left(px((super::project_header::PROJECT_HEADER_CHEVRON_INSET
                    + 3.0)
                    * scale))
                .right(px(0.0))
                .child(card),
        )
        .with_priority(15)
        .into_any_element()
    }
}
