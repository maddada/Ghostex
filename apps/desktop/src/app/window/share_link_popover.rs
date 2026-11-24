//! Frosted Share link popover for Live pages and detected localhost ports.
//!
//! Opened from:
//! - The floating Live page window's `⋯` menu (`window/visual_page_modal/more_menu.rs`).
//! - The chat Live page card (`native_chat/page_card.rs`).
//! - The Dev servers dropdown (`window/remote_sites.rs`).
//!
//! CDXC:Sharing 2026-11-24 DECISION:
//! User chose built-in Share links (OpenTunnel default) with a frosted popover carrying:
//! - Privacy note ("Anyone with the link can see this page and answer its questions" / port access).
//! - Expiry chips (1h / 24h / 7d / Until stopped; default 24h).
//! - Optional 6-digit passcode toggle (off by default).
//! - Copy link, optional Copy passcode, and Stop sharing.

use std::{
    collections::HashSet,
    sync::{LazyLock, Mutex},
    time::Duration,
};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, ClipboardItem, Context, FocusHandle, Focusable,
    FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, div, px, size, svg,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

use super::native_modal_kit::*;
use crate::app::gx_store::gx_rpc;
use crate::app::helpers::*;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

const POPOVER_WIDTH: f32 = 396.0;
const POPOVER_INITIAL_HEIGHT: f32 = 360.0;

const EXPIRY_ITEMS: [ModalSegmentedItem; 4] = [
    ModalSegmentedItem {
        icon: None,
        label: "1h",
    },
    ModalSegmentedItem {
        icon: None,
        label: "24h",
    },
    ModalSegmentedItem {
        icon: None,
        label: "7d",
    },
    ModalSegmentedItem {
        icon: None,
        label: "Until stopped",
    },
];

const EXPIRY_VALUES: [&str; 4] = ["1h", "24h", "7d", "untilStopped"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ShareLinkTarget {
    Page { page_id: String, title: String },
    Port { port: u16, title: String },
}

#[derive(Default)]
struct ActiveSharesCache {
    pages: HashSet<String>,
    ports: HashSet<u16>,
    refreshing: bool,
}

static ACTIVE_SHARES: LazyLock<Mutex<ActiveSharesCache>> =
    LazyLock::new(|| Mutex::new(ActiveSharesCache::new()));

impl ActiveSharesCache {
    fn new() -> Self {
        Self::default()
    }
}

pub(crate) fn update_active_shares_from_state(state: &Value) {
    let Some(shares) = state["shares"].as_array() else {
        return;
    };
    let mut cache = ACTIVE_SHARES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.pages.clear();
    cache.ports.clear();
    for share in shares {
        if share["kind"].as_str() == Some("page") {
            if let Some(page_id) = share["pageId"].as_str() {
                cache.pages.insert(page_id.to_string());
            }
        } else if share["kind"].as_str() == Some("port") {
            if let Some(port) = share["port"].as_u64().and_then(|p| u16::try_from(p).ok()) {
                cache.ports.insert(port);
            }
        }
    }
}

pub(crate) fn is_page_shared(page_id: &str) -> bool {
    ACTIVE_SHARES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .pages
        .contains(page_id)
}

pub(crate) fn is_port_shared(port: u16) -> bool {
    ACTIVE_SHARES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .ports
        .contains(&port)
}

/// Refreshes the in-memory active shares cache from local gxserver in the background.
pub(crate) fn refresh_active_shares_cache(cx: &mut App) {
    {
        let mut cache = ACTIVE_SHARES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cache.refreshing {
            return;
        }
        cache.refreshing = true;
    }
    cx.spawn(async move |_cx| {
        let result = gx_rpc(None, "/api/sharing", json!({ "op": "read" })).await;
        if let Ok(state) = result {
            update_active_shares_from_state(&state);
        }
        let mut cache = ACTIVE_SHARES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.refreshing = false;
    })
    .detach();
}

/// Mirrors `server/src/sharing/blocklist.rs` so UI can mark protected ports clearly before click.
pub(crate) fn is_port_share_blocked(port: u16, process: Option<&str>) -> Option<&'static str> {
    if (58744..=58760).contains(&port) {
        return Some("Ghostex internal port");
    }
    match port {
        22 | 2222 => return Some("SSH port"),
        5432 | 5433 | 3306 | 33060 | 27017 | 27018 | 6379 | 6380 | 11211 | 9200 | 9300 | 9042
        | 1433 | 1521 | 5984 | 8529 | 26257 => return Some("database port"),
        2375 | 2376 | 5900 | 3389 | 445 | 139 | 25 | 465 | 587 | 143 | 993 | 110 | 995 => {
            return Some("system service port");
        }
        _ => {}
    }
    if let Some(proc_name) = process {
        let lower = proc_name.to_ascii_lowercase();
        if [
            "gxserver",
            "sshd",
            "postgres",
            "mysqld",
            "mariadbd",
            "mongod",
            "redis-server",
            "valkey-server",
            "memcached",
            "dockerd",
        ]
        .iter()
        .any(|name| lower.contains(name))
        {
            return Some("protected local service");
        }
    }
    None
}

