//! Files and links: the floating menu above the chat box that lists every attachment, file and link
//! the session mentioned (a coordinator's threads included), opened from its More actions row or
//! its hotkey (CDXC:FilesAndLinks 2026-10-15 in actions.rs). It is a chat menu window
//! (option_menu/pill_menu.rs), like the working row's pill menus. The core
//! owns what is listed, the sections, the filter and where a jump lands
//! (`packages/gx-chat-core/src/composer/files_links.rs`); this draws it, opens a row's item and
//! scrolls the transcript once per jump.

use super::side_question::{answering_placeholder, side_button};
use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    state::NativeChatView,
    transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px, svg,
};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::{Value, json};

/// The menu-toggle key of the Files and links menu (menu_toggle.rs, chat_pills.rs).
pub(super) const FILES_LINKS_TRIGGER: &str = "chat-files-links";

/// The menu's blocks, in unscaled pixels: fixed, so the menu knows its size before it opens.
const ROW_HEIGHT: f32 = 40.0;
const ROW_GAP: f32 = 2.0;
const LINE_HEIGHT: f32 = 22.0;
const SECTION_GAP: f32 = 10.0;
const GAP: f32 = 8.0;
const FILTER_HEIGHT: f32 = 30.0;
const PLACEHOLDER_HEIGHT: f32 = 56.0;
const FOOTER_HEIGHT: f32 = 28.0;

fn row_icon(icon: &str) -> &'static str {
    match icon {
        "photo" => "titlebar/photo.svg",
        "folder" => "titlebar/folder.svg",
        "link" => "titlebar/link.svg",
        _ => "titlebar/file.svg",
    }
}

impl NativeChatView {
    /// Scrolls the transcript to the row a jump chose, once per click (the core bumps
    /// `revision`).
    pub(super) fn sync_files_links_jump(&mut self) {
        let jump = &self.snapshot["filesLinks"]["jump"];
        let Some(revision) = jump["revision"].as_u64() else {
            self.files_links_jump_revision = 0;
            return;
        };
        if revision == self.files_links_jump_revision {
            return;
        }
        self.files_links_jump_revision = revision;
        let Some(item) = jump["item"].as_u64() else {
            return;
        };
        self.release_answer_hold();
        // `scroll_to` (not `scroll_to_reveal_item`) also stops a chat at the bottom from following
        // its tail straight back (search.rs, `sync_search_scroll`).
        self.list.scroll_to(gpui::ListOffset {
            item_ix: item as usize,
            offset_in_item: px(0.0),
        });
    }

