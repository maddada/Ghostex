//! Native GPUI Handoff / Export dialog, the desktop twin of the React
//! `ExportTranscriptModal` in packages/core-ui/export-transcript-result-modal.tsx.
//!
//! CDXC:TranscriptExport 2026-09-15 DECISION:
//! User: the React app modals do not fill their GPUI child window and need a hand-tuned window height, so they are being rebuilt in GPUI one at a time, starting with Handoff / Export. The native dialog must match the React one 1 to 1: the same layout, copy, colors, states and behaviour in both appearances. It measures its own first layout and sizes the window to it instead of trusting a constant.
//! SEE-ALSO: packages/core-ui/export-transcript-result-modal.tsx and packages/core-ui/styles/modals.css (the React twin and the `.export-transcript-*` rules mirrored below), apps/desktop/src/app/window/native_modal_kit.rs (shared chrome and controls), apps/desktop/src/app/export_transcript_modal_lifecycle.rs (open, close, sidebar bridge), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, StatefulInteractiveElement as _,
    Styled as _, Window, div, px,
};
use gpui_component::tooltip::Tooltip;
use gpui_component::{h_flex, v_flex};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// The React dialog opens on the Rename Session width (`APP_MODAL_HOST_EXPORT_TRANSCRIPT_RESULT_WINDOW_WIDTH`).
pub(crate) const EXPORT_TRANSCRIPT_MODAL_WIDTH: f32 = 570.0;
/// First-frame height only. The window is resized to the measured layout as soon as the first prepaint reports it.
pub(crate) const EXPORT_TRANSCRIPT_MODAL_INITIAL_HEIGHT: f32 = 520.0;

const ICON_USER_SHARE: &str = "modals/export-transcript/user-share.svg";
const ICON_MARKDOWN: &str = "modals/export-transcript/markdown.svg";
const ICON_CHECK_CARD: &str = "modals/export-transcript/check-card.svg";
const ICON_CHECK_BUTTON: &str = "modals/export-transcript/check-button.svg";
const ICON_COPY: &str = "modals/export-transcript/copy.svg";
const ICON_FOLDER_SEARCH: &str = "modals/export-transcript/folder-search.svg";
const ICON_CIRCLE_CHECK: &str = "modals/export-transcript/circle-check-filled.svg";

const TITLE: &str = "Handoff / Export";
const DESCRIPTION: &str = "Ghostex writes this conversation to a Markdown file. Pick what to do with it and what to include.";
const HANDOFF_CARD_TITLE: &str = "Handoff to an agent";
const HANDOFF_CARD_DESCRIPTION: &str = "Start a new conversation with the handover attached.";
const EXPORT_CARD_TITLE: &str = "Export to Markdown";
const EXPORT_CARD_DESCRIPTION: &str = "Save the conversation as a file and copy its path.";
const CONTINUE_WITH: &str = "Continue with";
const SELECT_AGENT_PLACEHOLDER: &str = "Select agent";
const EXPORT_HINT: &str = "The file is saved in the Ghostex exports folder.";
const INCLUDE: &str = "Include";
const SAVED_AS_MARKDOWN: &str = "Saved as Markdown";
const OPEN_FILE_LOCATION: &str = "Open Location";
const EXPORT_FAILED: &str = "The transcript export failed.";

/// What the user wants to do with the written file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportTranscriptMode {
    Handoff,
    Export,
}

/// The include-toggles; user and agent messages are never optional, so only
/// the three optional record families are here. Defaults mirror the daemon's
/// historical selection: commands and patches in, reasoning out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptIncludeOptions {
    pub(crate) commands: bool,
    pub(crate) patches: bool,
    pub(crate) reasoning: bool,
}

impl Default for ExportTranscriptIncludeOptions {
    fn default() -> Self {
        Self {
            commands: true,
            patches: true,
            reasoning: false,
        }
    }
}

