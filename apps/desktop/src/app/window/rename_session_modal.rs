//! Native GPUI Rename Session dialog, the desktop twin of the React
//! `SessionRenameModal` in packages/core-ui/session-rename-modal.tsx (deleted 2026-10-01).
//!
//! CDXC:SessionTitles 2026-11-01 DECISION:
//! User (picking option A of the rename mockup): Generate only when it applies. Text of 1 to 70 characters is a plain rename (Cancel, Rename). Longer text, or an empty box ("longer than 70 and empty input also"), turns the counter amber, steps Rename back and makes "Generate name" the main button; the generated name lands in the box and the dialog is back to a plain rename. "dont show the dropdown to select what to use to generate the name. show text on the left saying \"Using <agent name> - Set it in settings.\"": the generator is the Title Generation Agent setting and the link opens that row. This supersedes the 2026-09-15 decision that kept the React dialog's fixed 570 x 440 frame and Generate-with dropdown; the window now fits its content.
//! SEE-ALSO: apps/desktop/src/app/window/native_modal_kit/ (shared chrome and controls), apps/desktop/src/app/rename_session_modal_lifecycle.rs (open, close, the `renameSession` command, the name generator), server/src/server/title_generation/title_preview.rs (`/api/previewSessionTitle`), apps/desktop/src/bin/native_modal_demo/rename_session.rs (standalone preview).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, Window, div, px,
};
use gpui_component::input::{Enter, Escape, InputEvent, TextareaState};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;
use std::sync::OnceLock;

pub(crate) const RENAME_SESSION_MODAL_WIDTH: f32 = 570.0;
/// The window fits its content after the first paint; this is only the size it opens at.
pub(crate) const RENAME_SESSION_MODAL_INITIAL_HEIGHT: f32 = 240.0;

/// `SESSION_RENAME_GENERATE_NAME_THRESHOLD`: longer text is a description to generate a name from.
const GENERATE_NAME_THRESHOLD: usize = 70;
/// The name box grows with its text up to this many lines, then scrolls.
const NAME_MAX_ROWS: usize = 6;

const TITLE: &str = "Rename Session";
const DESCRIPTION_WITH_HISTORY: &str = "Type a name, or paste longer text to generate one. Leave it empty to name it from this session.";
const DESCRIPTION: &str = "Type a name, or paste longer text to generate one.";
const CANCEL: &str = "Cancel";
const RENAME: &str = "Rename";
const GENERATE_NAME: &str = "Generate name";
const GENERATING: &str = "Generating…";
const TOO_LONG: &str = "too long for a name";
const EMPTY_WITH_HISTORY: &str = "generate one from this session";
const EMPTY: &str = "type a name";
const SET_IT_IN_SETTINGS: &str = "Set it in settings";

/// The counter's amber once the text is not a name (the mockup's `--d-warn`).
const WARNING_DARK: u32 = 0xe0b04a;
const WARNING_LIGHT: u32 = 0xa8740c;

/// `SESSION_RENAME_UNSUPPORTED_GLYPH_PATTERN`: everything that is not a letter,
/// a number, a space, or ASCII punctuation is dropped from a direct rename.
const UNSUPPORTED_GLYPH_PATTERN: &str = r##"[^\p{L}\p{N} !"#$%&'()*+,\-./:;<=>?@\[\\\]^_`{|}~]"##;

/// Port of `normalizeSessionRenameTitle`: NFKC-normalize, collapse whitespace
/// to single spaces, strip the unsupported glyphs, collapse again, trim.
/// `None` when nothing is left.
pub(crate) fn normalize_session_rename_title(title: &str) -> Option<String> {
    static UNSUPPORTED_GLYPHS: OnceLock<regex::Regex> = OnceLock::new();
    let unsupported = UNSUPPORTED_GLYPHS.get_or_init(|| {
        regex::Regex::new(UNSUPPORTED_GLYPH_PATTERN).expect("the rename glyph pattern compiles")
    });
    let collapse = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = icu_normalizer::ComposingNormalizer::new_nfkc().normalize(title);
    let collapsed = collapse(&normalized);
    let stripped = unsupported.replace_all(&collapsed, "");
    let normalized = collapse(&stripped);
    (!normalized.is_empty()).then_some(normalized)
}

