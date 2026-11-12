use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::LazyLock,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Definition {
    pub agent_id: String,
    pub binary: String,
    pub mise_tool: Option<String>,
    pub npm_package: Option<String>,
    #[serde(default)]
    pub npm_flags: Vec<String>,
    /// Packages a package-manager install puts on the computer in the same command, before `npm_package`.
    #[serde(default)]
    pub npm_companions: Vec<NpmCompanion>,
    pub package_managers: Option<Vec<String>>,
    pub brew_formula: Option<String>,
    #[serde(default)]
    pub brew_cask: bool,
    pub winget_id: Option<String>,
    pub native: Option<Native>,
    pub version_args: Option<Vec<String>>,
    #[serde(default)]
    pub install_dirs: InstallDirs,
    pub latest_version: Option<LatestVersion>,
    /// A sentence every install method's tooltip adds (Grok and Cursor both install `agent`).
    pub install_note: Option<String>,
    /// The update kills running processes of the CLI (Droid's installer runs `pkill droid`).
    #[serde(default)]
    pub update_stops_sessions: bool,
    /// The CLI's own updater for an agent without an official installer entry (`copilot update`).
    pub self_update: Option<String>,
    /// A command an npm or mise install or update runs next, once it succeeded (`dsh-tui update`).
    pub then_run: Option<String>,
    /// Arguments of the CLI's own "is there an update" check, for an agent whose version number does not
    /// move with its releases (Hermes updates by git commit).
    pub update_check: Option<Vec<String>>,
    /// The oldest Bun the bun-installed package runs on (OMP).
    pub minimum_bun_version: Option<String>,
}

/// CDXC:AgentProviders 2026-11-12 DECISION:
/// User: DeepSeek Harness installs with its documented `npm install -g @deepseek-ai/dsh @deepseek-harness-tui/dsh-tui`, keeps mise only where mise already manages it, and Ghostex no longer downloads mise for it. Supersedes the 2026-10-28 decision to install it only through mise. The terminal app Ghostex runs (`dsh-tui`) runs on the official `dsh` CLI (`@deepseek-ai/dsh`) and sets its profile up with pnpm, so the install names all of them; pnpm only when it is missing, because many users already have it from Homebrew or corepack.
/// WHY: dsh-tui's command and the profile copy it starts are installed separately, and a command a minor version ahead of its profile refuses to start, so after an install or update the job runs `dsh-tui update` (`then_run`), which brings the profile to the same release.
/// WHY: every `@deepseek-ai/dsh` release is a pre-release. mise skips those unless asked, and asked it takes the newest one (0.2.1-rc.4 on 2026-10-28), which dsh-tui 0.17 refused to install its profile on ("incompatible with dsh 0.2.1-rc.4"). npm's `latest` tag is DeepSeek's stable channel, so a mise that already manages it installs exactly that release (`pin_npm_latest`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NpmCompanion {
    pub package: String,
    /// Left out when this command is already on the computer.
    pub unless_command: Option<String>,
    /// Installed at the release npm's `latest` tag names, not mise's newest version.
    #[serde(default)]
    pub pin_npm_latest: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Native {
    install: String,
    update: Option<String>,
    windows_install: Option<String>,
    /// CDXC:AgentProviders 2026-10-25 WHY:
    /// The Windows update when the CLI's own updater breaks there: OpenCode 2.0.16's `opencode upgrade` hands bash a Windows path with its backslashes stripped (`/bin/bash: C:Usersmadda.cacheopencode…install: No such file`), so on Windows OpenCode updates by running its install script again, which installs the latest release over `~/.opencode/bin`.
    windows_update: Option<String>,
    /// Variables the official installer and updater need, such as `CODEX_NON_INTERACTIVE`.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Lowercase, `/`-separated path fragments that only this agent's official installer produces.
    #[serde(default)]
    path_markers: Vec<String>,
    /// Commands the official installer needs besides curl, per platform family.
    #[serde(default)]
    requires: InstallDirs,
    /// The macOS installer copies an app into /Applications (Kiro).
    #[serde(default)]
    needs_applications_folder: bool,
    /// Tooltip wording for an inline script that is not worth showing verbatim (Rovo Dev's download).
    plan: Option<String>,
    /// The installer needs Node.js and npm already (Pi's), so Ghostex installs its own Node.js first
    /// when npm is missing.
    #[serde(default)]
    needs_npm: bool,
    /// The CLI updates itself in the background while it runs and has no update command, and its
    /// installer does nothing over an existing install (Antigravity's).
    #[serde(default)]
    updates_itself: bool,
}

/// Folders the official installer puts the binary in, `~` and `%VAR%` expanded. An installer that does not
/// add its folder to PATH (Claude's) still leaves a CLI Ghostex can find, and put on PATH.
#[derive(Default, Deserialize)]
pub(crate) struct InstallDirs {
    #[serde(default)]
    windows: Vec<String>,
    #[serde(default)]
    unix: Vec<String>,
}

