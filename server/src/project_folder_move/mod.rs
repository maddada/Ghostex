//! `ghostex move-project-folder`: moving a project's folder on disk and everything Ghostex keeps
//! about it, or nothing.
//!
//! CDXC:Projects 2026-11-19 WHY: the plan and its review are docs/2026-11-19/project-move-folder/PLAN.md (revision 2, 16 risks). The order is checks (read only) → the journal (`planned`) → one rename that never replaces anything (`moved`) → one database transaction plus the state files (`recorded`) → git repair (`repaired`) → agent history → done. Every step after the rename is safe to run again, so a gxserver that dies part-way finishes the move at its next start from the journal (recovery below); before the database is written, a failure renames the folder back. Only the local CLI can ask for it (RemoteBlocked, off the peer allowlist) until this has been proven, and there is no button.
//! SEE-ALSO: agents/resume_plan.rs (`with_codex_removed_worktree_cwd` reads the moved-folder marker), zmx/provider.rs and domain/repository/session.rs (starts and new sessions refused while moving), external_sessions.rs and resume_lookup.rs (former paths), server/src/server/worktree_ops/rename.rs (the same path comparison for Rename Worktree).

mod checks;
mod fs_ops;
mod journal;
mod path_match;
mod record;
mod registry;
mod repair;

use rusqlite::Connection;
use serde_json::{json, Value};

pub(crate) use record::MOVED_FOLDER_SESSION_RUNTIME_KEY;
pub(crate) use registry::{
    current_for_former, default_root_dir, former_equivalents, path_is_moving, refuse_if_moving,
};

use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;
use journal::{Journal, Phase};

fn conflict(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "conflict",
        message: message.into(),
    }
}

/// What the endpoint was asked.
pub(crate) struct MoveRequest {
    pub(crate) project_id: String,
    pub(crate) new_path: String,
    pub(crate) dry_run: bool,
    pub(crate) force_open_files: bool,
}

