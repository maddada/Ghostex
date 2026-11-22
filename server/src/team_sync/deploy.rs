//! `ghostex team deploy`: puts Ghostex's Convex functions on the team's own Convex project with the
//! Convex CLI login of the person setting it up, and creates the team on the first deploy.
//!
//! Runs in the `ghostex` CLI process, not in gxserver: the Convex CLI may ask the person to pick a
//! Convex team or log in, so it needs their terminal.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use crate::paths::GxserverPaths;

/// The functions this Ghostex deploys, embedded so an installed Ghostex needs no repo checkout.
///
/// CDXC:TeamSync 2026-10-09 SEE-ALSO:
/// Every file under `packages/team-sync/convex/` (except `_generated/`, which the Convex CLI
/// regenerates on deploy) must be listed here, or deployed teams miss it.
const FUNCTION_FILES: &[(&str, &str)] = &[
    (
        "package.json",
        include_str!("../../../packages/team-sync/package.json"),
    ),
    (
        "convex.json",
        include_str!("../../../packages/team-sync/convex.json"),
    ),
    (
        "convex/tsconfig.json",
        include_str!("../../../packages/team-sync/convex/tsconfig.json"),
    ),
    (
        "convex/schema.ts",
        include_str!("../../../packages/team-sync/convex/schema.ts"),
    ),
    (
        "convex/lib/auth.ts",
        include_str!("../../../packages/team-sync/convex/lib/auth.ts"),
    ),
    (
        "convex/lib/signatures.ts",
        include_str!("../../../packages/team-sync/convex/lib/signatures.ts"),
    ),
    (
        "convex/teams.ts",
        include_str!("../../../packages/team-sync/convex/teams.ts"),
    ),
    (
        "convex/invites.ts",
        include_str!("../../../packages/team-sync/convex/invites.ts"),
    ),
    (
        "convex/commands.ts",
        include_str!("../../../packages/team-sync/convex/commands.ts"),
    ),
    (
        "convex/slackThreads.ts",
        include_str!("../../../packages/team-sync/convex/slackThreads.ts"),
    ),
    (
        "convex/slackRouting.ts",
        include_str!("../../../packages/team-sync/convex/slackRouting.ts"),
    ),
    (
        "convex/slackIntake.ts",
        include_str!("../../../packages/team-sync/convex/slackIntake.ts"),
    ),
    (
        "convex/linearIntake.ts",
        include_str!("../../../packages/team-sync/convex/linearIntake.ts"),
    ),
    (
        "convex/http.ts",
        include_str!("../../../packages/team-sync/convex/http.ts"),
    ),
    (
        "convex/lib/slackApi.ts",
        include_str!("../../../packages/team-sync/convex/lib/slackApi.ts"),
    ),
    (
        "convex/lib/linearApi.ts",
        include_str!("../../../packages/team-sync/convex/lib/linearApi.ts"),
    ),
    (
        "convex/lib/tickets.ts",
        include_str!("../../../packages/team-sync/convex/lib/tickets.ts"),
    ),
    (
        "convex/lib/threadContext.ts",
        include_str!("../../../packages/team-sync/convex/lib/threadContext.ts"),
    ),
    (
        "convex/teamFlow.ts",
        include_str!("../../../packages/team-sync/convex/teamFlow.ts"),
    ),
    (
        "convex/slackFlow.ts",
        include_str!("../../../packages/team-sync/convex/slackFlow.ts"),
    ),
    (
        "convex/slackFlowState.ts",
        include_str!("../../../packages/team-sync/convex/slackFlowState.ts"),
    ),
    (
        "convex/slackFlowReport.ts",
        include_str!("../../../packages/team-sync/convex/slackFlowReport.ts"),
    ),
    (
        "convex/slackPost.ts",
        include_str!("../../../packages/team-sync/convex/slackPost.ts"),
    ),
    (
        "convex/workPage.ts",
        include_str!("../../../packages/team-sync/convex/workPage.ts"),
    ),
    (
        "convex/linearKeys.ts",
        include_str!("../../../packages/team-sync/convex/linearKeys.ts"),
    ),
    (
        "convex/teamFlowSteps.ts",
        include_str!("../../../packages/team-sync/convex/teamFlowSteps.ts"),
    ),
    (
        "convex/slackGithubIssue.ts",
        include_str!("../../../packages/team-sync/convex/slackGithubIssue.ts"),
    ),
    ("convex/devMocks.ts", DEV_MOCKS_STUB),
];

