//! Every change to the ticket command record, under one writer lock: setting or re-checking a
//! workspace's command, switching to the id its command now answers with, clearing it, deleting
//! its workspace, and storing its credentials (`credential_save.rs`).
//!
//! A change that runs the command plans under the writer lock (the record, the generation,
//! whether the workspace exists and whether it is in a team, read together), runs `info` without
//! it, and commits under the lock again only if all of that is still true.
//!
//! Lock order: this store's writer lock, then the credentials file's (`mutate_credentials`), never
//! the reverse. Each change bumps the workspace's generation while it holds the writer lock, so a
//! later reader can tell that what it planned against is stale.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde_json::Value;

use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;

use super::*;

pub(crate) const TEAM_REFUSAL: &str = "A workspace connected to a team uses its team's tracker; ticket commands are for this computer's workspaces.";

/// How the refusal of a changed id ends. `ghostex issues source` looks for it to add its own
/// `--switch` hint, so the refusal itself stays the same for every client.
pub(crate) const SWITCH_OFFER: &str = "Switch to it to use the new id.";

const NOTHING_TO_CHECK: &str = "This workspace has no ticket command to check.";

/// Why a path holding a stored credential's value is refused: it would be kept and shown with
/// that value in it. The refusal quotes the typed path before it, and names no value.
pub(crate) const PATH_SPELLS_CREDENTIAL: &str = "can't be used: it contains the value of a credential this workspace keeps, which Ghostex can't hide in the command's path.";

/// An `info` answer that can't be kept: its id, an ID key or a credential name holds a stored
/// credential, which the record would keep as it is. Names no value.
pub(crate) const ANSWER_SPELLS_CREDENTIAL: &str = "This command's answer can't be used: its id, an ID key or a credential name contains the value of a credential this workspace keeps, which Ghostex can't hide there.";

/// What a change adds to what it says when an answer marked a field secret whose saved value is
/// too short to keep secret (`remove_short_secrets`). Names no field and no value.
pub(crate) const SHORT_SECRET_REMOVED: &str = "A credential this command now marks secret had a saved value too short to keep secret, so Ghostex removed it; save one of at least 3 characters.";

/// A change whose answer marked a field secret whose saved value is too short to keep secret, when
/// Ghostex couldn't remove that value.
pub(crate) const SHORT_SECRET_UNREMOVABLE: &str = "The command marks secret a credential whose saved value is too short to keep secret, and Ghostex couldn't remove that value, so it changed nothing; try again.";

const CHANGED_MEANWHILE: &str =
    "The ticket command's setup changed while it was being checked; try again.";

static WRITER: Mutex<()> = Mutex::new(());

/// The store's writer lock, which comes before the placement lock (`hold_project_placement`):
/// taken while holding that one, it fails the assertion in a debug build and answers
/// `LOCK_ORDER_REFUSED` in a release build, rather than waiting forever.
pub(super) fn writer() -> Result<MutexGuard<'static, ()>, DomainStateError> {
    if crate::domain::repository::project::placement_held_here() {
        debug_assert!(
            false,
            "the ticket command store's writer lock is taken while holding the placement lock"
        );
        return Err(crate::domain::repository::project::lock_order_refused());
    }
    Ok(WRITER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()))
}

/// The store's writer lock, for a background job (`jobs.rs`): held from its plan check until its
/// command has started, and again while it keeps the result, so no change lands in between.
pub(crate) fn hold_work_source_writer() -> Result<MutexGuard<'static, ()>, DomainStateError> {
    writer()
}

/// Whether some change holds the store's writer lock right now. Only the store's tests ask, and
/// they run on Unix.
#[cfg(all(test, unix))]
pub(crate) fn work_source_writer_held() -> bool {
    matches!(WRITER.try_lock(), Err(std::sync::TryLockError::WouldBlock))
}

fn generations() -> &'static Mutex<HashMap<String, u64>> {
    static GENERATIONS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    GENERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// How many times a workspace's ticket command setup changed in this process: its path, its pin,
/// its accepted `info`, its credentials or whether it is in a team.
pub(crate) fn work_source_generation(workspace_id: &str) -> u64 {
    generations()
        .lock()
        .ok()
        .and_then(|generations| generations.get(workspace_id).copied())
        .unwrap_or(0)
}

pub(super) fn bump_generation(workspace_id: &str) {
    if let Ok(mut generations) = generations().lock() {
        *generations.entry(workspace_id.to_string()).or_default() += 1;
    }
}

/// Runs `change` under the store's writer lock after bumping the workspace's generation: for a
/// change made elsewhere that alters what its ticket command may run with (joining or leaving a
/// team). `change` may take the credentials file's lock; it must not take this one again.
pub(crate) fn change_work_source_config<T>(
    workspace_id: &str,
    change: impl FnOnce() -> T,
) -> Result<T, DomainStateError> {
    let _writer = writer()?;
    bump_generation(workspace_id);
    Ok(change())
}

