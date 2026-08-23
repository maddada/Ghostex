// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the Agents Hub catalog builder: the
// profile/catalog-build item types, the `GpuiAgentsHubCatalogBuilder`
// struct/impl that assembles the catalog message, and the empty-catalog
// constructor. NOTE: `packages/shared/gpui-agents-hub-scanner-catalog.test.ts`
// reads this exact file's raw source and slices the region from
// `struct GpuiAgentsHubCatalogBuilder` through `fn
// gpui_empty_agents_hub_catalog_build` to regex-scan for catalog path
// literals -- keep that whole span in this one file.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::app::helpers::*;
use crate::*;

#[derive(Clone)]
pub(crate) struct GpuiAgentsHubProfileItem {
    pub(crate) agent_icon: &'static str,
    pub(crate) file_path: PathBuf,
    pub(crate) label: String,
    pub(crate) profile_path: PathBuf,
    pub(crate) target_path: Option<PathBuf>,
}

pub(crate) struct GpuiAgentsHubCatalogBuild {
    pub(crate) file_paths: HashSet<PathBuf>,
    pub(crate) message: serde_json::Value,
    pub(crate) open_paths: HashSet<PathBuf>,
}

pub(crate) struct GpuiAgentsHubCatalogBuilder {
    pub(crate) allowed_files: Vec<PathBuf>,
    pub(crate) allowed_roots: Vec<PathBuf>,
    pub(crate) file_paths: HashSet<PathBuf>,
    pub(crate) groups_by_tab: HashMap<&'static str, Vec<serde_json::Value>>,
    pub(crate) home: PathBuf,
    pub(crate) open_paths: HashSet<PathBuf>,
    pub(crate) seen_files: HashSet<PathBuf>,
}

impl GpuiAgentsHubCatalogBuilder {
    pub(crate) fn new(home: PathBuf) -> Self {
        let allowed_roots = gpui_agents_hub_allowed_roots(&home);
        let allowed_files = gpui_agents_hub_allowed_files(&home);
        Self {
            allowed_files,
            allowed_roots,
            file_paths: HashSet::new(),
            groups_by_tab: HashMap::new(),
            home,
            open_paths: HashSet::new(),
            seen_files: HashSet::new(),
        }
    }

    pub(crate) fn home_path(&self, parts: &[&str]) -> PathBuf {
        let mut path = self.home.clone();
        for part in parts {
            path.push(part);
        }
        path
    }

    pub(crate) fn add_group(
        &mut self,
        tab: &'static str,
        group_id: String,
        name: String,
        root_path: PathBuf,
        description: &'static str,
        files: Vec<PathBuf>,
        profiles: Vec<GpuiAgentsHubProfileItem>,
    ) {
        let resolved_files = files
            .into_iter()
            .filter_map(|candidate| self.file_item(&candidate, &root_path))
            .collect::<Vec<_>>();
        if resolved_files.is_empty() {
            return;
        }
        self.insert_open_path_if_allowed(&root_path);
        for profile in &profiles {
            self.insert_open_path_if_allowed(&profile.profile_path);
        }
        self.groups_by_tab.entry(tab).or_default().push(serde_json::json!({
            "description": description,
            "files": resolved_files,
            "id": group_id,
            "name": name,
            "path": gpui_path_string(&root_path),
            "profiles": profiles.into_iter().map(gpui_agents_hub_profile_json).collect::<Vec<_>>(),
        }));
    }

    pub(crate) fn file_item(&mut self, candidate_path: &Path, root_path: &Path) -> Option<serde_json::Value> {
        let resolved = self.valid_catalog_file(candidate_path)?;
        if self.seen_files.contains(&resolved) {
            return None;
        }
        self.seen_files.insert(resolved.clone());
        self.file_paths.insert(resolved.clone());
        if let Some(parent) = resolved.parent() {
            self.insert_open_path_if_allowed(parent);
        }
        let name = gpui_relative_path_name(&resolved, root_path)
            .unwrap_or_else(|| gpui_file_name_string(&resolved));
        Some(serde_json::json!({
            "id": gpui_agents_hub_file_id(&resolved),
            "language": gpui_agents_hub_language_for(&resolved),
            "name": name,
            "path": gpui_path_string(&resolved),
        }))
    }

