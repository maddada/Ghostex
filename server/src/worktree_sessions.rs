/*
CDXC:SidebarV2Worktrees 2026-07-29-00:00:
Sidebar V2 treats a worktree as an ATTRIBUTE of a session (its cwd plus the
branch checked out there), never as a registered sibling project. This module
owns the parts of that model that are pure or process-local:

- the temp-branch vocabulary (`ghostex/<8 lowercase hex>`) and the worktree
  directory naming that pairs with it,
- the marker gxserver stamps on a session it created inside a worktree, which is
  the ONLY authority the later branch auto-rename trusts,
- the small, time-boxed git plumbing those two need.

The endpoint orchestration (`/api/createWorktreeSession`,
`/api/removeSessionWorktree`) lives in `server.rs` next to the other worktree
endpoints, because it composes registered-project scope resolution, the typed
operation command builders, and the ordinary session-creation machinery.
*/

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rand::Rng;
use serde_json::{json, Map, Value};

use crate::session_git_status::{resolve_default_branch, DefaultBranch};

/// Every branch gxserver creates for a worktree session starts here.
pub const WORKTREE_TEMP_BRANCH_PREFIX: &str = "ghostex/";
/// `ghostex/<8 lowercase hex>` — the temporary name a worktree session starts on.
pub const WORKTREE_TEMP_BRANCH_SUFFIX_LENGTH: usize = 8;
/// Runtime-settings key holding the marker described at the top of this file.
pub const WORKTREE_SESSION_RUNTIME_KEY: &str = "worktreeSession";
/// Cadence of the temp-branch auto-rename pass.
pub const WORKTREE_BRANCH_RENAME_SWEEP_INTERVAL_SECONDS: u64 = 60;
/// At most this many branches are renamed per pass, so a machine that somehow
/// accumulated many pending renames still spends bounded time per minute.
pub const WORKTREE_BRANCH_RENAME_MAX_PER_PASS: usize = 8;

const RENAMED_BRANCH_SLUG_MAX_CHARS: usize = 48;
const BRANCH_COLLISION_ATTEMPTS: usize = 20;
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Local git plumbing (rev-parse, symbolic-ref, branch -m) is fast on a healthy
/// repository; anything slower is a wedged repository, not work worth waiting on.
pub const WORKTREE_GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
/// `git fetch origin` talks to a network remote, so it gets a real budget.
pub const WORKTREE_FETCH_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

// ---------------------------------------------------------------------------
// temp-branch vocabulary
// ---------------------------------------------------------------------------

/// A fresh 8-character lowercase hex suffix. Randomness is what makes a retry
/// after a failed create collide with neither a leftover branch nor a leftover
/// directory; cleanup still runs, this is the belt to that suspenders.
pub fn create_temp_branch_suffix() -> String {
    let mut rng = rand::thread_rng();
    (0..WORKTREE_TEMP_BRANCH_SUFFIX_LENGTH)
        .map(|_| {
            let digit: u8 = rng.gen_range(0..16);
            char::from_digit(u32::from(digit), 16).unwrap_or('0')
        })
        .collect()
}

pub fn temp_branch_name(suffix: &str) -> String {
    format!("{WORKTREE_TEMP_BRANCH_PREFIX}{suffix}")
}

