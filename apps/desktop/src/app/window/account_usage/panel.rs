use super::{data::*, style::*};
use crate::app::titlebar::account_usage::popup_account;
use crate::*;
use gpui::img;
use gpui_component::scroll::{Scrollbar, ScrollbarShow};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

pub(crate) struct AccountUsagePanel {
    pub(super) main_app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(super) id: ExtensionId,
    pub(super) account: Value,
    pub(super) palette: Palette,
    pub(super) models_open: bool,
    pub(super) resets_open: bool,
    pub(super) redeem_pending: bool,
    pub(super) model_focus: FocusHandle,
    pub(super) reset_focus: FocusHandle,
    pub(super) redeem_focus: FocusHandle,
    focus: FocusHandle,
    scroll: ScrollHandle,
    background: Option<(Size<Pixels>, Arc<gpui::Image>)>,
    logo: Arc<gpui::Image>,
    pub(super) now: i64,
}

impl AccountUsagePanel {
    pub(crate) fn new(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        id: ExtensionId,
        account: Value,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let codex = text(&account, "provider") == "codex";
        let logo = if codex {
            include_str!("../../../../assets/account-usage/codex.svg").replace("#ffffff", "#8cbbe8")
        } else {
            include_str!("../../../../assets/account-usage/claude.svg").to_string()
        };
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        this.now = now();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            main_app,
            id,
            account: popup_account(&account),
            palette: Palette::new(codex),
            models_open: false,
            resets_open: false,
            redeem_pending: false,
            model_focus: cx.focus_handle().tab_index(0),
            reset_focus: cx.focus_handle().tab_index(0),
            redeem_focus: cx.focus_handle().tab_index(0),
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            background: None,
            logo: Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Svg,
                logo.into_bytes(),
            )),
            now: now(),
        }
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    pub(crate) fn update_account(&mut self, account: Value, cx: &mut gpui::Context<Self>) {
        self.account = popup_account(&account);
        self.now = now();
        cx.notify();
    }

    pub(crate) fn dismiss_reset_menu(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.resets_open {
            return false;
        }
        self.resets_open = false;
        self.reset_focus.focus(window, cx);
        cx.notify();
        true
    }

    pub(super) fn toggle_models(&mut self, cx: &mut gpui::Context<Self>) {
        self.models_open = !self.models_open;
        self.resets_open = false;
        cx.notify();
    }

    pub(super) fn toggle_resets(&mut self, cx: &mut gpui::Context<Self>) {
        self.resets_open = !self.resets_open;
        cx.notify();
    }

    pub(super) fn redeem(&mut self, cx: &mut gpui::Context<Self>) {
        if self.redeem_pending
            || reset_credits(&self.account).is_empty()
            || self.account["status"] != "ready"
        {
            return;
        }
        self.redeem_pending = true;
        cx.notify();
        let id = self.id;
        let app = self.main_app.clone();
        // Leave the panel's render/event borrow before the action closes its window.
        cx.defer(move |cx| {
            let _ = app.update_in(cx, |app, window, cx| {
                if !app.titlebar_popup_menu_open(GpuiTitlebarPopupKind::AccountUsage(id)) {
                    return;
                }
                if let Some(account) = app
                    .titlebar_accounts
                    .iter()
                    .find(|a| a["titlebarKey"] == id.as_str())
                    .cloned()
                {
                    app.start_account_reset(&account, window, cx);
                }
            });
        });
    }

    fn render_header(&self) -> AnyElement {
        let p = self.palette;
        let provider = if p.codex { "Codex" } else { "Claude" };
        let indicator = match text(&self.account, "indicator") {
            "" => text(&self.account, "selector"),
            value => value,
        };
        let updated = local_date(text(&self.account, "usageUpdatedAt"), false);
        h_flex()
            .w_full()
            .h(px(43.25))
            .flex_shrink_0()
            .min_w_0()
            .items_center()
            .gap(px(11.0))
            .px(px(2.0))
            .pt(px(2.0))
            .pb(px(4.0))
            .child(
                h_flex()
                    .size(px(34.0))
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .border_color(p.accent_line)
                    .rounded(px(10.0))
                    .bg(mix(p.accent, 0.16, rgb(0x141312).into()))
                    .shadow(vec![
                        shadow(0.0, 0.0, 14.0, p.accent.opacity(0.22)).inset(),
                        shadow(0.0, 4.0, 14.0, p.accent.opacity(0.14)),
                    ])
                    .child(img(self.logo.clone()).size(px(20.0))),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(1.0))
                    .child(
                        h_flex()
                            .min_w_0()
                            .items_center()
                            .gap(px(7.0))
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(14.0))
                                    .line_height(px(20.3))
                                    .font_weight(usage_font_weight(650.0))
                                    .child(tracked(format!("{provider} usage"), -0.1)),
                            )
                            .when(!indicator.is_empty() && indicator != "-", |this| {
                                this.child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(p.accent)
                                        .child(tracked(indicator.to_string(), 0.2))
                                        .flex_shrink_0()
                                        .h(px(16.0))
                                        .px(px(5.0))
                                        .font_family(if cfg!(target_os = "macos") {
                                            ".AppleSystemUIFontMonospaced"
                                        } else {
                                            "Consolas"
                                        })
                                        .line_height(px(14.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .rounded(px(4.0))
                                        .border_1()
                                        .border_color(p.accent_line)
                                        .bg(p.soft),
                                )
                            }),
                    )
                    .child(
                        label(
                            match text(&self.account, "displayName") {
                                "" => "Account".to_string(),
                                name => name.to_string(),
                            },
                            11.0,
                            p.muted,
                        )
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis(),
                    ),
            )
            .when(!updated.is_empty(), |this| {
                this.child(
                    label(format!("Updated {updated}"), 10.0, p.dim)
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .self_start()
                        .pt(px(2.0)),
                )
            })
            .into_any_element()
    }

    fn render_footer(&self) -> AnyElement {
        let p = self.palette;
        let links = if p.codex {
            [
                "https://status.openai.com",
                "https://chatgpt.com/codex/settings/usage",
            ]
        } else {
            [
                "https://status.anthropic.com",
                "https://claude.ai/settings/usage",
            ]
        };
        h_flex()
            .w_full()
            .mt_auto()
            .flex_shrink_0()
            .gap(px(8.0))
            .children(
                [
                    ("account-usage-status", "Status"),
                    ("account-usage-dashboard", "Usage dashboard"),
                ]
                .into_iter()
                .zip(links)
                .map(|((id, title), url)| {
                    p.outline_button(id)
                        .tab_index(0)
                        .focus_visible(move |this| this.shadow(focus_ring(p)))
                        .on_click(move |_, _, _| {
                            let _ = gpui_open_external_http_url(url);
                        })
                        .on_key_down(move |event, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                let _ = gpui_open_external_http_url(url);
                            }
                        })
                        .child(title)
                        .child(label("↗", 10.0, p.dim))
                }),
            )
            .into_any_element()
    }
}

