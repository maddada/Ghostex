//! `ghostex project status [--computer <name> | --all-computers] [--project-id id | --path dir] [--json]`:
//! the git state of this project's checkout here and on every connected computer, as one table.
//!
//! CDXC:ProjectStatus 2026-11-15 WHY:
//! The user, before a release: "Always check the other machines that are connectable, check if they
//! have any work that wasn't committed and pushed", and on the decision page (Q3296) a general
//! command rather than a release check: "you call the command on the project on each machine, and
//! then from that, you would know", for every coordinator. This computer's checkout is read here
//! directly (no gxserver needed); another computer's goes through this computer's gxserver relay to
//! `readProjectGitState` with `checkout: true` there (project_git_state/checkout.rs), and the
//! project there is the one whose origin is this checkout's origin, whatever its folder is called.
//! An offline computer is listed as not checked with the time gxserver last saw it (Q3297: a release
//! goes ahead without it and says so). A gxserver too old for the read is named, not an error.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::args::{parse_args, Flags};
use super::computers::{connected_computers, pick_computer, relay_target, ConnectedComputer};
use super::output::print_json;
use super::rpc::{
    request_gxserver_rpc, resolve_local_gxserver_target, CliError, CliResult, Target,
};
use crate::project_git_state::checkout::{normalize_origin, read_checkout_state};

const HELP: &str = "Usage: ghostex project status [--computer <name> | --all-computers] [--project-id <id> | --path <folder>] [--json]

The git state of this project's checkout on this computer and on every computer it is connected
to (ghostex computers), so a coordinator sees work left anywhere before a release. Read-only.

For each checkout: its folder, branch and how far it is behind origin (as of its last fetch),
uncommitted files, commits not on origin, stashes, and the same for every submodule and every
other worktree. A commit counts as on origin when a remote branch has it, when origin's main has
the same patch (a cherry-picked or squashed worktree branch), or, in a submodule, when the
project's origin/main pins it. Local branches no checkout or worktree has checked out are not
read.

  --computer <name>   Only that computer (its name, the start of it, or its server id; \"this\"
                      for this one). An offline computer is listed with when it was last seen.
  --all-computers     This computer and every connected one (the default).
  --project-id <id>   The project (ghostex projects); default: the project of this folder.
  --path <folder>     The project at that folder.
  --json              Every field, for scripts.

On another computer the project is the one whose origin is this checkout's origin. Reading
another computer needs a Ghostex released after 11.7.1 on both computers; an older one is named
in the table. An offline computer is \"not checked\", with when this computer last saw it.";

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        Some("status") => status(&args[1..]),
        None | Some("-h" | "--help" | "help") => {
            println!("{HELP}");
            Ok(())
        }
        Some(other) => Err(CliError::Other(format!(
            "Unknown command \"project {other}\". Run: ghostex project status [--computer <name>] [--json]"
        ))),
    }
}

/// One computer's answer.
struct Row {
    name: String,
    server_id: Option<String>,
    this_computer: bool,
    /// `checked`, `offline`, `needsNewerGhostex`, `notChecked` or `noCheckout`.
    state: &'static str,
    message: Option<String>,
    last_seen_ms: Option<i64>,
    checkouts: Vec<Value>,
}

impl Row {
    fn new(name: &str, server_id: Option<&str>, this_computer: bool) -> Self {
        Row {
            name: name.to_string(),
            server_id: server_id.map(str::to_string),
            this_computer,
            state: "checked",
            message: None,
            last_seen_ms: None,
            checkouts: Vec::new(),
        }
    }

    fn not_checked(mut self, state: &'static str, message: String) -> Self {
        self.state = state;
        self.message = Some(message);
        self
    }
}

