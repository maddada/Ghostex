//! A checkout's work that is not on its remote yet: what `readProjectGitState` answers with
//! `checkout: true`, and what `ghostex project status` reads on this computer and asks every
//! connected one for.
//!
//! CDXC:ProjectStatus 2026-11-15 WHY:
//! Before a release the Main Coordinator on the Mac has to know that m7-arch and the Windows PC hold
//! no uncommitted or unpushed Ghostex work (the user: "I don't want you to push a release and miss
//! including things from the other machines"), and every coordinator should be able to look at the
//! same project on the other computers ("something more general that you can use", user, Q3296).
//! The only way there was a terminal typed into on the other computer. The read is fixed git
//! plumbing on the project's own folder: the caller sends no command, path or ref, so another
//! computer's agents may run it (connected_computers/scope.rs).
//!
//! What counts as "not on origin": commits reachable from a checkout's HEAD (the main checkout,
//! every worktree, every submodule) that no remote-tracking branch has, minus commits whose patch
//! origin's main already has (worktree branches land on main as cherry-picks and squashes), and,
//! in a submodule, minus what the superproject's origin/main pins. Without those two the check
//! cried wolf: m7-arch's zed submodule showed 8 "unpushed" commits that were the very commit main
//! pins, in a clone that had never fetched that branch. Local branches nothing has checked out are
//! left out: the Mac has 349 of them, nearly all landed already, and they hide real work.
//! Reads only: `GIT_OPTIONAL_LOCKS=0` keeps `git status` from rewriting the index, and nothing
//! fetches, so "behind" is as of the checkout's last fetch (`lastFetchMs`).

use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::platform::process::{background_command, run_bounded, Keep, RunLimits};

/// How many changed files and commits one repository lists; the counts are always whole.
const LIST_LIMIT: usize = 20;

