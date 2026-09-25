//! The floating formatting bar (the Docs "meo toolbar"): 37px, 10px radius, centred 14px above
//! the bottom of a Markdown document, collapsing to one pill. Its buttons edit the Markdown source
//! around the caret or selection, the same edits the web toolbar made.
//!
//! Adapted from the Docs prototype (`docs/2026-09-24/docs-gpui-editor-research/demo/src/format.rs`).

use std::ops::Range;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Entity, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div, px, svg,
};
use zorite_editor::EditorState;

use super::palette::DocsPalette;
use super::state::{DocsFileKind, DocsMarkdownMode};
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_tooltip;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DocsFormatMenu {
    #[default]
    None,
    Heading,
    Table,
}

/// One button of the bar, in the order it is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocsBarItem {
    Heading,
    Bullet,
    Numbered,
    Task,
    Table,
    Code,
    Link,
    Wiki,
    Image,
    Quote,
    Rule,
    Find,
    Width,
    Lines,
    Git,
}

/// The bar's metrics, the web toolbar's (`M/styles.ts:1354-1535`).
const BUTTON: f32 = 27.0;
const GROUP_GAP: f32 = 8.0;
const BUTTON_GAP: f32 = 1.0;
const SEPARATOR: f32 = 9.0;
const MODE_WIDTH: f32 = 76.0;
/// Padding and border on both sides.
const BAR_CHROME: f32 = 8.0;
/// The bar keeps 14px clear of the document's edges (`max-width: 100% - 28px`).
const BAR_EDGE_ROOM: f32 = 28.0;
/// Hidden buttons come back only once this much more room is free, so the bar does not flicker at
/// the threshold.
const REGROW_SLACK: f32 = 6.0;

impl DocsBarItem {
    const FORMAT_BLOCKS: [Self; 4] = [Self::Heading, Self::Bullet, Self::Numbered, Self::Task];
    const FORMAT_INSERTS: [Self; 7] = [
        Self::Table,
        Self::Code,
        Self::Link,
        Self::Wiki,
        Self::Image,
        Self::Quote,
        Self::Rule,
    ];
    const VIEW: [Self; 4] = [Self::Find, Self::Width, Self::Lines, Self::Git];

    /// CDXC:Docs 2026-09-25 DECISION:
    /// User: the formatting bar fits a document narrower than the bar, and the buttons that do not fit go into a "⋯" menu at the end of the bar (option b of three: fit like the web toolbar and keep the hidden buttons reachable). The web toolbar's order is kept: the three optional view toggles go first, then the formatting buttons from the end; Find, Live/Source and the collapse toggle always stay.
    const OVERFLOW_ORDER: [Self; 14] = [
        Self::Width,
        Self::Lines,
        Self::Git,
        Self::Rule,
        Self::Quote,
        Self::Image,
        Self::Wiki,
        Self::Link,
        Self::Code,
        Self::Table,
        Self::Task,
        Self::Numbered,
        Self::Bullet,
        Self::Heading,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Heading => "docs-fmt-heading",
            Self::Bullet => "docs-fmt-bullet",
            Self::Numbered => "docs-fmt-numbered",
            Self::Task => "docs-fmt-task",
            Self::Table => "docs-fmt-table",
            Self::Code => "docs-fmt-code",
            Self::Link => "docs-fmt-link",
            Self::Wiki => "docs-fmt-wiki",
            Self::Image => "docs-fmt-image",
            Self::Quote => "docs-fmt-quote",
            Self::Rule => "docs-fmt-rule",
            Self::Find => "docs-fmt-find",
            Self::Width => "docs-fmt-width",
            Self::Lines => "docs-fmt-lines",
            Self::Git => "docs-fmt-git",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Heading => "docs/l-heading-17.svg",
            Self::Bullet => "docs/l-list-17.svg",
            Self::Numbered => "docs/l-list-ordered-17.svg",
            Self::Task => "docs/l-list-todo-17.svg",
            Self::Table => "docs/l-table-2-17.svg",
            Self::Code => "docs/l-code-17.svg",
            Self::Link => "docs/l-link-17.svg",
            Self::Wiki => "docs/l-brackets-17.svg",
            Self::Image => "docs/l-image-17.svg",
            Self::Quote => "docs/l-quote-17.svg",
            Self::Rule => "docs/l-minus-17.svg",
            Self::Find => "docs/l-search-17.svg",
            Self::Width => "docs/l-panel-left-right-dashed-17.svg",
            Self::Lines => "docs/l-hash-17.svg",
            Self::Git => "docs/l-git-compare-17.svg",
        }
    }

    /// The name the action carries through a menu.
    pub(crate) fn command(self) -> &'static str {
        &self.id()["docs-fmt-".len()..]
    }

    pub(crate) fn from_command(command: &str) -> Option<Self> {
        Self::FORMAT_BLOCKS
            .into_iter()
            .chain(Self::FORMAT_INSERTS)
            .chain(Self::VIEW)
            .find(|item| item.command() == command)
    }
}