fn status(args: &[String]) -> CliResult<()> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        println!("{HELP}");
        return Ok(());
    }
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let local = resolve_local_gxserver_target()?;
    let none = Flags::default();
    let projects = request_gxserver_rpc(&local, "/api/listProjects", &json!({}), &none)?;
    let project = local_project(&projects, flags)?;
    let project_path = PathBuf::from(text(&project, "path"));
    let here = read_checkout_state(&project_path, None);
    if here.get("isRepo").and_then(Value::as_bool) != Some(true) {
        return Err(CliError::Other(format!(
            "{} is not a git checkout, so there is nothing to compare.",
            project_path.display()
        )));
    }
    let origin = here
        .get("originUrl")
        .and_then(Value::as_str)
        .map(str::to_string);

    let this = request_gxserver_rpc(&local, "/api/listConnectedComputers", &json!({}), &none).ok();
    let this_name = this
        .as_ref()
        .map(|answer| text(answer, "name").to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "This computer".to_string());
    let this_server_id = super::rpc::local_gxserver_server_id();
    let connected = connected_computers(&local);
    let known = crate::connected_computers::read_known_computers(
        &crate::paths::get_gxserver_paths(None).app_state_dir,
    );
    let offline: Vec<(String, String, i64)> = known
        .into_iter()
        .filter(|(id, _, _)| Some(id) != this_server_id.as_ref())
        .filter(|(id, _, _)| !connected.iter().any(|computer| &computer.server_id == id))
        .collect();

    let mut local_row = Row::new(&this_name, this_server_id.as_deref(), true);
    local_row.checkouts.push(here);
    let mut rows = Vec::new();
    match flags.text("computer") {
        Some(wanted) if !flags.truthy("all-computers") => {
            let lower = wanted.trim().to_lowercase();
            let offline_match = offline.iter().find(|(id, name, _)| {
                id == wanted.trim() || name.to_lowercase().starts_with(&lower)
            });
            match pick_computer(&connected, &wanted, this_server_id.as_deref()) {
                Ok(None) => rows.push(local_row),
                Ok(Some(computer)) => rows.extend(read_computers(
                    &local,
                    &[computer.clone()],
                    origin.as_deref(),
                )),
                Err(_) if offline_match.is_some() => {
                    rows.push(offline_row(offline_match.expect("matched above")))
                }
                Err(error) => return Err(error),
            }
        }
        _ => {
            rows.push(local_row);
            rows.extend(read_computers(&local, &connected, origin.as_deref()));
            rows.extend(offline.iter().map(offline_row));
        }
    }

    let project_name = text(&project, "name").to_string();
    if flags.truthy("json") {
        print_json(&json!({
            "ok": true,
            "project": {
                "name": project_name,
                "path": project_path.to_string_lossy(),
                "originUrl": origin,
            },
            "clean": rows.iter().all(|row| row.checkouts.iter().all(|checkout| !unpushed(checkout))),
            "notChecked": rows.iter().filter(|row| row.state != "checked").map(|row| row.name.clone()).collect::<Vec<_>>(),
            "computers": rows.iter().map(|row| json!({
                "name": row.name,
                "serverId": row.server_id,
                "thisComputer": row.this_computer,
                "state": row.state,
                "message": row.message,
                "lastSeenMs": row.last_seen_ms,
                "checkouts": row.checkouts,
            })).collect::<Vec<_>>(),
        }));
        return Ok(());
    }
    print_table(&project_name, origin.as_deref(), &rows);
    Ok(())
}

/// The project `--project-id` or `--path` names, else the one whose folder holds the current
/// folder (the longest match), else the one holding this checkout's main worktree.
fn local_project(projects: &Value, flags: &Flags) -> CliResult<Value> {
    let empty = Vec::new();
    let list = projects
        .get("projects")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    if let Some(id) = flags.text("project-id") {
        return list
            .iter()
            .find(|project| text(project, "projectId") == id)
            .cloned()
            .ok_or_else(|| {
                CliError::Other(format!(
                    "No project has the id {id} (ghostex projects lists them)."
                ))
            });
    }
    let folder = match flags.text("path") {
        Some(path) => PathBuf::from(path),
        None => std::env::current_dir().map_err(|error| CliError::Other(error.to_string()))?,
    };
    let folder = folder.canonicalize().unwrap_or(folder);
    let holding = |folder: &Path| {
        list.iter()
            .filter(|project| text(project, "visibility") != "hidden")
            .filter(|project| {
                let path = PathBuf::from(text(project, "path"));
                let path = path.canonicalize().unwrap_or(path);
                !text(project, "path").is_empty() && folder.starts_with(&path)
            })
            .max_by_key(|project| text(project, "path").len())
            .cloned()
    };
    if let Some(project) = holding(&folder) {
        return Ok(project);
    }
    // A worktree outside the project folder: the project of its main checkout.
    let main_checkout = std::process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(&folder)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
        .and_then(|git_dir| git_dir.parent().map(Path::to_path_buf));
    main_checkout
        .and_then(|checkout| holding(&checkout))
        .ok_or_else(|| {
            CliError::Other(format!(
                "{} is in no Ghostex project. Run it inside a project's folder, or pass --project-id (ghostex projects lists them).",
                folder.display()
            ))
        })
}

