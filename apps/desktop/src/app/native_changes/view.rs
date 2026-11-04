//! The Changes view's state and behaviour: which project and scope it shows, loading and
//! refreshing the diff, the display switches, viewed and folded files, picking lines, comments,
//! reverts and the commit bar. Drawing is in render.rs, rows.rs and commit_bar.rs.
//!
//! CDXC:Changes 2026-10-25 DECISION:
//! User: "create a new sidepanel view that we can use for diffs … should be able to comment on lines". The answers that shaped it: its own Changes tab in the side panel (Expand gives it the whole work area), Unified/Split toggle with unified first, a flat file list above the diff that can be hidden to leave just the stream with sticky file headers, Against main (default) and Uncommitted as its scopes, line comments that go into the session's chat composer and then stay on their lines as Sent markers until those lines change, and Open in Files, copy, Viewed, keyboard navigation, revert, Commit/Create PR and the whitespace and wrap switches as its actions. Per-turn diffs were left for later.
//! SEE-ALSO: server/src/typed_operations/git_changes.rs (`changesDiff`, `changesRevert`), apps/desktop/src/app/native_changes_host.rs (the desktop's answers to `ChangesRequest`), apps/gpui-web/src/app/web_host/changes.rs (the web build's).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, ListAlignment, ListOffset, ListState,
    Subscription, Task, Window, px,
};
use gpui_component::input::{InputEvent, TextareaState};
use serde_json::json;

use super::comments::{
    ChangesComment, CommentRange, CommentState, auto_resolve, now_ms, review_comment,
};
use super::model::{ChangesData, ChangesScope, HunkLine, LineAnchor, LineKind, Row, build_rows};
use super::style::ChangesStyle;
use crate::app::gx_store::git::calls;
use crate::app::window::native_modal_kit::{ModalPalette, ModalSelect};

/// The project the view shows.
#[derive(Clone)]
pub(crate) struct ChangesTarget {
    /// The store's scoped project id (`remote:<machine>:<id>` for a remote project).
    pub(crate) project_key: String,
    /// The id gxserver knows the project by.
    pub(crate) rpc_project_id: String,
    pub(crate) remote: calls::Remote,
}

/// The store's key names the machine too, so two targets with the same key are one project.
impl PartialEq for ChangesTarget {
    fn eq(&self, other: &Self) -> bool {
        self.project_key == other.project_key && self.rpc_project_id == other.rpc_project_id
    }
}

/// What the app gives the view each frame.
#[derive(Clone)]
pub(crate) struct ChangesHostInfo {
    pub(crate) palette: ModalPalette,
    /// `(agent id, name)` of the agents that can write a commit message.
    pub(crate) agents: Vec<(String, String)>,
    pub(crate) default_agent_id: Option<String>,
}

/// What an entry point asks the view to show when it opens it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChangesPreset {
    pub(crate) scope: ChangesScope,
    pub(crate) action: Option<CommitAction>,
    /// A file to show (absolute, or relative to the project): the view picks Uncommitted when
    /// the file has uncommitted work and Against main otherwise, and scrolls to it.
    pub(crate) reveal: Option<String>,
}

/// The commit bar's main button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CommitAction {
    #[default]
    Commit,
    CommitPush,
    CommitPushPr,
}

impl CommitAction {
    pub(crate) fn label(self, on_default_branch: bool) -> &'static str {
        match self {
            Self::Commit => "Commit",
            Self::CommitPush => "Commit & Push",
            Self::CommitPushPr if on_default_branch => "Commit, Push & Create PR on a new branch",
            Self::CommitPushPr => "Commit, Push & Create PR",
        }
    }

    pub(crate) fn short_label(self) -> &'static str {
        match self {
            Self::Commit => "Commit",
            Self::CommitPush => "Commit & Push",
            Self::CommitPushPr => "Commit, Push & PR",
        }
    }
}

/// What the view asks the app to do.
pub(crate) enum ChangesRequest {
    Commit {
        project_key: String,
        action: CommitAction,
        message: String,
        agent_id: Option<String>,
        /// `None` commits every uncommitted file.
        file_paths: Option<Vec<String>>,
        /// On the default branch, a pull request first moves the work onto a new branch.
        new_branch: bool,
    },
    MultipleCommits {
        project_key: String,
        agent_id: Option<String>,
    },
    MergeIntoMain {
        project_key: String,
        message: String,
        agent_id: Option<String>,
        file_paths: Option<Vec<String>>,
    },
    /// Adds `pill` after what is in the chat box of the session that owns the project, or pastes
    /// `text` (the full block) into its terminal; `done` gets where it went ("Claude's chat").
    Comment {
        text: String,
        pill: String,
        done: Box<dyn FnOnce(String, &mut App)>,
    },
    OpenInFiles {
        project_key: String,
        path: String,
        line: Option<u32>,
    },
    RevealFile {
        project_key: String,
        path: String,
    },
    /// The user picked the agent that writes commit messages (the app remembers it).
    AgentPicked(String),
    /// Something was copied (the app plays its copy sound).
    Copied,
    Toast {
        title: String,
        description: Option<String>,
        error: bool,
    },
    /// The working tree changed under the view (a revert): the app re-reads its Git numbers.
    WorkingTreeChanged {
        project_key: String,
    },
}