/// The bar's full width with `hidden` left out and, when anything is, the "⋯" button added.
fn bar_width(hidden: &[DocsBarItem]) -> f32 {
    let shown = |items: &[DocsBarItem]| items.iter().filter(|item| !hidden.contains(item)).count();
    let group = |buttons: usize, extra: f32, extra_children: usize| {
        let children = buttons + extra_children;
        buttons as f32 * BUTTON + extra + children.saturating_sub(1) as f32 * BUTTON_GAP
    };
    let (blocks, inserts) = (
        shown(&DocsBarItem::FORMAT_BLOCKS),
        shown(&DocsBarItem::FORMAT_INSERTS),
    );
    let separator = blocks > 0 && inserts > 0;
    let mut children: Vec<f32> = Vec::new();
    if blocks + inserts > 0 {
        children.push(group(
            blocks + inserts,
            if separator { SEPARATOR } else { 0.0 },
            usize::from(separator),
        ));
    }
    children.push(group(shown(&DocsBarItem::VIEW), 0.0, 0));
    children.push(MODE_WIDTH);
    if !hidden.is_empty() {
        children.push(BUTTON);
    }
    children.push(BUTTON);
    BAR_CHROME + children.iter().sum::<f32>() + (children.len() - 1) as f32 * GROUP_GAP
}

/// How many buttons (from the front of `OVERFLOW_ORDER`) to hide in `room`, given how many are
/// hidden now.
fn overflow_count(room: f32, hidden_now: usize) -> usize {
    let order = DocsBarItem::OVERFLOW_ORDER;
    let fitting = |slack: f32| {
        (0..=order.len())
            .find(|&count| bar_width(&order[..count]) + slack <= room)
            .unwrap_or(order.len())
    };
    let shrink = fitting(0.0);
    if shrink > hidden_now {
        return shrink;
    }
    fitting(REGROW_SLACK).min(hidden_now)
}

