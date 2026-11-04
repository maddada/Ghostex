use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use serde_json::{Value, json};

/// CDXC:SessionChat 2026-10-20 DECISION:
/// User: "please remove the summary mode button from the composer (it's already there under view mode) also please remove the stash prompt button and instead move it under the ... more actions button as the 2nd from the bottom there". Summary mode lives only in More actions > View, Stash prompt is a More actions row (actions.rs, `stash_menu_row`); supersedes the 2026-09-11 decision that put Summary mode on this row.
/// This order is also the order controls fold into More actions. Files and links left this row for More actions on 2026-10-15 (actions.rs, `files_links_row`).
///
/// CDXC:SessionChat 2026-10-20 DECISION:
/// User: "Please make the dictation button always show up here. At the bottom right next to the send button." Dictate is not in this list: `render_toolbar` draws it right before Send and it never folds into More actions. The phone does the same (apps/mobile/app/src/chat/native/composer/menus.ts).
pub(super) const COMPOSER_CONTROLS: [(&str, &str, &str, &str); 4] = [
    ("note", "sessionNote", "Session note", "titlebar/note.svg"),
    (
        "attach",
        "attachPath",
        "Attach a file or folder",
        "titlebar/paperclip.svg",
    ),
    (
        "maximize",
        "maximizeComposer",
        "Maximize",
        "titlebar/maximize.svg",
    ),
    (
        "terminal",
        "terminalView",
        "Terminal View",
        "titlebar/terminal-2.svg",
    ),
];

/// The Copy Report pill's width at scale 1 (icon, gap, label and padding), reserved while it shows.
const COPY_REPORT_PILL_WIDTH: f32 = 108.0;

/// Where the composer buttons that open a menu were last painted: each menu lines up with its
/// button's right edge (`MenuSide::Right`).
#[derive(Default)]
pub(super) struct ComposerButtonBounds {
    pub(super) more_actions: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    pub(super) attach: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    pub(super) send: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    pub(super) dictate: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
}

impl ComposerButtonBounds {
    pub(super) fn cell(
        &self,
        action: &str,
    ) -> Option<std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>> {
        match action {
            "moreActions" => Some(self.more_actions.clone()),
            "attachPath" => Some(self.attach.clone()),
            "dictate" => Some(self.dictate.clone()),
            _ => None,
        }
    }
}

/// The button's painted bounds, or a point at `position` before its first paint.
pub(super) fn button_trigger(
    bounds: gpui::Bounds<gpui::Pixels>,
    position: gpui::Point<gpui::Pixels>,
) -> gpui::Bounds<gpui::Pixels> {
    if bounds.size.width > px(0.0) {
        bounds
    } else {
        gpui::Bounds::new(position, gpui::size(px(0.0), px(0.0)))
    }
}

/// The Linux Attach button's trigger id, so a second press closes its Files/Folders menu.
const ATTACH_PATH_TRIGGER: &str = "chat-attach-path";