impl Native {
    /// The update command on this platform: `windowsUpdate` on Windows when the catalog has one.
    fn update_command(&self) -> Option<&str> {
        cfg!(windows)
            .then_some(self.windows_update.as_deref())
            .flatten()
            .or(self.update.as_deref())
    }
}

/// Where the newest published version is read: a plain-text version, or one field of a JSON reply.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LatestVersion {
    pub url: String,
    pub json_field: Option<String>,
}

impl Definition {
    pub(crate) fn install_dirs(&self, home: &Path) -> Vec<PathBuf> {
        let dirs = if cfg!(windows) {
            &self.install_dirs.windows
        } else {
            &self.install_dirs.unix
        };
        dirs.iter()
            .filter_map(|dir| expand_dir(dir, home))
            .collect()
    }

    /// The command the CLI updates itself with, whichever way it was installed.
    pub(crate) fn self_updater(&self) -> Option<&str> {
        self.native
            .as_ref()
            .and_then(Native::update_command)
            .or(self.self_update.as_deref())
    }

    /// The CLI updates itself while it runs and Ghostex has no update to run (Antigravity).
    pub(crate) fn updates_itself(&self) -> bool {
        self.native
            .as_ref()
            .is_some_and(|native| native.updates_itself && native.update_command().is_none())
    }

    /// The official installer for this platform family (unix: `native.install`).
    pub(crate) fn native_install(&self) -> Option<&str> {
        self.native.as_ref().map(|native| native.install.as_str())
    }

    pub(crate) fn native_env(&self) -> BTreeMap<String, String> {
        self.native
            .as_ref()
            .map(|native| native.env.clone())
            .unwrap_or_default()
    }
}

fn expand_dir(template: &str, home: &Path) -> Option<PathBuf> {
    let mut text = template.to_string();
    if let Some(rest) = text.strip_prefix("~/") {
        text = format!("{}/{rest}", home.to_string_lossy());
    }
    while let Some(start) = text.find('%') {
        let end = start + 1 + text[start + 1..].find('%')?;
        let value = std::env::var(&text[start + 1..end]).ok()?;
        text.replace_range(start..=end, &value);
    }
    let path = if cfg!(windows) {
        PathBuf::from(text.replace('/', "\\"))
    } else {
        PathBuf::from(text)
    };
    path.is_absolute().then_some(path)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Method {
    pub id: String,
    pub label: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    /// Tooltip text: exactly what one click does, including anything Ghostex installs first.
    pub plan: String,
    /// A tool Ghostex installs before running `command` ("node", "homebrew" or "systemTools").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prerequisite: Option<String>,
    /// Commands `systemTools` must provide first (Linux).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub system_tools: Vec<String>,
}

impl Method {
    fn new(id: &str, label: &str, command: String, plan: String) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            command,
            unavailable_reason: None,
            plan,
            prerequisite: None,
            system_tools: Vec::new(),
        }
    }
}

pub(crate) static CATALOG: LazyLock<Vec<Definition>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../packages/shared/agent-cli-catalog.json"
    ))
    .expect("valid bundled CLI catalog")
});

