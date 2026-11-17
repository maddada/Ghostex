//! The "Ghostex <new> isn't working on this computer" message: shown at launch, before any window
//! opens, after a new version crashed twice in its first day (go_back/window.rs opens it and runs
//! its buttons). The design the user approved: docs/2026-11-17/release-safety-simple/index.html.
//!
//! CDXC:Release 2026-11-17 DECISION:
//! User: "see a message on desktop if update fails so u can revert to older version easily".
//! Plain short words; Try again and Go back to <previous>, with "Also send the crash report"
//! checked. A deb or rpm install can't be swapped by Ghostex, so it shows the one command that
//! installs the older package; a computer with no kept copy offers the GitHub download.
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClipboardItem, Context, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

pub(crate) const GO_BACK_MODAL_WIDTH: f32 = 460.0;
pub(crate) const GO_BACK_MODAL_INITIAL_HEIGHT: f32 = 260.0;

const CONTENT_GAP: f32 = 14.0;
const FIT_EXTRA_HEIGHT: f32 =
    MODAL_WINDOW_PADDING * 2.0 + MODAL_SECTION_GAP + MODAL_FOOTER_BUTTON_HEIGHT;
const CHECK_ICON: &str = "modals/settings/check.svg";

/// How this computer goes back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GoBackModalWay {
    /// Ghostex swaps the kept version in.
    Swap,
    /// Run this command (deb, rpm).
    Command(String),
    /// Download it from GitHub.
    Download,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoBackModalConfig {
    pub(crate) current: String,
    pub(crate) previous: String,
    pub(crate) way: GoBackModalWay,
    /// The crash reporter can send the last crash.
    pub(crate) can_send_report: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GoBackModalCommand {
    TryAgain,
    GoBack { send_report: bool },
    CopyCommand(String),
    Download { send_report: bool },
}

pub(crate) type GoBackModalHost = Rc<dyn Fn(GoBackModalCommand, &mut App)>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Stage {
    Asking,
    Going,
    Failed(String),
}

pub(crate) struct GoBackModalWindow {
    config: GoBackModalConfig,
    palette: ModalPalette,
    host: GoBackModalHost,
    send_report: bool,
    copied: bool,
    stage: Stage,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GoBackModalWindow {
    pub(crate) fn new(
        config: GoBackModalConfig,
        palette: ModalPalette,
        host: GoBackModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            send_report: config.can_send_report,
            config,
            palette,
            host,
            copied: false,
            stage: Stage::Asking,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    /// Going back started: Ghostex is about to close.
    pub(crate) fn set_going(&mut self, cx: &mut Context<Self>) {
        self.stage = Stage::Going;
        self.fit.refit();
        cx.notify();
    }

    /// Going back could not start: says why and offers the download instead.
    pub(crate) fn set_failed(&mut self, reason: String, cx: &mut Context<Self>) {
        self.stage = Stage::Failed(reason);
        self.fit.refit();
        cx.notify();
    }

    #[allow(dead_code)] // The native-modal-demo shows each stage.
    pub(crate) fn set_copied(&mut self, cx: &mut Context<Self>) {
        self.copied = true;
        cx.notify();
    }

    fn send(&self, command: GoBackModalCommand, cx: &mut App) {
        (self.host)(command, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() == "escape" && self.stage != Stage::Going {
            self.send(GoBackModalCommand::TryAgain, cx);
            cx.stop_propagation();
        }
    }

    fn text(&self, text: impl Into<SharedString>) -> AnyElement {
        div()
            .text_size(px(13.0))
            .line_height(px(20.15))
            .text_color(hsla(self.palette.muted))
            .child(text.into())
            .into_any_element()
    }

    fn checkbox(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let checked = self.send_report;
        // Never a near-black fill on glass: light looks take the lifted grey the on switch uses.
        let fill = if p.light {
            gpui::rgb(0x5c5c5c)
        } else {
            p.primary
        };
        let check = if p.light {
            gpui::rgb(0xffffff)
        } else {
            p.primary_foreground
        };
        h_flex()
            .id("go-back-send-report")
            .role(gpui::Role::CheckBox)
            .aria_label("Also send the crash report to Ghostex")
            .gap(px(8.0))
            .items_center()
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.send_report = !this.send_report;
                cx.notify();
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .size(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.0))
                    .border_1()
                    .border_color(hsla(if checked { fill } else { p.hairline }))
                    .when(checked, |this| {
                        this.bg(hsla(fill))
                            .child(modal_icon(CHECK_ICON, 13.0, check))
                    }),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .child("Also send the crash report to Ghostex"),
            )
            .into_any_element()
    }

    fn command_box(&self, command: &str) -> AnyElement {
        let p = self.palette;
        modal_raised_box(&p)
            .w_full()
            .px(px(10.0))
            .py(px(8.0))
            .font_family(MODAL_MONO_FONT)
            .text_size(px(12.0))
            .line_height(px(18.0))
            .child(command.to_string())
            .into_any_element()
    }

    fn content(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.palette;
        let GoBackModalConfig {
            current, previous, ..
        } = &self.config;
        match &self.stage {
            Stage::Going => vec![
                modal_header(
                    &p,
                    format!("Going back to {previous}"),
                    Some(format!(
                        "Ghostex closes and opens again on {previous}. Your chats, projects and settings stay as they are."
                    )),
                ),
                h_flex()
                    .justify_center()
                    .child(modal_spinner(p.muted))
                    .into_any_element(),
            ],
            Stage::Failed(reason) => vec![
                modal_header(
                    &p,
                    format!("Couldn't go back to {previous}"),
                    Some(format!(
                        "You can download {previous} from GitHub and install it over this version."
                    )),
                ),
                modal_error(&p, reason.clone()),
            ],
            Stage::Asking => {
                let mut content = vec![modal_header(
                    &p,
                    format!("Ghostex {current} isn't working on this computer"),
                    Some(format!(
                        "It closed unexpectedly twice since it updated. You can go back to {previous}, the version you had before. Your chats, projects and settings stay as they are."
                    )),
                )];
                match &self.config.way {
                    GoBackModalWay::Swap => content.push(self.text(format!(
                        "Ghostex won't offer {current} again. You'll get the next version that fixes this."
                    ))),
                    GoBackModalWay::Command(command) => {
                        content.push(self.text(format!(
                            "To go back, run this in a terminal, then open Ghostex again:"
                        )));
                        content.push(self.command_box(command));
                    }
                    GoBackModalWay::Download => content.push(self.text(format!(
                        "Download {previous} from GitHub and install it over this version."
                    ))),
                }
                if self.config.can_send_report {
                    content.push(self.checkbox(cx));
                }
                content
            }
        }
    }

    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let previous = self.config.previous.clone();
        let try_again = modal_action_button(
            &p,
            "go-back-try-again",
            "Try again",
            None,
            ModalButtonTone::Neutral,
            self.stage == Stage::Going,
            |this: &mut Self, _, cx| this.send(GoBackModalCommand::TryAgain, cx),
            cx,
        );
        let download = |label: String, cx: &mut Context<Self>| {
            modal_action_button(
                &p,
                "go-back-download",
                label,
                None,
                ModalButtonTone::Primary,
                false,
                |this: &mut Self, _, cx| {
                    let send_report = this.send_report;
                    this.send(GoBackModalCommand::Download { send_report }, cx)
                },
                cx,
            )
        };
        let main = match (&self.stage, &self.config.way) {
            (Stage::Failed(_), _) | (Stage::Asking, GoBackModalWay::Download) => {
                download(format!("Download {previous}"), cx)
            }
            (_, GoBackModalWay::Command(command)) => {
                let command = command.clone();
                modal_action_button(
                    &p,
                    "go-back-copy-command",
                    if self.copied {
                        "Copied"
                    } else {
                        "Copy command"
                    },
                    None,
                    ModalButtonTone::Primary,
                    false,
                    move |this: &mut Self, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(command.clone()));
                        this.copied = true;
                        this.send(GoBackModalCommand::CopyCommand(command.clone()), cx);
                        cx.notify();
                    },
                    cx,
                )
            }
            _ => modal_action_button(
                &p,
                "go-back-confirm",
                format!("Go back to {previous}"),
                None,
                ModalButtonTone::Primary,
                self.stage == Stage::Going,
                |this: &mut Self, _, cx| {
                    let send_report = this.send_report;
                    this.send(GoBackModalCommand::GoBack { send_report }, cx);
                },
                cx,
            ),
        };
        modal_footer(vec![try_again, main])
    }
}

impl Render for GoBackModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let content = self.content(cx);
        let footer = self.footer(cx);
        let measured = v_flex()
            .w_full()
            .on_children_prepainted(self.fit.listener(FIT_EXTRA_HEIGHT))
            .child(v_flex().w_full().gap(px(CONTENT_GAP)).children(content));
        div()
            .id("ghostex-gpui-go-back-modal")
            .size_full()
            .overflow_hidden()
            .bg(hsla(p.surface))
            .font_family(MODAL_UI_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                v_flex()
                    .size_full()
                    .p(px(MODAL_WINDOW_PADDING))
                    .gap(px(MODAL_SECTION_GAP))
                    .child(measured.flex_1())
                    .child(footer),
            )
    }
}

impl ModalCornerClose for GoBackModalWindow {
    fn close_from_corner(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.send(GoBackModalCommand::TryAgain, cx);
    }

    fn shows_corner_close(&self, _cx: &App) -> bool {
        false
    }
}
