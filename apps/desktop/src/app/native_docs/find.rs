//! Find & Replace in the open file: the panel 8px above the formatting bar in a Markdown document,
//! and at the bottom right of a code or text file's editor, with Whole Word and Case Sensitive,
//! previous and next, Replace and Replace All.
//!
//! CDXC:Docs 2026-10-25 DECISION:
//! User: "i want to keep the old search in 1 file thingy that i had in markdown files and bring it to all other code files. should be just cmd + f to search a file and cmd + shift + f to search all files". Code and text files get the Markdown find panel itself (same look, keys, count and toggles), with every match marked in the editor and the current one marked brighter; the editor's own search panel is off so Cmd+F reaches this one.
//!
//! Adapted from the Docs prototype (`docs/2026-09-24/docs-gpui-editor-research/demo/src/find.rs`).

use std::ops::Range;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, Subscription, Window, div,
    px, svg,
};
use gpui_component::input::{
    EditorState, Input, InputEvent, InputState, TextDecoration, TextDecorationCollection,
};

use super::palette::DocsPalette;
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_tooltip;

pub(crate) struct DocsFind {
    pub(crate) visible: bool,
    pub(crate) query: Entity<InputState>,
    pub(crate) replace: Entity<InputState>,
    pub(crate) whole_word: bool,
    pub(crate) case_sensitive: bool,
    pub(crate) matches: Vec<Range<usize>>,
    pub(crate) active: usize,
    /// What the last step or replace reported (text, error), shown until the query changes; the
    /// match count shows otherwise (the Docs page's find status).
    pub(crate) status: Option<(String, bool)>,
    /// A code or text file's match marks, and which file they are on.
    code_marks: Option<(String, TextDecorationCollection)>,
    _subscriptions: Vec<Subscription>,
}

/// The editor the find panel works on: a Markdown document's live editor, or a code or text
/// file's editor.
enum FindTarget {
    Live(Entity<zorite_editor::EditorState>),
    Code(Entity<EditorState>),
}

/// The Markdown editor's own match fills (zorite `element.rs`), so both kinds of file look alike.
fn match_highlight(active: bool) -> gpui::HighlightStyle {
    gpui::HighlightStyle {
        background_color: Some(gpui::rgba(if active { 0x9BBCE052 } else { 0x9BBCE02E }).into()),
        ..Default::default()
    }
}

/// Every match of `needle` in `text` (byte ranges), honouring the case and whole-word options.
fn find_all(text: &str, needle: &str, case_sensitive: bool, whole_word: bool) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    let (hay, pat) = if case_sensitive {
        (text.to_string(), needle.to_string())
    } else {
        (text.to_lowercase(), needle.to_lowercase())
    };
    // Lowercasing can change byte lengths outside ASCII; match exactly then.
    let (hay, pat) = if hay.len() == text.len() && pat.len() == needle.len() {
        (hay, pat)
    } else {
        (text.to_string(), needle.to_string())
    };
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = hay[from..].find(&pat) {
        let start = from + found;
        let end = start + pat.len();
        let bounded = !whole_word
            || (text[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !is_word(c))
                && text[end..].chars().next().is_none_or(|c| !is_word(c)));
        if bounded {
            out.push(start..end);
        }
        from = end.max(start + 1);
        while from < hay.len() && !hay.is_char_boundary(from) {
            from += 1;
        }
    }
    out
}

impl GhostexGpuiApp {
    fn native_docs_live_editor(&self) -> Option<Entity<zorite_editor::EditorState>> {
        self.native_docs.active_document()?.live.clone()
    }

    fn native_docs_find_target(&self) -> Option<FindTarget> {
        self.native_docs_live_editor()
            .map(FindTarget::Live)
            .or_else(|| self.native_docs_code_find_editor().map(FindTarget::Code))
    }