    pub(crate) fn valid_catalog_file(&self, candidate_path: &Path) -> Option<PathBuf> {
        let resolved = fs::canonicalize(candidate_path).ok()?;
        if !self.is_allowed_path(&resolved) {
            return None;
        }
        let metadata = fs::metadata(&resolved).ok()?;
        if metadata.is_file() && metadata.len() <= GPUI_AGENTS_HUB_MAX_FILE_BYTES {
            Some(resolved)
        } else {
            None
        }
    }

    pub(crate) fn is_allowed_path(&self, resolved: &Path) -> bool {
        self.allowed_files.iter().any(|file| file == resolved)
            || self
                .allowed_roots
                .iter()
                .any(|root| gpui_path_is_relative_to(resolved, root))
    }

    pub(crate) fn insert_open_path_if_allowed(&mut self, candidate_path: &Path) {
        let Ok(resolved) = fs::canonicalize(candidate_path) else {
            return;
        };
        let Ok(metadata) = fs::metadata(&resolved) else {
            return;
        };
        if metadata.is_dir() && self.is_allowed_path(&resolved) {
            self.open_paths.insert(resolved);
        }
    }

    pub(crate) fn finish(mut self) -> GpuiAgentsHubCatalogBuild {
        for groups in self.groups_by_tab.values_mut() {
            groups.sort_by(|left, right| {
                let left_name = left
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let right_name = right
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                left_name.to_lowercase().cmp(&right_name.to_lowercase())
            });
        }
        let message = serde_json::json!({
            "generatedAt": gpui_status_generated_at(),
            "groupsByTab": {
                "configs": self.groups_by_tab.remove("configs").unwrap_or_default(),
                "hooks": self.groups_by_tab.remove("hooks").unwrap_or_default(),
                "mds": self.groups_by_tab.remove("mds").unwrap_or_default(),
                "skills": self.groups_by_tab.remove("skills").unwrap_or_default(),
            },
            "type": "agentsHubCatalog",
        });
        GpuiAgentsHubCatalogBuild {
            file_paths: self.file_paths,
            message,
            open_paths: self.open_paths,
        }
    }
}

pub(crate) fn gpui_agents_hub_catalog_message() -> serde_json::Value {
    gpui_agents_hub_catalog_build().message
}

