use super::keyboard::ComposerInputActions as _;
use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::StatefulInteractiveElement as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, Focusable as _, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use serde_json::json;

impl NativeChatView {
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
        super::search::register(cx);
        super::zoom::register(cx);
        self.last_render = Some(web_time::Instant::now());
        self.schedule_row_detail_sync(window, cx);
        self.note_drawn_in(window.window_handle(), cx);
        if self.maximized_window.is_none() {
            self.ensure_input(window, cx);
        }
        self.sync_chat_zoom_default();
        let p = ChatAppearance::current(&self.snapshot).on_window_glass(
            crate::app::helpers::window_glass_active_for(self.main_window),
        );
        let s = p.scale;
        self.sync_search_scroll();
        // A card above the composer that is opening or closing needs the next frame; the
        // transcript's rows ask for theirs as they are drawn.
        if self.disclosure_motion.borrow().running() {
            window.request_animation_frame();
        }
        self.sync_disclosure_anchor(window);
        /*
        CDXC:SessionChat 2026-09-18 WHY:
        React's maximized composer is a fixed overlay across the whole chat pane, so nothing of the
        conversation is left around it. The native one is a pane-sized child window over a
        translucent scrim, which leaves this pane painting underneath it: the transcript's rails,
        minimap and fork button showed through the margins. While it is up, the pane behind renders
        its background only.
        */
        let maximized = self.maximized_window.is_some();
        let glass = crate::app::helpers::window_glass_active_in(window);
        // Under glass the subagent viewer sits straight on the pane's glass, so the chat behind it
        // is not painted (subagent_view.rs, `render_subagent_viewer`).
        let covered = maximized || (glass && self.snapshot["subagent"].is_object());
        let search_bar = if covered {
            None
        } else {
            self.render_search_bar(&p, window, cx)
        };
        let fork_branch_badge = if covered {
            None
        } else {
            self.render_fork_branch_badge(&p, glass, cx)
        };
        let state = self.snapshot.clone();
        let error = self.error.clone();
        self.transcript_inset = if maximized {
            0.0
        } else {
            self.composer_frame(cx).transcript_inset
        };
        crate::app::helpers::indicator_animation::render_indicator_frames_animation_only(
            cx.entity_id(),
        );
        let transcript = self.render_transcript_host(window, cx);
        let rows = self.list.item_count();
        /*
        CDXC:SessionFork 2026-09-21 WHY:
        The fork button is placed against the conversation's own region, the transcript or the
        empty-session welcome that replaces it, rather than against the pane: anchored to the pane
        it would have covered the search bar and the "load earlier turns" row while those are up,
        and anchored to the transcript alone a forked session with no rows yet would have lost its
        switcher entirely.
        */
        let body = if covered {
            None
        } else {
            let content = transcript;
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
                    .children(fork_branch_badge)
                    .into_any_element(),
            )
        };
        let composer = if !covered {
            self.render_composer(&p, window, cx)
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
        div()
            .id("native-session-chat")
            .role(gpui::Role::Group)
            .aria_label("Session chat")
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
            .capture_action(cx.listener(Self::open_search_action))
            .capture_action(cx.listener(Self::chat_zoom_in_action))
            .capture_action(cx.listener(Self::chat_zoom_out_action))
            .capture_action(cx.listener(Self::chat_zoom_reset_action))
            .capture_key_down(cx.listener(Self::composer_key_down))
            .composer_input_actions(cx)
            .capture_key_up(cx.listener(|chat, _, _, _| chat.composer_held_key = None))
            .capture_action(cx.listener(Self::paste_attachments))
            .capture_action(cx.listener(Self::composer_copy))
            .capture_action(cx.listener(Self::composer_cut))
            .on_drop(cx.listener(|chat, paths: &gpui::ExternalPaths, _, cx| {
                let paths = paths
                    .0
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                chat.invoke(serde_json::json!({"type":"attachPaths","paths":paths}), cx);
                cx.stop_propagation();
            }))
            .when_some(error, |this, error| {
                this.child(
                    div()
                        .p(px(16.0 * s))
                        .text_color(gpui::rgb(0xef9999))
                        .child(error),
                )
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
            .when_some(subagent_viewer, |this, viewer| this.child(viewer))
    }
}