    /// The matches in a code or text file, marked in its editor (the current one brighter), and
    /// the marks taken off a file the panel no longer works on.
    fn native_docs_refresh_code_find(&mut self, cx: &mut Context<Self>) {
        let code = self.native_docs_code_find_editor();
        let path = self.native_docs.active.clone();
        let Some(find) = self.native_docs.find.as_mut() else {
            return;
        };
        let keep = find.visible && code.is_some();
        if let Some((marked, marks)) = find.code_marks.clone()
            && (!keep || path.as_ref() != Some(&marked))
        {
            marks.clear(cx);
            find.code_marks = None;
        }
        let (Some(editor), Some(path), true) = (code, path, keep) else {
            return;
        };
        let needle = find.query.read(cx).value().to_string();
        find.matches = find_all(
            &editor.read(cx).value(),
            &needle,
            find.case_sensitive,
            find.whole_word,
        );
        if find.active >= find.matches.len() {
            find.active = 0;
        }
        let decorations = find
            .matches
            .iter()
            .enumerate()
            .map(|(index, range)| {
                TextDecoration::new(range.clone(), match_highlight(index == find.active))
            })
            .collect();
        match &find.code_marks {
            Some((_, marks)) => marks.set(decorations, cx),
            None => {
                let marks = editor.update(cx, |state, cx| {
                    state.create_decorations_collection(decorations, cx)
                });
                find.code_marks = Some((path, marks));
            }
        }
    }

    /// The panel is open on a code or text file whose marks are for another file (the open file
    /// changed): mark this one.
    pub(crate) fn native_docs_sync_code_find(&mut self, cx: &mut Context<Self>) {
        let stale = self.native_docs.find.as_ref().is_some_and(|find| {
            find.visible
                && find.code_marks.as_ref().map(|(path, _)| path)
                    != self.native_docs.active.as_ref()
        });
        if stale && self.native_docs_code_find_editor().is_some() {
            self.native_docs_refresh_find(cx);
        }
    }

    /// The open code or text file's editor, when it is what the view shows (not a CSV table or a
    /// JSON tree standing in for it).
    pub(crate) fn native_docs_code_find_editor(&self) -> Option<Entity<EditorState>> {
        let document = self.native_docs.active_document()?;
        if document.live.is_some() || document.data_view_open {
            return None;
        }
        document.editor.clone()
    }