/// Each connected computer's checkouts of the repository `origin` names, read side by side.
fn read_computers(
    local: &Target,
    computers: &[ConnectedComputer],
    origin: Option<&str>,
) -> Vec<Row> {
    std::thread::scope(|scope| {
        let reads: Vec<_> = computers
            .iter()
            .map(|computer| scope.spawn(move || read_computer(local, computer, origin)))
            .collect();
        reads
            .into_iter()
            .zip(computers)
            .map(|(read, computer)| {
                read.join().unwrap_or_else(|_| {
                    Row::new(&computer.name, Some(&computer.server_id), false)
                        .not_checked("notChecked", "the read failed".to_string())
                })
            })
            .collect()
    })
}

fn read_computer(local: &Target, computer: &ConnectedComputer, origin: Option<&str>) -> Row {
    let row = Row::new(&computer.name, Some(&computer.server_id), false);
    if let Some(waiting) = &computer.waiting {
        return row.not_checked("notChecked", waiting.clone());
    }
    if !computer.agent_access {
        return row.not_checked("notChecked", "Agent access to it is off".to_string());
    }
    let Some(origin) = origin else {
        return row.not_checked(
            "notChecked",
            "this checkout has no origin remote to find the same project by".to_string(),
        );
    };
    let target = relay_target(local, &computer.server_id);
    let none = Flags::default();
    let projects = match request_gxserver_rpc(&target, "/api/listProjects", &json!({}), &none) {
        Ok(projects) => projects,
        Err(error) => return row.not_checked("notChecked", error.to_string()),
    };
    let empty = Vec::new();
    let mut candidates: Vec<&Value> = projects
        .get("projects")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .filter(|project| text(project, "visibility") != "hidden")
        .collect();
    // A folder named like the repository first; every project is still asked.
    let repo_name = normalize_origin(origin)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    candidates.sort_by_key(|project| !text(project, "path").to_lowercase().ends_with(&repo_name));
    let mut row = row;
    for project in candidates {
        let answer = request_gxserver_rpc(
            &target,
            "/api/readProjectGitState",
            &json!({
                "projectId": text(project, "projectId"),
                "checkout": true,
                "ifOrigin": origin,
            }),
            &none,
        );
        match answer {
            Ok(answer) => match answer.get("checkout") {
                Some(checkout) => {
                    if checkout.get("matches").and_then(Value::as_bool) == Some(true) {
                        row.checkouts.push(checkout.clone());
                    }
                }
                // An older gxserver there answers the plain git read and ignores `checkout`.
                None => return row.not_checked("needsNewerGhostex", newer_on(&computer.name)),
            },
            Err(error) => {
                let message = error.to_string();
                // This computer's relay predates the read: it refuses it before asking.
                if message.contains("Agents can't use") {
                    return row.not_checked("needsNewerGhostex", newer_on("this computer"));
                }
                // The other computer's gxserver predates it.
                if message.contains("not available to another computer")
                    || message.contains("Agents on another computer can't use")
                {
                    return row.not_checked("needsNewerGhostex", newer_on(&computer.name));
                }
                return row.not_checked("notChecked", message);
            }
        }
    }
    if row.checkouts.is_empty() {
        return row.not_checked(
            "noCheckout",
            format!(
                "none of its Ghostex projects is a checkout of {}",
                normalize_origin(origin)
            ),
        );
    }
    row
}

fn newer_on(where_: &str) -> String {
    format!("needs a newer Ghostex on {where_} (one released after 11.7.1)")
}

fn offline_row((server_id, name, last_seen_ms): &(String, String, i64)) -> Row {
    let mut row = Row::new(name, Some(server_id), false).not_checked(
        "offline",
        format!("offline, last seen {}", when(*last_seen_ms)),
    );
    row.last_seen_ms = Some(*last_seen_ms);
    row
}