    /// The filter field exists while the open card is long enough to offer it.
    fn sync_files_links_input(
        &mut self,
        wanted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let here = window.window_handle().window_id();
        if !wanted {
            self.files_links_input = None;
            self.files_links_subscription = None;
            self.files_links_input_window = None;
            return;
        }
        if self.files_links_input.is_some() && self.files_links_input_window == Some(here) {
            return;
        }
        self.files_links_input_window = Some(here);
        let query = text(&self.snapshot["filesLinks"], "query");
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("Filter by name, path or link");
            state.set_value(query, window, cx);
            state
        });
        self.files_links_subscription =
            Some(
                cx.subscribe_in(&input, window, |chat, input, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let query = input.read(cx).value().to_string();
                        chat.invoke(json!({"type":"filesLinksFilter","text":query}), cx);
                    }
                }),
            );
        self.files_links_input = Some(input);
    }

    /// One row: its icon, name, tag and where it lives, with the jumps on the right. A click opens
    /// the item: a picture in the image viewer (with the group's other pictures beside it), anything
    /// else the way the transcript's own pill or link opens it.
    fn files_links_row(
        &self,
        row: &Value,
        gallery: &[Value],
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(row, "key");
        let label = text(row, "label");
        let detail = text(row, "detail");
        let meta = text(row, "meta");
        let open = row["open"].clone();
        let image = row["image"].clone();
        let gallery: Vec<Value> = gallery.to_vec();
        let index = gallery
            .iter()
            .position(|other| *other == image)
            .unwrap_or(0);
        let jumps: Vec<AnyElement> = row["jumps"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(position, jump)| {
                let label = text(jump, "label");
                let tooltip = text(jump, "tooltip");
                let command = jump["command"].clone();
                div()
                    .id(SharedString::from(format!(
                        "files-links-jump:{key}:{position}"
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(format!("{label}: {tooltip}"))
                    .flex_shrink_0()
                    .px(px(7.0 * s))
                    .py(px(2.0 * s))
                    .rounded(px(5.0 * s))
                    .border_1()
                    .chat_filled(p.neutral_button_fills(), true)
                    .text_size(px(11.0 * s))
                    .text_color(p.primary)
                    .chat_cursor_pointer()
                    .child(label)
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
                    .into_any_element()
            })
            .collect();
        // Copy Path and the folder row (CDXC:SessionChat 2026-10-17 in file_actions.rs).
        let path = row["path"].as_str().unwrap_or_default().to_string();
        let mut path_actions: Vec<AnyElement> = Vec::new();
        if !path.is_empty() {
            let folder = row["icon"] == "folder";
            let mut actions = vec![("copy", "titlebar/copy.svg", "Copy Path".to_string())];
            if self.file_locate_available() {
                actions.push((
                    "locate",
                    "titlebar/folder-open.svg",
                    if folder {
                        "Open Folder Location".to_string()
                    } else {
                        "Open File Location".to_string()
                    },
                ));
            }
            for (action, icon, tooltip) in actions {
                let path = path.clone();
                let label = tooltip.clone();
                path_actions.push(
                    div()
                        .id(SharedString::from(format!("files-links-{action}:{key}")))
                        .role(gpui::Role::Button)
                        .aria_label(label)
                        .flex_shrink_0()
                        .size(px(22.0 * s))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.0 * s))
                        .chat_filled(p.neutral_button_fills(), true)
                        .chat_cursor_pointer()
                        .child(svg().path(icon).size(px(13.0 * s)).text_color(p.muted))
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if action == "copy" {
                                this.copy_file_path(&path, cx);
                            } else {
                                this.locate_file(&path, cx);
                            }
                        }))
                        .into_any_element(),
                );
            }
        }
        div()
            .id(SharedString::from(format!("files-links-row:{key}")))
            .role(gpui::Role::Button)
            .aria_label(format!("Open {label}"))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .h(px(ROW_HEIGHT * s))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .px(px(6.0 * s))
            .rounded(px(6.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.border.opacity(0.6)))
            .child(
                svg()
                    .path(row_icon(&text(row, "icon")))
                    .flex_shrink_0()
                    .size(px(15.0 * s))
                    .text_color(p.muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(6.0 * s))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(13.0 * s))
                                    .text_color(p.foreground)
                                    .child(label.clone()),
                            )
                            .when_some(row["tag"].as_str().map(str::to_string), |this, tag| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .px(px(5.0 * s))
                                        .rounded(px(4.0 * s))
                                        .bg(p.border)
                                        .text_size(px(10.0 * s))
                                        .text_color(p.muted)
                                        .child(tag),
                                )
                            }),
                    )
                    // Where it lives and when it was mentioned share one line, so a long list
                    // shows more rows; the path gives way first.
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(8.0 * s))
                            .text_size(px(11.5 * s))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(p.muted)
                                    .child(detail.clone()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(11.0 * s))
                                    .text_color(p.muted.opacity(0.75))
                                    .child(meta),
                            ),
                    ),
            )
            .children(jumps)
            .children(path_actions)
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(detail.clone()).build(window, cx)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if image.is_object() && image["transport"] != "none" {
                    this.open_image_viewer(gallery.clone(), index, cx);
                } else {
                    this.invoke(open.clone(), cx);
                }
            }))
            .into_any_element()
    }

    /// Opens the floating menu once the core says the list is open (the More actions row, the
    /// hotkey), and not again while a close it sent is on its way.
    pub(super) fn sync_files_links_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.snapshot["filesLinks"]["open"] == true;
        if !open {
            self.files_links_closing = false;
            return;
        }
        let shown = self
            .option_menu
            .as_ref()
            .is_some_and(|menu| menu.read(cx).pill() == Some(FILES_LINKS_TRIGGER));
        if shown || self.files_links_menu_pending || self.files_links_closing {
            return;
        }
        self.files_links_menu_pending = true;
        cx.defer_in(window, |this, window, cx| {
            this.files_links_menu_pending = false;
            this.open_files_links_menu(window, cx);
        });
    }

    /// The menu sits right above the chat box and as wide as it.
    fn open_files_links_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let composer = self.composer_bounds.get();
        let scale = super::appearance::ChatAppearance::current(&self.snapshot).scale;
        let Some(height) = self.pill_menu_height("filesLinks") else {
            return;
        };
        if composer.size.width <= px(0.0) {
            return;
        }
        // `composer_bounds` is measured in the window's content; the trigger a menu takes is too.
        let trigger = gpui::Bounds::new(composer.origin, gpui::size(composer.size.width, px(0.0)));
        self.menu_side = Some(super::option_menu::MenuSide::Right);
        self.show_chat_menu(
            vec![json!({"pillMenu": {"id": "filesLinks", "height": height}})],
            trigger,
            (composer.size.width.as_f32() / scale).max(360.0),
            window,
            cx,
        );
        if let Some(menu) = &self.option_menu {
            menu.update(cx, |menu, _| menu.set_pill(FILES_LINKS_TRIGGER));
        }
    }

    /// The menu went away (a click outside, Escape, another menu): the core closes the list, and
    /// the filter field goes with the window it was made in.
    pub(super) fn files_links_menu_closed(&mut self, cx: &mut Context<Self>) {
        self.files_links_input = None;
        self.files_links_subscription = None;
        self.files_links_input_window = None;
        if self.snapshot["filesLinks"]["open"] == true {
            self.files_links_closing = true;
            self.invoke(json!({"type":"filesLinksClose"}), cx);
        }
    }

    /// The menu's body: loading, the list in its three sections, or why there is none, with
    /// Refresh and Close. Every block has a fixed height (`files_links_menu_height`).
    pub(super) fn files_links_menu_body(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let files_links = self.snapshot["filesLinks"].clone();
        if files_links["open"] != true {
            return None;
        }
        self.sync_files_links_input(files_links["filter"] == true, window, cx);
        let s = p.scale;
        let loading = files_links["loading"] == true;
        let caption = text(&files_links, "caption");
        let header = super::chat_pills::pill_menu_header(
            "titlebar/link.svg",
            "Files and links",
            if loading && caption.is_empty() {
                "Reading the session…".to_string()
            } else {
                caption
            },
            None,
            p,
        );
        let mut column = div()
            .id("chat-files-links")
            .role(gpui::Role::Group)
            .aria_label("Files and links")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(GAP * s))
            .child(header);
        if let Some(input) = self.files_links_input.clone() {
            column = column.child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .h(px(FILTER_HEIGHT * s))
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .rounded(px(8.0 * s))
                    .border_1()
                    .border_color(p.input_border)
                    .bg(p.composer_background)
                    .px(px(10.0 * s))
                    .child(
                        svg()
                            .path("titlebar/search.svg")
                            .size(px(14.0 * s))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&input)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .w_full()
                                .p_0()
                                .text_size(px(13.0 * s))
                                .text_color(p.primary)
                                .placeholder_color(p.muted.opacity(0.6))
                                .caret_color(p.foreground),
                        ),
                    ),
            );
        }
        let sections = files_links["sections"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let line = |child: AnyElement| {
            div()
                .flex_shrink_0()
                .h(px(LINE_HEIGHT * s))
                .flex()
                .items_center()
                .px(px(4.0 * s))
                .text_size(px(13.0 * s))
                .child(child)
                .into_any_element()
        };
        if loading && sections.is_empty() {
            column = column.child(
                div()
                    .flex_shrink_0()
                    .h(px(PLACEHOLDER_HEIGHT * s))
                    .overflow_hidden()
                    .child(answering_placeholder(p)),
            );
        } else if let Some(error) = files_links["error"].as_str() {
            column = column.child(line(
                div()
                    .truncate()
                    .text_color(gpui::rgb(0xef9999))
                    .child(error.to_string())
                    .into_any_element(),
            ));
        } else if sections.is_empty() {
            column = column.child(line(
                div()
                    .truncate()
                    .text_color(p.muted)
                    .child(text(&files_links, "emptyLabel"))
                    .into_any_element(),
            ));
        } else {
            let mut list = div()
                .id("files-links-list")
                .flex_1()
                .min_h_0()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(SECTION_GAP * s))
                .overflow_y_scroll();
            for section in &sections {
                let id = text(section, "id");
                let mut block = div()
                    .flex_shrink_0()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(ROW_GAP * s))
                    .child(
                        div()
                            .h(px(LINE_HEIGHT * s))
                            .flex()
                            .items_center()
                            .gap(px(6.0 * s))
                            .px(px(4.0 * s))
                            .child(
                                div()
                                    .text_size(px(12.0 * s))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.foreground)
                                    .child(text(section, "label")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0 * s))
                                    .text_color(p.muted)
                                    .child(section["count"].as_u64().unwrap_or(0).to_string()),
                            ),
                    );
                for group in section["groups"].as_array().into_iter().flatten() {
                    let title = text(group, "title");
                    if !title.is_empty() {
                        block = block.child(
                            div()
                                .h(px(LINE_HEIGHT * s))
                                .flex()
                                .items_center()
                                .px(px(6.0 * s))
                                .text_size(px(11.0 * s))
                                .text_color(p.muted)
                                .child(div().truncate().child(title)),
                        );
                    }
                    let rows = group["rows"].as_array().cloned().unwrap_or_default();
                    let gallery: Vec<Value> = rows
                        .iter()
                        .filter(|row| {
                            row["image"].is_object() && row["image"]["transport"] != "none"
                        })
                        .map(|row| row["image"].clone())
                        .collect();
                    for row in &rows {
                        block = block.child(self.files_links_row(row, &gallery, p, cx));
                    }
                }
                let more = text(section, "moreLabel");
                if !more.is_empty() {
                    block = block.child(
                        div()
                            .id(SharedString::from(format!("files-links-more:{id}")))
                            .role(gpui::Role::Button)
                            .h(px(LINE_HEIGHT * s))
                            .flex()
                            .items_center()
                            .px(px(6.0 * s))
                            .text_size(px(12.0 * s))
                            .text_color(p.card_muted)
                            .chat_cursor_pointer()
                            .hover(|style| style.text_color(p.foreground))
                            .child(more)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.invoke(json!({"type":"filesLinksShowAll","section":id}), cx)
                            })),
                    );
                }
                list = list.child(block);
            }
            column = column.child(list);
        }
        column = column.child(
            div()
                .flex_shrink_0()
                .h(px(FOOTER_HEIGHT * s))
                .flex()
                .items_center()
                .justify_end()
                .gap(px(6.0 * s))
                .child(side_button(
                    "files-links-refresh",
                    Some("titlebar/refresh.svg"),
                    "Refresh",
                    None,
                    loading,
                    Some("Read the session again for anything mentioned since"),
                    p,
                    cx.listener(|this, _, _, cx| {
                        this.invoke(json!({"type":"filesLinksRefresh"}), cx)
                    }),
                ))
                .child(side_button(
                    "files-links-close",
                    Some("titlebar/x.svg"),
                    "Close",
                    None,
                    false,
                    None,
                    p,
                    cx.listener(|this, _, _, cx| {
                        this.invoke(json!({"type":"filesLinksClose"}), cx)
                    }),
                )),
        );
        Some(column.into_any_element())
    }
}