/// The move, start to end, on this gxserver's database. Returns the result and the ids of every
/// project it touched (for the presentation deltas).
pub(crate) fn move_project_folder(
    paths: &GxserverPaths,
    db: &Connection,
    server_id: &str,
    request: &MoveRequest,
) -> Result<(Value, Vec<String>), DomainStateError> {
    let root_dir = &paths.root_dir;
    // One move at a time, from before the checks to the end (Opus review, finding 2).
    let _one_move = registry::lock_moves()?;
    if let Some(stuck) = journal::read_journal(root_dir, &request.project_id) {
        return Err(conflict(format!(
            "A move of this project from {} to {} is unfinished (it stopped at \"{}\"). Run ghostex move-project-folder --project-id {} --resume to finish it, or --abandon.",
            stuck.old_path,
            stuck.new_path,
            stuck.phase.name(),
            request.project_id
        )));
    }
    let repository = DomainRepository::new(db, server_id);
    let checked = checks::plan(
        paths,
        &repository,
        &request.project_id,
        &request.new_path,
        request.force_open_files,
        request.dry_run,
    )?;
    let mut journal = checked.journal;
    if request.dry_run {
        let root = journal.root();
        let rows = record::rewrite_database(db, &root, false)?;
        return Ok((
            json!({
                "dryRun": true,
                "from": journal.old_path,
                "to": journal.new_path,
                "caseOnly": journal.case_only,
                "projects": journal.project_ids,
                "rows": rows.summary(),
                "sessionsMoved": rows.moved_sessions.len(),
                "files": record::rewrite_files(root_dir, &root, false),
                "gitCheckouts": journal.checkouts,
                "worktrees": journal.worktrees.iter().map(|worktree| json!({
                    "path": worktree.path,
                    "hasSubmodules": worktree.has_submodules,
                })).collect::<Vec<_>>(),
                "warnings": checked.warnings,
            }),
            Vec::new(),
        ));
    }

    // Nothing starts in these projects or the folder from here on (risk 12), then the running
    // check again, since a start could have slipped in after the first one.
    registry::begin_moving(journal.project_ids.clone(), journal.root())?;
    let projects = repository.list_projects_without_session_history()?;
    let running = checks::running_sessions(
        &repository,
        &journal.project_ids,
        &journal.root(),
        &projects,
    )
    .inspect_err(|_| registry::end_moving(&journal.project_id))?;
    if !running.is_empty() {
        registry::end_moving(&journal.project_id);
        return Err(DomainStateError::bad_request(format!(
            "A session started while checking: {}. Close or sleep it, then try again.",
            running.join("; ")
        )));
    }
    if let Err(error) = journal::create_journal(root_dir, &journal) {
        registry::end_moving(&journal.project_id);
        return Err(conflict(
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "Another move of this project is recorded as unfinished. Run --resume or --abandon first. Nothing was changed.".to_string()
            } else {
                format!("Could not write the move's journal: {error}. Nothing was changed.")
            },
        ));
    }
    journal::pause_for_test(Phase::Planned);

    // Step 2: the one rename (risk 3: no probe; risk 16: never over an existing folder).
    let old = std::path::PathBuf::from(&journal.old_path);
    let new = std::path::PathBuf::from(&journal.new_path);
    let mut result = fs_ops::rename_no_replace(&old, &new, journal.case_only);
    if result.as_ref().is_err_and(fs_ops::is_in_use_error) {
        std::thread::sleep(std::time::Duration::from_millis(500));
        result = fs_ops::rename_no_replace(&old, &new, journal.case_only);
    }
    if let Err(error) = result {
        journal::remove_journal(root_dir, &journal.project_id);
        registry::end_moving(&journal.project_id);
        return Err(DomainStateError::bad_request(
            if fs_ops::is_in_use_error(&error) {
                format!("Something on this computer still has files open in {}. Close editors, terminals and Explorer windows using it, then try again. Nothing was changed.", journal.old_path)
            } else if error.kind() == std::io::ErrorKind::AlreadyExists {
                format!(
                    "{} appeared while moving, so nothing was changed.",
                    journal.new_path
                )
            } else {
                format!("The folder could not be moved: {error}. Nothing was changed.")
            },
        ));
    }
    journal.phase = Phase::Moved;
    let _ = journal::write_journal(root_dir, &journal);
    journal::pause_for_test(Phase::Moved);

    // Step 3: record. Before it commits, a failure puts the folder back.
    let recorded = match record::record(db, root_dir, &journal) {
        Ok(recorded) => recorded,
        Err(error) => {
            return match fs_ops::rename_no_replace(&new, &old, journal.case_only) {
            Ok(()) => {
                journal::remove_journal(root_dir, &journal.project_id);
                registry::end_moving(&journal.project_id);
                Err(conflict(format!(
                    "Ghostex could not record the move ({}), so the folder was put back. Nothing was changed.",
                    error.message
                )))
            }
            Err(back) => Err(conflict(format!(
                "The folder moved to {}, but Ghostex could not record it ({}) or move it back ({back}). Ghostex finishes recording it at its next start; until then the project's sessions can't start.",
                journal.new_path, error.message
            ))),
        };
        }
    };
    finish(paths, db, journal, Some(recorded))
}

/// Steps 3 (when still `moved`) to the end, from wherever the journal stopped. Clears the moving
/// mark and the journal when done.
fn finish(
    paths: &GxserverPaths,
    db: &Connection,
    mut journal: Journal,
    recorded: Option<Value>,
) -> Result<(Value, Vec<String>), DomainStateError> {
    let root_dir = &paths.root_dir;
    let recorded = match recorded {
        Some(recorded) => recorded,
        None if journal.phase == Phase::Moved => record::record(db, root_dir, &journal)?,
        None => Value::Null,
    };
    if journal.phase == Phase::Moved {
        journal.phase = Phase::Recorded;
        let _ = journal::write_journal(root_dir, &journal);
        journal::pause_for_test(Phase::Recorded);
    }
    if journal.phase == Phase::Recorded {
        journal.notes.extend(repair::repair_git(&journal));
        journal.phase = Phase::Repaired;
        let _ = journal::write_journal(root_dir, &journal);
        journal::pause_for_test(Phase::Repaired);
    }
    journal.notes.extend(repair::carry_agent_history(&journal));
    journal::remove_journal(root_dir, &journal.project_id);
    registry::end_moving(&journal.project_id);
    let project_ids = journal.project_ids.clone();
    Ok((
        json!({
            "projectId": journal.project_id,
            "from": journal.old_path,
            "to": journal.new_path,
            "projects": project_ids,
            "recorded": recorded,
            "notes": journal.notes,
        }),
        project_ids,
    ))
}