/// The checkout at `path`: `{"isRepo": false}` when it is none, and when `if_origin` names another
/// repository only `{"isRepo", "path", "originUrl", "matches": false}`, read with one git call.
pub fn read_checkout_state(path: &Path, if_origin: Option<&str>) -> Value {
    let Some(top) = git(path, &["rev-parse", "--show-toplevel"]) else {
        return json!({ "isRepo": false, "path": path.to_string_lossy() });
    };
    let top = PathBuf::from(top.trim());
    let origin_url = git(&top, &["remote", "get-url", "origin"]).map(|url| url.trim().to_string());
    if let Some(wanted) = if_origin
        .map(normalize_origin)
        .filter(|url| !url.is_empty())
    {
        if origin_url.as_deref().map(normalize_origin).as_deref() != Some(wanted.as_str()) {
            return json!({
                "isRepo": true,
                "path": top.to_string_lossy(),
                "originUrl": origin_url,
                "matches": false,
            });
        }
    }
    let main_ref = main_ref(&top);
    // Two Ghostex projects can be one repository (a worktree added as a project of its own):
    // the caller keeps one checkout per `commonDir`.
    let common_dir = git(&top, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .map(|dir| dir.trim().to_string());
    let own_dir = git(&top, &["rev-parse", "--path-format=absolute", "--git-dir"])
        .map(|dir| dir.trim().to_string());
    let (submodule_states, worktree_states) = std::thread::scope(|scope| {
        let submodule_states = scope.spawn(|| submodules(&top, main_ref.as_deref()));
        let worktree_states = worktrees(&top, main_ref.as_deref());
        (submodule_states.join().unwrap_or_default(), worktree_states)
    });
    let mut state = repository(&top, main_ref.as_deref(), &[]);
    let object = state.as_object_mut().expect("repository answers an object");
    object.insert("isRepo".into(), json!(true));
    object.insert("matches".into(), json!(true));
    object.insert("originUrl".into(), json!(origin_url));
    object.insert("mainRef".into(), json!(main_ref));
    object.insert("lastFetchMs".into(), json!(last_fetch_ms(&top)));
    object.insert("commonDir".into(), json!(common_dir));
    object.insert("isMainWorktree".into(), json!(own_dir.is_some() && own_dir == common_dir));
    object.insert("stashes".into(), json!(stashes(&top)));
    object.insert("submodules".into(), json!(submodule_states));
    object.insert("worktrees".into(), json!(worktree_states));
    state
}

/// `git@github.com:Owner/Repo.git`, `https://github.com/owner/repo` and `ssh://git@github.com/owner/repo/`
/// all read `github.com/owner/repo`, so two computers that cloned one repository differently match.
pub fn normalize_origin(url: &str) -> String {
    let mut url = url.trim().trim_end_matches('/').to_ascii_lowercase();
    for scheme in ["https://", "http://", "ssh://", "git://"] {
        if let Some(rest) = url.strip_prefix(scheme) {
            url = rest.to_string();
        }
    }
    if let Some((_, rest)) = url.split_once('@') {
        url = rest.to_string();
    }
    // scp form `host:owner/repo`; a port in `host:22/owner/repo` goes too.
    if let Some((host, rest)) = url.split_once(':') {
        let rest = rest
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .trim_start_matches('/');
        url = format!("{host}/{rest}");
    }
    url.trim_end_matches(".git").to_string()
}

/// Branch, HEAD, the upstream's ahead/behind, changed files and the commits not on origin of the
/// repository at `dir`. `pins` are commits known to be pushed (a submodule's commits its
/// superproject's main records).
fn repository(dir: &Path, main_ref: Option<&str>, pins: &[String]) -> Value {
    let branch = git(dir, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .map(|branch| branch.trim().to_string())
        .filter(|branch| !branch.is_empty());
    let head = git(dir, &["rev-parse", "--short", "HEAD"]).map(|head| head.trim().to_string());
    let upstream = git(
        dir,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .map(|upstream| upstream.trim().to_string())
    .filter(|upstream| !upstream.is_empty());
    // Against the upstream, else origin's main: a detached checkout is still behind main.
    let (ahead, behind) = upstream
        .as_deref()
        .or(main_ref)
        .and_then(|base| {
            git(
                dir,
                &[
                    "rev-list",
                    "--left-right",
                    "--count",
                    &format!("HEAD...{base}"),
                ],
            )
        })
        .and_then(|counts| {
            let mut parts = counts.split_whitespace().map(|n| n.parse::<u64>().ok());
            Some((parts.next()??, parts.next()??))
        })
        .map_or((Value::Null, Value::Null), |(ahead, behind)| {
            (json!(ahead), json!(behind))
        });
    let changed = changed_files(dir);
    let (not_on_origin_count, not_on_origin) = commits_not_on_origin(dir, main_ref, pins);
    json!({
        "path": dir.to_string_lossy(),
        "branch": branch,
        "head": head,
        "upstream": upstream,
        "ahead": ahead,
        "behind": behind,
        "changedCount": changed.len(),
        "changed": changed.into_iter().take(LIST_LIMIT).collect::<Vec<_>>(),
        "notOnOriginCount": not_on_origin_count,
        "notOnOrigin": not_on_origin,
    })
}

/// `git status --porcelain` lines (`M src/a.rs`, `?? notes.md`); submodules report their own.
fn changed_files(dir: &Path) -> Vec<String> {
    git(
        dir,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignore-submodules=all",
        ],
    )
    .map(|status| {
        status
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.trim_end().to_string())
            .collect()
    })
    .unwrap_or_default()
}

/// The commits HEAD has that origin does not: on no remote-tracking branch, not one of `pins`'
/// ancestors, and not a patch origin's main already holds (`git cherry` marks those `-`).
/// The count, and the first `LIST_LIMIT` with their subjects.
fn commits_not_on_origin(
    dir: &Path,
    main_ref: Option<&str>,
    pins: &[String],
) -> (usize, Vec<Value>) {
    let mut args = vec!["rev-list", "--max-count=500", "HEAD", "--not", "--remotes"];
    args.extend(pins.iter().map(String::as_str));
    let Some(listed) = git(dir, &args) else {
        return (0, Vec::new());
    };
    let mut shas: Vec<String> = listed.lines().map(|sha| sha.trim().to_string()).collect();
    shas.retain(|sha| !sha.is_empty());
    if shas.is_empty() {
        return (0, Vec::new());
    }
    if let Some(base) = main_ref {
        if let Some(cherry) = git(dir, &["cherry", base, "HEAD"]) {
            let landed: std::collections::HashSet<&str> = cherry
                .lines()
                .filter_map(|line| line.strip_prefix("- "))
                .map(str::trim)
                .collect();
            shas.retain(|sha| !landed.contains(sha.as_str()));
        }
    }
    // Subjects of the listed ones only, in one call: a long-lived branch can hold hundreds.
    let listed: Vec<&str> = shas.iter().take(LIST_LIMIT).map(String::as_str).collect();
    let mut args = vec!["log", "--no-walk=unsorted", "--format=%H%x09%ct%x09%s"];
    args.extend(listed.iter().copied());
    let commits = git(dir, &args)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let sha = parts.next()?;
            let time = parts.next()?;
            Some(json!({
                "sha": &sha[..sha.len().min(10)],
                "subject": parts.next().unwrap_or(""),
                "committedMs": time.trim().parse::<i64>().ok().map(|seconds| seconds * 1000),
            }))
        })
        .collect();
    (shas.len(), commits)
}

