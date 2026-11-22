use std::io::Write;
use std::path::PathBuf;

use crate::ghostex_cli::args::{Flags, multi_value_flag, parse_args};
use crate::ghostex_cli::launchers;
use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::ghostex_cli::usage;

/*
CDXC:Cli 2026-07-13:
Faithful port of the Node CLI agent-skill surfaces. Public Ghostex skill
install commands delegate to gxserver's external `agent-skills` CLI wrapper
(resolved like `gx server ...`) instead of copying folders into one legacy
shared path, keeping the bundled skill package source local so installed app
builds install the skill version that matches their CLI commands.
*/

const GHOSTEX_EMBEDDED_BROWSER_SKILL_NAME: &str = "ghostex-embedded-browser-use";
const GHOSTEX_SPACEO_SKILL_NAME: &str = "ghostex-spaceo";
const GHOSTEX_CLI_SKILL_NAME: &str = "ghostex-cli";
const GHOSTEX_MANAGE_BEADS_SKILL_NAME: &str = "ghostex-manage-beads";
const GHOSTEX_AGENTS_ORCHESTRATION_SKILL_NAME: &str = "ghostex-agents";
const GHOSTEX_AUTO_RENAME_SESSION_SKILL_NAME: &str = "ghostex-auto-rename-session";
const GHOSTEX_MOVE_CODEX_SESSION_SKILL_NAME: &str = "ghostex-move-codex-session";
const GHOSTEX_HELP_SKILL_NAME: &str = "ghostex-help";
const GHOSTEX_VISUALS_SKILL_NAME: &str = "ghostex-visuals";
const GHOSTEX_SECRETS_SKILL_NAME: &str = "ghostex-secrets";
const GHOSTEX_AGENT_GUIDE_SKILL_NAME: &str = "ghostex";

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

/// The `skills/` folder of the gxserver package that owns this CLI
/// (`<package>/bin/ghostex` → `<package>/skills`).
///
/// CDXC:AgentSkills 2026-11-06 WHY:
/// A gxserver package that runs without the Ghostex app (the Linux desktop package, the WSL runtime, remote Ubuntu hosts, a remote Mac) has no app-bundle `CLI/skills` beside it, so skill installs there failed with "Could not find ghostex-cli" (GitHub issue #124 on Linux; a remote Mac mini on 2026-11-06). Every package now carries the skills in its own `skills/` folder, and this is the candidate that finds it; the macOS bundle's `CLI/skills` is a link to the same folder. Supersedes the 2026-09-11 note, which fixed only the Linux package.
/// SEE-ALSO: server/package-remote-linux.mjs, apps/desktop/scripts/build-macos-app.sh.
fn gxserver_package_skills_dir(cli_dir: &std::path::Path) -> PathBuf {
    launchers::js_path_resolve(&cli_dir.join("..")).join("skills")
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
    candidates.push(Some(gxserver_package_skills_dir(&cli_dir).join(skill_name)));
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
    candidates.push(Some(gxserver_package_skills_dir(&cli_dir)));
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
    if flags.truthy("offline") {
        gxserver_args.push("--offline".to_string());
    }
    if flags.truthy("json") {
        gxserver_args.push("--json".to_string());
        let launch = launchers::resolve_gxserver_cli_launch()?;
        let mut full_args = launch.args.clone();
        full_args.extend(gxserver_args);
        let mut child = crate::platform::process::background_command(&launch.command);
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

/// What `install-skill` answers for a skill retired for CLI help: nothing is installed, and the
/// message names what agents read instead.
///
/// CDXC:AgentSkills 2026-11-20 DECISION:
/// User (Q4262 "B"): Ghostex's own skills are retired for CLI help; only `ghostex` (`ghostex agent-guide install-skill`) still installs. The verbs stay so a script or an older app that still runs one gets a clear answer instead of "unknown command".
fn retired_skill_install(_args: &[String], name: &str, instead: &str) -> CliResult<()> {
    Err(CliError::Other(format!(
        "The {name} skill is retired: agents read {instead} instead, which always matches this Ghostex. Nothing to install."
    )))
}

/// `computer-use install-skill` and `browser-use install-skill`: Trycua's `cua-driver` skill, which
/// replaced the retired Ghostex wrappers (managed_tools/desktop_control.rs `install_trycua_skill`).
fn install_cua_driver_skill(args: &[String]) -> CliResult<()> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::paths::get_gxserver_paths(None).home_dir);
    let result = crate::managed_tools::desktop_control::install_trycua_skill(&home);
    if parse_args(args).flags.truthy("json") {
        let value = match &result {
            Ok(message) => {
                serde_json::json!({ "ok": true, "skill": "cua-driver", "message": message })
            }
            Err(error) => serde_json::json!({ "ok": false, "skill": "cua-driver", "error": error }),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
        return result.map(|_| ()).map_err(CliError::Other);
    }
    println!("{}", result.map_err(CliError::Other)?);
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
    retired_skill_install(
        args,
        GHOSTEX_EMBEDDED_BROWSER_SKILL_NAME,
        "`ghostex browser --help`",
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
    install_cua_driver_skill(args)
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
    install_cua_driver_skill(args)
}

/// How to drive Mac apps on SpaceO's hidden displays (spaceo_guide.md), which `ghostex spaceo guide` prints.
///
/// CDXC:AgentSkills 2026-11-20 DECISION:
/// User (Q4262 "B"): Ghostex's own skills are retired for CLI help; the `ghostex-spaceo` skill's text lives here now.
const SPACEO_GUIDE_MD: &str = include_str!("spaceo_guide.md");

pub fn spaceo_command(args: &[String]) -> CliResult<()> {
    let rest = args.get(1..).unwrap_or_default();
    let help = rest.iter().any(|arg| arg == "-h" || arg == "--help");
    match args.first().map(String::as_str) {
        Some("status") if !help => return super::spaceo_heal::status_command(rest),
        Some("repair") if !help => return super::spaceo_heal::repair_command(rest),
        Some("guide") if !help => {
            print!("{SPACEO_GUIDE_MD}");
            return Ok(());
        }
        _ => {}
    }
    skill_surface_command(
        args,
        &usage::spaceo_usage(),
        "spaceo",
        &install_spaceo_skill_command,
    )
}

/// CDXC:ManagedTools 2026-10-14 DECISION:
/// User: "SpaceO is macOS-only for now": on Windows and Linux every SpaceO surface is hidden, and the CLI refuses with a clear "SpaceO is only available on macOS" instead of failing oddly.
pub fn install_spaceo_skill_command(args: &[String]) -> CliResult<()> {
    retired_skill_install(args, GHOSTEX_SPACEO_SKILL_NAME, "`ghostex spaceo guide`")
}

pub fn cli_command(args: &[String]) -> CliResult<()> {
    skill_surface_command(args, &usage::cli_usage(), "cli", &install_cli_skill_command)
}

pub fn install_cli_skill_command(args: &[String]) -> CliResult<()> {
    retired_skill_install(args, GHOSTEX_CLI_SKILL_NAME, "`ghostex --help`")
}

/*
CDXC:AgentSkills 2026-08-24:
The `ghostex-manage-automations`, `ghostex-agent-orchestration`, and
`ghostex-find-prev-session` skills were folded into the CLI help; `$ghostex-cli`
is the single entry-point skill. `gx automations --help` stays as a plain
focused-help surface with no skill installer.
*/
pub fn automations_command(args: &[String]) -> CliResult<()> {
    let usage_text = usage::automations_usage();
    let subcommand = args.first().map(String::as_str).unwrap_or("help");
    match subcommand {
        "help" | "-h" | "--help" => {
            println!("{usage_text}");
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown automations command: {other}\n\n{usage_text}"
        ))),
    }
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
    The Project Board workflow runs through the machine-installed `bd` Beads
    CLI; the skill teaches the swimlane statuses, session-link comments, and
    safety rules around it.
    */
    retired_skill_install(
        args,
        GHOSTEX_MANAGE_BEADS_SKILL_NAME,
        "`ghostex board guide`",
    )
}

