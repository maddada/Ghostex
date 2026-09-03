use super::*;

#[derive(Debug)]
pub(crate) struct ProjectPathHttpError {
    code: &'static str,
    message: String,
}

impl ProjectPathHttpError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "badRequest",
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "notFound",
            message: message.into(),
        }
    }
}

/*
CDXC:ServerApi 2026-06-22-04:19:
The Rust HTTP dispatcher must implement the same generic filesystem API surface
as TypeScript gxserver before domain-specific worktree deletion parity lands.
Remote project picking may browse directory names through `/api/browseProjectDirectories`,
and local open-path routing may probe a Git root through `/api/resolveGitRootForPath`;
the broader filesystem/admin endpoints remain cataloged but unimplemented.
*/
pub(crate) fn handle_browse_project_directories_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_project_directory_browse_params(body) {
        Ok(params) => params,
        Err(error) => return project_path_error_response(endpoint_path, request_id, error),
    };
    match browse_project_directories(&params, &state.paths.home_dir) {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => project_path_error_response(endpoint_path, request_id, error),
    }
}

/*
CDXC:AddProject 2026-08-18:
The Add Project dialog creates its destination folder through this endpoint
instead of relying on the add/clone step to `mkdir -p` a typed path, so a user
can make a folder, browse into it, and only then decide whether to add it or
clone into it. The parent must already be an existing directory and the name is
a single segment, which keeps this strictly weaker than the browse endpoint the
caller already used to reach that parent.
*/
pub(crate) fn handle_create_project_directory_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_project_directory_browse_params(body) {
        Ok(params) => params,
        Err(error) => return project_path_error_response(endpoint_path, request_id, error),
    };
    match create_project_directory(&params, &state.paths.home_dir) {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => project_path_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn create_project_directory(
    params: &Map<String, Value>,
    home_dir: &Path,
) -> std::result::Result<Value, ProjectPathHttpError> {
    let parent_path =
        normalize_existing_directory_path(params.get("parentPath"), "parentPath", home_dir)?;
    let name = normalize_new_directory_name(params.get("name"))?;
    let path = PathBuf::from(&parent_path).join(&name);
    if path.exists() {
        return Err(ProjectPathHttpError::bad_request(format!(
            "A folder named {name} already exists here."
        )));
    }
    fs::create_dir(&path).map_err(|error| match error.kind() {
        std::io::ErrorKind::PermissionDenied => ProjectPathHttpError {
            code: "forbidden",
            message: format!("No permission to create a folder in {parent_path}"),
        },
        _ => ProjectPathHttpError::bad_request(format!(
            "Failed to create folder: {}",
            path_to_string(&path)
        )),
    })?;
    Ok(json!({
        "name": name,
        "parentPath": parent_path,
        "path": path_to_string(&path),
    }))
}

/*
A new folder name is a single path segment, never a path: separators, `.`, `..`,
and null bytes are rejected so `parentPath` stays the only directory this
endpoint can write into.
*/
pub(crate) fn normalize_new_directory_name(
    input: Option<&Value>,
) -> std::result::Result<String, ProjectPathHttpError> {
    let Some(value) = input.and_then(Value::as_str).map(str::trim) else {
        return Err(ProjectPathHttpError::bad_request(
            "name must be a non-empty folder name.",
        ));
    };
    if value.is_empty() {
        return Err(ProjectPathHttpError::bad_request(
            "name must be a non-empty folder name.",
        ));
    }
    if value.chars().count() > 255 {
        return Err(ProjectPathHttpError::bad_request(
            "name exceeds 255 characters.",
        ));
    }
    if value.contains('\0') || value.contains('/') || value.contains('\\') {
        return Err(ProjectPathHttpError::bad_request(
            "name must be a single folder name, without path separators.",
        ));
    }
    if value == "." || value == ".." {
        return Err(ProjectPathHttpError::bad_request(
            "name must be a single folder name, without path separators.",
        ));
    }
    Ok(value.to_string())
}

pub(crate) fn handle_resolve_git_root_for_path_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::INTERNAL_SERVER_ERROR,
                rpc_error("internalError", error.message, Some(request_id)),
            );
        }
    };
    match resolve_git_root_for_existing_directory(params.get("path"), &state.paths.home_dir) {
        Ok(git_root) => {
            let mut result = Map::new();
            if let Some(git_root) = git_root {
                result.insert("gitRoot".to_string(), Value::String(git_root));
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, Value::Object(result)),
            )
        }
        Err(error) => project_path_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn project_path_error_response(
    endpoint_path: String,
    request_id: String,
    error: ProjectPathHttpError,
) -> RoutedResponse {
    let status = match error.code {
        "forbidden" => StatusCode::FORBIDDEN,
        "notFound" => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}

pub(crate) fn read_project_directory_browse_params(
    body: &Value,
) -> std::result::Result<Map<String, Value>, ProjectPathHttpError> {
    let Some(object) = body.as_object() else {
        return Err(ProjectPathHttpError::bad_request(
            "RPC request body must be an object.",
        ));
    };
    let Some(params) = object.get("params").and_then(Value::as_object) else {
        return Err(ProjectPathHttpError::bad_request(
            "RPC params must be an object.",
        ));
    };
    Ok(params.clone())
}

pub(crate) fn browse_project_directories(
    params: &Map<String, Value>,
    home_dir: &Path,
) -> std::result::Result<Value, ProjectPathHttpError> {
    let partial_path = normalize_browse_path_input(params.get("partialPath"), "partialPath")?;
    let limit = normalize_browse_limit(params.get("limit"))?;
    let cwd = params.get("cwd").and_then(Value::as_str);
    let resolved_input_path = resolve_browse_target(cwd, &partial_path, home_dir)?;
    let ends_with_separator =
        partial_path == "~" || partial_path.ends_with('/') || partial_path.ends_with('\\');
    let parent_path = if ends_with_separator {
        resolved_input_path.clone()
    } else {
        resolved_input_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| resolved_input_path.clone())
    };
    let prefix = if ends_with_separator {
        String::new()
    } else {
        resolved_input_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    /*
    CDXC:AddProject 2026-07-30:
    A path browser walks directories the user may not be allowed to read, and a
    hard error there would replace the suggestion list with a failure every time
    the caret crosses one. Permission failures therefore answer with an empty
    entry list for the resolved parent (the shared filesystem browse
    contract); every other read failure still surfaces as `notFound`.
    */
    let dirents = match fs::read_dir(&parent_path) {
        Ok(dirents) => dirents,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(json!({
                "entries": Vec::<Value>::new(),
                "parentPath": path_to_string(&parent_path),
            }));
        }
        Err(_) => {
            return Err(ProjectPathHttpError::not_found(format!(
                "Unable to browse directory: {}",
                path_to_string(&parent_path)
            )));
        }
    };
    let show_hidden = ends_with_separator || prefix.starts_with('.');
    let lower_prefix = prefix.to_lowercase();
    let mut entries = Vec::new();
    for dirent in dirents.flatten() {
        let Ok(file_type) = dirent.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = dirent.file_name().to_string_lossy().to_string();
        if !name.to_lowercase().starts_with(&lower_prefix) {
            continue;
        }
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        entries.push(json!({
            "fullPath": path_to_string(&parent_path.join(&name)),
            "name": name,
            "sortKey": name.to_lowercase(),
        }));
    }
    /*
    CDXC:AddProject 2026-07-30:
    The browse list is read top-to-bottom by a human, so it sorts the way
    `localeCompare` does rather than by byte value: case-insensitive first, raw
    name only as the tiebreaker. A byte sort would file every capitalized folder
    ahead of every lowercase one.
    */
    entries.sort_by(|left, right| {
        let key = |value: &Value| {
            (
                value
                    .get("sortKey")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                value
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        };
        key(left).cmp(&key(right))
    });
    entries.truncate(limit);
    for entry in &mut entries {
        if let Some(object) = entry.as_object_mut() {
            object.remove("sortKey");
        }
    }
    Ok(json!({
        "entries": entries,
        "parentPath": path_to_string(&parent_path),
    }))
}

pub(crate) fn resolve_git_root_for_existing_directory(
    input: Option<&Value>,
    home_dir: &Path,
) -> std::result::Result<Option<String>, ProjectPathHttpError> {
    let cwd = normalize_existing_directory_path(input, "path", home_dir)?;
    let output = match StdCommand::new("git")
        .args(["-C", cwd.as_str(), "rev-parse", "--show-toplevel"])
        .output()
    {
        Ok(output) => output,
        Err(_) => return Ok(None),
    };
    if !output.status.success() {
        return Ok(None);
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        return Ok(None);
    }
    Ok(Some(match fs::canonicalize(&root) {
        Ok(path) => path_to_string(&path),
        Err(_) => {
            let trimmed = root.trim_end_matches('/').to_string();
            if trimmed.is_empty() {
                root
            } else {
                trimmed
            }
        }
    }))
}

pub(crate) fn resolve_browse_target(
    cwd: Option<&str>,
    partial_path: &str,
    home_dir: &Path,
) -> std::result::Result<PathBuf, ProjectPathHttpError> {
    if cfg!(not(windows)) && is_windows_absolute_path(partial_path) {
        return Err(ProjectPathHttpError::bad_request(
            "Windows-style paths are only supported on Windows.",
        ));
    }
    if !is_explicit_relative_path(partial_path) {
        return Ok(resolve_path_syntax(PathBuf::from(
            expand_home_path_for_browse(partial_path, home_dir),
        )));
    }
    let Some(cwd) = cwd else {
        return Err(ProjectPathHttpError::bad_request(
            "Relative filesystem browse paths require cwd.",
        ));
    };
    let cwd = normalize_absolute_path(Some(&Value::String(cwd.to_string())), "cwd", home_dir)?;
    Ok(resolve_path_syntax(PathBuf::from(cwd).join(partial_path)))
}

pub(crate) fn normalize_browse_path_input(
    input: Option<&Value>,
    field: &str,
) -> std::result::Result<String, ProjectPathHttpError> {
    let Some(value) = input.and_then(Value::as_str) else {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    };
    if value.is_empty() {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    }
    if value.contains('\0') {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must not contain null bytes."
        )));
    }
    if value.chars().count() > 1024 {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} exceeds 1024 characters."
        )));
    }
    Ok(value.to_string())
}

