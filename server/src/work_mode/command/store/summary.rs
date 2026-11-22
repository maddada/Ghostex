//! The summary of a workspace's ticket command. A secret
//! credential's value never leaves gxserver: only whether it is set. Every text in it goes through
//! the command's mask, whatever field it is, so nothing the record keeps can show a secret.

use base64::Engine;
use serde_json::{json, Value};

use crate::paths::GxserverPaths;

use super::*;

/// `{ workspaceId, team, command }`, `command` being `null` when none is set; `is_tracker`: the
/// workspace picked it as its tracker.
pub(crate) fn work_source_summary(
    paths: &GxserverPaths,
    workspace_id: &str,
    command: Option<&WorkspaceCommand>,
    team: bool,
    is_tracker: bool,
) -> Value {
    let command = command.map(|command| {
        let env = command_env_of(paths, workspace_id, Some(command));
        let stored = WorkCredentials::new(paths);
        let values = stored.source_fields(workspace_id, &command.source_id);
        let shown = |text: &str| env.shown(text);
        let credentials: Vec<Value> = command
            .info
            .credentials
            .iter()
            .map(|field| {
                let value = values
                    .and_then(|values| values.get(&field.name))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let mut entry = json!({
                    "name": shown(&field.name),
                    "label": shown(&field.label),
                    "secret": field.secret,
                    "required": field.required,
                    "placeholder": field.placeholder.as_deref().map(shown),
                    "set": value.is_some(),
                });
                if !field.secret {
                    entry["value"] = json!(value.map(shown));
                }
                entry
            })
            .collect();
        json!({
            "path": shown(&command.path),
            "resolvedPath": shown(&command.resolved_path.display().to_string()),
            "sourceId": shown(&command.source_id),
            "name": shown(&command.info.name),
            "icon": command
                .info
                .icon
                .as_ref()
                .and_then(|icon| icon.as_ref().ok())
                .filter(|icon| !env.reveals_secret(icon))
                .map(|icon| {
                format!(
                    "data:image/svg+xml;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(icon)
                )
            }),
            "iconError": command
                .info
                .icon
                .as_ref()
                .and_then(|icon| icon.as_ref().err())
                .map(|reason| shown(reason)),
            "ids": {
                "format": format_word(command.info.id_format),
                "keys": command.info.id_keys.iter().map(|key| shown(key)).collect::<Vec<_>>(),
            },
            "canCreate": command.info.can_create,
            "credentials": credentials,
            "checkedAt": shown(&command.checked_at),
            "lastError": command.last_error.as_deref().map(shown),
            "lastErrorDetail": command.last_error_detail.as_deref().map(shown),
            "offeredId": command.offered_id.as_deref().map(shown),
            "isTracker": is_tracker,
        })
    });
    json!({ "workspaceId": workspace_id, "team": team, "command": command })
}