/// `refs/remotes/origin/HEAD`'s branch, else origin's main or master, as `origin/<name>`.
fn main_ref(dir: &Path) -> Option<String> {
    if let Some(head) = git(
        dir,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    ) {
        let head = head.trim();
        if !head.is_empty() {
            return Some(head.to_string());
        }
    }
    ["origin/main", "origin/master"]
        .into_iter()
        .find_map(|name| {
            git(
                dir,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/remotes/{name}"),
                ],
            )
            .map(|_| name.to_string())
        })
}

/// When the checkout last fetched (FETCH_HEAD's time), so "behind" can say how fresh it is.
fn last_fetch_ms(dir: &Path) -> Option<i64> {
    let git_dir = git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    std::fs::metadata(Path::new(git_dir.trim()).join("FETCH_HEAD"))
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_millis() as i64)
}

fn stashes(dir: &Path) -> Vec<Value> {
    git(dir, &["stash", "list", "--format=%gd%x09%s%x09%ct"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            Some(json!({
                "ref": parts.next()?,
                "subject": parts.next()?,
                "createdMs": parts.next().and_then(|t| t.trim().parse::<i64>().ok()).map(|s| s * 1000),
            }))
        })
        .collect()
}

/// Each initialised submodule: whether its checkout moved off what HEAD records, its changed
/// files, and its commits not on origin, counting what the superproject's main pins as pushed.
fn submodules(top: &Path, project_main: Option<&str>) -> Vec<Value> {
    let Some(status) = git(top, &["submodule", "status"]) else {
        return Vec::new();
    };
    let entries: Vec<(char, String)> = status
        .lines()
        .filter_map(|line| {
            let marker = line.chars().next()?;
            let path = line.get(1..)?.split_whitespace().nth(1)?.to_string();
            Some((marker, path))
        })
        .collect();
    // The commit every submodule has on the superproject's main, read in one call.
    let mut pinned_on_main = std::collections::HashMap::new();
    if let Some(base) = project_main {
        let mut args = vec!["ls-tree", base, "--"];
        args.extend(entries.iter().map(|(_, path)| path.as_str()));
        for line in git(top, &args).unwrap_or_default().lines() {
            if let Some((meta, path)) = line.split_once('\t') {
                if let Some(sha) = meta.split_whitespace().nth(2) {
                    pinned_on_main.insert(path.to_string(), sha.to_string());
                }
            }
        }
    }
    parallel_map(entries, |(marker, path)| {
            if marker == '-' {
                return json!({ "path": path, "initialized": false });
            }
            let dir = top.join(&path);
            if !answers(&dir) {
                return json!({ "path": path, "initialized": true, "unreadable": true });
            }
            let pins: Vec<String> = pinned_on_main.get(&path).cloned().into_iter().collect();
            let mut state = repository(&dir, main_ref(&dir).as_deref(), &pins);
            let object = state.as_object_mut().expect("repository answers an object");
            object.insert("path".into(), json!(path));
            object.insert("initialized".into(), json!(true));
            object.insert("pointerMoved".into(), json!(marker == '+'));
            object.insert("conflicted".into(), json!(marker == 'U'));
            state
        })
}

