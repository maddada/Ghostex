//! Native GPUI Remote Setup (Mobile & Remote) dialog, the desktop twin of the
//! React `RemoteSetupModal` in packages/core-ui/remote-setup-modal.tsx.
//!
//! CDXC:RemotePairing 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt in native GPUI one at a time, and the new gpui modal must be EXACTLY 1 to 1 matching the React one: layout, copy, colors, radii, spacing, fonts, states and keys in both appearances. The two numbered sections, the in-panel Android install popover with its QR code, the Easy Connect and Tailscale option cards, and the Connect flow that hands off to Settings > Remote are all here; the gxserver calls stay on the app side and come back through `connect_finished`.
//! SEE-ALSO: packages/core-ui/remote-setup-modal.tsx and packages/core-ui/remote-setup-modal/ (the React twin) with the `.remote-setup-*` rules in packages/core-ui/styles/modals.css, apps/desktop/src/app/window/native_modal_kit.rs (shared chrome, the scrolling shell), apps/desktop/src/app/remote_setup_modal_lifecycle.rs (open, close, gxserver RPCs, Settings handoff), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, ClickEvent, Context, Div, FocusHandle,
    FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    Rgba, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, Transformation,
    Window, div, px, radians, rgb,
};
use gpui_component::tooltip::Tooltip;
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;
use std::time::Duration;

/// `APP_MODAL_HOST_REMOTE_SETUP_WINDOW_WIDTH` / `_HEIGHT`: the child window the React dialog opened in.
pub(crate) const REMOTE_SETUP_MODAL_WIDTH: f32 = 560.0;
/// First-frame height only. The window is resized to the measured layout as soon as the first prepaint reports it.
pub(crate) const REMOTE_SETUP_MODAL_INITIAL_HEIGHT: f32 = 760.0;

const ICON_BRAND_ANDROID: &str = "modals/remote-setup/brand-android.svg";
const ICON_BRAND_APPLE: &str = "modals/remote-setup/brand-apple.svg";
const ICON_CHEVRON_DOWN: &str = "modals/remote-setup/chevron-down.svg";
const ICON_EXTERNAL_LINK: &str = "modals/remote-setup/external-link.svg";
const ICON_COPY: &str = "modals/remote-setup/copy.svg";
const ICON_CHECK: &str = "modals/remote-setup/check.svg";
const ICON_QRCODE_CARD: &str = "modals/remote-setup/qrcode-card.svg";
const ICON_QRCODE_BUTTON: &str = "modals/remote-setup/qrcode-button.svg";
const ICON_SHIELD: &str = "modals/remote-setup/shield.svg";
const ICON_LOADER: &str = "modals/remote-setup/loader-2.svg";
const ICON_X: &str = "modals/remote-setup/x.svg";

const EYEBROW: &str = "MOBILE & REMOTE";
const TITLE: &str = "Remote Setup";
const INTRO: &str = "Use Ghostex from your phone, or from any other computer, from anywhere. Two steps: get the app, then connect it to this computer.";
const GET_APP_TITLE: &str = "Get the Ghostex app";
const ANDROID: &str = "Android";
const ANDROID_DETAIL: &str = "Install the APK from the latest GitHub release.";
const HOW_TO_INSTALL: &str = "How to install";
const IPHONE: &str = "iPhone";
const IPHONE_DETAIL: &str =
    "TestFlight only for now. Join the Discord and ask for TestFlight access.";
const JOIN_DISCORD: &str = "Join Discord";
const ANDROID_POPOVER_TITLE: &str = "Install on your Android phone";
const ANDROID_SCAN_HINT: &str =
    "Scan the code or open the link on the phone. It goes to the latest release on GitHub.";
