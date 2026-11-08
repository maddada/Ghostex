//! The Memory tab: what loads into the GBot when a chat starts (with the gauge of `MEMORY.md`
//! against its 200 lines / 24 KB), `MEMORY.md` itself with an editor, its topic notes, its daily
//! log and the journal of changes with Undo.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (Q2104 A, the mockup's Memory tab): the tab shows a gauge (lines and KB against the budget, amber from 80%), an editor that never overwrites what the bot wrote meanwhile, the topic list with Show in Finder, the daily log and Undo last change. The editor sends the text's hash from when it was read; gxserver refuses the save when MEMORY.md changed since, the tab keeps the typed text and says so, and Reload shows the newer file.
//! SEE-ALSO: server/src/gbots/memory/ops.rs (`memoryShow`, `memoryEdit`, `memoryUndo`), apps/mobile/app/src/screens/sessions-screen/GbotMemoryView.tsx (the phone's read-only view with Undo).
use super::super::native_modal_kit::*;
use super::memory_model::*;
use super::parts::{page_header, read_only, row, section};
use super::window::{GbotSettingsModalCommand, GpuiGbotSettingsModalWindow};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, px, relative,
};
use gpui_component::input::TextareaState;
use gpui_component::{h_flex, v_flex};
use serde_json::Value;

const AMBER: u32 = 0xf59e0b;
const RED: u32 = 0xef4444;
/// Changes the tab lists.
const CHANGES_SHOWN: usize = 8;

/// The Memory tab's state inside the window.
pub(crate) struct GbotMemoryTab {
    pub(super) view: Option<GbotMemoryView>,
    /// A read is on its way (the first one, a reload, or after a save or undo).
    pub(super) busy: bool,
    pub(super) requested: bool,
    pub(super) editing: bool,
    pub(super) error: Option<String>,
    /// The last save was refused because MEMORY.md changed meanwhile.
    pub(super) conflict: bool,
    pub(super) input: Entity<TextareaState>,
    pub(super) scroll: ScrollHandle,
}

impl GbotMemoryTab {
    pub(super) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            view: None,
            busy: false,
            requested: false,
            editing: false,
            error: None,
            conflict: false,
            input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("One note per line, e.g. - 2026-11-09 · Prefers aisle seats")
            }),
            scroll: ScrollHandle::new(),
        }
    }
}

/// Where a memory op's answer goes: the window that asked.
pub(crate) struct GbotMemoryReply {
    view: WeakEntity<GpuiGbotSettingsModalWindow>,
    window: AnyWindowHandle,
}

impl GbotMemoryReply {
    /// Hands the window gxserver's `memoryShow` answer after the op, or its refusal.
    pub(crate) fn finish(self, result: Result<Value, String>, cx: &mut App) {
        let view = self.view;
        let _ = self.window.update(cx, |_root, window, cx| {
            view.update(cx, |modal, cx| modal.finish_memory(result, window, cx))
        });
    }
}

impl GpuiGbotSettingsModalWindow {
    fn memory_reply(&self, window: &Window, cx: &mut Context<Self>) -> GbotMemoryReply {
        GbotMemoryReply {
            view: cx.entity().downgrade(),
            window: window.window_handle(),
        }
    }

