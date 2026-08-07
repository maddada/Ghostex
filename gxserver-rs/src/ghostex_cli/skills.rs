use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use crate::ghostex_cli::args::{multi_value_flag, parse_args, Flags};
use crate::ghostex_cli::launchers;
use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::ghostex_cli::usage;

/*
CDXC:GhostexRustCli 2026-07-13:
Faithful port of the Node CLI agent-skill surfaces. Public Ghostex skill
install commands delegate to gxserver's external `agent-skills` CLI wrapper
(resolved like `gx server ...`) instead of copying folders into one legacy
shared path, keeping the bundled skill package source local so installed app
builds install the skill version that matches their CLI commands.
*/

const GHOSTEX_BROWSER_SKILL_NAME: &str = "ghostex-browser-use";
const GHOSTEX_EMBEDDED_BROWSER_SKILL_NAME: &str = "ghostex-embedded-browser-use";
const GHOSTEX_COMPUTER_USE_SKILL_NAME: &str = "ghostex-computer-use";
const GHOSTEX_AGENT_ORCHESTRATION_SKILL_NAME: &str = "ghostex-agent-orchestration";
const GHOSTEX_FABLE_56_ORCHESTRATION_SKILL_NAME: &str = "ghostex-fable-5.6-orchestration";
const GHOSTEX_FIND_PREV_SESSION_SKILL_NAME: &str = "ghostex-find-prev-session";
const GHOSTEX_AUTO_RENAME_SESSION_SKILL_NAME: &str = "ghostex-auto-rename-session";
const GHOSTEX_MANAGE_BEADS_SKILL_NAME: &str = "ghostex-manage-beads";
const GHOSTEX_MOVE_CODEX_SESSION_SKILL_NAME: &str = "ghostex-move-codex-session";