/// JavaScript `String.length`: UTF-16 code units, the unit of the 70-character rule.
fn js_length(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Generates a name from the text (empty: from the session's own prompts) without applying it.
pub(crate) type RenameSessionGenerator =
    Rc<dyn Fn(String, &mut App) -> Task<Result<String, String>>>;

/// What the dialog asks its host to do. The dialog removes its own window
/// before sending any of these.
pub(crate) enum RenameSessionModalCommand {
    /// `renameSession { sessionId, title }`: the normalized title applied verbatim.
    Rename {
        title: String,
    },
    /// Settings › Agents at the Title Generation Agent row.
    OpenGeneratorSettings,
    Cancel,
}

pub(crate) type RenameSessionModalHost = Rc<dyn Fn(RenameSessionModalCommand, &mut App)>;

pub(crate) struct RenameSessionModalConfig {
    pub(crate) initial_title: String,
    /// `canGenerateNameFromSessionHistory`: the session's agent can name it from its transcript.
    pub(crate) can_generate_from_history: bool,
    /// The Title Generation Agent setting's label ("Codex", "Claude", ...).
    pub(crate) generator_name: String,
    pub(crate) generate: RenameSessionGenerator,
    pub(crate) palette: ModalPalette,
}

/// Which button is the main one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RenameMode {
    Rename,
    /// Longer than a name, or empty.
    Generate,
}

pub(crate) struct GpuiRenameSessionModalWindow {
    host: RenameSessionModalHost,
    generate: RenameSessionGenerator,
    palette: ModalPalette,
    generator_name: String,
    can_generate_from_history: bool,
    input: Entity<TextareaState>,
    /// Mirror of the editor's text, refreshed on every change.
    title: String,
    generating: bool,
    generate_error: Option<String>,
    _generate_task: Option<Task<()>>,
    mode: RenameMode,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiRenameSessionModalWindow {
    pub(crate) fn new(
        config: RenameSessionModalConfig,
        host: RenameSessionModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, NAME_MAX_ROWS)
                .default_value(config.initial_title.clone())
        });
        let change_subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.title = input.read(cx).value().to_string();
                    this.generate_error = None;
                    this.sync_mode();
                    cx.notify();
                }
            },
        );
        // Open with the caret in the name field and the existing title fully
        // selected so a replacement can be typed straight away.
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            let end = input.value().len();
            input.set_selected_range(0..end, cx);
        });
        let mut this = Self {
            host,
            generate: config.generate,
            palette: config.palette,
            generator_name: config.generator_name,
            can_generate_from_history: config.can_generate_from_history,
            input,
            title: config.initial_title,
            generating: false,
            generate_error: None,
            _generate_task: None,
            mode: RenameMode::Rename,
            fit: ModalFit::new(),
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![change_subscription],
        };
        this.mode = this.current_mode();
        this._subscriptions
            .extend(super::popup_dismissal::close_app_modal_on_click_away(
                window, cx,
            ));
        this
    }

    fn trimmed_title(&self) -> &str {
        self.title.trim()
    }

    fn current_mode(&self) -> RenameMode {
        let length = js_length(self.trimmed_title());
        if length == 0 || length > GENERATE_NAME_THRESHOLD {
            RenameMode::Generate
        } else {
            RenameMode::Rename
        }
    }

    /// The footer changes shape between the modes, so the window fits it again.
    fn sync_mode(&mut self) {
        let mode = self.current_mode();
        if mode != self.mode {
            self.mode = mode;
            self.fit.refit();
        }
    }

    fn can_rename(&self) -> bool {
        self.mode == RenameMode::Rename
            && !self.generating
            && normalize_session_rename_title(&self.title).is_some()
    }

    fn can_generate(&self) -> bool {
        self.mode == RenameMode::Generate
            && !self.generating
            && (!self.trimmed_title().is_empty() || self.can_generate_from_history)
    }

    fn close_window_and_send(
        &mut self,
        command: RenameSessionModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(RenameSessionModalCommand::Cancel, window, cx);
    }

    fn rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_rename() {
            return;
        }
        let Some(title) = normalize_session_rename_title(&self.title) else {
            return;
        };
        self.close_window_and_send(RenameSessionModalCommand::Rename { title }, window, cx);
    }

    fn open_generator_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(RenameSessionModalCommand::OpenGeneratorSettings, window, cx);
    }

    /// Asks for a name and puts it in the box; the user still presses Rename.
    fn generate_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_generate() {
            return;
        }
        self.generating = true;
        self.generate_error = None;
        let pending = (self.generate)(self.trimmed_title().to_string(), cx);
        self._generate_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = pending.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.generating = false;
                match result {
                    Ok(title) => {
                        this.input.update(cx, |input, cx| {
                            input.set_value(title.clone(), window, cx);
                            input.focus(window, cx);
                            // Selected like the opening title, so typing replaces it.
                            let end = input.value().len();
                            input.set_selected_range(0..end, cx);
                        });
                        this.title = title;
                        this.sync_mode();
                    }
                    Err(message) => this.generate_error = Some(message),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Enter does the main button of the current mode.
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.mode {
            RenameMode::Rename => self.rename(window, cx),
            RenameMode::Generate => self.generate_name(window, cx),
        }
    }

    /// Enter from inside the name field: plain Enter submits, Shift+Enter
    /// keeps its newline (the field's own handler runs after this capture).
    /// A capture-phase action listener must stop propagation itself, or the
    /// field's own handler and the frame's key listener run too.
    fn on_enter_action(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if action.shift {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
        self.submit(window, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.cancel(window, cx);
    }

    /// Keys while the frame holds focus (the name field lost it to a click elsewhere).
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => self.cancel(window, cx),
            "enter" if !event.is_held => self.submit(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn warning_color(&self) -> gpui::Rgba {
        modal_rgba(
            if self.palette.light {
                WARNING_LIGHT
            } else {
                WARNING_DARK
            },
            1.0,
        )
    }

    /// `14 / 70`, or in amber with the reason the text is not a name.
    fn render_counter(&self) -> AnyElement {
        let length = js_length(self.trimmed_title());
        let reason = match self.mode {
            RenameMode::Rename => None,
            RenameMode::Generate if length > 0 => Some(TOO_LONG),
            RenameMode::Generate if self.can_generate_from_history => Some(EMPTY_WITH_HISTORY),
            RenameMode::Generate => Some(EMPTY),
        };
        let text = match reason {
            Some(reason) => format!("{length} / {GENERATE_NAME_THRESHOLD} · {reason}"),
            None => format!("{length} / {GENERATE_NAME_THRESHOLD}"),
        };
        let color = if reason.is_some() {
            self.warning_color()
        } else {
            self.palette.muted
        };
        div()
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(hsla(color))
            .child(text)
            .into_any_element()
    }

    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .gap(px(8.0))
            // `.session-rename-modal-shadcn [data-slot='textarea']`: 14px at line-height 1.45.
            .line_height(px(20.3))
            .child(modal_text_area(
                &p,
                &self.input,
                Some(0.0),
                self.generating,
                window,
                cx,
            ))
            .child(self.render_counter())
            .when_some(self.generate_error.clone(), |this, error| {
                this.child(modal_error(&p, error))
            })
            .into_any_element()
    }

    /// "Using Codex - Set it in settings.", the link opening the Title Generation Agent row.
    fn render_generator_note(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        h_flex()
            .flex_1()
            .min_w_0()
            .flex_wrap()
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(hsla(p.muted))
            .child(format!("Using {} - ", self.generator_name))
            .child(
                div()
                    .id("rename-session-generator-settings")
                    .role(gpui::Role::Link)
                    .aria_label(SET_IT_IN_SETTINGS)
                    .text_color(hsla(p.foreground))
                    .underline()
                    .cursor_pointer()
                    .on_click(
                        cx.listener(|this, _, window, cx| this.open_generator_settings(window, cx)),
                    )
                    .child(SET_IT_IN_SETTINGS),
            )
            .child(".")
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let cancel = modal_action_button(
            &p,
            "rename-session-cancel",
            CANCEL,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let rename_tone = match self.mode {
            RenameMode::Rename => ModalButtonTone::Primary,
            RenameMode::Generate => ModalButtonTone::Neutral,
        };
        let rename = modal_action_button(
            &p,
            "rename-session-rename",
            RENAME,
            None,
            rename_tone,
            !self.can_rename(),
            |this, window, cx| this.rename(window, cx),
            cx,
        );
        if self.mode == RenameMode::Rename {
            return modal_footer(vec![cancel, rename]);
        }
        let generate = modal_action_button(
            &p,
            "rename-session-generate",
            if self.generating {
                GENERATING
            } else {
                GENERATE_NAME
            },
            self.generating
                .then(|| modal_spinner(p.primary_button_text())),
            ModalButtonTone::Primary,
            !self.can_generate(),
            |this, window, cx| this.generate_name(window, cx),
            cx,
        );
        let sized =
            |width: f32, button: AnyElement| h_flex().flex_none().w(px(width)).child(button);
        h_flex()
            .w_full()
            .gap(px(12.0))
            .items_center()
            .child(self.render_generator_note(cx))
            .child(
                h_flex()
                    .flex_none()
                    .gap(px(8.0))
                    .child(sized(80.0, cancel))
                    .child(sized(80.0, rename))
                    .child(sized(136.0, generate)),
            )
            .into_any_element()
    }
}

impl Render for GpuiRenameSessionModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let description = if self.can_generate_from_history {
            DESCRIPTION_WITH_HISTORY
        } else {
            DESCRIPTION
        };
        let content = vec![
            modal_header(&p, TITLE, Some(description)),
            self.render_body(window, cx),
        ];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-rename-session-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
        .capture_action(cx.listener(Self::on_enter_action))
        .capture_action(cx.listener(Self::on_escape_action))
    }
}

impl ModalCornerClose for GpuiRenameSessionModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
