use crate::platform::shell::shell_quote;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub(super) fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("").trim()
}
#[derive(Debug)]
pub(super) struct SetupRequired(pub String);
impl std::fmt::Display for SetupRequired {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for SetupRequired {}
#[derive(Clone)]
pub(super) struct Plan {
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub command: String,
    pub url: String,
    pub report: Option<(PathBuf, String)>,
    pub timeout: u64,
}
impl Plan {
    pub fn value(&self) -> Value {
        json!({"command": self.command, "cwd": self.cwd, "url": self.url})
    }
}

pub(super) fn resolve(params: &Value) -> Result<Plan> {
    let root = fs::canonicalize(text(params, "projectPath"))
        .context("The project directory is unavailable.")?;
    if !root.is_dir() {
        bail!("The project path is not a directory.");
    }
    let view = &params["view"];
    let source = &view["source"];
    let binding = &params["binding"];
    let directory = if text(binding, "cwd").is_empty() {
        text(source, "cwd")
    } else {
        text(binding, "cwd")
    };
    let cwd = root.join(if directory.is_empty() { "." } else { directory });
    let cwd = fs::canonicalize(cwd).context("The configured working directory does not exist.")?;
    if !cwd.is_dir() || !cwd.starts_with(&root) {
        bail!("Working directory must be inside this checkout.");
    }
    let mut plan = Plan {
        root: root.clone(),
        cwd,
        command: String::new(),
        url: String::new(),
        report: None,
        timeout: source["timeoutSeconds"]
            .as_u64()
            .unwrap_or(60)
            .clamp(5, 600),
    };
    match text(source, "kind") {
        "" | "website" => {
            let destination = text(source, "destination");
            let url = match destination {
                "github-issues" | "github-actions" | "github-pulls" => {
                    let override_url = text(binding, "repositoryUrl");
                    let remote = if override_url.is_empty() {
                        let origin = text(params, "repositoryOriginUrl");
                        if origin.is_empty() {
                            bail!("This project's GitHub repository is not available yet. Set a repository URL in Projects settings if it has no origin remote.");
                        }
                        origin.to_string()
                    } else {
                        override_url.to_string()
                    };
                    let repository = github_url(&remote)?;
                    format!(
                        "{repository}/{}",
                        match destination {
                            "github-actions" => "actions",
                            "github-pulls" => "pulls",
                            _ => "issues",
                        }
                    )
                }
                _ => {
                    let project_url = text(binding, "url");
                    if !project_url.is_empty() {
                        project_url.to_string()
                    } else if destination == "project" {
                        bail!("Set this project's URL in Settings → Projects.");
                    } else {
                        text(view, "url").to_string()
                    }
                }
            };
            plan.url = http_url(&url)?.to_string();
        }
        "dev-server" | "report" => {
            plan.command = if text(binding, "command").is_empty() {
                text(source, "command")
            } else {
                text(binding, "command")
            }
            .to_string();
            if text(source, "kind") == "report" {
                let directory = text(source, "reportDirectory");
                if directory.is_empty() {
                    bail!("Set an HTML report directory in the view settings.");
                }
                let relative =
                    crate::extensions::serve::decode_relative_path(directory).map_err(|_| {
                        anyhow::anyhow!("Report directory must be relative to the checkout.")
                    })?;
                if Path::new(directory).is_absolute() {
                    bail!("Report directory must be relative to the checkout.");
                }
                let entry = text(source, "entry");
                let entry = if entry.is_empty() {
                    "index.html"
                } else {
                    entry
                };
                crate::extensions::serve::decode_relative_path(entry)
                    .map_err(|_| anyhow::anyhow!("Invalid report entry page."))?;
                if Path::new(entry).is_absolute() {
                    bail!("Report entry must be relative.");
                }
                plan.report = Some((root.join(relative), entry.to_string()));
            } else {
                let mut script = String::new();
                if plan.command.is_empty() && text(source, "discovery") == "storybook" {
                    let candidates = discover_storybook(&plan.cwd)?;
                    if candidates.len() != 1 {
                        let message = format!("Found {} Storybook scripts. Set a command and package directory in Projects settings.{}", candidates.len(), candidates.iter().map(|c| format!("\n{}: {}", c.0.display(), c.1)).collect::<String>());
                        if candidates.len() > 1 {
                            return Err(SetupRequired(message).into());
                        }
                        bail!(message);
                    }
                    let (cwd, command, detected_script) = candidates.into_iter().next().unwrap();
                    plan.cwd = cwd;
                    plan.command = command;
                    script = detected_script;
                }
                if plan.command.is_empty() {
                    bail!("Configure a command for this view in Extensions or Projects settings.");
                }
                plan.url = if text(binding, "readinessUrl").is_empty() {
                    text(source, "readinessUrl")
                } else {
                    text(binding, "readinessUrl")
                }
                .to_string();
                if plan.url.is_empty() && !script.is_empty() {
                    let words = script.split_whitespace().collect::<Vec<_>>();
                    let mut port = "6006";
                    for (i, word) in words.iter().enumerate() {
                        if *word == "-p" || *word == "--port" {
                            port = words.get(i + 1).copied().unwrap_or("");
                        }
                        if let Some(value) = word.strip_prefix("--port=") {
                            port = value;
                        }
                    }
                    let port: u16 = port.parse().context(
                        "Set a Ready URL for this script's port in Extensions settings.",
                    )?;
                    plan.url = format!("http://127.0.0.1:{port}/");
                }
                let url = http_url(&plan.url.replace("{port}", "49152"))?;
                if !matches!(
                    url.host_str(),
                    Some("127.0.0.1" | "localhost" | "::1" | "[::1]")
                ) {
                    bail!("A managed dev server's Ready URL must use localhost.");
                }
                if plan.command.contains("{port}") != plan.url.contains("{port}") {
                    bail!(
                        "Use {port} in both the command and Ready URL.",
                        port = "{port}"
                    );
                }
            }
        }
        _ => bail!("Unknown project view source."),
    }
    Ok(plan)
}
fn http_url(value: &str) -> Result<url::Url> {
    let url = url::Url::parse(value).context("Enter a complete HTTP or HTTPS URL.")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("Enter an HTTP or HTTPS URL without embedded credentials.");
    }
    Ok(url)
}
fn github_url(remote: &str) -> Result<String> {
    let remote = if let Some(path) = remote.strip_prefix("git@github.com:") {
        format!("https://github.com/{path}")
    } else {
        remote.to_string()
    };
    let url = url::Url::parse(&remote)
        .context("This project does not have a supported GitHub remote.")?;
    if url.host_str() != Some("github.com") {
        bail!("Set a github.com repository URL in Projects settings.");
    }
    let segments = url
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>();
    if segments.len() < 2 {
        bail!("The GitHub remote is missing its owner or repository.");
    }
    Ok(format!(
        "https://github.com/{}/{}",
        segments[0],
        segments[1].trim_end_matches(".git")
    ))
}