pub(crate) fn gpui_agents_hub_catalog_build() -> GpuiAgentsHubCatalogBuild {
    /*
    CDXC:GPUIAgentsHubBridge 2026-06-24-12:26:
    GPUI Agents Hub scans the same machine-local agent/profile/skill/config roots as the macOS native helper, but catalog rows remain metadata-only. File bodies are deliberately omitted here so opening the Hub cannot bridge private instruction/config contents until the user selects a file.
    */
    let Some(home) = env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    else {
        return gpui_empty_agents_hub_catalog_build();
    };
    let mut builder = GpuiAgentsHubCatalogBuilder::new(home.clone());
    let main_target = builder.home_path(&[".agents", "main.md"]);
    let main_claude = gpui_agents_hub_profile(
        "claude",
        "Claude Code main",
        builder.home_path(&[".claude"]),
        builder.home_path(&[".claude", "CLAUDE.md"]),
        Some(main_target.clone()),
    );
    let main_codex = gpui_agents_hub_profile(
        "codex",
        "Codex main",
        builder.home_path(&[".codex"]),
        builder.home_path(&[".codex", "AGENTS.md"]),
        Some(main_target.clone()),
    );
    let mut profiles = vec![main_claude.clone()];
    for profile_path in gpui_agents_hub_list_directories(&builder.home_path(&[".claude-profiles"]))
    {
        let name = gpui_file_name_string(&profile_path);
        if !name.starts_with('.') && gpui_is_file(&profile_path.join("CLAUDE.md")) {
            profiles.push(gpui_agents_hub_profile(
                "claude",
                format!("Claude Code {name}"),
                profile_path.clone(),
                profile_path.join("CLAUDE.md"),
                Some(main_target.clone()),
            ));
        }
    }
    profiles.push(main_codex.clone());
    for profile_path in gpui_agents_hub_list_directories(&builder.home_path(&[".codex-profiles"])) {
        let name = gpui_file_name_string(&profile_path);
        if !name.starts_with('.')
            && (gpui_is_file(&profile_path.join("AGENTS.md"))
                || gpui_is_file(&profile_path.join("config.toml")))
        {
            profiles.push(gpui_agents_hub_profile(
                "codex",
                format!("Codex {name}"),
                profile_path.clone(),
                profile_path.join("AGENTS.md"),
                Some(main_target.clone()),
            ));
        }
    }
    let open_code = gpui_agents_hub_profile(
        "opencode",
        "OpenCode main",
        builder.home_path(&[".config", "opencode"]),
        builder.home_path(&[".config", "opencode", "opencode.json"]),
        None,
    );
    let pi_agent = gpui_agents_hub_profile(
        "pi",
        "Pi agent",
        builder.home_path(&[".pi", "agent"]),
        builder.home_path(&[".pi", "agent", "settings.json"]),
        None,
    );
    let linked_profiles = profiles.clone();

    let shared_agents_root = builder.home_path(&[".agents"]);
    builder.add_group(
        "mds",
        "md-shared-agents".to_string(),
        "Shared agent markdown".to_string(),
        shared_agents_root.clone(),
        "Shared instructions and best-practice markdown linked by agent profiles.",
        gpui_agents_hub_walk_files(&shared_agents_root, 1, |candidate| {
            gpui_agents_hub_extension(candidate) == ".md"
        }),
        linked_profiles.clone(),
    );
    let claude_profiles = profiles
        .iter()
        .filter(|profile| profile.agent_icon == "claude")
        .cloned()
        .collect::<Vec<_>>();
    builder.add_group(
        "mds",
        "md-claude-profiles".to_string(),
        "Claude profile instructions".to_string(),
        builder.home_path(&[".claude-profiles"]),
        "CLAUDE.md files owned by Claude profiles.",
        gpui_agents_hub_existing_files(
            claude_profiles
                .iter()
                .map(|item| item.file_path.clone())
                .collect(),
        ),
        claude_profiles,
    );
    let codex_profiles = profiles
        .iter()
        .filter(|profile| profile.agent_icon == "codex")
        .cloned()
        .collect::<Vec<_>>();
    builder.add_group(
        "mds",
        "md-codex-profiles".to_string(),
        "Codex profile instructions".to_string(),
        builder.home_path(&[".codex-profiles"]),
        "AGENTS.md files owned by Codex profiles.",
        gpui_agents_hub_existing_files(
            codex_profiles
                .iter()
                .map(|item| item.file_path.clone())
                .collect(),
        ),
        codex_profiles,
    );

    let skill_roots: Vec<(
        &str,
        PathBuf,
        &'static str,
        &'static str,
        Vec<GpuiAgentsHubProfileItem>,
    )> = vec![
        (
            "skill-shared-dot-agents",
            builder.home_path(&[".agents", "skills"]),
            "Shared skill installed under ~/.agents/skills.",
            "System skill installed in the shared agent skill folder.",
            linked_profiles.clone(),
        ),
        (
            "skill-legacy-shared-agents",
            home.join("agents").join("skills"),
            "Legacy shared skill installed under ~/agents/skills.",
            "System skill installed in the legacy shared agent skill folder.",
            linked_profiles.clone(),
        ),
        (
            "skill-claude",
            builder.home_path(&[".claude", "skills"]),
            "Claude Code global skill installed under ~/.claude/skills.",
            "Claude Code system skill installed under ~/.claude/skills.",
            vec![gpui_agents_hub_profile(
                "claude",
                "Claude Code skills",
                builder.home_path(&[".claude"]),
                builder.home_path(&[".claude", "skills"]),
                None,
            )],
        ),
        (
            "skill-codex",
            builder.home_path(&[".codex", "skills"]),
            "Codex global skill installed under ~/.codex/skills.",
            "Codex system skill installed under ~/.codex/skills.",
            vec![gpui_agents_hub_profile(
                "codex",
                "Codex skills",
                builder.home_path(&[".codex"]),
                builder.home_path(&[".codex", "skills"]),
                None,
            )],
        ),
        (
            "skill-cursor",
            builder.home_path(&[".cursor", "skills"]),
            "Cursor global skill installed under ~/.cursor/skills.",
            "Cursor system skill installed under ~/.cursor/skills.",
            vec![gpui_agents_hub_profile(
                "cursor-cli",
                "Cursor CLI skills",
                builder.home_path(&[".cursor"]),
                builder.home_path(&[".cursor", "skills"]),
                None,
            )],
        ),
        (
            "skill-opencode",
            builder.home_path(&[".config", "opencode", "skills"]),
            "OpenCode global skill installed under ~/.config/opencode/skills.",
            "OpenCode system skill installed under ~/.config/opencode/skills.",
            vec![gpui_agents_hub_profile(
                "opencode",
                "OpenCode skills",
                builder.home_path(&[".config", "opencode"]),
                builder.home_path(&[".config", "opencode", "skills"]),
                None,
            )],
        ),
        (
            "skill-pi",
            builder.home_path(&[".pi", "agent", "skills"]),
            "Pi global skill installed under ~/.pi/agent/skills.",
            "Pi system skill installed under ~/.pi/agent/skills.",
            vec![gpui_agents_hub_profile(
                "pi",
                "Pi skills",
                builder.home_path(&[".pi", "agent"]),
                builder.home_path(&[".pi", "agent", "skills"]),
                None,
            )],
        ),
        (
            "skill-gemini",
            builder.home_path(&[".gemini", "skills"]),
            "Gemini CLI global skill installed under ~/.gemini/skills.",
            "Gemini CLI system skill installed under ~/.gemini/skills.",
            vec![gpui_agents_hub_profile(
                "gemini",
                "Gemini CLI skills",
                builder.home_path(&[".gemini"]),
                builder.home_path(&[".gemini", "skills"]),
                None,
            )],
        ),
        (
            "skill-copilot",
            builder.home_path(&[".copilot", "skills"]),
            "GitHub Copilot global skill installed under ~/.copilot/skills.",
            "GitHub Copilot system skill installed under ~/.copilot/skills.",
            vec![gpui_agents_hub_profile(
                "copilot",
                "GitHub Copilot skills",
                builder.home_path(&[".copilot"]),
                builder.home_path(&[".copilot", "skills"]),
                None,
            )],
        ),
        (
            "skill-factory-droid",
            builder.home_path(&[".factory", "skills"]),
            "Factory Droid global skill installed under ~/.factory/skills.",
            "Factory Droid system skill installed under ~/.factory/skills.",
            vec![gpui_agents_hub_profile(
                "factory-droid",
                "Factory Droid skills",
                builder.home_path(&[".factory"]),
                builder.home_path(&[".factory", "skills"]),
                None,
            )],
        ),
        (
            "skill-antigravity-cli",
            builder.home_path(&[".gemini", "antigravity-cli", "skills"]),
            "Antigravity CLI global skill installed under ~/.gemini/antigravity-cli/skills.",
            "Antigravity CLI system skill installed under ~/.gemini/antigravity-cli/skills.",
            vec![gpui_agents_hub_profile(
                "antigravity-cli",
                "Antigravity CLI skills",
                builder.home_path(&[".gemini", "antigravity-cli"]),
                builder.home_path(&[".gemini", "antigravity-cli", "skills"]),
                None,
            )],
        ),
        (
            "skill-antigravity",
            builder.home_path(&[".gemini", "antigravity", "skills"]),
            "Antigravity global skill installed under ~/.gemini/antigravity/skills.",
            "Antigravity system skill installed under ~/.gemini/antigravity/skills.",
            vec![gpui_agents_hub_profile(
                "antigravity-cli",
                "Antigravity skills",
                builder.home_path(&[".gemini", "antigravity"]),
                builder.home_path(&[".gemini", "antigravity", "skills"]),
                None,
            )],
        ),
        (
            "skill-config-agents",
            builder.home_path(&[".config", "agents", "skills"]),
            "Universal agent skill installed under ~/.config/agents/skills.",
            "Universal agent system skill installed under ~/.config/agents/skills.",
            vec![gpui_agents_hub_profile(
                "amp-cli",
                "Universal agent skills",
                builder.home_path(&[".config", "agents"]),
                builder.home_path(&[".config", "agents", "skills"]),
                None,
            )],
        ),
        (
            "skill-hermes-agent",
            builder.home_path(&[".hermes", "skills"]),
            "Hermes Agent global skill installed under ~/.hermes/skills.",
            "Hermes Agent system skill installed under ~/.hermes/skills.",
            vec![gpui_agents_hub_profile(
                "hermes-agent",
                "Hermes Agent skills",
                builder.home_path(&[".hermes"]),
                builder.home_path(&[".hermes", "skills"]),
                None,
            )],
        ),
        (
            "skill-kiro",
            builder.home_path(&[".kiro", "skills"]),
            "Kiro CLI global skill installed under ~/.kiro/skills.",
            "Kiro CLI system skill installed under ~/.kiro/skills.",
            vec![gpui_agents_hub_profile(
                "kiro",
                "Kiro CLI skills",
                builder.home_path(&[".kiro"]),
                builder.home_path(&[".kiro", "skills"]),
                None,
            )],
        ),
        (
            "skill-codebuddy",
            builder.home_path(&[".codebuddy", "skills"]),
            "CodeBuddy global skill installed under ~/.codebuddy/skills.",
            "CodeBuddy system skill installed under ~/.codebuddy/skills.",
            vec![gpui_agents_hub_profile(
                "codebuddy",
                "CodeBuddy skills",
                builder.home_path(&[".codebuddy"]),
                builder.home_path(&[".codebuddy", "skills"]),
                None,
            )],
        ),
        (
            "skill-qoder",
            builder.home_path(&[".qoder", "skills"]),
            "Qoder global skill installed under ~/.qoder/skills.",
            "Qoder system skill installed under ~/.qoder/skills.",
            vec![gpui_agents_hub_profile(
                "qoder",
                "Qoder skills",
                builder.home_path(&[".qoder"]),
                builder.home_path(&[".qoder", "skills"]),
                None,
            )],
        ),
        (
            "skill-rovo-dev",
            builder.home_path(&[".rovodev", "skills"]),
            "Rovo Dev global skill installed under ~/.rovodev/skills.",
            "Rovo Dev system skill installed under ~/.rovodev/skills.",
            vec![gpui_agents_hub_profile(
                "rovo-dev",
                "Rovo Dev skills",
                builder.home_path(&[".rovodev"]),
                builder.home_path(&[".rovodev", "skills"]),
                None,
            )],
        ),
    ];
    for (id_prefix, root, description, system_description, profiles) in skill_roots {
        gpui_agents_hub_add_skill_root(
            &mut builder,
            id_prefix,
            &root,
            description,
            system_description,
            profiles,
        );
    }

    for plugins_root in [
        builder.home_path(&[".codex-profiles"]),
        builder.home_path(&[".claude-profiles"]),
    ] {
        for profile_dir in gpui_agents_hub_list_directories(&plugins_root) {
            if gpui_file_name_string(&profile_dir).starts_with('.') {
                continue;
            }
            let cache_root = profile_dir.join("plugins").join("cache");
            if !cache_root.exists() {
                continue;
            }
            let mut roots: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
            for plugin_file in gpui_agents_hub_walk_files(&cache_root, 7, |candidate| {
                gpui_file_name_string(candidate) == "SKILL.md"
                    || gpui_path_string(candidate).ends_with("/.codex-plugin/plugin.json")
                    || gpui_path_string(candidate).ends_with("/.claude-plugin/plugin.json")
            }) {
                if let Some(root) = gpui_agents_hub_plugin_root(&plugin_file) {
                    roots.entry(root).or_default().push(plugin_file);
                }
            }
            let mut roots = roots.into_iter().collect::<Vec<_>>();
            roots.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (root, files) in roots {
                let rel_name = gpui_relative_path_name(&root, &cache_root)
                    .unwrap_or_else(|| gpui_file_name_string(&root));
                builder.add_group(
                    "skills",
                    format!("skill-profile-{}", gpui_agents_hub_file_id(&root)),
                    rel_name,
                    root.clone(),
                    "Skill or plugin manifest installed inside an agent profile plugin cache.",
                    files,
                    gpui_agents_hub_profiles_for(
                        &root,
                        &home,
                        &profiles,
                        &linked_profiles,
                        &open_code,
                        &pi_agent,
                    ),
                );
            }
        }
    }

    let hooks_root = home.join("agents").join("hooks");
    let mut hooks_profiles = linked_profiles.clone();
    hooks_profiles.push(pi_agent.clone());
    builder.add_group(
        "hooks",
        "hooks-shared".to_string(),
        "Shared hooks".to_string(),
        hooks_root.clone(),
        "Shared hook scripts and documentation used by agent profiles.",
        gpui_agents_hub_walk_files(&hooks_root, 3, gpui_agents_hub_is_text_file),
        hooks_profiles,
    );
    builder.add_group(
        "hooks",
        "hooks-codex-main".to_string(),
        "Codex main hooks".to_string(),
        builder.home_path(&[".codex"]),
        "Global Codex hook configuration.",
        gpui_agents_hub_existing_files(vec![builder.home_path(&[".codex", "hooks.json"])]),
        vec![main_codex.clone()],
    );
    builder.add_group(
        "hooks",
        "hooks-codex-profiles".to_string(),
        "Codex profile hooks".to_string(),
        builder.home_path(&[".codex-profiles"]),
        "hooks.json files owned by Codex profiles.",
        gpui_agents_hub_walk_files(&builder.home_path(&[".codex-profiles"]), 2, |candidate| {
            gpui_file_name_string(candidate) == "hooks.json"
        }),
        profiles
            .iter()
            .filter(|item| item.agent_icon == "codex")
            .cloned()
            .collect(),
    );
    builder.add_group(
        "hooks",
        "hooks-cursor-main".to_string(),
        "Cursor hooks".to_string(),
        builder.home_path(&[".cursor"]),
        "Cursor Agent hook configuration.",
        gpui_agents_hub_existing_files(vec![builder.home_path(&[".cursor", "hooks.json"])]),
        Vec::new(),
    );
    builder.add_group(
        "hooks",
        "hooks-antigravity-main".to_string(),
        "Antigravity hooks".to_string(),
        builder.home_path(&[".gemini", "config"]),
        "Antigravity hook configuration.",
        gpui_agents_hub_existing_files(vec![builder.home_path(&[
            ".gemini",
            "config",
            "hooks.json",
        ])]),
        Vec::new(),
    );
    builder.add_group(
        "hooks",
        "hooks-grok-main".to_string(),
        "Grok hooks".to_string(),
        builder.home_path(&[".grok", "hooks"]),
        "Grok hook configuration.",
        gpui_agents_hub_existing_files(vec![builder.home_path(&[
            ".grok",
            "hooks",
            "ghostex-session.json",
        ])]),
        Vec::new(),
    );
    builder.add_group(
        "hooks",
        "hooks-pi-agent".to_string(),
        "Pi extensions".to_string(),
        builder.home_path(&[".pi", "agent"]),
        "Pi agent extension hooks and settings-adjacent TypeScript files.",
        gpui_agents_hub_walk_files(
            &builder.home_path(&[".pi", "agent", "extensions"]),
            2,
            |candidate| {
                matches!(
                    gpui_agents_hub_extension(candidate).as_str(),
                    ".ts" | ".js" | ".json"
                )
            },
        ),
        vec![pi_agent.clone()],
    );

    builder.add_group(
        "configs",
        "config-shared-agents".to_string(),
        "Shared agent config".to_string(),
        shared_agents_root.clone(),
        "Shared agent lock and setup files.",
        gpui_agents_hub_walk_files(&shared_agents_root, 1, |candidate| {
            gpui_file_name_string(candidate).ends_with(".json")
        }),
        linked_profiles.clone(),
    );
    builder.add_group(
        "configs",
        "config-claude-main".to_string(),
        "Claude main configs".to_string(),
        builder.home_path(&[".claude"]),
        "Global Claude Code settings and MCP configuration.",
        gpui_agents_hub_existing_files(vec![
            builder.home_path(&[".claude.json"]),
            builder.home_path(&[".claude", "settings.json"]),
            builder.home_path(&[".claude", "settings.local.json"]),
        ]),
        vec![main_claude.clone()],
    );
    for item in profiles.iter().filter(|profile| {
        profile.agent_icon == "claude"
            && gpui_path_string(&profile.profile_path).contains("-profiles")
    }) {
        let root = item.profile_path.clone();
        builder.add_group(
            "configs",
            format!("config-claude-{}", gpui_agents_hub_file_id(&root)),
            format!("Claude {} configs", gpui_file_name_string(&root)),
            root.clone(),
            "Claude profile-owned config and plugin registry files.",
            gpui_agents_hub_existing_files(vec![
                root.join(".claude.json"),
                root.join("settings.json"),
                root.join("settings.local.json"),
                root.join("policy-limits.json"),
                root.join("stats-cache.json"),
                root.join("plugins").join("installed_plugins.json"),
                root.join("plugins").join("known_marketplaces.json"),
                root.join("plugins").join("blocklist.json"),
            ]),
            vec![item.clone()],
        );
    }
    builder.add_group(
        "configs",
        "config-codex-main".to_string(),
        "Codex main configs".to_string(),
        builder.home_path(&[".codex"]),
        "Global Codex TOML configuration.",
        gpui_agents_hub_existing_files(vec![builder.home_path(&[".codex", "config.toml"])]),
        vec![main_codex.clone()],
    );
    for item in profiles.iter().filter(|profile| {
        profile.agent_icon == "codex"
            && gpui_path_string(&profile.profile_path).contains("-profiles")
    }) {
        let root = item.profile_path.clone();
        builder.add_group(
            "configs",
            format!("config-codex-{}", gpui_agents_hub_file_id(&root)),
            format!("Codex {} configs", gpui_file_name_string(&root)),
            root.clone(),
            "Codex profile-owned config, browser, and plugin registry files.",
            gpui_agents_hub_existing_files(vec![
                root.join("config.toml"),
                root.join(".codex-global-state.json"),
                root.join("browser").join("config.toml"),
                root.join("plugins").join("installed_plugins.json"),
                root.join("plugins").join("known_marketplaces.json"),
                root.join("plugins").join("blocklist.json"),
            ]),
            vec![item.clone()],
        );
    }
    builder.add_group(
        "configs",
        "config-opencode".to_string(),
        "OpenCode configs".to_string(),
        open_code.profile_path.clone(),
        "OpenCode JSON, package, and plugin files.",
        gpui_agents_hub_walk_files(&open_code.profile_path, 2, |candidate| {
            matches!(
                gpui_file_name_string(candidate).as_str(),
                "opencode.json" | "tui.json" | "package.json"
            ) || (gpui_file_name_string(candidate.parent().unwrap_or_else(|| Path::new("")))
                == "plugin"
                && gpui_agents_hub_extension(candidate) == ".js")
        }),
        vec![open_code.clone()],
    );
    builder.add_group(
        "configs",
        "config-pi".to_string(),
        "Pi configs".to_string(),
        pi_agent.profile_path.clone(),
        "Pi agent settings and local extension files.",
        gpui_agents_hub_walk_files(&pi_agent.profile_path, 2, |candidate| {
            matches!(
                gpui_agents_hub_extension(candidate).as_str(),
                ".json" | ".ts" | ".js"
            ) && gpui_file_name_string(candidate) != "auth.json"
        }),
        vec![pi_agent.clone()],
    );

    builder.finish()
}

