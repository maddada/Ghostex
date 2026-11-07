//! The chat header's Share popover (Ghostex Collab): the chat's link and code with Copy, what
//! guest messages do (Send, Ask me, View only), who is watching with Remove, when the link ends,
//! and Stop sharing. Before the chat is shared it offers Create link, and while Ghostex Collab is
//! off, its switch. Everything is read from and saved to gxserver (`/api/collab`,
//! server/src/server/collab_http.rs).
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User (Q1615): "Where Share lives: a button in the chat header", with its popover holding the link, code, expiry, mode, viewers and Stop sharing.
//! SEE-ALSO: apps/desktop/src/app/render/workarea_header/share_button.rs (the button), apps/desktop/src/app/window/settings_modal/tabs/collab.rs (Settings > Collab).

use super::native_modal_kit::{ModalPalette, ModalSegmentedItem, modal_segmented_control};
use super::resources_style::*;
use crate::app::gx_store::gx_rpc;
use crate::app::helpers::*;
use crate::*;
use ghostex_gx_core::SessionKey;
use serde_json::{Value, json};

pub(crate) const COLLAB_SHARE_POPUP_WIDTH: f32 = 380.0;
pub(crate) const COLLAB_SHARE_POPUP_HEIGHT: f32 = 392.0;

const MODES: [(&str, &str); 3] = [
    ("send", "Send"),
    ("approve", "Ask me"),
    ("view", "View only"),
];

pub(crate) struct CollabSharePanel {
    key: SessionKey,
    state: Option<Value>,
    error: Option<String>,
    busy: bool,
    copied: Option<(&'static str, std::time::Instant)>,
}

fn share_of<'a>(state: &'a Value, key: &SessionKey) -> Option<&'a Value> {
    state["shares"].as_array()?.iter().find(|share| {
        share["projectId"] == key.project_id.as_str()
            && share["sessionId"] == key.session_id.as_str()
    })
}

fn ends_label(expires_at_ms: Option<i64>) -> String {
    let Some(at) = expires_at_ms else {
        return "The link works until you stop sharing".to_string();
    };
    let left = (at - chrono::Utc::now().timestamp_millis()).max(0) / 60_000;
    match left {
        0 => "The link ends in under a minute".to_string(),
        1..=59 => format!("The link ends in {left} min"),
        60..=2879 => format!("The link ends in {} h", (left + 30) / 60),
        _ => format!("The link ends in {} days", (left + 720) / 1440),
    }
}

fn spaced_code(code: &str) -> String {
    if code.len() == 6 {
        format!("{} {}", &code[..3], &code[3..])
    } else {
        code.to_string()
    }
}

fn panel_button(id: String, label: &str, strong: bool) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id(id)
        .flex_shrink_0()
        .h(px(26.0))
        .px(px(10.0))
        .items_center()
        .justify_center()
        .rounded(px(RESOURCE_CONTROL_RADIUS))
        .border_1()
        .border_color(chrome_ink().opacity(if strong { 0.22 } else { 0.13 }))
        .bg(chrome_ink().opacity(if strong { 0.16 } else { 0.08 }))
        .text_size(px(12.0))
        .text_color(chrome_ink().opacity(0.92))
        .cursor_pointer()
        .hover(|this| this.bg(chrome_ink().opacity(if strong { 0.22 } else { 0.14 })))
        .child(label.to_string())
}