/// CDXC:ManagedTools 2026-09-29 DECISION:
/// User: "when they click on something, we help them install it on windows/macos/linux automatically (show a button with a tooltip explaining how we'll install) but 1 click installs it for them as much as possible". Every method carries its tooltip `plan`, and a method whose tool is missing but that Ghostex can install (npm through its own Node.js, Homebrew on a Mac, curl/unzip/git on Linux) stays available with that tool as its `prerequisite` instead of asking the user to install it first.
pub(crate) fn methods(
    definition: &Definition,
    executable: Option<&str>,
    home: &Path,
    mise_installation: Option<&super::mise::Installation>,
) -> Vec<Method> {
    let installed = executable.is_some();
    let mut methods = Vec::new();
    let note = |plan: String| -> String {
        let mut plan = plan;
        if let Some(note) = &definition.install_note {
            plan.push(' ');
            plan.push_str(note);
        }
        if installed && definition.update_stops_sessions {
            plan.push_str(" Updating stops sessions that are running it.");
        }
        plan
    };
    if let Some(native) = &definition.native {
        let install = if cfg!(windows) {
            native.windows_install.as_deref()
        } else {
            Some(native.install.as_str())
        };
        if let Some(install) = install {
            let command = if installed {
                native.update_command().unwrap_or(install)
            } else {
                install
            };
            let mut method = Method::new(
                "native",
                "Official installer",
                command.into(),
                note(match &native.plan {
                    Some(plan) => plan.clone(),
                    None => format!("Runs the official installer: {command}. No password needed."),
                }),
            );
            if installed && definition.updates_itself() {
                // CDXC:AgentProviders 2026-11-12 DECISION:
                // User: "Antigravity's Update says it updates itself instead of re-running a script that does nothing". Its installer stops at "already installed" and the CLI updates itself in the background while it runs, so the method names no command and says so.
                let name = crate::sidebar_hud::default_agent_name(&definition.agent_id)
                    .unwrap_or(&definition.binary);
                let reason = format!("{name} updates itself in the background while it runs, so there is nothing for Ghostex to run. Start a new session to use the newest version.");
                method.command = String::new();
                method.plan = reason.clone();
                method.unavailable_reason = Some(reason);
            } else if installed
                && command != install
                && !command.starts_with("curl ")
                && !command.contains("irm ")
            {
                // The CLI's own updater.
                method.unavailable_reason = None;
            } else {
                apply_native_requirements(&mut method, native, command, home);
                if native.needs_npm
                    && method.prerequisite.is_none()
                    && method.unavailable_reason.is_none()
                    && missing_command("npm", home).is_some()
                {
                    // CDXC:AgentProviders 2026-11-12 WHY:
                    // Pi's installer installs Node.js itself only after asking in a terminal, and a Ghostex job has none ("No terminal detected; install Node.js 22.19.0 or newer and npm"), so Ghostex installs its own Node.js first, as the npm method does.
                    match crate::managed_tools::node::supported() {
                        Ok(()) => {
                            method.prerequisite = Some("node".into());
                            method.plan = note(format!(
                                "Ghostex first downloads Node.js (LTS) from nodejs.org into its tools folder and adds it to the end of your PATH, then runs the official installer: {command}. No password needed."
                            ));
                        }
                        Err(reason) => method.unavailable_reason = Some(reason),
                    }
                }
            }
            methods.push(method);
        }
    }
    if let Some(package) = &definition.npm_package {
        let managers = definition
            .package_managers
            .clone()
            .unwrap_or_else(|| vec!["npm".into(), "bun".into(), "pnpm".into()]);
        let packages = npm_install_packages(definition, package, |command| {
            crate::agent_hooks::probing::command_exists(command, home)
        });
        for manager in managers {
            let verb = if manager == "pnpm" { "add" } else { "install" };
            let flags = if manager == "npm" && !definition.npm_flags.is_empty() {
                format!("{} ", definition.npm_flags.join(" "))
            } else {
                String::new()
            };
            let command = then_run(
                format!("{manager} {verb} -g {flags}{packages}"),
                definition.then_run.as_deref(),
            );
            let mut method = Method::new(
                &manager,
                &manager,
                command.clone(),
                note(format!("Runs {command}.")),
            );
            let missing = missing_command(&manager, home);
            if manager == "npm" && missing.is_some() {
                match crate::managed_tools::node::supported() {
                    Ok(()) => {
                        method.prerequisite = Some("node".into());
                        method.plan = note(format!(
                            "Ghostex first downloads Node.js (LTS) from nodejs.org into its tools folder and adds it to the end of your PATH, then runs {command}. No password needed."
                        ));
                    }
                    Err(reason) => method.unavailable_reason = Some(reason),
                }
            } else {
                method.unavailable_reason = missing;
            }
            if manager == "bun" && method.unavailable_reason.is_none() {
                method.unavailable_reason = definition
                    .minimum_bun_version
                    .as_deref()
                    .and_then(|minimum| bun_too_old(&definition.binary, minimum, home));
            }
            methods.push(method);
        }
    }
    if !cfg!(windows) {
        if let Some(formula) = &definition.brew_formula {
            let installed_formula = executable.and_then(brew_package);
            let formula = installed_formula
                .as_deref()
                .filter(|name| {
                    name.starts_with(&format!(
                        "{}@",
                        formula.rsplit('/').next().unwrap_or(formula)
                    ))
                })
                .unwrap_or(formula);
            let verb = if installed { "upgrade" } else { "install" };
            let cask = if definition.brew_cask { "--cask " } else { "" };
            let command = format!("brew {verb} {cask}{formula}");
            let mut method = Method::new(
                "brew",
                "Homebrew",
                command.clone(),
                note(format!("Runs {command} with Homebrew.")),
            );
            if let Some(missing) = missing_command("brew", home) {
                match crate::managed_tools::homebrew::can_install_homebrew() {
                    Ok(()) if !installed => {
                        method.prerequisite = Some("homebrew".into());
                        method.plan = note(format!(
                            "Ghostex first installs Homebrew with its official installer (macOS asks for your password once, and Apple's Command Line Tools are installed if missing), then runs {command}."
                        ));
                    }
                    Ok(()) => method.unavailable_reason = Some(missing),
                    Err(reason) => {
                        method.unavailable_reason = Some(if cfg!(target_os = "macos") {
                            reason
                        } else {
                            missing
                        })
                    }
                }
            }
            methods.push(method);
        }
    }
    if cfg!(windows) {
        if let Some(package) = &definition.winget_id {
            let verb = if installed { "upgrade" } else { "install" };
            let command = format!("winget {verb} --id {package} --exact --disable-interactivity");
            let mut method = Method::new(
                "winget",
                "WinGet",
                command.clone(),
                note(format!("Runs {command}.")),
            );
            method.unavailable_reason = missing_command("winget", home);
            methods.push(method);
        }
    }
    let runtime_update = mise_runtime_update(definition, mise_installation, home);
    if uses_mise(mise_installation, runtime_update.is_some()) {
        if let Some(method) =
            mise_method(definition, mise_installation, runtime_update, home, &note)
        {
            methods.insert(0, method);
        }
    }
    methods
}