thread_local! {
    /// The "⋯" button's bounds in each window that draws it, for its menu to open under.
    static MORE_ANCHORS: std::cell::RefCell<Vec<(gpui::WindowId, gpui::Bounds<gpui::Pixels>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

impl GhostexGpuiApp {
    /// Hands `f` back when `window` is the main window; otherwise runs the edit there instead.
    fn native_docs_take_for_main_window<F>(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
        f: F,
    ) -> Option<F>
    where
        F: FnOnce(&mut EditorState, &mut Context<EditorState>) + 'static,
    {
        let slot = std::rc::Rc::new(std::cell::Cell::new(Some(f)));
        let deferred = slot.clone();
        let moved = self.native_docs_defer_to_main_window(window, cx, move |this, window, cx| {
            if let Some(f) = deferred.take() {
                this.native_docs_edit(window, cx, f);
            }
        });
        if moved { None } else { slot.take() }
    }

    /// Runs a source edit on the open live editor, then gives it focus back.
    fn native_docs_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &mut Context<EditorState>) + 'static,
    ) {
        // A click in the bar's frosted window edits and focuses the editor in the main window.
        let Some(f) = self.native_docs_take_for_main_window(window, cx, f) else {
            return;
        };
        let Some(editor) = self
            .native_docs
            .active_document()
            .and_then(|document| document.live.clone())
        else {
            return;
        };
        editor.update(cx, |editor, cx| {
            f(editor, cx);
            editor.focus(window, cx);
        });
    }

    /// CDXC:Docs 2026-09-15 DECISION:
    /// User: the formatting bar floats near the bottom of the document with padding and rounded corners, so the Docs view no longer carries a tall stacked header, and it collapses to a single pill. The collapsed state is remembered across documents and restarts; Find keeps working from the pill because the panel stays mounted inside the bar.
    pub(crate) fn render_native_docs_format_bar(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let frosted = super::format_bar_window::format_bar_frosted(p);
        let room = self.native_docs.format_bar_room.get();
        if room > 0.0 {
            self.native_docs.format_bar_hidden =
                overflow_count(room - BAR_EDGE_ROOM, self.native_docs.format_bar_hidden);
        }
        let row = self.native_docs_format_bar_row(p, None, cx)?;
        let collapsed = self.native_docs.format_bar_collapsed;
        let menu = self.native_docs.format_menu;
        // Under glass the row itself is drawn in its frosted window (`format_bar_window.rs`); this
        // window keeps an invisible twin of it, which holds the row's place, reports where it is,
        // and anchors the menus and the find panel that open above it.
        let shell = if frosted {
            div()
                .relative()
                .flex()
                .child(row.invisible())
                .child(self.native_docs_format_bar_reporter(cx))
        } else {
            div().relative().flex().child(row)
        };
        let shell = shell
            .when(menu == DocsFormatMenu::Heading && !collapsed, |bar| {
                bar.child(self.render_native_docs_heading_menu(p, cx))
            })
            .when(menu == DocsFormatMenu::Table && !collapsed, |bar| {
                bar.child(self.render_native_docs_table_menu(p, cx))
            })
            .children(self.render_native_docs_find(p, cx));
        Some(
            div()
                .absolute()
                .bottom(px(14.0))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(shell)
                .child(self.native_docs_format_bar_room_probe())
                .into_any_element(),
        )
    }

    /// Measures the width the bar may use (the document's), redrawing when it changes so the
    /// bar can move buttons into or out of its menu.
    fn native_docs_format_bar_room_probe(&self) -> AnyElement {
        let room = self.native_docs.format_bar_room.clone();
        gpui::canvas(
            move |bounds, window, _| {
                let width = bounds.size.width.as_f32();
                if room.replace(width) != width {
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    /// The bar's own row of buttons, drawn in the Docs view or, under glass, in the bar's frosted
    /// window with `frosted` as its tint and border.
    pub(crate) fn native_docs_format_bar_row(
        &mut self,
        p: &DocsPalette,
        frosted: Option<(gpui::Hsla, gpui::Hsla)>,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        let document = self.native_docs.active_document()?;
        if document.kind != DocsFileKind::Markdown || document.live.is_none() {
            return None;
        }
        let live = document.mode == DocsMarkdownMode::Live;
        let path = document.path.clone();
        let collapsed = self.native_docs.format_bar_collapsed;
        let menu = self.native_docs.format_menu;
        let find_visible = self.native_docs_find_visible();
        let (constrain, numbers, git) = (
            self.native_docs.constrain_width,
            self.native_docs.line_numbers,
            self.native_docs.git_changes,
        );
        let (hover, text, muted, surface) = (p.control_hover, p.text, p.muted, p.row_surface);
        let button =
            move |id: &'static str, icon: &'static str, tooltip: &'static str, active: bool| {
                div()
                    .id(id)
                    .size(px(27.0))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .when(active, |b| b.bg(surface))
                    .hover(move |b| b.bg(hover))
                    .child(svg().path(icon).size(px(18.0)).text_color(if active {
                        text
                    } else {
                        muted
                    }))
                    .tooltip(move |window, cx| titlebar_tooltip(tooltip, window, cx))
            };
        let separator = || {
            div()
                .w(px(1.0))
                .h(px(18.0))
                .mx(px(4.0))
                .flex_none()
                .bg(p.border)
        };
        let toggle = button(
            "docs-bar-toggle",
            if collapsed {
                "docs/l-type-17.svg"
            } else {
                "docs/l-chevron-down-17.svg"
            },
            if collapsed {
                "Show formatting bar"
            } else {
                "Hide formatting bar"
            },
            false,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.native_docs.format_bar_collapsed = !this.native_docs.format_bar_collapsed;
            this.native_docs.format_menu = DocsFormatMenu::None;
            this.native_docs_persist_format_bar(cx);
            this.native_docs_notify(cx);
        }));
        let shell = div()
            .id("native-docs-format-bar")
            .relative()
            .h(px(37.0))
            .p(px(3.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(10.0))
            .border_1()
            .map(|shell| match frosted {
                Some((tint, border)) => shell.bg(tint).border_color(border),
                None => shell.bg(p.floating).border_color(p.border).shadow_lg(),
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let shell = if collapsed {
            shell.child(toggle)
        } else {
            let hidden = &DocsBarItem::OVERFLOW_ORDER[..self.native_docs.format_bar_hidden];
            let shown = |items: &[DocsBarItem]| -> Vec<DocsBarItem> {
                items
                    .iter()
                    .copied()
                    .filter(|item| !hidden.contains(item))
                    .collect()
            };
            let (blocks, inserts, view) = (
                shown(&DocsBarItem::FORMAT_BLOCKS),
                shown(&DocsBarItem::FORMAT_INSERTS),
                shown(&DocsBarItem::VIEW),
            );
            let item_button = |item: DocsBarItem| {
                let active = match item {
                    DocsBarItem::Heading => menu == DocsFormatMenu::Heading,
                    DocsBarItem::Table => menu == DocsFormatMenu::Table,
                    DocsBarItem::Find => find_visible,
                    DocsBarItem::Width => constrain,
                    DocsBarItem::Lines => numbers,
                    DocsBarItem::Git => git,
                    _ => false,
                };
                button(
                    item.id(),
                    item.icon(),
                    bar_item_label(item, constrain, numbers, git),
                    active,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.native_docs_run_bar_item(item, None, window, cx)
                }))
            };
            let format_group = (!blocks.is_empty() || !inserts.is_empty()).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(BUTTON_GAP))
                    .children(blocks.iter().map(|item| item_button(*item)))
                    .when(!blocks.is_empty() && !inserts.is_empty(), |group| {
                        group.child(separator())
                    })
                    .children(inserts.iter().map(|item| item_button(*item)))
            });
            let right_group = div()
                .flex()
                .items_center()
                .gap(px(BUTTON_GAP))
                .children(view.iter().map(|item| item_button(*item)));
            let more = (!hidden.is_empty()).then(|| {
                button("docs-fmt-more", "titlebar/dots.svg", "More", false)
                    .relative()
                    .child(
                        gpui::canvas(
                            |bounds, window, _| {
                                let id = window.window_handle().window_id();
                                MORE_ANCHORS.with(|anchors| {
                                    let mut anchors = anchors.borrow_mut();
                                    anchors.retain(|(window, _)| *window != id);
                                    anchors.push((id, bounds));
                                });
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.native_docs_show_bar_overflow_menu(window, cx)
                    }))
            });
            let mode = div()
                .id("docs-fmt-mode")
                .h(px(27.0))
                .min_w(px(MODE_WIDTH))
                .px(px(8.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .bg(p.row_surface)
                .border_1()
                .border_color(p.border_strong)
                .text_size(px(12.0))
                .text_color(p.text)
                .cursor_pointer()
                .child(if live { "Live" } else { "Source" })
                .tooltip(move |window, cx| {
                    titlebar_tooltip(
                        if live {
                            "Switch to Source"
                        } else {
                            "Switch to Live"
                        },
                        window,
                        cx,
                    )
                })
                .on_click(
                    cx.listener(move |this, _, _, cx| this.native_docs_toggle_live(&path, cx)),
                );
            shell
                .children(format_group)
                .child(right_group)
                .child(mode)
                .children(more)
                .child(toggle)
        };
        Some(shell)
    }

    /// A bar button's click, from the bar or from its "⋯" menu (`from_menu`). The menu runs the
    /// button's plain action: a heading level (`level`) or the default 3 by 2 table, since the
    /// pickers those buttons open hang from the bar.
    pub(crate) fn native_docs_run_bar_item(
        &mut self,
        item: DocsBarItem,
        from_menu: Option<u8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let toggle_menu = |this: &mut Self, which: DocsFormatMenu, cx: &mut Context<Self>| {
            this.native_docs.format_menu = if this.native_docs.format_menu == which {
                DocsFormatMenu::None
            } else {
                which
            };
            this.native_docs.table_hover = (0, 0);
            this.native_docs_notify(cx);
        };
        match item {
            DocsBarItem::Heading => match from_menu {
                Some(level) => {
                    self.native_docs_edit(window, cx, move |e, cx| set_heading(e, level, cx))
                }
                None => toggle_menu(self, DocsFormatMenu::Heading, cx),
            },
            DocsBarItem::Table => match from_menu {
                Some(_) => self.native_docs_edit(window, cx, |e, cx| insert_table(e, 3, 2, cx)),
                None => toggle_menu(self, DocsFormatMenu::Table, cx),
            },
            DocsBarItem::Bullet => {
                self.native_docs_edit(window, cx, |e, cx| toggle_list(e, ListKind::Bullet, cx))
            }
            DocsBarItem::Numbered => {
                self.native_docs_edit(window, cx, |e, cx| toggle_list(e, ListKind::Numbered, cx))
            }
            DocsBarItem::Task => {
                self.native_docs_edit(window, cx, |e, cx| toggle_list(e, ListKind::Task, cx))
            }
            DocsBarItem::Code => self.native_docs_edit(window, cx, code_block),
            DocsBarItem::Link => {
                self.native_docs_edit(window, cx, |e, cx| wrap_link(e, LinkKind::Link, cx))
            }
            DocsBarItem::Wiki => {
                self.native_docs_edit(window, cx, |e, cx| wrap_link(e, LinkKind::Wiki, cx))
            }
            DocsBarItem::Image => {
                self.native_docs_edit(window, cx, |e, cx| wrap_link(e, LinkKind::Image, cx))
            }
            DocsBarItem::Quote => self.native_docs_edit(window, cx, toggle_quote),
            DocsBarItem::Rule => self.native_docs_edit(window, cx, horizontal_rule),
            DocsBarItem::Find => {
                let toggle = |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                    if this.native_docs_find_visible() {
                        this.native_docs_hide_find(window, cx);
                    } else {
                        this.native_docs_show_find(window, cx);
                    }
                };
                if !self.native_docs_defer_to_main_window(window, cx, toggle) {
                    toggle(self, window, cx);
                }
            }
            DocsBarItem::Width => {
                self.native_docs.constrain_width = !self.native_docs.constrain_width;
                self.native_docs_notify(cx);
            }
            DocsBarItem::Lines => {
                self.native_docs.line_numbers = !self.native_docs.line_numbers;
                self.native_docs_notify(cx);
            }
            DocsBarItem::Git => {
                self.native_docs.git_changes = !self.native_docs.git_changes;
                self.native_docs_notify(cx);
            }
        }
    }

    /// The "⋯" menu: the buttons the bar has no room for, in the bar's order.
    fn native_docs_show_bar_overflow_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = window.window_handle().window_id();
        let Some(trigger) = MORE_ANCHORS.with(|anchors| {
            anchors
                .borrow()
                .iter()
                .find(|(window, _)| *window == id)
                .map(|(_, bounds)| *bounds)
        }) else {
            return;
        };
        let hidden = &DocsBarItem::OVERFLOW_ORDER[..self.native_docs.format_bar_hidden];
        let (constrain, numbers, git) = (
            self.native_docs.constrain_width,
            self.native_docs.line_numbers,
            self.native_docs.git_changes,
        );
        let action = |item: DocsBarItem, level: Option<u8>| -> Box<dyn gpui::Action> {
            Box::new(super::actions::NativeDocsAction {
                command: serde_json::json!({
                    "type": "barItem",
                    "item": item.command(),
                    "level": level.unwrap_or(1),
                }),
            })
        };
        let mut menu = crate::app::context_menu::GpuiContextMenu::new();
        let mut formatting = false;
        for item in DocsBarItem::FORMAT_BLOCKS
            .into_iter()
            .chain(DocsBarItem::FORMAT_INSERTS)
            .filter(|item| hidden.contains(item))
        {
            formatting = true;
            menu = if item == DocsBarItem::Heading {
                menu.submenu_with_icon(
                    "Heading",
                    Some(item.icon()),
                    (1..=6u8)
                        .map(|level| (format!("Heading {level}").into(), action(item, Some(level))))
                        .collect(),
                )
            } else {
                menu.menu_with_icon(
                    bar_item_label(item, constrain, numbers, git),
                    item.icon(),
                    false,
                    action(item, None),
                )
            };
        }
        let view: Vec<DocsBarItem> = DocsBarItem::VIEW
            .into_iter()
            .filter(|item| hidden.contains(item))
            .collect();
        if formatting && !view.is_empty() {
            menu = menu.separator();
        }
        for item in view {
            menu = menu.menu_with_icon(
                bar_item_label(item, constrain, numbers, git),
                item.icon(),
                false,
                action(item, None),
            );
        }
        self.native_docs_show_menu(menu, trigger, true, window, cx);
    }

    fn render_native_docs_heading_menu(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const ICONS: [&str; 6] = [
            "docs/l-heading-1-17.svg",
            "docs/l-heading-2-17.svg",
            "docs/l-heading-3-17.svg",
            "docs/l-heading-4-17.svg",
            "docs/l-heading-5-17.svg",
            "docs/l-heading-6-17.svg",
        ];
        let (hover, text) = (p.control_hover, p.text);
        popover(p)
            .flex()
            .flex_col()
            .gap(px(3.0))
            .w(px(170.0))
            .children((1..=6u8).map(|level| {
                div()
                    .id(("docs-heading-level", level as usize))
                    .h(px(30.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .text_color(text.opacity(0.88))
                    .hover(move |row| row.bg(hover))
                    .child(
                        svg()
                            .path(ICONS[level as usize - 1])
                            .size(px(15.0))
                            .text_color(text.opacity(0.72)),
                    )
                    .child(format!("Heading {level}"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.native_docs.format_menu = DocsFormatMenu::None;
                        this.native_docs_edit(window, cx, move |e, cx| set_heading(e, level, cx));
                    }))
            }))
            .into_any_element()
    }

    fn render_native_docs_table_menu(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (hover_c, hover_r) = self.native_docs.table_hover;
        let label = if hover_c == 0 {
            "Insert table".to_string()
        } else {
            format!("{hover_c} x {hover_r}")
        };
        let text = p.text;
        let mut grid = div().flex().flex_col().gap(px(2.0));
        for r in 1..=5usize {
            let mut row = div().flex().gap(px(2.0));
            for c in 1..=5usize {
                let lit = c <= hover_c && r <= hover_r;
                row = row.child(
                    div()
                        .id(("docs-table-cell", r * 10 + c))
                        .size(px(16.0))
                        .rounded(px(3.0))
                        .border_1()
                        .border_color(text.opacity(if lit { 0.5 } else { 0.16 }))
                        .when(lit, |cell| cell.bg(text.opacity(0.14)))
                        .cursor_pointer()
                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                            if *hovered {
                                this.native_docs.table_hover = (c, r);
                                this.native_docs_notify(cx);
                            }
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.native_docs.format_menu = DocsFormatMenu::None;
                            this.native_docs_edit(window, cx, move |e, cx| {
                                insert_table(e, c, r, cx)
                            });
                        })),
                );
            }
            grid = grid.child(row);
        }
        popover(p)
            .left(px(100.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(grid)
            .child(div().text_size(px(11.0)).text_color(p.muted).child(label))
            .into_any_element()
    }
}