pub(crate) fn gpui_agents_hub_add_skill_root(
    builder: &mut GpuiAgentsHubCatalogBuilder,
    id_prefix: &str,
    root: &Path,
    description: &'static str,
    system_description: &'static str,
    profiles: Vec<GpuiAgentsHubProfileItem>,
) {
    for skill_dir in gpui_agents_hub_list_directories(root) {
        let skill_name = gpui_file_name_string(&skill_dir);
        if skill_name.starts_with('.') && skill_name != ".system" {
            continue;
        }
        if skill_name == ".system" {
            for system_skill in gpui_agents_hub_list_directories(&skill_dir) {
                builder.add_group(
                    "skills",
                    format!(
                        "{id_prefix}-system-{}",
                        gpui_agents_hub_file_id(&system_skill)
                    ),
                    gpui_file_name_string(&system_skill),
                    system_skill.clone(),
                    system_description,
                    gpui_agents_hub_walk_files(&system_skill, 3, gpui_agents_hub_is_skill_file),
                    profiles.clone(),
                );
            }
            continue;
        }
        builder.add_group(
            "skills",
            format!("{id_prefix}-{}", gpui_agents_hub_file_id(&skill_dir)),
            skill_name,
            skill_dir.clone(),
            description,
            gpui_agents_hub_walk_files(&skill_dir, 3, gpui_agents_hub_is_skill_file),
            profiles.clone(),
        );
    }
}

pub(crate) fn gpui_empty_agents_hub_catalog_build() -> GpuiAgentsHubCatalogBuild {
    GpuiAgentsHubCatalogBuild {
        file_paths: HashSet::new(),
        message: serde_json::json!({
            "generatedAt": gpui_status_generated_at(),
            "groupsByTab": {
                "configs": [],
                "hooks": [],
                "mds": [],
                "skills": [],
            },
            "type": "agentsHubCatalog",
        }),
        open_paths: HashSet::new(),
    }
}