/// Why a change was not made.
#[derive(Debug)]
pub(crate) enum WorkSourceError {
    /// Refused before or after running anything; the text says why, already masked.
    Refused(String),
    /// The command ran and failed; `check` says how, for the person and for the log.
    Failed(CheckFailure),
    Storage(DomainStateError),
}

/// A failed run of `info`. Its `Debug` prints `declared_secret`'s size, never its names.
pub(crate) struct CheckFailure {
    /// What the person sees, masked and cut by the runner (`InfoFailure::masked_secret`).
    pub(crate) message: String,
    pub(crate) failure: CommandFailure,
    pub(crate) duration: Duration,
    /// For the log: the workspace and the source id, each through the same mask.
    pub(crate) workspace_id: String,
    pub(crate) source_id: Option<String>,
    /// The accepted info's fields that the failing answer declares secret, with a value or
    /// without (`InfoFailure::declared_secret`, less any name the accepted info doesn't have): the
    /// record makes these secret, and no others.
    pub(crate) declared_secret: Vec<String>,
    /// Every field the failing answer declares secret, those the accepted info doesn't have
    /// included (`InfoFailure::declared_secret`): only matched against the stored values' field
    /// names, to remove those too short to keep secret (`remove_short_secrets`), never kept.
    pub(crate) answer_secret: Vec<String>,
}

impl std::fmt::Debug for CheckFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CheckFailure")
            .field("message", &self.message)
            .field("failure", &self.failure)
            .field("duration", &self.duration)
            .field("workspace_id", &self.workspace_id)
            .field("source_id", &self.source_id)
            .field("declared_secret", &self.declared_secret.len())
            .field("answer_secret", &self.answer_secret.len())
            .finish()
    }
}

impl From<DomainStateError> for WorkSourceError {
    fn from(error: DomainStateError) -> Self {
        Self::Storage(error)
    }
}

/// The ids still named somewhere, which a retired id is never pruned for. Asked for inside the
/// writer's commit, right before pruning, so a link saved while `info` ran still counts.
pub(crate) type References<'a> = &'a dyn Fn() -> Result<BTreeSet<String>, DomainStateError>;

/// The ids a workspace's tracker or a session's hand-set `workLinks.tickets.<id>` names. A
/// failed read is an error, never "no references", which would let pruning drop a named id.
pub(crate) fn referenced_source_ids(
    repository: &DomainRepository,
    workspaces: &Value,
) -> Result<BTreeSet<String>, DomainStateError> {
    let mut ids: BTreeSet<String> = workspaces
        .get("workspaces")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(_, workspace)| workspace.get("tracker")?.as_str().map(str::to_string))
        .collect();
    for session in repository.list_sessions_with_runtime_object("workLinks")? {
        if let Some(tickets) = session
            .pointer("/runtimeSettings/workLinks/tickets")
            .and_then(Value::as_object)
        {
            ids.extend(tickets.keys().cloned());
        }
    }
    Ok(ids)
}

/// What a workspace's command is run with: its accepted declarations and their stored values.
pub(crate) fn command_env_of(
    paths: &GxserverPaths,
    workspace_id: &str,
    command: Option<&WorkspaceCommand>,
) -> CommandEnv {
    let Some(command) = command else {
        return CommandEnv::new(None, workspace_id, None, &[], None);
    };
    let credentials = WorkCredentials::new(paths);
    CommandEnv::new(
        Some(&command.source_id),
        workspace_id,
        None,
        &command.info.credentials,
        credentials.source_fields(workspace_id, &command.source_id),
    )
}

/// What a change that runs the command planned against, read together under the writer lock.
pub(super) struct Plan {
    pub(super) existing: Option<WorkspaceCommand>,
    pub(super) generation: u64,
    pub(super) env: CommandEnv,
}

/// Reads the plan. Refuses a workspace that is gone or connected to a team. The caller holds the
/// writer lock.
pub(super) fn plan(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
) -> Result<Plan, WorkSourceError> {
    ensure_workspace_may_set(db, paths, workspace_id)?;
    let existing = read_work_sources(db)?.workspaces.remove(workspace_id);
    let env = command_env_of(paths, workspace_id, existing.as_ref());
    Ok(Plan {
        existing,
        generation: work_source_generation(workspace_id),
        env,
    })
}

fn ensure_workspace_may_set(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
) -> Result<(), WorkSourceError> {
    if !crate::workspaces::workspace_exists(
        &crate::workspaces::read_sidebar_workspaces(db)?,
        workspace_id,
    ) {
        return Err(WorkSourceError::Refused("No such workspace.".to_string()));
    }
    if crate::team_sync::workspace_has_team(paths, workspace_id) {
        return Err(WorkSourceError::Refused(TEAM_REFUSAL.to_string()));
    }
    Ok(())
}

