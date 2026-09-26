use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
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

    pub(crate) fn submit(&mut self, mode: &str, window: &mut Window, cx: &mut Context<Self>) {
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
        if !self.composer_ready || self.draft.trim().is_empty() || self.pending_send {
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
        let submission = json!({"type":mode,"text":self.draft,
            "draftVersion":{"draftId":self.draft_id,"revision":self.draft_revision.max(1)}});
        self.draft.clear();
        self.draft_id = next_id;
        self.draft_revision = 1;
        if let Some(input) = &self.input {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.emit(super::state::NativeChatEvent::DraftState(true));
        self.invoke(submission, cx);
    }

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
            .border_color(p.border)
            .text_color(p.primary)
            .hover(|style| style.bg(p.border))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)))
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
            "summaryMode" => "Summary mode",
            "sessionNote" => "Session note",
            "stashPrompt" => "Stash prompt",
            "attachPath" => "Attach a file or folder",
            "terminalView" => "Terminal View",
            _ => action,
        };
        // CDXC:SessionChat 2026-09-18 SEE-ALSO: The stash count badge, the session-note presence dot and the pressed Summary/Note states come from the core's composer chrome (`packages/gx-chat-core/src/composer/note.rs`), ported from React's `session-chat-composer-actions.tsx` chrome.
        let chrome = &self.snapshot["composerChrome"];
        let pressed = match action {
            "summaryMode" => chrome["summaryPressed"] == true,
            "sessionNote" => chrome["notePressed"] == true,
            "maximizeComposer" => self.maximized_window.is_some(),
            "moreActions" => self.chat_menu_is_open(super::actions::MORE_ACTIONS_TRIGGER),
            _ => false,
        };
        let badge = match action {
            "stashPrompt" => chrome["stashBadge"].as_str().map(str::to_owned),
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
                // The configured chord, so rebinding it in Settings > Hotkeys changes the tooltip too.
                let hotkey = match action {
                    "summaryMode" => {
                        crate::app::hotkeys::gpui_configured_hotkey_label("toggleChatSummaryMode")
                    }
                    _ => None,
                };
                let text = match hotkey.filter(|hotkey| !hotkey.is_empty()) {
                    Some(hotkey) => format!("{label} ({hotkey})"),
                    None => label.to_owned(),
                };
                gpui_component::tooltip::Tooltip::new(text).build(window, cx)
            })
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    this.perform_composer_action(action, event.position(), window, cx)
                }),
            )
            .when(action == "stashPrompt", |this| {
                this.on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(|this, _, _, cx| {
                        this.host("stashedPrompts", json!({}), cx);
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
        let collapsed = self.composer_collapsed();
        // CDXC:SessionChat 2026-09-19 SEE-ALSO:
        // The chat box's tween, React's `use-session-chat-composer-transition.ts`. Timing, easing and
        // the collapsed and expanded metrics are shared through
        // `packages/gx-chat-core/visual/composer-animation.json`; `composer_animation.rs`
        // holds the interpolation.
        let metrics = &*super::composer_animation::METRICS;
        let frame = self.composer_frame(cx);
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
            .when(maximized, |this| this.p_0().max_w_full().h_full().min_h_0());
        if self.snapshot["incomingDraft"].is_object() {
            /*
            CDXC:Drafts 2026-09-18 DECISION:
            User (2026-09-10, React): the saved-draft notice previews the message it would restore.
            React hung a popover off a document icon; the GPUI row puts the same preview in the
            icon's tooltip so the notice stays one line high.
            */
            let preview: String = self.snapshot["incomingDraft"]["content"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(600)
                .collect();
            footer = footer.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .child(
                        div()
                            .id("incoming-draft-preview")
                            .flex_shrink_0()
                            .size(px(20.0 * s))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .hover(|style| style.bg(p.border))
                            .child(
                                gpui::svg()
                                    .path("titlebar/file-text.svg")
                                    .size(px(14.0 * s))
                                    .text_color(p.muted),
                            )
                            .tooltip(move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(preview.clone())
                                    .build(window, cx)
                            }),
                    )
                    .child(div().flex_1().child("Another saved draft is available"))
                    .child(self.chat_button(
                        "use-incoming".into(),
                        "Use".into(),
                        json!({"type":"useIncomingDraft"}),
                        p,
                        cx,
                    ))
                    .child(self.chat_button(
                        "dismiss-incoming".into(),
                        "Dismiss".into(),
                        json!({"type":"dismissIncomingDraft"}),
                        p,
                        cx,
                    ))
                    .child(super::suggestions::stack_marker(&stack_top)),
            );
        }
        if !maximized {
            if let Some(strip) = self.render_working_strip(p, cx) {
                footer = footer.child(strip);
            }
        }
        // A `composerNotReady` refusal gets its own card instead of the plain error line.
        if let Some(card) = self.render_composer_not_ready(p, cx) {
            footer = footer.child(super::suggestions::stack_member(card, &stack_top));
        } else if let Some(error) = self.snapshot["operationError"].as_str() {
            footer = footer.child(
                div()
                    .text_color(gpui::rgb(0xef9999))
                    .child(error.to_string())
                    .child(super::suggestions::stack_marker(&stack_top)),
            );
        }
        if !maximized {
            if let Some(tasks) = self.render_agent_tasks(p, cx) {
                footer = footer.child(super::suggestions::stack_member(tasks, &stack_top));
            }
            if let Some(fleet) = self.render_agent_fleet(p, cx) {
                footer = footer.child(super::suggestions::stack_member(fleet, &stack_top));
            }
        }
        if let Some(notice) = self.render_notice(p, window, cx) {
            footer = footer.child(notice);
        }
        if let Some(questions) = self.render_async_questions(p, window, cx) {
            footer = footer.child(questions);
        }
        if let Some(prompt) = self.render_prompt(p, window, cx) {
            if self.snapshot["prompt"]["kind"] == "question" {
                return div()
                    .flex()
                    .w_full()
                    .justify_center()
                    .flex_shrink_0()
                    .child(footer.child(prompt))
                    .into_any_element();
            }
            footer = footer.child(prompt);
        }
        if let Some(note) = self.render_note(p, window, cx) {
            footer = footer.child(note);
        }
        if maximized {
            if let Some(suggestions) = self.inline_suggestions(window, cx) {
                let height =
                    super::suggestions::suggestion_panel_height(&self.snapshot["suggestions"], s);
                footer = footer.child(
                    div()
                        .h(height)
                        .max_h(gpui::relative(0.4))
                        .flex_shrink_0()
                        .child(suggestions),
                );
            }
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
                            cx.notify();
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
                            this.invoke(json!({"type":"composerExpand","editor":true}), cx);
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
                    .when(!collapsed, |this| {
                        this.child(self.render_option_pills(&model, &effort, p, cx))
                    })
                    .child(self.render_toolbar(p, cx))
                    .when(!collapsed, |this| this.child(measurement)),
            )
            .when(!maximized, |this| this.child(content_measure));
        // CDXC:SessionChat 2026-09-18 WHY: React's inline composer had a zero-height notification section before its field, contributing one grid gap even while idle. Reserve that same gap here, after any cards or note.
        footer = footer.child(
            div()
                .relative()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .when(!maximized, |this| this.mt(px(8.0 * s)))
                .child(suggestion_anchor)
                .rounded(px(22.0 * s))
                .border_1()
                .border_color(p.composer_border)
                .bg(p.composer_background)
                .px(px(16.0 * s))
                .py(px(padding_block))
                .when(maximized, |this| this.flex_1().min_h_0())
                .when_some(frame.height.filter(|_| !maximized), |this, height| {
                    this.h(px(height)).overflow_hidden()
                })
                .child(content),
        );
        if self.status_line_reserved() {
            footer = footer.child(self.render_context_status(p, window, cx));
        }
        div()
            .flex()
            .w_full()
            .justify_center()
            .flex_shrink_0()
            .when(maximized, |this| this.h_full().min_h_0())
            .child(footer)
            .into_any_element()
    }
}