/// The user's last mode and include combination. A per-client UI preference,
/// so repeat exports reopen exactly as they were left without involving gxserver.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ExportTranscriptModalPrefs {
    pub(crate) mode: Option<ExportTranscriptMode>,
    pub(crate) include: ExportTranscriptIncludeOptions,
}

pub(crate) fn load_export_transcript_modal_prefs(path: &Path) -> ExportTranscriptModalPrefs {
    let Some(value) = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return ExportTranscriptModalPrefs::default();
    };
    let defaults = ExportTranscriptIncludeOptions::default();
    let flag = |key: &str, default: bool| {
        value
            .get(key)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(default)
    };
    ExportTranscriptModalPrefs {
        mode: match value.get("mode").and_then(serde_json::Value::as_str) {
            Some("export") => Some(ExportTranscriptMode::Export),
            Some("handoff") => Some(ExportTranscriptMode::Handoff),
            _ => None,
        },
        include: ExportTranscriptIncludeOptions {
            commands: flag("includeCommands", defaults.commands),
            patches: flag("includePatches", defaults.patches),
            reasoning: flag("includeReasoning", defaults.reasoning),
        },
    }
}

pub(crate) fn persist_export_transcript_modal_prefs(
    path: &Path,
    prefs: ExportTranscriptModalPrefs,
) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({
        "mode": match prefs.mode {
            Some(ExportTranscriptMode::Export) => "export",
            _ => "handoff",
        },
        "includeCommands": prefs.include.commands,
        "includePatches": prefs.include.patches,
        "includeReasoning": prefs.include.reasoning,
    });
    let _ = std::fs::write(path, payload.to_string());
}

/// A configured agent that can take the handoff (the React modal only offers
/// agents whose HUD button carries a launch command).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
}

/// The dialog's lifecycle, owned by the host: choose what to include, watch
/// the daemon write the file, then follow up on the result. `Failed` keeps the
/// dialog open with the daemon's message and a way back to the export.
#[derive(Clone, Debug)]
pub(crate) enum ExportTranscriptStage {
    Options,
    Exporting,
    Done {
        agent_id: Option<String>,
        can_reveal: bool,
        path: String,
    },
    Failed {
        message: String,
    },
}

/// What the dialog asks its host to do. The dialog removes its own window
/// before sending any command other than `RunExport`.
pub(crate) enum ExportTranscriptModalCommand {
    RunExport(ExportTranscriptIncludeOptions),
    StartConversation {
        agent_id: String,
    },
    Cancel,
    Reveal,
    /// The exported file's path was just written to the clipboard; the host gives the copy feedback.
    PathCopied,
}

pub(crate) type ExportTranscriptModalHost = Rc<dyn Fn(ExportTranscriptModalCommand, &mut App)>;

pub(crate) struct ExportTranscriptModalConfig {
    pub(crate) agents: Vec<ExportTranscriptAgent>,
    /// The exported session's own agent, preselected so "handoff to the same agent" is one click away.
    pub(crate) default_agent_id: Option<String>,
    /// The agent the chat model picker is handing over to. It fills the user's own selection, which the finished export's agent cannot outrank the way it outranks `default_agent_id`.
    pub(crate) target_agent_id: Option<String>,
    pub(crate) palette: ModalPalette,
    pub(crate) prefs_path: Option<PathBuf>,
    /// Overrides the remembered mode on open; the demo uses it to show one branch.
    pub(crate) initial_mode: Option<ExportTranscriptMode>,
}

