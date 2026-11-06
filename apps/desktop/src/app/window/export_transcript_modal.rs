//! Native GPUI Handoff / Export dialog, the desktop twin of the React
//! `ExportTranscriptModal` in packages/core-ui/export-transcript-result-modal.tsx (deleted 2026-10-01).
//!
//! CDXC:TranscriptExport 2026-09-15 DECISION:
//! User: the React app modals do not fill their GPUI child window and need a hand-tuned window height, so they are being rebuilt in GPUI one at a time, starting with Handoff / Export. The native dialog must match the React one 1 to 1: the same layout, copy, colors, states and behaviour in both appearances. It measures its own first layout and sizes the window to it instead of trusting a constant.
//! SEE-ALSO: packages/core-ui/export-transcript-result-modal.tsx (deleted 2026-10-01) and packages/core-ui/styles/modals.css (deleted 2026-11-06) (the React twin and the `.export-transcript-*` rules mirrored below), apps/desktop/src/app/window/native_modal_kit/ (shared chrome and controls), apps/desktop/src/app/export_transcript_modal_lifecycle.rs (open, close, sidebar bridge), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
use super::native_modal_kit::*;
use ghostex_gx_core::handoff_prefs::{
    HANDOFF_HISTORY_STOPS, HandoffCard, HandoffDestination, HandoffHistory, HandoffInclude,
    HandoffPrefs, handoff_history_estimate, handoff_history_label, handoff_history_stop_index,
    handoff_include_summary, handoff_prefs_json, parse_handoff_prefs,
    remembered_handoff_destination,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement as _,
    Pixels, Render, StatefulInteractiveElement as _, Styled as _, Window, div, px, relative,
};
use gpui_component::tooltip::Tooltip;
use gpui_component::{h_flex, v_flex};
use std::cell::Cell;
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
const ICON_CHEVRON_RIGHT: &str = "titlebar/chevron-right.svg";

const TITLE: &str = "Handoff / Export";
const DESCRIPTION: &str = "Ghostex writes this conversation to a Markdown file. Pick what to do with it and what to include.";
const HANDOFF_CARD_TITLE: &str = "Handoff to an agent";
const HANDOFF_CARD_DESCRIPTION: &str = "Start a new conversation with the handover attached.";
const EXPORT_CARD_TITLE: &str = "Handoff to MD";
const EXPORT_CARD_DESCRIPTION: &str = "Save the conversation as a file and copy its path.";
const CONTINUE_WITH: &str = "Continue with";
const SELECT_AGENT_PLACEHOLDER: &str = "Select agent";
const DESTINATION: &str = "Destination";
const DESTINATION_HERE: &str = "This project, same computer";
const DESTINATION_PICK_PROJECT: &str = "Pick a project";
const DESTINATION_CHANGE: &str = "Change…";
const DESTINATION_RESET: &str = "Use this project";
const DESTINATION_HIDE: &str = "Hide choices";
/// One width for the handoff rows' labels, so the selects line up.
const ROW_LABEL_WIDTH: f32 = 96.0;
const COMPUTER: &str = "Computer";
const SELECT_COMPUTER_PLACEHOLDER: &str = "Select computer";
const PROJECT: &str = "Project";
const SELECT_PROJECT_PLACEHOLDER: &str = "Select project";
const NO_COMPUTER_HINT: &str =
    "No computer is connected. Connect one from the sidebar's computer tabs to hand off to it.";
const BRANCH_NOTICE_TITLE: &str = "Please note the difference in branches";
const BRANCH_NOTICE_SUBTITLE: &str =
    "We're just handing off the session, not switching the branch there";
const EXPORT_HINT: &str = "The file is saved in the Ghostex exports folder.";
const INCLUDE: &str = "Include";
const INCLUDE_NOTHING: &str = "Messages only";
const INCLUDE_ROWS: [(&str, &str); 3] = [
    (
        "Commands & output",
        "Terminal commands the agent ran, with the tail of their output.",
    ),
    ("File changes", "The patches the agent applied to files."),
    ("Reasoning", "The agent's own reasoning sections."),
];
const HISTORY: &str = "History";
const WHOLE_CONVERSATION: &str = "Whole conversation";
const WHOLE_CONVERSATION_DESCRIPTION: &str =
    "Off, the file keeps the last turns and lists what you asked before them.";
const SAVED_AS_MARKDOWN: &str = "Saved as Markdown";
const OPEN_FILE_LOCATION: &str = "Open Location";
const EXPORT_FAILED: &str = "The transcript export failed.";

/// What the user wants to do with the written file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExportTranscriptMode {
    #[default]
    Handoff,
    Export,
}

impl ExportTranscriptMode {
    fn is_handoff(self) -> bool {
        self == Self::Handoff
    }
}

/// The include-toggles; user and agent messages are never optional, so only the three optional
/// record families are here. All off until the user changes them
/// (`ghostex_gx_core::handoff_prefs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExportTranscriptIncludeOptions {
    pub(crate) commands: bool,
    pub(crate) patches: bool,
    pub(crate) reasoning: bool,
}

/// What the dialog remembers between opens. A per-client UI preference, so repeat exports reopen
/// exactly as they were left without involving gxserver.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExportTranscriptModalPrefs {
    pub(crate) mode: ExportTranscriptMode,
    pub(crate) include: ExportTranscriptIncludeOptions,
    pub(crate) include_open: bool,
    pub(crate) agent_id: Option<String>,
    pub(crate) destination: Option<ExportTranscriptRemoteTarget>,
    pub(crate) destination_open: bool,
    pub(crate) history: HandoffHistory,
}

impl ExportTranscriptModalPrefs {
    fn from_saved(value: &serde_json::Value) -> Self {
        let saved = parse_handoff_prefs(value);
        Self {
            mode: match saved.card {
                HandoffCard::Agent => ExportTranscriptMode::Handoff,
                HandoffCard::Markdown => ExportTranscriptMode::Export,
            },
            include: ExportTranscriptIncludeOptions {
                commands: saved.include.commands,
                patches: saved.include.patches,
                reasoning: saved.include.reasoning,
            },
            include_open: saved.include_open,
            agent_id: saved.agent_id,
            destination: saved
                .destination
                .map(|destination| ExportTranscriptRemoteTarget {
                    machine_id: destination.machine_id,
                    project_id: destination.project_id,
                }),
            destination_open: saved.destination_open,
            history: saved.history,
        }
    }

    fn to_saved(&self) -> serde_json::Value {
        handoff_prefs_json(&HandoffPrefs {
            card: match self.mode {
                ExportTranscriptMode::Handoff => HandoffCard::Agent,
                ExportTranscriptMode::Export => HandoffCard::Markdown,
            },
            include: HandoffInclude {
                commands: self.include.commands,
                patches: self.include.patches,
                reasoning: self.include.reasoning,
            },
            include_open: self.include_open,
            agent_id: self.agent_id.clone(),
            destination: self
                .destination
                .as_ref()
                .map(|destination| HandoffDestination {
                    machine_id: destination.machine_id.clone(),
                    project_id: destination.project_id.clone(),
                }),
            destination_open: self.destination_open,
            history: self.history,
        })
    }
}