/// CDXC:AgentProviders 2026-11-12 DECISION:
/// User: "Ghostex must stop installing agent CLIs onto mise for users who don't already have them there". A CLI mise already manages (its own mise tool, or a global npm or bun package inside a mise runtime) keeps installing and updating through mise; every other CLI, missing or installed some other way, uses its official method (install script, npm, Homebrew, WinGet), so mise is not offered for it at all. Supersedes the 2026-09-14 decision that offered mise first for every agent on a computer that has mise.
/// WHY: mise fights each CLI's own updater, and its version list can name releases with no download (OpenCode's `@latest` returned 404s).
/// Since 2026-11-12 no agent installs only through mise (DeepSeek Harness moved to npm), so Ghostex never downloads mise to install an agent.
fn uses_mise(mise_installation: Option<&super::mise::Installation>, runtime_update: bool) -> bool {
    mise_installation.is_some_and(|installation| installation.tool.is_some()) || runtime_update
}

/// `command` followed by `then` once `command` succeeded (`npm install -g … && dsh-tui update`).
fn then_run(command: String, then: Option<&str>) -> String {
    match then {
        Some(then) if cfg!(windows) => format!("{command}; if (-not $LASTEXITCODE) {{ {then} }}"),
        Some(then) => format!("{command} && {then}"),
        None => command,
    }
}

/// A global npm (or bun) package inside a mise runtime updates with that runtime's own package
/// manager, run through mise, so it lands in the same folder instead of becoming a second copy.
fn mise_runtime_update(
    definition: &Definition,
    mise_installation: Option<&super::mise::Installation>,
    home: &Path,
) -> Option<String> {
    let (runtime, package) = mise_installation
        .filter(|installation| installation.tool.is_none())
        .and_then(|installation| installation.runtime.as_deref())
        .zip(definition.npm_package.as_deref())?;
    let manager = match runtime.split('@').next() {
        Some("node") => "npm",
        Some("bun") => "bun",
        _ => return None,
    };
    let allowed = definition
        .package_managers
        .as_ref()
        .is_none_or(|managers| managers.iter().any(|offered| offered == manager));
    allowed.then(|| {
        let packages = npm_install_packages(definition, package, |command| {
            crate::agent_hooks::probing::command_exists(command, home)
        });
        let flags = if manager == "npm" && !definition.npm_flags.is_empty() {
            format!("{} ", definition.npm_flags.join(" "))
        } else {
            String::new()
        };
        let command = format!(
            "mise exec {} -- {manager} install -g {flags}{packages}",
            super::process::quote(runtime)
        );
        match &definition.then_run {
            Some(then) => super::mise::then_run(&command, then),
            None => command,
        }
    })
}