    fn native_docs_ensure_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.native_docs.find.is_some() {
            return;
        }
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
        let replace = cx.new(|cx| InputState::new(window, cx).placeholder("Replace"));
        let subscriptions = vec![
            cx.subscribe_in(
                &query,
                window,
                |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        if let Some(find) = this.native_docs.find.as_mut() {
                            find.active = 0;
                            find.status = None;
                        }
                        this.native_docs_refresh_find(cx);
                        this.native_docs_reveal_match(window, cx);
                    }
                    InputEvent::PressEnter { shift, .. } => {
                        this.native_docs_step_find(if *shift { -1 } else { 1 }, window, cx);
                    }
                    _ => {}
                },
            ),
            cx.subscribe_in(
                &replace,
                window,
                |this: &mut Self, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.native_docs_replace_current(window, cx);
                    }
                },
            ),
        ];
        self.native_docs.find = Some(DocsFind {
            visible: false,
            query,
            replace,
            whole_word: false,
            case_sensitive: false,
            matches: Vec::new(),
            active: 0,
            status: None,
            code_marks: None,
            _subscriptions: subscriptions,
        });
    }

    /// Cmd+F in a Markdown document, or the bar's Find button: shows the panel with the selection
    /// as the query and puts the caret in it.
    ///
    /// CDXC:Docs 2026-09-12 DECISION:
    /// User: Cmd+F, or Ctrl+F on Windows, shows the document search and focuses it, and Escape closes it again while Docs has focus. The Docs file search keeps its own Escape, which clears the query.
    pub(crate) fn native_docs_show_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.native_docs_ensure_find(window, cx);
        let editor = self.native_docs_live_editor();
        let code = self.native_docs_code_find_editor();
        let Some(find) = self.native_docs.find.as_mut() else {
            return;
        };
        find.visible = true;
        let query = find.query.clone();
        let selected = if let Some(editor) = editor {
            let selection = editor.read(cx).selected_range();
            Some(editor.read(cx).text()[selection].to_string())
        } else {
            code.map(|code| code.read(cx).selected_text().to_string())
        };
        if let Some(selected) = selected
            && !selected.is_empty()
            && !selected.contains('\n')
        {
            query.update(cx, |query, cx| query.set_value(selected, window, cx));
        }
        super::actions::focus_and_select_all(&query, window);
        self.native_docs_refresh_find(cx);
        self.native_docs_notify(cx);
    }

    pub(crate) fn native_docs_hide_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = self.native_docs.find.as_mut() else {
            return;
        };
        if !find.visible {
            return;
        }
        find.visible = false;
        // Closing clears the query, the replacement and the status, like the Docs page.
        find.status = None;
        let (query, replace) = (find.query.clone(), find.replace.clone());
        query.update(cx, |query, cx| query.set_value("", window, cx));
        replace.update(cx, |replace, cx| replace.set_value("", window, cx));
        self.native_docs_refresh_find(cx);
        if let Some(editor) = self.native_docs_live_editor() {
            editor.update(cx, |editor, cx| editor.focus(window, cx));
        } else if let Some(code) = self.native_docs_code_find_editor() {
            code.update(cx, |state, cx| {
                state.set_selection_focus(None, cx);
                state.focus(window, cx);
            });
        }
        self.native_docs_notify(cx);
    }

    pub(crate) fn native_docs_find_visible(&self) -> bool {
        self.native_docs
            .find
            .as_ref()
            .is_some_and(|find| find.visible)
    }

    /// Recomputes the matches in the open editor and paints them.
    pub(crate) fn native_docs_refresh_find(&mut self, cx: &mut Context<Self>) {
        self.native_docs_refresh_code_find(cx);
        let Some(editor) = self.native_docs_live_editor() else {
            return;
        };
        let Some(find) = self.native_docs.find.as_mut() else {
            return;
        };
        let needle = find.query.read(cx).value().to_string();
        find.matches = if find.visible {
            find_all(
                editor.read(cx).text(),
                &needle,
                find.case_sensitive,
                find.whole_word,
            )
        } else {
            Vec::new()
        };
        if find.active >= find.matches.len() {
            find.active = 0;
        }
        let (matches, active) = (
            find.matches.clone(),
            (!find.matches.is_empty()).then_some(find.active),
        );
        editor.update(cx, |editor, cx| editor.set_search(matches, active, cx));
    }

    pub(crate) fn native_docs_step_find(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(find) = self.native_docs.find.as_mut() else {
            return;
        };
        if find.query.read(cx).value().is_empty() {
            find.status = Some(("Enter text".into(), true));
            self.native_docs_notify(cx);
            return;
        }
        if find.matches.is_empty() {
            find.status = Some(("No matches".into(), true));
            self.native_docs_notify(cx);
            return;
        }
        let len = find.matches.len() as isize;
        find.active = ((find.active as isize + delta).rem_euclid(len)) as usize;
        find.status = Some((format!("{}/{}", find.active + 1, find.matches.len()), false));
        self.native_docs_refresh_find(cx);
        self.native_docs_reveal_match(window, cx);
    }

    /// Scrolls so the active match sits a third of the way down the document.
    fn native_docs_reveal_match(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // A code or text file: select the match, which scrolls the editor to it, and keep the
        // selection showing while the caret stays in the find field.
        if let Some(code) = self.native_docs_code_find_editor() {
            let Some(find) = self.native_docs.find.as_ref() else {
                return;
            };
            let Some(range) = find.matches.get(find.active).cloned() else {
                return;
            };
            let focus = gpui::Focusable::focus_handle(find.query.read(cx), cx);
            code.update(cx, |state, cx| {
                state.set_selection_focus(Some(focus), cx);
                state.set_selected_range(range, cx);
            });
            return;
        }
        let Some(document) = self.native_docs.active_document() else {
            return;
        };
        let (Some(editor), Some(find)) = (document.live.clone(), self.native_docs.find.as_ref())
        else {
            return;
        };
        let Some(range) = find.matches.get(find.active) else {
            return;
        };
        let Some(top) = editor.read(cx).offset_screen_top(range.start) else {
            return;
        };
        let scroll = document.scroll.clone();
        let viewport = scroll.bounds();
        let mut offset = scroll.offset();
        let target = viewport.top() + viewport.size.height / 3.0;
        if top < viewport.top() + px(24.0) || top > viewport.bottom() - px(96.0) {
            offset.y = (offset.y - (top - target)).min(px(0.0));
            scroll.set_offset(offset);
        }
    }

    fn native_docs_replace_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.native_docs_find_target() else {
            return;
        };
        let Some(find) = self.native_docs.find.as_mut() else {
            return;
        };
        if find.query.read(cx).value().is_empty() {
            find.status = Some(("Enter text".into(), true));
            self.native_docs_notify(cx);
            return;
        }
        let Some(range) = find.matches.get(find.active).cloned() else {
            find.status = Some(("No matches".into(), true));
            self.native_docs_notify(cx);
            return;
        };
        let replacement = find.replace.read(cx).value().to_string();
        match &target {
            FindTarget::Live(editor) => editor.update(cx, |editor, cx| {
                editor.replace_range(range, &replacement, cx);
                cx.emit(zorite_editor::EditorEvent::Changed);
            }),
            FindTarget::Code(code) => code.update(cx, |state, cx| {
                state.set_selected_range(range, cx);
                state.replace(replacement, window, cx);
            }),
        }
        self.native_docs_refresh_find(cx);
        if let Some(find) = self.native_docs.find.as_mut() {
            find.status = Some((
                if find.matches.is_empty() {
                    "Replaced".to_string()
                } else {
                    format!("Replaced - {}/{}", find.active + 1, find.matches.len())
                },
                false,
            ));
        }
        self.native_docs_notify(cx);
    }

    fn native_docs_replace_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.native_docs_find_target() else {
            return;
        };
        let Some(find) = self.native_docs.find.as_mut() else {
            return;
        };
        if find.query.read(cx).value().is_empty() {
            find.status = Some(("Enter text".into(), true));
            self.native_docs_notify(cx);
            return;
        }
        if find.matches.is_empty() {
            find.status = Some(("No matches".into(), true));
            self.native_docs_notify(cx);
            return;
        }
        let replacement = find.replace.read(cx).value().to_string();
        let matches = find.matches.clone();
        find.active = 0;
        find.status = Some((format!("Replaced {} matches", matches.len()), false));
        match &target {
            FindTarget::Live(editor) => editor.update(cx, |editor, cx| {
                for range in matches.into_iter().rev() {
                    editor.replace_range(range, &replacement, cx);
                }
                cx.emit(zorite_editor::EditorEvent::Changed);
            }),
            FindTarget::Code(code) => code.update(cx, |state, cx| {
                for range in matches.into_iter().rev() {
                    state.set_selected_range(range, cx);
                    state.replace(replacement.clone(), window, cx);
                }
            }),
        }
        self.native_docs_refresh_find(cx);
    }

    /// CDXC:Docs 2026-09-16 DECISION:
    /// User: give the Docs find-and-replace pane the same look and background as the floating formatting bar in light and dark modes so it stands out from the document.
    pub(crate) fn render_native_docs_find(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        Some(
            self.native_docs_find_panel(p, cx)?
                .absolute()
                .bottom(px(45.0))
                .right(px(0.0))
                .into_any_element(),
        )
    }

    /// The same panel over a code or text file, at the editor's bottom right (clear of its
    /// scrollbar), where a Markdown document has its formatting bar.
    pub(crate) fn render_native_docs_code_find(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.native_docs_code_find_editor()?;
        self.native_docs_sync_code_find(cx);
        Some(
            self.native_docs_find_panel(p, cx)?
                .absolute()
                .bottom(px(14.0))
                .right(px(20.0))
                .into_any_element(),
        )
    }

    fn native_docs_find_panel(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        let find = self.native_docs.find.as_ref().filter(|find| find.visible)?;
        let needle_empty = find.query.read(cx).value().is_empty();
        // The find status, worded as the former React Docs page worded it.
        let (status, no_results) = match &find.status {
            Some((text, error)) => (text.clone(), *error),
            None if needle_empty => (String::new(), false),
            None if find.matches.is_empty() => ("No matches".into(), true),
            None => (format!("{} matches", find.matches.len()), false),
        };
        let (hover, icon_color) = (p.control_hover, p.toolbar_icon);
        let button =
            |id: &'static str, icon: &'static str, tooltip: &'static str, toggled: Option<bool>| {
                div()
                    .id(id)
                    .size(px(24.0))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .when(toggled == Some(false), |b| b.opacity(0.5))
                    .when(toggled == Some(true), |b| b.bg(hover))
                    .hover(move |b| b.bg(hover))
                    .child(svg().path(icon).size(px(16.0)).text_color(icon_color))
                    .tooltip(move |window, cx| titlebar_tooltip(tooltip, window, cx))
            };
        let field = |state: &Entity<InputState>, status: Option<(String, bool)>| {
            div()
                .w(px(200.0))
                .h(px(24.0))
                .px(px(7.0))
                .flex()
                .items_center()
                .rounded(px(5.0))
                .bg(p.text.opacity(0.05))
                .text_size(px(12.5))
                .child(div().flex_1().child(Input::new(state).appearance(false)))
                .when_some(status, |f, (status, error)| {
                    f.child(
                        div()
                            .flex_none()
                            .pl(px(6.0))
                            .text_size(px(11.0))
                            .text_color(if error { p.danger } else { p.subtle })
                            .child(status),
                    )
                })
        };
        let (whole_word, case_sensitive) = (find.whole_word, find.case_sensitive);
        Some(
            div()
                .id("native-docs-find")
                .p(px(8.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .rounded(px(10.0))
                // CDXC:Theming 2026-11-20 (decision in helpers/chrome_palette.rs): lifted under
                // glass, not the near-black #0b0b0b.
                .bg(p.glass_lifted(p.floating))
                .border_1()
                .border_color(p.border)
                .shadow_lg()
                .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        this.native_docs_hide_find(window, cx);
                        cx.stop_propagation();
                    } else if event.keystroke.key == "f3" {
                        let back = event.keystroke.modifiers.shift;
                        this.native_docs_step_find(if back { -1 } else { 1 }, window, cx);
                        cx.stop_propagation();
                    }
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(field(&find.query, Some((status, no_results))))
                        .child(
                            button(
                                "docs-find-word",
                                "files-view/l-whole-word-2.svg",
                                "Whole Word",
                                Some(whole_word),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(find) = this.native_docs.find.as_mut() {
                                    find.whole_word = !find.whole_word;
                                }
                                this.native_docs_refresh_find(cx);
                                this.native_docs_notify(cx);
                            })),
                        )
                        .child(
                            button(
                                "docs-find-case",
                                "files-view/l-case-sensitive-2.svg",
                                "Case Sensitive",
                                Some(case_sensitive),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(find) = this.native_docs.find.as_mut() {
                                    find.case_sensitive = !find.case_sensitive;
                                }
                                this.native_docs_refresh_find(cx);
                                this.native_docs_notify(cx);
                            })),
                        )
                        .child(
                            button(
                                "docs-find-prev",
                                "files-view/l-chevron-up-2.svg",
                                "Previous Match",
                                None,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| this.native_docs_step_find(-1, window, cx),
                            )),
                        )
                        .child(
                            button(
                                "docs-find-next",
                                "files-view/l-chevron-down-2.svg",
                                "Next Match",
                                None,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| this.native_docs_step_find(1, window, cx),
                            )),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(field(&find.replace, None))
                        .child(
                            button(
                                "docs-replace-one",
                                "files-view/l-replace-2.svg",
                                "Replace Current Match",
                                None,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| this.native_docs_replace_current(window, cx),
                            )),
                        )
                        .child(
                            button(
                                "docs-replace-all",
                                "files-view/l-replace-all-2.svg",
                                "Replace All Matches",
                                None,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| this.native_docs_replace_all(window, cx),
                            )),
                        )
                        .child(div().w(px(24.0)))
                        .child(
                            button(
                                "docs-find-close",
                                "files-view/l-x-2.svg",
                                "Close Find",
                                None,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| this.native_docs_hide_find(window, cx),
                            )),
                        ),
                ),
        )
    }
}