pub(crate) fn load_export_transcript_modal_prefs(path: &Path) -> ExportTranscriptModalPrefs {
    let value = read_saved_prefs(path)
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .unwrap_or(serde_json::Value::Null);
    ExportTranscriptModalPrefs::from_saved(&value)
}

pub(crate) fn persist_export_transcript_modal_prefs(
    path: &Path,
    prefs: &ExportTranscriptModalPrefs,
) {
    write_saved_prefs(path, &prefs.to_saved().to_string());
}

#[cfg(not(target_arch = "wasm32"))]
fn read_saved_prefs(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn write_saved_prefs(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, text);
}

/// A page has no files of its own, so its dialog remembers in the browser's storage.
#[cfg(target_arch = "wasm32")]
fn read_saved_prefs(path: &Path) -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(&path.to_string_lossy())
        .ok()?
}

#[cfg(target_arch = "wasm32")]
fn write_saved_prefs(path: &Path, text: &str) {
    if let Some(Ok(Some(storage))) = web_sys::window().map(|window| window.local_storage()) {
        let _ = storage.set_item(&path.to_string_lossy(), text);
    }
}

/// A configured agent that can take the handoff (the React modal only offers
/// agents whose HUD button carries a launch command).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
    /// The chat model picker's family for this agent's logo (`claude`, `codex`, …), when it has one.
    pub(crate) family: Option<String>,
}

/// The chat model picker's pick of another agent's model, which the dialog hands over to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptHandoffTarget {
    pub(crate) family: String,
    pub(crate) model: String,
}

/// A computer a handoff can start the new session on, with its projects ranked for this session
/// (`ghostex_gx_core::handoff_target`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptComputer {
    /// `local` for this computer, else the remote machine id.
    pub(crate) machine_id: String,
    pub(crate) label: String,
    /// The computer the session runs on: its transcript is already there, and the session's own
    /// project comes first among `projects`.
    pub(crate) is_source: bool,
    pub(crate) projects: Vec<ExportTranscriptProject>,
    pub(crate) preferred_project_id: Option<String>,
    /// The repository's name when this session's project has a git remote that no project on
    /// this computer matches: the dialog then offers to clone it there.
    pub(crate) missing_repository: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptProject {
    pub(crate) project_id: String,
    pub(crate) title: String,
    pub(crate) path: Option<String>,
    pub(crate) same_repo: bool,
    /// The project the session is in.
    pub(crate) is_own: bool,
}

/// Where a handoff to somewhere else starts the new session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptRemoteTarget {
    pub(crate) machine_id: String,
    pub(crate) project_id: String,
}

/// The dialog's picks, handed to the host when it closes for a clone and given back when it
/// reopens, so the user returns to the dialog as they left it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportTranscriptPicks {
    pub(crate) mode: ExportTranscriptMode,
    pub(crate) include: ExportTranscriptIncludeOptions,
    pub(crate) agent_id: Option<String>,
    pub(crate) machine_id: Option<String>,
    pub(crate) project_id: Option<String>,
    pub(crate) destination_open: bool,
}

/// The computer a handoff goes to: the picked one while it is still offered, else the
/// first.
pub(crate) fn handoff_computer<'a>(
    computers: &'a [ExportTranscriptComputer],
    selected_machine_id: Option<&str>,
) -> Option<&'a ExportTranscriptComputer> {
    selected_machine_id
        .and_then(|id| computers.iter().find(|computer| computer.machine_id == id))
        .or_else(|| computers.first())
}

/// The project on that computer: the pick when it is one of its projects, else the one with the
/// session's repository, else none (the user must pick one).
pub(crate) fn handoff_project<'a>(
    computer: &'a ExportTranscriptComputer,
    selected_project_id: Option<&str>,
) -> Option<&'a ExportTranscriptProject> {
    let find = |id: &str| {
        computer
            .projects
            .iter()
            .find(|project| project.project_id == id)
    };
    selected_project_id
        .and_then(find)
        .or_else(|| computer.preferred_project_id.as_deref().and_then(find))
}

/// The button of a handoff going somewhere else.
fn destination_label(
    computer: &ExportTranscriptComputer,
    project: &ExportTranscriptProject,
) -> String {
    ghostex_gx_core::handoff_target::handoff_destination_label(
        &project.title,
        &computer.label,
        computer.is_source,
    )
}

/// A Project select row.
fn project_label(project: &ExportTranscriptProject) -> String {
    ghostex_gx_core::handoff_target::handoff_project_label(
        &project.title,
        project.is_own,
        project.same_repo,
        project.path.as_deref(),
    )
}

/// What the dialog opens on: a remembered destination that still applies, else this project here.
fn opening_destination(
    remembered: Option<&ExportTranscriptRemoteTarget>,
    computers: &[ExportTranscriptComputer],
) -> Option<ExportTranscriptRemoteTarget> {
    let own = computers.iter().find(|computer| computer.is_source)?;
    let own = HandoffDestination {
        machine_id: own.machine_id.clone(),
        project_id: own
            .projects
            .iter()
            .find(|project| project.is_own)
            .map(|project| project.project_id.clone())
            .unwrap_or_default(),
    };
    let remembered = remembered.map(|target| HandoffDestination {
        machine_id: target.machine_id.clone(),
        project_id: target.project_id.clone(),
    });
    remembered_handoff_destination(remembered.as_ref(), &own, |destination| {
        computers.iter().any(|computer| {
            computer.machine_id == destination.machine_id
                && computer
                    .projects
                    .iter()
                    .any(|project| project.project_id == destination.project_id)
        })
    })
    .map(|destination| ExportTranscriptRemoteTarget {
        machine_id: destination.machine_id,
        project_id: destination.project_id,
    })
}

/// The agent a model picker handoff goes to: the first agent of the picked family in the list the
/// dialog shows, or none. Never the session's own agent standing in for a family that is missing.
pub(crate) fn handoff_target_agent<'a>(
    agents: &'a [ExportTranscriptAgent],
    family: &str,
) -> Option<&'a ExportTranscriptAgent> {
    agents
        .iter()
        .find(|agent| agent.family.as_deref() == Some(family))
}

/// The family's name as the dialog says it.
fn family_display_name(family: &str) -> &str {
    match family {
        "claude" => "Claude",
        "codex" => "Codex",
        "cursor" => "Cursor",
        "grok" => "Grok",
        "antigravity" => "Antigravity CLI",
        "opencode" => "OpenCode",
        other => other,
    }
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

/// The computer's count for the History hint (`/api/countSessionTranscriptTurns`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExportTranscriptTurnCount {
    Counted {
        turns: usize,
        whole_bytes: usize,
        /// Index `i` is the file's size with the last `i + 1` turns.
        last_turns_bytes: Vec<usize>,
    },
    /// That computer's Ghostex is from before the slider: its export would ignore `lastTurns`,
    /// so the dialog hides History rather than promise a cut it won't make.
    Unsupported,
    Failed,
}

