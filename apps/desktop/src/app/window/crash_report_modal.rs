//! "Ghostex closed unexpectedly": the crash window the next open shows after a crash. One line
//! says what happened, "What's sent" opens the exact report, Send report hands it to the host
//! (which posts it through gxserver to the feedback service), Don't send drops it. Then "Thanks,
//! it's sent (#<issue>)". The window knows nothing of crash files or gxserver, so the
//! native-modal-demo shows this same file.
//! The decision record is in crash_report/mod.rs (CDXC:CrashReports 2026-11-17 DECISION); the
//! design is docs/2026-11-17/release-safety-simple/index.html, "Report a crash in one click".
use super::native_modal_kit::*;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

pub(crate) const CRASH_REPORT_MODAL_WIDTH: f32 = 520.0;
pub(crate) const CRASH_REPORT_MODAL_INITIAL_HEIGHT: f32 = 300.0;

const CONTENT_GAP: f32 = 14.0;
const FIT_EXTRA_HEIGHT: f32 =
    MODAL_WINDOW_PADDING * 2.0 + MODAL_SECTION_GAP + MODAL_FOOTER_BUTTON_HEIGHT;
/// "What's sent" scrolls inside this height.
const REPORT_MAX_HEIGHT: f32 = 300.0;

/// What the window asks its host to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CrashReportModalCommand {
    /// Send report, with the person's note: the host sends it and answers with
    /// [`GpuiCrashReportModalWindow::set_sent`] or [`GpuiCrashReportModalWindow::set_failed`].
    Send { note: String },
    /// Don't send (the window is gone).
    DontSend,
    /// Open the issue it went to.
    OpenIssue(String),
    /// Closed without deciding, the crash is offered again at the next open (the window is gone).
    Closed,
    /// Close after it was sent (the window is gone).
    Done,
}

pub(crate) type CrashReportModalHost = Rc<dyn Fn(CrashReportModalCommand, &mut App)>;

/// The report exactly as it is sent, for a note.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CrashReportPreview {
    pub(crate) title: String,
    pub(crate) body: String,
    /// `Removed: your home folder (now ~), 2 emails`, when anything was.
    pub(crate) removed: Option<String>,
}

pub(crate) struct CrashReportModalConfig {
    pub(crate) palette: ModalPalette,
    /// What happened, in one line: `Ghostex 11.8.2 crashed in list.rs:488: RefCell already borrowed`.
    pub(crate) summary: String,
    /// The report with the person's note in it, so "What's sent" always shows what Send sends.
    pub(crate) report_for_note: Rc<dyn Fn(&str) -> CrashReportPreview>,
}

enum Step {
    Asking,
    Sent { number: u64, url: String },
}