pub(crate) struct GpuiExportTranscriptModalWindow {
    host: ExportTranscriptModalHost,
    palette: ModalPalette,
    prefs_path: Option<PathBuf>,
    agents: Vec<ExportTranscriptAgent>,
    default_agent_id: Option<String>,
    mode: ExportTranscriptMode,
    include: ExportTranscriptIncludeOptions,
    selected_agent_id: Option<String>,
    stage: ExportTranscriptStage,
    /// Set by a Handoff run so the done stage starts the conversation instead of showing the path.
    handoff_requested: bool,
    copied: bool,
    copied_generation: u64,
    agent_select: ModalSelect,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiExportTranscriptModalWindow {
    pub(crate) fn new(
        config: ExportTranscriptModalConfig,
        host: ExportTranscriptModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let prefs = config
            .prefs_path
            .as_deref()
            .map(load_export_transcript_modal_prefs)
            .unwrap_or_default();
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette: config.palette,
            prefs_path: config.prefs_path,
            agents: config.agents,
            default_agent_id: config.default_agent_id,
            mode: config
                .initial_mode
                .or(prefs.mode)
                .unwrap_or(ExportTranscriptMode::Handoff),
            include: prefs.include,
            selected_agent_id: config.target_agent_id,
            stage: ExportTranscriptStage::Options,
            handoff_requested: false,
            copied: false,
            copied_generation: 0,
            agent_select: ModalSelect::new(),
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    /// A fresh agent list from the host (the HUD read that finishes after open).
    pub(crate) fn set_agents(
        &mut self,
        agents: Vec<ExportTranscriptAgent>,
        cx: &mut Context<Self>,
    ) {
        if self.agents == agents {
            return;
        }
        self.agents = agents;
        if self.agents.is_empty() {
            self.agent_select.close();
        }
        cx.notify();
    }

    /// The store's answer to `RunExport` (gx_store/git/export_transcript.rs).
    pub(crate) fn receive_result(
        &mut self,
        ok: bool,
        path: Option<String>,
        can_reveal: bool,
        agent_id: Option<String>,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let handoff_requested = std::mem::take(&mut self.handoff_requested);
        match (ok, path) {
            (true, Some(path)) => {
                self.stage = ExportTranscriptStage::Done {
                    agent_id,
                    can_reveal,
                    path,
                };
                if handoff_requested && self.effective_mode() == ExportTranscriptMode::Handoff {
                    if let Some(agent) = self.effective_agent() {
                        let agent_id = agent.agent_id.clone();
                        self.close_window_and_send(
                            ExportTranscriptModalCommand::StartConversation { agent_id },
                            window,
                            cx,
                        );
                        return;
                    }
                }
            }
            _ => {
                self.stage = ExportTranscriptStage::Failed {
                    message: error
                        .map(|message| message.trim().to_string())
                        .filter(|message| !message.is_empty())
                        .unwrap_or_else(|| EXPORT_FAILED.to_string()),
                };
            }
        }
        cx.notify();
    }

    fn effective_mode(&self) -> ExportTranscriptMode {
        if self.agents.is_empty() {
            ExportTranscriptMode::Export
        } else {
            self.mode
        }
    }

    fn effective_agent(&self) -> Option<&ExportTranscriptAgent> {
        let done_agent_id = match &self.stage {
            ExportTranscriptStage::Done { agent_id, .. } => agent_id.as_deref(),
            _ => None,
        };
        self.selected_agent_id
            .as_deref()
            .and_then(|id| self.agents.iter().find(|agent| agent.agent_id == id))
            .or_else(|| {
                done_agent_id
                    .or(self.default_agent_id.as_deref())
                    .and_then(|id| self.agents.iter().find(|agent| agent.agent_id == id))
            })
            .or_else(|| self.agents.first())
    }

    fn effective_agent_index(&self) -> Option<usize> {
        self.effective_agent()
            .and_then(|agent| self.agents.iter().position(|candidate| candidate == agent))
    }

    fn is_done(&self) -> bool {
        matches!(self.stage, ExportTranscriptStage::Done { .. })
    }

    fn is_exporting(&self) -> bool {
        matches!(self.stage, ExportTranscriptStage::Exporting)
    }

    fn show_result(&self) -> bool {
        self.is_done() && self.effective_mode() == ExportTranscriptMode::Export
    }

    fn busy(&self) -> bool {
        self.is_exporting()
            || (self.is_done() && self.effective_mode() == ExportTranscriptMode::Handoff)
    }

    fn controls_disabled(&self) -> bool {
        self.busy() || self.is_done()
    }

    fn can_run(&self) -> bool {
        !self.is_done()
            && !self.is_exporting()
            && (self.effective_mode() == ExportTranscriptMode::Export
                || self.effective_agent().is_some())
    }

    fn primary_label(&self) -> String {
        if self.is_exporting() {
            return "Exporting…".to_string();
        }
        if self.is_done() && self.effective_mode() == ExportTranscriptMode::Handoff {
            return "Starting…".to_string();
        }
        if matches!(self.stage, ExportTranscriptStage::Failed { .. }) {
            return "Try Again".to_string();
        }
        match self.effective_mode() {
            ExportTranscriptMode::Handoff => match self.effective_agent() {
                Some(agent) => format!("Handoff to {}", agent.name),
                None => "Handoff".to_string(),
            },
            ExportTranscriptMode::Export => "Export".to_string(),
        }
    }

    fn persist_prefs(&self) {
        if let Some(path) = &self.prefs_path {
            persist_export_transcript_modal_prefs(
                path,
                ExportTranscriptModalPrefs {
                    mode: Some(self.mode),
                    include: self.include,
                },
            );
        }
    }

    fn choose_mode(&mut self, mode: ExportTranscriptMode, cx: &mut Context<Self>) {
        if self.controls_disabled() {
            return;
        }
        self.mode = mode;
        self.agent_select.close();
        self.persist_prefs();
        cx.notify();
    }

    fn toggle_include(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.controls_disabled() {
            return;
        }
        match index {
            0 => self.include.commands = !self.include.commands,
            1 => self.include.patches = !self.include.patches,
            _ => self.include.reasoning = !self.include.reasoning,
        }
        self.persist_prefs();
        cx.notify();
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        if !self.can_run() {
            return;
        }
        self.agent_select.close();
        self.handoff_requested = self.effective_mode() == ExportTranscriptMode::Handoff;
        self.stage = ExportTranscriptStage::Exporting;
        cx.notify();
        (self.host)(ExportTranscriptModalCommand::RunExport(self.include), cx);
    }

    fn copy_path(&mut self, cx: &mut Context<Self>) {
        let ExportTranscriptStage::Done { path, .. } = &self.stage else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.clone()));
        (self.host)(ExportTranscriptModalCommand::PathCopied, cx);
        self.copied = true;
        self.copied_generation = self.copied_generation.wrapping_add(1);
        let generation = self.copied_generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1_500))
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

    fn close_window_and_send(
        &mut self,
        command: ExportTranscriptModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(ExportTranscriptModalCommand::Cancel, window, cx);
    }

    fn reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(ExportTranscriptModalCommand::Reveal, window, cx);
    }

    fn toggle_agent_menu(&mut self, cx: &mut Context<Self>) {
        if self.controls_disabled() || self.agents.is_empty() {
            return;
        }
        let selected = self.effective_agent_index();
        self.agent_select.toggle(selected);
        cx.notify();
    }

    fn choose_agent(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(agent) = self.agents.get(index) {
            self.selected_agent_id = Some(agent.agent_id.clone());
        }
        self.agent_select.close();
        cx.notify();
    }

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
            "enter" => {
                if event.is_held {
                    return;
                }
                if self.show_result() {
                    self.copy_path(cx);
                } else {
                    self.run(cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_mode_card(
        &self,
        index: usize,
        mode: ExportTranscriptMode,
        icon_path: &'static str,
        title: &'static str,
        description: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let selected = self.effective_mode() == mode;
        let disabled = self.controls_disabled();
        h_flex()
            .id(("export-transcript-mode-card", index))
            .relative()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .items_start()
            .gap(px(10.0))
            .pt(px(10.0))
            .pr(px(12.0))
            .pb(px(11.0))
            .pl(px(10.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(if selected {
                p.card_selected_border()
            } else {
                p.hairline
            }))
            .bg(hsla(if selected {
                p.card_selected_background()
            } else {
                p.raised
            }))
            .when(disabled, |this| this.opacity(0.6).cursor_default())
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(hsla(p.raised_hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.choose_mode(mode, cx);
                    }))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(28.0))
                    .rounded(px(8.0))
                    .bg(hsla(if selected {
                        p.primary
                    } else {
                        p.chip_background()
                    }))
                    .child(modal_icon(
                        icon_path,
                        16.0,
                        if selected {
                            p.primary_foreground
                        } else {
                            p.muted
                        },
                    )),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .pr(px(16.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(16.9))
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(17.4))
                            .text_color(hsla(p.muted))
                            .child(description),
                    ),
            )
            .when(selected, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(10.0))
                        .right(px(10.0))
                        .child(modal_icon(ICON_CHECK_CARD, 14.0, p.foreground)),
                )
            })
            .into_any_element()
    }

    fn render_mode_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        // CSS grid stretches both cards to the taller one; gpui-component's row helper centers items.
        let mut grid = h_flex().w_full().items_stretch().gap(px(8.0));
        if !self.agents.is_empty() {
            grid = grid.child(self.render_mode_card(
                0,
                ExportTranscriptMode::Handoff,
                ICON_USER_SHARE,
                HANDOFF_CARD_TITLE,
                HANDOFF_CARD_DESCRIPTION,
                cx,
            ));
        }
        grid.child(self.render_mode_card(
            1,
            ExportTranscriptMode::Export,
            ICON_MARKDOWN,
            EXPORT_CARD_TITLE,
            EXPORT_CARD_DESCRIPTION,
            cx,
        ))
        .into_any_element()
    }

    /// Both branches share one height so the fitted window never clips when the mode changes.
    fn render_agent_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        if self.effective_mode() == ExportTranscriptMode::Export {
            return h_flex()
                .min_h(px(MODAL_CONTROL_HEIGHT))
                .items_center()
                .child(modal_hint(&p, EXPORT_HINT))
                .into_any_element();
        }
        let value = self.effective_agent().map(|agent| agent.name.clone());
        h_flex()
            .min_h(px(MODAL_CONTROL_HEIGHT))
            .items_center()
            .gap(px(10.0))
            .on_children_prepainted(capture_child_bounds(
                self.agent_select.trigger_bounds.clone(),
                1,
            ))
            .child(modal_section_title(&p, CONTINUE_WITH).flex_shrink_0())
            .child(modal_select_trigger(
                &p,
                &self.agent_select,
                "export-transcript-agent-select",
                value,
                SELECT_AGENT_PLACEHOLDER,
                self.controls_disabled(),
                |this, _window, cx| this.toggle_agent_menu(cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_include(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.palette;
        let disabled = self.controls_disabled();
        const ROWS: [(&str, &str); 3] = [
            (
                "Commands & output",
                "Terminal commands the agent ran, with the tail of their output.",
            ),
            ("File changes", "The patches the agent applied to files."),
            ("Reasoning", "The agent's own reasoning sections."),
        ];
        let values = [
            self.include.commands,
            self.include.patches,
            self.include.reasoning,
        ];
        let mut elements = vec![
            modal_section_title(&p, INCLUDE)
                .mt(px(4.0))
                .into_any_element(),
            modal_panel(&p)
                .children(
                    ROWS.iter()
                        .enumerate()
                        .map(|(index, (label, description))| {
                            modal_panel_row(
                                &p,
                                *label,
                                Some(*description),
                                modal_switch(&p, values[index], disabled),
                                index > 0,
                            )
                            .id(("export-transcript-toggle-row", index))
                            .when(!disabled, |this| {
                                this.cursor_pointer().on_click(cx.listener(
                                    move |this, _: &ClickEvent, _window, cx| {
                                        this.toggle_include(index, cx);
                                    },
                                ))
                            })
                        }),
                )
                .into_any_element(),
        ];
        if let ExportTranscriptStage::Failed { message } = &self.stage {
            elements.push(modal_error(&p, message.clone()));
        }
        elements
    }

    fn render_result(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let (path, can_reveal) = match &self.stage {
            ExportTranscriptStage::Done {
                path, can_reveal, ..
            } => (path.clone(), *can_reveal),
            _ => (String::new(), false),
        };
        modal_panel(&p)
            .mt(px(4.0))
            .gap(px(10.0))
            .p(px(12.0))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(13.0))
                    .line_height(px(18.57))
                    .font_weight(FontWeight::MEDIUM)
                    .child(modal_icon(ICON_CIRCLE_CHECK, 16.0, p.success))
                    .child(SAVED_AS_MARKDOWN),
            )
            .child(
                modal_raised_box(&p)
                    .flex()
                    .flex_row()
                    .w_full()
                    .items_center()
                    .gap(px(6.0))
                    .pt(px(8.0))
                    .pr(px(8.0))
                    .pb(px(8.0))
                    .pl(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(MODAL_MONO_FONT)
                            .text_size(px(12.0))
                            .line_height(px(17.4))
                            .child(path),
                    )
                    .when(can_reveal, |this| {
                        this.child(
                            modal_icon_button(
                                &p,
                                "export-transcript-reveal",
                                ICON_FOLDER_SEARCH,
                                15.0,
                                |this, window, cx| this.reveal(window, cx),
                                cx,
                            )
                            .tooltip(|window, cx| {
                                Tooltip::new(OPEN_FILE_LOCATION).build(window, cx)
                            }),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = v_flex()
            .w_full()
            .gap(px(10.0))
            .child(self.render_mode_grid(cx))
            .child(self.render_agent_row(cx));
        if self.show_result() {
            body = body.child(self.render_result(cx));
        } else {
            body = body.children(self.render_include(cx));
        }
        body.into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let show_result = self.show_result();
        let left = modal_action_button(
            &p,
            "export-transcript-cancel",
            if show_result { "Done" } else { "Cancel" },
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let right = if show_result {
            let copied = self.copied;
            modal_action_button(
                &p,
                "export-transcript-copy",
                if copied { "Path Copied" } else { "Copy Path" },
                Some(
                    modal_icon(
                        if copied { ICON_CHECK_BUTTON } else { ICON_COPY },
                        15.0,
                        p.primary_foreground,
                    )
                    .into_any_element(),
                ),
                ModalButtonTone::Primary,
                false,
                |this, _window, cx| this.copy_path(cx),
                cx,
            )
        } else {
            modal_action_button(
                &p,
                "export-transcript-primary",
                self.primary_label(),
                self.busy().then(|| modal_spinner(p.primary_foreground)),
                ModalButtonTone::Primary,
                !self.can_run(),
                |this, _window, cx| this.run(cx),
                cx,
            )
        };
        modal_footer(vec![left, right])
    }
}

impl Render for GpuiExportTranscriptModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let names: Vec<String> = self.agents.iter().map(|agent| agent.name.clone()).collect();
        let menu = modal_select_menu(
            &p,
            &self.agent_select,
            "export-transcript-agent-menu",
            &names,
            self.effective_agent_index(),
            |this, index, _window, cx| this.choose_agent(index, cx),
            |this, _window, cx| {
                this.agent_select.close();
                cx.notify();
            },
            window,
            cx,
        );
        let content = vec![
            modal_header(&p, TITLE, Some(DESCRIPTION)),
            self.render_body(cx),
        ];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-export-transcript-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            menu,
            cx,
        )
    }
}