/// True only for the exact `ghostex/<8 lowercase hex>` shape this module mints.
/// A branch a human named `ghostex/my-fix` is deliberately NOT a temp branch:
/// auto-rename and rollback branch deletion both key off this.
pub fn is_worktree_temp_branch(branch: &str) -> bool {
    let Some(suffix) = branch.strip_prefix(WORKTREE_TEMP_BRANCH_PREFIX) else {
        return false;
    };
    suffix.len() == WORKTREE_TEMP_BRANCH_SUFFIX_LENGTH
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// A branch gxserver is allowed to delete as part of worktree cleanup: either a
/// still-unnamed temp branch or the `ghostex/<slug>` it was renamed to.
pub fn is_managed_worktree_branch(branch: &str) -> bool {
    if is_worktree_temp_branch(branch) {
        return true;
    }
    let Some(slug) = branch.strip_prefix(WORKTREE_TEMP_BRANCH_PREFIX) else {
        return false;
    };
    !slug.is_empty()
        && slug != "automation"
        && !slug.starts_with("automation/")
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Worktrees stay siblings of the project directory, matching the existing
/// `createProjectWorktree` convention (`<project>-<name>`), so the typed
/// operation path guard ("inside the worktree family directory") accepts them.
pub fn worktree_directory_name(project_folder_name: &str, suffix: &str) -> String {
    let folder = project_folder_name.trim();
    if folder.is_empty() {
        format!("worktree-{suffix}")
    } else {
        format!("{folder}-{suffix}")
    }
}

/*
CDXC:WorktreeRename 2026-08-09-18:40:
Renaming a worktree types ONE name that becomes two things: the branch keeps it
verbatim (so `feat/kanban-assignee` is possible) while the folder gets this slug,
because `/` cannot be part of a directory name. This mirrors
`worktreeRenameFolderSlug` in `packages/shared/worktree-rename-name.ts` — the daemon slugs
the name itself rather than accepting a destination path from a renderer, so a
client can never point the move at a directory the user did not name. Case is
preserved on purpose: `slugify_branch_title` above lowercases because it slugs a
sentence, and lowercasing a name the user typed would rename their folder to
something they did not ask for.
*/
pub fn worktree_rename_folder_slug(name: &str) -> String {
    let mut slug = String::new();
    for character in name.trim().chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
            slug.push(character);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.len() <= RENAMED_BRANCH_SLUG_MAX_CHARS {
        return slug;
    }
    let cut = &slug[..RENAMED_BRANCH_SLUG_MAX_CHARS];
    let truncated = match cut.rfind('-') {
        Some(boundary) if boundary > 0 => &cut[..boundary],
        _ => cut,
    };
    truncated.trim_end_matches('-').to_string()
}

/// `origin/main`, `refs/remotes/origin/main` and `main` all name the same branch
/// on the remote; `startFromOrigin` needs the bare name to look it up.
pub fn base_branch_short_name(base_ref: &str) -> String {
    let trimmed = base_ref.trim();
    trimmed
        .strip_prefix("refs/remotes/origin/")
        .or_else(|| trimmed.strip_prefix("refs/heads/"))
        .or_else(|| trimmed.strip_prefix("origin/"))
        .unwrap_or(trimmed)
        .to_string()
}

// ---------------------------------------------------------------------------
// title slugs
// ---------------------------------------------------------------------------

/// `"Fix the flaky login test"` → `"fix-the-flaky-login-test"`. Restricted to
/// `[a-z0-9-]` so the result is always a legal ref component; `None` when a
/// title carries no usable characters at all (emoji-only, CJK-only, …).
pub fn slugify_branch_title(title: &str) -> Option<String> {
    let mut slug = String::new();
    for character in title.trim().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        return None;
    }
    let truncated = if slug.len() <= RENAMED_BRANCH_SLUG_MAX_CHARS {
        slug
    } else {
        let cut = &slug[..RENAMED_BRANCH_SLUG_MAX_CHARS];
        match cut.rfind('-') {
            Some(boundary) if boundary > 0 => cut[..boundary].to_string(),
            _ => cut.to_string(),
        }
    };
    let truncated = truncated.trim_matches('-').to_string();
    (!truncated.is_empty()).then_some(truncated)
}

/// The `ghostex/<slug>` name to rename onto, with a numeric suffix when the
/// obvious name is taken. `None` when the title yields no slug, or when every
/// candidate collides.
pub fn resolve_renamed_branch_name(
    title: &str,
    current_branch: &str,
    branch_exists: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let slug = slugify_branch_title(title)?;
    for index in 0..BRANCH_COLLISION_ATTEMPTS {
        let candidate = if index == 0 {
            format!("{WORKTREE_TEMP_BRANCH_PREFIX}{slug}")
        } else {
            format!("{WORKTREE_TEMP_BRANCH_PREFIX}{slug}-{}", index + 1)
        };
        if candidate == current_branch {
            return None;
        }
        if !branch_exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// session marker
// ---------------------------------------------------------------------------

/// What gxserver remembers about a session it started inside a worktree it made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeSessionMarker {
    pub branch: String,
    pub initial_title: String,
    pub path: String,
    pub renamed_at: Option<String>,
}

pub fn worktree_session_marker_value(
    branch: &str,
    path: &str,
    initial_title: &str,
    created_at: &str,
) -> Value {
    json!({
        "branch": branch,
        "createdAt": created_at,
        "initialTitle": initial_title,
        "path": path,
    })
}

pub fn read_worktree_session_marker(session: &Value) -> Option<WorktreeSessionMarker> {
    let marker = session
        .get("runtimeSettings")
        .and_then(Value::as_object)?
        .get(WORKTREE_SESSION_RUNTIME_KEY)
        .and_then(Value::as_object)?;
    let text = |key: &str| {
        marker
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some(WorktreeSessionMarker {
        branch: text("branch")?,
        initial_title: text("initialTitle").unwrap_or_default(),
        path: text("path")?,
        renamed_at: text("renamedAt"),
    })
}

/// The rename this session is due, or `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeBranchRenamePlan {
    pub from_branch: String,
    pub project_id: String,
    pub session_id: String,
    pub title: String,
    pub worktree_path: String,
}

/*
Auto-rename gate. All of these must hold, and they are deliberately strict:
renaming a branch is a mutation on the user's repository, so gxserver only ever
touches a branch it minted itself, on a session it created, that still carries
its original placeholder title source, and only once.

- the session carries this module's marker (so the branch is ours),
- the recorded branch is still the `ghostex/<8hex>` temp shape (a branch already
  renamed, or one the user renamed by hand, is finished business),
- the session has a REAL title: different from the one gxserver created it with,
  and no longer the placeholder title source every fresh row starts on.
*/
pub fn plan_worktree_branch_rename(session: &Value) -> Option<WorktreeBranchRenamePlan> {
    let marker = read_worktree_session_marker(session)?;
    if marker.renamed_at.is_some() || !is_worktree_temp_branch(&marker.branch) {
        return None;
    }
    let title = session
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())?;
    if title == marker.initial_title {
        return None;
    }
    let title_source = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("titleSource"))
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("placeholder");
    if title_source.is_empty() || title_source == "placeholder" {
        return None;
    }
    Some(WorktreeBranchRenamePlan {
        from_branch: marker.branch,
        project_id: session
            .get("projectId")
            .and_then(Value::as_str)?
            .to_string(),
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)?
            .to_string(),
        title: title.to_string(),
        worktree_path: marker.path,
    })
}