fn popover(p: &DocsPalette) -> gpui::Div {
    div()
        .absolute()
        .bottom(px(43.0))
        .left(px(0.0))
        .p(px(6.0))
        .rounded(px(8.0))
        .bg(p.raised)
        .border_1()
        .border_color(p.border_strong)
        .shadow_lg()
}

// ---- source edits -------------------------------------------------------------------------

/// Byte range of the whole lines the selection touches (without the final newline).
fn line_span(text: &str, selection: &Range<usize>) -> Range<usize> {
    let start = text[..selection.start].rfind('\n').map_or(0, |i| i + 1);
    let end_probe = if selection.end > selection.start && text[..selection.end].ends_with('\n') {
        selection.end - 1
    } else {
        selection.end
    };
    let end = text[end_probe..]
        .find('\n')
        .map_or(text.len(), |i| end_probe + i);
    start..end.max(start)
}

/// Replaces every line in the selection with `f(line, index)`, keeping the lines selected.
fn map_lines(
    editor: &mut EditorState,
    cx: &mut Context<EditorState>,
    f: impl Fn(&str, usize) -> String,
) {
    let text = editor.text().to_string();
    let selection = editor.selected_range();
    let span = line_span(&text, &selection);
    let mapped = text[span.clone()]
        .split('\n')
        .enumerate()
        .map(|(i, line)| f(line, i))
        .collect::<Vec<_>>()
        .join("\n");
    let new_end = span.start + mapped.len();
    editor.replace_range(span.clone(), &mapped, cx);
    if selection.is_empty() {
        editor.set_cursor(new_end, cx);
    } else {
        editor.select_range(span.start..new_end, cx);
    }
}