/// Whether what a plan read is still true: the same generation (no pin, path, credential or team
/// change since), and the workspace still there and not in a team. The caller holds the writer.
pub(super) fn still_current(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    generation: u64,
) -> Result<(), WorkSourceError> {
    if work_source_generation(workspace_id) != generation {
        return Err(WorkSourceError::Refused(CHANGED_MEANWHILE.to_string()));
    }
    ensure_workspace_may_set(db, paths, workspace_id)
}

/// Runs `info` for `program` with `env`, in the source's folder. `accepted` is the accepted
/// info's declarations, empty before the first one: a failure keeps only their names among the
/// fields it declares secret. `mask` is the change's one mask: its secrets mask the run's texts
/// as they are built and cut, and are never passed to the command (`CommandEnv::masking_too`), so
/// a value the command isn't given, such as one kept after a clear, can't come back raw either.
pub(super) fn check_info(
    paths: &GxserverPaths,
    program: &std::path::Path,
    env: &CommandEnv,
    accepted: &[CredentialField],
    mask: &CommandEnv,
) -> Result<CommandInfo, CheckFailure> {
    let env = &env.masking_too(mask);
    let started = Instant::now();
    let failed = |mut failed: InfoFailure| {
        let answer_secret = failed.declared_secret.clone();
        // A name the accepted info doesn't have is no field of the record, and could spell a
        // credential.
        failed
            .declared_secret
            .retain(|name| accepted.iter().any(|field| &field.name == name));
        // The runner's mask for this failure, so its name and ids are masked like its texts.
        let env = env.with_secret(&failed.masked_secret);
        let name = env.shown(env.source_id.as_deref().unwrap_or("The ticket command"));
        CheckFailure {
            message: failed.failure.message(&name),
            failure: failed.failure,
            duration: started.elapsed(),
            workspace_id: env.shown(&env.workspace_id),
            source_id: env.source_id.as_deref().map(|id| env.shown(id)),
            declared_secret: failed.declared_secret,
            answer_secret,
        }
    };
    let work_dir = command_work_dir(paths, env.source_id.as_deref()).map_err(|error| {
        failed(InfoFailure {
            failure: CommandFailure::Start(env.shown(&error.to_string())),
            masked_secret: env.secret_names(),
            declared_secret: Vec::new(),
        })
    })?;
    command_info(&CommandCall::new(
        program,
        CommandVerb::Info,
        None,
        &work_dir,
        env,
    ))
    .map_err(failed)
}

/// Which path a set runs `info` for.
pub(crate) enum SourcePath<'a> {
    /// A path the user typed.
    Typed(&'a str),
    /// The path the workspace's command already has, read under the writer lock: a check that
    /// runs `info` again.
    Stored,
}

/// What a change to a workspace's command answered, and the one mask its errors left through, for
/// the route to word its own errors after it with (`stored_values`).
#[derive(Debug)]
pub(crate) struct WorkSourceChange<T> {
    pub(crate) result: Result<T, WorkSourceError>,
    pub(crate) mask: CommandEnv,
}

impl<T> WorkSourceChange<T> {
    /// The change's errors through its mask, read under the writer lock before anything could be
    /// refused, so what the change words inside is raw. A failed check passes as the runner
    /// masked it (`WorkSourceError::masked`).
    fn through(result: Result<T, WorkSourceError>, mask: CommandEnv) -> Self {
        Self {
            result: result.map_err(|error| error.masked(&mask)),
            mask,
        }
    }

    /// A change refused before its mask could be read: its error is Ghostex's own fixed text.
    fn unmasked(error: WorkSourceError) -> Self {
        Self {
            result: Err(error),
            mask: CommandEnv::default(),
        }
    }
}

/// Sets a workspace's command to `path` (or checks it again with the path it has): resolves the
/// path, runs `info`, and pins the id it answers with. A command already pinned keeps its id: an
/// answer with another one is refused and offered for a Switch, which `switch` accepts.
///
/// Its errors leave through one mask, the stored values (`stored_values`), read under the
/// writer lock before the plan, like a credential save's. The same mask masks the check's own
/// texts as the runner builds them.
pub(crate) fn set_work_source(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    path: SourcePath,
    switch: bool,
    references: References,
) -> WorkSourceChange<WorkspaceCommand> {
    let held = match writer() {
        Ok(held) => held,
        Err(error) => return WorkSourceChange::unmasked(error.into()),
    };
    let stored = match stored_values(db, paths, workspace_id) {
        Ok(stored) => stored,
        Err(error) => return WorkSourceChange::unmasked(error),
    };
    let result = run_and_pin(
        db,
        paths,
        workspace_id,
        path,
        switch,
        references,
        &stored,
        held,
    );
    WorkSourceChange::through(result, stored.mask)
}