/// What the dialog asks its host to do. The dialog removes its own window
/// before sending any command other than `RunExport`.
pub(crate) enum ExportTranscriptModalCommand {
    /// `carry` asks for the file's text too, which a handoff takes to another computer.
    RunExport {
        include: ExportTranscriptIncludeOptions,
        /// History's last N turns; `None` is the whole conversation.
        last_turns: Option<usize>,
        carry: bool,
    },
    /// Count the conversation's turns under this Include, for History's hint; the host answers
    /// with [`GpuiExportTranscriptModalWindow::set_turn_count`].
    CountTurns(ExportTranscriptIncludeOptions),
    StartConversation {
        agent_id: String,
        /// `None` starts it beside the session (Handoff to an agent).
        target: Option<ExportTranscriptRemoteTarget>,
    },
    /// The Computer or Project select changed: the host reads both checkouts' branches and
    /// answers with [`GpuiExportTranscriptModalWindow::set_branch_notice`].
    TargetChanged(ExportTranscriptRemoteTarget),
    /// Clone the session's repository on `machine_id` (or add a project there when it has no
    /// remote). The dialog has removed its window; the host reopens it with `picks` afterwards.
    AddProjectOn {
        machine_id: String,
        picks: ExportTranscriptPicks,
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
    /// The chat model picker's pick. Its agent is resolved from `agents` (and again whenever they refresh), and it outranks `default_agent_id` and the finished export's agent.
    pub(crate) handoff_target: Option<ExportTranscriptHandoffTarget>,
    pub(crate) palette: ModalPalette,
    pub(crate) prefs_path: Option<PathBuf>,
    /// Overrides the remembered mode on open; the demo uses it to show one branch.
    pub(crate) initial_mode: Option<ExportTranscriptMode>,
    /// The computers a handoff can go to, the session's own first.
    pub(crate) computers: Vec<ExportTranscriptComputer>,
    /// The picks to reopen with after a clone (`AddProjectOn`), over the remembered prefs.
    pub(crate) restore: Option<ExportTranscriptPicks>,
}

pub(crate) struct GpuiExportTranscriptModalWindow {
    host: ExportTranscriptModalHost,
    palette: ModalPalette,
    prefs_path: Option<PathBuf>,
    agents: Vec<ExportTranscriptAgent>,
    default_agent_id: Option<String>,
    handoff_target: Option<ExportTranscriptHandoffTarget>,
    mode: ExportTranscriptMode,
    include: ExportTranscriptIncludeOptions,
    selected_agent_id: Option<String>,
    stage: ExportTranscriptStage,
    /// Set by a Handoff run so the done stage starts the conversation instead of showing the path.
    handoff_requested: bool,
    copied: bool,
    copied_generation: u64,
    agent_select: ModalSelect,
    computers: Vec<ExportTranscriptComputer>,
    selected_machine_id: Option<String>,
    selected_project_id: Option<String>,
    /// The Computer and Project choices are showing. Closed means this project here.
    destination_open: bool,
    /// What was saved at the last handoff: changing a card or a switch keeps these.
    remembered_agent_id: Option<String>,
    remembered_destination: Option<ExportTranscriptRemoteTarget>,
    include_open: bool,
    history: HandoffHistory,
    /// The last count and the Include it was made for; a count for another Include is stale.
    turn_count: Option<(ExportTranscriptIncludeOptions, ExportTranscriptTurnCount)>,
    /// The Include the host was last asked to count, so a repeat asks nothing.
    counted_include: Option<ExportTranscriptIncludeOptions>,
    slider_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    slider_dragging: bool,
    computer_select: ModalSelect,
    project_select: ModalSelect,
    /// The branch notice's first line, for the target it was read for.
    branch_notice: Option<(ExportTranscriptRemoteTarget, String)>,
    /// The target the host was last asked about, so a repeat pick asks nothing.
    announced_target: Option<ExportTranscriptRemoteTarget>,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _click_away: Vec<gpui::Subscription>,
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
        let restore = config.restore;
        // CDXC:TranscriptExport 2026-10-31 DECISION:
        // User: "we should remember what the user picked last time so we keep that selection for next time". A remembered destination opens with its choices showing and the button naming it, because a session of another project would otherwise leave for somewhere it never saw. The chat model picker's pick means "this conversation, on that model", so it opens here on that model's agent, whatever was picked last time.
        let remembered = (config.handoff_target.is_none() && restore.is_none())
            .then(|| opening_destination(prefs.destination.as_ref(), &config.computers))
            .flatten();
        let destination_open = match &restore {
            Some(picks) => picks.destination_open,
            None => {
                remembered.is_some() || (config.handoff_target.is_none() && prefs.destination_open)
            }
        };
        let selected_agent_id = match &restore {
            Some(picks) => picks.agent_id.clone(),
            None => prefs
                .agent_id
                .clone()
                .filter(|_| config.handoff_target.is_none()),
        };
        let mut this = Self {
            host,
            palette: config.palette,
            prefs_path: config.prefs_path,
            agents: config.agents,
            default_agent_id: config.default_agent_id,
            handoff_target: config.handoff_target,
            mode: restore
                .as_ref()
                .map(|picks| picks.mode)
                .or(config.initial_mode)
                .unwrap_or(prefs.mode),
            include: restore
                .as_ref()
                .map(|picks| picks.include)
                .unwrap_or(prefs.include),
            selected_agent_id,
            stage: ExportTranscriptStage::Options,
            handoff_requested: false,
            copied: false,
            copied_generation: 0,
            agent_select: ModalSelect::new(),
            computers: config.computers,
            selected_machine_id: restore
                .as_ref()
                .and_then(|picks| picks.machine_id.clone())
                .or_else(|| remembered.as_ref().map(|target| target.machine_id.clone())),
            selected_project_id: restore
                .as_ref()
                .and_then(|picks| picks.project_id.clone())
                .or_else(|| remembered.as_ref().map(|target| target.project_id.clone())),
            destination_open,
            remembered_agent_id: prefs.agent_id.clone(),
            remembered_destination: prefs.destination.clone(),
            include_open: prefs.include_open,
            history: prefs.history,
            turn_count: None,
            counted_include: None,
            slider_bounds: Rc::new(Cell::new(None)),
            slider_dragging: false,
            computer_select: ModalSelect::new(),
            project_select: ModalSelect::new(),
            branch_notice: None,
            announced_target: None,
            fit: ModalFit::new(),
            focus_handle,
            _click_away: super::popup_dismissal::close_app_modal_on_click_away(window, cx),
        };
        this.announce_target(cx);
        this.ask_turn_count(cx);
        this
    }

    /// Asks the host to count the conversation under the current Include, once per Include.
    fn ask_turn_count(&mut self, cx: &mut Context<Self>) {
        if self.counted_include == Some(self.include) {
            return;
        }
        self.counted_include = Some(self.include);
        (self.host)(ExportTranscriptModalCommand::CountTurns(self.include), cx);
    }

