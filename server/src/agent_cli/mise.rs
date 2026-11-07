use super::{catalog::Definition, process};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

pub(crate) struct Installation {
    pub tool: Option<String>,
    pub executable: String,
    /// The mise runtime (`node@26.12.1`) whose global packages hold the CLI, when `tool` is a
    /// runtime rather than the CLI's own mise tool (`npm install -g` inside mise's Node.js).
    pub runtime: Option<String>,
}

/// CDXC:AgentProviders 2026-09-14 DECISION:
/// User: support mise for CLI installs and updates; most agents on their computer are managed by mise.
/// WHY: Ask mise for the tool owning a binary, including custom backends, before classifying npm packages or native binaries by their paths.
/// A Node runtime managed by mise does not make its global npm packages separate mise tools.
/// ZCode publishes hyphenated release versions; its catalog entry enables mise prereleases for that package so latest resolves to a published version.
pub(crate) fn installation(
    definition: &Definition,
    executable: Option<&str>,
    home: &Path,
) -> Option<Installation> {
    let mise = process::resolve("mise", home)?;
    let path = query(&mise, &["which", &definition.binary], home)?;
    let tool = query(&mise, &["which", &definition.binary, "--plugin"], home)?;
    if tool.starts_with('-')
        || !tool
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@:/._-".contains(&byte))
    {
        return None;
    }
    if let Some(executable) = executable {
        let actual = canonical(executable);
        let expected = canonical(&path);
        // Activated shells can keep a previous mise version's directory on PATH until their next prompt.
        let normalized = path.replace('\\', "/");
        let same_install = normalized
            .split_once("/installs/")
            .is_some_and(|(base, rest)| {
                let tool_dir = rest.split('/').next().unwrap_or_default();
                actual.starts_with(Path::new(base).join("installs").join(tool_dir))
            });
        if actual != expected && !is_shim(executable, &mise) && !same_install {
            return None;
        }
    }
    // CDXC:AgentProviders 2026-10-28 WHY:
    // `mise which` names the tool's own file, and an `npm:` tool's file is a `#!/usr/bin/env node`
    // script: on a computer whose Node.js comes from mise (or that has none outside mise) running it
    // directly exits 127, so the version check called a fresh install failed (clean-HOME test,
    // 2026-10-28). The shim runs it inside mise's environment, the way a session does
    // (agent_cli/launch_path.rs), so the version check uses the shim when there is one.
    let shim = crate::platform::live_path::find_in(
        &definition.binary,
        &[super::launch_path::mise_data_dir(home).join("shims")],
    );
    let executable = match (executable, shim) {
        (Some(found), _) if is_shim(found, &mise) => found.to_string(),
        (_, Some(shim)) => shim.to_string_lossy().into_owned(),
        _ => crate::platform::live_path::runnable(&path),
    };
    let is_runtime = matches!(
        tool.rsplit(':').next(),
        Some("node" | "bun" | "pnpm" | "npm")
    );
    // CDXC:AgentProviders 2026-11-06 WHY:
    // Gemini installed with `npm install -g` while npm came from mise lives in
    // installs/node/26.12.1/bin and reaches PATH only through mise's shim, so it was neither a mise
    // tool nor a recognisable npm path and was refused as "another tool manages it". The runtime it
    // belongs to lets the update run that same npm through mise (catalog.rs `methods`).
    let runtime = is_runtime
        .then(|| runtime_of(&path, tool.rsplit(':').next().unwrap_or_default()))
        .flatten();
    Some(Installation {
        tool: (!is_runtime).then_some(tool),
        executable,
        runtime,
    })
}

/// `node@26.12.1` for `…/mise/installs/node/26.12.1/bin/gemini`.
fn runtime_of(path: &str, runtime: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let (_, rest) = normalized.split_once(&format!("/installs/{runtime}/"))?;
    let version = rest.split('/').next()?;
    (!version.is_empty()
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-+".contains(&byte)))
    .then(|| format!("{runtime}@{version}"))
}

/// The executable mise runs for `binary` (`mise which`), or `None` when mise provides no such tool.
pub(crate) fn which(mise: &str, binary: &str, home: &Path) -> Option<String> {
    query(mise, &["which", binary], home)
}

/// The install a mise shim runs for `binary`, or `None` when `path` is not a mise shim.
pub(crate) fn shim_target(path: &str, binary: &str, home: &Path) -> Option<String> {
    let mise = process::resolve("mise", home)?;
    if !is_shim(path, &mise) {
        return None;
    }
    query(&mise, &["which", binary], home)
}

fn canonical(path: &str) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.into())
}

fn is_shim(path: &str, mise: &str) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    normalized.contains("/mise/shims/") || canonical(path) == canonical(mise)
}