const ANDROID_INSTALL_STEPS: [&str; 3] = [
    "On the phone, open the link and download ghostex-android.apk.",
    "Open the downloaded file. If Android asks, allow your browser to install unknown apps.",
    "Tap Install, then open Ghostex and continue with step 2 below.",
];
const ANDROID_UPDATES_HINT: &str = "Updates: the app checks GitHub for newer releases. Settings → Updates downloads the new APK and opens the installer.";
const CONNECT_TITLE: &str = "Connect it to this computer";
const EASY_CONNECT_TITLE: &str = "Easy Connect (QR/Token)";
const RECOMMENDED: &str = "Recommended";
const EASY_CONNECT_SUB: &str =
    "Built into Ghostex. No VPN, no accounts, nothing to install on the computer.";
const EASY_CONNECT_STEPS: [&str; 2] = [
    "Click Connect. Easy Connect turns on and a pairing code appears. If SSH access is off, your computer asks for an admin password once to enable it.",
    "Scan it with the Ghostex app. That's it; the pairing stays until you remove it.",
];
const ABOUT_A_MINUTE: &str = "About a minute";
const CONNECT: &str = "Connect";
const CONNECTING: &str = "Connecting…";
const NO_SERVER_MESSAGE: &str = "The Ghostex server connection is unavailable.";
const TAILSCALE: &str = "Tailscale";
const TAILSCALE_BADGE: &str = "If you already use it";
const TAILSCALE_SUB: &str = "Your device joins your tailnet and connects over SSH. Best when Tailscale is already on both devices.";
const TAILSCALE_FOOT: &str = "You'll type the host, user and password on the device";
const SHOW_INSTRUCTIONS: &str = "Show instructions";

/// The short website link the popover shows and encodes; the site redirects it to the latest APK.
pub(crate) const GHOSTEX_ANDROID_INSTALL_URL: &str = "https://ghostex.dev/android";
const GHOSTEX_ANDROID_INSTALL_URL_LABEL: &str = "ghostex.dev/android";
pub(crate) const REMOTE_SETUP_DISCORD_URL: &str = "https://discord.gg/df7b3G92CS";

/// `.remote-setup-android-qr`: 112px, `#111113` modules on `#f4f4f5`, error correction M, a 2-module quiet zone.
const QR_SIZE: f32 = 112.0;
const QR_MARGIN_MODULES: usize = 2;
const QR_DARK: u32 = 0x111113;
const QR_LIGHT: u32 = 0xf4f4f5;
/// The shadcn `xs`/`sm` buttons inside `.gx-app-modal` take the modal's 32px control height and 14px text.
const BUTTON_HEIGHT: f32 = MODAL_CONTROL_HEIGHT;
const COPIED_RESET_MS: u64 = 1_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RemoteSettingsSection {
    EasyConnect,
    Tailscale,
}

impl RemoteSettingsSection {
    /// The `initialRemoteSection` value of the Settings open message.
    pub(crate) fn open_message_value(self) -> &'static str {
        match self {
            Self::EasyConnect => "easyConnect",
            Self::Tailscale => "tailscale",
        }
    }
}

/// What the dialog asks its host to do. The dialog removes its own window
/// before `Close` and `OpenRemoteSettings`; the other commands leave it open.
pub(crate) enum RemoteSetupModalCommand {
    OpenExternalUrl(String),
    /// The Android link was written to the clipboard; the host plays the copy sound.
    AndroidLinkCopied,
    /// Run the Easy Connect enablement; the host answers with `connect_finished`.
    Connect,
    OpenRemoteSettings(RemoteSettingsSection),
    Close,
}

pub(crate) type RemoteSetupModalHost = Rc<dyn Fn(RemoteSetupModalCommand, &mut App)>;

pub(crate) struct RemoteSetupModalConfig {
    pub(crate) palette: ModalPalette,
    /// `Settings.remoteTailscaleEnabled`; off hides the Tailscale card.
    pub(crate) tailscale_enabled: bool,
    /// Whether the local gxserver can be reached; off disables Connect with the reason as its tooltip.
    pub(crate) gxserver_available: bool,
    /// Opens with the Android popover expanded (preview only).
    pub(crate) android_open: bool,
}

