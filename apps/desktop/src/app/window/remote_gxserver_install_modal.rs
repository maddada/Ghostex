//! Native GPUI Install remote gxserver prompt, the desktop twin of the React
//! `RemoteGxserverInstallModal` in packages/core-ui/remote-gxserver-install-modal.tsx (deleted 2026-10-01).
//!
//! CDXC:RemoteMachines 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt in native GPUI one at a time, and each native modal must be EXACTLY 1 to 1 with its React twin: the same layout, copy, colors, radii, spacing, states, keyboard behaviour and bridge messages in both appearances. The install detail paragraph keeps its inline monospace chips for the install path and the exposed binaries, wrapping like the React text.
//! SEE-ALSO: packages/core-ui/remote-gxserver-install-modal.tsx (deleted 2026-10-01) (the React twin) and the `.remote-gxserver-install-modal-body code` rule in packages/core-ui/styles/modals.css (deleted 2026-11-06), apps/desktop/src/app/remote_gxserver_install_modal_lifecycle.rs (open, close, reconnect), apps/desktop/src/app/remote_conn/project_browse_and_add.rs (the opener that builds the `open` message).
use super::native_modal_kit::*;
use gpui::{
    AnyElement, App, Context, FocusHandle, IntoElement, KeyDownEvent, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

/// `APP_MODAL_HOST_REMOTE_GXSERVER_INSTALL_WINDOW_WIDTH`.
pub(crate) const REMOTE_GXSERVER_INSTALL_MODAL_WIDTH: f32 = 560.0;
/// First-frame height only (`APP_MODAL_HOST_REMOTE_GXSERVER_INSTALL_WINDOW_HEIGHT`); the window is resized to the measured layout on the first prepaint.
pub(crate) const REMOTE_GXSERVER_INSTALL_MODAL_INITIAL_HEIGHT: f32 = 380.0;

const TITLE: &str = "Install remote gxserver";
const CANCEL: &str = "Cancel";
const INSTALL_GXSERVER: &str = "Install gxserver";

/// One run of the card paragraph: plain text (split on spaces so it wraps
/// like browser text) or an inline `<code>` chip.
enum InlineRun {
    Text(&'static str),
    Code(&'static str),
}

/// The `CardDescription` of the React modal, run by run.
const DETAIL_RUNS: [InlineRun; 15] = [
    InlineRun::Text(
        "If you continue, Ghostex will copy its compatible bundled remote package over SSH, start gxserver, then connect through an SSH tunnel. On macOS, Linux, or WSL, the package is installed into ",
    ),
    InlineRun::Code("${XDG_DATA_HOME:-~/.local/share}/ghostex/gxserver"),
    InlineRun::Text(", exposing "),
    InlineRun::Code("gxserver"),
    InlineRun::Text(", "),
    InlineRun::Code("zmx"),
    InlineRun::Text(", "),
    InlineRun::Code("bd"),
    InlineRun::Text(", "),
    InlineRun::Code("ghostex"),
    InlineRun::Text(", and "),
    InlineRun::Code("gx"),
    InlineRun::Text(" from "),
    InlineRun::Code("~/.local/bin"),
    InlineRun::Text(
        " when possible. Windows connections follow Windows Environment on the remote computer, which defaults to native PowerShell. Selecting a WSL distribution in Advanced uses that distribution instead.",
    ),
];

/// The 13px description line box; every inline item is this tall so wrapped rows keep the text rhythm.
const LINE_HEIGHT: f32 = 20.15;
/// The advance of a space in 13px system UI text, used as the gap between inline items.
const SPACE_WIDTH: f32 = 3.7;

/// What the dialog asks its host to do. The dialog removes its own window before sending any of these.
pub(crate) enum RemoteGxserverInstallModalCommand {
    /// "Install gxserver": `reconnectRemoteMachine` with `installApproved: true`.
    Approve,
    /// "Cancel" or Escape: only closes the dialog.
    Cancel,
}

pub(crate) type RemoteGxserverInstallModalHost =
    Rc<dyn Fn(RemoteGxserverInstallModalCommand, &mut App)>;

pub(crate) struct RemoteGxserverInstallModalConfig {
    pub(crate) machine_name: String,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiRemoteGxserverInstallModalWindow {
    host: RemoteGxserverInstallModalHost,
    palette: ModalPalette,
    machine_name: String,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiRemoteGxserverInstallModalWindow {
    pub(crate) fn new(
        config: RemoteGxserverInstallModalConfig,
        host: RemoteGxserverInstallModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette: config.palette,
            machine_name: config.machine_name,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    fn close_window_and_send(
        &mut self,
        command: RemoteGxserverInstallModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() != "escape" {
            return;
        }
        self.close_window_and_send(RemoteGxserverInstallModalCommand::Cancel, window, cx);
        cx.stop_propagation();
    }

    /// `.remote-gxserver-install-modal-body code`: raised background, hairline border, 6px radius, 12px mono, 1px 5px padding.
    fn render_chip(&self, code: &'static str) -> AnyElement {
        let p = self.palette;
        div()
            .flex_shrink_0()
            .px(px(5.0))
            .py(px(1.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised))
            .font_family(MODAL_MONO_FONT)
            .text_size(px(12.0))
            .line_height(px(14.0))
            .text_color(hsla(p.foreground))
            .whitespace_nowrap()
            .child(code)
            .into_any_element()
    }

    /// The card paragraph as a wrapping row of words and chips. A chip and the
    /// punctuation glued to it share one item so they never break apart, the
    /// way the browser keeps `<code>,` together.
    fn render_detail_paragraph(&self) -> AnyElement {
        let p = self.palette;
        let mut items: Vec<AnyElement> = Vec::new();
        let mut pending_chip: Option<AnyElement> = None;
        let word = |text: &str| {
            div()
                .h(px(LINE_HEIGHT))
                .whitespace_nowrap()
                .child(text.to_string())
                .into_any_element()
        };
        for run in DETAIL_RUNS.iter() {
            match run {
                InlineRun::Code(code) => {
                    if let Some(chip) = pending_chip.take() {
                        items.push(chip);
                    }
                    pending_chip = Some(self.render_chip(code));
                }
                InlineRun::Text(text) => {
                    let mut pieces = text.split(' ');
                    // The first piece is glued to the previous chip unless the run starts with a space.
                    if let Some(first) = pieces.next() {
                        match pending_chip.take() {
                            Some(chip) if !first.is_empty() => items.push(
                                h_flex()
                                    .h(px(LINE_HEIGHT))
                                    .items_center()
                                    .child(chip)
                                    .child(word(first))
                                    .into_any_element(),
                            ),
                            Some(chip) => items.push(chip),
                            None if !first.is_empty() => items.push(word(first)),
                            None => {}
                        }
                    }
                    for piece in pieces.filter(|piece| !piece.is_empty()) {
                        items.push(word(piece));
                    }
                }
            }
        }
        if let Some(chip) = pending_chip.take() {
            items.push(chip);
        }
        h_flex()
            .w_full()
            .flex_wrap()
            .items_center()
            .gap_x(px(SPACE_WIDTH))
            .gap_y(px(0.0))
            .text_size(px(13.0))
            .line_height(px(LINE_HEIGHT))
            .text_color(hsla(p.muted))
            .children(items)
            .into_any_element()
    }

    /// `.remote-gxserver-install-modal-body`: one `Card size='sm'` whose header holds the detail paragraph.
    fn render_body(&self) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(
                // `CardHeader` lays its lone description out in a two-row grid, so the empty second row's 8px gap lands under the text.
                modal_panel(&p)
                    .pt(px(16.0))
                    .px(px(16.0))
                    .pb(px(24.0))
                    .child(self.render_detail_paragraph()),
            )
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        modal_footer(vec![
            modal_action_button(
                &p,
                "remote-gxserver-install-cancel",
                CANCEL,
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| {
                    this.close_window_and_send(
                        RemoteGxserverInstallModalCommand::Cancel,
                        window,
                        cx,
                    )
                },
                cx,
            ),
            modal_action_button(
                &p,
                "remote-gxserver-install-approve",
                INSTALL_GXSERVER,
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| {
                    this.close_window_and_send(
                        RemoteGxserverInstallModalCommand::Approve,
                        window,
                        cx,
                    )
                },
                cx,
            ),
        ])
    }
}

impl Render for GpuiRemoteGxserverInstallModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let description = format!(
            "Ghostex can connect to {}, but gxserver is not installed there. Ghostex needs gxserver on that machine to browse folders, add projects, clone repositories, and manage sessions remotely.",
            self.machine_name
        );
        let content = vec![
            modal_header(&p, TITLE, Some(description)),
            self.render_body(),
        ];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-remote-gxserver-install-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiRemoteGxserverInstallModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(RemoteGxserverInstallModalCommand::Cancel, window, cx);
    }
}
