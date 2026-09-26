//! Native GPUI Rename Session dialog, the desktop twin of the React
//! `SessionRenameModal` in packages/core-ui/session-rename-modal.tsx.
//!
//! CDXC:SessionTitles 2026-09-15 DECISION:
//! User: "make sure the new gpui modal is EXACTLY 1 to 1 matching the react one": the same layout, copy, colors, states, keyboard behaviour and `renameSession` command as the React dialog, in both appearances. The React twin is pinned to the child window's full height (`.rename-session-modal-shadcn` at 100vh), so this dialog keeps its 570 x 440 frame and stretches the name editor between the header and the footer instead of fitting the window to its content.
//! SEE-ALSO: packages/core-ui/session-rename-modal.tsx and the `.session-rename-modal-shadcn` rules in packages/core-ui/styles/modals.css (the React twin), packages/shared/session-grid-contract-session.ts (`normalizeSessionRenameTitle`, ported below), apps/desktop/src/app/window/native_modal_kit.rs (shared chrome and controls), apps/desktop/src/app/rename_session_modal_lifecycle.rs (open, close, the `renameSession` command), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, Styled as _, Subscription, Window, div,
    px,
};
use gpui_component::input::{Enter, Escape, InputEvent, TextareaState};
use gpui_component::{h_flex, v_flex};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;

/// `APP_MODAL_HOST_RENAME_SESSION_WINDOW_WIDTH` / `_HEIGHT`: the fixed frame the React dialog fills.
pub(crate) const RENAME_SESSION_MODAL_WIDTH: f32 = 570.0;
pub(crate) const RENAME_SESSION_MODAL_INITIAL_HEIGHT: f32 = 440.0;

/// `SESSION_RENAME_GENERATE_NAME_THRESHOLD`: longer typed text becomes a Generate Name submit.
const GENERATE_NAME_THRESHOLD: usize = 70;

const TITLE: &str = "Rename Session";
const DESCRIPTION_WITH_HISTORY: &str = "Rename directly, or Generate Name from longer text; leave the name unchanged to generate from the session's recent messages.";
const DESCRIPTION: &str = "Rename directly or generate a name from longer text.";
const FIELD_SESSION_NAME: &str = "Session name";
const FIELD_GENERATE_WITH: &str = "Generate with";
const SELECT_AGENT_PLACEHOLDER: &str = "Select agent";
const RENAME: &str = "Rename";
const GENERATE_NAME: &str = "Generate Name";

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

/// A configured agent that can generate the name (the React modal only
/// offers agents whose HUD button carries a launch command).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RenameSessionAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
}

/// Port of `resolvePromptAgentModalSelection`: the remembered choice, else the
/// Settings default prompt agent, else the first agent with a command.
pub(crate) fn resolve_prompt_agent_modal_selection(
    agents: &[RenameSessionAgent],
    saved_agent_id: Option<&str>,
    default_agent_id: Option<&str>,
) -> Option<String> {
    let find = |wanted: Option<&str>| {
        let wanted = wanted?;
        agents
            .iter()
            .find(|agent| agent.agent_id == wanted)
            .map(|agent| agent.agent_id.clone())
    };
    find(saved_agent_id)
        .or_else(|| find(default_agent_id))
        .or_else(|| agents.first().map(|agent| agent.agent_id.clone()))
}

/// The remembered Generate-with choice, the GPUI replacement for the React
/// host's `ghostex.promptAgent.renameSession` localStorage override. The host
/// cleared that override whenever the Settings default prompt agent changed,
/// so the file also records the default it was chosen under and only counts
/// while that default is still current.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RenameSessionModalPrefs {
    pub(crate) agent_id: String,
    pub(crate) default_prompt_agent_id: Option<String>,
}

pub(crate) fn load_rename_session_modal_prefs(path: &Path) -> Option<RenameSessionModalPrefs> {
    let value = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    Some(RenameSessionModalPrefs {
        agent_id: text("agentId")?,
        default_prompt_agent_id: text("defaultPromptAgentId"),
    })
}

pub(crate) fn persist_rename_session_modal_prefs(path: &Path, prefs: &RenameSessionModalPrefs) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({
        "agentId": prefs.agent_id,
        "defaultPromptAgentId": prefs.default_prompt_agent_id,
    });
    let _ = std::fs::write(path, payload.to_string());
}

/// What the dialog asks its host to do. The dialog removes its own window
/// before sending any of these.
pub(crate) enum RenameSessionModalCommand {
    /// `renameSession { sessionId, title }`: the normalized title applied verbatim.
    Rename {
        title: String,
    },
    /// `renameSession { agentId?, sessionId, shouldGenerateTitle: true, title }`:
    /// summarize `title` (or, when empty, the session's recent messages) into a name.
    GenerateName {
        title: String,
        agent_id: Option<String>,
    },
    Cancel,
}

