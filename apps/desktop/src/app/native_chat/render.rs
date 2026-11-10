use super::edit_shortcuts::ChatEditMenuActions as _;
use super::keyboard::ComposerInputActions as _;
use super::{appearance::ChatAppearance, indicator_overlay, state::NativeChatView};
use gpui::StatefulInteractiveElement as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Window,
    div, px,
};
use serde_json::json;

impl NativeChatView {
    /// Takes this frame's window width, which a Transcript width percentage is a share of. The
    /// list only remeasures its rows when its own width changes, so a percentage column that
    /// changed with the window while the pane kept its width remeasures them here.
    fn sync_window_width(&mut self, p: &ChatAppearance, window: &Window) {
        let width = window.viewport_size().width.as_f32();
        if (self.window_width - width).abs() < 0.5 {
            return;
        }
        let before = self.transcript_column_cap(p);
        self.window_width = width;
        if p.transcript_width.is_some() && (self.transcript_column_cap(p) - before).abs() > 0.5 {
            self.list.remeasure();
            self.subagent_list.remeasure();
        }
    }

    /// Decides this frame's `held_text_width`: while a window panel or the docked Session details
    /// column slides (`terminal_element::grid_resize_held`, `session_details.motion`) and the pane's width would change the transcript's
    /// text width, the rows keep the width they had before the slide, centred; the settling frame
    /// gives them the new one.
    ///
    /// CDXC:SessionChat 2026-10-10 WHY: a slide that squeezed or freed the chat (the side panel
    /// opening beside it) re-wrapped and re-measured every visible message on every frame; a long
    /// thread on a 4K 240 Hz screen drew it at ~30 ms a frame. A pane that only re-centres a
    /// capped transcript keeps sliding live, so nothing holds then. Terminals hold their grid the
    /// same way.
    fn hold_text_width_while_sliding(&mut self, p: &ChatAppearance) {
        let pane = self.bounds.get().size.width.as_f32();
        let natural = self.transcript_text_width(p);
        if crate::terminal_element::grid_resize_held()
            || self.session_details.motion.frame().animating
            || self.board.motion.frame().animating
        {
            self.held_text_width = self
                .settled_text_width
                .filter(|settled| (settled - natural).abs() > 0.5);
        } else {
            self.settled_text_width = (pane > 0.0).then_some(natural);
            self.held_text_width = None;
        }
    }

    /*
    CDXC:SessionChat 2026-09-20 WHY:
    The desktop shell lets this pane's transcript pass under the floating workarea header, and that
    is only safe while the transcript really is the first thing in the pane. Everything this render
    can put above it (the error banner, the "load earlier turns" button, the search bar, the fork
    branch button, and the maximized window's bare background) is chrome the header would hide, so
    the shell asks here first and keeps its old top edge instead. A new region above the transcript
    belongs in this list. The fork button is in it even though it costs the transcript no row: it is
    still drawn in the pane's top-right corner, which is exactly where the header would cover it.
    */
    pub(crate) fn renders_region_above_transcript(&self) -> bool {
        if self.maximized_window.is_some() {
            return true;
        }
        self.error.is_some()
            || (self.snapshot["hasMore"] == true && self.list.item_count() == 0)
            || self.search_open()
            || self.snapshot["forkBranches"]["count"].as_u64().is_some()
    }

    /// True while the transcript shows its first row from its very top, so nothing sits above the
    /// header to fade out.
    /// Set by the app each frame from `agents_column_flows_under_workarea_header`; the first row is
    /// remeasured when it changes so its top padding follows.
    pub(crate) fn set_under_workarea_header(&mut self, under: bool, cx: &mut Context<Self>) {
        if self.under_workarea_header == under {
            return;
        }
        self.under_workarea_header = under;
        if self.list.item_count() > 0 {
            self.list.splice(0..1, 1);
        }
        cx.notify();
    }

    pub(crate) fn transcript_scrolled_to_top(&self) -> bool {
        let top = self.list.logical_scroll_top();
        top.item_ix == 0 && top.offset_in_item <= px(0.0)
    }
}

