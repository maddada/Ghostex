//! The steps after a box has a machine, shared by create and resume: install the Linux package,
//! boot box mode, send logins and ticked secrets, and prepare the session. Every function blocks.

use std::path::Path;

use ghostex_gx_protocol::cloud_box::{
    box_files, box_private_dir, box_verbs, BoxBootResult, BoxBootTicket, BoxInjectedAccount,
    BoxPrepareSessionRequest, BoxPrepareSessionResult, BoxSecretFile, BoxSecretVar, BoxSecretsFile,
    CloudBoxDraftRef, CloudProviderId, ProjectCloudBoxSettings, BOX_BOOT_TICKET_VERSION,
    BOX_GHOSTEX_BIN, DEFAULT_IDLE_STOP_MINUTES,
};
use serde_json::Value;

use super::ops::{Ops, Target};
use crate::server::AppState;

/// The idle minutes setting (Q1011).
pub(super) fn idle_stop_minutes(state: &AppState) -> u64 {
    crate::shared_settings_file::read_shared_settings(&state.paths)
        .and_then(|settings| {
            settings
                .get(ghostex_gx_protocol::cloud_box::setting_keys::IDLE_STOP_MINUTES)
                .and_then(Value::as_u64)
        })
        .filter(|minutes| *minutes > 0)
        .unwrap_or(DEFAULT_IDLE_STOP_MINUTES)
}

/// Tailcat client keys allowed to reach a box (Q1021 private only): every paired phone's key.
///
/// CDXC:CloudBoxes 2026-11-02 SEE-ALSO: the computer's own client key joins this list with the box connection work (W3), which owns how the desktop dials a box over tailcat.
pub(super) fn allowed_device_keys(state: &AppState) -> Vec<String> {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let mut keys: Vec<String> = crate::remote_access::RemotePairingRepository::new(&db)
        .list_devices()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|device| device.tailcat_client_key)
        .filter(|key| !key.trim().is_empty())
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// The boot ticket of a box.
pub(super) fn ticket(
    state: &AppState,
    box_id: &str,
    provider: &CloudProviderId,
    home_project_id: Option<String>,
) -> BoxBootTicket {
    BoxBootTicket {
        version: BOX_BOOT_TICKET_VERSION,
        box_id: box_id.to_string(),
        provider: provider.clone(),
        home_machine_id: Some(state.metadata.server_id.clone()),
        home_project_id,
        allow_device_keys: if *provider == CloudProviderId::Local {
            Vec::new()
        } else {
            allowed_device_keys(state)
        },
        idle_stop_minutes: idle_stop_minutes(state),
        // Boat has no idle timer, so its boxes stop themselves; Instabox pauses idle boxes itself.
        self_stop: *provider == CloudProviderId::Boat,
        tailcat: *provider != CloudProviderId::Local,
    }
}

/// Puts this gxserver's version of the Linux package on the box unless it has it (real
/// providers download the release asset; `local` boxes get the package copied at create).
pub(super) fn install_package(ops: &Ops, target: &Target) -> Result<(), String> {
    if matches!(ops, Ops::Local { .. }) {
        return Ok(());
    }
    let script = ghostex_gx_cloud::install::install_script(crate::constants::GXSERVER_VERSION);
    let output = ops.exec(target, &["sh", "-c", &script], None, 600)?;
    if output.exit_code != 0 {
        return Err(format!(
            "Couldn't install Ghostex on the box: {}",
            output.failure_tail()
        ));
    }
    Ok(())
}

/// Runs a `ghostex box` verb on the box and reads its JSON answer.
pub(super) fn run_box_verb(
    ops: &Ops,
    target: &Target,
    verb: &str,
    extra: &[&str],
    timeout_seconds: u64,
) -> Result<Value, String> {
    let mut argv = vec![BOX_GHOSTEX_BIN, "box", verb];
    argv.extend_from_slice(extra);
    let output = ops.exec(target, &argv, None, timeout_seconds)?;
    match output.json_object() {
        Some(value) if value.get("ok").and_then(Value::as_bool) == Some(true) => Ok(value),
        Some(value) => Err(value
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("ghostex box {verb} failed."))),
        None => Err(format!(
            "ghostex box {verb} printed no answer: {}",
            output.failure_tail()
        )),
    }
}