    /// The host's answer to `CountTurns`.
    pub(crate) fn set_turn_count(
        &mut self,
        include: ExportTranscriptIncludeOptions,
        count: ExportTranscriptTurnCount,
        cx: &mut Context<Self>,
    ) {
        if include != self.include && count != ExportTranscriptTurnCount::Unsupported {
            return;
        }
        self.turn_count = Some((include, count));
        self.fit.refit();
        cx.notify();
    }

    /// Whether the computer can cut the file to the last turns.
    fn history_supported(&self) -> bool {
        !matches!(
            self.turn_count,
            Some((_, ExportTranscriptTurnCount::Unsupported))
        )
    }

    /// History's hint, once the computer has counted this Include.
    fn history_estimate(&self) -> Option<String> {
        match &self.turn_count {
            Some((
                include,
                ExportTranscriptTurnCount::Counted {
                    turns,
                    whole_bytes,
                    last_turns_bytes,
                },
            )) if *include == self.include => Some(handoff_history_estimate(
                &self.history,
                *turns,
                *whole_bytes,
                last_turns_bytes,
            )),
            _ => None,
        }
    }

    /// The host's answer to `TargetChanged`: the notice's first line, or `None` when both
    /// checkouts are on one branch (or either is unknown).
    pub(crate) fn set_branch_notice(
        &mut self,
        target: ExportTranscriptRemoteTarget,
        line: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.elsewhere().as_ref() != Some(&target) {
            return;
        }
        self.branch_notice = line.map(|line| (target, line));
        self.fit.refit();
        cx.notify();
    }

    /// The computer the choices point at: the pick, else the first one offered.
    fn effective_computer(&self) -> Option<&ExportTranscriptComputer> {
        handoff_computer(&self.computers, self.selected_machine_id.as_deref())
    }

    fn effective_computer_index(&self) -> Option<usize> {
        let computer = self.effective_computer()?;
        self.computers
            .iter()
            .position(|candidate| candidate.machine_id == computer.machine_id)
    }

    /// The project on that computer: the pick when it is one of its projects, else the one with
    /// this session's repository.
    fn effective_project(&self) -> Option<&ExportTranscriptProject> {
        handoff_project(
            self.effective_computer()?,
            self.selected_project_id.as_deref(),
        )
    }

    fn effective_project_index(&self) -> Option<usize> {
        let computer = self.effective_computer()?;
        let project = self.effective_project()?;
        computer
            .projects
            .iter()
            .position(|candidate| candidate.project_id == project.project_id)
    }

    /// Where the handoff goes when that is not this project here.
    fn elsewhere(&self) -> Option<ExportTranscriptRemoteTarget> {
        if !self.destination_open {
            return None;
        }
        let computer = self.effective_computer()?;
        let project = self.effective_project()?;
        (!(computer.is_source && project.is_own)).then(|| ExportTranscriptRemoteTarget {
            machine_id: computer.machine_id.clone(),
            project_id: project.project_id.clone(),
        })
    }

    fn is_leaving_computer(&self) -> bool {
        self.elsewhere().is_some()
            && self
                .effective_computer()
                .is_some_and(|computer| !computer.is_source)
    }

    /// The destination row's words: always where the session goes.
    fn destination_summary(&self) -> String {
        if !self.destination_open {
            return DESTINATION_HERE.to_string();
        }
        match (self.effective_computer(), self.effective_project()) {
            (Some(computer), Some(project)) if computer.is_source && project.is_own => {
                DESTINATION_HERE.to_string()
            }
            (Some(computer), Some(project)) if computer.is_source => {
                format!("{}, same computer", project.title)
            }
            (Some(computer), Some(project)) => format!("{} on {}", project.title, computer.label),
            _ => DESTINATION_PICK_PROJECT.to_string(),
        }
    }

    /// Tells the host which checkout the handoff now points at, once per change.
    fn announce_target(&mut self, cx: &mut Context<Self>) {
        let target = (self.mode == ExportTranscriptMode::Handoff)
            .then(|| self.elsewhere())
            .flatten();
        if target == self.announced_target {
            return;
        }
        self.announced_target = target.clone();
        if self
            .branch_notice
            .as_ref()
            .is_some_and(|(notice_target, _)| Some(notice_target) != target.as_ref())
        {
            self.branch_notice = None;
        }
        if let Some(target) = target {
            (self.host)(ExportTranscriptModalCommand::TargetChanged(target), cx);
        }
    }