pub(crate) type ChangesHostFn = Rc<dyn Fn(ChangesRequest, &mut Window, &mut App)>;

/// A pending "are you sure" on a revert or a merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Confirm {
    RevertFile(String),
    RevertHunk(String, usize),
    Merge,
}

/// The open comment editor.
pub(crate) struct CommentEditor {
    pub(crate) range: CommentRange,
    pub(crate) input: Entity<TextareaState>,
}

/// What a project keeps while the view shows another one.
#[derive(Default)]
struct ProjectMemory {
    scope: ChangesScope,
    collapsed: HashSet<String>,
    viewed: HashMap<String, u64>,
    comments: Vec<ChangesComment>,
    excluded: HashSet<String>,
}

pub(crate) struct ChangesView {
    pub(crate) host: ChangesHostFn,
    pub(crate) target: Option<ChangesTarget>,
    pub(crate) info: Option<ChangesHostInfo>,
    pub(crate) style: Option<ChangesStyle>,
    pub(crate) scope: ChangesScope,
    pub(crate) split: bool,
    pub(crate) wrap: bool,
    pub(crate) hide_whitespace: bool,
    /// The file count's jump-to-file menu, and where its button is (window coordinates).
    pub(crate) jump_open: bool,
    pub(crate) jump_anchor: gpui::Bounds<gpui::Pixels>,
    /// The scope menu (Whole branch / Uncommitted only), and where its button is.
    pub(crate) scope_open: bool,
    pub(crate) scope_anchor: gpui::Bounds<gpui::Pixels>,
    pub(crate) data: Option<ChangesData>,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    load_generation: u64,
    pub(crate) collapsed: HashSet<String>,
    /// Viewed files, with the hash of the diff they had when marked: a file that changes again
    /// is no longer viewed.
    pub(crate) viewed: HashMap<String, u64>,
    pub(crate) rows: Vec<Row>,
    pub(crate) list: ListState,
    /// The Files zoom the diff rows were last measured at (`sync_files_zoom`).
    drawn_files_zoom: Option<f32>,
    pub(crate) comments: Vec<ChangesComment>,
    next_comment_id: u64,
    pub(crate) show_resolved: bool,
    /// The picked rows (anchor, head), always inside one hunk.
    pub(crate) selection: Option<(usize, usize)>,
    pub(crate) dragging: bool,
    pub(crate) editor: Option<CommentEditor>,
    pub(crate) message: Option<Entity<TextareaState>>,
    pub(crate) commit_action: CommitAction,
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_select: ModalSelect,
    /// Uncommitted files left out of the next commit.
    pub(crate) excluded: HashSet<String>,
    pub(crate) commit_menu_open: bool,
    pub(crate) confirm: Option<Confirm>,
    /// The file whose header menu is open, and where it was asked for (view coordinates).
    pub(crate) file_menu: Option<(String, gpui::Point<gpui::Pixels>)>,
    /// The row the keyboard is on (a hunk header).
    pub(crate) cursor: Option<usize>,
    /// The file the user last moved into with J, K, ] or [; its header keeps its buttons showing.
    /// Only a key sets it, never a load, a scroll or a jump.
    pub(crate) key_file: Option<String>,
    /// The file header under the pointer (its path, and whether it is the pinned copy).
    pub(crate) hovered_header: Option<(String, bool)>,
    pub(crate) focus: FocusHandle,
    pub(crate) view_origin: gpui::Point<gpui::Pixels>,
    /// The view's width, so a menu opened near its right edge stays inside it.
    pub(crate) view_width: gpui::Pixels,
    /// The file an entry point asked to show, until the diff that has it arrives.
    pending_reveal: Option<String>,
    /// The file header row a jump scrolled to the top, to centre once the list has measured the
    /// rows around it (see `center_pending_file`).
    pending_center: Option<usize>,
    memory: HashMap<String, ProjectMemory>,
    last_render_ms: u64,
    _poll: Task<()>,
    _subscriptions: Vec<Subscription>,
}

const REFRESH_INTERVAL: Duration = Duration::from_secs(3);

