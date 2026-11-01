//! Every change to the ticket command record, under one writer lock: setting or re-checking a
//! workspace's command, switching to the id its command now answers with, clearing it, deleting
//! its workspace, and storing its credentials.
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
use serde_json::{Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;

use super::*;

/// A credential value: a single line of at most this many bytes.
const MAX_CREDENTIAL_BYTES: usize = 4 * 1024;

pub(crate) const TEAM_REFUSAL: &str = "A workspace connected to a team uses its team's tracker; ticket commands are for this computer's workspaces.";

const CHANGED_MEANWHILE: &str =
    "The ticket command's setup changed while it was being checked; try again.";

const STORED_VALUES_UNREADABLE: &str =
    "Ghostex couldn't read the credentials already stored, so it saved nothing; try again.";

static WRITER: Mutex<()> = Mutex::new(());

fn writer() -> MutexGuard<'static, ()> {
    WRITER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether some change holds the store's writer lock right now.
#[cfg(test)]
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

fn bump_generation(workspace_id: &str) {
    if let Ok(mut generations) = generations().lock() {
        *generations.entry(workspace_id.to_string()).or_default() += 1;
    }
}

/// Runs `change` under the store's writer lock after bumping the workspace's generation: for a
/// change made elsewhere that alters what its ticket command may run with (joining or leaving a
/// team). `change` may take the credentials file's lock; it must not take this one again.
pub(crate) fn change_work_source_config<T>(workspace_id: &str, change: impl FnOnce() -> T) -> T {
    let _writer = writer();
    bump_generation(workspace_id);
    change()
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
struct Plan {
    existing: Option<WorkspaceCommand>,
    generation: u64,
    env: CommandEnv,
}

/// Reads the plan. Refuses a workspace that is gone or connected to a team. The caller holds the
/// writer lock.
fn plan(
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
fn still_current(
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
/// fields it declares secret.
fn check_info(
    paths: &GxserverPaths,
    program: &std::path::Path,
    env: &CommandEnv,
    accepted: &[CredentialField],
) -> Result<CommandInfo, CheckFailure> {
    let started = Instant::now();
    let failed = |mut failed: InfoFailure| {
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

/// Sets a workspace's command to `typed_path` (or re-checks it with the same path): resolves the
/// path, runs `info`, and pins the id it answers with. A command already pinned keeps its id: an
/// answer with another one is refused and offered for a Switch, which `switch` accepts.
pub(crate) fn set_work_source(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    typed_path: &str,
    switch: bool,
    references: References,
) -> Result<WorkspaceCommand, WorkSourceError> {
    let (plan, resolved) = {
        let _writer = writer();
        let plan = plan(db, paths, workspace_id)?;
        // A cleared command keeps its credentials, but has no declarations for `plan.env`.
        // Include those retained values before quoting a refused path.
        let resolved = match resolve_command_path(typed_path) {
            Ok(resolved) => resolved,
            Err(error) => {
                let mask = credential_save_mask(db, paths, workspace_id, &Map::new())?;
                return Err(WorkSourceError::Refused(mask.shown(&error)));
            }
        };
        (plan, resolved)
    };
    let accepted = plan
        .existing
        .as_ref()
        .map_or(&[][..], |command| &command.info.credentials[..]);
    let checked = check_info(paths, &resolved, &plan.env, accepted);

    let _writer = writer();
    still_current(db, paths, workspace_id, plan.generation)?;
    let mut sources = read_work_sources(db)?;
    let env = &plan.env;
    let info = match checked {
        Ok(info) => info,
        Err(failure) => {
            if let Some(command) = sources.workspaces.get_mut(workspace_id) {
                record_failure(command, &failure, paths, workspace_id);
                write_work_sources(db, &sources)?;
                bump_generation(workspace_id);
            }
            return Err(WorkSourceError::Failed(failure));
        }
    };
    // Whatever is refused below, the fields this answer declares secret are secret in the record
    // from now on, and none of their values stays in it raw.
    let tightened = sources
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
        |sources: &WorkSources, text: String| -> Result<WorkspaceCommand, WorkSourceError> {
            if tightened {
                write_work_sources(db, sources)?;
                bump_generation(workspace_id);
            }
            Err(WorkSourceError::Refused(text))
        };
    if let Some(pinned) = plan
        .existing
        .as_ref()
        .filter(|pinned| pinned.source_id != info.id)
    {
        if !switch {
            let refused = format!(
                "This command now calls itself \"{}\"; this workspace's links use \"{}\". Switch to it to use the new id.",
                env.shown(&info.id),
                env.shown(&pinned.source_id)
            );
            if let Some(command) = sources.workspaces.get_mut(workspace_id) {
                command.offered_id = Some(info.id.clone());
                command.last_error = Some(env.redact(&refused));
                command.last_error_detail = None;
            }
            write_work_sources(db, &sources)?;
            bump_generation(workspace_id);
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
                &sources,
                format!(
                    "\"{}\" sends {} ids now, and this computer already has it with {} ids. Clear it everywhere to change its id format.",
                    env.shown(&info.id),
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
            &sources,
            "This computer has too many ticket source ids; clear the ones you no longer use."
                .to_string(),
        );
    }
    let command = WorkspaceCommand {
        path: typed_path.trim().to_string(),
        resolved_path: resolved,
        source_id: info.id.clone(),
        info,
        checked_at: now_iso(),
        last_error: None,
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
    write_work_sources(db, &sources)?;
    bump_generation(workspace_id);
    Ok(command)
}

/// Clears a workspace's command. Its id is retired (unless another workspace pins it) and its
/// credentials stay, so setting the same command again finds them. A team workspace may clear a
/// command it had before joining.
pub(crate) fn clear_work_source(
    db: &Connection,
    workspace_id: &str,
    references: References,
) -> Result<(), WorkSourceError> {
    let _writer = writer();
    let mut sources = read_work_sources(db)?;
    if let Some(command) = sources.workspaces.remove(workspace_id) {
        retire(&mut sources, &command);
        prune_retired(&mut sources, &references()?);
        write_work_sources(db, &sources)?;
    }
    bump_generation(workspace_id);
    Ok(())
}

/// A workspace deletion in progress: what it removed from the credentials file, and the store's
/// writer lock, held until the deletion is committed or undone, so no change to the workspace's
/// command or credentials can land in between (one would be left behind by a commit, or
/// overwritten by an undo).
#[must_use = "a deletion must be committed or undone"]
pub(crate) struct WorkSourceDeletion {
    workspace_id: String,
    removed_credentials: Option<Value>,
    _writer: MutexGuard<'static, ()>,
}

impl WorkSourceDeletion {
    /// The transaction it was made in committed: the writer lock is released.
    pub(crate) fn commit(self) {}

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

/// A workspace is being deleted: its command is cleared and its credentials removed. Never fails
/// on a bound, since retiring an id the record already holds adds none.
///
/// CDXC:WorkMode 2026-10-31 DECISION:
/// Leandro: `/api/deleteWorkspace` stays allowed from any client for a workspace with a ticket command, and removes the command and its credentials, like the Linear key it already removes, because deleting never sets a command, runs one or reveals a credential.
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
    let writer = writer();
    let mut sources = read_work_sources(db)?;
    if let Some(command) = sources.workspaces.remove(workspace_id) {
        retire(&mut sources, &command);
        prune_retired(&mut sources, &references()?);
        write_work_sources(db, &sources)?;
    }
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
        _writer: writer,
    })
}

/// Stores credential values (`null` or blank removes one) for the workspace's command, then
/// checks `info` again with them, since its keys may depend on what they can see. The values are
/// kept even when that check fails; the failure comes back for the person to see.
///
/// Its own errors leave through one mask, read once under the writer lock before anything is
/// planned (`credential_save_mask`) and applied once here, so what it words inside is raw. A failed
/// check is not masked again: the runner masked and cut its texts with every field that has a value
/// and is secret for it, and the record makes secret what it declares (`CheckFailure`).
pub(crate) fn set_work_source_credentials(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    values: &Map<String, Value>,
) -> Result<CredentialsSaved, WorkSourceError> {
    let held = writer();
    let mask = credential_save_mask(db, paths, workspace_id, values)?;
    match store_credentials(db, paths, workspace_id, values, held) {
        Ok((command, failure)) => Ok(CredentialsSaved {
            command,
            failure,
            mask,
        }),
        Err(error) => Err(error.masked(&mask)),
    }
}

/// A credential save that stored its values: the record after it, its check's failure, and the
/// mask its errors leave through, for the route to word its own errors with.
#[derive(Debug)]
pub(crate) struct CredentialsSaved {
    pub(crate) command: WorkspaceCommand,
    pub(crate) failure: Option<CheckFailure>,
    pub(crate) mask: CommandEnv,
}

/// The one mask a credential save's errors leave through, read under the writer lock before the
/// save plans anything: every proposed value (`proposed_values_mask`), and every value the
/// workspace has stored whose field the record marks secret. A stored value whose field no record
/// declares (a cleared command's values are kept for when it is set again, and its flags went with
/// it) counts as secret too. A failed read refuses the save: an empty mask would show what it
/// should hide.
fn credential_save_mask(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    values: &Map<String, Value>,
) -> Result<CommandEnv, WorkSourceError> {
    let unreadable = || WorkSourceError::Refused(STORED_VALUES_UNREADABLE.to_string());
    let command = read_work_sources(db)
        .map_err(|_| unreadable())?
        .workspaces
        .remove(workspace_id);
    let stored = read_workspace_source_credentials(paths, workspace_id).map_err(|error| {
        WorkSourceError::Refused(format!(
            "Ghostex couldn't read the credentials already stored, so it saved nothing: {error}"
        ))
    })?;
    let mut mask = proposed_values_mask(values);
    for (source_id, fields) in &stored {
        for (name, value) in fields.as_object().into_iter().flatten() {
            let Some(value) = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
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
    Ok(mask)
}

/// Every string value of at least `MIN_MASKED_IN_PLACE_CHARS` a credential save proposes, each a
/// secret from the moment it arrives whatever field it is for: an early refusal can come before
/// its field is known. The route masks an error it words before naming a workspace with it.
pub(crate) fn proposed_values_mask(values: &Map<String, Value>) -> CommandEnv {
    CommandEnv {
        credentials: values
            .iter()
            .filter_map(|(name, value)| {
                let value = value.as_str()?.trim();
                // WHY: a shorter secret is refused (`credential_changes`); as a mask it would
                // withhold almost any text.
                (value.chars().count() >= MIN_MASKED_IN_PLACE_CHARS).then(|| CommandCredential {
                    name: name.clone(),
                    value: value.to_string(),
                    secret: true,
                })
            })
            .collect(),
        ..CommandEnv::default()
    }
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

/// `set_work_source_credentials` without its mask: what it words is raw. `held` is the writer
/// lock its mask was read under.
fn store_credentials(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    values: &Map<String, Value>,
    held: MutexGuard<'static, ()>,
) -> Result<(WorkspaceCommand, Option<CheckFailure>), WorkSourceError> {
    // Planned, checked and stored in the writer section the mask was read in, so nothing can
    // change in between.
    let (current, stored) = {
        let _writer = held;
        let plan = plan(db, paths, workspace_id)?;
        let mut current = plan
            .existing
            .ok_or_else(|| WorkSourceError::Refused("Set the ticket command first.".to_string()))?;
        let changes = credential_changes(&current.info, values)?;
        for (name, value) in &changes {
            if let Some(value) = value {
                refuse_unmaskable(&current, workspace_id, name, value)?;
            }
        }
        mutate_credentials(paths, |credentials| {
            store_source_credentials(credentials, workspace_id, &current.source_id, &changes)
        })
        .map_err(|error| {
            WorkSourceError::Refused(format!("Could not save the credentials: {error}"))
        })?;
        // What the record already holds is masked with the new values, so none of them is kept
        // in it, or shown from it, as it was before they were secret.
        let mut sources = read_work_sources(db)?;
        if let Some(command) = sources.workspaces.get_mut(workspace_id) {
            sanitize(
                command,
                &command_env_of(paths, workspace_id, Some(&current)),
            );
            current = command.clone();
            write_work_sources(db, &sources)?;
        }
        bump_generation(workspace_id);
        (current, work_source_generation(workspace_id))
    };

    let env = command_env_of(paths, workspace_id, Some(&current));
    let checked = check_info(
        paths,
        &current.resolved_path,
        &env,
        &current.info.credentials,
    );
    let _writer = writer();
    if still_current(db, paths, workspace_id, stored).is_err() {
        // Something changed while `info` ran: its answer is dropped, the values stay stored.
        return Ok((current, None));
    }
    let mut sources = read_work_sources(db)?;
    let Some(command) = sources.workspaces.get_mut(workspace_id) else {
        return Ok((current, None));
    };
    let failure = match checked {
        Ok(info) if info.id != command.source_id => {
            command.offered_id = Some(info.id.clone());
            command.last_error = Some(format!(
                "This command now calls itself \"{}\"; this workspace's links use \"{}\".",
                env.shown(&info.id),
                env.shown(&command.source_id)
            ));
            command.last_error_detail = None;
            tighten_and_sanitize(
                command,
                paths,
                workspace_id,
                &declared_secret(&info.credentials),
            );
            None
        }
        Ok(info) if info.id_format != command.info.id_format => {
            command.offered_id = None;
            command.last_error = Some(format!(
                "It sends {} ids now; this workspace's links use {} ids.",
                format_word(info.id_format),
                format_word(command.info.id_format)
            ));
            command.last_error_detail = None;
            tighten_and_sanitize(
                command,
                paths,
                workspace_id,
                &declared_secret(&info.credentials),
            );
            None
        }
        Ok(info) => {
            command.info = info;
            command.checked_at = now_iso();
            command.last_error = None;
            command.last_error_detail = None;
            command.offered_id = None;
            None
        }
        Err(failure) => {
            record_failure(command, &failure, paths, workspace_id);
            Some(failure)
        }
    };
    let command = command.clone();
    write_work_sources(db, &sources)?;
    bump_generation(workspace_id);
    Ok((command, failure))
}

/// The changes a request asks for: only fields the accepted info declares, each a single line of
/// at most `MAX_CREDENTIAL_BYTES`, or `null` to remove it. A secret field's value must be at
/// least `MIN_MASKED_IN_PLACE_CHARS` long: a shorter one can't be masked where it appears (a
/// message holding it is withheld whole), and as a team number it would refuse every ticket id
/// containing it.
fn credential_changes(
    info: &CommandInfo,
    values: &Map<String, Value>,
) -> Result<Vec<(String, Option<String>)>, WorkSourceError> {
    let refused = |text: String| WorkSourceError::Refused(text);
    values
        .iter()
        .map(|(name, value)| {
            let Some(field) = info.credentials.iter().find(|field| &field.name == name) else {
                return Err(refused(format!(
                    "\"{name}\" isn't a credential this command asks for."
                )));
            };
            let value = match value {
                Value::Null => None,
                Value::String(text) => Some(text.trim().to_string()),
                _ => {
                    return Err(refused(format!(
                        "{} must be text or null.",
                        field.label
                    )))
                }
            };
            if let Some(text) = &value {
                if text.len() > MAX_CREDENTIAL_BYTES {
                    return Err(refused(format!(
                        "{} is longer than 4 KB.",
                        field.label
                    )));
                }
                if text.contains(['\n', '\r']) {
                    return Err(refused(format!(
                        "{} must be a single line.",
                        field.label
                    )));
                }
                if field.secret
                    && !text.is_empty()
                    && text.chars().count() < MIN_MASKED_IN_PLACE_CHARS
                {
                    return Err(refused(format!(
                        "{}: a value this short can't be kept secret; a secret needs at least {MIN_MASKED_IN_PLACE_CHARS} characters. If it isn't secret, the command's info should mark it \"secret\": false.",
                        field.label
                    )));
                }
            }
            Ok((name.clone(), value))
        })
        .collect()
}

/// Refuses a value that appears in something the record keeps as it is and Ghostex can't mask
/// without changing what it names: the source id, the workspace id, the command's path, an ID
/// key, a credential's name, or an id offered for a Switch.
fn refuse_unmaskable(
    command: &WorkspaceCommand,
    workspace_id: &str,
    name: &str,
    value: &str,
) -> Result<(), WorkSourceError> {
    if value.is_empty() {
        return Ok(());
    }
    let resolved = command.resolved_path.display().to_string();
    let mut fixed: Vec<(&str, &str)> = vec![
        ("source id", command.source_id.as_str()),
        ("workspace id", workspace_id),
        ("path", command.path.as_str()),
        ("path", resolved.as_str()),
    ];
    fixed.extend(command.offered_id.as_deref().map(|id| ("offered id", id)));
    fixed.extend(
        command
            .info
            .id_keys
            .iter()
            .map(|key| ("ID keys", key.as_str())),
    );
    fixed.extend(
        command
            .info
            .credentials
            .iter()
            .map(|field| ("credential names", field.name.as_str())),
    );
    match fixed.iter().find(|(_, text)| text.contains(value)) {
        Some((what, _)) => Err(WorkSourceError::Refused(format!(
            "{name} can't be saved: its value also appears in the command's {what}, which Ghostex can't hide."
        ))),
        None => Ok(()),
    }
}

/// Masks what the record keeps for display (the accepted info's name, labels and placeholders,
/// the last error and its detail) with `env`'s secrets, and drops an icon that spells one.
fn sanitize(command: &mut WorkspaceCommand, env: &CommandEnv) {
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
fn record_failure(
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

/// The fields an answer declares secret.
fn declared_secret(fields: &[CredentialField]) -> Vec<String> {
    fields
        .iter()
        .filter(|field| field.secret)
        .map(|field| field.name.clone())
        .collect()
}

/// Makes secret every field named in `secret`, never the reverse; then masks what the record
/// keeps with the values that are secret now, so it holds none of them raw.
fn tighten_and_sanitize(
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