pub(crate) fn looks_managed(path: &str) -> bool {
    let original = path.replace('\\', "/").to_lowercase();
    let real = canonical(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    if matches!(real.rsplit('/').next(), Some("mise" | "mise.exe")) {
        return true;
    }
    [original, real].iter().any(|path| {
        (path.contains("/mise/installs/")
            && !["node", "bun", "pnpm", "npm"]
                .iter()
                .any(|runtime| path.contains(&format!("/installs/{runtime}/"))))
            || path.contains("/mise/shims/")
    })
}

fn query(mise: &str, args: &[&str], home: &Path) -> Option<String> {
    let mut command = crate::platform::process::background_command(mise);
    command.args(args).current_dir(home).env("HOME", home);
    #[cfg(not(windows))]
    command.env(
        "PATH",
        crate::agent_hooks::probing::normalize_gxserver_process_path(
            std::env::var("PATH").ok().as_deref(),
            home,
        ),
    );
    crate::agent_hooks::probing::run_command_stdout_with_timeout(command, Duration::from_secs(3))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.contains(['\r', '\n']))
}

/// The backend `mise use` would install a tool with: an explicit backend as written, else the first backend
/// `mise registry` lists for this platform. `Some("")` means the registry has none that runs here.
pub(crate) fn backend(tool: &str, home: &Path) -> Option<String> {
    static CACHE: LazyLock<Mutex<HashMap<String, (Instant, String)>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    if tool.contains(':') {
        return Some(tool.to_string());
    }
    if let Some((_, backend)) = CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(tool).cloned())
        .filter(|(checked, _)| checked.elapsed() < Duration::from_secs(600))
    {
        return Some(backend);
    }
    let mise = process::resolve("mise", home)?;
    let mut command = crate::platform::process::background_command(&mise);
    command
        .args(["registry", tool])
        .current_dir(home)
        .env("HOME", home);
    // A failed or slow probe says nothing about the registry, so it is not cached.
    let listed = crate::agent_hooks::probing::run_command_stdout_with_timeout(
        command,
        Duration::from_secs(3),
    )?;
    let backend = listed
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(tool.to_string(), (Instant::now(), backend.clone()));
    }
    Some(backend)
}

/// CDXC:AgentProviders 2026-10-07 WHY:
/// mise's registry can list a tool whose only backends do not run on this platform (cursor-agent's only backend is `http:cursor-agent`, which mise does not support on Windows), so `mise use` failed at once and the one-click install never reached the official installer. `mise registry <tool>` prints nothing for such a tool, which makes the mise method unavailable there. Since 2026-10-28 an `npm:` tool on a computer without npm is installed with `node@lts` in the same `mise use` (agent_cli/catalog.rs `mise_needs_node`), which supersedes refusing the method there.
pub(crate) fn unavailable_reason(tool: &str, home: &Path) -> Option<String> {
    let backend = backend(tool, home)?;
    if backend.is_empty() {
        let platform = match std::env::consts::OS {
            "windows" => "Windows",
            "macos" => "macOS",
            "linux" => "Linux",
            other => other,
        };
        return Some(format!(
            "mise cannot install {tool} on {platform}. Use the official installer instead."
        ));
    }
    None
}

pub(crate) fn command(tool: &str, installed: bool) -> String {
    if installed {
        // Keep older versions available to agents and subprocesses that are already running.
        format!(
            "mise upgrade --bump --no-prune --yes {}",
            process::quote(tool)
        )
    } else {
        format!(
            "mise use --global --yes {}",
            process::quote(&format!("{tool}@latest"))
        )
    }
}

/// One `mise use` that installs `tool` at its latest release with the tools it runs on, each named
/// with its version (`npm:@deepseek-ai/dsh@0.2.1-rc.2`). Updating runs the same command; mise keeps
/// the versions it replaces.
pub(crate) fn use_command(tool: &str, companions: &[String]) -> String {
    std::iter::once(format!("{tool}@latest"))
        .chain(companions.iter().cloned())
        .map(|spec| process::quote(&spec))
        .fold("mise use --global --yes".to_string(), |command, spec| {
            format!("{command} {spec}")
        })
}

/// `command` followed by `then` inside the tools mise just set up, only when `command` succeeded.
pub(crate) fn then_run(command: &str, then: &str) -> String {
    if cfg!(windows) {
        format!("{command}; if (-not $LASTEXITCODE) {{ mise exec -- {then} }}")
    } else {
        format!("{command} && mise exec -- {then}")
    }
}

/// What mise does with a newer release than the installed one.
#[derive(Clone)]
pub(crate) struct HeldBack {
    pub message: String,
    /// The older release mise would still install now (`None`: nothing to install yet).
    pub installable: Option<String>,
}

/// CDXC:AgentProviders 2026-10-17 WHY:
/// mise skips a release younger than the user's `minimum_release_age` and still reports "All tools are up to date", so an OMP update "succeeded" while staying one release behind. That setting is the user's safety choice and Ghostex never overrides it: the state and the finished job say which release mise holds back and until when, instead of calling the CLI current or the update failed. When an older release is already eligible (Claude 2.1.301 while 2.1.302 waits), the update is still offered and says which release it installs.
pub(crate) async fn held_back(tool: &str, latest: &str, home: &Path) -> Option<HeldBack> {
    static CACHE: LazyLock<Mutex<HashMap<(String, String), (Instant, Option<HeldBack>)>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let key = (tool.to_string(), latest.to_string());
    if let Some((_, held)) = CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
        .filter(|(checked, _)| checked.elapsed() < Duration::from_secs(600))
    {
        return held;
    }
    let held = dry_run_held_back(tool, latest, home).await;
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, (Instant::now(), held.clone()));
    }
    held
}