impl ChangesView {
    pub(crate) fn new(host: ChangesHostFn, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(REFRESH_INTERVAL).await;
                let alive = this.update(cx, |this, cx| {
                    // Only while the view is on screen (drawn in the last few seconds).
                    if now_ms().saturating_sub(this.last_render_ms) < 4_000 && !this.loading {
                        this.load(false, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            host,
            target: None,
            info: None,
            style: None,
            scope: ChangesScope::AgainstMain,
            split: false,
            wrap: true,
            hide_whitespace: false,
            jump_open: false,
            key_file: None,
            hovered_header: None,
            jump_anchor: gpui::Bounds::default(),
            scope_open: false,
            scope_anchor: gpui::Bounds::default(),
            data: None,
            loading: false,
            error: None,
            load_generation: 0,
            collapsed: HashSet::new(),
            viewed: HashMap::new(),
            rows: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(800.0)),
            drawn_files_zoom: None,
            comments: Vec::new(),
            next_comment_id: 1,
            show_resolved: false,
            selection: None,
            dragging: false,
            editor: None,
            message: None,
            commit_action: CommitAction::Commit,
            agent_id: None,
            agent_select: ModalSelect::new(),
            excluded: HashSet::new(),
            commit_menu_open: false,
            confirm: None,
            file_menu: None,
            cursor: None,
            focus: cx.focus_handle(),
            view_origin: gpui::Point::default(),
            view_width: gpui::Pixels::default(),
            pending_reveal: None,
            pending_center: None,
            memory: HashMap::new(),
            last_render_ms: 0,
            _poll: poll,
            _subscriptions: Vec::new(),
        }
    }

    /// The app's per-frame hand-off: the project, the palette and agents, and an entry point's
    /// preset. True when something the view draws changed.
    pub(crate) fn sync(
        &mut self,
        target: Option<ChangesTarget>,
        info: ChangesHostInfo,
        preset: Option<ChangesPreset>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.last_render_ms = now_ms();
        let mut changed = false;
        if self.style.is_none()
            || self
                .info
                .as_ref()
                .is_none_or(|old| !same_palette(old, &info))
        {
            self.style = Some(ChangesStyle::resolve(info.palette));
            changed = true;
        }
        // A pick that is no longer offered (a cloud agent, a removed one) falls back to the default.
        if self
            .agent_id
            .as_ref()
            .is_none_or(|id| !info.agents.iter().any(|agent| &agent.0 == id))
        {
            self.agent_id = info
                .default_agent_id
                .clone()
                .or_else(|| info.agents.first().map(|agent| agent.0.clone()));
        }
        self.info = Some(info);
        self.ensure_message_input(window, cx);
        if target != self.target {
            self.switch_target(target, cx);
            changed = true;
        }
        if let Some(preset) = preset {
            if let Some(path) = preset.reveal.clone() {
                self.pending_reveal = Some(path);
                self.try_reveal(cx);
            }
            if preset.reveal.is_none() && self.scope != preset.scope {
                self.scope = preset.scope;
                self.data = None;
                self.load(true, cx);
            }
            if let Some(action) = preset.action {
                self.commit_action = action;
                if let Some(message) = &self.message {
                    message.update(cx, |input, cx| input.focus(window, cx));
                }
            }
            changed = true;
        }
        changed
    }

    fn ensure_message_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.message.is_some() {
            return;
        }
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Commit message (leave empty and the agent writes it)")
                .auto_grow(1, 6)
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter {
                    secondary: true, ..
                } => this.run_commit(window, cx),
                InputEvent::Change | InputEvent::Focus | InputEvent::Blur => cx.notify(),
                _ => {}
            },
        );
        self._subscriptions.push(subscription);
        self.message = Some(input);
    }

    fn switch_target(&mut self, target: Option<ChangesTarget>, cx: &mut Context<Self>) {
        if let Some(old) = self.target.take() {
            self.memory.insert(
                old.project_key,
                ProjectMemory {
                    scope: self.scope,
                    collapsed: std::mem::take(&mut self.collapsed),
                    viewed: std::mem::take(&mut self.viewed),
                    comments: std::mem::take(&mut self.comments),
                    excluded: std::mem::take(&mut self.excluded),
                },
            );
        }
        let memory = target
            .as_ref()
            .and_then(|target| self.memory.remove(&target.project_key))
            .unwrap_or_default();
        self.scope = memory.scope;
        self.collapsed = memory.collapsed;
        self.viewed = memory.viewed;
        self.comments = memory.comments;
        self.excluded = memory.excluded;
        self.target = target;
        self.data = None;
        self.error = None;
        self.selection = None;
        self.editor = None;
        self.confirm = None;
        self.cursor = None;
        self.rows.clear();
        self.list.reset(0);
        self.load(true, cx);
    }

    /// Reads the diff. `reset` draws the loading state while it runs (a new project or scope);
    /// a background refresh keeps the current rows until something changed.
    pub(crate) fn load(&mut self, reset: bool, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        self.load_generation += 1;
        let generation = self.load_generation;
        self.loading = true;
        if reset {
            cx.notify();
        }
        let params = json!({
            "scope": self.scope.wire(),
            "ignoreWhitespace": self.hide_whitespace,
        });
        cx.spawn(async move |this, cx| {
            let answer = calls::git_action(
                target.remote.clone(),
                target.rpc_project_id.clone(),
                "changesDiff",
                params,
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.load_generation {
                    return;
                }
                this.loading = false;
                match answer {
                    Ok(answer)
                        if answer.get("isRepo").and_then(serde_json::Value::as_bool)
                            == Some(false) =>
                    {
                        this.error = Some("This project is not a Git repository.".to_string());
                        this.data = None;
                        this.rebuild_rows();
                    }
                    Ok(answer) => {
                        let data = ChangesData::from_answer(&answer);
                        let same = this
                            .data
                            .as_ref()
                            .is_some_and(|old| old.fingerprint == data.fingerprint);
                        this.error = None;
                        if !same {
                            auto_resolve(&mut this.comments, &data);
                            let live: HashSet<&str> =
                                data.uncommitted_paths.iter().map(String::as_str).collect();
                            this.excluded.retain(|path| live.contains(path.as_str()));
                            this.data = Some(data);
                            this.forget_stale_viewed();
                            this.rebuild_rows();
                        }
                        this.try_reveal(cx);
                    }
                    Err(error) => {
                        this.error = Some(error.message.clone());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn forget_stale_viewed(&mut self) {
        let Some(data) = &self.data else {
            return;
        };
        let hashes: HashMap<&str, u64> = data
            .files
            .iter()
            .map(|file| (file.path.as_str(), file_hash(file)))
            .collect();
        self.viewed
            .retain(|path, hash| hashes.get(path.as_str()) == Some(hash));
    }

    /// Lays the rows out again, keeping the scroll position on the same row where it can.
    pub(crate) fn rebuild_rows(&mut self) {
        let top = self.list.logical_scroll_top();
        let anchor_row = self.rows.get(top.item_ix).copied();
        let Some(data) = &self.data else {
            self.rows.clear();
            self.list.reset(0);
            return;
        };
        let mut comments_after: HashMap<LineAnchor, Vec<u64>> = HashMap::new();
        for comment in &self.comments {
            if comment.is_open() || self.show_resolved {
                comments_after
                    .entry(comment.range.after.clone())
                    .or_default()
                    .push(comment.id);
            }
        }
        let mut collapsed = self.collapsed.clone();
        collapsed.extend(self.viewed.keys().cloned());
        let editor_after = self.editor.as_ref().map(|editor| &editor.range.after);
        self.rows = build_rows(data, self.split, &collapsed, &comments_after, editor_after);
        self.list.reset(self.rows.len());
        let restored = anchor_row
            .and_then(|row| self.rows.iter().position(|candidate| *candidate == row))
            .unwrap_or(0);
        if restored > 0 {
            self.list.scroll_to(ListOffset {
                item_ix: restored,
                offset_in_item: top.offset_in_item,
            });
        }
        if self
            .selection
            .is_some_and(|(a, b)| a.max(b) >= self.rows.len())
        {
            self.selection = None;
        }
    }

    /// Shows the file an entry point asked for once a diff is loaded: on Uncommitted when the file
    /// has uncommitted work, otherwise on Against main, scrolled to its header.
    fn try_reveal(&mut self, cx: &mut Context<Self>) {
        let Some(wanted) = self.pending_reveal.clone() else {
            return;
        };
        let Some(data) = &self.data else {
            return;
        };
        let matches = |path: &str| {
            wanted == path
                || wanted
                    .strip_suffix(path)
                    .is_some_and(|rest| rest.ends_with('/') || rest.ends_with('\\'))
        };
        let uncommitted = data.uncommitted_paths.iter().any(|path| matches(path));
        let want_scope = if uncommitted {
            ChangesScope::Uncommitted
        } else {
            ChangesScope::AgainstMain
        };
        if self.scope != want_scope {
            self.set_scope(want_scope, cx);
            return;
        }
        self.pending_reveal = None;
        if let Some(path) = data
            .files
            .iter()
            .find(|file| matches(&file.path))
            .map(|file| file.path.clone())
        {
            self.collapsed.remove(&path);
            self.viewed.remove(&path);
            self.rebuild_rows();
            self.scroll_to_file(&path, cx);
        }
    }

    pub(crate) fn set_scope(&mut self, scope: ChangesScope, cx: &mut Context<Self>) {
        if self.scope == scope {
            return;
        }
        self.scope = scope;
        self.data = None;
        self.selection = None;
        self.editor = None;
        self.rows.clear();
        self.list.reset(0);
        self.load(true, cx);
    }

    pub(crate) fn set_split(&mut self, split: bool, cx: &mut Context<Self>) {
        self.split = split;
        self.selection = None;
        self.rebuild_rows();
        cx.notify();
    }

    pub(crate) fn set_hide_whitespace(&mut self, hide: bool, cx: &mut Context<Self>) {
        self.hide_whitespace = hide;
        self.load(false, cx);
        cx.notify();
    }

    pub(crate) fn toggle_collapsed(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.viewed.remove(path).is_some() {
            self.collapsed.remove(path);
        } else if !self.collapsed.remove(path) {
            self.collapsed.insert(path.to_string());
        }
        self.selection = None;
        self.rebuild_rows();
        cx.notify();
    }

    pub(crate) fn toggle_viewed(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.viewed.remove(path).is_none() {
            let hash = self
                .data
                .as_ref()
                .and_then(|data| data.files.iter().find(|file| file.path == path))
                .map(file_hash);
            if let Some(hash) = hash {
                self.viewed.insert(path.to_string(), hash);
            }
        }
        self.collapsed.remove(path);
        self.rebuild_rows();
        cx.notify();
    }

    pub(crate) fn set_all_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        self.collapsed.clear();
        if collapsed && let Some(data) = &self.data {
            self.collapsed
                .extend(data.files.iter().map(|file| file.path.clone()));
        }
        self.rebuild_rows();
        cx.notify();
    }

    pub(crate) fn all_collapsed(&self) -> bool {
        self.data.as_ref().is_some_and(|data| {
            !data.files.is_empty()
                && data.files.iter().all(|file| {
                    self.collapsed.contains(&file.path) || self.viewed.contains_key(&file.path)
                })
        })
    }

    pub(crate) fn scroll_to_file(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(data) = &self.data else {
            return;
        };
        let Some(index) = data.files.iter().position(|file| file.path == path) else {
            return;
        };
        if let Some(row) = self.rows.iter().position(|row| *row == Row::File(index)) {
            self.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.0),
            });
            self.cursor = Some(row);
        }
        cx.notify();
    }

    /// Picking a file in the jump menu: unfolds it (a folded or viewed file) and scrolls its
    /// header to the middle of the view.
    ///
    /// CDXC:Changes 2026-10-27 DECISION: User: "selecting a file here in changes view should expand it and scroll to center it's top in the view". The header lands centred, or as near as the top and the end of the diff allow; ] and [ still put the header at the top.
    pub(crate) fn jump_to_file(&mut self, path: &str, cx: &mut Context<Self>) {
        let was_viewed = self.viewed.remove(path).is_some();
        if self.collapsed.remove(path) || was_viewed {
            self.selection = None;
            self.rebuild_rows();
        }
        self.scroll_to_file(path, cx);
        self.pending_center = self.cursor;
    }

    /// Runs after the list's layout, which measured the rows around the header a jump put at
    /// the top: scrolls it down to the middle, then draws again.
    pub(crate) fn center_pending_file(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.pending_center.take() else {
            return;
        };
        let Some(header) = self.list.bounds_for_item(row) else {
            return;
        };
        let viewport = self.list.viewport_bounds();
        let room_below = self
            .rows
            .len()
            .checked_sub(1)
            .and_then(|last| self.list.bounds_for_item(last))
            .map(|last| f32::from(last.bottom() - viewport.bottom()));
        let delta = center_scroll_delta(
            f32::from(header.top() - viewport.top()),
            f32::from(header.size.height),
            f32::from(viewport.size.height),
            f32::from(-self.list.scroll_px_offset_for_scrollbar().y),
            room_below,
        );
        if delta.abs() >= 0.5 {
            self.list.scroll_by(px(delta));
            cx.notify();
        }
    }

    // -- Picking lines ------------------------------------------------------------------------

    /// The hunk lines of `row` (one, or two in a split row).
    pub(crate) fn row_lines(&self, row: usize) -> Vec<(usize, &HunkLine)> {
        let Some(data) = &self.data else {
            return Vec::new();
        };
        match self.rows.get(row) {
            Some(Row::Line(file, hunk, line)) => data.files[*file].hunks[*hunk]
                .lines
                .get(*line)
                .map(|line| vec![(*file, line)])
                .unwrap_or_default(),
            Some(Row::Split(file, hunk, left, right)) => {
                let lines = &data.files[*file].hunks[*hunk].lines;
                let mut out = Vec::new();
                for index in [left, right].into_iter().flatten() {
                    // A context line fills both sides of its row; take it once.
                    if out
                        .iter()
                        .any(|(_, line): &(usize, &HunkLine)| std::ptr::eq(*line, &lines[*index]))
                    {
                        continue;
                    }
                    out.push((*file, &lines[*index]));
                }
                out
            }
            _ => Vec::new(),
        }
    }

    fn row_hunk(&self, row: usize) -> Option<(usize, usize)> {
        match self.rows.get(row)? {
            Row::Line(file, hunk, _) | Row::Split(file, hunk, _, _) => Some((*file, *hunk)),
            Row::Hunk(file, hunk) => Some((*file, *hunk)),
            _ => None,
        }
    }

    pub(crate) fn begin_selection(&mut self, row: usize, extend: bool, cx: &mut Context<Self>) {
        if self.row_lines(row).is_empty() {
            return;
        }
        match (extend, self.selection) {
            (true, Some((anchor, _))) if self.row_hunk(anchor) == self.row_hunk(row) => {
                self.selection = Some((anchor, row));
            }
            _ => self.selection = Some((row, row)),
        }
        self.dragging = true;
        cx.notify();
    }

    pub(crate) fn extend_selection(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some((anchor, head)) = self.selection else {
            return;
        };
        if !self.dragging || head == row || self.row_lines(row).is_empty() {
            return;
        }
        if self.row_hunk(anchor) != self.row_hunk(row) {
            return;
        }
        self.selection = Some((anchor, row));
        cx.notify();
    }

    pub(crate) fn end_selection(&mut self) {
        self.dragging = false;
    }

    pub(crate) fn row_selected(&self, row: usize) -> bool {
        self.selection
            .is_some_and(|(a, b)| (a.min(b)..=a.max(b)).contains(&row))
    }

    /// The picked lines in order, and the file they are in.
    fn selected_lines(&self) -> Option<(usize, Vec<&HunkLine>)> {
        let (a, b) = self.selection?;
        let mut file_index = None;
        let mut lines = Vec::new();
        for row in a.min(b)..=a.max(b) {
            for (file, line) in self.row_lines(row) {
                file_index = Some(file);
                lines.push(line);
            }
        }
        if self.split {
            // Removed lines read before the lines that replaced them.
            lines.sort_by_key(|line| match line.kind {
                LineKind::Removed => (line.old.unwrap_or(0), 0),
                _ => (line.new.unwrap_or(0), 1),
            });
        }
        Some((file_index?, lines))
    }

    /// ⌘C over picked lines: their text, without numbers or markers.
    pub(crate) fn copy_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((_, lines)) = self.selected_lines() else {
            return false;
        };
        let text = lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        (self.host)(ChangesRequest::Copied, window, cx);
        true
    }

    // -- Comments -----------------------------------------------------------------------------

    pub(crate) fn open_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((file_index, lines)) = self.selected_lines() else {
            return;
        };
        let Some(file) = self
            .data
            .as_ref()
            .and_then(|data| data.files.get(file_index))
        else {
            return;
        };
        let Some(range) = CommentRange::from_lines(file, &lines) else {
            return;
        };
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Add a comment for the agent…")
                .auto_grow(2, 10)
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter {
                    secondary: true, ..
                } => this.save_comment(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        );
        self._subscriptions.push(subscription);
        input.update(cx, |input, cx| input.focus(window, cx));
        self.editor = Some(CommentEditor { range, input });
        self.selection = None;
        self.rebuild_rows();
        cx.notify();
    }

    pub(crate) fn cancel_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.take().is_some() {
            self.rebuild_rows();
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    pub(crate) fn save_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        let text = editor.input.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let Some(editor) = self.editor.take() else {
            return;
        };
        let id = self.next_comment_id;
        self.next_comment_id += 1;
        let comment = review_comment(&editor.range, &text, self.scope);
        self.comments.push(ChangesComment {
            id,
            range: editor.range,
            text,
            state: CommentState::Sent { at_ms: now_ms() },
            target: String::new(),
        });
        self.rebuild_rows();
        window.focus(&self.focus, cx);
        cx.notify();
        let view = cx.weak_entity();
        (self.host)(
            ChangesRequest::Comment {
                text: comment.block(),
                pill: comment.reference(),
                // The host may answer before this update returns (the page inserts at once).
                done: Box::new(move |target, cx| {
                    cx.defer(move |cx| {
                        let _ = view.update(cx, |this, cx| {
                            if let Some(comment) =
                                this.comments.iter_mut().find(|comment| comment.id == id)
                            {
                                comment.target = target;
                            }
                            cx.notify();
                        });
                    });
                }),
            },
            window,
            cx,
        );
    }

    pub(crate) fn resolve_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(comment) = self.comments.iter_mut().find(|comment| comment.id == id) {
            comment.state = CommentState::Resolved { auto: false };
        }
        self.rebuild_rows();
        cx.notify();
    }

    pub(crate) fn delete_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.comments.retain(|comment| comment.id != id);
        self.rebuild_rows();
        cx.notify();
    }

    pub(crate) fn resolved_count(&self) -> usize {
        self.comments
            .iter()
            .filter(|comment| !comment.is_open())
            .count()
    }

    pub(crate) fn toggle_show_resolved(&mut self, cx: &mut Context<Self>) {
        self.show_resolved = !self.show_resolved;
        self.rebuild_rows();
        cx.notify();
    }

    // -- Revert -------------------------------------------------------------------------------

    pub(crate) fn revert(&mut self, confirm: Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        let Some(data) = &self.data else {
            return;
        };
        let (path, hunk) = match &confirm {
            Confirm::RevertFile(path) => (path.clone(), None),
            Confirm::RevertHunk(path, index) => {
                let raw = data
                    .files
                    .iter()
                    .find(|file| &file.path == path)
                    .and_then(|file| file.hunks.get(*index))
                    .map(|hunk| hunk.raw.clone());
                (path.clone(), raw)
            }
            Confirm::Merge => return,
        };
        self.confirm = None;
        let params = json!({
            "scope": self.scope.wire(),
            "ignoreWhitespace": self.hide_whitespace,
            "filePath": path,
            "hunk": hunk,
        });
        let host = self.host.clone();
        let project_key = target.project_key.clone();
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let result = calls::git_action(
                target.remote,
                target.rpc_project_id,
                "changesRevert",
                params,
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                this.load(false, cx);
            });
            let _ = window_handle.update(cx, |_, window, cx| {
                let request = match result {
                    Ok(_) => ChangesRequest::WorkingTreeChanged { project_key },
                    Err(error) => ChangesRequest::Toast {
                        title: "Could not revert".to_string(),
                        description: Some(error.message),
                        error: true,
                    },
                };
                host(request, window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    // -- Commit -------------------------------------------------------------------------------

    /// The uncommitted files the next commit takes, or `None` for all of them.
    fn commit_paths(&self) -> Option<Vec<String>> {
        let data = self.data.as_ref()?;
        if self.excluded.is_empty() {
            return None;
        }
        Some(
            data.uncommitted_paths
                .iter()
                .filter(|path| !self.excluded.contains(*path))
                .cloned()
                .collect(),
        )
    }

    pub(crate) fn commit_count(&self) -> usize {
        self.data.as_ref().map_or(0, |data| {
            data.uncommitted_paths
                .iter()
                .filter(|path| !self.excluded.contains(*path))
                .count()
        })
    }

    pub(crate) fn toggle_included(&mut self, path: &str, cx: &mut Context<Self>) {
        if !self.excluded.remove(path) {
            self.excluded.insert(path.to_string());
        }
        cx.notify();
    }

    pub(crate) fn include_all(&mut self, cx: &mut Context<Self>) {
        self.excluded.clear();
        cx.notify();
    }

    fn take_message(&mut self, window: &mut Window, cx: &mut Context<Self>) -> String {
        let Some(input) = self.message.clone() else {
            return String::new();
        };
        let message = input.read(cx).value().to_string();
        input.update(cx, |input, cx| input.set_value("", window, cx));
        trim_trailing_spaces(&message)
    }

    pub(crate) fn run_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        if self.commit_count() == 0 {
            return;
        }
        let new_branch = self.commit_action == CommitAction::CommitPushPr
            && self
                .data
                .as_ref()
                .is_some_and(|data| data.on_default_branch);
        let file_paths = self.commit_paths();
        let message = self.take_message(window, cx);
        self.commit_menu_open = false;
        (self.host)(
            ChangesRequest::Commit {
                project_key: target.project_key,
                action: self.commit_action,
                message,
                agent_id: self.agent_id.clone(),
                file_paths,
                new_branch,
            },
            window,
            cx,
        );
        cx.notify();
    }

    pub(crate) fn run_multiple_commits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        self.commit_menu_open = false;
        (self.host)(
            ChangesRequest::MultipleCommits {
                project_key: target.project_key,
                agent_id: self.agent_id.clone(),
            },
            window,
            cx,
        );
        cx.notify();
    }

    pub(crate) fn run_merge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        self.confirm = None;
        self.commit_menu_open = false;
        let file_paths = self.commit_paths();
        let message = self.take_message(window, cx);
        (self.host)(
            ChangesRequest::MergeIntoMain {
                project_key: target.project_key,
                message,
                agent_id: self.agent_id.clone(),
                file_paths,
            },
            window,
            cx,
        );
        cx.notify();
    }

    /// Merge into main is offered whenever the branch is not the default branch.
    pub(crate) fn can_merge(&self) -> bool {
        self.data.as_ref().is_some_and(|data| {
            data.branch.is_some() && data.default_branch.is_some() && !data.on_default_branch
        })
    }

    // -- Keyboard -----------------------------------------------------------------------------

    /// Moves the cursor to the next (or previous) row matching `stop`, and scrolls to it.
    pub(crate) fn step(
        &mut self,
        forward: bool,
        stop: impl Fn(&Row) -> bool,
        cx: &mut Context<Self>,
    ) {
        let start = self
            .cursor
            .unwrap_or_else(|| self.list.logical_scroll_top().item_ix);
        let found = if forward {
            (start + 1..self.rows.len()).find(|index| stop(&self.rows[*index]))
        } else {
            (0..start).rev().find(|index| stop(&self.rows[*index]))
        };
        if let Some(row) = found {
            self.cursor = Some(row);
            self.key_file = self.current_file();
            self.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.0),
            });
            cx.notify();
        }
    }

    /// The file the cursor (or the top of the view) is in.
    pub(crate) fn current_file(&self) -> Option<String> {
        let start = self
            .cursor
            .unwrap_or_else(|| self.list.logical_scroll_top().item_ix);
        let data = self.data.as_ref()?;
        (0..=start.min(self.rows.len().saturating_sub(1)))
            .rev()
            .find_map(|index| match self.rows.get(index) {
                Some(Row::File(file)) => data.files.get(*file).map(|file| file.path.clone()),
                _ => None,
            })
    }

    /// `c`: comment on the hunk under the cursor (its first changed line).
    pub(crate) fn comment_at_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_none() {
            let start = self
                .cursor
                .unwrap_or_else(|| self.list.logical_scroll_top().item_ix);
            let row = (start..self.rows.len()).find(|row| {
                self.row_lines(*row)
                    .iter()
                    .any(|(_, line)| line.kind != LineKind::Context)
            });
            let Some(row) = row else {
                return;
            };
            self.selection = Some((row, row));
        }
        self.open_editor(window, cx);
    }
}