/// `set_work_source` without its mask's exit: what it words is raw. `held` is the writer lock
/// `stored` was read under.
///
/// CDXC:WorkMode 2026-11-02 WHY:
/// A path to another program than the pinned one's is asked without the pinned command's credentials, and its answer changes what is secret only when it answers with the pinned id; its failure changes nothing in the record. Otherwise `set ~/bin/wrong-tool` would hand the real tracker's token to that tool, and its refused answer could still make the pinned record's fields secret or remove their short saved values. So "any answer" in the decision on `remove_short_secrets` means any answer of the pinned command's program, or of another program answering with its id.
#[allow(clippy::too_many_arguments)]
fn run_and_pin(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    path: SourcePath,
    switch: bool,
    references: References,
    stored: &StoredValues,
    held: MutexGuard<'static, ()>,
) -> Result<WorkspaceCommand, WorkSourceError> {
    let (plan, typed_path, resolved) = {
        let _writer = held;
        let plan = plan(db, paths, workspace_id)?;
        let typed_path = match path {
            SourcePath::Typed(typed) => typed.trim().to_string(),
            SourcePath::Stored => plan
                .existing
                .as_ref()
                .map(|command| command.path.clone())
                .ok_or_else(|| WorkSourceError::Refused(NOTHING_TO_CHECK.to_string()))?,
        };
        // A cleared command keeps its credentials, but has no declarations for `plan.env`.
        // `stored` holds those retained values too, so a path that spells one is refused before
        // it resolves, and a refused path is quoted only through `stored.mask`.
        refuse_path_spelling(stored, &typed_path, None)?;
        let resolved = resolve_command_path(&typed_path).map_err(WorkSourceError::Refused)?;
        refuse_path_spelling(stored, &typed_path, Some(&resolved))?;
        (plan, typed_path, resolved)
    };
    // Another program than the pinned one's is asked without the pinned command's credentials,
    // and in no source's folder, until its answer is pinned.
    let pinned_program = plan
        .existing
        .as_ref()
        .filter(|command| command.resolved_path == resolved);
    let checked = match pinned_program {
        Some(command) => check_info(
            paths,
            &resolved,
            &plan.env,
            &command.info.credentials,
            &stored.mask,
        ),
        None => check_info(
            paths,
            &resolved,
            &command_env_of(paths, workspace_id, None),
            &[],
            &stored.mask,
        ),
    };

    let _writer = writer()?;
    still_current(db, paths, workspace_id, plan.generation)?;
    let mut sources = read_work_sources(db)?;
    // Another program's answer changes what is secret, in the record and among the stored
    // values, only when it answers with the pinned id. The pinned program's own answers always do,
    // and with nothing pinned every answer does.
    let secrecy_applies = match (&plan.existing, &checked) {
        (None, _) => true,
        (Some(_), _) if pinned_program.is_some() => true,
        (Some(pinned), Ok(info)) => pinned.source_id == info.id,
        (Some(_), Err(_)) => false,
    };
    let declares_secret = if secrecy_applies {
        answer_declares_secret(&checked)
    } else {
        Vec::new()
    };
    // Before anything is written, so a value it can't remove refuses the change whole.
    let removed = remove_short_secrets(paths, workspace_id, &declares_secret)
        .map_err(|_| WorkSourceError::Refused(SHORT_SECRET_UNREMOVABLE.to_string()))?;
    if removed {
        bump_generation(workspace_id);
    }
    let info = match checked {
        Ok(info) => info,
        Err(mut failure) => {
            failure.message = noted(failure.message, removed);
            // Another program's failure says nothing about the pinned command.
            if let Some(command) = sources
                .workspaces
                .get_mut(workspace_id)
                .filter(|_| pinned_program.is_some())
            {
                record_failure(command, &failure, paths, workspace_id);
                save_work_sources(db, &sources)?;
                bump_generation(workspace_id);
            }
            return Err(WorkSourceError::Failed(failure));
        }
    };
    // Whatever is refused below, the fields this answer declares secret are secret in the record
    // from now on, and none of their values stays in it raw.
    let tightened = secrecy_applies
        && sources
            .workspaces
            .get_mut(workspace_id)
            .is_some_and(|command| {
                let before = command.clone();
                tighten_and_sanitize(
                    command,
                    paths,
                    workspace_id,
                    &declared_secret(&info.credentials),
                );
                *command != before
            });
    let refuse =
        |sources: &mut WorkSources, text: String| -> Result<WorkspaceCommand, WorkSourceError> {
            let noted_in_record = removed
                && sources
                    .workspaces
                    .get_mut(workspace_id)
                    .is_some_and(|command| {
                        command.last_error = Some(SHORT_SECRET_REMOVED.to_string());
                        command.last_error_detail = None;
                        true
                    });
            if tightened || noted_in_record {
                save_work_sources(db, sources)?;
                bump_generation(workspace_id);
            }
            Err(WorkSourceError::Refused(noted(text, removed)))
        };
    // The runner already refused an answer whose id, ID keys or credential names hold a value
    // secret for it (`AnswerGuard::structural`, with the mask too); a stored value that isn't
    // secret is kept out of them here, before any of them is kept, an id offered for a Switch
    // included.
    if answer_kept_as_is(&info)
        .iter()
        .any(|(_, text)| stored.found_in(text))
    {
        return refuse(&mut sources, ANSWER_SPELLS_CREDENTIAL.to_string());
    }
    if let Some(pinned) = plan
        .existing
        .as_ref()
        .filter(|pinned| pinned.source_id != info.id)
    {
        if !switch {
            let refused = noted(
                format!(
                    "{} \"{}\"; this workspace's links use \"{}\". {SWITCH_OFFER}",
                    if pinned_program.is_some() {
                        "This command now calls itself"
                    } else {
                        "That command calls itself"
                    },
                    stored.mask.shown(&info.id),
                    stored.mask.shown(&pinned.source_id)
                ),
                removed,
            );
            // Only the pinned program's own answer is offered in the record; another program's is
            // switched to by setting its path again with `switch`.
            if let Some(command) = sources
                .workspaces
                .get_mut(workspace_id)
                .filter(|_| pinned_program.is_some())
            {
                command.offered_id = Some(info.id.clone());
                command.last_error = Some(refused.clone());
                command.last_error_detail = None;
                save_work_sources(db, &sources)?;
                bump_generation(workspace_id);
            }
            return Err(WorkSourceError::Refused(refused));
        }
    }
    let format_here = plan
        .existing
        .as_ref()
        .filter(|pinned| pinned.source_id == info.id)
        .map(|pinned| pinned.info.id_format);
    if let Some(format) = format_here.or_else(|| sources.format_of(&info.id, workspace_id)) {
        if format != info.id_format {
            return refuse(
                &mut sources,
                format!(
                    "\"{}\" sends {} ids now, and this computer already has it with {} ids. Clear it everywhere to change its id format.",
                    stored.mask.shown(&info.id),
                    format_word(info.id_format),
                    format_word(format)
                ),
            );
        }
    }
    if !sources.known_ids().contains(info.id.as_str())
        && sources.known_ids().len() >= MAX_SOURCE_IDS
    {
        return refuse(
            &mut sources,
            "This computer has too many ticket source ids; clear the ones you no longer use."
                .to_string(),
        );
    }
    let command = WorkspaceCommand {
        path: typed_path,
        resolved_path: resolved,
        source_id: info.id.clone(),
        info,
        checked_at: now_iso(),
        last_error: removed.then(|| SHORT_SECRET_REMOVED.to_string()),
        last_error_detail: None,
        offered_id: None,
    };
    // The record as it is now, masked, is what the retired list keeps of the old id.
    if let Some(old) = sources
        .workspaces
        .remove(workspace_id)
        .filter(|old| old.source_id != command.source_id)
    {
        retire(&mut sources, &old);
    }
    sources
        .retired
        .retain(|retired| retired.id != command.source_id);
    sources
        .workspaces
        .insert(workspace_id.to_string(), command.clone());
    prune_retired(&mut sources, &references()?);
    save_work_sources(db, &sources)?;
    bump_generation(workspace_id);
    Ok(command)
}

