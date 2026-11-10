use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    state::NativeChatView,
    transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::StatefulInteractiveElement;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    Window, div, px,
};
use gpui_component::input::Textarea;
use serde_json::{Value, json};

impl NativeChatView {
    pub(crate) fn icon_command(
        &self,
        id: &'static str,
        label: &'static str,
        icon: &'static str,
        command: Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .chat_cursor_pointer()
            .size(px(24.0 * p.scale))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.0 * p.scale))
            .hover(|style| style.bg(p.border))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            })
            .child(
                gpui::svg()
                    .path(icon)
                    .size(px(14.0 * p.scale))
                    .text_color(p.primary),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)))
            .into_any_element()
    }
    pub(crate) fn send(&mut self, queued: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.submit(if queued { "queue" } else { "send" }, window, cx);
    }

    /// `mode` is `send`, `queue`, `compact`, or `interrupt` for interrupt and send (a send the core
    /// marks to stop the agent's running turn first).
    pub(crate) fn submit(&mut self, mode: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (mode, interrupt) = match mode {
            "interrupt" => ("send", true),
            mode => (mode, false),
        };
        crate::support_logs::append_for_scenario(
            crate::support_logs::GpuiSupportLog::SessionChat,
            "gpui.sessionChat.viewState",
            "sessionChat.nativeSubmit",
            json!({
                "composerReady": self.composer_ready,
                "hasDraft": !self.draft.trim().is_empty(),
                "pendingSend": self.pending_send,
                "status": self.snapshot["status"],
                "hasRuntime": self.runtime.is_some(),
            }),
        );
        if !self.composer_ready {
            return;
        }
        // A paste whose clipboard is still being read: the message waits for its tile
        // (composer_attach.rs).
        let requested = if interrupt { "interrupt" } else { mode };
        if self.hold_send_for_attachments(requested, cx) {
            return;
        }
        // A picture still being saved goes with the message, even one with no words.
        let pictures = self.attachments_on_their_way();
        if self.draft.trim().is_empty() && !pictures {
            return;
        }
        if mode != "send" && self.snapshot["queue"]["capabilities"]["canQueue"] != true {
            return;
        }
        let blocked = self.snapshot["sendBlockedReason"]
            .as_str()
            .map(str::to_owned);
        if let Some(reason) = blocked {
            self.report_send_blocked(&reason, cx);
            return;
        }
        self.update_suggestion_selection(cx);
        if mode == "send" && self.snapshot["composerCommand"].is_string() {
            self.invoke(json!({"type":"completeComposerCommand"}), cx);
            self.ensure_input(window, cx);
            return;
        }
        let next_id = match crate::app::helpers::gpui_random_uuid_string() {
            Ok(id) => id,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.close_maximized(cx);
        self.pending_send = true;
        let mut submission = json!({"type":mode,"text":self.draft,"interrupt":interrupt,
            "draftVersion":{"draftId":self.draft_id,"revision":self.draft_revision.max(1)}});
        // The pictures still on their way go where the caret is, as a paste that lands writes
        // them (packages/gx-chat-core/src/composer/attaching.rs).
        if pictures {
            let caret = self
                .input
                .as_ref()
                .map(|input| input.read(cx).selected_range().end)
                .unwrap_or(self.draft.len())
                .min(self.draft.len());
            submission["caret"] = json!(self.draft[..caret].encode_utf16().count());
            self.paste_tiles.claim_all();
        }
        self.draft.clear();
        self.draft_id = next_id;
        self.draft_revision = 1;
        if let Some(input) = &self.input {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.emit(super::state::NativeChatEvent::DraftState(true));
        self.end_answer_hold_for_send();
        self.invoke(submission, cx);
    }

    /// The chat's neutral filled text button (Load earlier turns, Retry, Use / Dismiss, the
    /// terminal picker's actions).
    pub(crate) fn chat_button(
        &self,
        id: String,
        label: String,
        command: Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .chat_cursor_pointer()
            .px(px(8.0 * p.scale))
            .py(px(4.0 * p.scale))
            .rounded(px(6.0 * p.scale))
            .border_1()
            .chat_filled(p.neutral_button_fills(), true)
            .text_color(p.primary)
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)))
            .into_any_element()
    }

    /// Retry on the "Couldn't reach Ghostex" line: a send or queue that failed goes again from the
    /// chat box it went back to, under its first `sendRequestId` (`operationErrorResend` in the
    /// core's document), and anything else rebuilds the chat's connection.
    fn unreachable_retry(&self, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let resend = self.snapshot["operationErrorResend"]
            .as_str()
            .map(str::to_string);
        div()
            .id("unreachable-retry")
            .role(gpui::Role::Button)
            .aria_label("Retry")
            .chat_cursor_pointer()
            .px(px(8.0 * p.scale))
            .py(px(4.0 * p.scale))
            .rounded(px(6.0 * p.scale))
            .border_1()
            .chat_filled(p.neutral_button_fills(), true)
            .text_color(p.primary)
            .child("Retry")
            .on_click(
                cx.listener(move |this, _, window, cx| match resend.as_deref() {
                    Some(mode) if !this.draft.trim().is_empty() => this.submit(mode, window, cx),
                    _ => this.invoke(json!({"type":"retry"}), cx),
                }),
            )
            .into_any_element()
    }

    pub(crate) fn host_button(
        &self,
        action: &'static str,
        icon: &'static str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let label = match action {
            "moreActions" => "More actions",
            "maximizeComposer" if self.maximized_window.is_some() => "Exit maximize",
            "maximizeComposer" => "Maximize",
            "sessionNote" => "Session note",
            "filesLinks" => "Files and links",
            "attachPath" => "Attach a file or folder",
            "terminalView" => "Terminal View",
            "dictate" if self.dictating(cx) => "Stop dictating",
            "dictate" => "Dictate",
            _ => action,
        };
        // CDXC:SessionChat 2026-09-18 SEE-ALSO: The session-note presence dot and pressed state come from the core's composer chrome (`packages/gx-chat-core/src/composer/note.rs`), ported from React's `session-chat-composer-actions.tsx` chrome.
        let chrome = &self.snapshot["composerChrome"];
        let pressed = match action {
            "sessionNote" => chrome["notePressed"] == true,
            "filesLinks" => self.snapshot["filesLinks"]["open"] == true,
            "maximizeComposer" => self.maximized_window.is_some(),
            "moreActions" => self.chat_menu_is_open(super::actions::MORE_ACTIONS_TRIGGER),
            "dictate" => self.dictating(cx),
            _ => false,
        };
        let badge = match action {
            "sessionNote" if chrome["notePresence"] == true => Some(String::new()),
            _ => None,
        };
        div()
            .id(action)
            .relative()
            .role(gpui::Role::Button)
            .aria_label(label)
            .chat_cursor_pointer()
            .size(px(28.0 * p.scale))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .when(pressed, |this| this.bg(p.border))
            .hover(|style| style.bg(p.border))
            .child(
                gpui::svg()
                    .path(icon)
                    .size(px(16.0 * p.scale))
                    .text_color(if pressed {
                        p.control_primary
                    } else {
                        p.primary
                    }),
            )
            .when_some(badge, |this, badge| {
                let dot = badge.is_empty();
                this.child(
                    div()
                        .absolute()
                        .top(px(0.0))
                        .right(px(0.0))
                        .size(px(if dot { 6.0 } else { 12.0 } * p.scale))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(p.foreground)
                        .text_color(p.background)
                        .text_size(px(9.0 * p.scale))
                        .line_height(px(9.0 * p.scale))
                        .child(badge),
                )
            })
            .tooltip(move |window, cx| {
                // The microphone says what will transcribe (dictation.rs).
                if action == "dictate" {
                    let text = super::dictation::dictation_tooltip_text(
                        crate::app::voice_agent::dictate_engine().as_ref(),
                        label == "Stop dictating",
                        &hotkey_chord(Some("toggleDictation")),
                        &super::dictation::dictate_hotkey_mode(),
                    );
                    return crate::app::native_sidebar::tooltips::compact_card_tooltip(
                        text, 240.0, window, cx,
                    );
                }
                let hotkey_id = match action {
                    "sessionNote" => Some("sessionNote"),
                    "attachPath" => Some("attachFileOrFolder"),
                    "maximizeComposer" => Some("toggleChatComposerMaximized"),
                    "dictate" => Some("toggleDictation"),
                    _ => None,
                };
                gpui_component::tooltip::Tooltip::new(label)
                    .keys(hotkey_chord(hotkey_id))
                    .build(window, cx)
            })
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    this.perform_composer_action(action, event.position(), window, cx)
                }),
            )
            // More actions and Attach open menus lined up with this button's right edge.
            .when_some(
                self.composer_button_bounds.cell(action),
                |this, measured| {
                    this.child(
                        gpui::canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                            .absolute()
                            .inset(px(0.0)),
                    )
                },
            )
            // Resting on the microphone offers Transcribe previous; right-click always opens its menu.
            .when(action == "dictate", |this| {
                let pressed = self.dictate_pressed.clone();
                // The press must not move focus to the chat surface: the chat box keeps its caret
                // where the dictated text will land (dictation.rs). It is also not a pointer that
                // came to rest: the hover menu waits until the pointer has left (`hover_dictation`).
                this.on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _: &gpui::MouseDownEvent, window, _| {
                        window.prevent_default();
                        this.press_dictation();
                    }),
                )
                .child(
                    gpui::canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            // The pointer has left the button: it moved outside it, or it left
                            // the window, which comes without a last movement.
                            let moved = pressed.clone();
                            window.on_mouse_event(move |event: &gpui::MouseMoveEvent, _, _, _| {
                                if moved.get() && !bounds.contains(&event.position) {
                                    moved.set(false);
                                }
                            });
                            window.on_mouse_event(move |_: &gpui::MouseExitEvent, _, _, _| {
                                pressed.set(false);
                            });
                        },
                    )
                    .absolute()
                    .inset(px(0.0)),
                )
                .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                    this.hover_dictation(*hovered, window, cx);
                }))
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                        this.show_dictation_menu(event.position, window, cx);
                        cx.stop_propagation();
                    }),
                )
            })
            .into_any_element()
    }

    pub(crate) fn render_composer(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        self.sync_composer_references(p, cx);
        let maximized = self.maximized_window.is_some();
        // CDXC:SessionChat 2026-09-19 SEE-ALSO:
        // The chat box's tween, React's `use-session-chat-composer-transition.ts`. Timing, easing and
        // the collapsed and expanded metrics are shared through
        // `packages/gx-chat-core/visual/composer-animation.json`; `composer_animation.rs`
        // holds the interpolation.
        let metrics = &*super::composer_animation::METRICS;
        let frame = self.composer_frame(cx);
        let collapsed = frame.collapsed;
        if frame.running && !maximized {
            window.request_animation_frame();
        }
        let input = self.input.as_ref().unwrap().clone();
        let attachment_previews = if collapsed {
            None
        } else {
            self.render_attachment_previews(p, cx)
        };
        let stack_top = super::suggestions::StackTop::default();
        let mut footer = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.0 * s))
            .px(px(16.0 * s))
            .pt(px(8.0 * s))
            .pb(px(12.0 * s))
            .max_w(px(768.0 * s))
            // CDXC:SessionChat 2026-11-07 DECISION:
            // Chat box matches transcript width (`chatComposerMatchesTranscriptWidth`, desktop only). User: "the input bar itself... is very narrow, it does not get wider with the rest". On, the chat box, its pill strip and the status line under it take the transcript column's width, the Transcript width percentage and the width the rows hold while a panel slides (render.rs), so they shrink on a narrow pane exactly as the rows do. Off keeps the fixed 768px chat box: "all of these things need to be advanced mode and if we don't enable anything then it should remain to look like it does now".
            .when(p.composer_matches_transcript && !maximized, |this| {
                self.with_transcript_width(this, p)
            })
            .when(maximized, |this| this.p_0().max_w_full().h_full().min_h_0());
        // First in the stack, right above whatever else waits on the user (working_card.rs). With
        // Status line in the chat on, the line is the transcript's last row instead
        // (status_tail.rs), and the box keeps the lead-in it has with no card above it; an empty
        // chat has no row to end with and keeps the card (gx-chat-core `inTranscript`).
        self.wake_when_worked_line_ends(cx);
        // A coordinator's dock (work_cards.rs), built first: with a dock the status rides on its
        // header bar (`work_dock_header`) and has no bar of its own.
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User, picking on the chat box bars page: "A. The status joins the dock's quiet line",
        // always on that bar while the chat has a dock; a chat with no dock keeps its own bar.
        let dock = if maximized {
            None
        } else {
            self.render_work_dock(p, cx)
        };
        let working_card = if maximized
            || dock.is_some()
            || self.snapshot["workingCard"]["inTranscript"] == true
        {
            None
        } else {
            self.render_working_card(p, cx)
        };
        // The session's computer is offline: its card tops the stack, above the working card
        // (computer_card.rs).
        let computer_card = if maximized {
            None
        } else {
            self.render_computer_card(p, cx)
        };
        let card_above_box = working_card.is_some() || computer_card.is_some();
        // The last transcript row's own gap is the space above the card (working_card.rs).
        footer = footer
            .when(card_above_box, |this| this.pt_0())
            .children(computer_card);
        // The working card joins the stack under the dock (below), so it can sit between the dock
        // and the box.
        let mut working_card = working_card;
        // What stacks between the card and the box.
        let mut waiting: Vec<AnyElement> = Vec::new();
        // A draft saved elsewhere is offered by the Saved draft pill in the box's pill strip
        // (saved_draft_pill.rs), which replaced the notice row here (CDXC:Drafts 2026-11-04).
        // A `composerNotReady` refusal gets its own card instead of the plain error line.
        if let Some(card) = self.render_composer_not_ready(p, cx) {
            waiting.push(super::suggestions::stack_member(card, &stack_top).into_any_element());
        } else if let Some(error) = self.snapshot["operationError"].as_str() {
            // An answer only the terminal can take (gxserver's `ANSWER_IN_TERMINAL`) offers the way
            // there; a call Ghostex didn't answer offers Retry. Both sit in the opened card.
            let mut actions = Vec::new();
            if self.snapshot["operationErrorCode"] == "answerInTerminal" {
                actions.push(self.chat_button(
                    "answer-in-terminal".into(),
                    "Open terminal".into(),
                    json!({"type":"switchToTerminal"}),
                    p,
                    cx,
                ));
            }
            if self.snapshot["operationErrorCode"] == "unreachable" {
                actions.push(self.unreachable_retry(p, cx));
            }
            let card = match &self.snapshot["operationErrorCard"] {
                card if card.is_object() => card.clone(),
                _ => ghostex_gx_chat_core::session::error_card::error_card(
                    error,
                    self.snapshot["operationErrorCode"].as_str(),
                ),
            };
            waiting.push(
                div()
                    .w_full()
                    .child(self.error_card("chat-operation-error", &card, actions, p, cx))
                    .child(super::suggestions::stack_marker(&stack_top))
                    .into_any_element(),
            );
        }
        if let Some(notice) = self.render_notice(p, window, cx) {
            waiting.push(notice.into_any_element());
        }
        if let Some(requests) = self.render_secret_requests(p, window, cx) {
            waiting.push(requests);
        }
        if let Some(collab) = self.render_collab(p, cx) {
            waiting.push(collab);
        }
        if let Some(line) = self.render_relayed_answer_lines(p) {
            waiting.push(line);
        }
        if let Some(card) = self.render_unrecorded_api_error(p, cx) {
            waiting.push(card.into_any_element());
        }
        if let Some(card) = self.render_model_hold_card(p, cx) {
            waiting.push(card);
        }
        if let Some(questions) = self.render_async_questions(p, window, cx) {
            waiting.push(questions.into_any_element());
        }
        if let Some(prompt) = self.render_prompt(p, window, cx) {
            if self.snapshot["prompt"]["kind"] == "question" {
                // The question replaces the box and the dock, so the status keeps its own bar.
                let working_card = match working_card {
                    None if dock.is_some()
                        && self.snapshot["workingCard"]["inTranscript"] != true =>
                    {
                        self.render_working_card(p, cx)
                    }
                    card => card,
                };
                return self
                    .composer_row()
                    .child(
                        footer
                            .children(working_card)
                            .children(waiting)
                            .child(prompt),
                    )
                    .into_any_element();
            }
            waiting.push(prompt.into_any_element());
        }
        if let Some(note) = self.render_note(p, window, cx) {
            waiting.push(note.into_any_element());
        }
        if let Some(card) = self.render_catch_up(p, window, cx) {
            waiting.push(card.into_any_element());
        }
        // A coordinator's dock (work_cards.rs) sits above the working card, which says how long
        // the turn took and what it changed, so the order over the box is the transcript, the
        // dock, that card, then the box. With no dock (none, stepped aside for a long message, or
        // the Work panel docked beside the chat) the card stays where it always was.
        //
        // CDXC:Coordinators 2026-11-10 DECISION:
        // User, of the "Worked for 34s · ... updated" bar above the dock: "please i beg you. move
        // this bar which shows how long we worked for etc to be below the work items cards
        // please". The gaps stay the stack's own.
        let dock_shown = dock.is_some();
        footer = footer.children(dock).children(working_card.take());
        self.sync_files_links_menu(window, cx);
        self.sync_resource_modal_menu(window, cx);
        if maximized && let Some(suggestions) = self.inline_suggestions(window, cx) {
            let height =
                super::suggestions::suggestion_panel_height(&self.snapshot["suggestions"], s);
            waiting.push(
                div()
                    .h(height)
                    .max_h(gpui::relative(0.4))
                    .flex_shrink_0()
                    .child(suggestions)
                    .into_any_element(),
            );
        }
        let model = text(&self.snapshot["optionLabels"], "model");
        let effort = text(&self.snapshot["optionLabels"], "options");
        let measurement = self.composer_measurement(p, window, cx);
        let composer_bounds = self.composer_bounds.clone();
        let chat = cx.weak_entity();
        let suggestion_anchor = gpui::canvas(
            move |bounds, _, cx| {
                // The card's left and right edges under the top of the panels stacked on it, which
                // prepainted before this canvas.
                let bounds = match stack_top.get() {
                    Some(top) if top < bounds.top() => gpui::Bounds::from_corners(
                        gpui::point(bounds.left(), top),
                        bounds.bottom_right(),
                    ),
                    _ => bounds,
                };
                if composer_bounds.replace(bounds) != bounds {
                    let chat = chat.clone();
                    cx.defer(move |cx| {
                        let _ = chat.update(cx, |chat, cx| chat.sync_suggestion_window(cx));
                    });
                }
            },
            |_, _, _, _| {},
        )
        // Absolute children are placed inside the card's padding box, so auto insets with
        // `size_full` measured a box shifted right by the padding. The popup anchors to the card's
        // painted edges, which sit one border width outside that box.
        .absolute()
        .inset(px(-1.0));
        let padding_block = if collapsed {
            metrics.collapsed_padding_block_px
        } else {
            metrics.expanded_padding_block_px
        } * s;
        // The box paints at its natural height whenever nothing is moving, so the tween needs the
        // height the content wants. The content column keeps that height even while the box is
        // pinned shorter (it never shrinks, and the box clips it), which is what lets a growing
        // editor or an arriving strip reveal itself over the tween instead of appearing whole.
        let measured = cx.weak_entity();
        let reported = self.composer_animation.reported();
        let min_delta = metrics.min_delta_px;
        let content_measure = gpui::canvas(
            move |bounds, window, cx| {
                let natural = bounds.size.height.as_f32() + padding_block * 2.0 + 2.0;
                if (reported.get() - natural).abs() < min_delta {
                    return;
                }
                reported.set(natural);
                let measured = measured.clone();
                window.defer(cx, move |_, cx| {
                    let reduce_motion = cx.reduce_motion();
                    let _ = measured.update(cx, |chat, cx| {
                        if chat.composer_animation.measured(natural, reduce_motion) {
                            chat.notify_composer(cx);
                        }
                    });
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let content = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(metrics.expanded_row_gap_px * s))
            .when(maximized, |this| this.flex_1().min_h_0())
            .when(!maximized, |this| this.flex_shrink_0())
            .when(collapsed, |this| {
                this.flex_row()
                    .items_center()
                    .gap(px(metrics.collapsed_row_gap_px * s))
            })
            .children(attachment_previews)
            .children(self.render_queue(p, cx))
            .child(
                self.with_catch_up_button(
                    div()
                        .id("composer-editor")
                        .role(gpui::Role::Group)
                        .aria_label("Message composer")
                        .min_w_0()
                        .w_full()
                        // React's floating input thumb stops 2px short of the viewport's right edge
                        // (app-scrollbars.ts places the 5px bar at `right - 7px`). gpui-component pins a
                        // custom-thickness thumb to the track edge, so the field gives back those 2px.
                        .pr(px(2.0 * s))
                        .when(collapsed, |this| {
                            this.flex_1()
                                .h(px(metrics.collapsed_height_px * s))
                                .overflow_hidden()
                        })
                        .when(maximized, |this| this.flex_1().h_full().min_h_0())
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                this.expand_composer(true, cx);
                                this.click_composer_reference(event, cx);
                            }),
                        )
                        .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                            this.hover_composer_reference(Some(event.position), cx);
                        }))
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            if !hovered {
                                this.hover_composer_reference(None, cx);
                            }
                        }))
                        .on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                                if this.show_composer_reference_menu(event, window, cx) {
                                    cx.stop_propagation();
                                }
                            }),
                        )
                        .child(
                            Textarea::new(&input)
                                .on_paste(Self::paste_handler(cx.entity().downgrade(), None))
                                .placeholder_color(p.muted.opacity(0.6))
                                // CDXC:SessionChat 2026-09-27 WHY: The chat draws in its own palette (sessionChatTheme can differ from the app theme, and the web page never switches GPUI Kit off its light default), so the kit theme's caret vanished against the composer. Every chat field takes its caret from the chat palette.
                                .caret_color(p.foreground)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                // The chat box's own scrollbar: React's 5px floating thumb that appears
                                // while the draft scrolls and fades out afterwards
                                // (packages/components/ui/app-scrollbars.ts, session-chat-lexical/input.css).
                                .scrollbar_thickness(px(5.0 * s))
                                .scrollbar_show(gpui_component::scroll::ScrollbarMode::Scrolling)
                                .w_full()
                                .p_0()
                                .text_size(px(14.0 * s))
                                .text_color(p.primary)
                                .line_height(px(if collapsed {
                                    metrics.collapsed_line_height_px
                                } else {
                                    metrics.expanded_line_height_px
                                } * s))
                                // CDXC:SessionChat 2026-09-14 DECISION: User: an empty, collapsed composer shows only the first line of its placeholder. The collapsed field is one line tall and its row clips, so the placeholder's second line never shows.
                                .when(collapsed, |this| {
                                    this.h(px(metrics.collapsed_height_px * s))
                                        .max_h(px(metrics.collapsed_height_px * s))
                                })
                                .when(!collapsed && !maximized, |this| {
                                    this.max_h(px(metrics.expanded_max_height_px * s))
                                })
                                .when(maximized, |this| this.h_full().flex_1().min_h_0()),
                        ),
                    !collapsed && !maximized,
                    p,
                    window,
                    cx,
                ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0 * s))
                    // React faded its option pills and toolbar back in over the second half of an
                    // expansion rather than having them appear the moment the box starts growing.
                    .when(frame.controls_opacity < 1.0, |this| {
                        this.opacity(frame.controls_opacity)
                            .top(px(frame.controls_offset * s))
                    })
                    .when(!collapsed && !self.collab_guest(), |this| {
                        this.child(self.render_option_pills(&model, &effort, p, cx))
                    })
                    .child(self.render_toolbar(p, cx))
                    .when(!collapsed, |this| this.child(measurement)),
            )
            .when(!maximized, |this| this.child(content_measure));
        if !maximized && let Some(run_location) = self.render_run_location(p, cx) {
            waiting.push(run_location.into_any_element());
        }
        // The worktree tray sits last, right on top of the box it is attached to.
        if !maximized && let Some(tray) = self.render_worktree_tray(p, window, cx) {
            waiting.push(tray);
        }
        // The box's own lead-in on top of the stack's 8px gap.
        let lead_in = if !waiting.is_empty() {
            0.0
        } else if card_above_box {
            super::working_card::WORKING_CARD_GAP - 8.0
        } else if dock_shown {
            0.0
        } else {
            8.0
        };
        footer = footer.children(waiting);
        // CDXC:SessionChat 2026-09-18 WHY: React's inline composer had a zero-height notification section before its field, contributing one grid gap even while idle. Reserve that same gap here when nothing stacks above the box.
        // The chat's pills ride in a strip along the box's top edge, inside its border
        // (pills_strip.rs); what the box holds under it is unchanged.
        let pills_strip = if maximized || self.collab_guest() {
            None
        } else {
            self.render_pills_strip(p, cx)
        };
        footer = footer.child(
            div()
                .relative()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .when(!maximized, |this| this.mt(px(lead_in * s)))
                .child(suggestion_anchor)
                .rounded(px(22.0 * s))
                .border_1()
                .border_color(p.composer_border)
                .bg(p.composer_background)
                .when(maximized, |this| this.flex_1().min_h_0())
                .children(pills_strip)
                .child(
                    div()
                        .relative()
                        .flex()
                        .flex_col()
                        .w_full()
                        .min_w_0()
                        .px(px(16.0 * s))
                        .py(px(padding_block))
                        .when(maximized, |this| this.flex_1().min_h_0())
                        .when_some(frame.height.filter(|_| !maximized), |this, height| {
                            this.h(px(height)).overflow_hidden()
                        })
                        .child(content)
                        // Dictate's pill floats just above the microphone, over the text field
                        // (dictation.rs), drawn last so it is on top.
                        .children(self.render_dictation_pill(p, cx)),
                ),
        );
        if self.status_line_reserved() && !self.collab_guest() {
            footer = footer.child(self.render_context_status(p, cx));
        }
        self.composer_row()
            .when(maximized, |this| this.h_full().min_h_0())
            .child(footer)
            .into_any_element()
    }

    /// The row the chat box is centred in.
    fn composer_row(&self) -> gpui::Div {
        div().flex().w_full().justify_center().flex_shrink_0()
    }
}

/// The action's configured chord for a tooltip's key caps (empty when unbound), so rebinding it in Settings > Hotkeys changes the tooltip too.
pub(super) fn hotkey_chord(hotkey_id: Option<&str>) -> String {
    hotkey_id
        .and_then(crate::app::hotkeys::gpui_configured_hotkey_label)
        .unwrap_or_default()
}