/// Opens the frosted Share link popover window for a Live page or localhost port.
pub(crate) fn open_share_link_popover(
    target: ShareLinkTarget,
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    cx: &mut App,
) {
    let bounds = WindowBounds::centered(size(px(POPOVER_WIDTH), px(POPOVER_INITIAL_HEIGHT)), cx);
    let glass = window_glass_active();
    let options = WindowOptions {
        window_bounds: Some(bounds),
        titlebar: None,
        focus: true,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: true,
        is_resizable: false,
        window_background: if glass {
            WindowBackgroundAppearance::Blurred
        } else {
            WindowBackgroundAppearance::Opaque
        },
        ..Default::default()
    };
    let _ = cx.open_window(options, move |window, cx| {
        cx.new(|cx| ShareLinkPopoverWindow::new(target, remote, window, cx))
    });
}

fn format_remaining(expires_at_ms: Option<i64>) -> String {
    let Some(at) = expires_at_ms else {
        return "Until stopped".to_string();
    };
    let mins = (at - chrono::Utc::now().timestamp_millis()).max(0) / 60_000;
    match mins {
        0 => "Expires in <1 min".to_string(),
        1..=59 => format!("Expires in {mins} min"),
        60..=1439 => format!("Expires in {} h", (mins + 30) / 60),
        _ => format!("Expires in {} d", (mins + 720) / 1440),
    }
}