impl CollabSharePanel {
    pub(crate) fn new(key: SessionKey, cx: &mut gpui::Context<Self>) -> Self {
        cx.spawn(async |this, cx| {
            loop {
                let _ = this.update(cx, |panel, cx| panel.call(json!({"op": "read"}), cx));
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this.update(cx, |_, _| ()).is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            key,
            state: None,
            error: None,
            busy: false,
            copied: None,
        }
    }

    fn call(&mut self, params: Value, cx: &mut gpui::Context<Self>) {
        let write = params["op"] != "read";
        if write {
            self.busy = true;
            cx.notify();
        }
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/collab", params).await;
            let _ = this.update(cx, |panel, cx| {
                if write {
                    panel.busy = false;
                }
                match result {
                    Ok(value) => {
                        panel.error = None;
                        // `share` answers with the one share; read again for the whole picture.
                        if value.get("share").is_some() {
                            panel.call(json!({"op": "read"}), cx);
                        } else {
                            panel.state = Some(value);
                        }
                    }
                    Err(error) => panel.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn copy(&mut self, what: &'static str, text: String, cx: &mut gpui::Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        self.copied = Some((what, std::time::Instant::now()));
        cx.notify();
        cx.spawn(async |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1600))
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.copied = None;
                cx.notify();
            });
        })
        .detach();
    }

    fn copied(&self, what: &str) -> bool {
        self.copied.is_some_and(|(copied, _)| copied == what)
    }

    fn ids(&self) -> Value {
        json!({"projectId": self.key.project_id, "sessionId": self.key.session_id})
    }

    fn row(label: &str, value: impl IntoElement, action: Option<AnyElement>) -> gpui::Div {
        h_flex()
            .w_full()
            .min_h(px(34.0))
            .gap(px(8.0))
            .items_center()
            .child(
                div()
                    .w(px(64.0))
                    .flex_shrink_0()
                    .text_size(px(12.0))
                    .text_color(chrome_ink().opacity(0.58))
                    .child(label.to_string()),
            )
            .child(div().flex_1().min_w_0().child(value))
            .children(action)
    }

    fn render_off(&self, cx: &mut gpui::Context<Self>) -> gpui::Div {
        v_flex()
            .gap(px(10.0))
            .p(px(14.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(chrome_ink().opacity(0.72))
                    .child("Ghostex Collab is off. Turn it on to share a chat through a link that opens it in any browser, with no app or account."),
            )
            .child(
                h_flex().child(
                    panel_button("collab-share-turn-on".into(), "Turn on Ghostex Collab", true).on_click(
                        cx.listener(|panel, _, _, cx| {
                            if !panel.busy {
                                panel.call(json!({"op": "set", "patch": {"enabled": true}}), cx);
                            }
                        }),
                    ),
                ),
            )
    }

    fn render_unshared(&self, config: &Value, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let mode = config["mode"].as_str().unwrap_or("approve");
        let mode_line = match mode {
            "send" => "Their messages go straight to the agent.",
            "view" => "They can watch, but not write.",
            _ => "You approve each message they send.",
        };
        let code = if config["requireCode"] == true {
            " They also type a six-digit code you give them."
        } else {
            ""
        };
        v_flex()
            .gap(px(10.0))
            .p(px(14.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(chrome_ink().opacity(0.72))
                    .child(format!(
                        "Anyone you send the link to can follow this chat live in their browser. {mode_line}{code}"
                    )),
            )
            .child(
                h_flex().child(
                    panel_button("collab-share-create".into(), "Create link", true).on_click(cx.listener(
                        |panel, _, _, cx| {
                            if !panel.busy {
                                let mut params = panel.ids();
                                params["op"] = json!("share");
                                panel.call(params, cx);
                            }
                        },
                    )),
                ),
            )
    }

    fn render_shared(
        &self,
        share: &Value,
        connected: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let link = share["link"].as_str().unwrap_or_default().to_string();
        let code = share["code"].as_str().unwrap_or_default().to_string();
        let share_id = share["shareId"].as_str().unwrap_or_default().to_string();
        let mode = share["mode"].as_str().unwrap_or("approve");
        let selected = MODES.iter().position(|(id, _)| *id == mode).unwrap_or(1);
        let mut palette = ModalPalette::resolve(chrome_uses_light_appearance(), None);
        palette.glass = window_glass_active();
        let mode_share = share_id.clone();
        let segmented = modal_segmented_control(
            &palette,
            "collab-share-mode",
            &MODES.map(|(_, label)| ModalSegmentedItem { icon: None, label }),
            selected,
            move |panel: &mut Self, index, _, cx| {
                panel.call(
                    json!({"op": "setMode", "shareId": mode_share, "mode": MODES[index].0}),
                    cx,
                );
            },
            cx,
        );
        let link_copy = link.clone();
        let code_copy = code.clone();
        let mut body =
            v_flex()
                .gap(px(4.0))
                .px(px(14.0))
                .pt(px(10.0))
                .child(Self::row(
                    "Link",
                    resource_detail_text()
                        .text_color(chrome_ink().opacity(0.86))
                        .child(link.clone()),
                    Some(
                        panel_button(
                            "collab-share-copy-link".into(),
                            if self.copied("link") {
                                "Copied"
                            } else {
                                "Copy"
                            },
                            true,
                        )
                        .on_click(cx.listener(move |panel, _, _, cx| {
                            panel.copy("link", link_copy.clone(), cx)
                        }))
                        .into_any_element(),
                    ),
                ));
        if !code.is_empty() {
            body =
                body.child(Self::row(
                    "Code",
                    div()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(chrome_ink().opacity(0.94))
                        .child(spaced_code(&code)),
                    Some(
                        panel_button(
                            "collab-share-copy-code".into(),
                            if self.copied("code") {
                                "Copied"
                            } else {
                                "Copy"
                            },
                            false,
                        )
                        .on_click(cx.listener(move |panel, _, _, cx| {
                            panel.copy("code", code_copy.clone(), cx)
                        }))
                        .into_any_element(),
                    ),
                ));
        }
        body = body
            .child(
                v_flex()
                    .gap(px(6.0))
                    .pt(px(8.0))
                    .child(resource_section_heading().px_0().child("Guest messages"))
                    .child(segmented),
            )
            .child(
                resource_section_heading()
                    .px_0()
                    .pt(px(8.0))
                    .child("Watching"),
            );
        let viewers = share["viewers"].as_array().cloned().unwrap_or_default();
        if viewers.is_empty() {
            body = body.child(resource_detail_text().py(px(4.0)).child(if connected {
                "Nobody yet"
            } else {
                "Connecting to the share relay\u{2026}"
            }));
        }
        for viewer in viewers {
            let id = viewer["id"].as_str().unwrap_or_default().to_string();
            let name = viewer["name"].as_str().unwrap_or_default().to_string();
            let remove_share = share_id.clone();
            body = body.child(
                h_flex()
                    .w_full()
                    .h(px(30.0))
                    .gap(px(8.0))
                    .items_center()
                    .child(
                        div()
                            .size(px(6.0))
                            .rounded_full()
                            .bg(chrome_color(0x7acb9d, 0x287a48)),
                    )
                    .child(resource_name_text().flex_1().child(name))
                    .child(
                        panel_button(format!("collab-share-remove-{id}"), "Remove", false).on_click(cx.listener(
                            move |panel, _, _, cx| {
                                panel.call(
                                    json!({"op": "removeViewer", "shareId": remove_share, "viewerId": id}),
                                    cx,
                                );
                            },
                        )),
                    ),
            );
        }
        let stop_share = share_id;
        v_flex()
            .flex_1()
            .min_h_0()
            .child(body)
            .child(div().flex_1())
            .child(
                h_flex()
                    .flex_shrink_0()
                    .px(px(14.0))
                    .py(px(10.0))
                    .gap(px(8.0))
                    .items_center()
                    .border_t_1()
                    .border_color(resource_card_border())
                    .child(
                        resource_detail_text()
                            .flex_1()
                            .child(ends_label(share["expiresAtMs"].as_i64())),
                    )
                    .child(
                        panel_button("collab-share-stop".into(), "Stop sharing", false)
                            .text_color(chrome_color(0xe28b88, 0xb43d39))
                            .on_click(cx.listener(move |panel, _, _, cx| {
                                panel.call(json!({"op": "stop", "shareId": stop_share}), cx);
                            })),
                    ),
            )
    }
}

impl Render for CollabSharePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let state = self.state.clone().unwrap_or(Value::Null);
        let enabled = state["config"]["enabled"] == true;
        let connected = state["relayConnected"] == true;
        let share = share_of(&state, &self.key).cloned();
        let status = match (&share, connected) {
            (Some(_), true) => Some(("Live", 0x7acb9d)),
            (Some(_), false) => Some(("Connecting", 0xd5ae6b)),
            _ => None,
        };
        let body = if self.state.is_none() {
            v_flex()
                .p(px(14.0))
                .child(resource_detail_text().child("Loading\u{2026}"))
        } else if !enabled {
            self.render_off(cx)
        } else if let Some(share) = &share {
            self.render_shared(share, connected, cx)
        } else {
            self.render_unshared(&state["config"], cx)
        };
        resource_panel_frame().child(
            v_flex()
                .size_full()
                .overflow_hidden()
                .child(
                    resource_header()
                        .child(
                            resource_heading()
                                .child(
                                    svg()
                                        .path("titlebar/users.svg")
                                        .size(px(16.0))
                                        .text_color(chrome_ink().opacity(0.9)),
                                )
                                .child(div().truncate().child("Share this chat")),
                        )
                        .when_some(status, |header, (label, color)| {
                            header.child(
                                h_flex()
                                    .flex_shrink_0()
                                    .px(px(12.0))
                                    .gap(px(6.0))
                                    .items_center()
                                    .text_size(px(11.5))
                                    .text_color(chrome_ink().opacity(0.7))
                                    .child(div().size(px(6.0)).rounded_full().bg(chrome_color(
                                        color,
                                        if color == 0x7acb9d {
                                            0x287a48
                                        } else {
                                            0x8a5c16
                                        },
                                    )))
                                    .child(label),
                            )
                        }),
                )
                .child(body)
                .when_some(self.error.clone(), |panel, error| {
                    panel.child(
                        div()
                            .px(px(14.0))
                            .pb(px(10.0))
                            .text_size(px(12.0))
                            .text_color(chrome_color(0xe28b88, 0xb43d39))
                            .child(error),
                    )
                }),
        )
    }
}
