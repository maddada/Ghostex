// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the "Open In" target catalog: built-in
// target definitions, custom/hidden/availability resolution from Settings,
// and target launch dispatch. See docs/2026-08-22/repo-restructure/SPLITS.md
// C1.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_open_path(path: &Path) -> Result<(), String> {
    gpui_spawn_os_open(path.as_os_str())
}

pub(crate) const GPUI_CUSTOM_OPEN_TARGET_ID_PREFIX: &str = "custom:";

pub(crate) struct GpuiBuiltInOpenTargetDefinition {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) commands: &'static [&'static str],
    pub(crate) base_args: &'static [&'static str],
    // Detection probe names mirroring macOSAppNames in
    // packages/shared/workspace-open-targets.ts; keep both catalogs in sync.
    pub(crate) macos_app_names: &'static [&'static str],
}

pub(crate) const GPUI_BUILT_IN_OPEN_TARGETS: &[GpuiBuiltInOpenTargetDefinition] = &[
    GpuiBuiltInOpenTargetDefinition {
        id: "cursor",
        label: "Cursor",
        commands: &["cursor"],
        base_args: &[],
        macos_app_names: &["Cursor"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "trae",
        label: "Trae",
        commands: &["trae"],
        base_args: &[],
        macos_app_names: &["Trae"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "kiro",
        label: "Kiro",
        commands: &["kiro"],
        base_args: &["ide"],
        macos_app_names: &["Kiro"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "vscode",
        label: "VS Code",
        commands: &["code"],
        base_args: &[],
        macos_app_names: &["Visual Studio Code"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "vscode-insiders",
        label: "VS Code Insiders",
        commands: &["code-insiders"],
        base_args: &[],
        macos_app_names: &["Visual Studio Code - Insiders"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "vscodium",
        label: "VSCodium",
        commands: &["codium"],
        base_args: &[],
        macos_app_names: &["VSCodium"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "zed",
        label: "Zed",
        commands: &["zed", "zeditor"],
        base_args: &[],
        macos_app_names: &["Zed", "Zed Preview"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "antigravity",
        label: "Antigravity",
        commands: &["agy-ide"],
        base_args: &[],
        macos_app_names: &["Antigravity"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "idea",
        label: "IntelliJ IDEA",
        commands: &["idea"],
        base_args: &[],
        macos_app_names: &["IntelliJ IDEA"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "aqua",
        label: "Aqua",
        commands: &["aqua"],
        base_args: &[],
        macos_app_names: &["Aqua"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "clion",
        label: "CLion",
        commands: &["clion"],
        base_args: &[],
        macos_app_names: &["CLion"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "datagrip",
        label: "DataGrip",
        commands: &["datagrip"],
        base_args: &[],
        macos_app_names: &["DataGrip"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "dataspell",
        label: "DataSpell",
        commands: &["dataspell"],
        base_args: &[],
        macos_app_names: &["DataSpell"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "goland",
        label: "GoLand",
        commands: &["goland"],
        base_args: &[],
        macos_app_names: &["GoLand"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "phpstorm",
        label: "PhpStorm",
        commands: &["phpstorm"],
        base_args: &[],
        macos_app_names: &["PhpStorm"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "pycharm",
        label: "PyCharm",
        commands: &["pycharm"],
        base_args: &[],
        macos_app_names: &["PyCharm"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "rider",
        label: "Rider",
        commands: &["rider"],
        base_args: &[],
        macos_app_names: &["Rider"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "rubymine",
        label: "RubyMine",
        commands: &["rubymine"],
        base_args: &[],
        macos_app_names: &["RubyMine"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "rustrover",
        label: "RustRover",
        commands: &["rustrover"],
        base_args: &[],
        macos_app_names: &["RustRover"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "webstorm",
        label: "WebStorm",
        commands: &["webstorm"],
        base_args: &[],
        macos_app_names: &["WebStorm"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "finder",
        label: "Open Folder",
        commands: &[],
        base_args: &[],
        macos_app_names: &[],
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiOpenTarget {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) launch: GpuiOpenTargetLaunch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiOpenTargetLaunch {
    Finder,
    BuiltIn {
        default_command: Option<&'static str>,
        base_args: &'static [&'static str],
        resolved_command: Option<String>,
        resolved_app_name: Option<String>,
    },
    Custom {
        command: String,
        args: Vec<String>,
    },
}

pub(crate) struct GpuiOpenTargetAvailability {
    pub(crate) available_ids: HashSet<String>,
    pub(crate) resolved_commands: HashMap<String, String>,
    pub(crate) resolved_app_names: HashMap<String, String>,
}

pub(crate) fn gpui_visible_open_targets_from_current_settings() -> Vec<GpuiOpenTarget> {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    gpui_visible_open_targets_from_settings(settings.object())
}

pub(crate) fn gpui_visible_open_targets_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> Vec<GpuiOpenTarget> {
    /*
    CDXC:GPUITitlebarOpenIn 2026-06-24-12:50:
    GPUI titlebar Open In consumes the same shared Settings fields as React: hidden built-in ids, availability resolved ids/commands/app names, and normalized custom targets. Finder/Open Folder remains always available unless hidden, custom targets follow built-ins, and no project path, command text, URL, stdout/stderr, or user content is logged or persisted here.
    */
    let hidden_ids = gpui_open_target_hidden_ids(settings.get("workspaceOpenTargetHiddenIds"));
    let availability =
        gpui_open_target_availability(settings.get("workspaceOpenTargetAvailability"));
    let mut targets = Vec::new();
    for definition in GPUI_BUILT_IN_OPEN_TARGETS {
        if hidden_ids.contains(definition.id) {
            continue;
        }
        if definition.id != "finder" && !availability.available_ids.contains(definition.id) {
            continue;
        }
        targets.push(GpuiOpenTarget {
            id: definition.id.to_string(),
            label: definition.label.to_string(),
            launch: if definition.id == "finder" {
                GpuiOpenTargetLaunch::Finder
            } else {
                GpuiOpenTargetLaunch::BuiltIn {
                    default_command: definition.commands.first().copied(),
                    base_args: definition.base_args,
                    resolved_command: availability.resolved_commands.get(definition.id).cloned(),
                    resolved_app_name: availability.resolved_app_names.get(definition.id).cloned(),
                }
            },
        });
    }
    targets.extend(gpui_custom_open_targets(
        settings.get("customWorkspaceOpenTargets"),
    ));
    targets
}

pub(crate) fn gpui_open_target_hidden_ids(
    candidate: Option<&serde_json::Value>,
) -> HashSet<String> {
    let built_in_ids = gpui_built_in_open_target_ids();
    candidate
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|id| built_in_ids.contains(*id))
        .map(str::to_string)
        .collect()
}

pub(crate) fn gpui_open_target_availability(
    candidate: Option<&serde_json::Value>,
) -> GpuiOpenTargetAvailability {
    let built_in_ids = gpui_built_in_open_target_ids();
    let object = candidate.and_then(serde_json::Value::as_object);
    let mut available_ids = HashSet::from(["finder".to_string()]);
    if let Some(ids) = object
        .and_then(|object| object.get("availableTargetIds"))
        .and_then(serde_json::Value::as_array)
    {
        for id in ids
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| built_in_ids.contains(*id))
        {
            available_ids.insert(id.to_string());
        }
    }
    let resolved_commands = gpui_open_target_resolution_map(
        object.and_then(|object| object.get("resolvedCommands")),
        &available_ids,
    );
    let resolved_app_names = gpui_open_target_resolution_map(
        object.and_then(|object| object.get("resolvedAppNames")),
        &available_ids,
    );
    GpuiOpenTargetAvailability {
        available_ids,
        resolved_commands,
        resolved_app_names,
    }
}

pub(crate) fn gpui_open_target_resolution_map(
    candidate: Option<&serde_json::Value>,
    available_ids: &HashSet<String>,
) -> HashMap<String, String> {
    candidate
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|object| object.iter())
        .filter_map(|(target_id, value)| {
            let value = value.as_str()?.trim();
            (available_ids.contains(target_id.as_str()) && !value.is_empty())
                .then(|| (target_id.clone(), value.to_string()))
        })
        .collect()
}

pub(crate) fn gpui_custom_open_targets(
    candidate: Option<&serde_json::Value>,
) -> Vec<GpuiOpenTarget> {
    let mut seen_ids = HashSet::new();
    let mut targets = Vec::new();
    let Some(entries) = candidate.and_then(serde_json::Value::as_array) else {
        return targets;
    };
    for entry in entries {
        let Some(object) = entry.as_object() else {
            continue;
        };
        let label = object
            .get("label")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let command = object
            .get("command")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if label.is_empty() || command.is_empty() {
            continue;
        }
        let requested_id = object
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let base_id = if requested_id.starts_with(GPUI_CUSTOM_OPEN_TARGET_ID_PREFIX) {
            requested_id.to_string()
        } else {
            format!(
                "{}{}",
                GPUI_CUSTOM_OPEN_TARGET_ID_PREFIX,
                gpui_open_target_slug(label)
            )
        };
        let mut id = base_id.clone();
        for suffix in 2.. {
            if !seen_ids.contains(&id) {
                break;
            }
            id = format!("{base_id}-{suffix}");
        }
        if !seen_ids.insert(id.clone()) {
            continue;
        }
        let args = object
            .get("args")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect();
        targets.push(GpuiOpenTarget {
            id,
            label: label.to_string(),
            launch: GpuiOpenTargetLaunch::Custom {
                command: command.to_string(),
                args,
            },
        });
    }
    targets
}

pub(crate) fn gpui_built_in_open_target_ids() -> HashSet<&'static str> {
    GPUI_BUILT_IN_OPEN_TARGETS
        .iter()
        .map(|target| target.id)
        .collect()
}

pub(crate) fn gpui_open_target_slug(label: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for character in label.trim().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "target".to_string()
    } else {
        slug.to_string()
    }
}

pub(crate) fn gpui_launch_open_target(
    target: &GpuiOpenTarget,
    project_path: &Path,
) -> Result<(), String> {
    /*
    CDXC:GPUITitlebarOpenIn 2026-06-24-12:50:
    Open In launch is bounded native process behavior: Finder/Open Folder uses the fixed OS opener, command targets use `/usr/bin/env` argv without shell splitting, macOS app-name launches use `/usr/bin/open -a`, child stdio is suppressed, and user paths/commands/errors are not copied into notifications or logs.
    */
    match &target.launch {
        GpuiOpenTargetLaunch::Finder => gpui_open_path(project_path)
            .map_err(|_| "Could not open the active project folder.".to_string()),
        GpuiOpenTargetLaunch::BuiltIn {
            default_command,
            base_args,
            resolved_command,
            resolved_app_name,
        } => {
            if let Some(command) = resolved_command
                .as_deref()
                .filter(|command| !command.trim().is_empty())
            {
                return gpui_spawn_open_target_command(command, *base_args, &[], project_path);
            }
            if let Some(app_name) = resolved_app_name
                .as_deref()
                .filter(|app_name| !app_name.trim().is_empty())
            {
                return gpui_spawn_open_target_app_name(app_name, project_path);
            }
            if let Some(command) = *default_command {
                return gpui_spawn_open_target_command(command, *base_args, &[], project_path);
            }
            Err("Could not launch the selected Open In target.".to_string())
        }
        GpuiOpenTargetLaunch::Custom { command, args } => {
            gpui_spawn_open_target_command(command, &[], args, project_path)
        }
    }
}