/// JS stringFlag: trimmed non-empty string or nothing.
fn string_flag(value: Option<String>) -> Option<String> {
    let trimmed = value?.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn string_flag_env(name: &str) -> Option<String> {
    string_flag(std::env::var(name).ok())
}

/// The /^[a-z][a-z0-9+.-]*:/i URL-scheme check used for explicit sources.
fn has_url_scheme(value: &str) -> bool {
    let Some(colon) = value.find(':') else {
        return false;
    };
    if colon == 0 {
        return false;
    }
    let bytes = value.as_bytes();
    if !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    value[1..colon]
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
}

fn path_dirname(value: &str) -> PathBuf {
    let parent = std::path::Path::new(value).parent();
    match parent {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

pub fn resolve_ghostex_agent_skill_source_dir(
    skill_name: &str,
    env_vars: &[&str],
) -> CliResult<PathBuf> {
    let cli_dir = launchers::cli_dir();
    let source_root = launchers::find_ghostex_source_root(None);
    let mut candidates: Vec<Option<PathBuf>> = env_vars
        .iter()
        .map(|env_var| string_flag_env(env_var).map(PathBuf::from))
        .collect();
    candidates.push(Some(cli_dir.join("skills").join(skill_name)));
    candidates.push(
        source_root
            .as_ref()
            .map(|root| root.join("skills").join(skill_name)),
    );
    candidates.push(
        source_root
            .as_ref()
            .map(|root| root.join("scripts").join("skills").join(skill_name)),
    );
    candidates.push(Some(
        launchers::js_path_resolve(&cli_dir.join(".."))
            .join(".agents")
            .join("skills")
            .join(skill_name),
    ));
    candidates.push(
        source_root
            .as_ref()
            .map(|root| root.join(".agents").join("skills").join(skill_name)),
    );
    for candidate in launchers::unique_paths(&candidates) {
        if launchers::file_exists_sync(&candidate.join("SKILL.md")) {
            return Ok(candidate);
        }
    }
    Err(CliError::Other(format!(
        "Could not find {skill_name}. Reinstall Ghostex or set {} to the skill directory.",
        env_vars.first().copied().unwrap_or("GHOSTEX_SKILL_SOURCE")
    )))
}

pub fn resolve_ghostex_agent_skill_package_source(
    skill_name: &str,
    env_vars: &[&str],
    flags: &Flags,
) -> CliResult<String> {
    let cli_dir = launchers::cli_dir();
    let explicit_source = string_flag(
        flags
            .text("source")
            .or_else(|| flags.text("packageSource"))
            .or_else(|| std::env::var("GHOSTEX_AGENT_SKILLS_SOURCE").ok()),
    );
    if let Some(explicit_source) = &explicit_source {
        if has_url_scheme(explicit_source) {
            return Ok(explicit_source.clone());
        }
    }
    let explicit_sources: Vec<Option<String>> = env_vars
        .iter()
        .map(|env_var| string_flag_env(env_var))
        .collect();
    let source_root = launchers::find_ghostex_source_root(None);
    let mut candidates: Vec<Option<PathBuf>> = vec![
        explicit_source.as_ref().map(PathBuf::from),
        explicit_source.as_ref().map(|source| path_dirname(source)),
    ];
    candidates.extend(
        explicit_sources
            .iter()
            .map(|candidate| candidate.as_ref().map(|source| path_dirname(source))),
    );
    candidates.push(Some(cli_dir.join("skills")));
    candidates.push(source_root.as_ref().map(|root| root.join("skills")));
    candidates.push(source_root.as_ref().map(|root| root.join("scripts")));
    candidates.push(
        source_root
            .as_ref()
            .map(|root| root.join("scripts").join("skills")),
    );
    candidates.push(Some(
        launchers::js_path_resolve(&cli_dir.join(".."))
            .join(".agents")
            .join("skills"),
    ));
    candidates.push(
        source_root
            .as_ref()
            .map(|root| root.join(".agents").join("skills")),
    );
    for candidate in launchers::unique_paths(&candidates) {
        if launchers::file_exists_sync(&candidate.join(skill_name).join("SKILL.md")) {
            return Ok(candidate.to_string_lossy().to_string());
        }
        if launchers::file_exists_sync(&candidate.join("skills").join(skill_name).join("SKILL.md"))
        {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    if let Some(explicit_source) = explicit_source {
        return Ok(explicit_source);
    }
    let legacy_source_dir = resolve_ghostex_agent_skill_source_dir(skill_name, env_vars)?;
    Ok(path_dirname(&legacy_source_dir.to_string_lossy())
        .to_string_lossy()
        .to_string())
}

fn install_ghostex_agent_skill(
    args: &[String],
    command: &str,
    env_vars: &[&str],
    skill_name: &str,
) -> CliResult<()> {
    let flags = parse_args(args).flags;
    let package_source = resolve_ghostex_agent_skill_package_source(skill_name, env_vars, &flags)?;
    let agent_ids = multi_value_flag(args, &["agent", "agents"]);
    /*
    Delegate to gxserver's external `skills` CLI wrapper so skill installs stay
    owned by the daemon package while the bundled skill package source remains
    local to this CLI copy.
    */
    let mut gxserver_args: Vec<String> = vec![
        "agent-skills".to_string(),
        "install".to_string(),
        skill_name.to_string(),
        "--source".to_string(),
        package_source,
    ];
    if !agent_ids.is_empty() {
        gxserver_args.push("--agent".to_string());
        gxserver_args.extend(agent_ids);
    }
    if flags.truthy("json") {
        gxserver_args.push("--json".to_string());
        let launch = launchers::resolve_gxserver_cli_launch()?;
        let mut full_args = launch.args.clone();
        full_args.extend(gxserver_args);
        let mut child = Command::new(&launch.command);
        child.args(&full_args);
        if let Some(cwd) = &launch.cwd {
            child.current_dir(cwd);
        }
        for (key, value) in &launch.env {
            child.env(key, value);
        }
        let output = child.output().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                CliError::Other(format!("spawn {} ENOENT", launch.command))
            }
            _ => CliError::Other(format!("spawn {} {error}", launch.command)),
        })?;
        if !output.status.success() {
            return Err(CliError::Other(format!(
                "Command failed: {} {}\n{}",
                launch.command,
                full_args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        let _ = std::io::stdout().write_all(&output.stdout);
        let _ = std::io::stderr().write_all(&output.stderr);
        return Ok(());
    }
    launchers::run_gxserver_cli_command(&gxserver_args)?;
    println!("Configure agents to run: {command}");
    Ok(())
}

/// Shared shape of the skill-surface commands (help/install-skill dispatch).
fn skill_surface_command(
    args: &[String],
    usage_text: &str,
    unknown_label: &str,
    install: &dyn Fn(&[String]) -> CliResult<()>,
) -> CliResult<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("help");
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();
    if rest.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{usage_text}");
        return Ok(());
    }
    match subcommand {
        "help" | "-h" | "--help" => {
            println!("{usage_text}");
            Ok(())
        }
        "install-skill" => install(&rest),
        other => Err(CliError::Other(format!(
            "Unknown {unknown_label} command: {other}\n\n{usage_text}"
        ))),
    }
}

pub fn install_browser_skill_command(args: &[String]) -> CliResult<()> {
    /*
    Agents should invoke Ghostex embedded browser control as
    `$ghostex-embedded-browser-use`, not the implementation-shaped
    `$ghostex-browser-devtools-mcp`.
    */
    install_ghostex_agent_skill(
        args,
        "ghostex browser mcp",
        &[
            "GHOSTEX_EMBEDDED_BROWSER_USE_SKILL_SOURCE",
            "GHOSTEX_BROWSER_SKILL_SOURCE",
        ],
        GHOSTEX_EMBEDDED_BROWSER_SKILL_NAME,
    )
}

pub fn browser_use_command(args: &[String]) -> CliResult<()> {
    skill_surface_command(
        args,
        &usage::browser_use_usage(),
        "browser-use",
        &install_browser_use_skill_command,
    )
}

pub fn install_browser_use_skill_command(args: &[String]) -> CliResult<()> {
    /*
    Browser Use is the Cua Driver page-content workflow. Keep it distinct from
    `ghostex browser`, which owns Ghostex's embedded CEF pane MCP server.
    */
    install_ghostex_agent_skill(
        args,
        "cua-driver",
        &["GHOSTEX_BROWSER_USE_SKILL_SOURCE"],
        GHOSTEX_BROWSER_SKILL_NAME,
    )
}

pub fn computer_use_command(args: &[String]) -> CliResult<()> {
    skill_surface_command(
        args,
        &usage::computer_use_usage(),
        "computer-use",
        &install_computer_use_skill_command,
    )
}

pub fn install_computer_use_skill_command(args: &[String]) -> CliResult<()> {
    /*
    Desktop Control setup must install an agent-facing `$ghostex-computer-use`
    skill in addition to Cua Driver so users can request computer use through a
    Ghostex-named wrapper instead of remembering the lower-level `$cua-driver`
    skill name.
    */
    install_ghostex_agent_skill(
        args,
        "cua-driver",
        &["GHOSTEX_COMPUTER_USE_SKILL_SOURCE"],
        GHOSTEX_COMPUTER_USE_SKILL_NAME,
    )
}

pub fn agent_orchestration_command(args: &[String]) -> CliResult<()> {
    skill_surface_command(
        args,
        &usage::agent_orchestration_usage(),
        "agent-orchestration",
        &install_agent_orchestration_skill_command,
    )
}

pub fn install_agent_orchestration_skill_command(args: &[String]) -> CliResult<()> {
    /*
    Agents need a Ghostex-native orchestration skill that teaches the CLI
    workflow for creating panes, sending messages to other agent sessions,
    checking status, and reading terminal output through `ghostex read-text`
    instead of reaching for raw zmx commands directly.
    */
    install_ghostex_agent_skill(
        args,
        "ghostex --help",
        &["GHOSTEX_AGENT_ORCHESTRATION_SKILL_SOURCE"],
        GHOSTEX_AGENT_ORCHESTRATION_SKILL_NAME,
    )
}

pub fn fable56_orchestration_command(args: &[String]) -> CliResult<()> {
    skill_surface_command(
        args,
        &usage::fable56_orchestration_usage(),
        "fable-5.6-orchestration",
        &install_fable56_orchestration_skill_command,
    )
}

pub fn install_fable56_orchestration_skill_command(args: &[String]) -> CliResult<()> {
    /*
    Agents need a bundled pipeline skill on top of `$ghostex-agent-orchestration`:
    plan a multi-phase task inline with Fable, launch one Codex gpt-5.6 worker
    pane per phase through supported Ghostex CLI commands, then verify with a
    Fable pane and spawn targeted fixers until verification passes.
    */
    install_ghostex_agent_skill(
        args,
        "ghostex --help",
        &["GHOSTEX_FABLE_56_ORCHESTRATION_SKILL_SOURCE"],
        GHOSTEX_FABLE_56_ORCHESTRATION_SKILL_NAME,
    )
}

pub fn find_prev_session_command(args: &[String]) -> CliResult<()> {
    skill_surface_command(
        args,
        &usage::find_prev_session_usage(),
        "find-prev-session",
        &install_find_prev_session_skill_command,
    )
}

pub fn install_find_prev_session_skill_command(args: &[String]) -> CliResult<()> {
    install_ghostex_agent_skill(
        args,
        "ghostex find --help",
        &["GHOSTEX_FIND_PREV_SESSION_SKILL_SOURCE"],
        GHOSTEX_FIND_PREV_SESSION_SKILL_NAME,
    )
}

pub fn generate_title_command(args: &[String]) -> CliResult<()> {
    skill_surface_command(
        args,
        &usage::generate_title_usage(),
        "generate-title",
        &install_generate_title_skill_command,
    )
}

pub fn install_generate_title_skill_command(args: &[String]) -> CliResult<()> {
    /*
    Agents must produce short titles, then stage `/rename <title>` and Enter
    through the supported `rename-command` session input path using stable
    self-session selectors.
    */
    install_ghostex_agent_skill(
        args,
        "ghostex rename-command --session-id \"${GHOSTEX_GLOBAL_SESSION_REF:-${GHOSTEX_SESSION_ID:-${ZMX_SESSION:-}}}\" --title \"<title>\"",
        &["GHOSTEX_GENERATE_TITLE_SKILL_SOURCE"],
        GHOSTEX_AUTO_RENAME_SESSION_SKILL_NAME,
    )
}

pub fn move_codex_session_command(args: &[String]) -> CliResult<()> {
    skill_surface_command(
        args,
        &usage::move_codex_session_usage(),
        "move-codex-session",
        &install_move_codex_session_skill_command,
    )
}

pub fn install_move_codex_session_skill_command(args: &[String]) -> CliResult<()> {
    /*
    Bundle `$ghostex-move-codex-session` so agents can give the supported
    `/status` plus `codex fork --yolo -C <folder-path> <SESSION_ID>` workflow
    instead of inventing an in-place `/cd` command.
    */
    install_ghostex_agent_skill(
        args,
        "codex fork --yolo -C <target-folder> <SESSION_ID>",
        &["GHOSTEX_MOVE_CODEX_SESSION_SKILL_SOURCE"],
        GHOSTEX_MOVE_CODEX_SESSION_SKILL_NAME,
    )
}

pub fn manage_beads_command(args: &[String]) -> CliResult<()> {
    skill_surface_command(
        args,
        &usage::manage_beads_usage(),
        "manage-beads",
        &install_manage_beads_skill_command,
    )
}

pub fn install_manage_beads_skill_command(args: &[String]) -> CliResult<()> {
    /*
    Agent-facing bead workflows must go through `gx bd` so installed agents use
    Ghostex's pinned bundled Beads binary instead of whichever shell `bd`
    happens to be first on PATH.
    */
    install_ghostex_agent_skill(
        args,
        "gx bd --help",
        &["GHOSTEX_MANAGE_BEADS_SKILL_SOURCE"],
        GHOSTEX_MANAGE_BEADS_SKILL_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "gx-cli-skills-{}-{}-{}",
            std::process::id(),
            name,
            TEMP_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, b"").expect("write");
    }

    fn source_flags(source: &Path) -> Flags {
        let mut flags = Flags::default();
        flags.insert_text("source", &source.to_string_lossy());
        flags
    }

    #[test]
    fn url_scheme_detection_matches_js_regex() {
        assert!(has_url_scheme("https://example.com/pkg"));
        assert!(has_url_scheme("file:/tmp/skills"));
        assert!(has_url_scheme("Git+ssh://host/repo"));
        assert!(!has_url_scheme("/absolute/path"));
        assert!(!has_url_scheme("relative/path"));
        assert!(!has_url_scheme(":oops"));
        assert!(!has_url_scheme("1abc:x"));
        assert!(!has_url_scheme("no colon here"));
    }

    #[test]
    fn explicit_url_source_is_returned_unresolved() {
        let flags = {
            let mut flags = Flags::default();
            flags.insert_text("source", "https://registry.example/skills.tgz");
            flags
        };
        let source = resolve_ghostex_agent_skill_package_source("gx-test-skill-nope", &[], &flags)
            .expect("url source");
        assert_eq!(source, "https://registry.example/skills.tgz");
    }

    #[test]
    fn package_source_resolves_directory_containing_skill() {
        let root = temp_root("package-source");
        let skill_name = "gx-test-skill-direct";
        let skills_dir = root.join("skills");
        touch(&skills_dir.join(skill_name).join("SKILL.md"));

        // A source pointing at the skills folder matches <candidate>/<skill>/SKILL.md.
        let source =
            resolve_ghostex_agent_skill_package_source(skill_name, &[], &source_flags(&skills_dir))
                .expect("skills dir source");
        assert_eq!(source, skills_dir.to_string_lossy());

        // A source pointing at the parent matches <candidate>/skills/<skill>/SKILL.md.
        let source =
            resolve_ghostex_agent_skill_package_source(skill_name, &[], &source_flags(&root))
                .expect("root source");
        assert_eq!(source, root.to_string_lossy());
    }

    #[test]
    fn unresolved_explicit_source_is_returned_verbatim() {
        let root = temp_root("package-source-verbatim");
        let missing = root.join("nowhere");
        let source = resolve_ghostex_agent_skill_package_source(
            "gx-test-skill-missing",
            &[],
            &source_flags(&missing),
        )
        .expect("explicit fallback");
        assert_eq!(source, missing.to_string_lossy().trim());
    }

    #[test]
    fn source_dir_error_names_first_env_var() {
        let error =
            resolve_ghostex_agent_skill_source_dir("gx-test-skill-error", &["GX_TEST_SKILL_SRC"])
                .expect_err("missing skill");
        assert_eq!(
            error.to_string(),
            "Could not find gx-test-skill-error. Reinstall Ghostex or set GX_TEST_SKILL_SRC to the skill directory."
        );
        let error = resolve_ghostex_agent_skill_source_dir("gx-test-skill-error", &[])
            .expect_err("missing skill without env vars");
        assert_eq!(
            error.to_string(),
            "Could not find gx-test-skill-error. Reinstall Ghostex or set GHOSTEX_SKILL_SOURCE to the skill directory."
        );
    }

    #[test]
    fn unknown_subcommand_error_embeds_usage() {
        let error = computer_use_command(&["frobnicate".to_string()]).expect_err("unknown");
        let message = error.to_string();
        assert!(message.starts_with("Unknown computer-use command: frobnicate\n\n"));
        assert!(message.contains("Ghostex Computer Use - install the agent skill"));

        let error = browser_use_command(&["frobnicate".to_string()]).expect_err("unknown");
        let message = error.to_string();
        assert!(message.starts_with("Unknown browser-use command: frobnicate\n\n"));
        assert!(message
            .contains("Ghostex Browser Use - install the Cua Driver browser-page agent skill"));
    }
}