/// The mise method: the runtime's package manager for a package inside a mise runtime, else
/// `mise upgrade` for the mise tool that holds the CLI, else `mise use` for an agent mise must install.
fn mise_method(
    definition: &Definition,
    mise_installation: Option<&super::mise::Installation>,
    runtime_update: Option<String>,
    home: &Path,
    note: &dyn Fn(String) -> String,
) -> Option<Method> {
    if let Some(command) = runtime_update {
        return Some(Method::new(
            "mise",
            "mise",
            command.clone(),
            note(format!("Runs {command} with your mise.")),
        ));
    }
    // A package published for bun only (omp's starts with `#!/usr/bin/env bun`) does not run from mise's npm install.
    let npm_runnable = definition
        .package_managers
        .as_ref()
        .is_none_or(|managers| managers.iter().any(|manager| manager == "npm"));
    let tool = mise_installation
        .and_then(|installation| installation.tool.clone())
        .or_else(|| definition.mise_tool.clone())
        .or_else(|| {
            definition
                .npm_package
                .as_ref()
                .filter(|_| npm_runnable)
                .map(|package| format!("npm:{package}"))
        })?;
    let managed = mise_installation.is_some_and(|installation| installation.tool.is_some());
    let exists = |command: &str| crate::agent_hooks::probing::command_exists(command, home);
    let have_mise = exists("mise") || crate::managed_tools::mise::managed_executable().is_some();
    let mut companions =
        mise_companion_tools(definition, exists, super::latest::npm_latest_blocking);
    // A registry name (`gemini`) can be an `npm:` tool underneath; mise's registry says which.
    let backend = (have_mise && !exists("npm"))
        .then(|| super::mise::backend(&tool, home))
        .flatten();
    let needs_node = mise_needs_node(
        backend.as_deref().unwrap_or(&tool),
        &companions,
        exists("npm"),
    );
    let (command, companion_reason) = match &mut companions {
        Ok(tools) if tools.is_empty() && !needs_node => {
            (super::mise::command(&tool, managed), None)
        }
        // An agent that needs other tools installs and updates them together in one `mise use`.
        Ok(tools) => {
            if needs_node {
                tools.insert(0, MISE_NODE.to_string());
            }
            (super::mise::use_command(&tool, tools), None)
        }
        Err(reason) => (super::mise::use_command(&tool, &[]), Some(reason.clone())),
    };
    let command = match &definition.then_run {
        Some(then) => super::mise::then_run(&command, then),
        None => command,
    };
    let mut method = Method::new(
        "mise",
        "mise",
        command.clone(),
        note(format!("Runs {command} with your mise.")),
    );
    if have_mise {
        method.unavailable_reason = companion_reason.or_else(|| {
            (!managed)
                .then(|| super::mise::unavailable_reason(&tool, home))
                .flatten()
        });
    } else {
        // Offered only for a CLI mise already manages, so mise is there; Ghostex never downloads it for an agent.
        method.unavailable_reason =
            Some("Ghostex can't find mise on this computer's PATH. Open a new terminal and check that mise runs, then try again.".into());
    }
    Some(method)
}

/// The Node.js mise installs beside an `npm:` tool on a computer without npm.
const MISE_NODE: &str = "node@lts";

/// CDXC:ManagedTools 2026-10-28 WHY:
/// mise's `npm:` backend installs with npm and refuses to run without it ("requires configured install dependency node", tried on a computer with no Node.js). Asked for `node@lts` in the same `mise use`, mise installs Node.js first and its shims put that Node.js on PATH whenever the agent runs, so an agent installed through mise starts on a computer that never had Node.js.
fn mise_needs_node(tool: &str, companions: &Result<Vec<String>, String>, have_npm: bool) -> bool {
    !have_npm
        && (tool.starts_with("npm:")
            || companions
                .as_ref()
                .is_ok_and(|tools| tools.iter().any(|tool| tool.starts_with("npm:"))))
}

/// The versioned mise tools an agent's companions add to its `mise use`, or why they cannot be
/// named now (npm's registry did not answer).
fn mise_companion_tools(
    definition: &Definition,
    command_exists: impl Fn(&str) -> bool,
    npm_latest: impl Fn(&str) -> Option<String>,
) -> Result<Vec<String>, String> {
    definition
        .npm_companions
        .iter()
        .filter(|companion| {
            companion
                .unless_command
                .as_deref()
                .is_none_or(|command| !command_exists(command))
        })
        .map(|companion| {
            let version = if companion.pin_npm_latest {
                npm_latest(&companion.package).ok_or_else(|| {
                    format!(
                        "Ghostex could not read the current {} release from npm. Check your internet connection, then try again.",
                        companion.package
                    )
                })?
            } else {
                "latest".to_string()
            };
            Ok(format!("npm:{}@{version}", companion.package))
        })
        .collect()
}

