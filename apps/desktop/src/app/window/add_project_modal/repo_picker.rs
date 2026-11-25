//! Clone a repo's GitHub repository list: with GitHub picked, the Repository field opens the
//! repositories the chosen computer's `gh` can read, one line each, grouped by owner (the signed-in
//! account, then each organisation, then Shared with you), newest push first, filtered by what is
//! typed and walked with the arrow keys. A computer whose `gh` is signed out shows GitHub's
//! one-time code from that computer's `gh auth login --web` right in the list.
//!
//! CDXC:AddProject 2026-11-25 DECISION:
//! User, on the Clone a repo page (Q4961 one line, Q4962 recently pushed, Q4963 mine, orgs,
//! shared and forks, Q4964 sign in right in the list, "we must use gh cli please", Q4965 the
//! chosen computer's sign-in). The last list per computer is kept for the app's run and by that
//! computer's gxserver, so the field shows it at once and asks for a fresh one behind it. A
//! computer whose gxserver has no `listRepositories` keeps the plain text field. Typing an
//! `owner/repo`, a URL or a clone command still works for anything not in the list.
//! SEE-ALSO: server/src/source_control/repositories.rs (the requests),
//! apps/mobile/app src/addProject/repositoryList.ts (the phone's list).
use super::super::native_modal_kit::*;
use super::model::{AddProjectSourceId, describe_add_project_error};
use super::render::SHEET_WIDTH;
use super::skin::*;
use super::window::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, ClipboardItem, Context, FontWeight, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

pub(super) const ICON_GITHUB: &str = "modals/add-project/brand-github.svg";
const ICON_LOCK: &str = "titlebar/lock.svg";
const ICON_FORK: &str = "titlebar/git-fork.svg";
const ICON_USER: &str = "titlebar/user.svg";
const ICON_BUILDING: &str = "titlebar/building.svg";
const ICON_USERS: &str = "titlebar/users.svg";
const ICON_COPY: &str = "titlebar/copy.svg";
const ICON_EXTERNAL: &str = "titlebar/external-link.svg";

/// How often a waiting sign-in asks the computer whether GitHub said yes.
const SIGN_IN_POLL: Duration = Duration::from_secs(2);
/// Polls that may fail in a row (a reconnecting computer) before the sign-in reads as stopped.
const SIGN_IN_POLL_ERRORS: u32 = 5;
/// Rows drawn at once; typing narrows a longer list.
const ROWS_DRAWN: usize = 400;
const ROW_HEIGHT: f32 = 28.0;

/// One repository the computer's `gh` can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RepoItem {
    pub(super) owner: String,
    pub(super) organization: bool,
    pub(super) name: String,
    pub(super) private: bool,
    pub(super) fork: bool,
    pub(super) pushed_at: Option<String>,
}