pub(crate) struct GpuiCrashReportModalWindow {
    config: CrashReportModalConfig,
    host: CrashReportModalHost,
    report: CrashReportPreview,
    note_input: Entity<InputState>,
    note: String,
    expanded: bool,
    step: Step,
    busy: bool,
    error: Option<String>,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiCrashReportModalWindow {
    pub(crate) fn new(
        config: CrashReportModalConfig,
        host: CrashReportModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let note_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("What were you doing? (optional)"));
        let subscriptions = vec![cx.subscribe_in(
            &note_input,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.note = input.read(cx).value().to_string();
                    this.report = (this.config.report_for_note)(&this.note);
                    cx.notify();
                }
            },
        )];
        let report = (config.report_for_note)("");
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            config,
            host,
            report,
            note_input,
            note: String::new(),
            expanded: false,
            step: Step::Asking,
            busy: false,
            error: None,
            fit: ModalFit::new(),
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    /// The report reached the feedback service.
    pub(crate) fn set_sent(
        &mut self,
        number: u64,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = false;
        self.error = None;
        self.step = Step::Sent { number, url };
        self.fit.refit();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// The report could not be sent; Send report can be pressed again.
    pub(crate) fn set_failed(&mut self, reason: String, cx: &mut Context<Self>) {
        self.busy = false;
        self.error = Some(reason);
        self.fit.refit();
        cx.notify();
    }

    /// "What's sent" open, for the preview.
    pub(crate) fn set_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        self.expanded = expanded;
        self.fit.refit();
        cx.notify();
    }

    fn finish(&mut self, command: CrashReportModalCommand, window: &mut Window, cx: &mut App) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn dont_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.busy {
            self.finish(CrashReportModalCommand::DontSend, window, cx);
        }
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        if self.busy || !matches!(self.step, Step::Asking) {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        (self.host)(
            CrashReportModalCommand::Send {
                note: self.note.clone(),
            },
            cx,
        );
    }

    fn open_issue(&mut self, cx: &mut Context<Self>) {
        if let Step::Sent { url, .. } = &self.step {
            (self.host)(CrashReportModalCommand::OpenIssue(url.clone()), cx);
        }
    }

    fn toggle_expanded(&mut self, cx: &mut Context<Self>) {
        self.set_expanded(!self.expanded, cx);
    }

    fn close_command(&self) -> CrashReportModalCommand {
        match self.step {
            Step::Asking => CrashReportModalCommand::Closed,
            Step::Sent { .. } => CrashReportModalCommand::Done,
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" if !self.busy => {
                let command = self.close_command();
                self.finish(command, window, cx);
            }
            "enter" if !event.is_held => match self.step {
                Step::Asking => self.send(cx),
                Step::Sent { .. } => self.finish(CrashReportModalCommand::Done, window, cx),
            },
            _ => return,
        }
        cx.stop_propagation();
    }

    fn whats_sent(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette();
        let toggle = h_flex()
            .id("crash-report-whats-sent")
            .w_full()
            .justify_between()
            .items_center()
            .px(px(12.0))
            .py(px(9.0))
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(switch_row_wash(p.foreground))))
            .on_bound_press(bind_press(cx, |this: &mut Self, _window, cx| {
                this.toggle_expanded(cx)
            }))
            .child(div().text_size(px(13.0)).child("What's sent"))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(hsla(p.muted))
                    .child(if self.expanded { "Hide" } else { "Show" }),
            );
        let mut panel = modal_panel(&p).child(toggle);
        if self.expanded {
            let text = format!("{}\n\n{}", self.report.title, self.report.body);
            panel = panel.child(
                div()
                    .id("crash-report-text")
                    .w_full()
                    .max_h(px(REPORT_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .border_t_1()
                    .border_color(hsla(p.hairline))
                    .px(px(12.0))
                    .py(px(10.0))
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(11.5))
                    .line_height(px(16.5))
                    .whitespace_normal()
                    .child(text),
            );
            if let Some(removed) = &self.report.removed {
                panel = panel.child(
                    div()
                        .w_full()
                        .border_t_1()
                        .border_color(hsla(p.hairline))
                        .px(px(12.0))
                        .py(px(8.0))
                        .child(modal_hint(&p, removed.clone())),
                );
            }
        }
        panel.into_any_element()
    }

    fn palette(&self) -> ModalPalette {
        self.config.palette
    }

    fn content(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.palette();
        match &self.step {
            Step::Sent { number, .. } => vec![modal_header(
                &p,
                format!("Thanks, it's sent (#{number})"),
                Some("We'll start on a fix. You can follow it on GitHub."),
            )],
            Step::Asking => {
                let mut content = vec![
                    modal_header(
                        &p,
                        "Ghostex closed unexpectedly",
                        Some(
                            "Send the report and we'll start on a fix. It has what crashed and \
                             your system details, never your chats, files or keys.",
                        ),
                    ),
                    modal_raised_box(&p)
                        .w_full()
                        .px(px(12.0))
                        .py(px(9.0))
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .child(self.config.summary.clone())
                        .into_any_element(),
                    self.whats_sent(cx),
                    modal_text_input(&p, &self.note_input, self.busy, window, cx),
                ];
                if let Some(error) = self.error.clone() {
                    content.push(modal_error(&p, error));
                }
                content
            }
        }
    }

    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette();
        let buttons = match self.step {
            Step::Asking => vec![
                modal_action_button(
                    &p,
                    "crash-report-dont-send",
                    "Don't send",
                    None,
                    ModalButtonTone::Neutral,
                    self.busy,
                    |this: &mut Self, window, cx| this.dont_send(window, cx),
                    cx,
                ),
                modal_action_button(
                    &p,
                    "crash-report-send",
                    "Send report",
                    self.busy.then(|| modal_spinner(p.primary_button_text())),
                    ModalButtonTone::Primary,
                    self.busy,
                    |this: &mut Self, _window, cx| this.send(cx),
                    cx,
                ),
            ],
            Step::Sent { .. } => vec![
                modal_action_button(
                    &p,
                    "crash-report-close",
                    "Close",
                    None,
                    ModalButtonTone::Neutral,
                    false,
                    |this: &mut Self, window, cx| {
                        this.finish(CrashReportModalCommand::Done, window, cx)
                    },
                    cx,
                ),
                modal_action_button(
                    &p,
                    "crash-report-open-issue",
                    "Open issue",
                    None,
                    ModalButtonTone::Primary,
                    false,
                    |this: &mut Self, _window, cx| this.open_issue(cx),
                    cx,
                ),
            ],
        };
        modal_footer(buttons)
    }
}

impl Render for GpuiCrashReportModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette();
        let content = self.content(window, cx);
        let footer = self.footer(cx);
        let measured = v_flex()
            .w_full()
            .on_children_prepainted(self.fit.listener(FIT_EXTRA_HEIGHT))
            .child(v_flex().w_full().gap(px(CONTENT_GAP)).children(content));
        div()
            .id("ghostex-gpui-crash-report-modal")
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

impl ModalCornerClose for GpuiCrashReportModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let command = self.close_command();
        self.finish(command, window, cx);
    }
}