/// The QR modules, row-major, `true` for a dark module.
struct QrGrid {
    width: usize,
    dark: Vec<bool>,
}

fn qr_grid(text: &str) -> Option<QrGrid> {
    let code =
        qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M).ok()?;
    let width = code.width();
    let dark = code
        .to_colors()
        .into_iter()
        .map(|color| color == qrcode::Color::Dark)
        .collect();
    Some(QrGrid { width, dark })
}

pub(crate) struct GpuiRemoteSetupModalWindow {
    host: RemoteSetupModalHost,
    palette: ModalPalette,
    tailscale_enabled: bool,
    gxserver_available: bool,
    android_open: bool,
    copied: bool,
    copied_generation: u64,
    connecting: bool,
    connect_error: Option<String>,
    qr: Option<QrGrid>,
    fit: ModalScrollFit,
    focus_handle: FocusHandle,
}

impl GpuiRemoteSetupModalWindow {
    pub(crate) fn new(
        config: RemoteSetupModalConfig,
        host: RemoteSetupModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette: config.palette,
            tailscale_enabled: config.tailscale_enabled,
            gxserver_available: config.gxserver_available,
            android_open: config.android_open,
            copied: false,
            copied_generation: 0,
            connecting: false,
            connect_error: None,
            qr: qr_grid(GHOSTEX_ANDROID_INSTALL_URL),
            fit: ModalScrollFit::new(),
            focus_handle,
        }
    }

    /// The host's answer to `Connect`. Success closes the dialog and hands off
    /// to Settings > Remote on the Easy Connect card; a failure stays here with
    /// the message under the steps.
    pub(crate) fn connect_finished(
        &mut self,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.connecting = false;
        match result {
            Ok(()) => {
                self.close_window_and_send(
                    RemoteSetupModalCommand::OpenRemoteSettings(RemoteSettingsSection::EasyConnect),
                    window,
                    cx,
                );
            }
            Err(message) => {
                self.connect_error = Some(message);
                cx.notify();
            }
        }
    }

    fn close_window_and_send(
        &mut self,
        command: RemoteSetupModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(RemoteSetupModalCommand::Close, window, cx);
    }

    /// Preview hook for the standalone demo binary: expand the Android popover after open.
    #[allow(dead_code)] // used by src/bin/native_modal_demo/remote_setup.rs only
    pub(crate) fn preview_toggle_android(&mut self, cx: &mut Context<Self>) {
        self.toggle_android(cx);
    }

    fn toggle_android(&mut self, cx: &mut Context<Self>) {
        self.android_open = !self.android_open;
        cx.notify();
    }

    fn open_discord(&mut self, cx: &mut Context<Self>) {
        (self.host)(
            RemoteSetupModalCommand::OpenExternalUrl(REMOTE_SETUP_DISCORD_URL.to_string()),
            cx,
        );
    }

    fn copy_android_link(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            GHOSTEX_ANDROID_INSTALL_URL.to_string(),
        ));
        (self.host)(RemoteSetupModalCommand::AndroidLinkCopied, cx);
        self.copied = true;
        self.copied_generation = self.copied_generation.wrapping_add(1);
        let generation = self.copied_generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(COPIED_RESET_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.copied_generation == generation {
                    this.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        if !self.gxserver_available || self.connecting {
            return;
        }
        self.connecting = true;
        self.connect_error = None;
        cx.notify();
        (self.host)(RemoteSetupModalCommand::Connect, cx);
    }

    fn show_tailscale_instructions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(
            RemoteSetupModalCommand::OpenRemoteSettings(RemoteSettingsSection::Tailscale),
            window,
            cx,
        );
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() == "escape" {
            self.close(window, cx);
            cx.stop_propagation();
        }
    }

    /// shadcn `--border`: white at 10% in dark, black at 14% in light (modals-light.css).
    fn shadcn_border(&self) -> Rgba {
        if self.palette.light {
            self.palette.hairline
        } else {
            modal_rgba(0xffffff, 0.10)
        }
    }

    /// shadcn `--muted`: `oklch(0.269 0 0)` = #262626 in dark, #f1f1f1 in light; the button hover fill.
    fn shadcn_muted(&self) -> Rgba {
        if self.palette.light {
            rgb(0xf1f1f1)
        } else {
            self.palette.accent
        }
    }

    /// shadcn `--secondary`: `oklch(0.274 0.006 286.033)` = #27272a in dark, #f1f1f1 in light; the close button fill.
    fn shadcn_secondary(&self) -> Rgba {
        if self.palette.light {
            rgb(0xf1f1f1)
        } else {
            rgb(0x27272a)
        }
    }

    /// `.remote-setup-section-number` (20px) and `.remote-setup-step-number` (18px): raised pill, 11px/500.
    fn render_number_pill(&self, number: usize, size: f32) -> AnyElement {
        let p = self.palette;
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .size(px(size))
            .rounded_full()
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised))
            .text_size(px(11.0))
            .line_height(px(15.71))
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(p.foreground))
            .child(SharedString::from(number.to_string()))
            .into_any_element()
    }

    fn render_section_head(&self, number: usize, title: &'static str) -> AnyElement {
        let p = self.palette;
        h_flex()
            .w_full()
            .items_center()
            .gap(px(8.0))
            .child(self.render_number_pill(number, 20.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(16.9))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.foreground))
                    .child(title),
            )
            .into_any_element()
    }

    /// shadcn `variant='outline'` inside the modal: surface fill, `--border`
    /// edge, 8px radius, 32px tall, 14px text, muted fill on hover.
    fn outline_button(
        &self,
        id: &'static str,
        padding_x: f32,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let p = self.palette;
        let hover = self.shadcn_muted();
        h_flex()
            .id(id)
            .flex_shrink_0()
            .h(px(BUTTON_HEIGHT))
            .px(px(padding_x))
            .gap(px(4.0))
            .items_center()
            .justify_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(self.shadcn_border()))
            .bg(if p.glass {
                transparent()
            } else {
                hsla(p.background)
            })
            .text_size(px(14.0))
            .line_height(px(20.0))
            .font_weight(FontWeight::NORMAL)
            .text_color(hsla(p.foreground))
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                on_click(this, window, cx);
            }))
    }

    fn render_row(
        &self,
        icon: &'static str,
        label: &'static str,
        detail: &'static str,
        divider: bool,
        button: AnyElement,
    ) -> AnyElement {
        let p = self.palette;
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .px(px(12.0))
            .py(px(10.0))
            .when(divider, |this| {
                this.border_t_1().border_color(hsla(p.hairline))
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(13.0))
                            .line_height(px(18.57))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child(modal_icon(icon, 16.0, p.muted))
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(17.4))
                            .text_color(hsla(p.muted))
                            .child(detail),
                    ),
            )
            .child(button)
            .into_any_element()
    }

    /// The QR modules painted as runs of squares on the light quiet zone,
    /// snapped to the same pixel boundaries the `qrcode` canvas renderer uses
    /// (`floor(pixel / scale)` picks the module).
    fn render_qr(&self) -> AnyElement {
        let mut frame = div()
            .relative()
            .flex_shrink_0()
            .size(px(QR_SIZE))
            .rounded(px(6.0))
            .overflow_hidden()
            .bg(hsla(rgb(QR_LIGHT)));
        if let Some(grid) = &self.qr {
            let total = grid.width + QR_MARGIN_MODULES * 2;
            let scale = QR_SIZE / total as f32;
            let edge = |index: usize| (index as f32 * scale).ceil();
            for row in 0..grid.width {
                let top = edge(row + QR_MARGIN_MODULES);
                let bottom = edge(row + QR_MARGIN_MODULES + 1);
                let mut column = 0;
                while column < grid.width {
                    if !grid.dark[row * grid.width + column] {
                        column += 1;
                        continue;
                    }
                    let start = column;
                    while column < grid.width && grid.dark[row * grid.width + column] {
                        column += 1;
                    }
                    let left = edge(start + QR_MARGIN_MODULES);
                    let right = edge(column + QR_MARGIN_MODULES);
                    frame = frame.child(
                        div()
                            .absolute()
                            .left(px(left))
                            .top(px(top))
                            .w(px(right - left))
                            .h(px(bottom - top))
                            .bg(hsla(rgb(QR_DARK))),
                    );
                }
            }
        }
        frame.into_any_element()
    }

    fn render_step(&self, number: usize, text: &'static str) -> AnyElement {
        let p = self.palette;
        h_flex()
            .w_full()
            .items_start()
            .gap(px(8.0))
            .child(self.render_number_pill(number, 18.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.5))
                    .line_height(px(18.125))
                    .text_color(hsla(p.foreground))
                    .child(text),
            )
            .into_any_element()
    }

    fn render_muted(&self, text: &'static str) -> Div {
        div()
            .text_size(px(12.0))
            .line_height(px(17.4))
            .text_color(hsla(self.palette.muted))
            .child(text)
    }

    fn render_android_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let copied = self.copied;
        let hover = self.shadcn_muted();
        let copy_button = div()
            .id("remote-setup-copy-android-link")
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .w(px(24.0))
            .h(px(BUTTON_HEIGHT))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(hover)))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.copy_android_link(cx);
            }))
            .child(modal_icon(
                if copied { ICON_CHECK } else { ICON_COPY },
                12.0,
                p.foreground,
            ));
        v_flex()
            .w_full()
            .gap(px(10.0))
            .p(px(12.0))
            .border_t_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised))
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .gap(px(14.0))
                    .child(self.render_qr())
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(18.57))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(hsla(p.foreground))
                                    .child(ANDROID_POPOVER_TITLE),
                            )
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .items_center()
                                    .gap(px(6.0))
                                    .pt(px(4.0))
                                    .pr(px(6.0))
                                    .pb(px(4.0))
                                    .pl(px(10.0))
                                    .rounded(px(MODAL_RADIUS_CONTROL))
                                    .border_1()
                                    .border_color(hsla(p.hairline))
                                    .bg(hsla(p.panel))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .font_family(MODAL_MONO_FONT)
                                            .text_size(px(12.0))
                                            .line_height(px(17.14))
                                            .text_color(hsla(p.foreground))
                                            .child(GHOSTEX_ANDROID_INSTALL_URL_LABEL),
                                    )
                                    .child(copy_button),
                            )
                            .child(self.render_muted(ANDROID_SCAN_HINT)),
                    ),
            )
            .child(
                v_flex().w_full().gap(px(6.0)).children(
                    ANDROID_INSTALL_STEPS
                        .iter()
                        .enumerate()
                        .map(|(index, step)| self.render_step(index + 1, step)),
                ),
            )
            .child(self.render_muted(ANDROID_UPDATES_HINT))
            .into_any_element()
    }

    fn render_get_app_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let android_open = self.android_open;
        let install_button = self
            .outline_button(
                "remote-setup-android-install",
                10.0,
                |this, _window, cx| this.toggle_android(cx),
                cx,
            )
            .child(HOW_TO_INSTALL)
            .child(
                modal_icon(ICON_CHEVRON_DOWN, 12.0, p.foreground).with_transformation(
                    Transformation::rotate(radians(if android_open {
                        std::f32::consts::PI
                    } else {
                        0.0
                    })),
                ),
            )
            .into_any_element();
        let discord_button = self
            .outline_button(
                "remote-setup-join-discord",
                10.0,
                |this, _window, cx| this.open_discord(cx),
                cx,
            )
            .child(JOIN_DISCORD)
            .child(modal_icon(ICON_EXTERNAL_LINK, 12.0, p.foreground))
            .into_any_element();
        v_flex()
            .w_full()
            .gap(px(10.0))
            .child(self.render_section_head(1, GET_APP_TITLE))
            .child(
                modal_panel(&p)
                    .child(self.render_row(
                        ICON_BRAND_ANDROID,
                        ANDROID,
                        ANDROID_DETAIL,
                        false,
                        install_button,
                    ))
                    .when(android_open, |this| {
                        this.child(self.render_android_popover(cx))
                    })
                    .child(self.render_row(
                        ICON_BRAND_APPLE,
                        IPHONE,
                        IPHONE_DETAIL,
                        true,
                        discord_button,
                    )),
            )
            .into_any_element()
    }

    /// `.remote-setup-tag` (primary at 18% under primary text) and `.remote-setup-badge` (raised pill, muted).
    fn render_pill(&self, text: &'static str, recommended: bool) -> AnyElement {
        let p = self.palette;
        div()
            .flex_shrink_0()
            .px(px(7.0))
            .py(px(3.0))
            .rounded_full()
            .text_size(px(10.5))
            .line_height(px(10.5))
            .font_weight(FontWeight::MEDIUM)
            .when(recommended, |this| {
                this.bg(hsla(rgba_of(p.primary, 0.18)))
                    .text_color(hsla(p.primary))
            })
            .when(!recommended, |this| {
                this.border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.raised))
                    .text_color(hsla(p.muted))
            })
            .child(text)
            .into_any_element()
    }

    fn render_option_head(
        &self,
        icon: &'static str,
        title: &'static str,
        pill: AnyElement,
        subtitle: &'static str,
    ) -> AnyElement {
        let p = self.palette;
        h_flex()
            .w_full()
            .items_start()
            .gap(px(10.0))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(32.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.raised))
                    .child(modal_icon(icon, 18.0, p.foreground)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        h_flex()
                            .w_full()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(13.0))
                            .line_height(px(18.57))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child(title)
                            .child(pill),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(17.4))
                            .text_color(hsla(p.muted))
                            .child(subtitle),
                    ),
            )
            .into_any_element()
    }

    fn option_card(&self, recommended: bool) -> Div {
        let p = self.palette;
        v_flex()
            .w_full()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            // `.remote-setup-option-card[data-recommended='true']` is the literal rgba(255,255,255,0.14) in both appearances.
            .border_color(hsla(if recommended {
                modal_rgba(0xffffff, 0.14)
            } else {
                p.hairline
            }))
            .bg(hsla(p.panel))
    }

    fn render_spinner(&self, color: Rgba) -> AnyElement {
        modal_icon(ICON_LOADER, 16.0, color)
            .with_animation(
                "remote-setup-spinner",
                Animation::new(Duration::from_millis(900)).repeat(),
                |svg, delta| {
                    svg.with_transformation(Transformation::rotate(radians(
                        delta * std::f32::consts::TAU,
                    )))
                },
            )
            .into_any_element()
    }

    /// shadcn default (`bg-primary`) `size='sm'` Connect button: 32px, 12px padding, 4px gap, 80% primary on hover, half opacity when disabled.
    fn render_connect_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let disabled = !self.gxserver_available || self.connecting;
        let connecting = self.connecting;
        let hover = rgba_of(p.primary, p.primary.a * 0.8);
        h_flex()
            .id("remote-setup-connect")
            .flex_shrink_0()
            .h(px(BUTTON_HEIGHT))
            .px(px(12.0))
            .gap(px(4.0))
            .items_center()
            .justify_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .bg(hsla(p.primary))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.primary_foreground))
            .whitespace_nowrap()
            .when(disabled, |this| this.opacity(0.5).cursor_default())
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(hsla(hover)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.connect(cx);
                    }))
            })
            .when(!self.gxserver_available, |this| {
                this.tooltip(|window, cx| Tooltip::new(NO_SERVER_MESSAGE).build(window, cx))
            })
            .child(if connecting {
                self.render_spinner(p.primary_foreground)
            } else {
                modal_icon(ICON_QRCODE_BUTTON, 16.0, p.primary_foreground).into_any_element()
            })
            .child(if connecting { CONNECTING } else { CONNECT })
            .into_any_element()
    }

    fn render_easy_connect_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        self.option_card(true)
            .child(self.render_option_head(
                ICON_QRCODE_CARD,
                EASY_CONNECT_TITLE,
                self.render_pill(RECOMMENDED, true),
                EASY_CONNECT_SUB,
            ))
            .child(
                v_flex().w_full().gap(px(6.0)).children(
                    EASY_CONNECT_STEPS
                        .iter()
                        .enumerate()
                        .map(|(index, step)| self.render_step(index + 1, step)),
                ),
            )
            .children(self.connect_error.clone().map(|message| {
                div()
                    .text_size(px(12.0))
                    .line_height(px(17.4))
                    .text_color(hsla(p.destructive))
                    .child(message)
            }))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(self.render_muted(ABOUT_A_MINUTE))
                    .child(self.render_connect_button(cx)),
            )
            .into_any_element()
    }

    fn render_tailscale_card(&self, cx: &mut Context<Self>) -> AnyElement {
        self.option_card(false)
            .child(self.render_option_head(
                ICON_SHIELD,
                TAILSCALE,
                self.render_pill(TAILSCALE_BADGE, false),
                TAILSCALE_SUB,
            ))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(self.render_muted(TAILSCALE_FOOT))
                    .child(
                        self.outline_button(
                            "remote-setup-tailscale-instructions",
                            12.0,
                            |this, window, cx| this.show_tailscale_instructions(window, cx),
                            cx,
                        )
                        .child(SHOW_INSTRUCTIONS),
                    ),
            )
            .into_any_element()
    }

    fn render_connect_section(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .w_full()
            .gap(px(10.0))
            .child(self.render_section_head(2, CONNECT_TITLE))
            .child(self.render_easy_connect_card(cx))
            .when(self.tailscale_enabled, |this| {
                this.child(self.render_tailscale_card(cx))
            })
            .into_any_element()
    }

    fn render_header(&self) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .line_height(px(15.71))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.muted))
                    .child(EYEBROW),
            )
            .child(div().text_size(px(16.0)).line_height(px(20.8)).child(TITLE))
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(20.15))
                    .text_color(hsla(p.muted))
                    .child(INTRO),
            )
            .into_any_element()
    }

    /// The dialog's `showCloseButton`: a 28px ghost icon button on `--secondary`
    /// 16px from the top-right corner, its fill clipped inside a 1px transparent border.
    fn render_close_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let hover = self.shadcn_muted();
        div()
            .id("remote-setup-close")
            .absolute()
            .top(px(17.0))
            .right(px(17.0))
            .flex()
            .items_center()
            .justify_center()
            .size(px(26.0))
            .rounded(px(9.0))
            .bg(hsla(self.shadcn_secondary()))
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(hover)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.close(window, cx);
            }))
            .child(modal_icon(ICON_X, 16.0, p.foreground))
            .into_any_element()
    }
}

impl Render for GpuiRemoteSetupModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let body = v_flex()
            .w_full()
            .gap(px(18.0))
            .child(self.render_get_app_section(cx))
            .child(self.render_connect_section(cx))
            .into_any_element();
        modal_shell_scrolling(
            &p,
            "ghostex-gpui-remote-setup-modal",
            "ghostex-gpui-remote-setup-modal-body",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            self.render_header(),
            body,
            None,
            None,
            cx,
        )
        .child(self.render_close_button(cx))
    }
}