fn strip_heading(line: &str) -> (usize, &str) {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        (hashes, &line[hashes + 1..])
    } else {
        (0, line)
    }
}

fn set_heading(editor: &mut EditorState, level: u8, cx: &mut Context<EditorState>) {
    map_lines(editor, cx, |line, _| {
        let (current, rest) = strip_heading(line);
        if current == level as usize {
            rest.to_string()
        } else {
            format!("{} {rest}", "#".repeat(level as usize))
        }
    });
}

#[derive(Clone, Copy, PartialEq)]
enum ListKind {
    Bullet,
    Numbered,
    Task,
}

fn strip_list(line: &str) -> (Option<ListKind>, &str, &str) {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, body) = line.split_at(indent_len);
    for prefix in ["- [ ] ", "- [x] ", "- [X] "] {
        if let Some(rest) = body.strip_prefix(prefix) {
            return (Some(ListKind::Task), indent, rest);
        }
    }
    for prefix in ["- ", "* ", "+ "] {
        if let Some(rest) = body.strip_prefix(prefix) {
            return (Some(ListKind::Bullet), indent, rest);
        }
    }
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && body[digits..].starts_with(". ") {
        return (Some(ListKind::Numbered), indent, &body[digits + 2..]);
    }
    (None, indent, body)
}