pub(crate) fn normalize_browse_limit(
    input: Option<&Value>,
) -> std::result::Result<usize, ProjectPathHttpError> {
    let Some(value) = input else {
        return Ok(200);
    };
    let Some(limit) = json_number_to_positive_integer(value) else {
        return Err(ProjectPathHttpError::bad_request(
            "limit must be a positive integer.",
        ));
    };
    Ok(limit.min(500))
}

pub(crate) fn json_number_to_positive_integer(value: &Value) -> Option<usize> {
    match value {
        Value::Number(number) => {
            if let Some(value) = number.as_u64() {
                return usize::try_from(value).ok().filter(|value| *value >= 1);
            }
            if let Some(value) = number.as_i64() {
                return usize::try_from(value).ok().filter(|value| *value >= 1);
            }
            let value = number.as_f64()?;
            if value.is_finite() && value >= 1.0 && value.fract() == 0.0 {
                Some(value.min(usize::MAX as f64) as usize)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn normalize_existing_directory_path(
    input: Option<&Value>,
    field: &str,
    home_dir: &Path,
) -> std::result::Result<String, ProjectPathHttpError> {
    let normalized = normalize_absolute_path(input, field, home_dir)?;
    let metadata = fs::metadata(&normalized).map_err(|_| {
        ProjectPathHttpError::not_found(format!("{field} does not exist: {normalized}"))
    })?;
    if !metadata.is_dir() {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} is not a directory: {normalized}"
        )));
    }
    Ok(normalized)
}