/// How far to scroll (positive scrolls down) so a header `top` pixels below the view's top sits
/// in the middle of a `viewport`-tall view, held back by the content there is to scroll:
/// `above` pixels above the view's top and `below` under its bottom (`None` when the end is
/// further than the list has measured, so there is room enough).
pub(crate) fn center_scroll_delta(
    top: f32,
    height: f32,
    viewport: f32,
    above: f32,
    below: Option<f32>,
) -> f32 {
    let wanted = top - (viewport - height).max(0.0) / 2.0;
    wanted
        .min(below.unwrap_or(f32::INFINITY).max(0.0))
        .max(-above.max(0.0))
}

fn same_palette(a: &ChangesHostInfo, b: &ChangesHostInfo) -> bool {
    a.palette.light == b.palette.light
        && a.palette.glass == b.palette.glass
        && a.palette.surface == b.palette.surface
        && a.palette.foreground == b.palette.foreground
}

pub(crate) fn file_hash(file: &super::model::ChangedFile) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for hunk in &file.hunks {
        hunk.raw.hash(&mut hasher);
    }
    file.binary.hash(&mut hasher);
    hasher.finish()
}

/// Commit messages carry no trailing spaces at line ends; a blank message stays empty so the agent
/// writes it.
fn trim_trailing_spaces(message: &str) -> String {
    message
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::center_scroll_delta;

    #[test]
    fn a_header_at_the_top_moves_to_the_middle() {
        // 600 tall view, 36 tall header: its top lands at (600 - 36) / 2 = 282.
        assert_eq!(center_scroll_delta(0.0, 36.0, 600.0, 5_000.0, None), -282.0);
        assert_eq!(
            center_scroll_delta(0.0, 36.0, 600.0, 5_000.0, Some(900.0)),
            -282.0
        );
    }

    #[test]
    fn a_header_below_the_middle_scrolls_up_to_it() {
        assert_eq!(
            center_scroll_delta(500.0, 36.0, 600.0, 0.0, Some(1_000.0)),
            218.0
        );
    }

    #[test]
    fn the_first_file_stays_at_the_top() {
        // Nothing above the list's start: the header cannot come further down than it is.
        assert_eq!(center_scroll_delta(0.0, 36.0, 600.0, 0.0, None), 0.0);
        assert_eq!(center_scroll_delta(0.0, 36.0, 600.0, 120.0, None), -120.0);
    }

    #[test]
    fn a_file_near_the_end_never_scrolls_past_it() {
        // Only 50 pixels of diff left under the view: scroll those and no more.
        assert_eq!(
            center_scroll_delta(500.0, 36.0, 600.0, 0.0, Some(50.0)),
            50.0
        );
        // The end already in view: stay put.
        assert_eq!(
            center_scroll_delta(500.0, 36.0, 600.0, 0.0, Some(-80.0)),
            0.0
        );
    }

    #[test]
    fn a_centred_header_stays() {
        assert_eq!(
            center_scroll_delta(282.0, 36.0, 600.0, 400.0, Some(400.0)),
            0.0
        );
    }

    #[test]
    fn a_header_taller_than_the_view_goes_to_the_top() {
        assert_eq!(center_scroll_delta(40.0, 700.0, 600.0, 400.0, None), 40.0);
    }
}

impl ChangesView {
    /// Reads the Files zoom the diff draws at and, when it changed since the last draw (a zoom
    /// chord in Files or Settings), drops the rows' measured heights: `ListState` only re-measures
    /// when its width changes, which a zoom does not.
    pub(crate) fn sync_files_zoom(&mut self) {
        let zoom = crate::ui_fonts::sync_files_zoom(
            crate::shared_settings::shared_sidebar_settings_snapshot().object(),
        );
        if self
            .drawn_files_zoom
            .replace(zoom)
            .is_some_and(|drawn| drawn != zoom)
        {
            self.list.remeasure();
        }
    }
}