/// What a checkout holds that is not on origin, summed over it, its submodules and worktrees:
/// `None` when nothing.
fn work(checkout: &Value) -> Option<(u64, u64, u64)> {
    let count = |value: &Value, key: &str| value.get(key).and_then(Value::as_u64).unwrap_or(0);
    let mut changed = count(checkout, "changedCount");
    let mut commits = count(checkout, "notOnOriginCount");
    for part in list(checkout, "submodules")
        .iter()
        .chain(list(checkout, "worktrees"))
    {
        changed += count(part, "changedCount");
        commits += count(part, "notOnOriginCount");
        if part.get("pointerMoved").and_then(Value::as_bool) == Some(true) {
            changed += 1;
        }
    }
    let stashes = list(checkout, "stashes").len() as u64;
    (changed + commits + stashes > 0).then_some((changed, commits, stashes))
}

/// Uncommitted files, a moved submodule or commits not on origin: work a release would miss.
/// Stashes are listed but left out, since they are usually set aside on purpose.
fn unpushed(checkout: &Value) -> bool {
    work(checkout).is_some_and(|(changed, commits, _)| changed + commits > 0)
}

fn print_table(project_name: &str, origin: Option<&str>, rows: &[Row]) {
    println!(
        "{project_name}{}\n",
        origin
            .map(|origin| format!(" ({})", normalize_origin(origin)))
            .unwrap_or_default()
    );
    let mut table = vec![[
        "COMPUTER".to_string(),
        "CHECKOUT".to_string(),
        "BRANCH".to_string(),
        "CHANGED".to_string(),
        "NOT ON ORIGIN".to_string(),
        "STASHES".to_string(),
        "WORKTREES".to_string(),
    ]];
    let mut unchecked: Vec<(String, String)> = Vec::new();
    for row in rows {
        let name = if row.this_computer {
            format!("{} (this)", row.name)
        } else {
            row.name.clone()
        };
        if row.state != "checked" {
            unchecked.push((name, row.message.clone().unwrap_or_default()));
            continue;
        }
        for checkout in &row.checkouts {
            let (changed, commits, stashes) = work(checkout).unwrap_or((0, 0, 0));
            let worktrees = list(checkout, "worktrees");
            let busy = worktrees
                .iter()
                .filter(|worktree| work(worktree).is_some())
                .count();
            table.push([
                name.clone(),
                text(checkout, "path").to_string(),
                branch_line(checkout),
                changed.to_string(),
                commits.to_string(),
                stashes.to_string(),
                if busy > 0 {
                    format!("{} ({busy} with work)", worktrees.len())
                } else {
                    worktrees.len().to_string()
                },
            ]);
        }
    }
    let widths: Vec<usize> = (0..7)
        .map(|column| {
            table
                .iter()
                .map(|line| line[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut widths = widths;
    widths[0] = unchecked
        .iter()
        .map(|(name, _)| name.chars().count())
        .fold(widths[0], usize::max);
    // Only the header when no computer was read: leave it out.
    for line in table.iter().filter(|_| table.len() > 1) {
        let cells: Vec<String> = line
            .iter()
            .zip(&widths)
            .map(|(cell, width)| format!("{cell:<width$}"))
            .collect();
        println!("{}", cells.join("  ").trim_end());
    }
    // A computer that wasn't read says why on its own line, past the columns it has no values for.
    for (name, why) in &unchecked {
        println!("{name:<width$}  not checked: {why}", width = widths[0]);
    }

    let mut details = Vec::new();
    for row in rows.iter().filter(|row| row.state == "checked") {
        for checkout in &row.checkouts {
            if work(checkout).is_none() {
                continue;
            }
            details.push(format!("\n{} {}", row.name, text(checkout, "path")));
            describe_repo(&mut details, "  ", checkout);
            for submodule in list(checkout, "submodules") {
                let moved = submodule.get("pointerMoved").and_then(Value::as_bool) == Some(true);
                if work(submodule).is_none() && !moved {
                    continue;
                }
                details.push(format!(
                    "  submodule {}{}",
                    text(submodule, "path"),
                    if moved {
                        ": checked out at a commit the project doesn't record (pointer moved)"
                    } else {
                        ""
                    }
                ));
                describe_repo(&mut details, "    ", submodule);
            }
            for worktree in list(checkout, "worktrees") {
                if work(worktree).is_none() {
                    continue;
                }
                details.push(format!(
                    "  worktree {} ({})",
                    text(worktree, "path"),
                    branch_line(worktree)
                ));
                describe_repo(&mut details, "    ", worktree);
            }
            let stashes = list(checkout, "stashes");
            if let Some(newest) = stashes.first() {
                details.push(format!(
                    "  {} stash{}, newest {}: {}{}",
                    stashes.len(),
                    if stashes.len() == 1 { "" } else { "es" },
                    text(newest, "ref"),
                    text(newest, "subject"),
                    newest
                        .get("createdMs")
                        .and_then(Value::as_i64)
                        .map(|ms| format!(" ({})", when(ms)))
                        .unwrap_or_default()
                ));
            }
        }
    }
    for line in &details {
        println!("{line}");
    }
    println!();
    let with_work: Vec<String> = rows
        .iter()
        .filter(|row| {
            row.checkouts
                .iter()
                .any(|checkout| unpushed(checkout))
        })
        .map(|row| row.name.clone())
        .collect();
    let stashed: Vec<String> = rows
        .iter()
        .filter_map(|row| {
            let count: usize = row.checkouts.iter().map(|checkout| list(checkout, "stashes").len()).sum();
            (count > 0).then(|| format!("{} ({count})", row.name))
        })
        .collect();
    let checked = rows.iter().filter(|row| row.state == "checked").count();
    if checked == 0 {
        println!("No computer could be read.");
    } else if with_work.is_empty() {
        println!(
            "Nothing to commit or push on the {checked} computer{} checked.",
            if checked == 1 { "" } else { "s" }
        );
    } else {
        println!("Work not on origin yet: {}.", with_work.join(", "));
    }
    if !stashed.is_empty() {
        println!("Stashes, not counted as work: {}.", stashed.join(", "));
    }
    if !unchecked.is_empty() {
        let names: Vec<&str> = unchecked.iter().map(|(name, _)| name.as_str()).collect();
        println!("Not checked: {}.", names.join(", "));
    }
}

/// `main, 2 behind origin/main (fetched 3 h ago)` / `detached at 3a1ded0`.
fn branch_line(repo: &Value) -> String {
    let mut line = match repo.get("branch").and_then(Value::as_str) {
        Some(branch) => branch.to_string(),
        None => format!("detached at {}", text(repo, "head")),
    };
    let behind = repo.get("behind").and_then(Value::as_u64).unwrap_or(0);
    if behind > 0 {
        line.push_str(&format!(", {behind} behind"));
    }
    if let Some(fetched) = repo.get("lastFetchMs").and_then(Value::as_i64) {
        line.push_str(&format!(" (fetched {})", when(fetched)));
    }
    line
}

fn describe_repo(details: &mut Vec<String>, indent: &str, repo: &Value) {
    let changed = repo
        .get("changedCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if changed > 0 {
        details.push(format!("{indent}{changed} uncommitted:"));
        for file in list(repo, "changed") {
            details.push(format!("{indent}  {}", file.as_str().unwrap_or("")));
        }
        if changed as usize > list(repo, "changed").len() {
            details.push(format!(
                "{indent}  …and {} more",
                changed as usize - list(repo, "changed").len()
            ));
        }
    }
    let commits = repo
        .get("notOnOriginCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if commits > 0 {
        details.push(format!(
            "{indent}{commits} commit{} not on origin:",
            if commits == 1 { "" } else { "s" }
        ));
        for commit in list(repo, "notOnOrigin") {
            details.push(format!(
                "{indent}  {} {}",
                text(commit, "sha"),
                text(commit, "subject")
            ));
        }
        if commits as usize > list(repo, "notOnOrigin").len() {
            details.push(format!(
                "{indent}  …and {} more",
                commits as usize - list(repo, "notOnOrigin").len()
            ));
        }
    }
}

/// `5 min ago`, `3 h ago`, `2 days ago, Nov 14 05:07`.
fn when(ms: i64) -> String {
    let now = chrono::Utc::now().timestamp_millis();
    let minutes = (now - ms).max(0) / 60_000;
    let ago = match minutes {
        0 => "just now".to_string(),
        1..=59 => format!("{minutes} min ago"),
        60..=1439 => format!("{} h ago", minutes / 60),
        1440..=2879 => "1 day ago".to_string(),
        _ => format!("{} days ago", minutes / 1440),
    };
    if minutes < 1440 {
        return ago;
    }
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|time| {
            format!(
                "{ago}, {}",
                time.with_timezone(&chrono::Local).format("%b %-d %H:%M")
            )
        })
        .unwrap_or(ago)
}

fn list<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}