/// Clears a workspace's command. Its id is retired (unless another workspace pins it) and its
/// credentials stay, so setting the same command again finds them. A team workspace may clear a
/// command it had before joining. Its errors leave through the stored values' mask, like a set's.
pub(crate) fn clear_work_source(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    references: References,
) -> WorkSourceChange<()> {
    let _writer = match writer() {
        Ok(writer) => writer,
        Err(error) => return WorkSourceChange::unmasked(error.into()),
    };
    let mask = match stored_values(db, paths, workspace_id) {
        Ok(stored) => stored.mask,
        Err(error) => return WorkSourceChange::unmasked(error),
    };
    let cleared = (|| {
        let mut sources = read_work_sources(db)?;
        if let Some(command) = sources.workspaces.remove(workspace_id) {
            retire(&mut sources, &command);
            prune_retired(&mut sources, &references()?);
            save_work_sources(db, &sources)?;
        }
        bump_generation(workspace_id);
        Ok(())
    })();
    WorkSourceChange::through(cleared, mask)
}

/// A workspace deletion in progress: what it removed from the credentials file, and the store's
/// writer lock, held until the deletion is committed or undone, so no change to the workspace's
/// command or credentials can land in between (one would be left behind by a commit, or
/// overwritten by an undo).
#[must_use = "a deletion must be committed or undone"]
pub(crate) struct WorkSourceDeletion {
    workspace_id: String,
    removed_credentials: Option<Value>,
    /// The record it read and the one it wrote, published to the copy in memory on `commit`.
    published: Option<(WorkSources, WorkSources)>,
    _writer: MutexGuard<'static, ()>,
}