fn toggle_list(editor: &mut EditorState, kind: ListKind, cx: &mut Context<EditorState>) {
    let text = editor.text().to_string();
    let span = line_span(&text, &editor.selected_range());
    let all_same = text[span]
        .split('\n')
        .all(|line| strip_list(line).0 == Some(kind));
    map_lines(editor, cx, |line, index| {
        let (_, indent, body) = strip_list(line);
        if all_same {
            format!("{indent}{body}")
        } else {
            match kind {
                ListKind::Bullet => format!("{indent}- {body}"),
                ListKind::Numbered => format!("{indent}{}. {body}", index + 1),
                ListKind::Task => format!("{indent}- [ ] {body}"),
            }
        }
    });
}

fn toggle_quote(editor: &mut EditorState, cx: &mut Context<EditorState>) {
    let text = editor.text().to_string();
    let span = line_span(&text, &editor.selected_range());
    let all_quoted = text[span].split('\n').all(|line| line.starts_with('>'));
    map_lines(editor, cx, |line, _| {
        if all_quoted {
            line.strip_prefix("> ")
                .or_else(|| line.strip_prefix('>'))
                .unwrap_or(line)
                .to_string()
        } else {
            format!("> {line}")
        }
    });
}

fn code_block(editor: &mut EditorState, cx: &mut Context<EditorState>) {
    let text = editor.text().to_string();
    let selection = editor.selected_range();
    if selection.is_empty() {
        let line = line_span(&text, &selection);
        let at = line.end;
        let lead = if text[line].trim().is_empty() {
            ""
        } else {
            "\n"
        };
        let insert = format!("{lead}```\n\n```");
        editor.replace_range(at..at, &insert, cx);
        editor.set_cursor(at + lead.len() + 4, cx);
    } else {
        let span = line_span(&text, &selection);
        let wrapped = format!("```\n{}\n```", &text[span.clone()]);
        editor.replace_range(span.clone(), &wrapped, cx);
        editor.set_cursor(span.start + 3, cx);
    }
}