    fn picks(&self) -> ExportTranscriptPicks {
        ExportTranscriptPicks {
            mode: self.mode,
            include: self.include,
            agent_id: self.selected_agent_id.clone(),
            machine_id: self
                .effective_computer()
                .map(|computer| computer.machine_id.clone()),
            project_id: self.selected_project_id.clone(),
            destination_open: self.destination_open,
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
                if handoff_requested
                    && self.effective_mode().is_handoff()
                    && let Some(agent) = self.effective_agent()
                {
                    let agent_id = agent.agent_id.clone();
                    let target = self.elsewhere();
                    self.close_window_and_send(
                        ExportTranscriptModalCommand::StartConversation { agent_id, target },
                        window,
                        cx,
                    );
                    return;
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
        let selected = self
            .selected_agent_id
            .as_deref()
            .and_then(|id| self.agents.iter().find(|agent| agent.agent_id == id));
        // CDXC:TranscriptExport 2026-10-25 WHY:
        // A model picker handoff resolves its agent from the list this dialog shows. Until 2026-10-25 the host matched it against a second agent list and the dialog fell back to the session's own agent when that id was not in its list, so picking GPT 6 Astra in a Claude session said "Handoff to Claude" and started Claude. A family with no agent here leaves the choice empty and says so (`target_unavailable_hint`).
        if let Some(target) = &self.handoff_target {
            return selected.or_else(|| handoff_target_agent(&self.agents, &target.family));
        }
        selected
            .or_else(|| {
                done_agent_id
                    .or(self.default_agent_id.as_deref())
                    .and_then(|id| self.agents.iter().find(|agent| agent.agent_id == id))
            })
            .or_else(|| self.agents.first())
    }

    /// What the dialog says when the picked model's agent is not one it can start.
    fn target_unavailable_hint(&self) -> Option<String> {
        let target = self.handoff_target.as_ref()?;
        if self.agents.is_empty()
            || !self.effective_mode().is_handoff()
            || self.effective_agent().is_some()
        {
            return None;
        }
        let family = family_display_name(&target.family);
        Some(format!(
            "No {family} agent is set up here, so this can't hand off to {}. Pick another agent, or add {family} in Settings > Agents.",
            target.model
        ))
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
        self.is_exporting() || (self.is_done() && self.effective_mode().is_handoff())
    }

    fn controls_disabled(&self) -> bool {
        self.busy() || self.is_done()
    }

    fn can_run(&self) -> bool {
        !self.is_done()
            && !self.is_exporting()
            && match self.effective_mode() {
                ExportTranscriptMode::Export => true,
                ExportTranscriptMode::Handoff => {
                    self.effective_agent().is_some()
                        && (!self.destination_open || self.effective_project().is_some())
                }
            }
    }

    fn primary_label(&self) -> String {
        if self.is_exporting() {
            return "Exporting…".to_string();
        }
        if self.is_done() && self.effective_mode().is_handoff() {
            return "Starting…".to_string();
        }
        if matches!(self.stage, ExportTranscriptStage::Failed { .. }) {
            return "Try Again".to_string();
        }
        match self.effective_mode() {
            ExportTranscriptMode::Handoff => {
                // This project here names the agent; anywhere else names where it goes (Q862).
                if self.elsewhere().is_some()
                    && let (Some(computer), Some(project)) =
                        (self.effective_computer(), self.effective_project())
                {
                    return destination_label(computer, project);
                }
                match self.effective_agent() {
                    Some(agent) if !self.destination_open || self.effective_project().is_some() => {
                        format!("Handoff to {}", agent.name)
                    }
                    _ => "Handoff".to_string(),
                }
            }
            ExportTranscriptMode::Export => "Export".to_string(),
        }
    }

    fn persist_prefs(&self) {
        if let Some(path) = &self.prefs_path {
            persist_export_transcript_modal_prefs(
                path,
                &ExportTranscriptModalPrefs {
                    mode: self.mode,
                    include: self.include,
                    include_open: self.include_open,
                    agent_id: self.remembered_agent_id.clone(),
                    destination: self.remembered_destination.clone(),
                    destination_open: self.remembered_destination.is_some(),
                    history: self.history,
                },
            );
        }
    }

    /// A handoff remembers its agent and destination when it runs, so a pick the user backed out
    /// of doesn't come back. The model picker's handoff is about that one model, not a preference.
    fn remember_handoff(&mut self) {
        if self.effective_mode() != ExportTranscriptMode::Handoff || self.handoff_target.is_some() {
            return;
        }
        self.remembered_agent_id = self.selected_agent_id.clone();
        self.remembered_destination = self.elsewhere();
        self.persist_prefs();
    }

    fn open_destination(&mut self, cx: &mut Context<Self>) {
        if self.controls_disabled() {
            return;
        }
        self.destination_open = true;
        self.fit.refit();
        self.announce_target(cx);
        cx.notify();
    }

    fn use_this_project(&mut self, cx: &mut Context<Self>) {
        if self.controls_disabled() {
            return;
        }
        self.destination_open = false;
        self.selected_machine_id = None;
        self.selected_project_id = None;
        self.close_menus();
        self.fit.refit();
        self.announce_target(cx);
        cx.notify();
    }

    fn toggle_include_open(&mut self, cx: &mut Context<Self>) {
        if self.controls_disabled() {
            return;
        }
        self.include_open = !self.include_open;
        self.persist_prefs();
        self.fit.refit();
        cx.notify();
    }

    fn choose_mode(&mut self, mode: ExportTranscriptMode, cx: &mut Context<Self>) {
        if self.controls_disabled() {
            return;
        }
        self.mode = mode;
        self.close_menus();
        self.persist_prefs();
        self.fit.refit();
        self.announce_target(cx);
        cx.notify();
    }

    fn close_menus(&mut self) {
        self.agent_select.close();
        self.computer_select.close();
        self.project_select.close();
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
        self.ask_turn_count(cx);
        cx.notify();
    }

    fn toggle_whole_conversation(&mut self, cx: &mut Context<Self>) {
        if self.controls_disabled() {
            return;
        }
        self.history.all = !self.history.all;
        self.persist_prefs();
        self.fit.refit();
        cx.notify();
    }

    /// Moves History's slider to the stop under `x`, a window position.
    fn slide_history_to(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(bounds) = self.slider_bounds.get() else {
            return;
        };
        let width = f32::from(bounds.size.width).max(1.0);
        let fraction = (f32::from(x - bounds.origin.x) / width).clamp(0.0, 1.0);
        let last = HANDOFF_HISTORY_STOPS.len() - 1;
        let index = (fraction * last as f32).round() as usize;
        let turns = HANDOFF_HISTORY_STOPS[index.min(last)];
        if turns != self.history.turns {
            self.history.turns = turns;
            cx.notify();
        }
    }

    fn end_history_slide(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.slider_dragging) {
            self.persist_prefs();
            cx.notify();
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        if !self.can_run() {
            return;
        }
        self.close_menus();
        self.remember_handoff();
        self.handoff_requested = self.effective_mode().is_handoff();
        self.stage = ExportTranscriptStage::Exporting;
        cx.notify();
        (self.host)(
            ExportTranscriptModalCommand::RunExport {
                include: self.include,
                last_turns: self
                    .history_supported()
                    .then(|| self.history.last_turns())
                    .flatten(),
                carry: self.effective_mode().is_handoff() && self.is_leaving_computer(),
            },
            cx,
        );
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
        self.computer_select.close();
        self.project_select.close();
        self.agent_select.toggle(selected);
        cx.notify();
    }

    fn toggle_computer_menu(&mut self, cx: &mut Context<Self>) {
        if self.controls_disabled() || self.computers.is_empty() {
            return;
        }
        let selected = self.effective_computer_index();
        self.agent_select.close();
        self.project_select.close();
        self.computer_select.toggle(selected);
        cx.notify();
    }

    fn choose_computer(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(computer) = self.computers.get(index) {
            if self.selected_machine_id.as_deref() != Some(computer.machine_id.as_str()) {
                self.selected_project_id = None;
            }
            self.selected_machine_id = Some(computer.machine_id.clone());
        }
        self.computer_select.close();
        self.fit.refit();
        self.announce_target(cx);
        cx.notify();
    }

    fn project_count(&self) -> usize {
        self.effective_computer()
            .map(|computer| computer.projects.len())
            .unwrap_or(0)
    }

    fn toggle_project_menu(&mut self, cx: &mut Context<Self>) {
        if self.controls_disabled() || self.project_count() == 0 {
            return;
        }
        let selected = self.effective_project_index();
        self.agent_select.close();
        self.computer_select.close();
        self.project_select.toggle(selected);
        cx.notify();
    }

    fn choose_project(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(project) = self
            .effective_computer()
            .and_then(|computer| computer.projects.get(index))
        {
            self.selected_project_id = Some(project.project_id.clone());
        }
        self.project_select.close();
        self.fit.refit();
        self.announce_target(cx);
        cx.notify();
    }

    /// Q544: the Add Project clone flow on that computer, prefilled with this session's repository.
    fn add_project_on_computer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(machine_id) = self
            .effective_computer()
            .map(|computer| computer.machine_id.clone())
        else {
            return;
        };
        let picks = self.picks();
        self.close_window_and_send(
            ExportTranscriptModalCommand::AddProjectOn { machine_id, picks },
            window,
            cx,
        );
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
        match self.computer_select.handle_key(key, self.computers.len()) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_computer(index, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        let project_count = self.project_count();
        match self.project_select.handle_key(key, project_count) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_project(index, cx);
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
        let label = |text: &'static str| {
            modal_section_title(&p, text)
                .flex_shrink_0()
                .w(px(ROW_LABEL_WIDTH))
        };
        let agent_row = h_flex()
            .min_h(px(MODAL_CONTROL_HEIGHT))
            .items_center()
            .gap(px(10.0))
            .on_children_prepainted(capture_child_bounds(
                self.agent_select.trigger_bounds.clone(),
                1,
            ))
            .child(label(CONTINUE_WITH))
            .child(modal_select_trigger(
                &p,
                &self.agent_select,
                "export-transcript-agent-select",
                value,
                SELECT_AGENT_PLACEHOLDER,
                self.controls_disabled(),
                |this, _window, cx| this.toggle_agent_menu(cx),
                cx,
            ));
        // CDXC:TranscriptExport 2026-10-31 DECISION:
        // User: Handoff to an agent "needs to get all the options" of the other card "but we need to make it more organized and allow either handing off in the current project and current machine and all that (default) or handing off to another machine/another project". The common case is one quiet row that says where the session goes; Change… opens the Computer and Project choices under it, and the row keeps saying where it goes.
        let open = self.destination_open;
        let button_label = match (open, self.elsewhere().is_some()) {
            (false, _) => DESTINATION_CHANGE,
            (true, true) => DESTINATION_RESET,
            (true, false) => DESTINATION_HIDE,
        };
        let destination_row = h_flex()
            .min_h(px(MODAL_CONTROL_HEIGHT))
            .items_center()
            .gap(px(10.0))
            .child(label(DESTINATION))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .child(self.destination_summary()),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .w(px(148.0))
                    .child(modal_action_button(
                        &p,
                        "export-transcript-destination",
                        button_label,
                        None,
                        ModalButtonTone::Neutral,
                        self.controls_disabled(),
                        move |this: &mut Self, _window, cx| {
                            if open {
                                this.use_this_project(cx);
                            } else {
                                this.open_destination(cx);
                            }
                        },
                        cx,
                    )),
            );
        if !self.destination_open {
            return v_flex()
                .w_full()
                .gap(px(10.0))
                .child(agent_row)
                .child(destination_row)
                .into_any_element();
        }
        let computer = self.effective_computer();
        let project_value = self.effective_project().map(project_label);
        let computer_row = h_flex()
            .min_h(px(MODAL_CONTROL_HEIGHT))
            .items_center()
            .gap(px(10.0))
            .on_children_prepainted(capture_child_bounds(
                self.computer_select.trigger_bounds.clone(),
                1,
            ))
            .child(label(COMPUTER))
            .child(modal_select_trigger(
                &p,
                &self.computer_select,
                "export-transcript-computer-select",
                computer.map(|computer| computer.label.clone()),
                SELECT_COMPUTER_PLACEHOLDER,
                self.controls_disabled() || self.computers.is_empty(),
                |this, _window, cx| this.toggle_computer_menu(cx),
                cx,
            ));
        let project_row = h_flex()
            .min_h(px(MODAL_CONTROL_HEIGHT))
            .items_center()
            .gap(px(10.0))
            .on_children_prepainted(capture_child_bounds(
                self.project_select.trigger_bounds.clone(),
                1,
            ))
            .child(label(PROJECT))
            .child(modal_select_trigger(
                &p,
                &self.project_select,
                "export-transcript-project-select",
                project_value,
                SELECT_PROJECT_PLACEHOLDER,
                self.controls_disabled() || self.project_count() == 0,
                |this, _window, cx| this.toggle_project_menu(cx),
                cx,
            ));
        let mut rows = v_flex()
            .w_full()
            .gap(px(10.0))
            .child(agent_row)
            .child(destination_row)
            .child(computer_row)
            .child(project_row);
        if self.computers.is_empty() {
            rows = rows.child(modal_hint(&p, NO_COMPUTER_HINT));
        } else if let Some(computer) = computer
            && computer.projects.iter().all(|project| !project.same_repo)
        {
            rows = rows.child(self.render_missing_project(computer, cx));
        }
        if let Some((_, line)) = &self.branch_notice {
            rows = rows.child(self.render_branch_notice(line.clone()));
        }
        rows.into_any_element()
    }