/// Runs `mise <args> <tool>` through the job shell (so mise's npm backend finds npm on the same PATH
/// an update would use) and returns what it printed, stdout and stderr together.
async fn mise_output(args: &str, tool: &str, home: &Path) -> Option<String> {
    let mise = process::resolve("mise", home)?;
    let output = std::sync::Arc::new(Mutex::new(String::new()));
    let capture = output.clone();
    let call = if cfg!(windows) { "& " } else { "" };
    let script = format!(
        "{call}{} {args} {}",
        process::quote(&mise),
        process::quote(tool)
    );
    let env = std::collections::BTreeMap::new();
    process::run(&script, home, &env, Duration::from_secs(8), move |chunk| {
        if let Ok(mut output) = capture.lock() {
            if output.len() < 16 * 1024 {
                output.push_str(&chunk);
            }
        }
    })
    .await
    .ok()?;
    let text = output.lock().ok()?.clone();
    Some(text)
}

async fn dry_run_held_back(tool: &str, latest: &str, home: &Path) -> Option<HeldBack> {
    let text = mise_output("upgrade --dry-run --bump --yes", tool, home).await?;
    // "Would install claude@2.1.301": the eligible release this update installs.
    let installable = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("Would install "))
        .and_then(|target| target.rsplit_once('@'))
        .map(|(_, version)| version.trim().to_string())
        .filter(|version| !version.is_empty());
    if let Some(message) = held_back_message(&text, installable.as_deref()) {
        return Some(HeldBack {
            message,
            installable,
        });
    }
    if installable.is_some() {
        return None;
    }
    /*
    CDXC:Updates 2026-11-07 FIX:
    User: "open code isn't updating pls fix" (the top bar's Updates list showed OpenCode at "Waiting…").
    WHY: for an aqua tool (OpenCode is `aqua:anomalyco/opencode`) mise's dry run prints only "All tools are up to date" while minimum_release_age hides the newer release (2.2.6, a few hours old), with no "ignored by minimum_release_age" line, so OpenCode read as an update, Update ran `mise upgrade` that installed nothing, and the row came back with the same update. `mise ls-remote` still warns "1 newer opencode release hidden by minimum_release_age", so that warning holds it back too.
    */
    let listed = mise_output("ls-remote", tool, home).await?;
    hidden_by_release_age(&listed).then(|| HeldBack {
        message: format!(
            "mise holds back {latest} because of your mise minimum_release_age setting. Update again after that; Ghostex does not change this setting."
        ),
        installable: None,
    })
}

/// Whether `mise ls-remote` warned that minimum_release_age hides newer releases ("1 newer
/// opencode release hidden by minimum_release_age").
pub(crate) fn hidden_by_release_age(output: &str) -> bool {
    output
        .lines()
        .any(|line| line.contains("hidden by minimum_release_age") && line.contains("newer"))
}

/// The plain-words form of mise's "newer … release 18.11.3 (released …, eligible 2026-10-18 01:52 +04)
/// ignored by minimum_release_age (24h)" warning.
pub(crate) fn held_back_message(output: &str, installable: Option<&str>) -> Option<String> {
    let line = output
        .lines()
        .find(|line| line.contains("ignored by minimum_release_age"))?;
    let between = |start: &str, end: char| -> Option<String> {
        let rest = &line[line.find(start)? + start.len()..];
        Some(rest[..rest.find(end)?].trim().to_string())
    };
    let release = between(" release ", ' ').unwrap_or_else(|| "the newest release".to_string());
    let until = between("eligible ", ')').map_or(String::new(), |date| format!(" until {date}"));
    let age =
        between("minimum_release_age (", ')').map_or(String::new(), |age| format!(" ({age})"));
    Some(match installable {
        Some(version) => format!(
            "mise can update to {version} now; it holds back {release}{until} because of your mise minimum_release_age setting{age}. Ghostex does not change this setting."
        ),
        None => format!(
            "mise holds back {release}{until} because of your mise minimum_release_age setting{age}. Update again after that; Ghostex does not change this setting."
        ),
    })
}

#[cfg(test)]
mod release_age_tests {
    use super::hidden_by_release_age;

    #[test]
    fn ls_remote_warning_counts_as_held_back() {
        let output =
            "2.2.4\n2.2.5\nmise WARN  1 newer opencode release hidden by minimum_release_age\n";
        assert!(hidden_by_release_age(output));
        assert!(!hidden_by_release_age("2.2.4\n2.2.5\n"));
    }
}