#[derive(Clone, Copy)]
enum LinkKind {
    Link,
    Wiki,
    Image,
}

fn wrap_link(editor: &mut EditorState, kind: LinkKind, cx: &mut Context<EditorState>) {
    let selection = editor.selected_range();
    let label = editor.text()[selection.clone()].to_string();
    let (insert, select) = match kind {
        LinkKind::Link => {
            let insert = format!("[{label}](url)");
            let url = insert.len() - 4..insert.len() - 1;
            (insert, if label.is_empty() { 1..1 } else { url })
        }
        LinkKind::Wiki => {
            let insert = format!("[[{label}]]");
            let caret = 2 + label.len();
            (insert, caret..caret)
        }
        LinkKind::Image => {
            let insert = format!("![{label}](path)");
            let path = insert.len() - 5..insert.len() - 1;
            (insert, path)
        }
    };
    editor.replace_range(selection.clone(), &insert, cx);
    editor.select_range(
        selection.start + select.start..selection.start + select.end,
        cx,
    );
}

fn horizontal_rule(editor: &mut EditorState, cx: &mut Context<EditorState>) {
    let text = editor.text().to_string();
    let line = line_span(&text, &editor.selected_range());
    let at = line.end;
    let insert = if text[line].trim().is_empty() {
        "---\n".to_string()
    } else {
        "\n\n---\n".to_string()
    };
    editor.replace_range(at..at, &insert, cx);
    editor.set_cursor(at + insert.len(), cx);
}