/// CDXC:Extensions 2026-09-09 WHY:
/// Ghostex's Storybook config is nested and its package script first generates CSS, so discovery preserves the script entry point and never executes configuration files.
fn discover_storybook(root: &Path) -> Result<Vec<(PathBuf, String, String)>> {
    let package = read_package(root)?;
    let manager = package_manager(root, &package);
    let direct = scripts(root, &package, &manager);
    if !direct.is_empty() {
        return Ok(direct);
    }
    let workspace_value = &package["workspaces"];
    let workspace_patterns = workspace_value
        .as_array()
        .or_else(|| workspace_value["packages"].as_array());
    let mut patterns = workspace_patterns
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Ok(yaml) = fs::read_to_string(root.join("pnpm-workspace.yaml")) {
        for line in yaml.lines() {
            if let Some(value) = line.trim().strip_prefix("- ") {
                patterns.push(value.trim_matches(['\'', '"']).to_string());
            }
        }
    }
    let mut directories = vec![root.to_path_buf()];
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    while let Some(dir) = directories.pop() {
        if seen.len() >= 4096 {
            bail!("Workspace discovery limit reached. Select a package directory in Projects settings.");
        }
        if !seen.insert(dir.clone()) {
            continue;
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.')
                || [
                    "node_modules",
                    "target",
                    "dist",
                    "build",
                    "out",
                    "coverage",
                    "vendor",
                    "storybook-static",
                ]
                .contains(&name.as_str())
            {
                continue;
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            if relative.split('/').count() > 8 {
                continue;
            }
            if patterns
                .iter()
                .any(|pattern| glob_matches(pattern, &relative))
                && path.join("package.json").is_file()
            {
                let child = read_package(&path)?;
                found.extend(scripts(&path, &child, &manager));
            }
            if !patterns.is_empty() {
                directories.push(path);
            }
        }
    }
    found.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    Ok(found)
}
fn read_package(path: &Path) -> Result<Value> {
    let bytes = fs::read(path.join("package.json"))
        .context("No package.json found. Set a project command or package directory.")?;
    if bytes.len() > 1024 * 1024 {
        bail!("package.json is too large for discovery.");
    }
    serde_json::from_slice(&bytes).context("Could not parse package.json.")
}
fn package_manager(root: &Path, package: &Value) -> String {
    let declared = text(package, "packageManager")
        .split('@')
        .next()
        .unwrap_or("");
    if ["bun", "npm", "pnpm", "yarn"].contains(&declared) {
        return declared.into();
    }
    for (file, manager) in [
        ("bun.lock", "bun"),
        ("bun.lockb", "bun"),
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("package-lock.json", "npm"),
    ] {
        if root.join(file).exists() {
            return manager.into();
        }
    }
    "npm".into()
}
fn scripts(root: &Path, package: &Value, manager: &str) -> Vec<(PathBuf, String, String)> {
    package["scripts"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, value)| {
            let command = value.as_str()?;
            if name.starts_with("build") || command.contains("storybook build") {
                return None;
            }
            if name != "storybook"
                && !command.contains("storybook dev")
                && !command.contains("start-storybook")
            {
                return None;
            }
            Some((
                root.to_path_buf(),
                format!("{manager} run {}", shell_quote(name)),
                command.to_string(),
            ))
        })
        .collect()
}
fn glob_matches(pattern: &str, value: &str) -> bool {
    fn go(p: &[u8], v: &[u8]) -> bool {
        if p.is_empty() {
            return v.is_empty();
        }
        if p.starts_with(b"**") {
            return go(&p[2..], v)
                || (p.get(2) == Some(&b'/') && go(&p[3..], v))
                || (!v.is_empty() && go(p, &v[1..]));
        }
        if p[0] == b'*' {
            return go(&p[1..], v) || (!v.is_empty() && v[0] != b'/' && go(p, &v[1..]));
        }
        !v.is_empty() && p[0] == v[0] && go(&p[1..], &v[1..])
    }
    go(pattern.as_bytes(), value.as_bytes())
}
