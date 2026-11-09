//! The Bot API tab: whether an app or script may message this GBot with a key, Turn on (which
//! asks the user's approval and shows the key once, inside the command to paste), New key, and
//! Turn off.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! Coordinator brief (M11), on the user's behalf: GBot Settings has a "Bot API" section "showing the command to paste with the key filled in once", on the desktop, the web build and the phone. The key is shown only in the answer that made it (gxserver keeps its SHA-256 only), so this tab holds it until the window closes; afterwards the command shows a stand-in and New key makes another.
//! SEE-ALSO: server/src/gbots/api/ (`api`, `apiOn`, `apiNewKey`, `apiOff`), apps/mobile/app/src/screens/sessions-screen/GbotApiView.tsx (the phone's tab).
use super::super::native_modal_kit::*;
use super::parts::{page_header, row, section};
use super::window::{GbotSettingsModalCommand, GpuiGbotSettingsModalWindow};
use gpui::{
    AnyElement, AnyWindowHandle, App, ClipboardItem, Context, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, WeakEntity,
    Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

/// The Bot API tab's state inside the window.
#[derive(Default)]
pub(crate) struct GbotApiTab {
    /// `/api/gbots {op: "api"}` as gxserver last answered it; `None` until the first read.
    pub(super) status: Option<Value>,
    pub(super) requested: bool,
    /// The op on its way (`"api"` for a read).
    pub(super) busy: Option<&'static str>,
    pub(super) error: Option<String>,
    /// The key gxserver just made, shown until the window closes.
    pub(super) key: Option<String>,
    /// What was last copied, for the button's "Copied".
    pub(super) copied: Option<&'static str>,
}

/// Where a Bot API op's answer goes: the window that asked.
pub(crate) struct GbotApiReply {
    view: WeakEntity<GpuiGbotSettingsModalWindow>,
    window: AnyWindowHandle,
}

impl GbotApiReply {
    /// Hands the window gxserver's answer (the API's state, and the key when one was just made),
    /// or its refusal.
    pub(crate) fn finish(self, result: Result<Value, String>, cx: &mut App) {
        let view = self.view;
        let _ = self.window.update(cx, |_root, _window, cx| {
            view.update(cx, |modal, cx| modal.finish_bot_api(result, cx))
        });
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// "2026-11-09T00:49:24.943Z" as "9 Nov, 00:49" (UTC as gxserver stores it; the day is what
/// matters here).
fn when(at: &str) -> String {
    let date = at.get(..10).unwrap_or(at);
    let time = at.get(11..16).unwrap_or("");
    let mut parts = date.split('-');
    let (Some(_year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return at.to_string();
    };
    let month = match month {
        "01" => "Jan",
        "02" => "Feb",
        "03" => "Mar",
        "04" => "Apr",
        "05" => "May",
        "06" => "Jun",
        "07" => "Jul",
        "08" => "Aug",
        "09" => "Sep",
        "10" => "Oct",
        "11" => "Nov",
        _ => "Dec",
    };
    format!("{} {month}, {time}", day.trim_start_matches('0'))
}

impl GpuiGbotSettingsModalWindow {
    /// Reads the API's state the first time the tab shows.
    pub(super) fn ensure_bot_api(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.bot_api.requested {
            return;
        }
        self.bot_api.requested = true;
        self.bot_api_op("api", window, cx);
    }

    fn bot_api_op(&mut self, op: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.bot_api_op_with(op, None, window, cx);
    }

    fn bot_api_op_with(
        &mut self,
        op: &'static str,
        on: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.bot_api.busy = Some(op);
        self.bot_api.error = None;
        cx.notify();
        let reply = GbotApiReply {
            view: cx.entity().downgrade(),
            window: window.window_handle(),
        };
        (self.host)(
            GbotSettingsModalCommand::BotApi {
                bot_id: self.original.bot_id.clone(),
                op: (op != "api").then(|| {
                    let mut op = json!({ "op": op, "bot": self.original.bot_id });
                    if let Some(on) = on {
                        op["on"] = json!(on);
                    }
                    op
                }),
                reply,
            },
            cx,
        );
    }

    fn finish_bot_api(&mut self, result: Result<Value, String>, cx: &mut Context<Self>) {
        let op = self.bot_api.busy.take();
        match result {
            Ok(answer) => {
                if let Some(key) = answer["key"].as_str() {
                    self.bot_api.key = Some(key.to_string());
                } else if op == Some("apiOff") {
                    self.bot_api.key = None;
                }
                self.bot_api.copied = None;
                self.bot_api.status = Some(answer);
            }
            Err(message) => self.bot_api.error = Some(message),
        }
        cx.notify();
    }

    fn copy_bot_api(&mut self, what: &'static str, value: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(value));
        self.bot_api.copied = Some(what);
        cx.notify();
    }

    fn api_button(
        &self,
        id: &'static str,
        label: &str,
        tone: ModalButtonTone,
        disabled: bool,
        press: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .flex_shrink_0()
            .min_w(px(104.0))
            .child(modal_action_button(
                &self.kit,
                id,
                label.to_string(),
                None,
                tone,
                disabled,
                press,
                cx,
            ))
            .into_any_element()
    }

    /// The command to paste, in a code block, with its Copy button (and Copy key while the key is
    /// shown).
    fn api_command_block(&self, command: &str, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let copied = self.bot_api.copied;
        let command_owned = command.to_string();
        let mut buttons = h_flex().gap(px(8.0)).child(self.api_button(
            "gbot-api-copy-command",
            if copied == Some("command") {
                "Copied"
            } else {
                "Copy command"
            },
            ModalButtonTone::Neutral,
            false,
            move |this, _window, cx| this.copy_bot_api("command", command_owned.clone(), cx),
            cx,
        ));
        if let Some(key) = self.bot_api.key.clone() {
            buttons = buttons.child(self.api_button(
                "gbot-api-copy-key",
                if copied == Some("key") {
                    "Copied"
                } else {
                    "Copy key"
                },
                ModalButtonTone::Neutral,
                false,
                move |this, _window, cx| this.copy_bot_api("key", key.clone(), cx),
                cx,
            ));
        }
        v_flex()
            .w_full()
            .px(px(20.0))
            .py(px(14.0))
            .gap(px(10.0))
            .child(
                // Each line of the command stays whole (a wrapped line would leave its `\` alone
                // on the next); a narrow window scrolls the block sideways instead.
                div()
                    .id("gbot-api-command")
                    .w_full()
                    .min_w_0()
                    .overflow_x_scroll()
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.input_background()))
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(hsla(p.foreground))
                    .children(
                        command
                            .lines()
                            .map(|line| div().whitespace_nowrap().child(line.to_string())),
                    ),
            )
            .child(buttons)
            .into_any_element()
    }

    pub(super) fn render_bot_api(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let kit = self.kit;
        let name = match self.original.name.trim() {
            "" => "this GBot".to_string(),
            name => name.to_string(),
        };
        let mut column = v_flex()
            .w_full()
            .gap(px(16.0))
            .child(page_header(
                &p,
                "Bot API",
                &format!(
                    "Let an app or script message {name} with a key and get its reply. It stays off until you turn it on, and turning it on asks for your approval."
                ),
            ));
        if let Some(error) = &self.bot_api.error {
            column = column.child(modal_error(&kit, error.clone()));
        }
        let muted_line = |line: String| {
            div()
                .w_full()
                .px(px(2.0))
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(hsla(p.muted))
                .whitespace_normal()
                .child(line)
                .into_any_element()
        };
        let Some(status) = &self.bot_api.status else {
            return column
                .child(muted_line("Reading the Bot API…".to_string()))
                .into_any_element();
        };
        let api = &status["api"];
        let on = api["on"] == true;
        let busy = self.bot_api.busy.is_some();
        let opening = matches!(
            self.bot_api.busy,
            Some("apiOn" | "apiNewKey" | "apiShareMemory")
        );
        let switch_label = format!("Apps may message {name}");
        let switch_row = h_flex()
            .id("gbot-api-switch")
            .role(gpui::Role::Switch)
            .aria_toggled(a11y_toggled(on))
            .aria_label(switch_label.clone())
            .w_full()
            .switch_row_press(
                switch_row_wash(p.foreground),
                busy,
                bind_press(cx, move |this: &mut Self, window, cx| {
                    this.bot_api_op(if on { "apiOff" } else { "apiOn" }, window, cx)
                }),
            )
            .child(row(&p, &switch_label, modal_switch(&kit, on, busy)))
            .into_any_element();
        let mut rows = vec![switch_row];
        if on {
            let key = &api["key"];
            let mut used = vec![format!("Made {}", when(text(key, "createdAt")))];
            match key["lastUsedAt"].as_str() {
                Some(at) => used.push(format!(
                    "last used {} ({} calls)",
                    when(at),
                    key["uses"].as_i64().unwrap_or(0)
                )),
                None => used.push("not used yet".to_string()),
            }
            let shared = key["shareMemory"] == true;
            let share_label = format!("Apps may read {name}'s memory");
            rows.push(
                h_flex()
                    .id("gbot-api-share-memory")
                    .role(gpui::Role::Switch)
                    .aria_toggled(a11y_toggled(shared))
                    .aria_label(share_label.clone())
                    .w_full()
                    .switch_row_press(
                        switch_row_wash(p.foreground),
                        busy,
                        bind_press(cx, move |this: &mut Self, window, cx| {
                            this.bot_api_op_with("apiShareMemory", Some(!shared), window, cx)
                        }),
                    )
                    .child(row(&p, &share_label, modal_switch(&kit, shared, busy)))
                    .into_any_element(),
            );
            rows.push(row(
                &p,
                &used.join(" · "),
                self.api_button(
                    "gbot-api-new-key",
                    "New key",
                    ModalButtonTone::Neutral,
                    busy,
                    |this, window, cx| this.bot_api_op("apiNewKey", window, cx),
                    cx,
                ),
            ));
        }
        column = column.child(section(&p, "Access", rows));
        if opening {
            column = column.child(muted_line(
                "Waiting for your approval on this computer or your phone…".to_string(),
            ));
        }
        if on {
            let command = text(api, "command").to_string();
            let title = if self.bot_api.key.is_some() {
                "Copy the key now"
            } else {
                "Send a message"
            };
            column = column.child(section(
                &p,
                title,
                vec![self.api_command_block(&command, cx)],
            ));
            column = column.child(muted_line(if self.bot_api.key.is_some() {
                "Ghostex shows this key once and keeps only its fingerprint. Lost it? New key makes another, and the old one stops working.".to_string()
            } else {
                format!(
                    "The key was shown once, when it was made. New key makes another, and the old one stops working. The reply comes back in the answer; {name}'s chats for apps are titled \"API · …\"."
                )
            }));
        }
        column = column.child(
            v_flex()
                .w_full()
                .gap(px(6.0))
                .child(
                    div()
                        .px(px(2.0))
                        .text_size(px(13.5))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(p.foreground))
                        .child("Who can reach it"),
                )
                .child(muted_line(text(api, "reach").to_string()))
                .child(muted_line(format!(
                    "A message from an app is information, never your instructions: {name}'s chats for apps start in their own folder, can't read your secret folders or Ghostex's own files, don't read its memory unless you let them, never save to it, and only you answer what they wait on."
                ))),
        );
        div()
            .id("gbot-settings-api-scroll")
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.page_scroll)
            .child(column.pr(px(16.0)))
            .into_any_element()
    }
}