impl RepoItem {
    pub(super) fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// A computer's repository list, as its gxserver answered.
#[derive(Clone, Debug)]
pub(super) enum RepoList {
    Loading,
    Ready {
        viewer: Option<String>,
        repos: Vec<RepoItem>,
        /// A fresh list is on its way behind the kept one.
        refreshing: bool,
    },
    SignedOut,
    /// No `gh` there: Clone a repo's Install button covers it.
    Missing,
    Failed(String),
    /// An older gxserver (or no answer): the plain text field.
    Unsupported,
}

#[derive(Clone, Debug)]
pub(super) enum SignInStep {
    Starting,
    Waiting { code: Option<String>, url: String },
    Failed(String),
}

#[derive(Clone, Debug)]
pub(super) struct RepoSignIn {
    pub(super) machine_id: String,
    pub(super) step: SignInStep,
    pub(super) poll_errors: u32,
    /// The code stays on screen while the window is in the background (the user is typing it
    /// on GitHub), until Escape or a click outside puts it away.
    pub(super) pinned: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SignInAction {
    Start,
    Status,
}

/// A row of the open list: a group's header or one repository.
#[derive(Clone, Debug)]
enum RepoRow {
    Header { title: String, kind: GroupKind },
    Repo(RepoItem),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroupKind {
    Personal,
    Organisation,
    Shared,
}

thread_local! {
    /// The last ready list per computer, for the rest of the app's run: a reopened dialog shows it
    /// at once while the fresh one loads.
    static KEPT_LISTS: RefCell<HashMap<String, (Option<String>, Vec<RepoItem>)>> =
        RefCell::new(HashMap::new());
}

fn kept_list(machine_id: &str) -> Option<(Option<String>, Vec<RepoItem>)> {
    KEPT_LISTS.with(|lists| lists.borrow().get(machine_id).cloned())
}

fn keep_list(machine_id: &str, list: Option<(Option<String>, Vec<RepoItem>)>) {
    KEPT_LISTS.with(|lists| {
        let mut lists = lists.borrow_mut();
        match list {
            Some(list) => {
                lists.insert(machine_id.to_string(), list);
            }
            None => {
                lists.remove(machine_id);
            }
        }
    });
}

/// `listRepositories`' answer. One without a `status` is an older gxserver's.
fn read_repo_list(value: &serde_json::Value) -> RepoList {
    let text = |value: &serde_json::Value, key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    match value.get("status").and_then(serde_json::Value::as_str) {
        Some("ready") => {
            let repos = value
                .get("repositories")
                .and_then(serde_json::Value::as_array)
                .map(|raw| {
                    raw.iter()
                        .filter_map(|repo| {
                            Some(RepoItem {
                                owner: text(repo, "owner")?,
                                organization: repo
                                    .get("ownerKind")
                                    .and_then(serde_json::Value::as_str)
                                    == Some("organization"),
                                name: text(repo, "name")?,
                                private: repo.get("private").and_then(serde_json::Value::as_bool)
                                    == Some(true),
                                fork: repo.get("fork").and_then(serde_json::Value::as_bool)
                                    == Some(true),
                                pushed_at: text(repo, "pushedAt"),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            RepoList::Ready {
                viewer: text(value, "viewer"),
                repos,
                refreshing: value.get("fromCache").and_then(serde_json::Value::as_bool)
                    == Some(true),
            }
        }
        Some("signedOut") => RepoList::SignedOut,
        Some("missing") => RepoList::Missing,
        Some("failed") => RepoList::Failed(
            text(value, "detail")
                .unwrap_or_else(|| "GitHub CLI couldn't list your repositories.".to_string()),
        ),
        _ => RepoList::Unsupported,
    }
}

/// `5h`, `3w`, `2mo`, `1y` since the last push.
fn pushed_ago(pushed_at: Option<&str>) -> String {
    let Some(pushed) = pushed_at.and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
    else {
        return String::new();
    };
    let seconds = (chrono::Utc::now() - pushed.with_timezone(&chrono::Utc))
        .num_seconds()
        .max(0);
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    if minutes < 60 {
        format!("{}m", minutes.max(1))
    } else if hours < 24 {
        format!("{hours}h")
    } else if days < 7 {
        format!("{days}d")
    } else if days < 30 {
        format!("{}w", days / 7)
    } else if days < 365 {
        format!("{}mo", days / 30)
    } else {
        format!("{}y", days / 365)
    }
}

/// The rows the list shows for `filter`: the account's group, then each organisation (the one
/// pushed to most recently first), then Shared with you. Inside a group the server's order (newest
/// push first) stays, except that names starting with the filter come before names containing it.
fn grouped_rows(viewer: Option<&str>, repos: &[RepoItem], filter: &str) -> (Vec<RepoRow>, usize) {
    let needle = filter.trim().to_lowercase();
    let matches =
        |repo: &RepoItem| needle.is_empty() || repo.full_name().to_lowercase().contains(&needle);
    let starts = |repo: &RepoItem| {
        !needle.is_empty()
            && (repo.name.to_lowercase().starts_with(&needle)
                || repo.full_name().to_lowercase().starts_with(&needle))
    };
    let viewer = viewer
        .map(str::to_string)
        .or_else(|| busiest_user_owner(repos));
    let is_personal = |repo: &RepoItem| {
        !repo.organization
            && viewer
                .as_deref()
                .is_some_and(|viewer| viewer.eq_ignore_ascii_case(&repo.owner))
    };
    let mut groups: Vec<(String, GroupKind, Vec<RepoItem>)> = Vec::new();
    let mut add = |title: String, kind: GroupKind, repo: &RepoItem| match groups
        .iter_mut()
        .find(|(name, k, _)| *k == kind && *name == title)
    {
        Some((_, _, items)) => items.push(repo.clone()),
        None => groups.push((title, kind, vec![repo.clone()])),
    };
    for repo in repos.iter().filter(|repo| matches(repo)) {
        if is_personal(repo) {
            add(repo.owner.clone(), GroupKind::Personal, repo);
        } else if repo.organization {
            add(repo.owner.clone(), GroupKind::Organisation, repo);
        } else {
            add("Shared with you".to_string(), GroupKind::Shared, repo);
        }
    }
    // Organisations keep the order of their newest push (the server sends newest first).
    groups.sort_by_key(|(_, kind, _)| match kind {
        GroupKind::Personal => 0,
        GroupKind::Organisation => 1,
        GroupKind::Shared => 2,
    });
    let mut rows = Vec::new();
    let mut count = 0;
    for (title, kind, mut items) in groups {
        items.sort_by_key(|repo| !starts(repo));
        rows.push(RepoRow::Header { title, kind });
        for repo in items {
            count += 1;
            if count <= ROWS_DRAWN {
                rows.push(RepoRow::Repo(repo));
            }
        }
    }
    (rows, count)
}

fn busiest_user_owner(repos: &[RepoItem]) -> Option<String> {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for repo in repos.iter().filter(|repo| !repo.organization) {
        match counts.iter_mut().find(|(owner, _)| *owner == repo.owner) {
            Some((_, count)) => *count += 1,
            None => counts.push((repo.owner.as_str(), 1)),
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(owner, _)| owner.to_string())
}

impl GpuiAddProjectModalWindow {
    fn repo_list_for_machine(&self) -> Option<&RepoList> {
        self.machine_id
            .as_ref()
            .and_then(|machine_id| self.repo_lists.get(machine_id))
    }

    /// The field lists repositories here (GitHub picked, a computer that answers the request).
    pub(super) fn repo_picker_available(&self) -> bool {
        self.sheet == Some(Sheet::Clone)
            && self.clone_source == AddProjectSourceId::Github
            && matches!(
                self.repo_list_for_machine(),
                Some(
                    RepoList::Loading
                        | RepoList::Ready { .. }
                        | RepoList::SignedOut
                        | RepoList::Failed(_)
                )
            )
    }

    /// The computer's `gh` is signed out, so the list (not the readiness hint) offers the sign-in.
    pub(super) fn repo_picker_signs_in(&self) -> bool {
        self.repo_picker_available()
            && matches!(self.repo_list_for_machine(), Some(RepoList::SignedOut))
    }

    /// The list is open under the field.
    pub(super) fn repo_list_visible(&self) -> bool {
        self.repo_picker_available()
            && self.busy.is_none()
            && (self.repo_list_open
                || self.repo_sign_in.as_ref().is_some_and(|sign_in| {
                    sign_in.pinned && Some(&sign_in.machine_id) == self.machine_id.as_ref()
                }))
    }

    pub(super) fn set_repo_list_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if open && let Some(sign_in) = self.repo_sign_in.as_mut() {
            sign_in.pinned = true;
        }
        if self.repo_list_open != open {
            self.repo_list_open = open;
            if open {
                self.repo_highlight = None;
            }
            cx.notify();
        }
    }

    /// Asks the computer for its list once Clone a repo shows GitHub: the kept list (this run's,
    /// or gxserver's) first, then a fresh one.
    pub(super) fn load_repo_list(&mut self, machine_id: &str, cx: &mut Context<Self>) {
        if self.repo_lists.contains_key(machine_id) {
            return;
        }
        match kept_list(machine_id) {
            Some((viewer, repos)) => {
                self.repo_lists.insert(
                    machine_id.to_string(),
                    RepoList::Ready {
                        viewer,
                        repos,
                        refreshing: true,
                    },
                );
                self.request_repo_list(machine_id, true, cx);
            }
            None => {
                self.repo_lists
                    .insert(machine_id.to_string(), RepoList::Loading);
                self.request_repo_list(machine_id, false, cx);
            }
        }
    }

    fn request_repo_list(&mut self, machine_id: &str, refresh: bool, cx: &mut Context<Self>) {
        self.request(
            "listRepositories",
            machine_id,
            serde_json::json!({ "provider": "github", "refresh": refresh }),
            Pending::RepoList {
                machine_id: machine_id.to_string(),
                refresh,
            },
            cx,
        );
    }

    pub(super) fn receive_repo_list(
        &mut self,
        machine_id: String,
        refresh: bool,
        result: Result<serde_json::Value, String>,
        cx: &mut Context<Self>,
    ) {
        let kept = self.repo_lists.get(&machine_id).cloned();
        let answer = match result {
            Ok(value) => read_repo_list(&value),
            Err(_) => RepoList::Unsupported,
        };
        let next = match (answer, kept) {
            (
                RepoList::Ready {
                    viewer,
                    repos,
                    refreshing,
                },
                _,
            ) => {
                keep_list(&machine_id, Some((viewer.clone(), repos.clone())));
                if refreshing && !refresh {
                    self.request_repo_list(&machine_id, true, cx);
                }
                RepoList::Ready {
                    viewer,
                    repos,
                    refreshing: refreshing && !refresh,
                }
            }
            // A fresh list that failed leaves the kept one on screen.
            (
                RepoList::Failed(_) | RepoList::Unsupported,
                Some(RepoList::Ready { viewer, repos, .. }),
            ) => RepoList::Ready {
                viewer,
                repos,
                refreshing: false,
            },
            (answer @ (RepoList::SignedOut | RepoList::Missing), _) => {
                keep_list(&machine_id, None);
                answer
            }
            (answer, _) => answer,
        };
        self.repo_lists.insert(machine_id, next);
        cx.notify();
    }

    /// Discovery found the provider's CLI after an install: the list asks again.
    pub(super) fn retry_missing_repo_list(&mut self, machine_id: &str) {
        if matches!(self.repo_lists.get(machine_id), Some(RepoList::Missing)) {
            self.repo_lists.remove(machine_id);
        }
    }

    // -- sign-in ----------------------------------------------------------------------------------

    pub(super) fn start_github_sign_in(&mut self, cx: &mut Context<Self>) {
        let Some(machine_id) = self.machine_id.clone() else {
            return;
        };
        if self.repo_sign_in.as_ref().is_some_and(|sign_in| {
            sign_in.machine_id == machine_id
                && matches!(
                    sign_in.step,
                    SignInStep::Starting | SignInStep::Waiting { .. }
                )
        }) {
            return;
        }
        self.repo_sign_in = Some(RepoSignIn {
            machine_id: machine_id.clone(),
            step: SignInStep::Starting,
            poll_errors: 0,
            pinned: true,
        });
        self.request(
            "githubSignIn",
            &machine_id,
            serde_json::json!({ "action": "start" }),
            Pending::GithubSignIn {
                machine_id: machine_id.clone(),
                action: SignInAction::Start,
            },
            cx,
        );
        cx.notify();
    }

    fn poll_github_sign_in_later(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SIGN_IN_POLL).await;
            let _ = this.update(cx, |this, cx| {
                let Some(sign_in) = this.repo_sign_in.clone() else {
                    return;
                };
                if !matches!(sign_in.step, SignInStep::Waiting { .. }) {
                    return;
                }
                this.request(
                    "githubSignIn",
                    &sign_in.machine_id,
                    serde_json::json!({ "action": "status" }),
                    Pending::GithubSignIn {
                        machine_id: sign_in.machine_id.clone(),
                        action: SignInAction::Status,
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    pub(super) fn receive_github_sign_in(
        &mut self,
        machine_id: String,
        action: SignInAction,
        result: Result<serde_json::Value, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(mut sign_in) = self
            .repo_sign_in
            .clone()
            .filter(|sign_in| sign_in.machine_id == machine_id)
        else {
            return;
        };
        let mut poll = false;
        let mut signed_in = false;
        match (action, result) {
            (SignInAction::Start, Ok(value)) => {
                let code = value
                    .get("code")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let url = value
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .filter(|url| url.starts_with("https://github.com/"))
                    .unwrap_or("https://github.com/login/device")
                    .to_string();
                sign_in.step = SignInStep::Waiting { code, url };
                poll = true;
            }
            (SignInAction::Start, Err(error)) => {
                sign_in.step = SignInStep::Failed(describe_add_project_error(
                    &error,
                    "GitHub sign-in didn't start.",
                ));
            }
            (SignInAction::Status, Ok(value)) => {
                sign_in.poll_errors = 0;
                match value.get("state").and_then(serde_json::Value::as_str) {
                    Some("done") => signed_in = true,
                    Some("waiting") => poll = true,
                    _ => {
                        sign_in.step = SignInStep::Failed(
                            "GitHub sign-in didn't finish. Try again.".to_string(),
                        );
                    }
                }
            }
            (SignInAction::Status, Err(_)) => {
                sign_in.poll_errors += 1;
                if sign_in.poll_errors >= SIGN_IN_POLL_ERRORS {
                    sign_in.step = SignInStep::Failed(
                        "Lost touch with the computer while signing in. Try again.".to_string(),
                    );
                } else {
                    poll = true;
                }
            }
        }
        if signed_in {
            self.repo_sign_in = None;
            // Discovery and the list ask again, now that `gh` is signed in.
            self.discovery.remove(&machine_id);
            self.repo_lists
                .insert(machine_id.clone(), RepoList::Loading);
            self.repo_list_open = true;
            self.request_repo_list(&machine_id, true, cx);
        } else {
            self.repo_sign_in = Some(sign_in);
            if poll {
                self.poll_github_sign_in_later(cx);
            }
        }
        cx.notify();
    }

    // -- keyboard ---------------------------------------------------------------------------------

    fn repo_rows(&self) -> (Vec<RepoRow>, usize, usize) {
        match self.repo_list_for_machine() {
            Some(RepoList::Ready { viewer, repos, .. }) => {
                let (rows, count) = grouped_rows(viewer.as_deref(), repos, &self.clone_repo_text);
                (rows, count, repos.len())
            }
            _ => (Vec::new(), 0, 0),
        }
    }

    fn repo_names(rows: &[RepoRow]) -> Vec<String> {
        rows.iter()
            .filter_map(|row| match row {
                RepoRow::Repo(repo) => Some(repo.full_name()),
                RepoRow::Header { .. } => None,
            })
            .collect()
    }

    /// The highlighted repository: the one the arrows moved to, else the first match.
    fn highlighted_repo(&self, rows: &[RepoRow]) -> Option<String> {
        let names = Self::repo_names(rows);
        self.repo_highlight
            .clone()
            .filter(|name| names.contains(name))
            .or_else(|| names.first().cloned())
    }

    /// Up and Down in the Repository field. True when the list took the key.
    pub(super) fn move_repo_highlight(&mut self, direction: isize, cx: &mut Context<Self>) -> bool {
        if !self.repo_list_visible() {
            return false;
        }
        let (rows, _, _) = self.repo_rows();
        let names = Self::repo_names(&rows);
        if names.is_empty() {
            return true;
        }
        let current = self
            .highlighted_repo(&rows)
            .and_then(|name| names.iter().position(|entry| *entry == name))
            .unwrap_or(0);
        let next = (current as isize + direction).clamp(0, names.len() as isize - 1) as usize;
        self.repo_highlight = Some(names[next].clone());
        if let Some(row) = rows
            .iter()
            .position(|row| matches!(row, RepoRow::Repo(repo) if repo.full_name() == names[next]))
        {
            self.repo_list_scroll.scroll_to_item(row);
        }
        cx.notify();
        true
    }

    /// Enter in the Repository field: the highlighted repository, or the sign-in. True when the
    /// list took the key; otherwise what is typed goes to Clone & Add as before.
    pub(super) fn press_repo_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.repo_list_visible() {
            return false;
        }
        if self.repo_picker_signs_in() {
            if !matches!(
                self.repo_sign_in.as_ref().map(|sign_in| &sign_in.step),
                Some(SignInStep::Starting | SignInStep::Waiting { .. })
            ) {
                self.start_github_sign_in(cx);
            }
            return true;
        }
        let (rows, _, _) = self.repo_rows();
        match self.highlighted_repo(&rows) {
            Some(name) if name != self.clone_repo_text.trim() => {
                self.pick_repo(name, window, cx);
                true
            }
            _ => {
                self.repo_list_open = false;
                false
            }
        }
    }

    /// Escape closes the list before the panel. True when it did.
    pub(super) fn close_repo_list(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.repo_list_visible() {
            return false;
        }
        self.dismiss_repo_list(cx);
        true
    }

    /// Escape or a click outside: the list closes, and a waiting sign-in's code with it (the
    /// field brings both back).
    fn dismiss_repo_list(&mut self, cx: &mut Context<Self>) {
        if let Some(sign_in) = self.repo_sign_in.as_mut() {
            sign_in.pinned = false;
        }
        self.repo_list_open = false;
        cx.notify();
    }

    /// Fills the field with `owner/repo`; Clone & Add looks it up as if it were typed.
    fn pick_repo(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.repo_list_open = false;
        self.repo_highlight = None;
        self.clone_repo_text = name.clone();
        self.clone_preview = None;
        self.clone_flow = None;
        self.clone_generation += 1;
        self.error = None;
        self.repo_input.update(cx, |input, cx| {
            input.set_value(name.clone(), window, cx);
            let end = name.len();
            input.set_selected_range(end..end, cx);
        });
        self.settle(window, cx);
    }

    // -- drawing ----------------------------------------------------------------------------------

    /// The list under the Repository field, when it is open.
    pub(super) fn repo_list_element(
        &mut self,
        skin: &AddProjectSkin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.repo_list_visible() {
            return None;
        }
        let list = self.repo_list_for_machine().cloned()?;
        let body: AnyElement = match list {
            RepoList::Ready {
                repos, refreshing, ..
            } => {
                let (rows, count, total) = self.repo_rows();
                let highlighted = self.highlighted_repo(&rows);
                let typed = self.clone_repo_text.trim().to_string();
                let mut column = v_flex()
                    .id("add-project-repo-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.repo_list_scroll)
                    .p(px(4.0));
                if count == 0 {
                    column = column.child(self.repo_typed_row(skin, &typed, repos.is_empty(), cx));
                }
                for row in rows {
                    column = column.child(match row {
                        RepoRow::Header { title, kind } => {
                            self.repo_group_header(skin, title, kind)
                        }
                        RepoRow::Repo(repo) => {
                            let chosen = highlighted.as_deref() == Some(repo.full_name().as_str());
                            self.repo_row(skin, repo, &typed, chosen, cx)
                        }
                    });
                }
                let status = if refreshing {
                    "Updating…".to_string()
                } else if typed.is_empty() {
                    format!("{total} repositories")
                } else {
                    format!("{count} of {total}")
                };
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .child(column)
                    .child(self.repo_list_footer(skin, status))
                    .into_any_element()
            }
            RepoList::Loading => div()
                .px(px(12.0))
                .py(px(10.0))
                .text_size(px(12.0))
                .text_color(hsla(skin.muted()))
                .child("Loading your GitHub repositories…")
                .into_any_element(),
            RepoList::Failed(detail) => v_flex()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(10.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(hsla(skin.muted()))
                        .child(detail),
                )
                .child(h_flex().child(self.filled_button(
                    skin,
                    "add-project-repo-list-retry",
                    "Try again",
                    None,
                    true,
                    false,
                    std::rc::Rc::new(
                        |this: &mut Self, _window: &mut Window, cx: &mut Context<Self>| {
                            if let Some(machine_id) = this.machine_id.clone() {
                                this.repo_lists
                                    .insert(machine_id.clone(), RepoList::Loading);
                                this.request_repo_list(&machine_id, true, cx);
                                cx.notify();
                            }
                        },
                    ),
                    cx,
                )))
                .into_any_element(),
            RepoList::SignedOut => self.repo_sign_in_card(skin, cx),
            RepoList::Missing | RepoList::Unsupported => return None,
        };
        // It floats over the rest of the panel, down to the window's bottom edge.
        let field = self.repo_field_bounds.get();
        let width = field
            .map(|field| field.size.width)
            .unwrap_or(px(SHEET_WIDTH - 32.0));
        let room = field
            .map(|field| f32::from(window.viewport_size().height - field.bottom()) - 18.0)
            .unwrap_or(260.0)
            .clamp(140.0, 420.0);
        let field_bounds = self.repo_field_bounds.clone();
        Some(
            div()
                .child(
                    gpui::deferred(
                        gpui::anchored().snap_to_window_with_margin(px(8.0)).child(
                            v_flex()
                                .id("add-project-repo-popover")
                                .occlude()
                                .w(width)
                                .max_h(px(room))
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(hsla(skin.p.menu_border))
                                .bg(hsla(if skin.p.glass {
                                    skin.p.lifted_menu_fill()
                                } else {
                                    skin.p.menu_background
                                }))
                                .shadow_lg()
                                .on_mouse_down_out(cx.listener(
                                    move |this, event: &gpui::MouseDownEvent, _window, cx| {
                                        if field_bounds
                                            .get()
                                            .is_some_and(|field| field.contains(&event.position))
                                        {
                                            return;
                                        }
                                        this.dismiss_repo_list(cx);
                                    },
                                ))
                                .child(body),
                        ),
                    )
                    .with_priority(1),
                )
                .into_any_element(),
        )
    }

    fn repo_group_header(
        &self,
        skin: &AddProjectSkin,
        title: String,
        kind: GroupKind,
    ) -> AnyElement {
        let (icon, caption) = match kind {
            GroupKind::Personal => (ICON_USER, "Personal"),
            GroupKind::Organisation => (ICON_BUILDING, "Organisation"),
            GroupKind::Shared => (ICON_USERS, ""),
        };
        h_flex()
            .h(px(ROW_HEIGHT))
            .flex_shrink_0()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .pt(px(4.0))
            .text_size(px(11.5))
            .text_color(hsla(skin.muted()))
            .child(
                div()
                    .flex_shrink_0()
                    .child(modal_icon(icon, 12.0, skin.muted())),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(skin.fg()))
                    .child(title),
            )
            .child(div().flex_1())
            .child(caption)
            .into_any_element()
    }

    fn repo_row(
        &self,
        skin: &AddProjectSkin,
        repo: RepoItem,
        typed: &str,
        chosen: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = repo.full_name();
        let hover = skin.muted_fill_at(0.7);
        let lower = repo.name.to_lowercase();
        let needle = typed.to_lowercase();
        // The typed part of the name is drawn in the strong colour.
        let label: AnyElement = match lower.find(&needle).filter(|_| !needle.is_empty()) {
            Some(start)
                if repo.name.is_char_boundary(start)
                    && repo.name.is_char_boundary(start + needle.len()) =>
            {
                h_flex()
                    .min_w_0()
                    .overflow_hidden()
                    .child(div().flex_shrink_0().child(repo.name[..start].to_string()))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(skin.p.foreground))
                            .child(repo.name[start..start + needle.len()].to_string()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(repo.name[start + needle.len()..].to_string()),
                    )
                    .into_any_element()
            }
            _ => div()
                .min_w_0()
                .truncate()
                .child(repo.name.clone())
                .into_any_element(),
        };
        let pick = name.clone();
        h_flex()
            .id(SharedString::from(format!("add-project-repo-{name}")))
            .h(px(ROW_HEIGHT))
            .flex_shrink_0()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .rounded(px(5.0))
            .text_size(px(12.5))
            .text_color(hsla(skin.fg()))
            .when(chosen, |this| this.bg(hsla(skin.muted_fill)))
            .hover(move |this| this.bg(hsla(hover)))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.pick_repo(pick.clone(), window, cx);
            }))
            .child(label)
            .when(repo.private, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .child(modal_icon(ICON_LOCK, 12.0, skin.muted())),
                )
            })
            .when(repo.fork, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .child(modal_icon(ICON_FORK, 12.0, skin.muted())),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .flex_shrink_0()
                    .pl(px(8.0))
                    .text_size(px(11.5))
                    .text_color(hsla(skin.muted()))
                    .child(pushed_ago(repo.pushed_at.as_deref())),
            )
            .into_any_element()
    }

    /// With nothing matching: the typed repository, cloned as before.
    fn repo_typed_row(
        &self,
        skin: &AddProjectSkin,
        typed: &str,
        empty_account: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if typed.is_empty() {
            return div()
                .px(px(8.0))
                .py(px(6.0))
                .text_size(px(11.5))
                .text_color(hsla(skin.muted()))
                .child(if empty_account {
                    "No repositories on this GitHub account yet."
                } else {
                    "No repositories."
                })
                .into_any_element();
        }
        v_flex()
            .child(
                h_flex()
                    .id("add-project-repo-typed")
                    .h(px(ROW_HEIGHT))
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .rounded(px(5.0))
                    .bg(hsla(skin.muted_fill))
                    .text_size(px(12.5))
                    .text_color(hsla(skin.fg()))
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.repo_list_open = false;
                        this.submit_clone_sheet(window, cx);
                    }))
                    .child(div().flex_shrink_0().child(modal_icon(ICON_GITHUB, 13.0, skin.muted())))
                    .child(div().flex_shrink_0().child("Clone"))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(typed.to_string()),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.5))
                            .text_color(hsla(skin.muted()))
                            .child("Enter"),
                    ),
            )
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(4.0))
                    .pb(px(6.0))
                    .text_size(px(11.5))
                    .line_height(px(15.0))
                    .text_color(hsla(skin.muted()))
                    .child("Not one of your repositories. Ghostex looks it up on GitHub when you pick it."),
            )
            .into_any_element()
    }

    fn repo_list_footer(&self, skin: &AddProjectSkin, status: String) -> AnyElement {
        let key = |label: &'static str| {
            div()
                .flex_shrink_0()
                .px(px(4.0))
                .rounded(px(4.0))
                .bg(hsla(skin.p.raised))
                .text_color(hsla(skin.fg()))
                .text_size(px(10.5))
                .line_height(px(16.0))
                .child(label)
        };
        h_flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.0))
            .px(px(10.0))
            .py(px(6.0))
            .border_t_1()
            .border_color(hsla(skin.p.menu_border))
            .text_size(px(11.0))
            .text_color(hsla(skin.muted()))
            .child(key("↑"))
            .child(key("↓"))
            .child(div().mr(px(6.0)).child("move"))
            .child(key("Enter"))
            .child(div().child("pick"))
            .child(div().flex_1())
            .child(status)
            .into_any_element()
    }

    /// Signed out: the computer's own `gh auth login`, with GitHub's one-time code shown here.
    fn repo_sign_in_card(&self, skin: &AddProjectSkin, cx: &mut Context<Self>) -> AnyElement {
        let machine_label = self
            .machine_id
            .as_ref()
            .and_then(|id| {
                self.machines
                    .iter()
                    .find(|machine| &machine.machine_id == id)
            })
            .map(|machine| machine.label.clone())
            .unwrap_or_else(|| "this computer".to_string());
        let step = self
            .repo_sign_in
            .as_ref()
            .filter(|sign_in| Some(&sign_in.machine_id) == self.machine_id.as_ref())
            .map(|sign_in| sign_in.step.clone());
        let title = h_flex()
            .gap(px(7.0))
            .items_center()
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(hsla(skin.p.foreground))
            .child(
                div()
                    .flex_shrink_0()
                    .child(modal_icon(ICON_GITHUB, 14.0, skin.fg())),
            )
            .child(
                div()
                    .min_w_0()
                    .child(format!("Sign in to GitHub on {machine_label}")),
            );
        let line = |text: String, bad: bool| {
            div()
                .text_size(px(11.5))
                .line_height(px(15.0))
                .text_color(hsla(if bad { skin.destructive } else { skin.muted() }))
                .child(text)
        };
        let mut card = v_flex().gap(px(8.0)).p(px(10.0)).child(title);
        match step {
            Some(SignInStep::Waiting { code, url }) => {
                card = card.child(line(
                    "Enter this code on GitHub to list your repositories here.".to_string(),
                    false,
                ));
                if let Some(code) = code {
                    let copied = code.clone();
                    let hover = skin.muted_fill_at(0.7);
                    card = card.child(
                        h_flex().child(
                            h_flex()
                                .gap(px(10.0))
                                .items_center()
                                .px(px(10.0))
                                .py(px(5.0))
                                .rounded(px(6.0))
                                .border_1()
                                .border_color(hsla(skin.bar_border))
                                .bg(hsla(skin.bar_background))
                                .child(
                                    div()
                                        .font_family(MODAL_MONO_FONT)
                                        .text_size(px(15.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(hsla(skin.p.foreground))
                                        .child(code),
                                )
                                .child(
                                    div()
                                        .id("add-project-github-code-copy")
                                        .p(px(2.0))
                                        .rounded(px(4.0))
                                        .hover(move |this| this.bg(hsla(hover)))
                                        .on_mouse_down(MouseButton::Left, |_, window, _| {
                                            window.prevent_default()
                                        })
                                        .on_click(move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                copied.clone(),
                                            ));
                                        })
                                        .child(modal_icon(ICON_COPY, 13.0, skin.muted())),
                                ),
                        ),
                    );
                }
                card = card
                    .child(h_flex().child(self.filled_button(
                        skin,
                        "add-project-github-open",
                        "Open github.com",
                        Some(ICON_EXTERNAL),
                        true,
                        false,
                        std::rc::Rc::new(
                            move |_this: &mut Self,
                                  _window: &mut Window,
                                  cx: &mut Context<Self>| {
                                cx.open_url(&url);
                            },
                        ),
                        cx,
                    )))
                    .child(line("Waiting for GitHub…".to_string(), false));
            }
            Some(SignInStep::Starting) => {
                card = card.child(line("Starting the GitHub CLI sign-in…".to_string(), false));
            }
            step => {
                if let Some(SignInStep::Failed(error)) = step {
                    card = card.child(line(error, true));
                } else {
                    card = card.child(line(
                        "Ghostex runs the GitHub CLI's sign-in on that computer, then lists your repositories here."
                            .to_string(),
                        false,
                    ));
                }
                card = card.child(h_flex().child(self.filled_button(
                    skin,
                    "add-project-github-sign-in",
                    "Sign in",
                    None,
                    true,
                    false,
                    std::rc::Rc::new(
                        |this: &mut Self, _window: &mut Window, cx: &mut Context<Self>| {
                            this.start_github_sign_in(cx);
                        },
                    ),
                    cx,
                )));
            }
        }
        card.into_any_element()
    }
}
