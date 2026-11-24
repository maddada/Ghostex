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
        // One line per send, always kept (send_watch.rs).
        self.log_send(mode, interrupt);
        if !self.composer_ready {
            return;
        }
        // Pictures whose upload failed go up again, and this send takes them along
        // (composer_attach.rs, issue #618 Q3274 A).
        self.retry_failed_uploads(cx);
        // A paste whose clipboard is still being read: the message waits for its tile
        // (composer_attach.rs).
        let requested = if interrupt { "interrupt" } else { mode };
        if self.hold_send_for_attachments(requested, cx) {
            return;
        }
        // A reference to a file on another computer than the session's goes once its copy is
        // there (carry_attachments.rs).
        if self.hold_send_for_carry(requested, cx) {
            return;
        }
        // A picture still being saved goes with the message, even one with no words.
        // A coordinator draft can also start with no extra prompt (sends the coordinator guide line).
        let pictures = self.attachments_on_their_way();
        let starting_coordinator = self.draft_coordinator_mode && mode == "send";
        if self.draft.trim().is_empty() && !pictures && !starting_coordinator {
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
        // Text that looks like a key waits for a second Send (secret_send_warning.rs).
        if self.hold_secret_like_send(cx) {
            return;
        }
        // Long chats whose prompt cache expired offer to compact first (compact_prompt.rs, Q4396).
        if self.hold_compact_before_send(mode, interrupt, cx) {
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
        // The message's name from here to the agent: every attempt goes under it, the chat core
        // takes an action carrying it once, and gxserver delivers it once (send_watch.rs).
        let send_request_id = match crate::app::helpers::gpui_random_uuid_string() {
            Ok(id) => id,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.close_maximized(cx);
        self.pending_send = true;
        let outgoing_text = if starting_coordinator {
            self.draft_coordinator_mode = false;
            let guide = ghostex_gx_chat_core::composer::skill_invocation::COORDINATOR_GUIDE_LINE;
            let trimmed = self.draft.trim();
            let skill_only = trimmed.is_empty();
            let remote = self.config.remote.clone();
            let project_id = self.config.project_id.clone();
            let session_id = self.config.session_id.clone();
            let promote_params = json!({
                "projectId": project_id,
                "sessionId": session_id,
                "goal": "",
                "forDraft": true,
                "skillOnlyStart": skill_only,
            });
            cx.background_executor()
                .spawn(async move {
                    let _ = super::rpc::request(remote, "/api/promoteCoordinator", &promote_params)
                        .await;
                })
                .detach();
            if skill_only {
                guide.to_string()
            } else {
                format!("{guide}\n\n{}", self.draft)
            }
        } else {
            self.draft.clone()
        };
        let mut submission = json!({"type":mode,"text":outgoing_text,"interrupt":interrupt,
            "sendRequestId":send_request_id,
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
        self.follow_tail_for_send(mode);
        self.invoke(submission.clone(), cx);
        // A message left the chat box: an open secret card takes the keyboard back (secret_request.rs).
        self.return_keyboard_to_card(window, cx);
        // The sidebar's star request counts sent prompts (native_sidebar/github_star.rs).
        crate::app::native_sidebar::github_star::note_prompt_sent();
        // The box is empty now: the view watches the message until the core has it.
        self.watch_send(send_request_id, submission, cx);
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
            "maximizeComposer" => "Maximize chat box",
            "sessionNote" => "Note",
            "filesLinks" => "Files & Links",
            "attachPath" => "Attach a file or folder",
            "terminalView" => "Terminal View",
            "dictate" if self.dictating(cx) => "Stop dictating",
            "dictate" if super::dictation::dictation_unready(cx) => "Dictate isn't ready",
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
        // Greyed, still pressed: a press says why (the Unavailable card in `dictation.rs`).
        let dimmed = action == "dictate" && !pressed && super::dictation::dictation_unready(cx);
        // ••• turns red and says why while the terminal, off the row, reads not ready
        // (CDXC:Experience 2026-11-15 in gx-chat-core composer/row.rs).
        let issue = (action == "moreActions")
            .then(|| self.snapshot["composerRow"]["terminalIssue"].as_str())
            .flatten()
            .map(|reason| format!("Terminal problem: {reason}"));
        let issue_tint = issue
            .as_ref()
            .map(|_| gpui::Hsla::from(gpui::rgb(0xf0a3a3)));
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
            .child(gpui::svg().path(icon).size(px(16.0 * p.scale)).text_color(
                if let Some(tint) = issue_tint {
                    tint
                } else if pressed {
                    p.control_primary
                } else if dimmed {
                    p.primary.opacity(0.38)
                } else {
                    p.primary
                },
            ))
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
                if let Some(issue) = issue.clone() {
                    return crate::app::native_sidebar::tooltips::compact_card_tooltip(
                        issue, 280.0, window, cx,
                    );
                }
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
        let frame = self
            .pending_composer_frame
            .take()
            .unwrap_or_else(|| self.composer_frame(cx));
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
        // A new coordinator's first message, waiting for Start (first_message_card.rs).
        let first_message_card = if collapsed {
            None
        } else {
            self.render_first_message_card(p, cx)
        };
        let stack_top = super::suggestions::StackTop::default();
        let mut footer = div()
            .relative()
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
        // The card, or the dock's bar that carries its line, sets its lifted spark and track again
        // when it draws (indicator_overlay.rs).
        self.reset_working_lifts();
        self.wake_when_worked_line_ends(cx);
        // Error and issue banners sit above the Inbox dock (and temporarily collapse it to its
        // header bar while shown), ordered consistently: send/operation refusals first, then
        // error/warning/wait terminal notices (usage limits, agent errors, terminal issues).
        // Interactive dialogs, pickers and approvals stay below the dock in `waiting`.
        //
        // CDXC:SessionChat 2026-11-24 DECISION:
        // Error banners stack above the coordinator Inbox dock rather than below it, and collapse
        // the dock to its header bar while shown (still expandable; restores its previous state
        // when the error clears).
        let mut errors: Vec<AnyElement> = Vec::new();
        // A draft saved elsewhere is offered by the Saved draft pill in the box's pill strip
        // (saved_draft_pill.rs), which replaced the notice row here (CDXC:Drafts 2026-11-04).
        // A `composerNotReady` refusal gets its own card instead of the plain error line.
        if let Some(card) = self.render_composer_not_ready(p, cx) {
            errors.push(super::suggestions::stack_member(card, &stack_top).into_any_element());
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
            errors.push(
                div()
                    .w_full()
                    .child(self.error_card("chat-operation-error", &card, actions, p, cx))
                    .child(super::suggestions::stack_marker(&stack_top))
                    .into_any_element(),
            );
        }
        // An approval waiting on the agent is a row of the dock instead (gx-chat-core
        // extras/dock_approvals.rs); its card stays out of the stack.
        let approval_in_dock = self.approval_in_dock();
        let notice_is_error =
            ghostex_gx_chat_core::questions::notice_tone::is_error_terminal_notice(
                &self.snapshot["terminalNotice"],
            );
        let rendered_notice = if approval_in_dock != "notice" {
            self.render_notice(p, window, cx)
        } else {
            None
        };
        // What stacks between the dock/working card and the box.
        let mut waiting: Vec<AnyElement> = Vec::new();
        if let Some(notice) = rendered_notice {
            if notice_is_error {
                errors
                    .push(super::suggestions::stack_member(notice, &stack_top).into_any_element());
            } else {
                waiting.push(notice.into_any_element());
            }
        }
        let has_error_banner = !maximized && !errors.is_empty();
        // A coordinator's dock (work_cards.rs), built after errors so it knows whether an error
        // banner is shown above it: with a dock the status rides on its header bar
        // (`work_dock_header`) and has no bar of its own.
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User, picking on the chat box bars page: "A. The status joins the dock's quiet line",
        // always on that bar while the chat has a dock; a chat with no dock keeps its own bar.
        let dock = if maximized {
            None
        } else {
            self.render_work_dock(
                p,
                window.viewport_size().height.as_f32(),
                has_error_banner,
                cx,
            )
        };
        let working_card = if maximized
            || dock.is_some()
            || self.snapshot["workingCard"]["inTranscript"] == true
        {
            None
        } else {
            self.render_working_card(p, cx)
        };
        // The session's computer being offline is a row of the dock (gx-chat-core
        // extras/dock_stack.rs), the one bar above the box.
        let card_above_box = working_card.is_some();
        // The last transcript row's own gap is the space above the card (working_card.rs), when
        // the working card is the top element in the stack.
        footer = footer.when(card_above_box && errors.is_empty(), |this| this.pt_0());
        // The working card joins the stack under the dock (below), so it can sit between the dock
        // and the box.
        let mut working_card = working_card;
        if let Some(requests) = self.render_secret_requests(p, window, cx) {
            waiting.push(requests);
        }
        if let Some(questions) = self.render_async_questions(p, window, cx) {
            waiting.push(questions.into_any_element());
        }
        if let Some(prompt) = (approval_in_dock != "prompt")
            .then(|| self.render_prompt(p, window, cx))
            .flatten()
        {
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
                            .children(errors)
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
        if let Some(card) = self.render_compact_prompt(p, cx) {
            waiting.push(card);
        }
        // Error banners sit at the top of the stack, above the coordinator's dock (work_cards.rs),
        // which sits above the working card, so the order over the box is: transcript, errors,
        // dock, working card, interactive cards, then the box. With no dock (none, stepped aside
        // for a long message, or the Work panel docked beside the chat) the working card stays
        // below any errors.
        //
        // CDXC:Coordinators 2026-11-10 DECISION:
        // User, of the "Worked for 34s · ... updated" bar above the dock: "please i beg you. move
        // this bar which shows how long we worked for etc to be below the work items cards
        // please". The gaps stay the stack's own.
        let dock_shown = dock.is_some();
        let has_errors = !errors.is_empty();
        footer = footer
            .children(errors)
            .children(dock)
            .children(working_card.take());
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
        let resting_padding_block = if collapsed {
            metrics.collapsed_padding_block_px
        } else {
            metrics.expanded_padding_block_px
        } * s;
        // While transitioning between expanded and collapsed shapes, ease vertical padding from
        // `expanded_padding_block` (10px) to half the leftover around one line in the collapsed
        // box (`(48 - 24) / 2 = 12px`). Combined with the inner clip viewport below, line 1 lands
        // at the exact pixel position and caret height of the collapsed row and line 2 is clipped
        // to 0px before the row hands off.
        let collapsed_total = self.composer_animation.collapsed_estimate(s);
        let collapsed_text_pad = ((collapsed_total - metrics.collapsed_height_px * s) * 0.5)
            .max(metrics.collapsed_padding_block_px * s);
        let padding_block = if !collapsed && !maximized && frame.height.is_some() {
            resting_padding_block
                + (collapsed_text_pad - resting_padding_block) * frame.collapse_progress
        } else {
            resting_padding_block
        };
        // The box paints at its natural height whenever nothing is moving, so the tween needs the
        // height the content wants. The content column keeps that height even while the box is
        // pinned shorter (it never shrinks, and the box clips it), which is what lets a growing
        // editor or an arriving strip reveal itself over the tween instead of appearing whole.
        // The animated container is inside the outer bordered card (below `pills_strip`), so its
        // natural height is content + resting vertical padding with no border added.
        let measured = cx.weak_entity();
        let reported = self.composer_animation.reported();
        let min_delta = metrics.min_delta_px;
        let content_measure = gpui::canvas(
            move |bounds, window, cx| {
                let natural = bounds.size.height.as_f32() + resting_padding_block * 2.0;
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
        let editor_line_height = if collapsed {
            metrics.collapsed_line_height_px
        } else {
            metrics.expanded_line_height_px
        } * s;
        let placeholder_line = input
            .read(cx)
            .value()
            .is_empty()
            .then(|| self.input_placeholder.lines().next().unwrap_or_default())
            .filter(|line| !line.is_empty());
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
                    .min_h(px(
                        super::composer_animation::COLLAPSED_ROW_MIN_HEIGHT_PX * s
                    ))
                    .gap(px(metrics.collapsed_row_gap_px * s))
            })
            .children(first_message_card)
            .children(attachment_previews)
            .children(self.render_queue(p, cx))
            .child(
                self.with_composer_top_right(
                    div()
                        .id("composer-editor")
                        .relative()
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
                        // A click into the box is the user choosing it over an open secret card
                        // (CDXC:Secrets 2026-11-19 in secret_request.rs); seen before the field's
                        // own press focuses it.
                        .capture_any_mouse_down(cx.listener(
                            |this, event: &gpui::MouseDownEvent, _, _| {
                                if event.button == gpui::MouseButton::Left {
                                    this.note_composer_press();
                                }
                            },
                        ))
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
                        // Draw the placeholder as a single-line truncated overlay because
                        // gpui-component's TextElement shapes placeholder lines without ellipsis;
                        // TextareaState keeps the placeholder string for accessibility with transparent ink.
                        .when_some(placeholder_line, |this, line| {
                            this.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right(px(2.0 * s))
                                    .text_size(px(14.0 * s))
                                    .line_height(px(editor_line_height))
                                    .text_color(p.muted.opacity(0.6))
                                    .truncate()
                                    .child(line.to_owned()),
                            )
                        })
                        .child(
                            Textarea::new(&input)
                                .on_paste(Self::paste_handler(cx.entity().downgrade(), None))
                                // Cut, Copy, Paste, Select All and Save as Boost (boosts.rs).
                                .context_menu(Self::composer_context_menu(cx.entity().downgrade()))
                                .placeholder_color(gpui::transparent_black())
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
                                .line_height(px(editor_line_height))
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
            // Quiet lines about the box itself (box_notes.rs), under its text.
            .children((!collapsed).then(|| self.render_box_notes(p, cx)).flatten())
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
        } else if dock_shown || has_errors {
            0.0
        } else {
            8.0
        };
        let has_above_box =
            !maximized && (has_errors || dock_shown || card_above_box || !waiting.is_empty());
        let above_top = self.composer_above_top.clone();
        let above_top_offset = if card_above_box && !has_errors {
            0.0
        } else {
            8.0 * s
        };
        let above_box_measure = gpui::canvas(
            move |bounds, _, _| {
                above_top.set(has_above_box.then_some(bounds.top()));
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top(px(above_top_offset))
        .left_0()
        .size_0();
        footer = footer.children(waiting).child(above_box_measure);
        // CDXC:SessionChat 2026-09-18 WHY: React's inline composer had a zero-height notification section before its field, contributing one grid gap even while idle. Reserve that same gap here when nothing stacks above the box.
        // The chat's pills ride in a strip along the box's top edge, inside its border
        // (pills_strip.rs); what the box holds under it is unchanged.
        let pills_strip = if maximized || self.collab_guest() {
            self.set_where_pill_shows_project(false, cx);
            None
        } else {
            let strip = self.render_pills_strip(p, cx);
            let shows = self.where_pill_project_in_strip();
            self.set_where_pill_shows_project(shows, cx);
            strip
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
                // A sidebar session row held over the box shows where its link goes: the box
                // takes the focus ring (composer_drop.rs), with no split zones over the pane.
                .when(self.composer_drop_hover, |this| {
                    this.border_color(p.ring).shadow(vec![
                        gpui::BoxShadow::new(px(0.0), px(0.0), p.ring.opacity(0.2))
                            .spread_radius(px(3.0 * s)),
                    ])
                })
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
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .w_full()
                                .min_w_0()
                                .when(maximized, |this| this.flex_1().min_h_0())
                                .when(frame.height.is_some() && !maximized, |this| {
                                    this.flex_1().min_h_0().overflow_hidden()
                                })
                                .child(content),
                        )
                        // Cross-fade the compact `...` and Send controls in their exact collapsed
                        // slot during collapse/expand so they glide in/out instead of popping.
                        .when(
                            !collapsed && !maximized && frame.compact_controls_opacity > 0.01,
                            |this| {
                                this.child(self.render_compact_toolbar_overlay(
                                    frame.compact_controls_opacity,
                                    p,
                                ))
                            },
                        )
                        // Dictate's pill floats just above the microphone, over the text field
                        // (dictation.rs), drawn last so it is on top.
                        .children(self.render_dictation_pill(p, cx)),
                ),
        );
        if self.status_line_shown() && !self.collab_guest() {
            footer = footer.child(self.render_context_status(p, cx));
        }
        // A dock or card disclosure started during this composer render (e.g., expanding the
        // Inbox dock locally while an error banner is shown) needs the next animation frame.
        if self.disclosure_motion.borrow().running() {
            window.request_animation_frame();
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