pub(super) fn boot(
    ops: &Ops,
    target: &Target,
    ticket: &BoxBootTicket,
) -> Result<BoxBootResult, String> {
    let encoded = ghostex_gx_cloud::ticket::encode_ticket(ticket);
    let value = run_box_verb(ops, target, box_verbs::BOOT, &["--ticket", &encoded], 180)?;
    serde_json::from_value(value)
        .map_err(|error| format!("ghostex box boot answered something unexpected: {error}"))
}

/// The Codex accounts allowed on boxes, with their `auth.json` from xswap's saved login (D6).
fn codex_accounts(state: &AppState) -> Vec<BoxInjectedAccount> {
    let allowed = super::agent_logins::allowed_codex_account_ids(state);
    if allowed.is_empty() {
        return Vec::new();
    }
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let Ok(registry) = crate::accounts::store::read(&db) else {
        return Vec::new();
    };
    let root = crate::accounts::launch::xswap_root(&state.paths.home_dir).join("accounts");
    registry
        .accounts
        .into_iter()
        .filter(|account| allowed.contains(&account.id))
        .filter_map(|account| {
            let auth =
                std::fs::read_to_string(root.join(&account.selector).join("auth.json")).ok()?;
            Some(BoxInjectedAccount {
                account_id: account.id,
                agent: "codex".to_string(),
                name: account.name,
                kind: "codexAuthJson".to_string(),
                value: auth,
            })
        })
        .collect()
}

/// A ticked variable's value: from the project's `.env*` files (last one wins), else this
/// process's environment.
fn var_value(root: Option<&Path>, files: &[String], name: &str) -> Option<String> {
    let from_files = root.and_then(|root| {
        files.iter().rev().find_map(|file| {
            let text = std::fs::read_to_string(root.join(file)).ok()?;
            text.lines().rev().find_map(|line| {
                let line = line.trim().trim_start_matches("export ");
                let (key, value) = line.split_once('=')?;
                (key.trim() == name).then(|| {
                    value
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string()
                })
            })
        })
    });
    from_files.or_else(|| std::env::var(name).ok())
}

/// `secrets.json`: the logins allowed on boxes and the project's ticked secrets (Q1018, Q1020).
pub(super) fn secrets_file(
    state: &AppState,
    project_path: Option<&str>,
    settings: &ProjectCloudBoxSettings,
) -> BoxSecretsFile {
    let mut accounts = super::agent_logins::injected_claude_accounts(state);
    accounts.extend(codex_accounts(state));
    let root = project_path.map(Path::new);
    let all_env_files = root.map(super::secret_files::env_files).unwrap_or_default();
    let files = settings
        .secrets
        .files
        .iter()
        .filter(|file| !file.contains("..") && !file.starts_with('/'))
        .filter_map(|file| {
            let contents = std::fs::read_to_string(root?.join(file)).ok()?;
            Some(BoxSecretFile {
                path: file.clone(),
                contents,
            })
        })
        .collect();
    let vars = settings
        .secrets
        .vars
        .iter()
        .filter_map(|name| {
            Some(BoxSecretVar {
                name: name.clone(),
                value: var_value(root, &all_env_files, name)?,
            })
        })
        .collect();
    BoxSecretsFile {
        accounts,
        files,
        vars,
    }
}

/// Writes `secrets.json` into the box's private folder. `false` when there was nothing to send.
pub(super) fn send_secrets(
    ops: &Ops,
    target: &Target,
    secrets: &BoxSecretsFile,
) -> Result<bool, String> {
    if secrets.accounts.is_empty() && secrets.files.is_empty() && secrets.vars.is_empty() {
        return Ok(false);
    }
    let path = format!(
        "{}/{}",
        box_private_dir(&ops.provider()),
        box_files::SECRETS
    );
    let bytes = serde_json::to_vec(secrets).map_err(|error| error.to_string())?;
    ops.write_file(target, &path, &bytes)?;
    Ok(true)
}