/// The checkout's other worktrees (the first entry is the checkout itself): branch or detached,
/// changed files, and commits not on origin; a missing folder is reported, not read.
fn worktrees(top: &Path, main_ref: Option<&str>) -> Vec<Value> {
    let listed = git(top, &["worktree", "list", "--porcelain"]).unwrap_or_default();
    let found: Vec<(PathBuf, Option<String>, bool)> = listed
        .split("\n\n")
        .filter_map(|block| {
            let mut path = None;
            let mut branch = None;
            let mut prunable = false;
            let mut bare = false;
            for line in block.lines() {
                if let Some(rest) = line.strip_prefix("worktree ") {
                    path = Some(PathBuf::from(rest));
                } else if let Some(rest) = line.strip_prefix("branch ") {
                    branch = Some(rest.trim_start_matches("refs/heads/").to_string());
                } else if line.starts_with("prunable") {
                    prunable = true;
                } else if line == "bare" {
                    bare = true;
                }
            }
            let path = path?;
            (!bare).then_some((path, branch, prunable))
        })
        .skip(1)
        .collect();
    parallel_map(found, |(path, branch, prunable)| {
            if prunable || !path.is_dir() {
                return json!({ "path": path.to_string_lossy(), "branch": branch, "missing": true });
            }
            if !answers(&path) {
                return json!({ "path": path.to_string_lossy(), "branch": branch, "missing": false, "unreadable": true });
            }
            let mut state = repository(&path, main_ref, &[]);
            if let Some(object) = state.as_object_mut() {
                object.insert("missing".into(), json!(false));
            }
            state
        })
}

/// How many git readers run at once: the Mac's checkout has 15 worktrees and 10 submodules, and
/// one after another they took seconds (each read is a handful of short git calls).
const READERS: usize = 8;

/// `items` through `read` on up to `READERS` threads, answers in the items' order.
pub(crate) fn parallel_map<T: Send, R: Send>(
    items: Vec<T>,
    read: impl Fn(T) -> R + Sync,
) -> Vec<R> {
    let total = items.len();
    let queue = std::sync::Mutex::new(items.into_iter().enumerate());
    let answers = std::sync::Mutex::new((0..total).map(|_| None).collect::<Vec<Option<R>>>());
    std::thread::scope(|scope| {
        for _ in 0..READERS.min(total) {
            scope.spawn(|| loop {
                let next = queue.lock().unwrap_or_else(|p| p.into_inner()).next();
                let Some((index, item)) = next else { break };
                let answer = read(item);
                answers.lock().unwrap_or_else(|p| p.into_inner())[index] = Some(answer);
            });
        }
    });
    answers
        .into_inner()
        .unwrap_or_else(|p| p.into_inner())
        .into_iter()
        .flatten()
        .collect()
}

/// How long one git call may take. Some folders block every read (on the Mac, projects under
/// ~/dev/references and on the offload volume hang `ls` itself); git there never returned, so the
/// whole read missed the relay's time limit and each call left a git process behind for good.
const GIT_CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// One read-only git call in `dir`, ended after `GIT_CALL_TIMEOUT` with everything it started;
/// `None` when git is missing, the call fails or it ran out of time.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let mut command = background_command("git");
    command
        .arg("-c")
        .arg("safe.directory=*")
        .arg("-c")
        .arg("core.quotePath=false")
        .args(args)
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0");
    let output = run_bounded(
        command,
        RunLimits {
            timeout: GIT_CALL_TIMEOUT,
            stdin: None,
            stdout: Keep::Head(32 << 20),
            stderr: Keep::Head(0),
        },
    )
    .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Whether git answers in `dir` at all: a worktree or submodule on a folder that blocks is
/// reported as unreadable after one call instead of costing every call its time limit.
fn answers(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--git-dir"]).is_some()
}