/// What the journal's phase and the disk say about an unfinished move: finish it, forget it (the
/// folder never moved), or leave it for the user.
enum Next {
    Finish(Journal),
    Forget,
    Stuck(String),
}

fn next_step(mut journal: Journal) -> Next {
    let old_exists = std::path::Path::new(&journal.old_path).exists();
    let new_exists = std::path::Path::new(&journal.new_path).exists();
    // A case-only rename is one folder under both names.
    let (old_exists, new_exists) = if journal.case_only {
        (false, new_exists)
    } else {
        (old_exists, new_exists)
    };
    match journal.phase {
        Phase::Planned => match (old_exists, new_exists) {
            (true, false) => Next::Forget,
            // It died right after the rename, before the journal said so.
            (false, true) => {
                journal.phase = Phase::Moved;
                Next::Finish(journal)
            }
            _ => Next::Stuck(format!(
                "both or neither of {} and {} exist",
                journal.old_path, journal.new_path
            )),
        },
        Phase::Moved => match (old_exists, new_exists) {
            (false, true) => Next::Finish(journal),
            // The rename back after a failed record went through.
            (true, false) => Next::Forget,
            _ => Next::Stuck(format!(
                "both or neither of {} and {} exist",
                journal.old_path, journal.new_path
            )),
        },
        Phase::Recorded | Phase::Repaired => Next::Finish(journal),
    }
}

/// CDXC:Projects 2026-11-19 WHY: runs at gxserver's start before it serves anything (server/foreground.rs), so a move a crash interrupted is finished or forgotten before any session of the project can start (risk 2). A move that can't be decided from the disk keeps its projects refused until `--resume` or `--abandon`.
pub(crate) fn recover_on_startup(paths: &GxserverPaths, server_id: &str) {
    let _one_move = registry::lock_moves();
    for journal in journal::read_all_journals(&paths.root_dir) {
        let project_id = journal.project_id.clone();
        let phase = journal.phase.name();
        match next_step(journal.clone()) {
            Next::Forget => {
                journal::remove_journal(&paths.root_dir, &project_id);
                eprintln!("[gxserver] folder move of {project_id}: the folder never moved; forgot the move.");
            }
            Next::Stuck(why) => {
                registry::keep_moving(journal.project_ids.clone(), journal.root());
                eprintln!("[gxserver] folder move of {project_id} stopped at {phase} and can't be finished on its own ({why}). Its sessions can't start until ghostex move-project-folder --project-id {project_id} --resume or --abandon.");
            }
            Next::Finish(journal) => {
                registry::keep_moving(journal.project_ids.clone(), journal.root());
                let outcome = crate::storage::open_gxserver_database_with_busy_timeout(
                    paths,
                    std::time::Duration::from_secs(10),
                )
                .map_err(|error| conflict(error.to_string()))
                .and_then(|db| finish(paths, &db, journal, None));
                let _ = server_id;
                match outcome {
                    Ok((result, _)) => eprintln!("[gxserver] folder move of {project_id} stopped at {phase}; finished it: {result}"),
                    Err(error) => eprintln!("[gxserver] folder move of {project_id} stopped at {phase}; finishing it failed: {}", error.message),
                }
            }
        }
    }
}

/// `--resume`: finish an unfinished move now, the way startup would.
pub(crate) fn resume(
    paths: &GxserverPaths,
    db: &Connection,
    project_id: &str,
) -> Result<(Value, Vec<String>), DomainStateError> {
    let _one_move = registry::lock_moves()?;
    let journal = journal::read_journal(&paths.root_dir, project_id)
        .ok_or_else(|| DomainStateError::not_found("This project has no unfinished move."))?;
    match next_step(journal.clone()) {
        Next::Finish(journal) => {
            // Its own entry (left by the startup recovery or a failed record) is replaced; one
            // that overlaps another unfinished move is refused and the block kept.
            registry::end_moving(project_id);
            if let Err(error) = registry::begin_moving(journal.project_ids.clone(), journal.root())
            {
                registry::keep_moving(journal.project_ids.clone(), journal.root());
                return Err(error);
            }
            finish(paths, db, journal, None)
        }
        Next::Forget => {
            journal::remove_journal(&paths.root_dir, project_id);
            registry::end_moving(project_id);
            Ok((
                json!({ "projectId": project_id, "forgotten": true, "notes": ["The folder never moved, so there was nothing to finish."] }),
                Vec::new(),
            ))
        }
        Next::Stuck(why) => Err(conflict(format!(
            "The move can't be finished on its own: {why}. {}",
            stuck_by_hand(&journal)
        ))),
    }
}