impl WorkSourceDeletion {
    /// The transaction it was made in committed: the copy in memory follows the record, then the
    /// writer lock is released.
    pub(crate) fn commit(self) {
        if let Some((read, written)) = &self.published {
            publish(read, written);
        }
        forget_workspace_ticket_titles(&self.workspace_id);
    }

    /// Puts the removed credentials back, merged into the file as it is now, then releases the
    /// writer lock. The record's change is undone by rolling back the transaction it was made in.
    pub(crate) fn undo(self, paths: &GxserverPaths) -> std::io::Result<()> {
        let Some(removed) = self.removed_credentials else {
            return Ok(());
        };
        mutate_credentials(paths, |credentials| {
            restore_workspace_source_credentials(credentials, &self.workspace_id, removed)
        })?;
        bump_generation(&self.workspace_id);
        Ok(())
    }
}

/// A workspace is being deleted: its command is cleared, its credentials removed and its tickets'
/// last titles deleted. Never fails on a bound, since retiring an id the record already holds
/// adds none.
///
/// CDXC:WorkMode 2026-10-31 WHY:
/// `/api/deleteWorkspace` stays allowed from any client for a workspace with a ticket command, and removes the command and its credentials, like the Linear key it already removes: deleting never sets a command, runs one or reveals a credential.
///
/// Call it inside a transaction on `db`, before the workspace document is written. Once the
/// transaction commits, `commit` it; if anything fails first, roll the transaction back (the
/// record) and `undo` it (the credentials file), so the workspace, its command and its
/// credentials all stay as they were. It holds the store's writer lock until then.
pub(crate) fn delete_workspace_work_source(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    references: References,
) -> Result<WorkSourceDeletion, WorkSourceError> {
    let writer = writer()?;
    let read = read_work_sources(db)?;
    let mut sources = read.clone();
    let mut published = None;
    if let Some(command) = sources.workspaces.remove(workspace_id) {
        retire(&mut sources, &command);
        prune_retired(&mut sources, &references()?);
        // Inside the caller's transaction: the copy in memory follows only once it commits.
        write_work_sources(db, &sources)?;
        published = Some((read, sources));
    }
    // Whether or not it has a command now: a cleared command's titles stay until the workspace
    // goes.
    delete_workspace_ticket_titles(db, workspace_id)?;
    // Written only when it held some, so a workspace without command credentials never needs the
    // file to be writable.
    let removed_credentials = mutate_credentials_when(paths, |credentials| {
        remove_workspace_source_credentials(credentials, workspace_id)
    })
    .map_err(|error| {
        WorkSourceError::Storage(DomainStateError {
            code: "internalError",
            message: format!("Could not remove the ticket command's credentials: {error}"),
        })
    })?;
    bump_generation(workspace_id);
    Ok(WorkSourceDeletion {
        workspace_id: workspace_id.to_string(),
        removed_credentials,
        published,
        _writer: writer,
    })
}

/// What a workspace has stored, read once under the writer lock before a change plans anything.
#[derive(Debug)]
pub(crate) struct StoredValues {
    /// The mask the change starts from: every stored value whose field the record marks secret. A
    /// stored value whose field no record declares (a cleared command's values are kept for when
    /// it is set again, and its flags went with it) counts as secret too from 3 characters on. A
    /// credential save adds its proposed values to it for its errors (`proposed_values_mask`).
    pub(crate) mask: CommandEnv,
    /// Every stored value, secret or not, kept ones included: none may appear in what the record
    /// keeps as it is (`appears_in`). A value that isn't secret is only compared, never masked.
    pub(crate) values: Vec<String>,
}

impl StoredValues {
    /// Whether `text`, which the record would keep as it is, holds a stored value.
    pub(crate) fn found_in(&self, text: &str) -> bool {
        self.values.iter().any(|value| appears_in(text, value))
    }
}

/// The one comparison of the rule that keeps credentials out of what the record keeps as it is,
/// used both ways: a proposed value against the record (`refuse_unmaskable`), and the stored
/// values against a path or an answer (`StoredValues::found_in`). A value shorter than
/// `MIN_MASKED_IN_PLACE_CHARS` is exempt: a secret that short is refused when saved
/// (`credential_changes`), and one that isn't secret would refuse almost any text, a workspace
/// id's 32 hex digits included.
pub(super) fn appears_in(text: &str, value: &str) -> bool {
    value.chars().count() >= MIN_MASKED_IN_PLACE_CHARS && text.contains(value)
}

