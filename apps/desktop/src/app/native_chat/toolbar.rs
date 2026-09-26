use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    Window, div, px,
};
use serde_json::{Value, json};

/// CDXC:SessionChat 2026-09-11 DECISION:
/// User: show Summary mode between More actions and Session note when there is room; keep it in More actions in the compact toolbar.
/// This order is also the order controls fold into More actions.
pub(super) const COMPOSER_CONTROLS: [(&str, &str, &str, &str); 6] = [
    (
        "summary",
        "summaryMode",
        "Summary mode",
        "titlebar/list-details.svg",
    ),
    ("note", "sessionNote", "Session note", "titlebar/note.svg"),
    (
        "stash",
        "stashPrompt",
        "Stash prompt",
        "titlebar/stack-push.svg",
    ),
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
        let actions = &self.snapshot["composerActions"];
        actions.is_null() || actions[id] != false
    }

    pub(super) fn perform_composer_action(
        &mut self,
        action: &str,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            "moreActions" => self.show_actions(position,window,cx),
            "maximizeComposer" => self.toggle_maximized(window,cx),
            "summaryMode" => self.invoke(json!({"type":"toggleSummary"}),cx),
            "attachPath" if cfg!(target_os = "linux") => self.show_chat_menu_at(vec![
                json!({"label":"Images or files…","command":{"type":"host","action":"pickAttachments","requestId":"native-chat-attachments","selection":"files"}}),
                json!({"label":"Folders…","command":{"type":"host","action":"pickAttachments","requestId":"native-chat-attachments","selection":"folders"}}),
            ], position, 200.0, window, cx),
            "attachPath" => self.host("pickAttachments",json!({"requestId":"native-chat-attachments"}),cx),
            "stashPrompt" if !self.draft.trim().is_empty() => self.invoke(json!({"type":"stash","text":self.draft,"draftVersion":{"draftId":self.draft_id,"revision":self.draft_revision}}),cx),
            "stashPrompt" => self.host("stashedPrompts",json!({}),cx),
            "sessionNote" => self.invoke(json!({"type":"toggleNote"}),cx),
            _ => self.host(action,json!({}),cx),
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
            if self.composer_collapsed()
                || self.composer_control_overflowed(id)
                || !self.composer_control_available(id)
            {
                continue;
            }
            let icon = match id {
                "maximize" if self.maximized_window.is_some() => "titlebar/minimize.svg",
                // The Summary glyph swaps with the mode, the way the More actions row does.
                "summary" if self.snapshot["summaryMode"] == true => "titlebar/list-check.svg",
                _ => icon,
            };
            // The Terminal View control also carries the agent CLI's readiness tint and screen preview.
            toolbar = toolbar.child(if id == "terminal" {
                self.render_terminal_view_button(p, cx)
            } else {
                self.host_button(action, icon, p, cx)
            });
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
        let chat = cx.weak_entity();
        gpui::canvas(
            move |bounds, window, cx| {
                let measurements = json!({
                    "available":bounds.size.width.as_f32(), "options":options_width,
                    "actions":262.0*scale, "footerGap":8.0*scale, "actionGap":6.0*scale,
                    "clearance":16.0*scale, "hasOverflowOptions":has_overflow_options,
                    "controls":controls.clone(),
                });
                let chat = chat.clone();
                window.defer(cx, move |_, cx| {
                    let _ = chat.update(cx, |chat, cx| {
                        if chat.composer_measurements.as_ref() == Some(&measurements) {
                            return;
                        }
                        chat.composer_measurements = Some(measurements.clone());
                        // Geometry is available before the session is. Apply the fit rules
                        // immediately, including while the controller is still booting.
                        if let Ok(measured) = serde_json::from_value(measurements.clone()) {
                            let fit = ghostex_gx_chat_core::composer::layout::fit_composer_controls(
                                &measured,
                            );
                            std::sync::Arc::make_mut(&mut chat.snapshot)["composerOverflow"] =
                                json!(fit);
                            cx.notify();
                        }
                        if chat.composer_ready {
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