/// The session's runtime settings with `mutate` applied to its V2 worktree
/// marker, ready to hand to `update_session`. `None` when the session carries no
/// marker, which is the signal that it is not a worktree session at all — the
/// one shape both marker rewrites below share.
fn runtime_settings_with_mutated_worktree_marker(
    session: &Value,
    mutate: impl FnOnce(&mut Map<String, Value>),
) -> Option<Map<String, Value>> {
    let mut runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut marker = runtime_settings
        .get(WORKTREE_SESSION_RUNTIME_KEY)
        .and_then(Value::as_object)
        .cloned()?;
    mutate(&mut marker);
    runtime_settings.insert(
        WORKTREE_SESSION_RUNTIME_KEY.to_string(),
        Value::Object(marker),
    );
    Some(runtime_settings)
}

/// The session's runtime settings with the marker's branch replaced and the
/// rename stamped, ready to hand to `update_session`.
pub fn runtime_settings_with_renamed_worktree_branch(
    session: &Value,
    branch: &str,
    renamed_at: &str,
) -> Option<Map<String, Value>> {
    runtime_settings_with_mutated_worktree_marker(session, |marker| {
        marker.insert("branch".to_string(), json!(branch));
        marker.insert("renamedAt".to_string(), json!(renamed_at));
    })
}

/*
CDXC:WorktreeRename 2026-08-09-18:40:
A moved worktree leaves the marker's `path` pointing at a folder that no longer
exists, and `run_worktree_git` no-ops on a non-directory cwd — so a stale marker
silently disables that session's auto-rename forever, and `remove_session_worktree`
rejects the published path and strands the checkout. Rewrite `path` (and `branch`
in the same pass when the rename covered the branch too, so the caller writes one
`update_session` per session instead of two) and leave every other marker field
alone: `initialTitle`, `createdAt`, and `renamedAt` still mean what they meant.
*/
pub fn runtime_settings_with_moved_worktree_path(
    session: &Value,
    path: &str,
    branch: Option<&str>,
) -> Option<Map<String, Value>> {
    runtime_settings_with_mutated_worktree_marker(session, |marker| {
        marker.insert("path".to_string(), json!(path));
        if let Some(branch) = branch {
            marker.insert("branch".to_string(), json!(branch));
        }
    })
}

// ---------------------------------------------------------------------------
// git plumbing
// ---------------------------------------------------------------------------

/*
A time-boxed `git` capture, shaped like the session-git-status probe's: stdout is
drained by a helper thread for the child's whole lifetime (a poll-then-read loop
deadlocks against a child blocked on a full pipe), optional locks are disabled so
gxserver never contends with the user's own git, and terminal prompts are off so
a credential prompt cannot wedge the daemon. A timeout kills the child and reads
as an ordinary failure.
*/
pub fn run_worktree_git(cwd: &str, args: &[&str], timeout: Duration) -> Option<String> {
    if cwd.trim().is_empty() || !Path::new(cwd).is_dir() {
        return None;
    }
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0");
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(COMMAND_POLL_INTERVAL);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let output = reader.join().unwrap_or_default();
    if !status?.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output).trim().to_string())
}

