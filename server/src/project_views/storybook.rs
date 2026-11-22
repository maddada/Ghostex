use super::resolve::{discover_packages, Plan, SetupRequired};
use crate::platform::shell::shell_quote;
use anyhow::{bail, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// CDXC:Workarea 2026-09-22 DECISION:
/// User: Storybook is a built-in view, shown only for projects with Storybook, without the original extension's persistent Node server. Build the project's static workshop and let gxserver serve it after the build process exits.
pub(super) fn resolve(mut plan: Plan) -> Result<Plan> {
    let mut packages = discover_packages(&plan.root, candidate)?;
    if packages.is_empty() {
        bail!("This project does not have Storybook.");
    }
    if packages.len() > 1 {
        return Err(SetupRequired(
            "This workspace has several Storybooks. Open the package you want as a project, or add a root build:storybook script that builds one workshop.".into(),
        ).into());
    }
    let (cwd, command, _) = packages.remove(0);
    if command.is_empty() {
        return Err(SetupRequired(
            "Add a build:storybook script to this project's package.json (for example, storybook build), then choose Rebuild. Include --config-dir if your Storybook configuration is outside .storybook.".into(),
        ).into());
    }
    // A fresh owned directory avoids clearing an existing build or serving half of a rebuild.
    let output = plan
        .root
        .join("node_modules")
        .join(".cache")
        .join("ghostex-storybook")
        .join(uuid::Uuid::new_v4().to_string());
    let mut ancestor = output.as_path();
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid Storybook output directory."))?;
    }
    if !std::fs::canonicalize(ancestor)?.starts_with(&plan.root) {
        bail!("Storybook output must stay inside this checkout.");
    }
    plan.cwd = cwd;
    plan.command = format!(
        "{command} --output-dir {}",
        shell_quote(&output.to_string_lossy())
    );
    plan.owned_output = Some(output.clone());
    plan.report = Some((output, "index.html".into()));
    plan.timeout = 600;
    Ok(plan)
}

fn candidate(root: &Path, package: &Value, manager: &str) -> Vec<(PathBuf, String, String)> {
    let scripts = package["scripts"].as_object();
    let build = scripts.and_then(|scripts| {
        ["build:storybook", "build-storybook", "storybook:build"]
            .into_iter()
            .find(|name| scripts.get(*name).and_then(Value::as_str).is_some())
            .or_else(|| {
                scripts.iter().find_map(|(name, value)| {
                    let command = value.as_str()?;
                    (command.contains("storybook build") || command.contains("build-storybook"))
                        .then_some(name.as_str())
                })
            })
    });
    let detected = build.is_some()
        || root.join(".storybook").is_dir()
        || ["dependencies", "devDependencies"].iter().any(|key| {
            package[key].as_object().is_some_and(|deps| {
                deps.keys()
                    .any(|name| name == "storybook" || name.starts_with("@storybook/"))
            })
        })
        || scripts.is_some_and(|scripts| {
            scripts.iter().any(|(name, value)| {
                name == "storybook"
                    || value.as_str().is_some_and(|command| {
                        command.contains("storybook dev") || command.contains("start-storybook")
                    })
            })
        });
    if !detected {
        return Vec::new();
    }
    let command = build
        .map(|name| {
            let separator = if manager == "npm" { " --" } else { "" };
            format!("{manager} run {}{separator}", shell_quote(name))
        })
        .unwrap_or_default();
    vec![(root.to_path_buf(), command, String::new())]
}
