//! Drawing the Changes view: the toolbar with its jump-to-file menu, the stream with its pinned
//! file header, the file menu and the keys.

use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    canvas, div, list, px,
};
use gpui_component::{h_flex, v_flex};

use super::model::{ChangesScope, Row};
use super::rows::{ICON_CHECK, ICON_CHEVRON_DOWN, stat, status_badge};
use super::style::{
    CODE_SIZE, CONTROL_HEIGHT, ChangesStyle, FILE_HEADER_HEIGHT, ICON_SIZE, SMALL_SIZE, UI_SIZE,
};
use super::view::{ChangesRequest, ChangesView, Confirm};
use crate::app::window::native_modal_kit::{
    MODAL_UI_FONT, hsla, modal_icon, modal_spinner, rgba_of,
};

const ICON_UNIFIED: &str = "modals/git-commit/layout-rows.svg";
const ICON_SPLIT: &str = "modals/git-commit/columns-2.svg";
const ICON_WRAP: &str = "modals/git-commit/text-wrap.svg";
const ICON_WHITESPACE: &str = "modals/git-commit/pilcrow.svg";
const ICON_REFRESH: &str = "titlebar/refresh.svg";
const ICON_FOLD: &str = "titlebar/arrows-diagonal-minimize.svg";
const ICON_UNFOLD: &str = "titlebar/arrows-diagonal-2.svg";
const ICON_BRANCH: &str = "titlebar/git-branch.svg";
const ICON_PENCIL: &str = "titlebar/pencil.svg";