impl Render for NativeChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        super::scroll_bottom::register(cx);
        super::tray_hotkeys::register(cx);
        super::search::register(cx);
        super::zoom::register(cx);
        self.last_render = Some(web_time::Instant::now());
        self.schedule_row_detail_sync(window, cx);
        self.refresh_issue_links(cx);
        self.refresh_links_pill(cx);
        self.note_drawn_in(window, cx);
        if self.maximized_window.is_none() {
            self.ensure_input(window, cx);
        }
        self.release_held_send(window, cx);
        self.sync_form_inputs(window, cx);
        self.sync_chat_zoom();
        let p = ChatAppearance::current(&self.snapshot).on_window_glass(
            crate::app::helpers::window_glass_active_for(self.main_window),
        );
        let s = p.scale;
        // Docked, floating or none: decided before anything measures the transcript's width, and
        // before the hold below reads whether the docked column is sliding.
        self.sync_session_details(&p, window, cx);
        self.sync_coordinator_board(&p, window, cx);
        self.sync_window_width(&p, window);
        self.hold_text_width_while_sliding(&p);
        self.sync_search_scroll();
        self.sync_files_links_jump();
        self.sync_open_question_jump(window);
        // A card above the composer that is opening or closing needs the next frame; the
        // transcript's rows ask for theirs as they are drawn.
        if self.disclosure_motion.borrow().running() {
            window.request_animation_frame();
        }
        self.sync_disclosure_anchor(window);
        self.sync_reply_arrival(cx);
        self.sync_transcript_focus_anchor(window, cx);
        /*
        CDXC:SessionChat 2026-09-18 WHY:
        React's maximized composer is a fixed overlay across the whole chat pane, so nothing of the
        conversation is left around it. The native one is a pane-sized child window over a
        translucent scrim, which leaves this pane painting underneath it: the transcript's rails and
        minimap showed through the margins. While it is up, the pane behind renders
        its background only.
        */
        let maximized = self.maximized_window.is_some();
        let glass = crate::app::helpers::window_glass_active_in(window);
        // Under glass the subagent viewer sits straight on the pane's glass, so the chat behind it
        // is not painted (subagent_view.rs, `render_subagent_viewer`).
        let covered = maximized || (glass && self.snapshot["subagent"].is_object());
        self.sync_search_folds(&p, cx);
        let search_bar = if covered {
            None
        } else {
            self.render_search_bar(&p, window, cx)
        };
        let state = self.snapshot.clone();
        // The chat's own failure (it could not load or start) is the same card as the chat box's.
        let error_card = self.error.clone().map(|error| {
            let card = ghostex_gx_chat_core::session::error_card::error_card(&error, None);
            self.error_card("chat-load-error", &card, Vec::new(), &p, cx)
        });
        self.transcript_inset = if maximized {
            0.0
        } else {
            self.composer_frame(cx).transcript_inset
        };
        crate::app::helpers::indicator_animation::render_indicator_frames_animation_only(
            cx.entity_id(),
        );
        // Status line in the chat: the working line joins or leaves the rows before they are drawn.
        self.sync_status_tail(&p, window);
        let transcript = self.render_transcript_host(window, cx);
        let rows = self.list.item_count();
        // The repeating indicators over the rows and in the composer's stack are drawn by the pane
        // over the chat when it can, so their ticks leave the chat cached (indicator_overlay.rs).
        let lifted = self.indicators_lifted(cx.entity_id());
        let overlay = self.indicator_overlay.clone();
        let body = if covered {
            overlay.set(indicator_overlay::STATUS_TAIL_LEAD, None);
            overlay.set(indicator_overlay::ROW_MOTION, None);
            None
        } else {
            let content = transcript;
            let lone_scroll_bottom = self.render_lone_scroll_bottom(&p, cx);
            // The lone scroll-down pill is drawn over the rows, so the pane must not draw above it.
            let lift_rows = lifted && lone_scroll_bottom.is_none();
            let in_place = |key: &'static str, layer: Option<indicator_overlay::Layer>| {
                overlay.set(key, layer.clone().filter(|_| lift_rows));
                layer.filter(|_| !lift_rows).map(|layer| layer())
            };
            let status_tail_lead = in_place(
                indicator_overlay::STATUS_TAIL_LEAD,
                self.status_tail_lead_layer(&p),
            );
            let row_motion = in_place(indicator_overlay::ROW_MOTION, self.row_motion_layer());
            Some(
                div()
                    .id("chat-transcript")
                    .role(gpui::Role::Document)
                    .aria_label("Conversation")
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .w_full()
                    .child(content)
                    // After the rows, so it draws where the status row painted this frame.
                    .children(status_tail_lead)
                    // The running tool calls' shimmer and spinner, over their rows (row_motion.rs).
                    .children(row_motion)
                    .children(lone_scroll_bottom)
                    .into_any_element(),
            )
        };
        if covered {
            // The composer is not drawn, so neither is its working card.
            overlay.set(indicator_overlay::WORKING_LEAD, None);
            overlay.set(indicator_overlay::WORKING_TRACK, None);
        }
        let composer = if !covered {
            self.render_composer_host(cx)
        } else if maximized {
            div().h(px(148.0 * s)).into_any_element()
        } else {
            div().into_any_element()
        };
        let bounds = self.bounds.clone();
        let picker_chat = cx.weak_entity();
        // Whatever a pane-frame change has to act on: the windows that cover the pane, and any open menu.
        let pane_frame_watched = self.pane_windows_open() || self.option_menu.is_some();
        let suggestion_shadow = self.render_suggestion_shadow(&p);
        let content_ready = self.error.is_none()
            && (rows > 0
                || matches!(
                    state["status"].as_str(),
                    Some("ready" | "working" | "empty")
                ));
        let session_id = self.config.sidebar_session_id.clone();
        let shell_session_id = self.config.shell_session_id;
        let app = self.config.app.clone();
        let pane_focused = self.pane_focused;
        let composer_ready = self.composer_ready;
        let account_switch_card = self.render_account_switch_card(&p, cx);
        // The transcript that places the floating controls' own windows is not drawn while
        // maximized, and outside window glass they are drawn in the pane (scroll_bottom.rs,
        // fork_branches.rs).
        if maximized || !glass {
            self.hide_frosted_overlays(cx);
        }
        let subagent_viewer = self.render_subagent_viewer(&p, window, cx);
        let edit_menu = self.edit_menu(window, cx);
        let session_details_dock = self.render_session_details_dock(&p, window, cx);
        // A coordinator's board docks between the chat and Session details (coordinator_board.rs).
        let board_dock = self.render_coordinator_board_dock(&p, cx);
        // A side chat says whose it is, with Bring back and Delete (side_chats.rs).
        let side_chat_bar = self.side_chat_bar(&p, cx);
        let pane_bounds = self.session_details_pane_bounds();
        let chat = div()
            .id("native-session-chat")
            .role(gpui::Role::Group)
            .aria_label("Session chat")
            /*
            CDXC:SessionChat 2026-09-28 WHY:
            User: clicking the chat transcript and then typing must send the keys to the chat box (the 2026-09-22 background typing decision in edit_shortcuts.rs). A press on empty transcript space hit nothing focusable, so focus stayed wherever it was and the keys never passed this view's capture listener, which is what hands them to the composer. The view takes focus itself; GPUI gives a press to the innermost focusable element, so the composer, the answer fields and the transcript text keep their own clicks.
            */
            .track_focus(&self.surface_focus)
            .tab_stop(false)
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_action(cx.listener(Self::handle_action))
            .font_family(p.font.clone())
            .text_size(px(14.0 * s))
            .line_height(px(22.75 * s))
            // Under window glass the transcript sits on the frosted column like the terminals do.
            .bg(if crate::app::helpers::window_glass_active_in(window) {
                gpui::transparent_black()
            } else {
                p.background
            })
            .text_color(p.primary)
            .capture_any_mouse_down(cx.listener(
                |chat, event: &gpui::MouseDownEvent, window, cx| {
                    super::focus::reclaim_keyboard_focus(window);
                    chat.note_short_pane_composer_press(event.position, cx);
                },
            ))
            .capture_action(cx.listener(Self::scroll_bottom_action))
            .capture_action(cx.listener(Self::tray_hotkey_action))
            .capture_action(cx.listener(Self::open_search_action))
            .capture_action(cx.listener(Self::chat_zoom_in_action))
            .capture_action(cx.listener(Self::chat_zoom_out_action))
            .capture_action(cx.listener(Self::chat_zoom_reset_action))
            .capture_key_down(cx.listener(Self::composer_key_down))
            .composer_input_actions(cx)
            .capture_key_up(cx.listener(|chat, _, _, _| chat.composer_held_key = None))
            .capture_action(cx.listener(Self::composer_copy))
            .capture_action(cx.listener(Self::composer_cut))
            .capture_action(cx.listener(Self::note_paste_started))
            .chat_edit_menu_actions(edit_menu)
            .on_drop(cx.listener(|chat, paths: &gpui::ExternalPaths, _, cx| {
                let paths = paths
                    .0
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                chat.attach_to_composer(Vec::new(), paths, cx);
                cx.stop_propagation();
            }))
            .when_some(error_card, |this, card| {
                this.child(div().p(px(16.0 * s)).child(card))
            })
            // Only React's one manual case: a transcript with no rows yet. A filled
            // one pages itself near the top and keeps its anchor (pagination.rs).
            .when(
                !covered && state["hasMore"] == true && self.list.item_count() == 0,
                |this| {
                    this.child(
                        div().flex().justify_center().child(
                            self.chat_button(
                                "load-earlier".into(),
                                if state["loadingEarlier"] == true {
                                    "Loading earlier turns…"
                                } else {
                                    "Load earlier turns"
                                }
                                .into(),
                                json!({"type":"loadEarlier"}),
                                &p,
                                cx,
                            ),
                        ),
                    )
                },
            )
            .children(side_chat_bar)
            // The search bar is a sibling region above the list, never an overlay on it.
            .when_some(search_bar, |this, bar| this.child(bar))
            // The transcript (or the welcome that stands in for an empty one), carrying the fork
            // button in its own top-right corner.
            .children(body)
            .child(composer)
            .children(suggestion_shadow)
            .child(
                gpui::canvas(
                    move |rect, window, cx| {
                        if bounds.replace(rect) != rect && pane_frame_watched {
                            let chat = picker_chat.clone();
                            window.defer(cx, move |_, cx| {
                                let _ = chat.update(cx, |chat, cx| {
                                    chat.pane_frame_changed(cx);
                                });
                            });
                        }
                    },
                    move |rect, _, _, cx| {
                        if rect.size.width > px(0.0) && rect.size.height > px(0.0) {
                            super::diagnostics::content_frame_painted(
                                &session_id,
                                rows,
                                content_ready,
                                composer_ready,
                                pane_focused,
                                || {
                                    app.as_ref()
                                        .and_then(|app| app.upgrade())
                                        .is_some_and(|app| {
                                            app.read(cx)
                                                .focused_agents_or_companion_shell_session_id()
                                                == Some(shell_session_id)
                                        })
                                },
                            );
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .when_some(account_switch_card, |this, card| this.child(card))
            // The subagent transcript is a modal over the whole pane, like the account-switch card.
            .when_some(subagent_viewer, |this, viewer| this.child(viewer));
        // The Session details panel docks beside the chat as its own column (session_details/).
        div()
            .id("native-session-chat-pane")
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .child(chat)
            .children(board_dock)
            .children(session_details_dock)
            .child(
                gpui::canvas(move |rect, _, _| pane_bounds.set(rect), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
    }
}