pub(crate) struct ShareLinkPopoverWindow {
    target: ShareLinkTarget,
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    state: Option<Value>,
    loading: bool,
    busy: bool,
    error: Option<String>,
    expiry_index: usize,
    passcode_on: bool,
    expiry_initialized: bool,
    copied: Option<&'static str>,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl ShareLinkPopoverWindow {
    fn new(
        target: ShareLinkTarget,
        remote: Option<GpuiRemoteGxserverRequestTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let mut popover = Self {
            target,
            remote,
            state: None,
            loading: true,
            busy: false,
            error: None,
            expiry_index: 1, // 24h default
            passcode_on: false,
            expiry_initialized: false,
            copied: None,
            fit: ModalFit::new(),
            focus_handle,
        };
        popover.reload(cx);
        popover
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let remote = self.remote.clone();
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(remote, "/api/sharing", json!({ "op": "read" })).await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(state) => {
                        if this.remote.is_none() {
                            update_active_shares_from_state(&state);
                        }
                        if !this.expiry_initialized {
                            if let Some(exp) = state["config"]["defaultExpiry"].as_str() {
                                if let Some(idx) = EXPIRY_VALUES.iter().position(|v| *v == exp) {
                                    this.expiry_index = idx;
                                }
                            }
                            this.passcode_on = state["config"]["defaultPasscode"] == true;
                            this.expiry_initialized = true;
                        }
                        this.state = Some(state);
                        this.error = None;
                        this.fit.refit();
                    }
                    Err(error) => {
                        this.error = Some(error.message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn active_share(&self) -> Option<&Value> {
        let shares = self.state.as_ref()?["shares"].as_array()?;
        match &self.target {
            ShareLinkTarget::Page { page_id, .. } => shares.iter().find(|s| {
                s["kind"].as_str() == Some("page") && s["pageId"].as_str() == Some(page_id.as_str())
            }),
            ShareLinkTarget::Port { port, .. } => shares.iter().find(|s| {
                s["kind"].as_str() == Some("port") && s["port"].as_u64() == Some(*port as u64)
            }),
        }
    }

    fn sharing_enabled(&self) -> bool {
        self.state
            .as_ref()
            .and_then(|s| s["config"]["enabled"].as_bool())
            .unwrap_or(true)
    }

    fn enable_sharing(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let remote = self.remote.clone();
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(
                remote,
                "/api/sharing",
                json!({ "op": "setConfig", "patch": { "enabled": true } }),
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(state) => {
                        if this.remote.is_none() {
                            update_active_shares_from_state(&state);
                        }
                        this.state = Some(state);
                        this.fit.refit();
                    }
                    Err(error) => this.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn create_share(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let expiry = EXPIRY_VALUES[self.expiry_index];
        let passcode = self.passcode_on;
        let payload = match &self.target {
            ShareLinkTarget::Page { page_id, .. } => json!({
                "op": "createPage",
                "pageId": page_id,
                "expiry": expiry,
                "requirePasscode": passcode,
            }),
            ShareLinkTarget::Port { port, title } => json!({
                "op": "createPort",
                "port": port,
                "title": title,
                "expiry": expiry,
                "requirePasscode": passcode,
            }),
        };
        let remote = self.remote.clone();
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(remote, "/api/sharing", payload).await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(reply) => {
                        if let Some(state) = reply
                            .get("status")
                            .or_else(|| reply.get("state"))
                            .filter(|v| v.is_object())
                        {
                            if this.remote.is_none() {
                                update_active_shares_from_state(state);
                            }
                            this.state = Some(state.clone());
                        } else {
                            this.reload(cx);
                        }
                        if let Some(url) = reply["share"]["url"].as_str() {
                            cx.write_to_clipboard(ClipboardItem::new_string(url.to_string()));
                            this.flash_copied("link", cx);
                        }
                        this.error = None;
                        this.fit.refit();
                    }
                    Err(error) => {
                        this.error = Some(error.message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn stop_share(&mut self, share_id: String, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let remote = self.remote.clone();
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(
                remote,
                "/api/sharing",
                json!({ "op": "stop", "shareId": share_id }),
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(state) => {
                        if this.remote.is_none() {
                            update_active_shares_from_state(&state);
                        }
                        this.state = Some(state);
                        this.error = None;
                        this.fit.refit();
                    }
                    Err(error) => {
                        this.error = Some(error.message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn flash_copied(&mut self, which: &'static str, cx: &mut Context<Self>) {
        self.copied = Some(which);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(2))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.copied == Some(which) {
                    this.copied = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, _cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
            window.remove_window();
        }
    }

    fn palette(&self) -> ModalPalette {
        let light = chrome_uses_light_appearance();
        let base = ModalPalette::resolve(light, None);
        if window_glass_active() {
            let fill = popup_menu_frame().fill;
            base.frosted_dialog(fill.into())
        } else {
            base
        }
    }
}

impl Focusable for ShareLinkPopoverWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ShareLinkPopoverWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette();
        let surface = popup_menu_frame();
        let title_label = match &self.target {
            ShareLinkTarget::Page { title, .. } => title.clone(),
            ShareLinkTarget::Port { port, title } => format!("{title} (:{port})"),
        };
        let privacy_note = match &self.target {
            ShareLinkTarget::Page { .. } => {
                "Anyone with the link can view this Live page and answer its questions. Outside answers arrive in this chat labeled with their name."
            }
            ShareLinkTarget::Port { port, .. } => {
                let _ = port;
                "Anyone with the link can reach this local port while Ghostex is open on this computer."
            }
        };
        let active = self.active_share().cloned();
        let enabled = self.sharing_enabled();
        let green = if p.light {
            gpui::rgb(0x168049)
        } else {
            gpui::rgb(0x34d399)
        };

        let header = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .size(px(28.0))
                            .rounded(px(8.0))
                            .bg(hsla(p.raised))
                            .border_1()
                            .border_color(hsla(p.hairline))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg()
                                    .path(if active.is_some() {
                                        "titlebar/world.svg"
                                    } else {
                                        "titlebar/link.svg"
                                    })
                                    .size(px(15.0))
                                    .text_color(if active.is_some() {
                                        green.into()
                                    } else {
                                        hsla(p.foreground)
                                    }),
                            ),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap(px(1.0))
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(hsla(p.foreground))
                                            .child(if active.is_some() {
                                                "Shared link"
                                            } else {
                                                "Share link"
                                            }),
                                    )
                                    .when(active.is_some(), |this| {
                                        this.child(
                                            h_flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .px(px(6.0))
                                                .py(px(1.0))
                                                .rounded_full()
                                                .bg(gpui::Hsla::from(green).opacity(0.15))
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(green)
                                                .child(
                                                    div()
                                                        .size(px(6.0))
                                                        .rounded_full()
                                                        .bg(green),
                                                )
                                                .child("Live"),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.0))
                                    .text_color(hsla(p.muted))
                                    .child(title_label),
                            ),
                    ),
            )
            .child(
                div()
                    .id("share-popover-close")
                    .role(gpui::Role::Button)
                    .aria_label("Close")
                    .flex_shrink_0()
                    .size(px(26.0))
                    .rounded(px(7.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |style| style.bg(hsla(p.raised_hover)))
                    .child(
                        svg()
                            .path("titlebar/x.svg")
                            .size(px(14.0))
                            .text_color(hsla(p.muted)),
                    )
                    .on_click(|_, window, _| {
                        window.remove_window();
                    }),
            );

        let mut body = v_flex().w_full().gap(px(12.0));

        if let Some(error) = &self.error {
            body = body.child(
                div()
                    .w_full()
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(hsla(p.destructive).opacity(0.35))
                    .bg(hsla(p.destructive).opacity(0.10))
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .text_color(hsla(p.destructive))
                    .child(error.clone()),
            );
        }

        if self.loading {
            body = body.child(
                div()
                    .py(px(14.0))
                    .text_size(px(12.5))
                    .text_color(hsla(p.muted))
                    .child("Checking sharing status…"),
            );
        } else if !enabled && active.is_none() {
            body = body
                .child(
                    div()
                        .w_full()
                        .px(px(12.0))
                        .py(px(10.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .bg(hsla(p.panel))
                        .text_size(px(12.5))
                        .line_height(px(18.0))
                        .text_color(hsla(p.muted))
                        .child("Sharing is turned off in Settings > Sharing. Turn it on to create public tunnel links from this computer."),
                )
                .child(modal_action_button(
                    &p,
                    "share-popover-enable",
                    if self.busy {
                        "Turning on…"
                    } else {
                        "Turn on Sharing"
                    },
                    None,
                    ModalButtonTone::Primary,
                    self.busy,
                    |this, _window, cx| this.enable_sharing(cx),
                    cx,
                ));
        } else if let Some(share) = active {
            let share_id = share["shareId"]
                .as_str()
                .or_else(|| share["id"].as_str())
                .unwrap_or_default()
                .to_string();
            let url = share["url"].as_str().unwrap_or_default().to_string();
            let passcode = share["passcode"].as_str().map(str::to_string);
            let remaining = format_remaining(share["expiresAtMs"].as_i64());
            let copy_url = url.clone();
            let link_copied = self.copied == Some("link");

            body = body
                .child(
                    div()
                        .w_full()
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .bg(hsla(p.panel))
                        .text_size(px(12.0))
                        .line_height(px(17.0))
                        .text_color(hsla(p.muted))
                        .child(privacy_note),
                )
                .child(
                    v_flex()
                        .w_full()
                        .gap(px(6.0))
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .child(modal_section_title(&p, "Public URL"))
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(hsla(p.muted))
                                        .child(remaining),
                                ),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .gap(px(8.0))
                                .items_center()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .h(px(32.0))
                                        .px(px(10.0))
                                        .rounded(px(8.0))
                                        .border_1()
                                        .border_color(hsla(p.hairline))
                                        .bg(hsla(p.raised))
                                        .flex()
                                        .items_center()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .font_family(MODAL_MONO_FONT)
                                                .text_size(px(12.0))
                                                .text_color(hsla(p.foreground))
                                                .child(url),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("share-popover-copy-link")
                                        .role(gpui::Role::Button)
                                        .aria_label("Copy link")
                                        .flex_shrink_0()
                                        .h(px(32.0))
                                        .px(px(12.0))
                                        .rounded(px(8.0))
                                        .bg(hsla(p.primary_button_fills().rest))
                                        .text_color(hsla(p.primary_button_text()))
                                        .text_size(px(12.5))
                                        .font_weight(FontWeight::MEDIUM)
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .cursor_pointer()
                                        .child(
                                            svg()
                                                .path(if link_copied {
                                                    "titlebar/check.svg"
                                                } else {
                                                    "titlebar/copy.svg"
                                                })
                                                .size(px(13.0))
                                                .text_color(hsla(p.primary_button_text())),
                                        )
                                        .child(if link_copied { "Copied" } else { "Copy link" })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                copy_url.clone(),
                                            ));
                                            this.flash_copied("link", cx);
                                        })),
                                ),
                        ),
                );

            if let Some(code) = passcode {
                let copy_code = code.clone();
                let code_copied = self.copied == Some("code");
                body = body.child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .px(px(12.0))
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .bg(hsla(p.panel))
                        .child(
                            h_flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    svg()
                                        .path("titlebar/lock.svg")
                                        .size(px(14.0))
                                        .text_color(hsla(p.muted)),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .text_color(hsla(p.muted))
                                        .child("Passcode"),
                                )
                                .child(
                                    div()
                                        .font_family(MODAL_MONO_FONT)
                                        .text_size(px(14.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(hsla(p.foreground))
                                        .child(code),
                                ),
                        )
                        .child(
                            div()
                                .id("share-popover-copy-code")
                                .role(gpui::Role::Button)
                                .aria_label("Copy passcode")
                                .px(px(9.0))
                                .py(px(4.0))
                                .rounded(px(6.0))
                                .bg(hsla(p.raised))
                                .border_1()
                                .border_color(hsla(p.hairline))
                                .text_size(px(12.0))
                                .text_color(hsla(p.foreground))
                                .cursor_pointer()
                                .hover(move |style| style.bg(hsla(p.raised_hover)))
                                .child(if code_copied { "Copied" } else { "Copy" })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_code.clone(),
                                    ));
                                    this.flash_copied("code", cx);
                                })),
                        ),
                );
            }

            let stop_id = share_id;
            body = body.child(
                div().pt(px(2.0)).child(modal_action_button(
                    &p,
                    "share-popover-stop",
                    if self.busy {
                        "Stopping…"
                    } else {
                        "Stop sharing"
                    },
                    None,
                    ModalButtonTone::Danger,
                    self.busy,
                    move |this, _window, cx| this.stop_share(stop_id.clone(), cx),
                    cx,
                )),
            );
        } else {
            let passcode_on = self.passcode_on;
            let busy = self.busy;
            body = body
                .child(
                    div()
                        .w_full()
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .bg(hsla(p.panel))
                        .text_size(px(12.0))
                        .line_height(px(17.0))
                        .text_color(hsla(p.muted))
                        .child(privacy_note),
                )
                .child(
                    v_flex()
                        .w_full()
                        .gap(px(6.0))
                        .child(modal_section_title(&p, "Link expires"))
                        .child(modal_segmented_control(
                            &p,
                            "share-popover-expiry",
                            &EXPIRY_ITEMS,
                            self.expiry_index,
                            |this, idx, _window, cx| {
                                this.expiry_index = idx;
                                cx.notify();
                            },
                            cx,
                        )),
                )
                .child(
                    modal_panel(&p).child(
                        modal_panel_row(
                            &p,
                            "Require passcode",
                            Some("Ask visitors for a 6-digit code before opening."),
                            modal_switch(&p, passcode_on, busy),
                            false,
                        )
                        .id("share-popover-passcode-row")
                        .switch_row_press(
                            p.raised_hover,
                            busy,
                            RowToggle {
                                press: bind_press(cx, |this, _window, cx| {
                                    this.passcode_on = !this.passcode_on;
                                    cx.notify();
                                }),
                                switch: Some(RowSwitchA11y {
                                    label: SharedString::from("Require passcode"),
                                    checked: passcode_on,
                                    accessibility_id: Some(SharedString::from(
                                        "share-popover-passcode",
                                    )),
                                }),
                            },
                        ),
                    ),
                )
                .child(modal_action_button(
                    &p,
                    "share-popover-create",
                    if self.busy {
                        "Creating link…"
                    } else {
                        "Create share link"
                    },
                    self.busy.then(|| modal_spinner(p.primary_button_text())),
                    ModalButtonTone::Primary,
                    self.busy,
                    |this, _window, cx| this.create_share(cx),
                    cx,
                ));
        }

        let card = v_flex()
            .id("ghostex-share-link-popover")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .font_family(MODAL_UI_FONT)
            .w_full()
            .p(px(16.0))
            .gap(px(14.0))
            .rounded(px(surface.radius))
            .border_1()
            .border_color(surface.border)
            .bg(surface.fill)
            .text_color(hsla(p.foreground))
            .child(header)
            .child(body);

        let fit_listener = self.fit.listener(0.0);
        div()
            .size_full()
            .child(
                div()
                    .w(px(POPOVER_WIDTH))
                    .on_children_prepainted(move |bounds, window, cx| {
                        fit_listener(bounds, window, cx);
                    })
                    .child(card),
            )
    }
}
