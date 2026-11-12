//! Native GPUI New bot dialog, opened from the Bots list's New bot row: the new Hermes profile's
//! name, an optional one-line description, and what it starts from (a copy of a bot, or blank).
//! Create stays in the dialog until Hermes answers, so a refusal (the name is taken or reserved)
//! shows here and the name can be changed.
//!
//! CDXC:Bots 2026-11-12 DECISION:
//! User: "Yes, add it" to "Should the Bots list get a New bot button that asks for a name and creates the profile for you?". The dialog asks for the name (Hermes's rule shown, anything else refused with a plain sentence), a one-line description (optional) and Start from: "A copy of <current bot>" first and picked, or Blank. A copy takes the bot's settings, SOUL.md and skills, never its Discord or Telegram tokens.
//! SEE-ALSO: apps/desktop/src/app/new_bot_modal_lifecycle.rs (open, create, the new row selected), server/src/server/bot_create.rs (`/api/createBot`), server/src/bot_projects.rs (`is_new_bot_profile_name`, the same rule on the server).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, SharedString,
    Styled as _, Subscription, WeakEntity, Window, px,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState};
use gpui_component::v_flex;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

pub(crate) const NEW_BOT_MODAL_WIDTH: f32 = 480.0;
pub(crate) const NEW_BOT_MODAL_INITIAL_HEIGHT: f32 = 540.0;

const TITLE: &str = "New bot";
const DESCRIPTION: &str = "A bot is a Hermes profile with its own settings, SOUL.md and skills. It joins this list as soon as it's made.";
const FIELD_NAME: &str = "Name";
const NAME_PLACEHOLDER: &str = "e.g. researcher";
const NAME_RULE: &str =
    "Lowercase letters and digits, plus - and _ after the first. It's the name you give hermes -p.";
const NAME_REFUSED: &str =
    "Use only lowercase letters and digits, with - or _ after the first one.";
const NAME_IS_HERMES: &str = "That name is the Hermes bot itself. Pick another.";
const FIELD_DESCRIPTION: &str = "Description (optional)";
const DESCRIPTION_PLACEHOLDER: &str = "e.g. Finds and compares flights";
const FIELD_START_FROM: &str = "Start from";
const BLANK: &str = "Blank";
const BLANK_HINT: &str = "Starts with Hermes's default settings and no SOUL.md of its own.";
const CANCEL: &str = "Cancel";
const CREATE: &str = "Create bot";
const CREATING: &str = "Creating…";

/// The bot a new one can be copied from: its Hermes profile and the name its row shows.
#[derive(Clone, Debug)]
pub(crate) struct NewBotSource {
    pub(crate) profile: String,
    pub(crate) title: String,
}

pub(crate) enum NewBotModalCommand {
    Create {
        profile: String,
        /// The bot it copies (`hermes profile create --clone-from`); `None` for Blank.
        clone_from: Option<String>,
        description: Option<String>,
        reply: NewBotCreateReply,
    },
    Cancel,
}

pub(crate) type NewBotModalHost = Rc<dyn Fn(NewBotModalCommand, &mut App)>;

pub(crate) struct NewBotModalConfig {
    pub(crate) palette: ModalPalette,
    /// The bot "A copy of" offers: the one open in the list, else Hermes. `None` offers only Blank.
    pub(crate) source: Option<NewBotSource>,
    /// The other computer the bot is made on, named in the description; `None` on this one.
    pub(crate) computer: Option<String>,
}

/// Where Create's outcome goes: the dialog that sent it, in its own window.
pub(crate) struct NewBotCreateReply {
    pub(crate) view: WeakEntity<GpuiNewBotModalWindow>,
    pub(crate) window: AnyWindowHandle,
}

impl NewBotCreateReply {
    /// Created closes the dialog; a refusal keeps it open with Hermes's reason. Returns whether
    /// the dialog was still there to take it.
    pub(crate) fn finish(self, result: Result<(), String>, cx: &mut App) -> bool {
        let view = self.view;
        self.window
            .update(cx, |_root, window, cx| {
                view.update(cx, |dialog, cx| dialog.finish_create(result, window, cx))
                    .is_ok()
            })
            .unwrap_or(false)
    }
}

/// What is wrong with a typed name, or `None` when Hermes takes it (or nothing is typed yet).
/// Hermes's own rule (`PROFILE_ID_RE`), as `is_new_bot_profile_name` in server/src/bot_projects.rs.
pub(crate) fn new_bot_name_problem(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return None;
    }
    if name == "default" {
        return Some(NAME_IS_HERMES);
    }
    let mut characters = name.chars();
    let first_fits = characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit());
    let rest_fits = characters.all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '-' | '_')
    });
    (!first_fits || !rest_fits || name.len() > 64).then_some(NAME_REFUSED)
}

/// A segment label the shared segmented control can hold (it takes `&'static str`): each distinct
/// "A copy of <bot>" is kept once for the life of the app.
fn segment_label(text: String) -> &'static str {
    static LABELS: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut labels = LABELS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(label) = labels.get(text.as_str()) {
        return label;
    }
    let label: &'static str = Box::leak(text.into_boxed_str());
    labels.insert(label);
    label
}