/// What a package-manager install names after `-g`: the companions this computer still needs, then the
/// agent's own package, each at `@latest`.
fn npm_install_packages(
    definition: &Definition,
    package: &str,
    command_exists: impl Fn(&str) -> bool,
) -> String {
    definition
        .npm_companions
        .iter()
        .filter(|companion| {
            companion
                .unless_command
                .as_deref()
                .is_none_or(|command| !command_exists(command))
        })
        .map(|companion| companion.package.as_str())
        .chain(std::iter::once(package))
        .map(|package| format!("{package}@latest"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// CDXC:AgentProviders 2026-10-17 WHY:
/// OMP runs on Bun and refuses a Bun older than 1.3.14 (phase 0 hit 1.3.12 on Windows). Bun is not a tool Ghostex installs for the user, so like git outside Linux the bun method names what to update and how instead of failing halfway; the official installer always downloads OMP's own binary (`--binary`), which needs no Bun.
fn bun_too_old(binary: &str, minimum: &str, home: &Path) -> Option<String> {
    let bun = super::process::resolve("bun", home)?;
    let mut command = crate::platform::process::background_command(&bun);
    command.arg("--version").current_dir(home);
    let current = crate::agent_hooks::probing::run_command_stdout_with_timeout(
        command,
        std::time::Duration::from_secs(3),
    )
    .and_then(|text| super::latest::version_in(&text))?;
    super::latest::is_newer(minimum, &current).then(|| {
        format!("{binary} needs Bun {minimum} or newer, and this computer has Bun {current}. Run bun upgrade in a terminal, then try again, or use the official installer, which needs no Bun.")
    })
}

/// The CLI's own updater, offered when Ghostex cannot tell how the CLI was installed.
pub(crate) fn self_method(definition: &Definition) -> Option<Method> {
    let command = definition.self_updater()?;
    Some(Method::new(
        "self",
        "Built-in updater",
        command.into(),
        format!("Runs {command}, the CLI's own updater, which updates it the way it was installed. No password needed."),
    ))
}

/// What the official installer needs on this computer, and what Ghostex can install for it.
fn apply_native_requirements(method: &mut Method, native: &Native, command: &str, home: &Path) {
    let mut wanted: Vec<&str> = Vec::new();
    if command.starts_with("curl ") {
        wanted.extend(["curl", "ca-certificates"]);
    }
    let extra = if cfg!(windows) {
        &native.requires.windows
    } else {
        &native.requires.unix
    };
    wanted.extend(extra.iter().map(String::as_str));
    if native.needs_applications_folder && cfg!(target_os = "macos") && !applications_writable() {
        method.unavailable_reason = Some("This installer copies an app into /Applications, which this account can't change. Sign in as an administrator, or ask one to install it.".into());
        return;
    }
    if cfg!(target_os = "linux") {
        let missing = crate::managed_tools::system_tools::missing(home, &wanted);
        if missing.is_empty() {
            return;
        }
        let list = missing
            .iter()
            .map(|name| {
                if *name == "ca-certificates" {
                    "certificates"
                } else {
                    name
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        if crate::managed_tools::system_tools::can_install_in_background() {
            method.prerequisite = Some("systemTools".into());
            method.system_tools = missing.iter().map(|name| name.to_string()).collect();
            method.plan = format!(
                "Ghostex first installs {list} with your package manager (your computer asks for your password once), then {}",
                lowercase_first(&method.plan)
            );
        } else {
            method.unavailable_reason = Some(format!(
                "This installer needs {list}. Use Install system tools in Settings > Integrations > Tools (it opens a terminal that asks for your password), then try again."
            ));
        }
        return;
    }
    for &tool in &wanted {
        if tool == "ca-certificates" || crate::agent_hooks::probing::command_exists(tool, home) {
            continue;
        }
        // CDXC:ManagedTools 2026-09-29 DECISION:
        // User (8B): when git is missing outside Linux (Hermes on a Mac without Apple's Command Line Tools, OpenCode on Windows without Git for Windows; Claude Code no longer needs Git for Windows to install since 2026-11-12, as its docs say), show a clear "install git first" message instead of installing it.
        method.unavailable_reason = Some(match (tool, cfg!(target_os = "macos")) {
            ("git", true) => "This agent needs git. Install Apple's Command Line Tools first (run xcode-select --install in a terminal), then try again.".into(),
            ("git", false) => "This agent needs Git for Windows. Install it from git-scm.com/downloads/win, then try again.".into(),
            _ => format!("This installer needs {tool}. Install it first, then try again."),
        });
        return;
    }
    if cfg!(target_os = "macos") && wanted.contains(&"git") && !mac_git_usable() {
        method.unavailable_reason = Some("This agent needs git. Install Apple's Command Line Tools first (run xcode-select --install in a terminal), then try again.".into());
    }
}

fn lowercase_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(unix)]
fn applications_writable() -> bool {
    let path = std::ffi::CString::new("/Applications").expect("static path");
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

#[cfg(not(unix))]
fn applications_writable() -> bool {
    true
}

/// `/usr/bin/git` on a Mac is a stub that opens Apple's installer dialog until the Command Line
/// Tools (or Xcode) are installed; a git from Homebrew works either way.
fn mac_git_usable() -> bool {
    std::process::Command::new("/usr/bin/xcode-select")
        .arg("-p")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
        || ["/opt/homebrew/bin/git", "/usr/local/bin/git"]
            .iter()
            .any(|path| Path::new(path).is_file())
}

/// Prerequisites (npm, curl, brew, winget, mise) come from the shared 60 second probe cache: a list of every
/// agent asks for the same few tools many times over.
fn missing_command(command: &str, home: &Path) -> Option<String> {
    (!crate::agent_hooks::probing::command_exists(command, home))
        .then(|| format!("Install {command} on this computer first."))
}

fn brew_package(path: &str) -> Option<String> {
    let path = std::fs::canonicalize(path)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    let (_, rest) = path
        .split_once("/caskroom/")
        .or_else(|| path.split_once("/cellar/"))?;
    let name = rest.split('/').next()?;
    (!name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@._-+".contains(&byte)))
    .then(|| name.to_string())
}

/// CDXC:AgentProviders 2026-09-14 WHY:
/// A global npm install can leave a second CLI behind a Homebrew or native binary on PATH.
/// Resolve symlinks before choosing an updater. An unknown installation never gets a package manager's updater (that installs a second copy); since 2026-10-17 it updates through the CLI's own updater when it has one (`copilot update`, `pi update self`, see `Definition::self_updater`), which knows how it was installed, and only a CLI without one needs an explicit method selection.
pub(crate) fn detected_method(path: &str, definition: &Definition) -> Option<String> {
    if super::mise::looks_managed(path) {
        // Unresolved shims must not offer a native self-updater that would modify mise's installation.
        return Some("manual".into());
    }
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.into());
    let normalized = real.to_string_lossy().replace('\\', "/").to_lowercase();
    let original = path.replace('\\', "/").to_lowercase();
    if normalized.contains("/winget/") || original.contains("/winget/") {
        return Some("winget".into());
    }
    if normalized.contains("/scoop/")
        || original.contains("/scoop/")
        || normalized.contains("/nix/store/")
    {
        return Some("manual".into());
    }
    if normalized.contains("/cellar/") || normalized.contains("/caskroom/") {
        return definition.brew_formula.as_ref().map(|_| "brew".into());
    }
    if definition.npm_package.is_some() {
        // On Windows npm leaves only a `.cmd` shim beside Ghostex's node.exe, which never resolves
        // into node_modules.
        if Path::new(path).starts_with(crate::managed_tools::paths::node_dir()) {
            return Some("npm".into());
        }
        if original.contains("/.bun/") || normalized.contains("/.bun/") {
            return Some("bun".into());
        }
        if original.contains("/pnpm/") || normalized.contains("/pnpm/") {
            return Some("pnpm".into());
        }
        if normalized.contains("/node_modules/")
            || (cfg!(windows) && original.contains("/appdata/roaming/npm/"))
        {
            return Some("npm".into());
        }
    }
    if let Some(native) = &definition.native {
        if native.path_markers.iter().any(|marker| {
            normalized.contains(marker.as_str()) || original.contains(marker.as_str())
        }) {
            return Some("native".into());
        }
    }
    if definition.native.is_some()
        && [
            "/.local/",
            "/.claude/",
            "/.cursor/",
            "/.grok/",
            "/.factory/",
            "/.amp/",
            "/.opencode/",
            "/.hermes/",
            "/.kiro/",
            "/agy/",
        ]
        .iter()
        .any(|part| normalized.contains(part) || original.contains(part))
    {
        return Some("native".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dsh() -> &'static Definition {
        CATALOG
            .iter()
            .find(|definition| definition.agent_id == "dsh")
            .expect("DeepSeek Harness is in the bundled catalog")
    }

    #[test]
    fn deepseek_harness_installs_with_npm_then_updates_its_profile() {
        let definition = dsh();
        assert_eq!(definition.binary, "dsh-tui");
        let home = std::env::temp_dir();
        let methods = methods(definition, None, &home, None);
        assert_eq!(
            methods
                .iter()
                .map(|method| method.id.as_str())
                .collect::<Vec<_>>(),
            ["npm"]
        );
        assert!(methods[0]
            .command
            .starts_with("npm install -g @deepseek-ai/dsh@latest "));
        assert!(methods[0]
            .command
            .contains(" @deepseek-harness-tui/dsh-tui@latest && dsh-tui update"));
        assert_eq!(definition.self_updater(), Some("dsh-tui update"));
    }

    #[test]
    fn the_dsh_cli_follows_npm_latest_and_pnpm_joins_only_when_missing() {
        let definition = dsh();
        let tag = |package: &str| (package == "@deepseek-ai/dsh").then(|| "0.2.1-rc.2".to_string());
        assert_eq!(
            mise_companion_tools(definition, |_| false, tag).unwrap(),
            ["npm:@deepseek-ai/dsh@0.2.1-rc.2", "npm:pnpm@latest"]
        );
        assert_eq!(
            mise_companion_tools(definition, |command| command == "pnpm", tag).unwrap(),
            ["npm:@deepseek-ai/dsh@0.2.1-rc.2"]
        );
        let offline = mise_companion_tools(definition, |_| true, |_| None).unwrap_err();
        assert!(offline.contains("could not read the current @deepseek-ai/dsh release"));
        assert_eq!(
            super::super::mise::use_command(
                "npm:@deepseek-harness-tui/dsh-tui",
                &["npm:@deepseek-ai/dsh@0.2.1-rc.2".to_string()]
            ),
            "mise use --global --yes 'npm:@deepseek-harness-tui/dsh-tui@latest' 'npm:@deepseek-ai/dsh@0.2.1-rc.2'"
        );
    }

    #[test]
    fn an_npm_tool_brings_node_along_only_when_npm_is_missing() {
        let none: Result<Vec<String>, String> = Ok(Vec::new());
        assert!(mise_needs_node("npm:@google/gemini-cli", &none, false));
        assert!(!mise_needs_node("npm:@google/gemini-cli", &none, true));
        assert!(!mise_needs_node("claude", &none, false));
        let dsh_companions: Result<Vec<String>, String> =
            Ok(vec!["npm:@deepseek-ai/dsh@0.2.1-rc.2".to_string()]);
        assert!(mise_needs_node("aqua:x/y", &dsh_companions, false));
    }

    /// Every catalog agent, in both cases: a CLI mise does not manage installs with its official
    /// method and is never offered mise; a CLI mise manages updates through mise. Run with
    /// `--nocapture` for the table.
    #[test]
    fn mise_only_for_a_cli_already_on_mise() {
        let home = std::env::temp_dir();
        let ids = |methods: &[Method]| {
            methods
                .iter()
                .map(|method| method.id.clone())
                .collect::<Vec<_>>()
                .join(",")
        };
        // The Windows-only parts, which this computer cannot run: Claude Code's installer needs no
        // Git for Windows, and Pi's runs PowerShell's install.ps1.
        let native = |id: &str| {
            CATALOG
                .iter()
                .find(|definition| definition.agent_id == id)
                .and_then(|definition| definition.native.as_ref())
                .unwrap()
        };
        assert!(native("claude").requires.windows.is_empty());
        assert_eq!(
            native("pi").windows_install.as_deref(),
            Some("irm https://pi.dev/install.ps1 | iex")
        );
        for definition in CATALOG.iter() {
            let off = methods(definition, None, &home, None);
            assert!(
                off.iter()
                    .all(|method| method.id != "mise"
                        && method.prerequisite.as_deref() != Some("mise")),
                "{} offers mise without being on mise: {}",
                definition.agent_id,
                ids(&off)
            );
            let tool = definition
                .mise_tool
                .clone()
                .unwrap_or_else(|| format!("npm:{}", definition.binary));
            let fake = format!(
                "/home/me/.local/share/mise/installs/x/1.0.0/bin/{}",
                definition.binary
            );
            let on_tool = super::super::mise::Installation {
                tool: Some(tool),
                executable: fake.clone(),
                runtime: None,
            };
            let on = methods(definition, Some(&fake), &home, Some(&on_tool));
            assert_eq!(
                on[0].id,
                "mise",
                "{} on mise: {}",
                definition.agent_id,
                ids(&on)
            );
            let in_node = super::super::mise::Installation {
                tool: None,
                executable: fake.clone(),
                runtime: Some("node@24.11.0".into()),
            };
            let runtime = methods(definition, Some(&fake), &home, Some(&in_node));
            let runtime_first = runtime.first().map(|method| method.command.clone());
            // Installed by its own method (the official installer's folder, or npm's), not on mise.
            let own = home.join(".pi-check").join(&definition.binary);
            let own = own.to_string_lossy().into_owned();
            let elsewhere = methods(definition, Some(&own), &home, None);
            assert!(elsewhere.iter().all(|method| method.id != "mise"));
            if definition.agent_id == "antigravity" {
                let native = elsewhere.iter().find(|m| m.id == "native").unwrap();
                assert!(native.command.is_empty());
                assert!(native
                    .unavailable_reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains("updates itself")));
            }
            println!(
                "{:<14} not on mise: {:<28} default {:?}\n{:<14} mise tool:   {:<28} default {:?}\n{:<14} mise node:   {:<28} default {:?}",
                definition.agent_id,
                ids(&off),
                off.iter().find(|m| m.unavailable_reason.is_none()).or(off.first()).map(|m| &m.command),
                "",
                ids(&on),
                on[0].command,
                "",
                ids(&runtime),
                runtime_first,
            );
            println!(
                "{:<14} installed elsewhere: {}",
                "",
                elsewhere
                    .iter()
                    .map(|m| match &m.unavailable_reason {
                        Some(reason) => format!("{} [{}] ({reason})", m.id, m.command),
                        None => format!("{} [{}]", m.id, m.command),
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }
    }

    #[test]
    fn npm_companions_join_a_package_manager_install_unless_already_present() {
        let definition = dsh();
        assert_eq!(
            npm_install_packages(definition, "@deepseek-harness-tui/dsh-tui", |_| false),
            "@deepseek-ai/dsh@latest pnpm@latest @deepseek-harness-tui/dsh-tui@latest"
        );
        assert_eq!(
            npm_install_packages(definition, "@deepseek-harness-tui/dsh-tui", |command| {
                command == "pnpm"
            }),
            "@deepseek-ai/dsh@latest @deepseek-harness-tui/dsh-tui@latest"
        );
        let freebuff = CATALOG
            .iter()
            .find(|definition| definition.agent_id == "freebuff")
            .unwrap();
        assert_eq!(
            npm_install_packages(freebuff, "freebuff", |_| false),
            "freebuff@latest"
        );
    }
}
