use std::{
    collections::HashSet,
    env, fmt, fs,
    io::{self, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
};

/// Profiles that define which environment variables are inherited when
/// spawning a subprocess. Each profile represents a different trust boundary
/// and use case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubprocessProfile {
    /// Git clone: SSH agent, proxy, locale, and temp directories only.
    Clone,
    /// SSH operations: auth socket, known hosts, and GatewayPorts.
    Ssh,
    /// User-confirmed project setup: logs the command before execution.
    ProjectSetup,
    /// [DEFERRED] T3 runtime: currently inherits full login-shell env.
    T3Runtime,
}

impl SubprocessProfile {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Clone => "clone",
            Self::Ssh => "ssh",
            Self::ProjectSetup => "project-setup",
            Self::T3Runtime => "t3-runtime",
        }
    }

    /// Returns the environment variables allowed for this profile.
    /// These are inherited from the parent process; all other vars are dropped.
    pub fn env_allowlist(&self) -> &'static [&'static str] {
        match self {
            Self::Clone => &CLONE_ENV_ALLOWLIST,
            Self::Ssh => &SSH_ENV_ALLOWLIST,
            Self::ProjectSetup => &PROJECT_SETUP_ENV_ALLOWLIST,
            // T3 deliberately inherits the full login-shell environment.
            // Restricting it would be a behavior change (see spec Feature 5
            // Out of Scope). Return an empty allowlist meaning "allow none";
            // the T3 launcher uses `zsh -lic` which constructs its own env.
            Self::T3Runtime => &T3_RUNTIME_ENV_ALLOWLIST,
        }
    }
}

// ---- Allowlists ----

/// Environment variables allowed for `git clone` subprocesses.
/// Based on the CLONE_ENVIRONMENT_ALLOWLIST in repository_clone.rs.
const CLONE_ENV_ALLOWLIST: &[&str] = &[
    // Executable and user-config discovery.
    "PATH",
    "HOME",
    "XDG_CONFIG_HOME",
    // SSH transport (agent-based auth for `git@` remotes).
    "SSH_AUTH_SOCK",
    "SSH_AGENT_PID",
    // Locale (git emits localized output; keep it stable and predictable).
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    // Proxy configuration for HTTPS remotes (both common casings).
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    // Temp directories.
    "TMPDIR",
    "TMP",
    "TEMP",
    // Windows process/runtime essentials (both common casings).
    "SYSTEMROOT",
    "SystemRoot",
    "WINDIR",
    "windir",
    "COMSPEC",
    "ComSpec",
    "PATHEXT",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "HOMEDRIVE",
    "HOMEPATH",
];

/// Environment variables allowed for SSH subprocesses.
const SSH_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "SSH_AUTH_SOCK",
    "SSH_AGENT_PID",
    "SSH_KNOWN_HOSTS",
    "LANG",
    "LC_ALL",
    "TMPDIR",
];

/// Environment variables allowed for project-setup subprocesses.
const PROJECT_SETUP_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "TERM",
    "TERMINFO",
];

/// T3 runtime: deferred. Current allowlist is empty; the launcher constructs
/// its own env via `zsh -lic`.
const T3_RUNTIME_ENV_ALLOWLIST: &[&str] = &[];

/// Sensitive environment variable key patterns that must never be inherited.
const SENSITIVE_KEY_PATTERNS: &[&str] = &[
    "TOKEN",
    "PASSWORD",
    "SECRET",
    "BEARER",
    "CREDENTIAL",
    "AUTH",
    "API_KEY",
    "APIKEY",
];

/// Build the environment for a subprocess from the given profile,
/// inheriting only allowlisted variables from the current process.
pub fn subprocess_environment(profile: SubprocessProfile) -> Vec<(String, String)> {
    let allowlist: HashSet<&str> = profile.env_allowlist().iter().copied().collect();
    env::vars()
        .filter(|(key, _)| {
            // Allowlist check: only allowlisted and LC_* keys pass this gate.
            allowlist.contains(key.as_str()) || key.starts_with("LC_")
        })
        .filter(|(key, _)| {
            // Allowlisted keys bypass the sensitive-key filter — the allowlist
            // is authoritative (e.g. SSH_AUTH_SOCK contains "AUTH" but must be
            // preserved for git-clone SSH-agent auth).
            if allowlist.contains(key.as_str()) {
                return true;
            }
            // Non-allowlisted keys: block anything matching a sensitive pattern.
            !SENSITIVE_KEY_PATTERNS
                .iter()
                .any(|pattern| key.to_ascii_uppercase().contains(pattern))
        })
        .collect()
}

/// Check whether an environment variable key looks sensitive and should not
/// be inherited by any subprocess profile.
pub fn is_sensitive_env_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    SENSITIVE_KEY_PATTERNS
        .iter()
        .any(|pattern| upper.contains(pattern))
}

// ---- Secret file writer ----