impl Render for ChangesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(s) = self.style else {
            return div().into_any_element();
        };
        self.sync_files_zoom();
        let view = cx.entity();
        let body = self.render_body(&s, window, cx);
        v_flex()
            .id("changes-view")
            .key_context("ChangesView")
            .track_focus(&self.focus)
            .size_full()
            .min_w_0()
            .min_h_0()
            .relative()
            .font_family(MODAL_UI_FONT)
            .text_color(hsla(s.text))
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.end_selection();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.file_menu.take().is_some()
                        || std::mem::take(&mut this.jump_open)
                        || std::mem::take(&mut this.scope_open)
                        || this.commit_menu_open
                        || this.agent_select.open
                    {
                        this.commit_menu_open = false;
                        this.agent_select.close();
                        cx.notify();
                    }
                }),
            )
            // The view's own origin, so menus asked for at a pointer position land under it.
            .child(
                canvas(
                    move |bounds, _, cx| {
                        view.update(cx, |this, _| {
                            this.view_origin = bounds.origin;
                            this.view_width = bounds.size.width;
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(self.render_toolbar(&s, cx))
            .child(body)
            .children(self.render_commit_bar(&s, window, cx))
            .children(self.render_file_menu(&s, cx))
            .children(self.render_jump_menu(&s, cx))
            .children(self.render_scope_menu(&s, cx))
            .into_any_element()
    }
}

impl ChangesView {
    fn render_toolbar(&mut self, s: &ChangesStyle, cx: &mut Context<Self>) -> AnyElement {
        let data = self.data.as_ref();
        let refs = data.map(|data| {
            let branch = data.branch.clone().unwrap_or_else(|| "HEAD".to_string());
            match self.scope {
                // CDXC:Changes 2026-10-25 DECISION: User: "remove '(uncommitted)' which comes after the main branch name there"; on the default branch the toolbar names just the branch.
                ChangesScope::AgainstMain if data.on_default_branch => branch,
                ChangesScope::AgainstMain => format!("{branch} \u{2192} {}", data.base_label),
                ChangesScope::Uncommitted => branch,
            }
        });
        let file_count = data.map_or(0, |data| data.files.len());
        let toggle = |id: &'static str, icon: &'static str, on: bool, tip: &'static str| {
            div()
                .id(id)
                .flex_none()
                .size(px(CONTROL_HEIGHT))
                .rounded(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .when(on, |this| this.bg(hsla(s.selected)))
                .hover(|this| this.bg(hsla(s.hover)))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tip).build(window, cx)
                })
                .child(modal_icon(
                    icon,
                    ICON_SIZE + 1.0,
                    if on { s.text } else { s.muted },
                ))
        };
        let all_collapsed = self.all_collapsed();
        // CDXC:Changes 2026-10-25 DECISION: User: "please make the files dropdown on the 1st row"; the toolbar is one row, and the branch name gives way first when the panel is narrow.
        h_flex()
            .flex_none()
            .w_full()
            .overflow_hidden()
            .min_h(px(CONTROL_HEIGHT + 12.0))
            .px(px(8.0))
            .py(px(6.0))
            .gap(px(4.0))
            .border_b_1()
            .border_color(hsla(s.hairline))
            .whitespace_nowrap()
            .when_some(refs, |this, refs| {
                this.child(
                    h_flex()
                        .min_w_0()
                        .max_w(px(280.0))
                        .flex_shrink(1.0)
                        .gap(px(5.0))
                        .overflow_hidden()
                        .text_size(px(SMALL_SIZE))
                        .text_color(hsla(s.muted))
                        .child(modal_icon(ICON_BRANCH, ICON_SIZE - 1.0, s.muted))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(refs),
                        ),
                )
            })
            // CDXC:Changes 2026-10-25 DECISION: User: "please remove the files list at the top here no need it's duplicate of the stuff below". The file headers in the stream carry the checkboxes and counts; the file count opens a menu that jumps to a file.
            .when(file_count > 0, |this| {
                let anchor = cx.entity().downgrade();
                this.child(
                    h_flex()
                        .id("changes-jump-toggle")
                        .relative()
                        .flex_none()
                        .h(px(CONTROL_HEIGHT))
                        .px(px(10.0))
                        .gap(px(5.0))
                        .rounded(px(6.0))
                        .cursor_pointer()
                        // CDXC:Changes 2026-10-27 DECISION: User (Q653, "Yes, match it"): the file count uses the same size as the scope buttons and the branch name beside it.
                        .text_size(px(SMALL_SIZE))
                        .text_color(hsla(s.muted))
                        .when(self.jump_open, |this| this.bg(hsla(s.selected)))
                        .hover(|this| this.bg(hsla(s.hover)))
                        .tooltip(|window, cx| {
                            gpui_component::tooltip::Tooltip::new("Jump to a file")
                                .build(window, cx)
                        })
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.jump_open = !this.jump_open;
                            this.scope_open = false;
                            this.file_menu = None;
                            cx.notify();
                        }))
                        .child(
                            canvas(
                                move |bounds, _, cx| {
                                    let _ = anchor.update(cx, |this, _| this.jump_anchor = bounds);
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        )
                        .child(format!(
                            "{file_count} {}",
                            if file_count == 1 { "file" } else { "files" }
                        ))
                        .child(modal_icon(ICON_CHEVRON_DOWN, ICON_SIZE - 2.0, s.muted)),
                )
            })
            .child(div().flex_1().min_w(px(0.0)))
            .when(self.loading, |this| {
                this.child(div().flex_none().child(modal_spinner(s.muted)))
            })
            .child(self.render_scope_button(s, cx))
            .child(
                h_flex()
                    .flex_none()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(hsla(s.hairline))
                    .child(
                        toggle("changes-unified", ICON_UNIFIED, !self.split, "Unified")
                            .on_click(cx.listener(|this, _, _, cx| this.set_split(false, cx))),
                    )
                    .child(
                        toggle("changes-split", ICON_SPLIT, self.split, "Split")
                            .on_click(cx.listener(|this, _, _, cx| this.set_split(true, cx))),
                    ),
            )
            .child(
                toggle(
                    "changes-wrap",
                    ICON_WRAP,
                    self.wrap || self.split,
                    if self.split {
                        "Split view always wraps"
                    } else {
                        "Wrap long lines"
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.wrap = !this.wrap;
                    cx.notify();
                })),
            )
            .child(
                toggle(
                    "changes-whitespace",
                    ICON_WHITESPACE,
                    self.hide_whitespace,
                    "Hide whitespace changes",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    let hide = !this.hide_whitespace;
                    this.set_hide_whitespace(hide, cx);
                })),
            )
            .child(
                toggle(
                    "changes-fold-all",
                    if all_collapsed {
                        ICON_UNFOLD
                    } else {
                        ICON_FOLD
                    },
                    false,
                    if all_collapsed {
                        "Expand all files"
                    } else {
                        "Collapse all files"
                    },
                )
                .on_click(
                    cx.listener(move |this, _, _, cx| this.set_all_collapsed(!all_collapsed, cx)),
                ),
            )
            .child(
                toggle("changes-refresh", ICON_REFRESH, false, "Refresh")
                    .on_click(cx.listener(|this, _, _, cx| this.load(false, cx))),
            )
            .into_any_element()
    }

    /// The scope's dropdown button on the right of the toolbar: the scope shown, and a menu that
    /// switches to the other.
    ///
    /// CDXC:Changes 2026-10-27 DECISION: User: "i don't like "Uncommited" and "Against main" being shown as 2 buttons, should just be a toggle button on the right side that toggles these 2 states and should be clearer name". From the options page they picked the dropdown-style button (Q650 "dropdown") named "Whole branch & Uncommitted only" (Q651); Whole branch still compares against the project's base (main, develop). The phone's Changes screen got the same (Q652 "same").
    fn render_scope_button(&mut self, s: &ChangesStyle, cx: &mut Context<Self>) -> AnyElement {
        let anchor = cx.entity().downgrade();
        let icon = match self.scope {
            ChangesScope::AgainstMain => ICON_BRANCH,
            ChangesScope::Uncommitted => ICON_PENCIL,
        };
        h_flex()
            .id("changes-scope")
            .relative()
            .flex_none()
            .h(px(CONTROL_HEIGHT))
            .pl(px(9.0))
            .pr(px(7.0))
            .gap(px(5.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(hsla(s.hairline))
            .cursor_pointer()
            .text_size(px(SMALL_SIZE))
            .text_color(hsla(s.text))
            .when(self.scope_open, |this| this.bg(hsla(s.selected)))
            .hover(|this| this.bg(hsla(s.hover)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                this.scope_open = !this.scope_open;
                this.jump_open = false;
                this.file_menu = None;
                cx.notify();
            }))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = anchor.update(cx, |this, _| this.scope_anchor = bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(modal_icon(icon, ICON_SIZE - 1.0, s.muted))
            .child(self.scope.label())
            .child(modal_icon(ICON_CHEVRON_DOWN, ICON_SIZE - 2.0, s.muted))
            .into_any_element()
    }

    /// The scope button's menu: both scopes with a line on what each shows, the current one ticked.
    fn render_scope_menu(
        &mut self,
        s: &ChangesStyle,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.scope_open {
            return None;
        }
        let base = self
            .data
            .as_ref()
            .map_or_else(|| "main".to_string(), |data| data.base_label.clone());
        let width = px(300.0);
        let left = (self.scope_anchor.right() - self.view_origin.x - width)
            .min(self.view_width - width - px(8.0))
            .max(px(8.0));
        let top = self.scope_anchor.bottom() - self.view_origin.y + px(4.0);
        let items: Vec<_> = [ChangesScope::AgainstMain, ChangesScope::Uncommitted]
            .into_iter()
            .map(|scope| {
                let on = self.scope == scope;
                h_flex()
                    .id(SharedString::from(format!(
                        "changes-scope-{}",
                        scope.wire()
                    )))
                    .w_full()
                    .items_start()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .when(on, |this| this.bg(hsla(s.selected)))
                    .hover(|this| this.bg(hsla(s.hover)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.scope_open = false;
                        this.set_scope(scope, cx);
                        cx.notify();
                    }))
                    .child(div().flex_none().w(px(14.0)).pt(px(2.0)).when(on, |this| {
                        this.child(modal_icon(ICON_CHECK, 12.0, s.accent))
                    }))
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .text_size(px(SMALL_SIZE))
                                    .text_color(hsla(s.text))
                                    .child(scope.label()),
                            )
                            .child(
                                div()
                                    .text_size(px(SMALL_SIZE - 1.0))
                                    .text_color(hsla(s.muted))
                                    .whitespace_normal()
                                    .child(scope.detail(&base)),
                            ),
                    )
            })
            .collect();
        Some(
            v_flex()
                .id("changes-scope-menu")
                .absolute()
                .left(left)
                .top(top)
                .w(width)
                .p(px(4.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(s.menu_bubble().1)
                .bg(s.menu_bubble().0)
                .shadow_lg()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(items)
                .into_any_element(),
        )
    }

    fn render_body(
        &mut self,
        s: &ChangesStyle,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.target.is_none() {
            return message(s, "Open a project to see its changes.", None);
        }
        if let Some(error) = &self.error
            && self.data.is_none()
        {
            return message(s, "Could not read the changes", Some(error.clone()));
        }
        let Some(data) = &self.data else {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(modal_spinner(s.muted))
                .into_any_element();
        };
        if data.files.is_empty() {
            let text = match self.scope {
                ChangesScope::AgainstMain if data.on_default_branch => "No uncommitted changes.",
                ChangesScope::AgainstMain => "This branch has no changes against its base.",
                ChangesScope::Uncommitted => "No uncommitted changes.",
            };
            return message(s, text, None);
        }
        let truncated = data.truncated;
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .when(truncated, |this| {
                this.child(
                    div()
                        .px(px(12.0))
                        .py(px(4.0))
                        .text_size(px(SMALL_SIZE))
                        .text_color(hsla(s.badge_modified))
                        .child(
                            "The diff is too large to show completely; some files are left out.",
                        ),
                )
            })
            .child(self.render_stream(s, cx))
            .into_any_element()
    }

    /// The file count's menu: every file with its badge, folder and counts; picking one scrolls
    /// the stream to its header.
    fn render_jump_menu(&mut self, s: &ChangesStyle, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.jump_open {
            return None;
        }
        let data = self.data.as_ref()?;
        let current = self.current_file();
        let width = px(360.0);
        let left = (self.jump_anchor.left() - self.view_origin.x)
            .min(self.view_width - width - px(8.0))
            .max(px(8.0));
        // The diff's totals, which the one-row toolbar has no room for.
        let totals = h_flex()
            .flex_none()
            .w_full()
            .h(px(FILE_HEADER_HEIGHT - 10.0))
            .px(px(8.0))
            .gap(px(8.0))
            .text_size(px(SMALL_SIZE))
            .text_color(hsla(s.muted))
            .child(format!(
                "{} {} changed",
                data.files.len(),
                if data.files.len() == 1 {
                    "file"
                } else {
                    "files"
                }
            ))
            .child(div().flex_1())
            .child(stat(s, data.additions(), data.deletions()));
        let top = self.jump_anchor.bottom() - self.view_origin.y + px(4.0);
        let rows: Vec<_> = data
            .files
            .iter()
            .map(|file| {
                let path = file.path.clone();
                let scroll_path = path.clone();
                h_flex()
                    .id(SharedString::from(format!("changes-jump-{path}")))
                    .flex_none()
                    .w_full()
                    .h(px(FILE_HEADER_HEIGHT - 6.0))
                    .px(px(8.0))
                    .gap(px(8.0))
                    .rounded(px(5.0))
                    .text_size(px(CODE_SIZE))
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .when(current.as_deref() == Some(path.as_str()), |this| {
                        this.bg(hsla(s.selected))
                    })
                    .hover(|this| this.bg(hsla(s.hover)))
                    .when(self.viewed.contains_key(&path), |this| this.opacity(0.55))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.jump_open = false;
                        this.jump_to_file(&scroll_path, cx);
                    }))
                    .child(status_badge(s, file.status))
                    .child(
                        div()
                            .flex_none()
                            .text_color(hsla(s.text))
                            .child(file.file_name().to_string()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_shrink(1.0)
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(hsla(s.muted))
                            .child(file.folder().trim_end_matches('/').to_string()),
                    )
                    .child(div().flex_1())
                    .child(stat(s, file.additions, file.deletions))
            })
            .collect();
        Some(
            v_flex()
                .id("changes-jump-menu")
                .absolute()
                .left(left)
                .top(top)
                .w(width)
                .max_h(px(420.0))
                .overflow_y_scroll()
                .p(px(4.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(s.menu_bubble().1)
                .bg(s.menu_bubble().0)
                .shadow_lg()
                // A menu takes the pointer, so scrolling it never scrolls the diff behind it.
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(totals)
                .children(rows)
                .into_any_element(),
        )
    }

    fn render_stream(&mut self, s: &ChangesStyle, cx: &mut Context<Self>) -> AnyElement {
        let pinned = self.pinned_file_header();
        let header = pinned.map(|(file, shift)| {
            div()
                .absolute()
                .top(shift)
                .left_0()
                .w_full()
                .child(self.render_file_header(s, file, true, cx))
        });
        let resolved = self.resolved_count();
        let view = cx.entity().downgrade();
        let center_view = view.clone();
        let list_state = self.list.clone();
        list_state.set_scroll_handler(move |_, _, cx| {
            // A header that scrolls away never hears the pointer leave it.
            let _ = view.update(cx, |this, cx| {
                this.hovered_header = None;
                cx.notify();
            });
        });
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(hsla(s.code_bg))
            .child(
                list(
                    self.list.clone(),
                    cx.processor(|this, index, window, cx| this.render_row(index, window, cx)),
                )
                .size_full(),
            )
            // After the list's layout, so the rows around a jumped-to header are measured.
            .child(
                canvas(
                    move |_, _, cx| {
                        let _ = center_view
                            .update(cx, |this: &mut Self, cx| this.center_pending_file(cx));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_0(),
            )
            .children(header)
            .when(resolved > 0, |this| {
                let label = if self.show_resolved {
                    format!("Hide resolved comments ({resolved})")
                } else {
                    format!("Show resolved comments ({resolved})")
                };
                this.child(
                    div()
                        .id("changes-show-resolved")
                        .absolute()
                        .bottom(px(8.0))
                        .right(px(12.0))
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(hsla(s.hairline))
                        .bg(hsla(s.sticky_bg))
                        .text_size(px(SMALL_SIZE))
                        .text_color(hsla(s.muted))
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_show_resolved(cx)))
                        .child(label),
                )
            })
            .into_any_element()
    }

    /// The file whose header stays pinned at the top, and how far the next file's header has
    /// pushed it up. Only an expanded file pins, and only while its own lines scroll under it.
    ///
    /// CDXC:Changes 2026-10-25 WHY: a folded file's header used to stay pinned while the next
    /// files' headers scrolled under it, drawing two rows over each other; now a folded file never
    /// pins and the next header pushes the pinned one off instead of sliding beneath it.
    fn pinned_file_header(&self) -> Option<(usize, gpui::Pixels)> {
        let top = self.list.logical_scroll_top();
        let (index, file) = (0..=top.item_ix.min(self.rows.len().checked_sub(1)?))
            .rev()
            .find_map(|index| match self.rows.get(index) {
                Some(Row::File(file)) => Some((index, *file)),
                _ => None,
            })?;
        if index == top.item_ix && top.offset_in_item <= px(0.0) {
            return None;
        }
        let path = &self.data.as_ref()?.files.get(file)?.path;
        if self.collapsed.contains(path) || self.viewed.contains_key(path) {
            return None;
        }
        let height = px(FILE_HEADER_HEIGHT);
        let shift = (index + 1..self.rows.len())
            .find(|next| matches!(self.rows[*next], Row::File(_)))
            .and_then(|next| self.list.bounds_for_item(next))
            .map(|bounds| (bounds.top() - self.list.viewport_bounds().top() - height).min(px(0.0)))
            .unwrap_or(px(0.0));
        (shift > -height).then_some((file, shift))
    }

    fn render_file_menu(&mut self, s: &ChangesStyle, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (path, position) = self.file_menu.clone()?;
        let item = |id: &'static str, label: &'static str, danger: bool| {
            div()
                .id(id)
                .h(px(FILE_HEADER_HEIGHT - 6.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded(px(5.0))
                .text_size(px(CODE_SIZE))
                .cursor_pointer()
                .text_color(hsla(if danger { s.danger } else { s.text }))
                .hover(|this| this.bg(hsla(s.hover)))
                .child(label)
        };
        let (copy, open, reveal, revert) = (path.clone(), path.clone(), path.clone(), path.clone());
        Some(
            v_flex()
                .id("changes-file-menu")
                .absolute()
                .left(position.x)
                .top(position.y)
                .w(px(240.0))
                .p(px(4.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(s.menu_bubble().1)
                .bg(s.menu_bubble().0)
                .shadow_lg()
                // A menu takes the pointer, so scrolling it never scrolls the diff behind it.
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    item("changes-menu-copy", "Copy Path", false).on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.file_menu = None;
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy.clone()));
                            (this.host)(ChangesRequest::Copied, window, cx);
                        },
                    )),
                )
                .child(
                    item("changes-menu-open", "Open in Files", false).on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.file_menu = None;
                            this.open_in_files(&open, None, window, cx);
                        },
                    )),
                )
                .child(
                    item("changes-menu-reveal", "Open File/Folder Location", false).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.file_menu = None;
                            if let Some(target) = this.target.clone() {
                                (this.host)(
                                    ChangesRequest::RevealFile {
                                        project_key: target.project_key,
                                        path: reveal.clone(),
                                    },
                                    window,
                                    cx,
                                );
                            }
                        }),
                    ),
                )
                .child(div().h(px(1.0)).my(px(4.0)).bg(hsla(s.hairline)))
                .child(
                    item("changes-menu-revert", "Revert file\u{2026}", true).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.file_menu = None;
                            this.confirm = Some(Confirm::RevertFile(revert.clone()));
                            this.scroll_to_file(&revert, cx);
                        },
                    )),
                )
                .into_any_element(),
        )
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Keys typed into the commit message or a comment belong to that field.
        if self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.input.read(cx).focus_handle(cx).is_focused(window))
            || self
                .message
                .as_ref()
                .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window))
        {
            return;
        }
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.platform || modifiers.control {
            if keystroke.key == "c" && self.copy_selection(window, cx) {
                cx.stop_propagation();
            }
            return;
        }
        let handled = match keystroke.key.as_str() {
            "j" => {
                self.step(true, |row| matches!(row, Row::Hunk(..)), cx);
                true
            }
            "k" => {
                self.step(false, |row| matches!(row, Row::Hunk(..)), cx);
                true
            }
            "]" => {
                self.step(true, |row| matches!(row, Row::File(..)), cx);
                true
            }
            "[" => {
                self.step(false, |row| matches!(row, Row::File(..)), cx);
                true
            }
            "c" => {
                self.comment_at_cursor(window, cx);
                true
            }
            "v" => {
                if let Some(path) = self.current_file() {
                    self.toggle_viewed(&path, cx);
                    self.step(true, |row| matches!(row, Row::File(..)), cx);
                }
                true
            }
            "o" => {
                if let Some(path) = self.current_file() {
                    self.open_in_files(&path, None, window, cx);
                }
                true
            }
            "escape" => {
                let had = self.selection.take().is_some()
                    || self.file_menu.take().is_some()
                    || std::mem::take(&mut self.jump_open)
                    || std::mem::take(&mut self.scope_open)
                    || self.confirm.take().is_some()
                    || std::mem::take(&mut self.commit_menu_open);
                cx.notify();
                had
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }
}

fn message(s: &ChangesStyle, title: &str, detail: Option<String>) -> AnyElement {
    v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .p(px(24.0))
        .child(
            div()
                .text_size(px(UI_SIZE))
                .text_color(hsla(s.text))
                .child(title.to_string()),
        )
        .when_some(detail, |this, detail| {
            this.child(
                div()
                    .text_size(px(SMALL_SIZE))
                    .text_color(hsla(rgba_of(s.muted, 0.9)))
                    .child(detail),
            )
        })
        .into_any_element()
}