    /// Q544: the other computer has no project with this repository (or no project at all).
    fn render_missing_project(
        &self,
        computer: &ExportTranscriptComputer,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let (text, button) = match &computer.missing_repository {
            Some(repository) => (
                format!("{} doesn't have {repository} yet.", computer.label),
                "Clone it there…",
            ),
            None if computer.projects.is_empty() => (
                format!("{} has no projects yet.", computer.label),
                "Add a project there…",
            ),
            None => return div().into_any_element(),
        };
        h_flex()
            .w_full()
            .items_center()
            .gap(px(10.0))
            .child(modal_hint(&p, text).flex_1().min_w_0())
            .child(modal_action_button(
                &p,
                "export-transcript-add-project",
                button,
                None,
                ModalButtonTone::Neutral,
                self.controls_disabled(),
                |this, window, cx| this.add_project_on_computer(window, cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_branch_notice(&self, line: String) -> AnyElement {
        let p = self.palette;
        modal_panel(&p)
            .gap(px(2.0))
            .px(px(12.0))
            .py(px(10.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(BRANCH_NOTICE_TITLE),
            )
            .child(modal_hint(&p, line))
            .child(modal_hint(&p, BRANCH_NOTICE_SUBTITLE))
            .into_any_element()
    }

    /// CDXC:TranscriptExport 2026-10-31 DECISION:
    /// User: "The 'include' part needs to be collapsed by default". The heading says what is in the file, so a closed section hides nothing that matters.
    fn render_include(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.palette;
        let disabled = self.controls_disabled();
        let values = [
            self.include.commands,
            self.include.patches,
            self.include.reasoning,
        ];
        let chosen: Vec<&str> = INCLUDE_ROWS
            .iter()
            .zip(values)
            .filter(|(_, on)| *on)
            .map(|((label, _), _)| *label)
            .collect();
        let include_text = if chosen.is_empty() {
            INCLUDE_NOTHING.to_string()
        } else {
            chosen.join(", ")
        };
        let summary = if self.history_supported() {
            handoff_include_summary(&self.history, &include_text)
        } else {
            include_text
        };
        let include_open = self.include_open;
        let mut elements = vec![
            h_flex()
                .id("export-transcript-include")
                .w_full()
                .min_h(px(MODAL_CONTROL_HEIGHT))
                .items_center()
                .gap(px(6.0))
                .when(!disabled, |this| {
                    this.cursor_pointer().on_click(cx.listener(
                        |this, _: &ClickEvent, _window, cx| {
                            this.toggle_include_open(cx);
                        },
                    ))
                })
                .child(
                    modal_icon(ICON_CHEVRON_RIGHT, 14.0, p.muted).with_transformation(
                        gpui::Transformation::rotate(gpui::percentage(if include_open {
                            0.25
                        } else {
                            0.0
                        })),
                    ),
                )
                .child(modal_section_title(&p, INCLUDE).flex_shrink_0())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_right()
                        .child(modal_hint(&p, summary)),
                )
                .into_any_element(),
        ];
        if include_open && self.history_supported() {
            elements.push(self.render_history(cx));
        }
        if include_open {
            elements.push(
                modal_panel(&p)
                    .children(INCLUDE_ROWS.iter().enumerate().map(
                        |(index, (label, description))| {
                            modal_panel_row(
                                &p,
                                *label,
                                Some(*description),
                                modal_switch(&p, values[index], disabled),
                                index > 0,
                            )
                            .id(("export-transcript-toggle-row", index))
                            .switch_row_press(
                                switch_row_wash(p.foreground),
                                disabled,
                                bind_press(cx, move |this: &mut Self, _window, cx| {
                                    this.toggle_include(index, cx);
                                }),
                            )
                        },
                    ))
                    .into_any_element(),
            );
        }
        if let ExportTranscriptStage::Failed { message } = &self.stage {
            elements.push(modal_error(&p, message.clone()));
        }
        elements
    }

    /// CDXC:TranscriptExport 2026-11-03 DECISION:
    /// User (Q1150, Q1155, Q1156, Q1158): History sits inside Include: a "Whole conversation" switch, and while it is off a slider over the last 1 to 100 turns with its value on the label's line, and under them what the pick holds ("10 of 120 turns · about 9k tokens (the whole is 87k)"). The slider is drawn here rather than taken from GPUI Kit because this dialog also runs in the web build, which has no `gpui_base`.
    fn render_history(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let disabled = self.controls_disabled();
        let whole = self.history.all;
        let mut panel = modal_panel(&p).child(
            modal_panel_row(
                &p,
                WHOLE_CONVERSATION,
                Some(WHOLE_CONVERSATION_DESCRIPTION),
                modal_switch(&p, whole, disabled),
                false,
            )
            .id("export-transcript-whole-conversation")
            .switch_row_press(
                switch_row_wash(p.foreground),
                disabled,
                bind_press(cx, |this: &mut Self, _window, cx| {
                    this.toggle_whole_conversation(cx);
                }),
            ),
        );
        let estimate = self.history_estimate();
        if !whole {
            panel = panel.child(
                v_flex()
                    .w_full()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .py(px(9.0))
                    .border_t_1()
                    .border_color(hsla(p.hairline))
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .gap(px(12.0))
                            .text_size(px(13.0))
                            .line_height(px(18.57))
                            .child(div().font_weight(FontWeight::MEDIUM).child(HISTORY))
                            .child(
                                div()
                                    .text_color(hsla(p.muted))
                                    .child(handoff_history_label(&self.history)),
                            ),
                    )
                    .child(self.render_history_slider(disabled, cx))
                    .children(estimate.map(|estimate| modal_hint(&p, estimate))),
            );
        } else if let Some(estimate) = estimate {
            panel = panel.child(
                div()
                    .px(px(12.0))
                    .pb(px(9.0))
                    .child(modal_hint(&p, estimate)),
            );
        }
        panel.into_any_element()
    }

    /// A 4px track with a dot at each stop, filled up to a 16px thumb.
    fn render_history_slider(&self, disabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let last = (HANDOFF_HISTORY_STOPS.len() - 1) as f32;
        let at = handoff_history_stop_index(self.history.turns) as f32 / last;
        let track = div()
            .id("export-transcript-history-slider")
            .relative()
            .w_full()
            .h(px(20.0))
            .when(disabled, |this| this.opacity(0.5))
            .child(
                div()
                    .absolute()
                    .top(px(8.0))
                    .left_0()
                    .w_full()
                    .h(px(4.0))
                    .rounded(px(2.0))
                    .bg(hsla(p.switch_off)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(8.0))
                    .left_0()
                    .w(relative(at))
                    .h(px(4.0))
                    .rounded(px(2.0))
                    .bg(hsla(p.primary)),
            )
            .children((0..HANDOFF_HISTORY_STOPS.len()).map(|index| {
                let fraction = index as f32 / last;
                div()
                    .absolute()
                    .top(px(8.0))
                    .left(relative(fraction))
                    .ml(px(-2.0))
                    .size(px(4.0))
                    .rounded_full()
                    .bg(hsla(if fraction <= at {
                        p.primary_foreground
                    } else {
                        p.muted
                    }))
                    .opacity(0.6)
            }))
            .child(
                div()
                    .absolute()
                    .top(px(2.0))
                    .left(relative(at))
                    .ml(px(-8.0))
                    .size(px(16.0))
                    .rounded_full()
                    .border_2()
                    .border_color(hsla(p.primary))
                    .bg(hsla(p.background))
                    .shadow_sm(),
            )
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.slider_dragging = true;
                            this.slide_history_to(event.position.x, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        if !this.slider_dragging {
                            return;
                        }
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.end_history_slide(cx);
                            return;
                        }
                        this.slide_history_to(event.position.x, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _window, cx| this.end_history_slide(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _window, cx| this.end_history_slide(cx)),
                    )
            });
        // Pad the ends so the thumb's half past each end stays inside the panel.
        h_flex()
            .w_full()
            .px(px(8.0))
            .on_children_prepainted(capture_child_bounds(self.slider_bounds.clone(), 0))
            .child(track)
            .into_any_element()
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

    /// CDXC:TranscriptExport 2026-09-15 DECISION:
    /// User: Handoff / Export is one page, not an options page followed by a result page, and "Handoff to an agent" and "Export to Markdown" are explicit choices on it (later Handoff to MD, with a second card for a while until 2026-10-31). The mode cards sit above the include-toggles; Handoff writes the file and starts the follow-up conversation in one click, while Export swaps the toggles for the saved path with Copy and Reveal. Starting a conversation never sends a prompt for the user: the mention is typed into the new agent's input and left unsubmitted. Reveal is omitted, not disabled, when the file lives on another machine.
    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = v_flex()
            .w_full()
            .gap(px(10.0))
            .child(self.render_mode_grid(cx))
            .child(self.render_agent_row(cx));
        if let Some(hint) = self.target_unavailable_hint() {
            body = body.child(modal_hint(&self.palette, hint));
        }
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
        let agent_menu = modal_select_menu(
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
        let computer_names: Vec<String> = self
            .computers
            .iter()
            .map(|computer| computer.label.clone())
            .collect();
        let computer_menu = modal_select_menu(
            &p,
            &self.computer_select,
            "export-transcript-computer-menu",
            &computer_names,
            self.effective_computer_index(),
            |this, index, _window, cx| this.choose_computer(index, cx),
            |this, _window, cx| {
                this.computer_select.close();
                cx.notify();
            },
            window,
            cx,
        );
        let project_names: Vec<String> = self
            .effective_computer()
            .map(|computer| computer.projects.iter().map(project_label).collect())
            .unwrap_or_default();
        let project_menu = modal_select_menu(
            &p,
            &self.project_select,
            "export-transcript-project-menu",
            &project_names,
            self.effective_project_index(),
            |this, index, _window, cx| this.choose_project(index, cx),
            |this, _window, cx| {
                this.project_select.close();
                cx.notify();
            },
            window,
            cx,
        );
        let menu = agent_menu.or(computer_menu).or(project_menu);
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

impl ModalCornerClose for GpuiExportTranscriptModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }

    /// Only the result closes on a click away; the options and a running export stay.
    fn keeps_open_on_click_away(&self, _window: &Window, _cx: &App) -> bool {
        matches!(
            self.stage,
            ExportTranscriptStage::Options | ExportTranscriptStage::Exporting
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(agent_id: &str, family: Option<&str>) -> ExportTranscriptAgent {
        ExportTranscriptAgent {
            agent_id: agent_id.to_string(),
            name: agent_id.to_string(),
            family: family.map(str::to_string),
        }
    }

    #[test]
    fn handoff_target_is_the_picked_family_not_the_sessions_agent() {
        let agents = [
            agent("claude", Some("claude")),
            agent("opencode", None),
            agent("codex", Some("codex")),
        ];
        assert_eq!(
            handoff_target_agent(&agents, "codex").map(|agent| agent.agent_id.as_str()),
            Some("codex")
        );
    }

    fn computer(
        machine_id: &str,
        projects: &[&str],
        preferred: Option<&str>,
    ) -> ExportTranscriptComputer {
        ExportTranscriptComputer {
            machine_id: machine_id.to_string(),
            label: machine_id.to_string(),
            is_source: false,
            projects: projects
                .iter()
                .map(|id| ExportTranscriptProject {
                    project_id: id.to_string(),
                    title: id.to_string(),
                    path: None,
                    same_repo: Some(*id) == preferred,
                    is_own: false,
                })
                .collect(),
            preferred_project_id: preferred.map(str::to_string),
            missing_repository: None,
        }
    }

    #[test]
    fn remote_target_defaults_to_the_first_computer_and_its_matching_project() {
        let computers = [
            computer("studio", &["site", "ghostex"], Some("ghostex")),
            computer("linux", &["notes"], None),
        ];
        let first = handoff_computer(&computers, None).unwrap();
        assert_eq!(first.machine_id, "studio");
        assert_eq!(handoff_project(first, None).unwrap().project_id, "ghostex");
        // A computer that went away falls back to the first one offered.
        assert_eq!(
            handoff_computer(&computers, Some("gone"))
                .unwrap()
                .machine_id,
            "studio"
        );
    }

    #[test]
    fn remote_target_keeps_a_pick_and_never_guesses_without_a_match() {
        let computers = [
            computer("studio", &["site", "ghostex"], Some("ghostex")),
            computer("linux", &["notes"], None),
        ];
        let studio = handoff_computer(&computers, Some("studio")).unwrap();
        assert_eq!(
            handoff_project(studio, Some("site")).unwrap().project_id,
            "site"
        );
        // A project id from the other computer is not one of this computer's.
        assert_eq!(
            handoff_project(studio, Some("notes")).unwrap().project_id,
            "ghostex"
        );
        let linux = handoff_computer(&computers, Some("linux")).unwrap();
        assert_eq!(handoff_project(linux, None), None);
    }

    #[test]
    fn the_button_names_the_project_and_only_a_computer_it_leaves_for() {
        let project = ExportTranscriptProject {
            project_id: "p".to_string(),
            title: "acme-web".to_string(),
            path: None,
            same_repo: false,
            is_own: false,
        };
        let computer = |is_source| ExportTranscriptComputer {
            machine_id: "m".to_string(),
            label: "studio".to_string(),
            is_source,
            projects: vec![project.clone()],
            preferred_project_id: None,
            missing_repository: None,
        };
        assert_eq!(
            destination_label(&computer(true), &project),
            "Handoff to acme-web"
        );
        assert_eq!(
            destination_label(&computer(false), &project),
            "Handoff to acme-web on studio"
        );
    }

    fn prefs_from(text: &str) -> ExportTranscriptModalPrefs {
        ExportTranscriptModalPrefs::from_saved(&serde_json::from_str(text).unwrap())
    }

    #[test]
    fn a_saved_handoff_to_remote_comes_back_as_a_handoff_to_an_agent_with_its_choices_open() {
        let prefs = prefs_from(r#"{"mode":"handoffRemote","includeCommands":true}"#);
        assert_eq!(prefs.mode, ExportTranscriptMode::Handoff);
        assert!(prefs.destination_open);
        assert_eq!(prefs.include, ExportTranscriptIncludeOptions::default());
    }

    #[test]
    fn include_starts_closed_and_off_even_with_the_old_dialogs_saved_switches() {
        let prefs = prefs_from(
            r#"{"mode":"handoff","includeCommands":true,"includePatches":true,"includeReasoning":false}"#,
        );
        assert_eq!(prefs.include, ExportTranscriptIncludeOptions::default());
        assert!(!prefs.include_open);
    }

    #[test]
    fn a_remembered_destination_opens_only_while_it_still_applies() {
        let mut own = computer("mac", &["own", "acme"], Some("own"));
        own.is_source = true;
        own.projects[0].is_own = true;
        let computers = [own, computer("studio", &["site"], None)];
        let target = |machine_id: &str, project_id: &str| ExportTranscriptRemoteTarget {
            machine_id: machine_id.to_string(),
            project_id: project_id.to_string(),
        };
        let acme = target("mac", "acme");
        assert_eq!(
            opening_destination(Some(&acme), &computers),
            Some(acme.clone())
        );
        let site = target("studio", "site");
        assert_eq!(
            opening_destination(Some(&site), &computers),
            Some(site.clone())
        );
        // The session's own project, a project that's gone, a computer that's gone.
        assert_eq!(
            opening_destination(Some(&target("mac", "own")), &computers),
            None
        );
        assert_eq!(
            opening_destination(Some(&target("mac", "gone")), &computers),
            None
        );
        assert_eq!(
            opening_destination(Some(&target("gone", "site")), &computers),
            None
        );
    }

    #[test]
    fn what_the_user_changes_round_trips_through_the_prefs_file() {
        let dir =
            std::env::temp_dir().join(format!("ghostex-export-modal-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefs.json");
        let prefs = ExportTranscriptModalPrefs {
            mode: ExportTranscriptMode::Export,
            include: ExportTranscriptIncludeOptions {
                commands: true,
                patches: false,
                reasoning: true,
            },
            include_open: true,
            agent_id: Some("codex".to_string()),
            destination: Some(ExportTranscriptRemoteTarget {
                machine_id: "studio".to_string(),
                project_id: "acme".to_string(),
            }),
            destination_open: true,
            history: HandoffHistory {
                all: false,
                turns: 50,
            },
        };
        persist_export_transcript_modal_prefs(&path, &prefs);
        assert_eq!(load_export_transcript_modal_prefs(&path), prefs);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
