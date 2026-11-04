use super::*;

pub(super) fn env_or_empty(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

/// normalizedEnvironmentString: String(value ?? "").trim() || undefined.
pub(super) fn normalized_environment_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(super) fn normalized_env(key: &str) -> Option<String> {
    normalized_environment_string(&env_or_empty(key))
}

/// String(value ?? "") for JSON scalars (used where JS coerces payload fields).
pub(super) fn js_string_or_empty(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

/// String(message.status) including the JS "undefined" spelling.
pub(super) fn js_string_or_undefined(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

pub(super) fn elapsed_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis() as u64
}

pub(super) fn now_millis() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

/// new Date().toISOString().
pub(super) fn iso_timestamp() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Date.now().toString(36) / Math.random().toString(36) digits.
pub(super) fn to_base36(mut value: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut output = Vec::new();
    while value > 0 {
        output.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    output.reverse();
    String::from_utf8(output).unwrap_or_default()
}

pub(super) fn random_base36(length: usize) -> String {
    use rand::RngExt;
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut rng = rand::rng();
    (0..length)
        .map(|_| DIGITS[rng.random_range(0..36)] as char)
        .collect()
}

/// JS shellQuote from ghostex-cli.mjs (always single-quotes, unlike the
/// foundation's word-preserving rpc::shell_quote).
pub(super) fn cli_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// path.resolve(value) — lexical, against the current working directory.
pub(super) fn js_path_resolve(value: &str) -> PathBuf {
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    js_path_resolve_from(&base, value)
}

/// path.resolve(base, value).
pub(super) fn js_path_resolve_from(base: &Path, value: &str) -> PathBuf {
    let raw = Path::new(value);
    let combined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        base.join(raw)
    };
    normalize_lexically(&combined)
}

pub(super) fn normalize_lexically(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(part) => result.push(part),
        }
    }
    if result.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        result
    }
}

pub(super) fn current_dir_string() -> String {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string())
}

/// await mkdtemp(path.join(tmpdir(), prefix)).
pub(super) fn mkdtemp(prefix: &str) -> CliResult<PathBuf> {
    let base = std::env::temp_dir();
    for _ in 0..64 {
        let candidate = base.join(format!("{prefix}{}", random_base36(6)));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(CliError::Other(error.to_string())),
        }
    }
    Err(CliError::Other(format!(
        "Could not create a temporary directory for {prefix}."
    )))
}

pub(super) fn is_executable_file(path: &str) -> bool {
    #[cfg(unix)]
    {
        let Ok(cpath) = std::ffi::CString::new(path) else {
            return false;
        };
        unsafe { libc::access(cpath.as_ptr(), libc::X_OK) == 0 }
    }
    #[cfg(not(unix))]
    {
        std::fs::metadata(path).is_ok()
    }
}

/// fileExistsSync (realpathSync-based existence probe).
pub(super) fn file_exists(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
}