/// The repository default branch, resolved by P3's shared rules (origin/HEAD →
/// origin/main|master → local main|master).
pub fn resolve_repository_default_branch(repository_path: &str) -> Option<DefaultBranch> {
    resolve_default_branch(&|args| {
        run_worktree_git(repository_path, args, WORKTREE_GIT_COMMAND_TIMEOUT)
    })
}

pub fn worktree_branch_exists(repository_path: &str, branch: &str) -> bool {
    run_worktree_git(
        repository_path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
        WORKTREE_GIT_COMMAND_TIMEOUT,
    )
    .map(|value| !value.trim().is_empty())
    .unwrap_or(false)
}

/*
CDXC:WorktreeRename 2026-08-09-18:40:
`refs/heads/<x>` cannot be a ref while `refs/heads/<x>/…` holds refs, and vice
versa: with `test/board-batch` present, `git branch -m test/other test` fails with
`error: 'refs/heads/test/board-batch' exists; cannot create 'refs/heads/test'`
even though `git check-ref-format` passes `test` and no branch called `test`
exists. `-M` does not help — force cannot dissolve a ref namespace. So the only
way to give the user a sentence instead of a raw git failure is to probe both
directions before running the rename: does the target name have refs *under* it,
and is any *ancestor* of the target already a leaf ref. Returns the first
blocking refname with `refs/heads/` stripped.

The probe deliberately lives here rather than as a new `for-each-ref` git action:
that would be a general ref-listing primitive on the typed-operation surface, and
this needs exactly two bounded lookups.
*/
pub fn worktree_branch_namespace_blocker(repository_path: &str, branch: &str) -> Option<String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return None;
    }
    if let Some(found) = run_worktree_git(
        repository_path,
        &[
            "for-each-ref",
            "--count=1",
            "--format=%(refname)",
            &format!("refs/heads/{branch}/**"),
        ],
        WORKTREE_GIT_COMMAND_TIMEOUT,
    ) {
        let found = found.trim();
        if !found.is_empty() {
            return Some(
                found
                    .strip_prefix("refs/heads/")
                    .unwrap_or(found)
                    .to_string(),
            );
        }
    }
    /*
    The ancestor direction cannot reuse `for-each-ref`: git matches a pattern
    "completely or from the beginning up to a slash", so `refs/heads/test`
    happily returns `refs/heads/test/board-batch` and every namespaced name
    would report itself as its own blocker. Resolve each ancestor exactly.
    */
    let mut prefix = String::new();
    for component in branch.split('/').filter(|component| !component.is_empty()) {
        if prefix.is_empty() {
            prefix = component.to_string();
        } else {
            prefix = format!("{prefix}/{component}");
        }
        if prefix == branch {
            break;
        }
        if worktree_branch_exists(repository_path, &prefix) {
            return Some(prefix);
        }
    }
    None
}

/*
CDXC:WorktreeRename 2026-08-09-18:40:
`git worktree move` hard-refuses a worktree with POPULATED submodules
(`fatal: working trees containing submodules cannot be moved or removed`); an
uninitialised gitlink is fine. `mv` plus `git worktree repair` is not a
workaround — the top level repairs and the submodules do not, leaving a checkout
whose `git status` fails in a way the user will not notice for days. So detect
the condition and refuse with a sentence naming the fix. `submodule status`
prints one line per gitlink and prefixes uninitialised ones with `-`, which is
exactly the populated/not-populated split git itself refuses on.
*/
pub fn worktree_has_populated_submodules(worktree_path: &str) -> bool {
    let Some(status) = run_worktree_git(
        worktree_path,
        &["submodule", "status", "--recursive"],
        WORKTREE_GIT_COMMAND_TIMEOUT,
    ) else {
        return false;
    };
    status
        .lines()
        .filter(|line| !line.trim().is_empty())
        .any(|line| !line.starts_with('-'))
}

/*
CDXC:WorktreeRename 2026-08-09-18:40:
`git worktree move` refuses a locked worktree with
`fatal: cannot move a locked working tree`, and the override is `move -f -f`,
which this feature deliberately does not offer — a lock is someone saying "do not
touch this checkout". Read the lock straight off `worktree list --porcelain`,
whose per-worktree block carries a bare `locked` line (or `locked <reason>`),
rather than forwarding git's stderr — which typed-operation results do not carry
to the user anyway. The reason itself is deliberately dropped: the caller's
refusal is a fixed sentence, so this answers only whether a lock is there.
*/
pub fn worktree_is_locked(repository_path: &str, worktree_path: &str) -> bool {
    let Some(listing) = run_worktree_git(
        repository_path,
        &["worktree", "list", "--porcelain"],
        WORKTREE_GIT_COMMAND_TIMEOUT,
    ) else {
        return false;
    };
    let target = worktree_path.trim_end_matches('/');
    let mut in_target_block = false;
    for line in listing.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            in_target_block = path.trim().trim_end_matches('/') == target;
            continue;
        }
        if in_target_block && (line == "locked" || line.starts_with("locked ")) {
            return true;
        }
    }
    false
}