impl Render for AccountUsagePanel {
    /// CDXC:AgentProviders 2026-09-12 DECISION:
    /// User wants text in the usage dropdowns to be non-selectable.
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let size = size(
            (viewport.width - px(2.0)).max(px(0.0)),
            (viewport.height - px(2.0)).max(px(0.0)),
        );
        if self.background.as_ref().is_none_or(|(old, _)| *old != size) {
            self.background = Some((
                size,
                self.palette
                    .background(size.width.as_f32(), size.height.as_f32()),
            ));
        }
        let notice = match text(&self.account, "usageError") {
            "" => match text(&self.account, "status") {
                "loading" => "Loading account usage…",
                "ready" => "",
                _ => "Reconnect this account in Settings > Accounts.",
            },
            error => error,
        }
        .to_string();
        let p = self.palette;
        let main = v_flex()
            .w_full()
            .min_h(size.height)
            .p(px(14.0))
            .gap(px(10.0))
            .child(self.render_header())
            .child(self.render_limits(cx))
            .when(!notice.is_empty(), |this| {
                this.child(
                    label(notice, 10.5, rgb(0xf0a94f).into())
                        .py(px(7.0))
                        .px(px(10.0))
                        .rounded(px(8.0))
                        .bg(rgb(0xf0a94f).opacity(0.09))
                        .border_1()
                        .border_color(rgb(0xf0a94f).opacity(0.18)),
                )
            })
            .child(self.render_history())
            .child(self.render_footer());
        let panel = div()
            .id("account-usage-panel")
            .relative()
            .size_full()
            .overflow_hidden()
            .font_family(if cfg!(target_os = "macos") {
                ".AppleSystemUIFont"
            } else {
                "Segoe UI"
            })
            .font_weight(FontWeight::NORMAL)
            .font_smoothing(false)
            .text_size(px(12.0))
            .line_height(px(17.4))
            .text_color(rgb(0xf2f0ee))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "tab" {
                    if event.keystroke.modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    cx.stop_propagation();
                } else {
                    let page = window.viewport_size().height * 0.875;
                    let mut offset = this.scroll.offset();
                    match event.keystroke.key.as_str() {
                        "up" => offset.y += px(40.0),
                        "down" => offset.y -= px(40.0),
                        "pageup" => offset.y += page,
                        "pagedown" | "space" => {
                            offset.y -= if event.keystroke.modifiers.shift {
                                -page
                            } else {
                                page
                            }
                        }
                        "home" => offset.y = px(0.0),
                        "end" => offset.y = -this.scroll.max_offset().y,
                        _ => return,
                    }
                    offset.y = offset.y.clamp(-this.scroll.max_offset().y, px(0.0));
                    this.scroll.set_offset(offset);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_click(cx.listener(|this, _, _, cx| {
                if this.resets_open {
                    this.resets_open = false;
                    cx.notify();
                }
            }))
            .child(
                img(self.background.as_ref().unwrap().1.clone())
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .id("account-usage-scroll")
                    .relative()
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(main),
            )
            .child(Scrollbar::vertical(&self.scroll).scrollbar_show(ScrollbarShow::Scrolling))
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        window.paint_quad(gpui::quad(
                            bounds,
                            px(2.0),
                            gpui::transparent_black(),
                            px(1.0),
                            p.outline,
                            gpui::BorderStyle::Solid,
                        ));
                    },
                )
                .absolute()
                .size_full(),
            );
        div()
            .size_full()
            .overflow_hidden()
            .rounded(px(2.0))
            .border_1()
            .border_color(titlebar_popup_menu_border_color())
            .child(panel)
    }
}