/// What teams get in place of `convex/devMocks.ts`.
///
/// CDXC:TeamSync 2026-10-09 WHY:
/// The dev mocks (stand-ins for the Slack and Linear APIs, answering only with
/// `GHOSTEX_DEV_MOCKS=1`) exist to test the Slack flow on Ghostex's own dev deployment. A team's
/// deployment never needs them, so it gets this no-op instead of the mock routes and their
/// internal functions; `http.ts` still imports the same name. Its empty `devMockCalls` table stays
/// in the shared schema.
const DEV_MOCKS_STUB: &str = r#"import type { HttpRouter } from "convex/server";

/** Teams' deployments carry no dev mocks; Ghostex's dev deployment has the real file. */
export function registerDevMocks(_http: HttpRouter): void {}
"#;

pub(crate) struct DeployOptions {
    pub(crate) workspace_id: String,
    /// The Convex project to create on the first deploy (default `ghostex-<workspace id>`).
    pub(crate) project: Option<String>,
    /// The Convex team (slug) to create it in; the Convex CLI asks when there are several.
    pub(crate) convex_team: Option<String>,
    pub(crate) team_name: Option<String>,
    pub(crate) owner_name: Option<String>,
    /// Use the project's development deployment instead of production.
    pub(crate) dev: bool,
}

pub(crate) struct DeployOutcome {
    pub(crate) deployment_url: String,
    pub(crate) functions_dir: PathBuf,
    /// The owner's member token when this deploy created the team.
    pub(crate) owner_member_token: Option<String>,
}

fn safe_folder_name(workspace_id: &str) -> String {
    let name: String = workspace_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    if name.trim_matches('-').is_empty() {
        "workspace".to_string()
    } else {
        name
    }
}

/// One folder per workspace, kept between deploys: the Convex CLI's `.env.local` there remembers
/// which Convex project this workspace deploys to.
pub(super) fn functions_dir(paths: &GxserverPaths, workspace_id: &str) -> PathBuf {
    paths
        .app_data_dir
        .join("team-sync")
        .join(safe_folder_name(workspace_id))
}

fn write_function_files(dir: &Path) -> Result<(), String> {
    // Files removed from Ghostex since the last deploy must not be deployed again.
    let _ = fs::remove_dir_all(dir.join("convex"));
    for (relative, contents) in FUNCTION_FILES {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
        }
        fs::write(&path, contents)
            .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    }
    Ok(())
}

/// Records which `package.json` the folder's `node_modules` was installed from.
fn install_stamp_path(dir: &Path) -> PathBuf {
    dir.join("node_modules").join(".ghostex-package.json")
}

fn package_json() -> &'static str {
    FUNCTION_FILES
        .iter()
        .find(|(relative, _)| *relative == "package.json")
        .map(|(_, contents)| *contents)
        .unwrap_or_default()
}

fn npm_command() -> Command {
    if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", "npm"]);
        command
    } else {
        Command::new("npm")
    }
}

pub(super) fn ensure_dependencies(dir: &Path) -> Result<(), String> {
    if fs::read_to_string(install_stamp_path(dir)).ok().as_deref() == Some(package_json()) {
        return Ok(());
    }
    eprintln!("Installing the Convex CLI for this workspace…");
    let status = npm_command()
        .args(["install", "--no-audit", "--no-fund", "--loglevel=error"])
        .current_dir(dir)
        .status()
        .map_err(|error| {
            format!("Could not run npm ({error}). Deploying needs Node.js: https://nodejs.org")
        })?;
    if !status.success() {
        return Err("npm install failed; see the output above.".to_string());
    }
    let _ = fs::write(install_stamp_path(dir), package_json());
    Ok(())
}

/// The Convex CLI from this folder's `node_modules`, run with `node` directly (no `npx` and no
/// `.cmd` shim, so JSON arguments reach it intact on Windows).
pub(super) fn convex_cli(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("node");
    command
        .arg(dir.join("node_modules/convex/bin/main.js"))
        .args(args)
        .current_dir(dir);
    command
}

fn run_interactive(dir: &Path, args: &[&str]) -> Result<(), String> {
    let status = convex_cli(dir, args).status().map_err(|error| {
        format!("Could not run the Convex CLI ({error}). Is Node.js installed?")
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`convex {}` failed; see the output above.",
            args.join(" ")
        ))
    }
}