pub(crate) fn normalize_absolute_path(
    input: Option<&Value>,
    field: &str,
    home_dir: &Path,
) -> std::result::Result<String, ProjectPathHttpError> {
    let Some(value) = input.and_then(Value::as_str).map(str::trim) else {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    };
    if value.is_empty() {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    }
    let expanded = expand_user_path(value, home_dir);
    if !Path::new(&expanded).is_absolute() {
        return Err(ProjectPathHttpError::bad_request(format!(
            "{field} must be an absolute path or start with ~/"
        )));
    }
    Ok(path_to_string(&resolve_path_syntax(PathBuf::from(
        expanded,
    ))))
}

pub(crate) fn resolve_path_syntax(path: PathBuf) -> PathBuf {
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

pub(crate) fn expand_home_path_for_browse(input: &str, home_dir: &Path) -> String {
    if input == "~" {
        return path_to_string(home_dir);
    }
    if input.starts_with("~/") || input.starts_with("~\\") {
        return path_to_string(&home_dir.join(&input[2..]));
    }
    input.to_string()
}

pub(crate) fn expand_user_path(input: &str, home_dir: &Path) -> String {
    if input == "~" {
        return path_to_string(home_dir);
    }
    if let Some(rest) = input.strip_prefix("~/") {
        return path_to_string(&home_dir.join(rest));
    }
    input.to_string()
}

pub(crate) fn is_explicit_relative_path(value: &str) -> bool {
    value == "."
        || value == ".."
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with(".\\")
        || value.starts_with("..\\")
}

pub(crate) fn is_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with("\\\\")
        || (bytes.len() >= 2
            && bytes[1] == b':'
            && bytes[0].is_ascii_alphabetic()
            && (bytes.len() == 2 || bytes[2] == b'/' || bytes[2] == b'\\'))
}

pub(crate) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}
