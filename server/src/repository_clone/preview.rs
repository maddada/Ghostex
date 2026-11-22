use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::*;

/*
CDXC:AddProject 2026-07-30:
The Add Project dialog's clone step asks for ONE destination path the user typed
or browsed to. A missing path (`~/projects/my-app`) is the folder Git creates,
and an existing empty directory is cloned into directly. An existing non-empty
directory (`~/projects/`) is the selected parent, so the repository name is
appended and Git creates `~/projects/my-app`. The resolved path is then split
into the parent Git runs in and the leaf folder it creates; missing parent
chains are created by the job immediately before Git runs.

The older `parentPath` + `destinationFolderName` shape is unchanged, including
its stricter rule that ANY existing destination blocks the clone; the Clone
Repository modal depends on that warning. `destinationBlocked` is what the start
endpoint reads, so the two shapes can disagree about what "exists" means without
either one bending to the other.
*/
pub(super) fn preview_repository_clone(
    params: &Map<String, Value>,
) -> Result<Value, RepositoryCloneError> {
    let repository_input = read_required_string(
        params
            .get("repositoryInput")
            .filter(|value| !value.is_null())
            .or_else(|| params.get("remoteUrl")),
        "repositoryInput",
    )?;
    let parsed = parse_repository_clone_input(&repository_input)
        .ok_or_else(|| RepositoryCloneError::bad_request("Enter a Git repository to clone."))?;
    let default_folder_name = normalize_repository_destination_folder_name(
        Some(&Value::String(parsed.repository_name.clone())),
        "repository",
    )?;
    let destination_path_input = params
        .get("destinationPath")
        .filter(|value| !value.is_null());
    let (parent_path, destination_folder_name, destination_path, allow_empty_destination) =
        match destination_path_input {
            Some(input) => {
                let requested_path = normalize_absolute_path(Some(input), "destinationPath")?;
                let requested_destination = read_destination_status(&requested_path)?;
                let destination_path = if requested_destination.exists
                    && requested_destination.kind.as_deref() == Some("directory")
                    && requested_destination.is_empty == Some(false)
                {
                    normalize_path_string(Path::new(&requested_path).join(&default_folder_name))
                } else {
                    requested_path
                };
                let path = PathBuf::from(&destination_path);
                let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
                    return Err(RepositoryCloneError::bad_request(
                        "destinationPath must name a folder to create.",
                    ));
                };
                (
                    normalize_path_string(parent),
                    name.to_string_lossy().to_string(),
                    destination_path,
                    true,
                )
            }
            None => {
                let parent_value = params
                    .get("parentPath")
                    .or_else(|| params.get("folderPath"));
                let parent_path = normalize_existing_directory_path(parent_value, "parentPath")?;
                let requested_folder = params
                    .get("destinationFolderName")
                    .or_else(|| params.get("newFolderName"));
                let destination_folder_name = normalize_repository_destination_folder_name(
                    requested_folder,
                    &default_folder_name,
                )?;
                let destination_path =
                    normalize_path_string(Path::new(&parent_path).join(&destination_folder_name));
                (
                    parent_path,
                    destination_folder_name,
                    destination_path,
                    false,
                )
            }
        };
    if !is_path_inside(&parent_path, &destination_path) || destination_path == parent_path {
        return Err(RepositoryCloneError::bad_request(
            "destinationFolderName must create a child folder inside parentPath.",
        ));
    }
    let destination = read_destination_status(&destination_path)?;
    let destination_blocked = destination.exists
        && !(allow_empty_destination
            && destination.kind.as_deref() == Some("directory")
            && destination.is_empty == Some(true));
    let branch_name = normalize_repository_branch_name(params.get("branchName"))?;
    let mut preview = Map::new();
    if let Some(branch_name) = branch_name {
        preview.insert("branchName".to_string(), json!(branch_name));
    }
    preview.insert(
        "cloneMainOnly".to_string(),
        json!(params.get("cloneMainOnly").and_then(Value::as_bool) == Some(true)),
    );
    preview.insert("cloneUrl".to_string(), json!(parsed.clone_url));
    preview.insert("defaultFolderName".to_string(), json!(default_folder_name));
    preview.insert("destinationBlocked".to_string(), json!(destination_blocked));
    preview.insert("destinationExists".to_string(), json!(destination.exists));
    if let Some(kind) = destination.kind.clone() {
        preview.insert("destinationExistsKind".to_string(), json!(kind));
    }
    preview.insert(
        "destinationFolderName".to_string(),
        json!(destination_folder_name),
    );
    if let Some(is_empty) = destination.is_empty {
        preview.insert("destinationIsEmpty".to_string(), json!(is_empty));
    }
    preview.insert(
        "destinationPath".to_string(),
        json!(destination_path.clone()),
    );
    preview.insert("parentPath".to_string(), json!(parent_path));
    preview.insert("repositoryName".to_string(), json!(parsed.repository_name));
    preview.insert(
        "shallowClone".to_string(),
        json!(params.get("shallowClone").and_then(Value::as_bool) == Some(true)),
    );
    if destination_blocked {
        let warning = if allow_empty_destination {
            if destination.kind.as_deref() == Some("directory") {
                "Destination path already exists and is not empty.".to_string()
            } else {
                "Destination path already exists and is not a directory.".to_string()
            }
        } else {
            format!(
                "A {} already exists at {}. Choose a new folder name before cloning.",
                destination
                    .kind
                    .clone()
                    .unwrap_or_else(|| "filesystem item".to_string()),
                destination_path
            )
        };
        preview.insert("warning".to_string(), json!(warning));
    }
    Ok(Value::Object(preview))
}

pub(super) fn build_repository_clone_git_args(preview: &Value) -> Vec<String> {
    // `--progress` keeps git's meter on although stderr is a pipe (clone_process.rs publishes it).
    let mut args = vec!["clone".to_string(), "--progress".to_string()];
    if let Some(branch) = preview.get("branchName").and_then(Value::as_str) {
        args.extend(["--branch".to_string(), branch.to_string()]);
    }
    if preview
        .get("cloneMainOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        args.push("--single-branch".to_string());
    }
    if preview
        .get("shallowClone")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        args.extend(["--depth".to_string(), "1".to_string()]);
    }
    args.push("--".to_string());
    args.push(
        preview
            .get("cloneUrl")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    );
    args.push(
        preview
            .get("destinationFolderName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    );
    args
}