/// Reads what the workspace has stored (`StoredValues`). A failed read refuses the change, and
/// runs no background call: an empty mask would show what it should hide, and an empty list
/// would let a value in.
pub(crate) fn stored_values(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
) -> Result<StoredValues, WorkSourceError> {
    let mut mask = CommandEnv::default();
    let mut values = Vec::new();
    // An unreadable store answers in its own fixed words, which quote nothing it holds.
    let command = read_work_sources(db)?.workspaces.remove(workspace_id);
    let stored = read_workspace_source_credentials(paths, workspace_id).map_err(|error| {
        WorkSourceError::Refused(format!(
            "Ghostex couldn't read the credentials already stored, so it changed nothing: {error}"
        ))
    })?;
    for (source_id, fields) in &stored {
        for (name, value) in fields.as_object().into_iter().flatten() {
            let Some(value) = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            values.push(value.to_string());
            let declared = command
                .as_ref()
                .filter(|command| &command.source_id == source_id)
                .and_then(|command| {
                    command
                        .info
                        .credentials
                        .iter()
                        .find(|field| &field.name == name)
                });
            let secret = match declared {
                Some(field) => field.secret,
                // WHY: its flag is unknown; a shorter one would withhold almost any text.
                None => value.chars().count() >= MIN_MASKED_IN_PLACE_CHARS,
            };
            if secret {
                mask.credentials.push(CommandCredential {
                    name: name.clone(),
                    value: value.to_string(),
                    secret: true,
                });
            }
        }
    }
    Ok(StoredValues { mask, values })
}

impl WorkSourceError {
    /// This error with its text through `mask`. A failed check's texts are the runner's, already
    /// masked and cut with the fields secret for them, and pass as they are.
    pub(crate) fn masked(self, mask: &CommandEnv) -> Self {
        match self {
            Self::Refused(text) => Self::Refused(mask.shown(&text)),
            failed @ Self::Failed(_) => failed,
            Self::Storage(mut error) => {
                error.message = mask.shown(&error.message);
                Self::Storage(error)
            }
        }
    }
}

/// What an accepted answer puts in the record that the record keeps as it is, each with what it
/// is: its id, its ID keys and its credential names.
pub(super) fn answer_kept_as_is(info: &CommandInfo) -> Vec<(&'static str, &str)> {
    let mut kept = vec![("source id", info.id.as_str())];
    kept.extend(info.id_keys.iter().map(|key| ("ID keys", key.as_str())));
    kept.extend(
        info.credentials
            .iter()
            .map(|field| ("credential names", field.name.as_str())),
    );
    kept
}

/// Refuses a path, as typed or as resolved, that holds a stored value, secret or not. The record
/// keeps both as they are, and only a credential's own refusal keeps a value out of them
/// (`refuse_unmaskable`), so a path holding a value stored already is refused here, before it is
/// run or kept. The typed path is checked before it is resolved too, so the resolver's refusal
/// never quotes one; this refusal quotes only the typed path.
pub(crate) fn refuse_path_spelling(
    stored: &StoredValues,
    typed: &str,
    resolved: Option<&std::path::Path>,
) -> Result<(), WorkSourceError> {
    let resolved = resolved.map(|path| path.display().to_string());
    if stored.found_in(typed) || resolved.is_some_and(|path| stored.found_in(&path)) {
        // The typed path, never the resolved one, leaves through the change's mask: what it still shows is a value that isn't secret or has 1 or 2 characters, which the workspace shows openly anyway.
        return Err(WorkSourceError::Refused(format!(
            "\"{typed}\" {PATH_SPELLS_CREDENTIAL}"
        )));
    }
    Ok(())
}

/// Masks what the record keeps for display (the accepted info's name, labels and placeholders,
/// the last error and its detail) with `env`'s secrets, and drops an icon that spells one.
pub(super) fn sanitize(command: &mut WorkspaceCommand, env: &CommandEnv) {
    let info = &mut command.info;
    info.name = fit_name(&env.redact(&info.name));
    for field in &mut info.credentials {
        field.label = env.redact(&field.label);
        field.placeholder = field.placeholder.as_deref().map(|text| env.redact(text));
    }
    info.icon = info.icon.take().map(|icon| match icon {
        Ok(svg) if env.reveals_secret(&svg) => Err(ICON_REFUSED.to_string()),
        Ok(svg) => Ok(svg),
        Err(reason) => Err(env.redact(&reason)),
    });
    command.last_error = command.last_error.as_deref().map(|text| env.redact(text));
    command.last_error_detail = command
        .last_error_detail
        .as_deref()
        .map(|text| env.redact(text));
}