pub(crate) struct GpuiNewBotModalWindow {
    host: NewBotModalHost,
    palette: ModalPalette,
    name_input: Entity<InputState>,
    description_input: Entity<InputState>,
    name: String,
    source: Option<NewBotSource>,
    computer: Option<String>,
    /// Start from: 0 is the copy, 1 is Blank.
    start_from: usize,
    pending: bool,
    /// Why the last Create was refused, Hermes's own words.
    error: Option<String>,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiNewBotModalWindow {
    pub(crate) fn new(
        config: NewBotModalConfig,
        host: NewBotModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder(NAME_PLACEHOLDER));
        let description_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(DESCRIPTION_PLACEHOLDER));
        let name_subscription = cx.subscribe_in(
            &name_input,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.name = input.read(cx).value().trim().to_string();
                    this.error = None;
                    cx.notify();
                }
            },
        );
        name_input.update(cx, |input, cx| input.focus(window, cx));
        let start_from = if config.source.is_some() { 0 } else { 1 };
        Self {
            host,
            palette: config.palette,
            name_input,
            description_input,
            name: String::new(),
            source: config.source,
            computer: config.computer,
            start_from,
            pending: false,
            error: None,
            fit: ModalFit::new(),
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![name_subscription],
        }
    }

    /// Types a name and shows a refusal, for the preview binary.
    #[allow(dead_code)]
    pub(crate) fn preview_fill(
        &mut self,
        name: &str,
        description: &str,
        refusal: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.name_input.update(cx, |input, cx| {
            input.set_value(name.to_string(), window, cx)
        });
        self.description_input.update(cx, |input, cx| {
            input.set_value(description.to_string(), window, cx)
        });
        self.name = name.to_string();
        self.error = refusal.map(str::to_string);
        cx.notify();
    }

    fn can_create(&self) -> bool {
        !self.pending && !self.name.is_empty() && new_bot_name_problem(&self.name).is_none()
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(NewBotModalCommand::Cancel, cx);
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_create() {
            return;
        }
        let description = self
            .description_input
            .read(cx)
            .value()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let clone_from = self
            .source
            .as_ref()
            .filter(|_| self.start_from == 0)
            .map(|source| source.profile.clone());
        self.pending = true;
        self.error = None;
        cx.notify();
        (self.host)(
            NewBotModalCommand::Create {
                profile: self.name.clone(),
                clone_from,
                description: Some(description).filter(|text| !text.is_empty()),
                reply: NewBotCreateReply {
                    view: cx.entity().downgrade(),
                    window: window.window_handle(),
                },
            },
            cx,
        );
    }

    fn finish_create(
        &mut self,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending = false;
        match result {
            Ok(()) => window.remove_window(),
            Err(refusal) => {
                self.error = Some(refusal);
                cx.notify();
            }
        }
    }

    fn on_enter_action(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.create(window, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.cancel(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => self.cancel(window, cx),
            "enter" if !event.is_held => self.create(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_start_from(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let source = self.source.as_ref()?;
        let p = self.palette;
        let items = [
            ModalSegmentedItem {
                icon: None,
                label: segment_label(format!("A copy of {}", source.title)),
            },
            ModalSegmentedItem {
                icon: None,
                label: BLANK,
            },
        ];
        let hint = if self.start_from == 0 {
            format!(
                "Copies {}'s settings, SOUL.md and skills. Its Discord and Telegram tokens stay with {}.",
                source.title, source.title
            )
        } else {
            BLANK_HINT.to_string()
        };
        Some(
            v_flex()
                .gap(px(8.0))
                .child(modal_section_title(&p, FIELD_START_FROM))
                .child(modal_segmented_control(
                    &p,
                    "new-bot-start-from",
                    &items,
                    self.start_from,
                    |this: &mut Self, index, _window, cx| {
                        this.start_from = index;
                        cx.notify();
                    },
                    cx,
                ))
                .child(modal_hint(&p, hint))
                .into_any_element(),
        )
    }

    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let problem = new_bot_name_problem(&self.name);
        v_flex()
            .w_full()
            .gap(px(16.0))
            .child(
                v_flex()
                    .gap(px(8.0))
                    .child(modal_section_title(&p, FIELD_NAME))
                    .child(modal_text_input(
                        &p,
                        &self.name_input,
                        self.pending,
                        window,
                        cx,
                    ))
                    .child(match problem {
                        Some(problem) => modal_error(&p, problem),
                        None => modal_hint(&p, NAME_RULE).into_any_element(),
                    }),
            )
            .child(
                v_flex()
                    .gap(px(8.0))
                    .child(modal_section_title(&p, FIELD_DESCRIPTION))
                    .child(modal_text_input(
                        &p,
                        &self.description_input,
                        self.pending,
                        window,
                        cx,
                    )),
            )
            .children(self.render_start_from(cx))
            .when_some(self.error.clone(), |body, error| {
                body.child(modal_error(&p, SharedString::from(error)))
            })
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let cancel = modal_action_button(
            &p,
            "new-bot-cancel",
            CANCEL,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let create = modal_action_button(
            &p,
            "new-bot-create",
            if self.pending { CREATING } else { CREATE },
            None,
            ModalButtonTone::Primary,
            !self.can_create(),
            |this, window, cx| this.create(window, cx),
            cx,
        );
        modal_footer(vec![cancel, create])
    }
}

impl Render for GpuiNewBotModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let description = match &self.computer {
            Some(computer) => format!("{DESCRIPTION} It's made on {computer}."),
            None => DESCRIPTION.to_string(),
        };
        let content = vec![
            modal_header(&p, TITLE, Some(description)),
            self.render_body(window, cx),
        ];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-new-bot-modal",
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

impl ModalCornerClose for GpuiNewBotModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