fn insert_table(
    editor: &mut EditorState,
    columns: usize,
    rows: usize,
    cx: &mut Context<EditorState>,
) {
    let text = editor.text().to_string();
    let line = line_span(&text, &editor.selected_range());
    let at = line.end;
    let header = (1..=columns)
        .map(|c| format!("Column {c}"))
        .collect::<Vec<_>>()
        .join(" | ");
    let rule = vec!["---"; columns].join(" | ");
    let body = (0..rows.saturating_sub(1).max(1))
        .map(|_| vec!["   "; columns].join(" | "))
        .map(|row| format!("| {row} |"))
        .collect::<Vec<_>>()
        .join("\n");
    let lead = if text[line].trim().is_empty() {
        ""
    } else {
        "\n\n"
    };
    let insert = format!("{lead}| {header} |\n| {rule} |\n{body}\n");
    editor.replace_range(at..at, &insert, cx);
    editor.set_cursor(at + lead.len() + 2, cx);
}

/// Wraps the live editor's selection in inline markers (the selection toolbar's formatting mode),
/// or removes them when the selection already has them.
pub(crate) fn wrap_inline(
    editor: &Entity<EditorState>,
    before: &str,
    after: &str,
    cx: &mut gpui::App,
) {
    editor.update(cx, |editor, cx| {
        let selection = editor.selected_range();
        let selected = editor.text()[selection.clone()].to_string();
        let replacement = if selected.len() >= before.len() + after.len()
            && selected.starts_with(before)
            && selected.ends_with(after)
        {
            selected[before.len()..selected.len() - after.len()].to_string()
        } else {
            format!("{before}{selected}{after}")
        };
        let len = replacement.len();
        editor.replace_range(selection.clone(), &replacement, cx);
        editor.select_range(selection.start..selection.start + len, cx);
    });
}

/// A bar button's tooltip, and its row in the "⋯" menu.
fn bar_item_label(item: DocsBarItem, constrain: bool, numbers: bool, git: bool) -> &'static str {
    match item {
        DocsBarItem::Heading => "Heading",
        DocsBarItem::Bullet => "Bullet List",
        DocsBarItem::Numbered => "Numbered List",
        DocsBarItem::Task => "Task",
        DocsBarItem::Table => "Table",
        DocsBarItem::Code => "Code Block",
        DocsBarItem::Link => "Link",
        DocsBarItem::Wiki => "Wiki Link",
        DocsBarItem::Image => "Image",
        DocsBarItem::Quote => "Quote",
        DocsBarItem::Rule => "Horizontal Rule",
        DocsBarItem::Find => "Find and Replace",
        DocsBarItem::Width if constrain => "Use Full Content Width",
        DocsBarItem::Width => "Constrain Content Width",
        DocsBarItem::Lines if numbers => "Hide Line Numbers",
        DocsBarItem::Lines => "Show Line Numbers",
        DocsBarItem::Git if git => "Hide Git Changes",
        DocsBarItem::Git => "Show Git Changes",
    }
}