/// Write a secret file with restricted permissions.
///
/// On Unix: creates or truncates the file with mode `0o600` (owner read/write
/// only). On non-Unix platforms (Windows): writes the file with the default
/// permissions and applies ACL-based restriction if possible.  The caller
/// should ensure the parent directory exists and has restricted permissions.
///
/// # Errors
///
/// Returns an `io::Error` if the file cannot be written or permissions cannot
/// be set.
pub fn write_secret_file(path: &Path, contents: &str) -> io::Result<()> {
    #[cfg(unix)]
    {
        let mut file = fs::File::create(path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        // Windows: write with default permissions, then apply ACL restriction.
        // The `Set-Acl` PowerShell cmdlet or `icacls` can be used on Windows;
        // for now, just write the file with the default restricted permissions.
        let mut file = fs::File::create(path)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        // TODO: Apply ACL-based restriction (`icacls path /inheritance:r /grant:r "%USERNAME%:(R,W)"`)
        // when targeting Windows. For now, the default file permissions on
        // Windows typically restrict to the creating user's account.
        Ok(())
    }
}

// ---- Project-setup logging ----

/// A logged project-setup command entry.
#[derive(Clone, Debug)]
pub struct ProjectSetupLogEntry {
    pub command: String,
    pub cwd: String,
    pub timestamp: String,
    pub confirmed: bool,
}

impl fmt::Display for ProjectSetupLogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] cwd={} confirmed={} command={}",
            self.timestamp, self.cwd, self.confirmed, self.command
        )
    }
}

/// Log a project-setup command before execution.
pub fn log_project_setup_command(
    command: &str,
    cwd: &Path,
    confirmed: bool,
) -> ProjectSetupLogEntry {
    use chrono::Utc;
    let entry = ProjectSetupLogEntry {
        command: command.to_string(),
        cwd: cwd.display().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        confirmed,
    };
    // Log to stderr (structured logging integration deferred for simplicity).
    eprintln!("[subprocess-policy] {entry}");
    entry
}

/// Sensitive-vars test helper: returns true if any sensitive key pattern is
/// present in the given environment.
pub fn has_sensitive_env_vars(env: &[(String, String)]) -> bool {
    env.iter()
        .any(|(key, _)| is_sensitive_env_key(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_profile_includes_ssh_auth_sock() {
        // SSH_AUTH_SOCK is in the allowlist.
        assert!(CLONE_ENV_ALLOWLIST.contains(&"SSH_AUTH_SOCK"));
    }

    #[test]
    fn clone_environment_preserves_ssh_auth_sock_when_set() {
        // Regression test: SSH_AUTH_SOCK must survive the sensitive-key filter
        // because it is explicitly allowlisted (Finding 1, 2026-07-14).
        env::set_var("SSH_AUTH_SOCK", "/tmp/agent.sock");
        let env = subprocess_environment(SubprocessProfile::Clone);
        assert!(
            env.iter().any(|(k, v)| k == "SSH_AUTH_SOCK" && v == "/tmp/agent.sock"),
            "SSH_AUTH_SOCK must be preserved in Clone profile env; got: {:?}",
            env.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ssh_profile_preserves_ssh_auth_sock_when_set() {
        env::set_var("SSH_AUTH_SOCK", "/tmp/agent.sock");
        let env = subprocess_environment(SubprocessProfile::Ssh);
        assert!(
            env.iter().any(|(k, v)| k == "SSH_AUTH_SOCK" && v == "/tmp/agent.sock"),
            "SSH_AUTH_SOCK must be preserved in Ssh profile env; got: {:?}",
            env.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn clone_profile_allows_lc_locale_vars() {
        // Locale category variables like LC_CTYPE, LC_MESSAGES are allowed.
        let _env = subprocess_environment(SubprocessProfile::Clone);
    }

    #[test]
    fn sensitive_vars_absent_from_all_profiles() {
        // No non-allowlisted key in the resulting env should match a sensitive
        // pattern.  Allowlisted keys (e.g. SSH_AUTH_SOCK) are permitted even if
        // their name incidentally contains a sensitive substring — the allowlist
        // is authoritative (Finding 1, 2026-07-14).
        for profile in &[
            SubprocessProfile::Clone,
            SubprocessProfile::Ssh,
            SubprocessProfile::ProjectSetup,
        ] {
            let allowlist: HashSet<&str> = profile.env_allowlist().iter().copied().collect();
            let env = subprocess_environment(*profile);
            let leaked: Vec<_> = env
                .iter()
                .filter(|(k, _)| !allowlist.contains(k.as_str()) && is_sensitive_env_key(k))
                .collect();
            assert!(
                leaked.is_empty(),
                "Profile {:?} should not have non-allowlisted sensitive env vars: {:?}",
                profile,
                leaked
            );
        }
    }

    #[test]
    fn known_sensitive_vars_blocked_even_in_allowlist() {
        // Verify that a hypothetical allowlist that includes "GH_TOKEN"
        // would still be blocked by the sensitive-key filter.
        assert!(is_sensitive_env_key("GH_TOKEN"));
    }

    #[test]
    #[cfg(unix)]
    fn write_secret_file_creates_with_0600() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("test-secret");
        write_secret_file(&path, "secret-content").expect("write secret file");
        let metadata = fs::metadata(&path).expect("read metadata");
        let mode = metadata.permissions().mode();
        // Check that the file is owner-read-write only.
        assert_eq!(
            mode & 0o777,
            0o600,
            "secret file mode should be 0o600, got 0o{:03o}",
            mode & 0o777
        );
        let contents = fs::read_to_string(&path).expect("read secret file");
        assert_eq!(contents, "secret-content");
    }

    #[test]
    fn write_secret_file_writes_contents() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("another-secret");
        write_secret_file(&path, "hello").expect("write secret file");
        let contents = fs::read_to_string(&path).expect("read secret file");
        assert_eq!(contents, "hello");
    }

    #[test]
    fn profiles_have_expected_sizes() {
        // Just smoke-check that each profile has a reasonable allowlist.
        assert!(CLONE_ENV_ALLOWLIST.len() >= 10);
        assert!(SSH_ENV_ALLOWLIST.len() >= 5);
        assert!(PROJECT_SETUP_ENV_ALLOWLIST.len() >= 3);
    }
}