impl NativeChatView {
    pub(super) fn composer_control_overflowed(&self, id: &str) -> bool {
        self.snapshot["composerOverflow"]["overflowed"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|value| value.as_str() == Some(id)))
    }

    /// React rendered each of these controls only when the host handed it the handler
    /// (`onSessionNote`, `onStash`, `onAttach`, the Terminal View switch); the core projects
    /// the same answer under `composerActions` so a control that cannot do anything is not drawn.
    pub(super) fn composer_control_available(&self, id: &str) -> bool {
        // Dictation is the platform's own (`dictation.rs`), so the core does not gate it.
        if id == "dictate" {
            return super::dictation::dictation_drawn();
        }
        let actions = &self.snapshot["composerActions"];
        actions.is_null() || actions[id] != false
    }

    /// Stash prompt: stashes the draft, or opens the saved prompts when the chat box is empty.
    /// The More actions row, the Stash Prompt hotkey and the toolbar all end here.
    pub(crate) fn stash_prompt_or_open_saved(&mut self, cx: &mut Context<Self>) {
        if self.draft.trim().is_empty() {
            self.host("stashedPrompts", json!({}), cx);
        } else {
            self.invoke(json!({"type":"stash","text":self.draft,"draftVersion":{"draftId":self.draft_id,"revision":self.draft_revision}}),cx);
        }
    }

    pub(super) fn perform_composer_action(
        &mut self,
        action: &str,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            "moreActions" => self.show_actions(position, window, cx),
            "maximizeComposer" => self.toggle_maximized(window, cx),
            "dictate" => self.toggle_dictation(window, cx),
            "transcribePreviousDictation" => self.transcribe_previous_dictation(window, cx),
            "dictationHistory" => self.open_dictation_history(cx),
            "attachPath"
                if cfg!(target_os = "linux")
                    && self.chat_menu_toggled_shut(ATTACH_PATH_TRIGGER, cx) => {}
            "attachPath" if cfg!(target_os = "linux") => {
                self.menu_side = Some(super::option_menu::MenuSide::Right);
                let trigger = button_trigger(self.composer_button_bounds.attach.get(), position);
                self.show_chat_menu(vec![
                    json!({"label":"Images or files…","command":{"type":"host","action":"pickAttachments","requestId":"native-chat-attachments","selection":"files"}}),
                    json!({"label":"Folders…","command":{"type":"host","action":"pickAttachments","requestId":"native-chat-attachments","selection":"folders"}}),
                ], trigger, 200.0, window, cx)
            }
            "attachPath" => self.host(
                "pickAttachments",
                json!({"requestId":"native-chat-attachments"}),
                cx,
            ),
            "stashPrompt" => self.stash_prompt_or_open_saved(cx),
            "sessionNote" => self.invoke(json!({"type":"toggleNote"}), cx),
            "filesLinks" => self.invoke(json!({"type":"filesLinks"}), cx),
            _ => self.host(action, json!({}), cx),
        }
    }

    pub(super) fn render_toolbar(&self, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let mut toolbar = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.0 * p.scale))
            .child(self.host_button("moreActions", "titlebar/dots.svg", p, cx));
        for (id, action, _, icon) in COMPOSER_CONTROLS {
            if self.composer_animation.shown_collapsed()
                || self.composer_control_overflowed(id)
                || !self.composer_control_available(id)
            {
                continue;
            }
            let icon = match id {
                "maximize" if self.maximized_window.is_some() => "titlebar/minimize.svg",
                _ => icon,
            };
            // CDXC:SessionChat 2026-10-25 DECISION: User: Copy Report shows "when the chat composer shows red", i.e. while the Terminal View glyph reads not ready, so its pill sits right before that glyph. The phone's NativeComposer.tsx matches.
            if id == "terminal" && self.copy_report_pill_shown() {
                toolbar = toolbar.child(self.render_copy_report_pill(p, cx));
            }
            // The Terminal View control also carries the agent CLI's readiness tint and screen preview.
            toolbar = toolbar.child(if id == "terminal" {
                self.render_terminal_view_button(p, cx)
            } else {
                self.host_button(action, icon, p, cx)
            });
        }
        if !self.composer_animation.shown_collapsed() && self.composer_control_available("dictate")
        {
            let icon = if self.dictating(cx) {
                "titlebar/player-stop.svg"
            } else {
                "titlebar/microphone.svg"
            };
            toolbar = toolbar.child(self.host_button("dictate", icon, p, cx));
        }
        toolbar
            .child(self.render_send_control(p, cx))
            .into_any_element()
    }

    pub(super) fn composer_measurement(
        &self,
        p: &ChatAppearance,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let options_width = self.option_pills_width(p, window);
        let scale = p.scale;
        // With the merged model picker there is no options pill to move into More actions.
        let has_overflow_options = (self.snapshot["optionLabels"]["showOptions"] == true
            && !self.snapshot["modelMenu"].is_object())
            || self.snapshot["contextMeter"].is_object();
        // A control the host cannot serve is never drawn, so it must not claim room either.
        let controls: Vec<Value> = COMPOSER_CONTROLS
            .iter()
            .filter(|(id, _, _, _)| self.composer_control_available(id))
            .map(|(id, _, _, _)| json!({"id":id,"width":28.0*scale}))
            .collect();
        // More actions, each drawn control, Dictate (never folded) and Send, with the gaps between them.
        let buttons = 1 + controls.len() + usize::from(self.composer_control_available("dictate"));
        let report_pill = if self.copy_report_pill_shown() {
            COPY_REPORT_PILL_WIDTH + 6.0
        } else {
            0.0
        };
        let actions_width = (buttons as f32 * (28.0 + 6.0) + 24.0 + report_pill) * scale;
        let chat = cx.weak_entity();
        gpui::canvas(
            move |bounds, window, cx| {
                let measurements = json!({
                    "available":bounds.size.width.as_f32(), "options":options_width,
                    "actions":actions_width, "footerGap":8.0*scale, "actionGap":6.0*scale,
                    "clearance":16.0*scale, "hasOverflowOptions":has_overflow_options,
                    "controls":controls.clone(),
                });
                let chat = chat.clone();
                window.defer(cx, move |_, cx| {
                    let _ = chat.update(cx, |chat, cx| {
                        if chat.composer_measurements.as_ref() == Some(&measurements) {
                            return;
                        }
                        // CDXC:SessionChat 2026-10-10 WHY: while a panel slides the composer's width changes every frame, and each measurement the core hears makes it rebuild the whole thread's document, which in a very long thread made the slide stutter. The fit below is applied every frame; the core hears the width the slide settles at (the settling frame refreshes every view).
                        let sliding = crate::terminal_element::grid_resize_held();
                        if !sliding {
                            chat.composer_measurements = Some(measurements.clone());
                        }
                        // Geometry is available before the session is. Apply the fit rules
                        // immediately, including while the controller is still booting.
                        if let Ok(measured) = serde_json::from_value(measurements.clone()) {
                            let fit = json!(
                                ghostex_gx_chat_core::composer::layout::fit_composer_controls(
                                    &measured,
                                )
                            );
                            if chat.snapshot["composerOverflow"] != fit {
                                std::sync::Arc::make_mut(&mut chat.snapshot)["composerOverflow"] =
                                    fit;
                                cx.notify();
                            }
                        }
                        if chat.composer_ready && !sliding {
                            chat.invoke(
                                json!({"type":"measureComposer","measurements":measurements}),
                                cx,
                            );
                        }
                    });
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element()
    }
}