/// The prepare-session request for a home draft: its agent, account and first message, the
/// project's repository and remembered base branch.
pub(super) fn prepare_request(
    state: &AppState,
    box_id: &str,
    draft: &CloudBoxDraftRef,
    repo_url: Option<String>,
    project_name: &str,
    snapshot_id: Option<String>,
    worktree: bool,
    provider: CloudProviderId,
) -> Result<BoxPrepareSessionRequest, String> {
    let db =
        crate::storage::open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
    let repository = crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str());
    let session = repository
        .get_session(&draft.project_id, &draft.session_id)
        .map_err(|error| error.message)?
        .ok_or_else(|| {
            format!(
                "The draft {}/{} no longer exists.",
                draft.project_id, draft.session_id
            )
        })?;
    let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
    let project = repository.get_project(&draft.project_id).ok().flatten();
    let base_branch = project.as_ref().and_then(|project| {
        project
            .pointer("/runtimeSettings/newChatWorktree/baseBranch")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    // A draft keeps its first message in its runtime settings.
    let first_message = text("firstUserMessage").or_else(|| {
        session
            .pointer("/runtimeSettings/firstUserMessage")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let branch = session_branch(
        box_id,
        &draft.project_id,
        project
            .as_ref()
            .and_then(|project| project.get("path"))
            .and_then(Value::as_str)
            .unwrap_or_default(),
        first_message.as_deref(),
    );
    let image_paths = session
        .pointer("/runtimeSettings/cloudBox/imagePaths")
        .and_then(Value::as_array)
        .map(|paths| {
            paths
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let slug: String = project_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    Ok(BoxPrepareSessionRequest {
        box_id: box_id.to_string(),
        repo_url,
        checkout_path: format!(
            "{}/code/{}",
            box_home(&provider),
            if slug.is_empty() { "project" } else { &slug }
        ),
        base_branch,
        branch: Some(branch),
        worktree,
        agent_id: text("agentId").unwrap_or_else(|| "claude".to_string()),
        account_id: text("accountId"),
        title: text("title"),
        first_message,
        image_paths,
        home_project_id: Some(draft.project_id.clone()),
        home_machine_id: Some(state.metadata.server_id.clone()),
        home_session_id: Some(draft.session_id.clone()),
        snapshot_id,
    })
}

/// CDXC:CloudBoxes 2026-11-02 DECISION:
/// User (Q1005 "fetch new branch", Q1014 "push branch"): every box session works on its own new
/// branch cut from the freshly fetched base, named the way a new worktree's branch is (from the
/// first message: a ticket key and its title, else the message's words), so the agent can push it
/// and the box chip's Open PR and Pull find it. The box makes the name unique against its own and
/// origin's branches.
/// SEE-ALSO: server/src/worktree_branch_name.rs, server/src/cloud_boxes/box_mode/prepare_session.rs.
fn session_branch(
    box_id: &str,
    project_id: &str,
    cwd: &str,
    first_message: Option<&str>,
) -> String {
    let hint = first_message
        .filter(|message| !message.trim().is_empty())
        .map(|message| {
            crate::worktree_branch_name::worktree_name_hint(
                message,
                &crate::worktree_branch_name::ReferenceScope { project_id, cwd },
            )
        })
        .unwrap_or_default();
    let hint: String = hint
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(|c| matches!(c, '-' | '/' | '.'))
        .to_string();
    if hint.is_empty() {
        format!("cloud-box-{}", box_id.trim_start_matches("gx_"))
    } else {
        hint
    }
}

/// Copies the first message's pictures from this computer into the box's private folder
/// (`images/`) and answers their paths on the box, in order. A picture that can't be read is
/// left out.
pub(super) fn send_images(
    ops: &Ops,
    target: &Target,
    local_paths: &[String],
) -> Result<Vec<String>, String> {
    let folder = format!("{}/images", box_private_dir(&ops.provider()));
    let mut on_box = Vec::new();
    for (index, local) in local_paths.iter().enumerate() {
        let Ok(bytes) = std::fs::read(local) else {
            continue;
        };
        // Box file paths take letters, digits and `._-` only.
        let name: String = std::path::Path::new(local)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "image.png".to_string())
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let path = format!("{folder}/{index}-{name}");
        ops.write_file(target, &path, &bytes)?;
        on_box.push(path);
    }
    Ok(on_box)
}

/// The box user's home: Instabox keeps it on its storage disk, Boat's user is `user`, a `local`
/// container runs as root.
fn box_home(provider: &CloudProviderId) -> &'static str {
    match provider {
        CloudProviderId::Instabox => "/data/home",
        CloudProviderId::Local => "/root",
        _ => "/home/user",
    }
}

/// Writes the request into the private folder and runs `prepare-session`.
pub(super) fn prepare_session(
    ops: &Ops,
    target: &Target,
    request: &BoxPrepareSessionRequest,
) -> Result<BoxPrepareSessionResult, String> {
    let path = format!(
        "{}/{}",
        box_private_dir(&ops.provider()),
        box_files::PREPARE_SESSION_REQUEST
    );
    let bytes = serde_json::to_vec(request).map_err(|error| error.to_string())?;
    ops.write_file(target, &path, &bytes)?;
    let value = run_box_verb(
        ops,
        target,
        box_verbs::PREPARE_SESSION,
        &["--request", &path],
        600,
    )?;
    serde_json::from_value(value).map_err(|error| {
        format!("ghostex box prepare-session answered something unexpected: {error}")
    })
}

/// `last_refresh` of a Codex `auth.json` (ISO-8601), to tell which copy is newer.
fn codex_last_refresh(text: &str) -> Option<String> {
    serde_json::from_str::<Value>(text)
        .ok()?
        .get("last_refresh")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// D6 "copy and send back": takes the Codex logins the box refreshed (`ghostex box write-back`),
/// saves each into its account's saved login on this computer when the box's copy is newer, and
/// acknowledges them. Runs before a box stops, since a stopped box loses its private folder.
/// Returns how many logins came home.
pub(super) fn write_back_logins(
    state: &AppState,
    ops: &Ops,
    target: &Target,
) -> Result<usize, String> {
    let value = run_box_verb(ops, target, box_verbs::WRITE_BACK, &[], 120)?;
    let result: ghostex_gx_protocol::cloud_box::BoxWriteBackResult =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    if result.items.is_empty() {
        return Ok(0);
    }
    let db =
        crate::storage::open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
    let registry = crate::accounts::store::read(&db).map_err(|error| error.message)?;
    drop(db);
    let root = crate::accounts::launch::xswap_root(&state.paths.home_dir).join("accounts");
    let mut saved = 0;
    let mut acked = Vec::new();
    for item in result
        .items
        .iter()
        .filter(|item| item.kind == "codexAuthJson")
    {
        if serde_json::from_str::<Value>(&item.value).is_err() {
            continue;
        }
        let Some(account) = registry.accounts.iter().find(|account| {
            account.id == item.account_id
                && account.provider == crate::accounts::model::Provider::Codex
        }) else {
            // The account is gone from this computer: nothing to save it into.
            acked.push(item.id.clone());
            continue;
        };
        let path = root.join(&account.selector).join("auth.json");
        let current = std::fs::read_to_string(&path).ok();
        let box_newer = match (
            codex_last_refresh(&item.value),
            current.as_deref().and_then(codex_last_refresh),
        ) {
            (Some(from_box), Some(here)) => from_box > here,
            _ => true,
        };
        if box_newer {
            let temporary = path.with_extension("json.ghostex-write-back");
            std::fs::write(&temporary, item.value.as_bytes()).map_err(|error| error.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600));
            }
            std::fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
            saved += 1;
        }
        acked.push(item.id.clone());
    }
    if !acked.is_empty() {
        let ack = acked.join(",");
        run_box_verb(ops, target, box_verbs::WRITE_BACK, &["--ack", &ack], 120)?;
    }
    Ok(saved)
}