/// The shell's move command, for the steps a user takes by hand.
fn move_command() -> &'static str {
    if cfg!(windows) {
        "move"
    } else {
        "mv"
    }
}

/// What to do by hand when the disk can't say whether the move happened: both folders exist, or
/// neither.
fn stuck_by_hand(journal: &Journal) -> String {
    let (old, new) = (&journal.old_path, &journal.new_path);
    let mv = move_command();
    let id = &journal.project_id;
    if std::path::Path::new(old).exists() {
        format!(
            "Look in both {old} and {new} and keep the one holding the project. If that is {new}, rename the other aside ({mv} \"{old}\" \"{old}.leftover\") and run ghostex move-project-folder --project-id {id} --resume to finish the move. If it is {old}, rename {new} aside ({mv} \"{new}\" \"{new}.leftover\") and run --resume, which then forgets the move and leaves the project at {old}."
        )
    } else {
        format!(
            "Find the project's folder and move it back by hand ({mv} \"<where it is>\" \"{old}\"), then run ghostex move-project-folder --project-id {id} --resume, which forgets the move; or move it to {new} and run --resume to finish the move."
        )
    }
}

/// `--abandon`: forget an unfinished move, leaving the folder and the records as they are.
///
/// CDXC:Projects 2026-11-20 WHY: refused while the folder's place shows `--resume` would finish the move (the folder is at the new path, or the database already names it): forgetting it then would leave the folder moved and Ghostex pointing at the old path, or git unrepaired, with no journal left to fix it from (Opus review, finding 7).
pub(crate) fn abandon(paths: &GxserverPaths, project_id: &str) -> Result<Value, DomainStateError> {
    let _one_move = registry::lock_moves()?;
    let journal = journal::read_journal(&paths.root_dir, project_id)
        .ok_or_else(|| DomainStateError::not_found("This project has no unfinished move."))?;
    if let Next::Finish(_) = next_step(journal.clone()) {
        let (old, new) = (&journal.old_path, &journal.new_path);
        let undo = if matches!(journal.phase, Phase::Planned | Phase::Moved) {
            format!("To undo it instead, move the folder back by hand ({} \"{new}\" \"{old}\"), then run --abandon.", move_command())
        } else {
            format!("Ghostex already names the new folder, so to undo it, finish it with --resume, then move it back with ghostex move-project-folder --project-id {project_id} \"{old}\".")
        };
        return Err(conflict(format!(
            "This move can still be finished: the folder is at {new}. Run ghostex move-project-folder --project-id {project_id} --resume instead. {undo}"
        )));
    }
    journal::remove_journal(&paths.root_dir, project_id);
    registry::end_moving(project_id);
    Ok(json!({
        "projectId": project_id,
        "abandoned": true,
        "notes": [format!("Forgot the move from {} to {}, which stopped at {}. Ghostex still names the folder its records had; check the project's folder and sessions.", journal.old_path, journal.new_path, journal.phase.name())],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn journal(dir: &std::path::Path, project_id: &str, phase: Phase) -> Journal {
        let old = dir.join("Old").to_string_lossy().to_string();
        Journal {
            version: 1,
            project_id: project_id.to_string(),
            old_path: old.clone(),
            new_path: dir.join("New").to_string_lossy().to_string(),
            old_spellings: vec![old],
            ignore_case: false,
            case_only: false,
            project_ids: vec![project_id.to_string()],
            checkouts: Vec::new(),
            worktrees: Vec::new(),
            claude_old_keys: Vec::new(),
            trusted_by: Vec::new(),
            phase,
            started_at: String::new(),
            notes: Vec::new(),
        }
    }

    /// What `next_step` decides with the folder at the old place, the new one, both or neither.
    fn decide(phase: Phase, old: bool, new: bool) -> &'static str {
        let dir = tempfile::tempdir().unwrap();
        if old {
            std::fs::create_dir(dir.path().join("Old")).unwrap();
        }
        if new {
            std::fs::create_dir(dir.path().join("New")).unwrap();
        }
        match next_step(journal(dir.path(), "p", phase)) {
            Next::Finish(journal) if journal.phase == Phase::Moved => "finish from moved",
            Next::Finish(_) => "finish",
            Next::Forget => "forget",
            Next::Stuck(_) => "stuck",
        }
    }

    #[test]
    fn each_phase_is_finished_forgotten_or_left_by_where_the_folder_is() {
        // Planned: the rename may or may not have happened.
        assert_eq!(decide(Phase::Planned, true, false), "forget");
        assert_eq!(decide(Phase::Planned, false, true), "finish from moved");
        assert_eq!(decide(Phase::Planned, true, true), "stuck");
        assert_eq!(decide(Phase::Planned, false, false), "stuck");
        // Moved: the database still names the old folder.
        assert_eq!(decide(Phase::Moved, false, true), "finish from moved");
        assert_eq!(decide(Phase::Moved, true, false), "forget");
        assert_eq!(decide(Phase::Moved, true, true), "stuck");
        assert_eq!(decide(Phase::Moved, false, false), "stuck");
        // Recorded and repaired: the database names the new folder; always finished.
        for phase in [Phase::Recorded, Phase::Repaired] {
            assert_eq!(decide(phase, false, true), "finish");
            assert_eq!(decide(phase, true, true), "finish");
        }
    }

    #[test]
    fn abandon_is_refused_while_resume_would_finish_the_move() {
        let _serial = registry::test_serial();
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::get_gxserver_paths(Some(home.path().to_path_buf()));
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("New")).unwrap();
        let project_id = "abandon-test-project";
        journal::create_journal(
            &paths.root_dir,
            &journal(dir.path(), project_id, Phase::Moved),
        )
        .unwrap();
        let refused = abandon(&paths, project_id).unwrap_err();
        assert!(refused.message.contains("--resume"), "{}", refused.message);
        let undo = format!(
            "move the folder back by hand ({} \"{}\" \"{}\"), then run --abandon",
            move_command(),
            dir.path().join("New").display(),
            dir.path().join("Old").display()
        );
        assert!(refused.message.contains(&undo), "{}", refused.message);
        assert!(journal::read_journal(&paths.root_dir, project_id).is_some());
        // Both folders there: it can't be finished, so it may be forgotten.
        std::fs::create_dir(dir.path().join("Old")).unwrap();
        let abandoned = abandon(&paths, project_id).unwrap();
        assert_eq!(abandoned["abandoned"], true);
        assert!(journal::read_journal(&paths.root_dir, project_id).is_none());
    }

    #[test]
    fn resume_says_what_to_do_by_hand_when_the_disk_cant_tell() {
        let _serial = registry::test_serial();
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::get_gxserver_paths(Some(home.path().to_path_buf()));
        let db = Connection::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project_id = "stuck-test-project";
        journal::create_journal(
            &paths.root_dir,
            &journal(dir.path(), project_id, Phase::Moved),
        )
        .unwrap();
        // Neither folder: find it and move it back.
        let neither = resume(&paths, &db, project_id).unwrap_err().message;
        assert!(neither.contains("<where it is>"), "{neither}");
        assert!(neither.contains(&dir.path().join("Old").display().to_string()));
        // Both folders: keep one, rename the other aside.
        std::fs::create_dir(dir.path().join("Old")).unwrap();
        std::fs::create_dir(dir.path().join("New")).unwrap();
        let both = resume(&paths, &db, project_id).unwrap_err().message;
        assert!(both.contains(".leftover"), "{both}");
        assert!(journal::read_journal(&paths.root_dir, project_id).is_some());
        journal::remove_journal(&paths.root_dir, project_id);
    }

    #[test]
    fn a_second_journal_for_the_project_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let first = journal(dir.path(), "journal-test-project", Phase::Planned);
        journal::create_journal(home.path(), &first).unwrap();
        let mut second = first.clone();
        second.new_path = dir.path().join("Other").to_string_lossy().to_string();
        let error = journal::create_journal(home.path(), &second).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(
            journal::read_journal(home.path(), "journal-test-project")
                .unwrap()
                .new_path,
            first.new_path
        );
    }
}