pub fn current_worktree_branch(worktree_path: &str) -> Option<String> {
    run_worktree_git(
        worktree_path,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        WORKTREE_GIT_COMMAND_TIMEOUT,
    )
    .map(|value| value.trim().to_string())
    .filter(|branch| !branch.is_empty())
}

/// `git fetch origin` in the repository the worktree will be cut from.
pub fn fetch_worktree_origin(repository_path: &str) -> bool {
    run_worktree_git(
        repository_path,
        &["fetch", "origin"],
        WORKTREE_FETCH_COMMAND_TIMEOUT,
    )
    .is_some()
}

/// The commit `origin/<base>` points at, which is what "start from origin"
/// means: base the new branch on the REMOTE tip, not on whatever the local
/// branch happens to be.
pub fn resolve_origin_base_commit(repository_path: &str, base_branch: &str) -> Option<String> {
    let short = base_branch_short_name(base_branch);
    if short.is_empty() {
        return None;
    }
    run_worktree_git(
        repository_path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/remotes/origin/{short}^{{commit}}"),
        ],
        WORKTREE_GIT_COMMAND_TIMEOUT,
    )
    .map(|value| value.trim().to_string())
    .filter(|commit| !commit.is_empty())
}

/// Renames the branch checked out in a worktree. Returns false on any failure;
/// the caller leaves the marker untouched so the next pass can retry.
pub fn rename_worktree_branch(worktree_path: &str, from_branch: &str, to_branch: &str) -> bool {
    run_worktree_git(
        worktree_path,
        &["branch", "-m", from_branch, to_branch],
        WORKTREE_GIT_COMMAND_TIMEOUT,
    )
    .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_branch_names_are_eight_lowercase_hex_characters() {
        for _ in 0..64 {
            let suffix = create_temp_branch_suffix();
            assert_eq!(suffix.len(), 8);
            assert!(suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
            assert!(is_worktree_temp_branch(&temp_branch_name(&suffix)));
        }
    }

    #[test]
    fn only_the_exact_temp_shape_counts_as_a_temp_branch() {
        assert!(is_worktree_temp_branch("ghostex/0123abcd"));
        assert!(!is_worktree_temp_branch("ghostex/0123ABCD"));
        assert!(!is_worktree_temp_branch("ghostex/0123abc"));
        assert!(!is_worktree_temp_branch("ghostex/0123abcde"));
        assert!(!is_worktree_temp_branch("ghostex/my-fix"));
        assert!(!is_worktree_temp_branch("feature/0123abcd"));
        assert!(!is_worktree_temp_branch("main"));
    }

    #[test]
    fn managed_branches_cover_renamed_slugs_but_not_foreign_or_automation_branches() {
        assert!(is_managed_worktree_branch("ghostex/0123abcd"));
        assert!(is_managed_worktree_branch("ghostex/fix-the-login-test"));
        assert!(!is_managed_worktree_branch("ghostex/automation/abc"));
        assert!(!is_managed_worktree_branch("ghostex/Mixed-Case"));
        assert!(!is_managed_worktree_branch("ghostex/"));
        assert!(!is_managed_worktree_branch("main"));
        assert!(!is_managed_worktree_branch("release/1.0"));
    }

    #[test]
    fn worktree_directories_stay_siblings_named_after_the_project() {
        assert_eq!(
            worktree_directory_name("Ghostex", "0123abcd"),
            "Ghostex-0123abcd"
        );
        assert_eq!(
            worktree_directory_name("  ", "0123abcd"),
            "worktree-0123abcd"
        );
    }

    #[test]
    fn rename_folder_slugs_fold_separators_without_lowercasing() {
        /*
        CDXC:WorktreeRename 2026-08-09-18:40:
        Same table as `packages/shared/worktree-rename-name.test.ts`. The daemon computes
        the destination folder itself, so if these two rules ever drift the
        modal's live preview stops describing the folder the user actually gets.
        */
        assert_eq!(
            worktree_rename_folder_slug("feat/kanban-assignee"),
            "feat-kanban-assignee"
        );
        assert_eq!(worktree_rename_folder_slug("feat/UI-Polish"), "feat-UI-Polish");
        assert_eq!(worktree_rename_folder_slug("a-b_c.d"), "a-b_c.d");
        assert_eq!(worktree_rename_folder_slug("  feat/x  "), "feat-x");
        // The empty result is load-bearing: `worktree_rename_destination_path`
        // reads it as "this name cannot become a folder" and refuses the rename.
        assert_eq!(worktree_rename_folder_slug("🎉🎉"), "");
        let long = worktree_rename_folder_slug(
            "rewrite-the-entire-presentation-snapshot-projection-pipeline-for-sidebar",
        );
        assert!(long.len() <= RENAMED_BRANCH_SLUG_MAX_CHARS);
        assert!(!long.ends_with('-'));
        assert!(long.starts_with("rewrite-the-entire-presentation-snapshot"));
    }

    #[test]
    fn base_branch_short_names_drop_every_remote_prefix_form() {
        assert_eq!(base_branch_short_name("main"), "main");
        assert_eq!(base_branch_short_name("origin/main"), "main");
        assert_eq!(base_branch_short_name("refs/remotes/origin/main"), "main");
        assert_eq!(
            base_branch_short_name("refs/heads/release/1.0"),
            "release/1.0"
        );
    }

    #[test]
    fn slugs_are_lowercase_hyphenated_and_bounded() {
        assert_eq!(
            slugify_branch_title("Fix the flaky login test"),
            Some("fix-the-flaky-login-test".to_string())
        );
        assert_eq!(
            slugify_branch_title("  Add __CACHE__ v2!! "),
            Some("add-cache-v2".to_string())
        );
        assert_eq!(slugify_branch_title("🎉🎉"), None);
        assert_eq!(slugify_branch_title(""), None);
        let long = slugify_branch_title(
            "Rewrite the entire presentation snapshot projection pipeline for sidebar v2",
        )
        .expect("slug");
        assert!(long.len() <= RENAMED_BRANCH_SLUG_MAX_CHARS);
        assert!(!long.ends_with('-'));
        assert!(long.starts_with("rewrite-the-entire-presentation-snapshot"));
    }

    #[test]
    fn renamed_branches_get_a_numeric_suffix_when_the_obvious_name_is_taken() {
        let taken = |branch: &str| branch == "ghostex/fix-login" || branch == "ghostex/fix-login-2";
        assert_eq!(
            resolve_renamed_branch_name("Fix login", "ghostex/0123abcd", &|_| false),
            Some("ghostex/fix-login".to_string())
        );
        assert_eq!(
            resolve_renamed_branch_name("Fix login", "ghostex/0123abcd", &taken),
            Some("ghostex/fix-login-3".to_string())
        );
        assert_eq!(
            resolve_renamed_branch_name("🎉", "ghostex/0123abcd", &|_| false),
            None
        );
        assert_eq!(
            resolve_renamed_branch_name("fix login", "ghostex/fix-login", &|_| false),
            None
        );
        assert_eq!(
            resolve_renamed_branch_name("Fix login", "ghostex/0123abcd", &|_| true),
            None
        );
    }

    fn worktree_session(title: &str, marker: Value, title_source: &str) -> Value {
        json!({
            "projectId": "P1ab",
            "runtimeSettings": {
                "titleSource": title_source,
                WORKTREE_SESSION_RUNTIME_KEY: marker,
            },
            "sessionId": "G1ab",
            "title": title,
        })
    }

    fn temp_marker() -> Value {
        worktree_session_marker_value(
            "ghostex/0123abcd",
            "/repos/project-0123abcd",
            "Codex Session",
            "2026-07-29T00:00:00.000Z",
        )
    }

    #[test]
    fn a_real_title_on_a_temp_branch_plans_a_rename() {
        let plan =
            plan_worktree_branch_rename(&worktree_session("Fix login", temp_marker(), "generated"))
                .expect("plan");
        assert_eq!(plan.from_branch, "ghostex/0123abcd");
        assert_eq!(plan.title, "Fix login");
        assert_eq!(plan.worktree_path, "/repos/project-0123abcd");
        assert_eq!(plan.project_id, "P1ab");
        assert_eq!(plan.session_id, "G1ab");
    }

    #[test]
    fn placeholder_unchanged_already_renamed_and_foreign_branches_plan_nothing() {
        assert_eq!(
            plan_worktree_branch_rename(&worktree_session(
                "Fix login",
                temp_marker(),
                "placeholder"
            )),
            None,
            "a placeholder title source is not a real title"
        );
        assert_eq!(
            plan_worktree_branch_rename(&worktree_session(
                "Codex Session",
                temp_marker(),
                "generated"
            )),
            None,
            "the title gxserver created the row with is not a rename"
        );
        let mut renamed = temp_marker();
        renamed
            .as_object_mut()
            .expect("marker")
            .insert("renamedAt".to_string(), json!("2026-07-29T01:00:00.000Z"));
        assert_eq!(
            plan_worktree_branch_rename(&worktree_session("Fix login", renamed, "generated")),
            None,
            "a branch is renamed at most once"
        );
        let human_branch = worktree_session_marker_value(
            "feature/login",
            "/repos/project-0123abcd",
            "Codex Session",
            "2026-07-29T00:00:00.000Z",
        );
        assert_eq!(
            plan_worktree_branch_rename(&worktree_session("Fix login", human_branch, "generated")),
            None,
            "only branches gxserver minted are renamed"
        );
        assert_eq!(
            plan_worktree_branch_rename(&json!({
                "projectId": "P1ab",
                "sessionId": "G1ab",
                "title": "Fix login",
            })),
            None,
            "a session without the marker is not a worktree session"
        );
    }

    #[test]
    fn applying_a_rename_updates_the_marker_in_place() {
        let session = worktree_session("Fix login", temp_marker(), "generated");
        let runtime_settings = runtime_settings_with_renamed_worktree_branch(
            &session,
            "ghostex/fix-login",
            "2026-07-29T02:00:00.000Z",
        )
        .expect("runtime settings");
        let marker = runtime_settings
            .get(WORKTREE_SESSION_RUNTIME_KEY)
            .and_then(Value::as_object)
            .expect("marker");
        assert_eq!(marker.get("branch"), Some(&json!("ghostex/fix-login")));
        assert_eq!(
            marker.get("renamedAt"),
            Some(&json!("2026-07-29T02:00:00.000Z"))
        );
        assert_eq!(marker.get("initialTitle"), Some(&json!("Codex Session")));
        assert_eq!(
            runtime_settings.get("titleSource"),
            Some(&json!("generated")),
            "unrelated runtime settings survive"
        );
        assert_eq!(
            plan_worktree_branch_rename(&json!({
                "projectId": "P1ab",
                "runtimeSettings": Value::Object(runtime_settings),
                "sessionId": "G1ab",
                "title": "Fix login",
            })),
            None,
            "the updated marker is no longer due a rename"
        );
    }

    #[test]
    fn moving_a_worktree_marker_keeps_every_other_field() {
        let session = worktree_session("Fix login", temp_marker(), "generated");
        let runtime_settings =
            runtime_settings_with_moved_worktree_path(&session, "/repos/project-renamed", None)
                .expect("runtime settings");
        let marker = runtime_settings
            .get(WORKTREE_SESSION_RUNTIME_KEY)
            .and_then(Value::as_object)
            .expect("marker");
        assert_eq!(marker.get("path"), Some(&json!("/repos/project-renamed")));
        assert_eq!(marker.get("branch"), Some(&json!("ghostex/0123abcd")));
        assert_eq!(marker.get("initialTitle"), Some(&json!("Codex Session")));
        assert_eq!(
            marker.get("createdAt"),
            Some(&json!("2026-07-29T00:00:00.000Z"))
        );
        assert_eq!(
            runtime_settings.get("titleSource"),
            Some(&json!("generated")),
            "unrelated runtime settings survive"
        );
        assert_eq!(
            runtime_settings_with_moved_worktree_path(
                &json!({ "runtimeSettings": { "titleSource": "generated" } }),
                "/repos/project-renamed",
                None
            ),
            None,
            "a session without the marker has nothing to move"
        );
    }

    #[test]
    fn moving_a_marker_can_carry_the_branch_rename_in_the_same_pass() {
        let session = worktree_session("Fix login", temp_marker(), "generated");
        let runtime_settings = runtime_settings_with_moved_worktree_path(
            &session,
            "/repos/project-renamed",
            Some("feat/login"),
        )
        .expect("runtime settings");
        let marker = runtime_settings
            .get(WORKTREE_SESSION_RUNTIME_KEY)
            .and_then(Value::as_object)
            .expect("marker");
        assert_eq!(marker.get("path"), Some(&json!("/repos/project-renamed")));
        assert_eq!(marker.get("branch"), Some(&json!("feat/login")));
        assert_eq!(marker.get("initialTitle"), Some(&json!("Codex Session")));
    }

    #[test]
    fn a_moved_marker_still_plans_no_rename_for_a_human_branch() {
        /*
        CDXC:WorktreeRename 2026-08-09-18:40:
        Moving a worktree must not hand the auto-rename sweep a branch it never
        minted. A marker whose branch the user renamed by hand stays finished
        business after the move, and a marker moved onto a temp branch is still
        only due a rename because of the title, never because it moved.
        */
        let human_branch = worktree_session_marker_value(
            "feature/login",
            "/repos/project-0123abcd",
            "Codex Session",
            "2026-07-29T00:00:00.000Z",
        );
        let session = worktree_session("Fix login", human_branch, "generated");
        let runtime_settings =
            runtime_settings_with_moved_worktree_path(&session, "/repos/project-renamed", None)
                .expect("runtime settings");
        assert_eq!(
            plan_worktree_branch_rename(&json!({
                "projectId": "P1ab",
                "runtimeSettings": Value::Object(runtime_settings),
                "sessionId": "G1ab",
                "title": "Fix login",
            })),
            None,
            "only branches gxserver minted are renamed"
        );

        let temp_session = worktree_session("Fix login", temp_marker(), "generated");
        let moved = runtime_settings_with_moved_worktree_path(
            &temp_session,
            "/repos/project-renamed",
            None,
        )
        .expect("runtime settings");
        let plan = plan_worktree_branch_rename(&json!({
            "projectId": "P1ab",
            "runtimeSettings": Value::Object(moved),
            "sessionId": "G1ab",
            "title": "Fix login",
        }))
        .expect("plan");
        assert_eq!(
            plan.worktree_path, "/repos/project-renamed",
            "the sweep follows the worktree to its new folder"
        );
    }

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("git command");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn branch_helpers_read_and_rename_a_real_repository() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        std::fs::create_dir_all(&root).expect("repo dir");
        git(&root, &["init", "--quiet", "-b", "main"]);
        git(&root, &["config", "user.email", "tests@example.invalid"]);
        git(&root, &["config", "user.name", "Ghostex Tests"]);
        std::fs::write(root.join("README.md"), "hello\n").expect("readme");
        git(&root, &["add", "README.md"]);
        git(&root, &["commit", "--quiet", "-m", "initial"]);
        git(&root, &["checkout", "--quiet", "-b", "ghostex/0123abcd"]);

        let root_path = root.to_string_lossy().to_string();
        assert_eq!(
            current_worktree_branch(&root_path).as_deref(),
            Some("ghostex/0123abcd")
        );
        assert!(worktree_branch_exists(&root_path, "main"));
        assert!(!worktree_branch_exists(&root_path, "ghostex/fix-login"));
        assert_eq!(
            resolve_repository_default_branch(&root_path).map(|branch| branch.name),
            Some("main".to_string())
        );
        assert!(rename_worktree_branch(
            &root_path,
            "ghostex/0123abcd",
            "ghostex/fix-login"
        ));
        assert_eq!(
            current_worktree_branch(&root_path).as_deref(),
            Some("ghostex/fix-login")
        );
        assert!(!rename_worktree_branch(
            &root_path,
            "ghostex/0123abcd",
            "ghostex/fix-login"
        ));
        assert_eq!(
            resolve_origin_base_commit(&root_path, "main"),
            None,
            "a repository without an origin has no remote tip to start from"
        );
        assert_eq!(
            run_worktree_git(
                "/definitely/not/a/directory",
                &["status"],
                WORKTREE_GIT_COMMAND_TIMEOUT
            ),
            None
        );
    }

    #[test]
    fn ref_namespace_collisions_are_detected_in_both_directions() {
        /*
        CDXC:WorktreeRename 2026-08-09-18:40:
        This is the failure that produces a raw `fatal: branch rename failed`
        with no explanation: `test` is a legal ref name and no branch called
        `test` exists, yet git still refuses while `test/board-batch` does,
        because a ref cannot be both a leaf and a namespace. Both directions are
        checked here because both happen in practice.
        */
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        std::fs::create_dir_all(&root).expect("repo dir");
        git(&root, &["init", "--quiet", "-b", "main"]);
        git(&root, &["config", "user.email", "tests@example.invalid"]);
        git(&root, &["config", "user.name", "Ghostex Tests"]);
        std::fs::write(root.join("README.md"), "hello\n").expect("readme");
        git(&root, &["add", "README.md"]);
        git(&root, &["commit", "--quiet", "-m", "initial"]);
        git(&root, &["branch", "test/board-batch"]);
        git(&root, &["branch", "leaf"]);

        let root_path = root.to_string_lossy().to_string();
        assert_eq!(
            worktree_branch_namespace_blocker(&root_path, "test").as_deref(),
            Some("test/board-batch"),
            "an existing ref under the target name blocks it"
        );
        assert_eq!(
            worktree_branch_namespace_blocker(&root_path, "leaf/child").as_deref(),
            Some("leaf"),
            "an ancestor that is already a leaf ref blocks it"
        );
        assert_eq!(
            worktree_branch_namespace_blocker(&root_path, "feat/login"),
            None,
            "an unrelated name is not blocked"
        );
        assert_eq!(
            worktree_branch_namespace_blocker(&root_path, "test/board-batch"),
            None,
            "the branch's own name is not its own blocker"
        );
        assert!(
            !worktree_has_populated_submodules(&root_path),
            "a repository with no gitlinks has nothing populated"
        );
    }
}