    /// Reads the memory the first time the tab shows.
    pub(super) fn ensure_memory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.memory.requested {
            return;
        }
        self.memory.requested = true;
        self.reload_memory(window, cx);
    }

    fn reload_memory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.memory.busy = true;
        self.memory.error = None;
        self.memory.conflict = false;
        self.memory.editing = false;
        self.pending_memory_op = Some(MemoryOpTag::Read);
        cx.notify();
        let reply = self.memory_reply(window, cx);
        (self.host)(
            GbotSettingsModalCommand::MemoryLoad {
                bot_id: self.original.bot_id.clone(),
                reply,
            },
            cx,
        );
    }

    fn start_memory_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = &self.memory.view else {
            return;
        };
        let text = view.text.clone();
        self.memory.input.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.focus(window, cx);
        });
        self.memory.editing = true;
        self.memory.error = None;
        self.memory.conflict = false;
        cx.notify();
    }

    pub(super) fn cancel_memory_edit(&mut self, cx: &mut Context<Self>) {
        self.memory.editing = false;
        self.memory.error = None;
        self.memory.conflict = false;
        cx.notify();
    }

    /// Unsaved text in the memory editor.
    pub(super) fn memory_edited(&self, cx: &App) -> bool {
        self.memory.editing
            && self.memory.view.as_ref().is_some_and(|view| {
                self.memory.input.read(cx).value().trim_end() != view.text.trim_end()
            })
    }

    pub(super) fn save_memory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = &self.memory.view else {
            return;
        };
        if self.memory.busy || !self.memory.editing {
            return;
        }
        let text = self.memory.input.read(cx).value().to_string();
        let base = view.hash.clone();
        self.memory.busy = true;
        self.memory.error = None;
        self.pending_memory_op = Some(MemoryOpTag::Save);
        cx.notify();
        let reply = self.memory_reply(window, cx);
        (self.host)(
            GbotSettingsModalCommand::MemorySave {
                bot_id: self.original.bot_id.clone(),
                text,
                base,
                reply,
            },
            cx,
        );
    }

    fn undo_memory(&mut self, change: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.memory.busy {
            return;
        }
        self.memory.busy = true;
        self.memory.error = None;
        self.pending_memory_op = Some(MemoryOpTag::Undo);
        cx.notify();
        let reply = self.memory_reply(window, cx);
        (self.host)(
            GbotSettingsModalCommand::MemoryUndo {
                bot_id: self.original.bot_id.clone(),
                change,
                reply,
            },
            cx,
        );
    }

    fn reveal(&mut self, path: String, cx: &mut Context<Self>) {
        (self.host)(GbotSettingsModalCommand::Reveal { path }, cx);
    }

    fn finish_memory(
        &mut self,
        result: Result<Value, String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let op = self.pending_memory_op.take().unwrap_or(MemoryOpTag::Read);
        self.memory.busy = false;
        match result {
            Ok(answer) => {
                match GbotMemoryView::from_show(&answer) {
                    Some(view) => self.memory.view = Some(view),
                    None => {
                        self.memory.error = Some("Ghostex couldn't read this GBot's memory.".into())
                    }
                }
                if op == MemoryOpTag::Save {
                    self.memory.editing = false;
                }
                self.memory.conflict = false;
            }
            Err(message) => {
                // A refused save keeps the editor and its text; the sentence says why.
                self.memory.conflict = op == MemoryOpTag::Save && message.contains("changed while");
                self.memory.error = Some(message);
            }
        }
        cx.notify();
    }

    pub(super) fn render_memory(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mut column = v_flex()
            .w_full()
            .pr(px(20.0))
            .pb(px(8.0))
            .gap(px(24.0))
            .child(page_header(
                &p,
                "Memory",
                "What this GBot keeps between chats, and what loads when a chat starts.",
            ));
        match &self.memory.view {
            None if self.memory.error.is_some() => {
                column = column.child(self.memory_error_line(cx));
            }
            None => {
                column = column.child(read_only(&p, "Reading the memory…"));
            }
            Some(view) => {
                column = column
                    .child(self.render_loads(view))
                    .child(self.render_memory_file(view, window, cx))
                    .child(self.render_topics(view, cx))
                    .child(self.render_log(view))
                    .child(self.render_changes(view, cx));
            }
        }
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                div()
                    .id("gbot-settings-memory-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.memory.scroll)
                    .child(column),
            )
            .child(modal_edge_scrollbar(&self.memory.scroll))
            .into_any_element()
    }

    fn level_color(&self, level: GbotMemoryLevel) -> gpui::Rgba {
        match level {
            GbotMemoryLevel::Ok => self.palette.settings_accent,
            GbotMemoryLevel::Amber => modal_rgba(AMBER, 1.0),
            GbotMemoryLevel::Red => modal_rgba(RED, 1.0),
        }
    }

    fn render_gauge(&self, view: &GbotMemoryView) -> AnyElement {
        let p = self.palette;
        let fill = self.level_color(view.level);
        let note = match view.level {
            GbotMemoryLevel::Ok => format!(
                "When it fills up, the oldest notes move to the archive ({} there now).",
                view.archive_notes
            ),
            GbotMemoryLevel::Amber => {
                "Nearly full: the next note moves the oldest ones to the archive.".to_string()
            }
            GbotMemoryLevel::Red => {
                "Over what loads: the end of MEMORY.md doesn't reach new chats until older notes move to the archive. Saving it here moves them."
                    .to_string()
            }
        };
        v_flex()
            .w_full()
            .gap(px(8.0))
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(hsla(p.foreground))
                            .child("MEMORY.md"),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(hsla(if view.level == GbotMemoryLevel::Ok {
                                p.muted
                            } else {
                                fill
                            }))
                            .child(view.gauge_label()),
                    ),
            )
            .child(
                div()
                    .id("gbot-memory-gauge")
                    .w_full()
                    .h(px(8.0))
                    .rounded(px(4.0))
                    .bg(hsla(p.hairline))
                    .child(
                        div()
                            .h_full()
                            .rounded(px(4.0))
                            .bg(hsla(fill))
                            .w(relative(view.share.clamp(0.02, 1.0) as f32)),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .line_height(px(17.5))
                    .text_color(hsla(p.muted))
                    .child(note),
            )
            .into_any_element()
    }

    fn render_loads(&self, view: &GbotMemoryView) -> AnyElement {
        let p = self.palette;
        let lines = |count: u64| match count {
            0 => "None".to_string(),
            1 => "1 line".to_string(),
            count => format!("{count} lines"),
        };
        let topics = match view.topics_indexed {
            0 => "None yet".to_string(),
            1 => "1 topic, read when needed".to_string(),
            count => format!("{count} topics, read when needed"),
        };
        let team = match view.team {
            0 | 1 => "Chief of Staff".to_string(),
            count => format!("Chief of Staff and {count} GBots"),
        };
        section(
            &p,
            "What loads when a chat starts",
            vec![
                div()
                    .w_full()
                    .px(px(20.0))
                    .py(px(14.0))
                    .child(self.render_gauge(view))
                    .into_any_element(),
                row(
                    &p,
                    "Instructions",
                    read_only(&p, lines(view.instructions_lines)),
                ),
                row(
                    &p,
                    "Shared preferences",
                    read_only(&p, lines(view.preferences_lines)),
                ),
                row(&p, "Topic index", read_only(&p, topics)),
                row(&p, "Team", read_only(&p, team)),
            ],
        )
    }

    fn memory_error_line(&self, cx: &mut Context<Self>) -> AnyElement {
        let kit = self.kit;
        let error = self.memory.error.clone().unwrap_or_default();
        h_flex()
            .w_full()
            .gap(px(12.0))
            .items_center()
            .child(div().flex_1().min_w_0().child(modal_error(&kit, error)))
            .when(self.memory.conflict || self.memory.view.is_none(), |this| {
                this.child(small_button(
                    &self.palette,
                    "gbot-memory-reload".into(),
                    "Reload",
                    false,
                    cx.listener(|this, _, window, cx| this.reload_memory(window, cx)),
                ))
            })
            .into_any_element()
    }

    fn render_memory_file(
        &self,
        view: &GbotMemoryView,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let busy = self.memory.busy;
        let mut buttons = h_flex().gap(px(8.0)).flex_shrink_0();
        let body: AnyElement = if self.memory.editing {
            buttons = buttons
                .child(small_button(
                    &p,
                    "gbot-memory-cancel-edit".into(),
                    "Cancel",
                    busy,
                    cx.listener(|this, _, _, cx| this.cancel_memory_edit(cx)),
                ))
                .child(small_button(
                    &p,
                    "gbot-memory-save".into(),
                    if busy { "Saving…" } else { "Save memory" },
                    busy,
                    cx.listener(|this, _, window, cx| this.save_memory(window, cx)),
                ));
            v_flex()
                .w_full()
                .line_height(px(20.3))
                .child(modal_text_area(
                    &self.kit,
                    &self.memory.input,
                    Some(240.0),
                    busy,
                    window,
                    cx,
                ))
                .into_any_element()
        } else {
            buttons = buttons
                .child(small_button(
                    &p,
                    "gbot-memory-reveal".into(),
                    "Show in Finder",
                    false,
                    {
                        let path = view.path.clone();
                        cx.listener(move |this, _, _, cx| this.reveal(path.clone(), cx))
                    },
                ))
                .child(small_button(
                    &p,
                    "gbot-memory-edit".into(),
                    "Edit",
                    busy,
                    cx.listener(|this, _, window, cx| this.start_memory_edit(window, cx)),
                ));
            let text = if view.text.trim().is_empty() {
                "(Empty)".to_string()
            } else {
                view.text.trim_end().to_string()
            };
            div()
                .w_full()
                .px(px(12.0))
                .py(px(10.0))
                .rounded(px(MODAL_RADIUS_CONTROL))
                .bg(hsla(p.input_background()))
                .text_size(px(13.0))
                .line_height(px(19.0))
                .text_color(hsla(p.foreground))
                .child(text)
                .into_any_element()
        };
        let head = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.5))
                    .line_height(px(17.5))
                    .text_color(hsla(p.muted))
                    .child(if self.memory.editing {
                        "Saving checks that the GBot didn't write to it meanwhile, and moves the oldest dated notes to the archive if it's over what loads."
                    } else {
                        "Dated notes the GBot keeps with `ghostex gbots memory add`. Lines you write by hand stay where they are."
                    }),
            )
            .child(buttons);
        let mut rows = vec![
            v_flex()
                .w_full()
                .px(px(20.0))
                .py(px(14.0))
                .gap(px(12.0))
                .child(head)
                .child(body)
                .into_any_element(),
        ];
        if self.memory.error.is_some() {
            rows.push(
                div()
                    .w_full()
                    .px(px(20.0))
                    .py(px(12.0))
                    .child(self.memory_error_line(cx))
                    .into_any_element(),
            );
        }
        section(&p, "MEMORY.md", rows)
    }

    fn render_topics(&self, view: &GbotMemoryView, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let rows = if view.topics.is_empty() {
            vec![empty_row(
                &p,
                "No topics yet. A GBot keeps longer detail in a topic with `ghostex gbots memory add --topic <name>`.",
            )]
        } else {
            view.topics
                .iter()
                .map(|topic| {
                    let path = topic.path.clone();
                    h_flex()
                        .w_full()
                        .min_h(px(52.0))
                        .px(px(20.0))
                        .py(px(10.0))
                        .gap(px(16.0))
                        .items_center()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .text_color(hsla(p.foreground))
                                        .child(topic.title.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .line_height(px(17.5))
                                        .text_color(hsla(p.muted))
                                        .child(if topic.description.is_empty() {
                                            format!("memory/{}.md", topic.name)
                                        } else {
                                            format!(
                                                "{} · memory/{}.md",
                                                topic.description, topic.name
                                            )
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(12.5))
                                .text_color(hsla(p.muted))
                                .child(gbot_memory_size(topic.bytes)),
                        )
                        .child(small_button(
                            &p,
                            SharedString::from(format!("gbot-memory-topic-{}", topic.name)),
                            "Show in Finder",
                            false,
                            cx.listener(move |this, _, _, cx| this.reveal(path.clone(), cx)),
                        ))
                        .into_any_element()
                })
                .collect()
        };
        section(&p, "Topics", rows)
    }

    fn render_log(&self, view: &GbotMemoryView) -> AnyElement {
        let p = self.palette;
        let days: Vec<&GbotMemoryDay> = view
            .log
            .iter()
            .filter(|day| !day.lines.is_empty())
            .collect();
        let rows = if days.is_empty() {
            vec![empty_row(
                &p,
                "Nothing yet. Ghostex adds a line here each time one of this GBot's chats finishes a turn.",
            )]
        } else {
            days.iter()
                .map(|day| {
                    let heading = if day.date == view.today {
                        "Today".to_string()
                    } else {
                        day.date.clone()
                    };
                    v_flex()
                        .w_full()
                        .px(px(20.0))
                        .py(px(12.0))
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(hsla(p.foreground))
                                .child(heading),
                        )
                        .children(day.lines.iter().rev().take(12).map(|line| {
                            div()
                                .text_size(px(13.0))
                                .line_height(px(18.5))
                                .text_color(hsla(p.muted))
                                .child(line.clone())
                        }))
                        .into_any_element()
                })
                .collect()
        };
        section(&p, "Daily log", rows)
    }

    fn render_changes(&self, view: &GbotMemoryView, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let busy = self.memory.busy;
        let rows = if view.changes.is_empty() {
            vec![empty_row(
                &p,
                "No changes yet. Every change to this GBot's memory shows here and can be undone.",
            )]
        } else {
            view.changes
                .iter()
                .take(CHANGES_SHOWN)
                .map(|change| {
                    let id = change.id.clone();
                    h_flex()
                        .w_full()
                        .min_h(px(44.0))
                        .px(px(20.0))
                        .py(px(8.0))
                        .gap(px(16.0))
                        .items_center()
                        .when(change.undone, |this| this.opacity(0.55))
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(104.0))
                                .text_size(px(12.5))
                                .text_color(hsla(p.muted))
                                .child(change.when.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(13.5))
                                .text_color(hsla(p.foreground))
                                .child(if change.undone {
                                    format!("{} · {} (undone)", change.by, change.what)
                                } else {
                                    format!("{} · {}", change.by, change.what)
                                }),
                        )
                        .when(change.undoable, |this| {
                            this.child(small_button(
                                &p,
                                SharedString::from(format!("gbot-memory-undo-{}", change.id)),
                                "Undo",
                                busy,
                                cx.listener(move |this, _, window, cx| {
                                    this.undo_memory(id.clone(), window, cx)
                                }),
                            ))
                        })
                        .into_any_element()
                })
                .collect()
        };
        section(&p, "Changes", rows)
    }
}

/// What `pending_memory_op` holds (the window keeps one op in flight at a time).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryOpTag {
    Read,
    Save,
    Undo,
}

fn empty_row(p: &super::super::settings_modal::palette::SettingsPalette, text: &str) -> AnyElement {
    div()
        .w_full()
        .px(px(20.0))
        .py(px(14.0))
        .text_size(px(13.0))
        .line_height(px(18.5))
        .text_color(hsla(p.muted))
        .child(text.to_string())
        .into_any_element()
}

/// A compact neutral button for a row (Edit, Show in Finder, Undo).
fn small_button(
    p: &super::super::settings_modal::palette::SettingsPalette,
    id: SharedString,
    label: &'static str,
    disabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let rest = p.input_background();
    let hover = p.raised_hover;
    h_flex()
        .id(id.clone())
        .role(gpui::Role::Button)
        .aria_label(SharedString::from(label))
        .flex_shrink_0()
        .h(px(28.0))
        .px(px(12.0))
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(rest))
        .text_size(px(13.0))
        .text_color(hsla(p.foreground))
        .whitespace_nowrap()
        .child(label)
        .when(disabled, |this| this.opacity(0.5))
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(hover)))
                .on_click(on_click)
        })
        .into_any_element()
}