pub(crate) type RenameSessionModalHost = Rc<dyn Fn(RenameSessionModalCommand, &mut App)>;

pub(crate) struct RenameSessionModalConfig {
    /// Already filtered to the agents with a launch command.
    pub(crate) agents: Vec<RenameSessionAgent>,
    /// `hud.settings.defaultPromptAgentId`.
    pub(crate) default_prompt_agent_id: Option<String>,
    pub(crate) initial_title: String,
    /// `canGenerateNameFromSessionHistory`: the session's agent can name it from its transcript.
    pub(crate) can_generate_from_history: bool,
    pub(crate) palette: ModalPalette,
    pub(crate) prefs_path: Option<PathBuf>,
}

pub(crate) struct GpuiRenameSessionModalWindow {
    host: RenameSessionModalHost,
    palette: ModalPalette,
    prefs_path: Option<PathBuf>,
    agents: Vec<RenameSessionAgent>,
    default_prompt_agent_id: Option<String>,
    initial_title: String,
    can_generate_from_history: bool,
    input: Entity<TextareaState>,
    /// Mirror of the editor's text, refreshed on every change.
    title: String,
    selected_agent_id: Option<String>,
    agent_select: ModalSelect,
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
        let saved_agent_id = config
            .prefs_path
            .as_deref()
            .and_then(load_rename_session_modal_prefs)
            .filter(|prefs| prefs.default_prompt_agent_id == config.default_prompt_agent_id)
            .map(|prefs| prefs.agent_id);
        let selected_agent_id = resolve_prompt_agent_modal_selection(
            &config.agents,
            saved_agent_id.as_deref(),
            config.default_prompt_agent_id.as_deref(),
        );
        let input =
            cx.new(|cx| TextareaState::new(window, cx).default_value(config.initial_title.clone()));
        let change_subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.title = input.read(cx).value().to_string();
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
        Self {
            host,
            palette: config.palette,
            prefs_path: config.prefs_path,
            agents: config.agents,
            default_prompt_agent_id: config.default_prompt_agent_id,
            title: config.initial_title.clone(),
            initial_title: config.initial_title,
            can_generate_from_history: config.can_generate_from_history,
            input,
            selected_agent_id,
            agent_select: ModalSelect::new(),
            fit: ModalFit::fixed(),
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![change_subscription],
        }
    }

    fn trimmed_title(&self) -> &str {
        self.title.trim()
    }

    /// Entered text longer than 70 characters is summarized instead of applied.
    fn can_generate_title(&self) -> bool {
        js_length(self.trimmed_title()) > GENERATE_NAME_THRESHOLD
    }

    /// An empty or unchanged name means "generate from the session's recent messages".
    fn can_generate_title_from_session_history(&self) -> bool {
        let trimmed = self.trimmed_title();
        self.can_generate_from_history
            && (trimmed.is_empty() || trimmed == self.initial_title.trim())
    }

    fn selected_agent_index(&self) -> Option<usize> {
        let selected = self.selected_agent_id.as_deref()?;
        self.agents
            .iter()
            .position(|agent| agent.agent_id == selected)
    }

    fn selected_agent_name(&self) -> Option<String> {
        self.selected_agent_index()
            .map(|index| self.agents[index].name.clone())
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

    fn confirm_generate_from_session_history(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let agent_id = self.selected_agent_id.clone();
        self.close_window_and_send(
            RenameSessionModalCommand::GenerateName {
                title: String::new(),
                agent_id,
            },
            window,
            cx,
        );
    }

    fn confirm_title(
        &mut self,
        next_title: &str,
        should_generate_title: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let normalized = if should_generate_title {
            Some(next_title.trim().to_string()).filter(|title| !title.is_empty())
        } else {
            normalize_session_rename_title(next_title)
        };
        let Some(title) = normalized else {
            return;
        };
        let command = if should_generate_title {
            RenameSessionModalCommand::GenerateName {
                title,
                agent_id: self.selected_agent_id.clone(),
            }
        } else {
            RenameSessionModalCommand::Rename { title }
        };
        self.close_window_and_send(command, window, cx);
    }

    fn rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.title.clone();
        self.confirm_title(&title, false, window, cx);
    }

    fn generate_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_generate_title_from_session_history() {
            self.confirm_generate_from_session_history(window, cx);
            return;
        }
        let title = self.title.clone();
        self.confirm_title(&title, true, window, cx);
    }

    /// Enter in the name field: generate from history when the field is
    /// empty and that is allowed, otherwise rename, or summarize long text.
    fn submit_from_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_generate_title_from_session_history() && self.trimmed_title().is_empty() {
            self.confirm_generate_from_session_history(window, cx);
            return;
        }
        let title = self.title.clone();
        let should_generate = self.can_generate_title();
        self.confirm_title(&title, should_generate, window, cx);
    }

    fn toggle_agent_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agents.is_empty() {
            return;
        }
        // The React trigger is a button that takes focus from the textarea;
        // the frame plays that role here so the menu's keys reach `on_key_down`.
        self.focus_handle.focus(window, cx);
        let selected = self.selected_agent_index();
        self.agent_select.toggle(selected);
        cx.notify();
    }

    fn choose_agent(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(agent) = self.agents.get(index) {
            self.selected_agent_id = Some(agent.agent_id.clone());
            if let Some(path) = &self.prefs_path {
                persist_rename_session_modal_prefs(
                    path,
                    &RenameSessionModalPrefs {
                        agent_id: agent.agent_id.clone(),
                        default_prompt_agent_id: self.default_prompt_agent_id.clone(),
                    },
                );
            }
        }
        self.agent_select.close();
        cx.notify();
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
        self.submit_from_editor(window, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.agent_select.open {
            self.agent_select.close();
            cx.notify();
            return;
        }
        self.cancel(window, cx);
    }

    /// Keys while the frame holds focus (after the Generate-with trigger was used).
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        match self.agent_select.handle_key(key, self.agents.len()) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_agent(index, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        match key {
            "escape" => self.cancel(window, cx),
            // The focused React trigger opens its menu on Enter.
            "enter" if !event.is_held => self.toggle_agent_menu(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// `[data-slot='field-description']`: 14px muted copy at line-height 1.5.
    fn render_field_description(&self, text: String) -> AnyElement {
        div()
            .text_size(px(14.0))
            .line_height(px(21.0))
            .text_color(hsla(self.palette.muted))
            .child(text)
            .into_any_element()
    }

    fn render_name_field(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let count = format!(
            "{} / {} characters",
            js_length(self.trimmed_title()),
            GENERATE_NAME_THRESHOLD
        );
        v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(12.0))
            // `.session-rename-modal-shadcn [data-slot='textarea']`: 14px at line-height 1.45.
            .line_height(px(20.3))
            .child(modal_section_title(&p, FIELD_SESSION_NAME))
            .child(modal_text_area(&p, &self.input, None, false, window, cx))
            .child(self.render_field_description(count))
            .into_any_element()
    }

    fn render_agent_field(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(modal_section_title(&p, FIELD_GENERATE_WITH))
            .child(
                h_flex()
                    .w_full()
                    .on_children_prepainted(capture_child_bounds(
                        self.agent_select.trigger_bounds.clone(),
                        0,
                    ))
                    .child(modal_select_trigger(
                        &p,
                        &self.agent_select,
                        "rename-session-agent-select",
                        self.selected_agent_name(),
                        SELECT_AGENT_PLACEHOLDER,
                        false,
                        |this, window, cx| this.toggle_agent_menu(window, cx),
                        cx,
                    )),
            )
            .into_any_element()
    }

    /// `.session-rename-field-group`: the fields 16px apart, the first one
    /// taking the height between the header and the footer.
    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(16.0))
            .child(self.render_name_field(window, cx))
            .when(!self.agents.is_empty(), |this| {
                this.child(self.render_agent_field(cx))
            })
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let rename = modal_action_button(
            &p,
            "rename-session-rename",
            RENAME,
            None,
            ModalButtonTone::Neutral,
            normalize_session_rename_title(&self.title).is_none(),
            |this, window, cx| this.rename(window, cx),
            cx,
        );
        let generate = modal_action_button(
            &p,
            "rename-session-generate",
            GENERATE_NAME,
            None,
            ModalButtonTone::Neutral,
            !self.can_generate_title() && !self.can_generate_title_from_session_history(),
            |this, window, cx| this.generate_name(window, cx),
            cx,
        );
        modal_footer(vec![generate, rename])
    }
}

impl Render for GpuiRenameSessionModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let names: Vec<String> = self.agents.iter().map(|agent| agent.name.clone()).collect();
        let menu = modal_select_menu(
            &p,
            &self.agent_select,
            "rename-session-agent-menu",
            &names,
            self.selected_agent_index(),
            |this, index, _window, cx| this.choose_agent(index, cx),
            |this, _window, cx| {
                this.agent_select.close();
                cx.notify();
            },
            window,
            cx,
        );
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
            menu,
            cx,
        )
        .capture_action(cx.listener(Self::on_enter_action))
        .capture_action(cx.listener(Self::on_escape_action))
    }
}