/// The menu's content height (unscaled) for the document's `filesLinks`, capped at `max` (the list
/// scrolls inside), or `None` while the list is closed.
pub(super) fn files_links_menu_height(files_links: &Value, max: f32) -> Option<f32> {
    if files_links["open"] != true {
        return None;
    }
    let sections = files_links["sections"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut height = super::chat_pills::MENU_HEADER_HEIGHT + GAP + FOOTER_HEIGHT + GAP;
    if files_links["filter"] == true {
        height += FILTER_HEIGHT + GAP;
    }
    let body = if files_links["loading"] == true && sections.is_empty() {
        PLACEHOLDER_HEIGHT
    } else if files_links["error"].is_string() || sections.is_empty() {
        LINE_HEIGHT
    } else {
        let mut list = 0.0;
        for (index, section) in sections.iter().enumerate() {
            let mut lines = vec![LINE_HEIGHT];
            for group in section["groups"].as_array().into_iter().flatten() {
                if !text(group, "title").is_empty() {
                    lines.push(LINE_HEIGHT);
                }
                let rows = group["rows"].as_array().map_or(0, Vec::len);
                lines.extend(std::iter::repeat_n(ROW_HEIGHT, rows));
            }
            if !text(section, "moreLabel").is_empty() {
                lines.push(LINE_HEIGHT);
            }
            list += lines.iter().sum::<f32>() + lines.len().saturating_sub(1) as f32 * ROW_GAP;
            if index > 0 {
                list += SECTION_GAP;
            }
        }
        list
    };
    Some((height + body).min(max))
}
