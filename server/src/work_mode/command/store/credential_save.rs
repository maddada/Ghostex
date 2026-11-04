//! Storing a workspace's ticket command credentials (`set_work_source_credentials`): under the
//! store's writer lock, the values are checked and written, then the command's `info` runs again
//! with them, and its answer is kept only if nothing changed meanwhile. The rest of the record's
//! changes, and the lock and the mask they share, are in `changes.rs`.

use rusqlite::Connection;
use serde_json::{Map, Value};
use std::sync::MutexGuard;

use crate::paths::GxserverPaths;

use super::changes::*;
use super::*;
// This module's own, not the contract's (which reads an answer's JSON).
use super::changes::declared_secret;

/// A credential value: one line of at most this many bytes, without a NUL character.
const MAX_CREDENTIAL_BYTES: usize = 4 * 1024;

/// A credential save whose check marked a field secret whose saved value is too short to keep
/// secret, when Ghostex couldn't remove that value. The save's values were written before its
/// check ran, so they stay; only the check's answer is dropped.
pub(crate) const SAVED_BUT_SHORT_SECRET_UNREMOVABLE: &str = "Ghostex saved the values, but the command now marks secret a credential whose saved value is too short to keep secret, and Ghostex couldn't remove it, so it didn't use the check's answer. Save one of at least 3 characters.";

/// Stores credential values (`null` or blank removes one) for the workspace's command, then
/// checks `info` again with them, since its keys may depend on what they can see. The values are
/// kept even when that check fails; the failure comes back for the person to see.
///
/// Its own errors leave through one mask, read once under the writer lock before anything is
/// planned (`stored_values`) and applied once here, so what it words inside is raw. A failed
/// check is not masked again: the runner masked and cut its texts with every field that has a value
/// and is secret for it, and the record makes secret what it declares (`CheckFailure`).
pub(crate) fn set_work_source_credentials(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    values: &Map<String, Value>,
) -> Result<CredentialsSaved, WorkSourceError> {
    let held = writer()?;
    let mut stored = stored_values(db, paths, workspace_id)?;
    let mask = proposed_values_mask(values).masking_too(&stored.mask);
    // Its check runs with the values it stores, as they are declared; what it masks besides is
    // only what the workspace kept as secret, never a proposed value declared not secret. Its
    // answer is held to the values it stores too; WHY: one it removes is still compared for this
    // one check, which only refuses more.
    stored.values.extend(
        values
            .values()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    );
    match store_credentials(db, paths, workspace_id, values, &stored, &mask, held) {
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

/// `set_work_source_credentials` without its mask's exit: what it words is raw. Its check is also
/// masked with `stored`'s mask, the values the workspace stored as secret before it, and its
/// answer is refused when it holds one of `stored`'s values. `held` is the writer lock `stored`
/// was read under. `mask` is the save's, which what it words after its check is worded through.
fn store_credentials(
    db: &Connection,
    paths: &GxserverPaths,
    workspace_id: &str,
    values: &Map<String, Value>,
    stored: &StoredValues,
    mask: &CommandEnv,
    held: MutexGuard<'static, ()>,
) -> Result<(WorkspaceCommand, Option<CheckFailure>), WorkSourceError> {
    // Planned, checked and stored in the writer section the mask was read in, so nothing can
    // change in between.
    let (current, generation) = {
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
            save_work_sources(db, &sources)?;
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
        &stored.mask,
    );
    let _writer = writer()?;
    if still_current(db, paths, workspace_id, generation).is_err() {
        // Something changed while `info` ran: its answer is dropped, the values stay stored.
        return Ok((current, None));
    }
    let mut sources = read_work_sources(db)?;
    let Some(command) = sources.workspaces.get_mut(workspace_id) else {
        return Ok((current, None));
    };
    // As for a set (`run_and_pin`), before anything else is written. The values were written
    // before the check ran, so a value it can't remove leaves them saved and drops only the
    // check's answer: none of its declarations or its info is applied.
    let Ok(removed) = remove_short_secrets(paths, workspace_id, &answer_declares_secret(&checked))
    else {
        command.last_error = Some(SAVED_BUT_SHORT_SECRET_UNREMOVABLE.to_string());
        command.last_error_detail = None;
        let command = command.clone();
        save_work_sources(db, &sources)?;
        bump_generation(workspace_id);
        return Ok((command, None));
    };
    let failure = match checked {
        // The values stay stored; the answer is dropped before any of it is kept, an id offered
        // for a Switch included.
        Ok(info)
            if answer_kept_as_is(&info)
                .iter()
                .any(|(_, text)| stored.found_in(text)) =>
        {
            command.offered_id = None;
            command.last_error = Some(noted(ANSWER_SPELLS_CREDENTIAL.to_string(), removed));
            command.last_error_detail = None;
            tighten_and_sanitize(
                command,
                paths,
                workspace_id,
                &declared_secret(&info.credentials),
            );
            None
        }
        Ok(info) if info.id != command.source_id => {
            command.offered_id = Some(info.id.clone());
            command.last_error = Some(noted(
                format!(
                    "This command now calls itself \"{}\"; this workspace's links use \"{}\".",
                    mask.shown(&info.id),
                    mask.shown(&command.source_id)
                ),
                removed,
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
            command.last_error = Some(noted(
                format!(
                    "It sends {} ids now; this workspace's links use {} ids.",
                    format_word(info.id_format),
                    format_word(command.info.id_format)
                ),
                removed,
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
            command.last_error = removed.then(|| SHORT_SECRET_REMOVED.to_string());
            command.last_error_detail = None;
            command.offered_id = None;
            None
        }
        Err(mut failure) => {
            failure.message = noted(failure.message, removed);
            record_failure(command, &failure, paths, workspace_id);
            Some(failure)
        }
    };
    let command = command.clone();
    save_work_sources(db, &sources)?;
    bump_generation(workspace_id);
    Ok((command, failure))
}

/// The changes a request asks for: only fields the accepted info declares, each one line of at
/// most `MAX_CREDENTIAL_BYTES` without a NUL character, or `null` to remove it. A secret field's value must be at
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
                // A NUL keeps the command from starting: the value reaches it as an environment
                // variable. A line break is refused because a credential is one line (the CLI
                // reads one from stdin).
                if text.contains(['\0', '\n', '\r']) {
                    return Err(refused(format!(
                        "{} must be a single line, without a NUL character.",
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
/// without changing what it names: the workspace id, the command's path, an id offered for a
/// Switch, or what its answer put there (`answer_kept_as_is`). Secret or not, a value shorter than
/// `MIN_MASKED_IN_PLACE_CHARS` is never refused here (`appears_in`).
fn refuse_unmaskable(
    command: &WorkspaceCommand,
    workspace_id: &str,
    name: &str,
    value: &str,
) -> Result<(), WorkSourceError> {
    let resolved = command.resolved_path.display().to_string();
    let mut fixed = answer_kept_as_is(&command.info);
    fixed.extend([
        ("workspace id", workspace_id),
        ("path", command.path.as_str()),
        ("path", resolved.as_str()),
    ]);
    fixed.extend(command.offered_id.as_deref().map(|id| ("offered id", id)));
    match fixed.iter().find(|(_, text)| appears_in(text, value)) {
        Some((what, _)) => Err(WorkSourceError::Refused(format!(
            "{name} can't be saved: its value also appears in the command's {what}, which Ghostex can't hide."
        ))),
        None => Ok(()),
    }
}