pub fn agents_orchestration_command(args: &[String]) -> CliResult<()> {
    skill_surface_command(
        args,
        &usage::agents_orchestration_usage(),
        "agents-orchestration",
        &install_agents_orchestration_skill_command,
    )
}

pub fn install_agents_orchestration_skill_command(args: &[String]) -> CliResult<()> {
    /*
    The skill carries no command reference of its own: it sends the agent to
    `ghostex agents --help` and `ghostex --help` and adds model-independent
    orchestration habits on top.
    */
    retired_skill_install(
        args,
        GHOSTEX_AGENTS_ORCHESTRATION_SKILL_NAME,
        "`ghostex agents guide`",
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
    retired_skill_install(
        args,
        GHOSTEX_AUTO_RENAME_SESSION_SKILL_NAME,
        "`ghostex agents guide`",
    )
}

pub fn install_help_skill_command(args: &[String]) -> CliResult<()> {
    /*
    `$ghostex-help` teaches agents to explain Ghostex from `ghostex guide` and
    to change settings through `ghostex settings`, so the configure command it
    advertises is the guide entry point rather than a session command.
    */
    retired_skill_install(args, GHOSTEX_HELP_SKILL_NAME, "`ghostex help guide`")
}

pub fn install_visuals_skill_command(args: &[String]) -> CliResult<()> {
    /*
    `$ghostex-visuals` teaches agents the chat's ```visual blocks and `ghostex show` pages; its
    entry point is the `visual` verb that checks a block before the agent replies.
    */
    retired_skill_install(args, GHOSTEX_VISUALS_SKILL_NAME, "`ghostex visual guide`")
}

/// `$ghostex-secrets` teaches agents to ask for a password or token with `ghostex secret request`
/// instead of in the chat.
pub fn install_secrets_skill_command(args: &[String]) -> CliResult<()> {
    retired_skill_install(args, GHOSTEX_SECRETS_SKILL_NAME, "`ghostex secret guide`")
}

/// `$ghostex-agent-guide` gives agents without a launch note the Ghostex note as a skill they may
/// load on their own, which sends them to `ghostex agent-guide`.
pub fn install_agent_guide_skill_command(args: &[String]) -> CliResult<()> {
    install_ghostex_agent_skill(
        args,
        "ghostex agent-guide",
        &["GHOSTEX_AGENT_GUIDE_SKILL_SOURCE"],
        GHOSTEX_AGENT_GUIDE_SKILL_NAME,
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
    retired_skill_install(
        args,
        GHOSTEX_MOVE_CODEX_SESSION_SKILL_NAME,
        "`ghostex agents guide`",
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
        assert!(message.contains("Ghostex Computer Use - native desktop app control"));

        let error = browser_use_command(&["frobnicate".to_string()]).expect_err("unknown");
        let message = error.to_string();
        assert!(message.starts_with("Unknown browser-use command: frobnicate\n\n"));
        assert!(message.contains("Ghostex Browser Use - pages in other browsers"));
    }
}