/// After a failed check: every field the record has that the failing answer declares secret is
/// secret from now on, with a value or without, so a value saved later is held to the secret
/// rules; the runner masked the failure's texts with those that have a value. None of their values
/// stays in the record raw. Stdout that isn't JSON declares nothing, so it makes no field secret.
pub(super) fn record_failure(
    command: &mut WorkspaceCommand,
    failure: &CheckFailure,
    paths: &GxserverPaths,
    workspace_id: &str,
) {
    command.last_error = Some(failure.message.clone());
    command.last_error_detail = match &failure.failure {
        CommandFailure::Unreadable { stdout, .. } => Some(stdout.clone()),
        _ => None,
    };
    command.offered_id = None;
    tighten_and_sanitize(command, paths, workspace_id, &failure.declared_secret);
}

/// The fields an answer declares secret, accepted or not: a failing one's are what its JSON
/// declares (`CheckFailure::answer_secret`).
pub(super) fn answer_declares_secret(checked: &Result<CommandInfo, CheckFailure>) -> Vec<String> {
    match checked {
        Ok(info) => declared_secret(&info.credentials),
        Err(failure) => failure.answer_secret.clone(),
    }
}

/// Removes every value the workspace stored, for any source, of a field named in `secret` whose
/// trimmed value is shorter than `MIN_MASKED_IN_PLACE_CHARS`: it can't be kept secret, the same
/// rule as saving one (`credential_changes`). Matched by field name, so a cleared command's
/// values, an old id's after a Switch and a field the record no longer lists go too. Whether it
/// removed any. Through the strict writer (`mutate_credentials_when`): a file it can't change
/// drops the check's answer, like a file `stored_values` can't read.
///
/// CDXC:WorkMode 2026-11-02 WHY:
/// A value of 1 or 2 characters is never secret and never hidden, not even in the request that removes it: it was shown openly while it wasn't secret, so hiding it then protects nothing. When any answer marks its field secret, Ghostex removes that saved value, the same rule as saving one, says why and asks for one of at least 3 characters. The record's shape stays as it is.
pub(super) fn remove_short_secrets(
    paths: &GxserverPaths,
    workspace_id: &str,
    secret: &[String],
) -> std::io::Result<bool> {
    if secret.is_empty() {
        return Ok(false);
    }
    mutate_credentials_when(paths, |credentials| {
        remove_source_credentials_where(credentials, workspace_id, |name, value| {
            secret.iter().any(|secret| secret == name)
                && (1..MIN_MASKED_IN_PLACE_CHARS).contains(&value.chars().count())
        })
        .then_some(())
    })
    .map(|removed| removed.is_some())
}

/// `text`, after `SHORT_SECRET_REMOVED` when the change removed a value: first, so that a cut to
/// `MAX_MESSAGE_CHARS` never drops it and a refusal still ends with `SWITCH_OFFER`.
pub(super) fn noted(text: String, removed: bool) -> String {
    if removed {
        format!("{SHORT_SECRET_REMOVED} {text}")
    } else {
        text
    }
}

/// The fields an answer declares secret.
pub(super) fn declared_secret(fields: &[CredentialField]) -> Vec<String> {
    fields
        .iter()
        .filter(|field| field.secret)
        .map(|field| field.name.clone())
        .collect()
}

/// Makes secret every field named in `secret`, never the reverse; then masks what the record
/// keeps with the values that are secret now, so it holds none of them raw.
pub(super) fn tighten_and_sanitize(
    command: &mut WorkspaceCommand,
    paths: &GxserverPaths,
    workspace_id: &str,
    secret: &[String],
) {
    for field in &mut command.info.credentials {
        field.secret |= secret.contains(&field.name);
    }
    let env = command_env_of(paths, workspace_id, Some(command));
    sanitize(command, &env);
}

/// Moves a workspace's id to the retired list, unless another workspace still pins it. Takes no
/// new place among `MAX_SOURCE_IDS`: the id is already known.
fn retire(sources: &mut WorkSources, command: &WorkspaceCommand) {
    let still_pinned = sources
        .workspaces
        .values()
        .any(|other| other.source_id == command.source_id);
    if still_pinned
        || sources
            .retired
            .iter()
            .any(|retired| retired.id == command.source_id)
    {
        return;
    }
    sources.retired.push(RetiredSource {
        id: command.source_id.clone(),
        format: command.info.id_format,
        name: command.info.name.clone(),
        icon: command
            .info
            .icon
            .as_ref()
            .and_then(|icon| icon.as_ref().ok())
            .cloned(),
        retired_at: now_iso(),
    });
}

/// Drops the oldest retired ids nothing refers to until `RETIRED_PRUNING_TARGET` are left; ids
/// a saved link or a tracker still names stay, past the target if need be.
fn prune_retired(sources: &mut WorkSources, referenced: &BTreeSet<String>) {
    let mut excess = sources.retired.len().saturating_sub(RETIRED_PRUNING_TARGET);
    sources.retired.retain(|retired| {
        if excess > 0 && !referenced.contains(&retired.id) {
            excess -= 1;
            return false;
        }
        true
    });
}
