//! Find & Replace in the open Markdown document: the panel 8px above the formatting bar, with
//! Whole Word and Case Sensitive, previous and next, Replace and Replace All.
//!
//! Adapted from the Docs prototype (`docs/2026-09-24/docs-gpui-editor-research/demo/src/find.rs`).

use std::ops::Range;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, Subscription, Window, div,
    px, svg,
};
use gpui_component::input::{Input, InputEvent, InputState};

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
    _subscriptions: Vec<Subscription>,
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
                |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.native_docs_replace_current(cx);
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
        let Some(find) = self.native_docs.find.as_mut() else {
            return;
        };
        find.visible = true;
        let query = find.query.clone();
        if let Some(editor) = editor {
            let selection = editor.read(cx).selected_range();
            let selected = editor.read(cx).text()[selection].to_string();
            if !selected.is_empty() && !selected.contains('\n') {
                query.update(cx, |query, cx| query.set_value(selected, window, cx));
            }
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
        self.native_docs_refresh_find(cx);
        if let Some(editor) = self.native_docs_live_editor() {
            editor.update(cx, |editor, cx| editor.focus(window, cx));
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

    fn native_docs_step_find(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = self.native_docs.find.as_mut() else {
            return;
        };
        if find.matches.is_empty() {
            return;
        }
        let len = find.matches.len() as isize;
        find.active = ((find.active as isize + delta).rem_euclid(len)) as usize;
        self.native_docs_refresh_find(cx);
        self.native_docs_reveal_match(window, cx);
    }

    /// Scrolls so the active match sits a third of the way down the document.
    fn native_docs_reveal_match(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
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

    fn native_docs_replace_current(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.native_docs_live_editor() else {
            return;
        };
        let Some(find) = self.native_docs.find.as_ref() else {
            return;
        };
        let Some(range) = find.matches.get(find.active).cloned() else {
            return;
        };
        let replacement = find.replace.read(cx).value().to_string();
        editor.update(cx, |editor, cx| {
            editor.replace_range(range, &replacement, cx)
        });
        self.native_docs_refresh_find(cx);
    }

    fn native_docs_replace_all(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.native_docs_live_editor() else {
            return;
        };
        let Some(find) = self.native_docs.find.as_mut() else {
            return;
        };
        let replacement = find.replace.read(cx).value().to_string();
        let matches = find.matches.clone();
        find.active = 0;
        editor.update(cx, |editor, cx| {
            for range in matches.into_iter().rev() {
                editor.replace_range(range, &replacement, cx);
            }
        });
        self.native_docs_refresh_find(cx);
    }

    /// CDXC:Docs 2026-09-16 DECISION:
    /// User: give the Docs find-and-replace pane the same look and background as the floating formatting bar in light and dark modes so it stands out from the document.
    pub(crate) fn render_native_docs_find(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let find = self.native_docs.find.as_ref().filter(|find| find.visible)?;
        let needle_empty = find.query.read(cx).value().is_empty();
        let status = if needle_empty {
            String::new()
        } else if find.matches.is_empty() {
            "No results".into()
        } else {
            format!("{} of {}", find.active + 1, find.matches.len())
        };
        let no_results = status == "No results";
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
                .absolute()
                .bottom(px(45.0))
                .right(px(0.0))
                .p(px(8.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .rounded(px(10.0))
                .bg(p.floating)
                .border_1()
                .border_color(p.border)
                .shadow_lg()
                .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        this.native_docs_hide_find(window, cx);
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
                                "docs/l-whole-word-2.svg",
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
                                "docs/l-case-sensitive-2.svg",
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
                                "docs/l-chevron-up-2.svg",
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
                                "docs/l-chevron-down-2.svg",
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
                                "docs/l-replace-2.svg",
                                "Replace Current Match",
                                None,
                            )
                            .on_click(
                                cx.listener(|this, _, _, cx| this.native_docs_replace_current(cx)),
                            ),
                        )
                        .child(
                            button(
                                "docs-replace-all",
                                "docs/l-replace-all-2.svg",
                                "Replace All Matches",
                                None,
                            )
                            .on_click(
                                cx.listener(|this, _, _, cx| this.native_docs_replace_all(cx)),
                            ),
                        )
                        .child(div().w(px(24.0)))
                        .child(
                            button("docs-find-close", "docs/l-x-2.svg", "Close Find", None)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.native_docs_hide_find(window, cx)
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}