/// Runs the Convex CLI and returns its stdout (stderr goes to the terminal).
fn run_captured(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = convex_cli(dir, args)
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| {
            format!("Could not run the Convex CLI ({error}). Is Node.js installed?")
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        Err(format!(
            "`convex {}` failed: {}",
            args.join(" "),
            stdout.trim()
        ))
    }
}

/// `convex deploy --yes`, echoing its output; returns stdout and stderr together, because the CLI
/// reports the deployment URL ("Deployed Convex functions to https://….convex.cloud") on stderr.
fn run_deploy(dir: &Path) -> Result<String, String> {
    let output = convex_cli(dir, &["deploy", "--yes"])
        .stdin(Stdio::inherit())
        .output()
        .map_err(|error| {
            format!("Could not run the Convex CLI ({error}). Is Node.js installed?")
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{stdout}");
    eprint!("{stderr}");
    if !output.status.success() {
        return Err("`convex deploy` failed; see the output above.".to_string());
    }
    Ok(format!(
        "{stdout}
{stderr}"
    ))
}

fn env_local_value(dir: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(dir.join(".env.local")).ok()?;
    text.lines()
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| {
                value
                    .split('#')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"')
                    .to_string()
            })
        })
        .filter(|value| !value.is_empty())
}

fn first_cloud_url(text: &str) -> Option<String> {
    text.split(|character: char| character.is_whitespace() || character == '"' || character == '\'')
        .map(|word| word.trim_end_matches(['.', ',', ')']))
        .find(|word| word.starts_with("https://") && word.ends_with(".convex.cloud"))
        .map(str::to_string)
}

fn default_owner_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "Owner".to_string())
}

pub(crate) fn deploy_team_functions(
    paths: &GxserverPaths,
    options: &DeployOptions,
) -> Result<DeployOutcome, String> {
    let dir = functions_dir(paths, &options.workspace_id);
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not create {}: {error}", dir.display()))?;
    write_function_files(&dir)?;
    ensure_dependencies(&dir)?;

    let configured = env_local_value(&dir, "CONVEX_DEPLOYMENT").is_some();
    if !configured {
        let project = options
            .project
            .clone()
            .unwrap_or_else(|| format!("ghostex-{}", safe_folder_name(&options.workspace_id)));
        eprintln!("Creating the Convex project \"{project}\" with your Convex CLI login…");
        let mut args = vec!["dev", "--once", "--configure", "new", "--project", &project];
        if let Some(team) = options.convex_team.as_deref() {
            args.extend(["--team", team]);
        }
        run_interactive(&dir, &args)?;
    }

    let deployment_url = if options.dev {
        if configured {
            run_interactive(&dir, &["dev", "--once"])?;
        }
        env_local_value(&dir, "CONVEX_URL")
            .ok_or("The Convex CLI did not record the development deployment URL.")?
    } else {
        let output = run_deploy(&dir)?;
        first_cloud_url(&output)
            .ok_or("Could not find the production deployment URL in the Convex CLI output.")?
    };

    let target: &[&str] = if options.dev { &[] } else { &["--prod"] };
    let run = |function: &str, args: Option<String>| -> Result<String, String> {
        let mut command_args = vec!["run"];
        command_args.extend_from_slice(target);
        command_args.push(function);
        if let Some(args) = args.as_deref() {
            command_args.push(args);
        }
        run_captured(&dir, &command_args)
    };
    let has_team = run("teams:hasTeam", None)?.trim() == "true";
    let owner_member_token = if has_team {
        run("teams:recordFunctionsVersion", None)?;
        None
    } else {
        let args = json!({
            "name": options.team_name.clone().unwrap_or_else(|| options.workspace_id.clone()),
            "ownerName": options.owner_name.clone().unwrap_or_else(default_owner_name),
        });
        let created = run("teams:createTeam", Some(args.to_string()))?;
        let created: Value = serde_json::from_str(created.trim())
            .map_err(|_| "The Convex CLI returned an unreadable team.".to_string())?;
        Some(
            created
                .get("memberToken")
                .and_then(Value::as_str)
                .ok_or("Creating the team returned no member token.")?
                .to_string(),
        )
    };
    Ok(DeployOutcome {
        deployment_url,
        functions_dir: dir,
        owner_member_token,
    })
}
