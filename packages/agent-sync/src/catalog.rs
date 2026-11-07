//! The agent catalog: where every supported agent keeps its global skills,
//! its instruction entry file, and (for a few) its hook scripts. Data, not code:
//! detection, the Hub list, the plan, and the CLI all read this one table.
//!
//! CDXC:AgentSync 2026-09-16 WHY:
//! The paths mirror the `skills` CLI agent table (vercel-labs/skills `src/agents.ts`) so a
//! skill installed by `npx skills add -g` and one linked by Agent Sync land in the same folder.
//! The instruction-file and hooks columns are Ghostex additions the CLI does not have.
//! Amp, Cursor and OpenCode read `~/.agents/skills` directly (gxserver already treats them as
//! universal in `agent_skill_global_dir`), so they are flagged `universal` and never get links.
//!
//! CDXC:AgentSync 2026-10-14 WHY:
//! A probe skill placed only in `~/.agents/skills` was listed by Codex 0.162 (and every Codex
//! profile, which reads `$HOME/.agents/skills` whatever its CODEX_HOME), Gemini CLI 0.41, GitHub
//! Copilot CLI 1.0.80 and Pi 0.99, so they are universal too: a link in their own folder made
//! them load each skill twice (Gemini logs a "Skill conflict" for each one). Claude Code 2.1.284
//! and Hermes Agent 0.21 did not list it, so they keep per-skill links.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// The source-of-truth folder under HOME.
pub const SOURCE_DIR_NAME: &str = ".agents";
/// The one line every instruction entry file carries.
pub const POINTER_LINE: &str =
    "Before starting any task, read ~/.agents/main.md in full and follow its instructions.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentKind {
    Agent,
    Profile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstructionKind {
    /// A plain markdown file holding only the pointer line.
    Plain,
    /// A Cursor `.mdc` rule: YAML frontmatter with `alwaysApply: true` around the pointer line.
    Mdc,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSpec {
    pub id: String,
    pub display_name: String,
    /// A `SidebarAgentIcon` id the UI can render, when one exists.
    pub icon: Option<String>,
    pub kind: AgentKind,
    /// The folder whose presence means the agent is installed.
    pub root: PathBuf,
    pub skills_dir: Option<PathBuf>,
    /// Reads `~/.agents/skills` directly, so per-skill links would duplicate every skill.
    pub universal: bool,
    pub instruction_file: Option<PathBuf>,
    pub instruction_kind: InstructionKind,
    pub hooks_dir: Option<PathBuf>,
    pub lock_link: Option<PathBuf>,
}

struct Row {
    id: &'static str,
    display_name: &'static str,
    icon: Option<&'static str>,
    root: &'static str,
    skills_dir: Option<&'static str>,
    universal: bool,
    instruction_file: Option<&'static str>,
    instruction_kind: InstructionKind,
    hooks: bool,
}

const fn row(
    id: &'static str,
    display_name: &'static str,
    icon: Option<&'static str>,
    root: &'static str,
    skills_dir: Option<&'static str>,
    instruction_file: Option<&'static str>,
) -> Row {
    Row {
        id,
        display_name,
        icon,
        root,
        skills_dir,
        universal: false,
        instruction_file,
        instruction_kind: InstructionKind::Plain,
        hooks: false,
    }
}

/// Paths are relative to HOME unless they start with `$XDG/` (the XDG config
/// home, `~/.config` by default) or `$ENV:NAME:default/` (an absolute override
/// from the environment, as `CODEX_HOME`-style variables).
const ROWS: &[Row] = &[
    Row {
        hooks: true,
        ..row(
            "claude-code",
            "Claude Code",
            Some("claude"),
            ".claude",
            Some(".claude/skills"),
            Some(".claude/CLAUDE.md"),
        )
    },
    Row {
        hooks: true,
        universal: true,
        ..row(
            "codex",
            "Codex",
            Some("codex"),
            ".codex",
            Some(".codex/skills"),
            Some(".codex/AGENTS.md"),
        )
    },
    Row {
        universal: true,
        ..row(
            "gemini-cli",
            "Gemini CLI",
            Some("gemini"),
            ".gemini",
            Some(".gemini/skills"),
            Some(".gemini/GEMINI.md"),
        )
    },
    row(
        "antigravity",
        "Antigravity",
        Some("antigravity-cli"),
        ".gemini/antigravity",
        Some(".gemini/antigravity/skills"),
        None,
    ),
    row(
        "antigravity-cli",
        "Antigravity CLI",
        Some("antigravity-cli"),
        ".gemini/antigravity-cli",
        Some(".gemini/antigravity-cli/skills"),
        None,
    ),
    Row {
        universal: true,
        instruction_kind: InstructionKind::Mdc,
        ..row(
            "cursor",
            "Cursor",
            Some("cursor-cli"),
            ".cursor",
            Some(".cursor/skills"),
            Some(".cursor/rules/main.mdc"),
        )
    },
    Row {
        universal: true,
        ..row(
            "opencode",
            "OpenCode",
            Some("opencode"),
            "$XDG/opencode",
            Some("$XDG/opencode/skills"),
            Some("$XDG/opencode/AGENTS.md"),
        )
    },
    Row {
        universal: true,
        ..row(
            "amp",
            "Amp",
            Some("amp-cli"),
            "$XDG/amp",
            Some("$XDG/agents/skills"),
            Some("$XDG/amp/AGENTS.md"),
        )
    },
    Row {
        universal: true,
        ..row(
            "github-copilot",
            "GitHub Copilot",
            Some("copilot"),
            ".copilot",
            Some(".copilot/skills"),
            Some(".copilot/copilot-instructions.md"),
        )
    },
    row(
        "droid",
        "Droid",
        Some("factory-droid"),
        ".factory",
        Some(".factory/skills"),
        Some(".factory/AGENTS.md"),
    ),
    row(
        "kiro-cli",
        "Kiro CLI",
        Some("kiro"),
        ".kiro",
        Some(".kiro/skills"),
        Some(".kiro/steering/ghostex-shared.md"),
    ),
    Row {
        universal: true,
        ..row(
            "pi",
            "Pi",
            Some("pi"),
            ".pi/agent",
            Some(".pi/agent/skills"),
            Some(".pi/agent/AGENTS.md"),
        )
    },
    row(
        "qwen-code",
        "Qwen Code",
        None,
        ".qwen",
        Some(".qwen/skills"),
        Some(".qwen/QWEN.md"),
    ),
    row(
        "goose",
        "Goose",
        None,
        "$XDG/goose",
        Some("$XDG/goose/skills"),
        Some("$XDG/goose/.goosehints"),
    ),
    row(
        "roo",
        "Roo Code",
        None,
        ".roo",
        Some(".roo/skills"),
        Some(".roo/rules/ghostex-shared.md"),
    ),
    row(
        "continue",
        "Continue",
        None,
        ".continue",
        Some(".continue/skills"),
        None,
    ),
    row(
        "augment",
        "Augment",
        None,
        ".augment",
        Some(".augment/skills"),
        None,
    ),
    row(
        "kilo",
        "Kilo Code",
        None,
        ".kilocode",
        Some(".kilocode/skills"),
        Some(".kilocode/rules/ghostex-shared.md"),
    ),
    row("trae", "Trae", None, ".trae", Some(".trae/skills"), None),
    row(
        "hermes-agent",
        "Hermes Agent",
        Some("hermes-agent"),
        "$ENV:HERMES_HOME:.hermes",
        Some("$ENV:HERMES_HOME:.hermes/skills"),
        None,
    ),
    row(
        "qoder",
        "Qoder",
        Some("qoder"),
        ".qoder",
        Some(".qoder/skills"),
        None,
    ),
    row(
        "codebuddy",
        "CodeBuddy",
        Some("codebuddy"),
        ".codebuddy",
        Some(".codebuddy/skills"),
        None,
    ),
    row(
        "rovodev",
        "Rovo Dev",
        Some("rovo-dev"),
        ".rovodev",
        Some(".rovodev/skills"),
        None,
    ),
    row(
        "grok",
        "Grok Build",
        Some("grok-build"),
        "$ENV:GROK_HOME:.grok",
        Some("$ENV:GROK_HOME:.grok/skills"),
        None,
    ),
    row(
        "command-code",
        "Command Code",
        Some("command-code"),
        ".commandcode",
        Some(".commandcode/skills"),
        None,
    ),
    row(
        "zcode",
        "ZCode",
        Some("zcode"),
        ".zcode",
        Some(".zcode/skills"),
        None,
    ),
    row(
        "windsurf",
        "Windsurf",
        None,
        ".codeium/windsurf",
        Some(".codeium/windsurf/skills"),
        Some(".codeium/windsurf/memories/global_rules.md"),
    ),
    row(
        "crush",
        "Crush",
        None,
        "$XDG/crush",
        Some("$XDG/crush/skills"),
        None,
    ),
    row(
        "devin",
        "Devin for Terminal",
        Some("devin"),
        "$XDG/devin",
        Some("$XDG/devin/skills"),
        None,
    ),
    row(
        "junie",
        "Junie",
        None,
        ".junie",
        Some(".junie/skills"),
        None,
    ),
    row(
        "openhands",
        "OpenHands",
        None,
        ".openhands",
        Some(".openhands/skills"),
        None,
    ),
    row(
        "mistral-vibe",
        "Mistral Vibe",
        None,
        "$ENV:VIBE_HOME:.vibe",
        Some("$ENV:VIBE_HOME:.vibe/skills"),
        None,
    ),
    row(
        "forgecode",
        "ForgeCode",
        None,
        ".forge",
        Some(".forge/skills"),
        None,
    ),
    row("mux", "Mux", None, ".mux", Some(".mux/skills"), None),
    row(
        "moxby",
        "Moxby",
        None,
        ".moxby",
        Some(".moxby/skills"),
        None,
    ),
    row("ona", "Ona", None, ".ona", Some(".ona/skills"), None),
    row(
        "tabnine-cli",
        "Tabnine CLI",
        None,
        ".tabnine/agent",
        Some(".tabnine/agent/skills"),
        None,
    ),
    row(
        "iflow-cli",
        "iFlow CLI",
        None,
        ".iflow",
        Some(".iflow/skills"),
        None,
    ),
    row(
        "lingma",
        "Lingma",
        None,
        ".lingma",
        Some(".lingma/skills"),
        None,
    ),
    row("kode", "Kode", None, ".kode", Some(".kode/skills"), None),
    row("jazz", "Jazz", None, ".jazz", Some(".jazz/skills"), None),
    row("bob", "IBM Bob", None, ".bob", Some(".bob/skills"), None),
    row("adal", "AdaL", None, ".adal", Some(".adal/skills"), None),
    row(
        "openclaw",
        "OpenClaw",
        None,
        ".openclaw",
        Some(".openclaw/skills"),
        None,
    ),
    row(
        "deepagents",
        "Deep Agents",
        None,
        ".deepagents/agent",
        Some(".deepagents/agent/skills"),
        None,
    ),
    row(
        "firebender",
        "Firebender",
        None,
        ".firebender",
        Some(".firebender/skills"),
        None,
    ),
    row(
        "codemaker",
        "Codemaker",
        None,
        ".codemaker",
        Some(".codemaker/skills"),
        None,
    ),
    row(
        "codestudio",
        "Code Studio",
        None,
        ".codestudio",
        Some(".codestudio/skills"),
        None,
    ),
    row(
        "cortex",
        "Cortex Code",
        None,
        ".snowflake/cortex",
        Some(".snowflake/cortex/skills"),
        None,
    ),
    row(
        "aider-desk",
        "AiderDesk",
        None,
        ".aider-desk",
        Some(".aider-desk/skills"),
        None,
    ),
    row(
        "astrbot",
        "AstrBot",
        None,
        ".astrbot",
        Some(".astrbot/data/skills"),
        None,
    ),
    row(
        "autohand-code",
        "Autohand Code CLI",
        None,
        "$ENV:AUTOHAND_HOME:.autohand",
        Some("$ENV:AUTOHAND_HOME:.autohand/skills"),
        None,
    ),
    row(
        "codearts-agent",
        "CodeArts Agent",
        None,
        ".codeartsdoer",
        Some(".codeartsdoer/skills"),
        None,
    ),
    row(
        "inference-sh",
        "inference.sh",
        None,
        ".inferencesh",
        Some(".inferencesh/skills"),
        None,
    ),
    row(
        "mcpjam",
        "MCPJam",
        None,
        ".mcpjam",
        Some(".mcpjam/skills"),
        None,
    ),
    row(
        "minimax-code",
        "MiniMax Code",
        None,
        ".minimax",
        Some(".minimax/skills"),
        None,
    ),
    row(
        "neovate",
        "Neovate",
        None,
        ".neovate",
        Some(".neovate/skills"),
        None,
    ),
    row(
        "pochi",
        "Pochi",
        None,
        ".pochi",
        Some(".pochi/skills"),
        None,
    ),
    row(
        "posit-assistant",
        "Posit Assistant",
        None,
        ".posit/assistant",
        Some(".posit/assistant/skills"),
        None,
    ),
    row(
        "qoder-cn",
        "Qoder CN",
        None,
        ".qoder-cn",
        Some(".qoder-cn/skills"),
        None,
    ),
    row(
        "reasonix",
        "Reasonix",
        None,
        ".reasonix",
        Some(".reasonix/skills"),
        None,
    ),
    row(
        "terramind",
        "Terramind",
        None,
        ".terramind",
        Some(".terramind/skills"),
        None,
    ),
    row(
        "tinycloud",
        "Tinycloud",
        None,
        ".tinycloud",
        Some(".tinycloud/skills"),
        None,
    ),
    row(
        "trae-cn",
        "Trae CN",
        None,
        ".trae-cn",
        Some(".trae-cn/skills"),
        None,
    ),
    row(
        "zencoder",
        "Zencoder",
        None,
        ".zencoder",
        Some(".zencoder/skills"),
        None,
    ),
    row(
        "sarvam-code",
        "Sarvam Code",
        None,
        "$ENV:SARVAM_HOME:.sarvam",
        None,
        None,
    ),
    row("warp", "Warp", None, ".warp", None, None),
    row(
        "kimi-code-cli",
        "Kimi Code CLI",
        Some("kimi"),
        ".kimi",
        None,
        None,
    ),
];

/// Agents that read `~/.agents/skills` natively and have no folder of their own
/// (their `skills_dir` is `None` and `universal` is set).
const NATIVE_SOURCE_READERS: &[&str] = &["sarvam-code", "warp", "kimi-code-cli"];

fn resolve(home: &Path, xdg_config: &Path, spec: &str) -> PathBuf {
    if let Some(rest) = spec.strip_prefix("$XDG/") {
        return xdg_config.join(rest);
    }
    if let Some(rest) = spec.strip_prefix("$ENV:") {
        // `$ENV:NAME:default[/suffix]`
        let (name, remainder) = rest.split_once(':').unwrap_or((rest, ""));
        let (default_dir, suffix) = match remainder.split_once('/') {
            Some((default_dir, suffix)) => (default_dir, Some(suffix)),
            None => (remainder, None),
        };
        let base = std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(default_dir));
        return match suffix {
            Some(suffix) => base.join(suffix),
            None => base,
        };
    }
    // Codex's own folder: on Windows the Profile folder's, whatever HOME says.
    if spec == ".codex" {
        return ghostex_paths::agent_homes::codex_default_home(home);
    }
    if let Some(rest) = spec.strip_prefix(".codex/") {
        return ghostex_paths::agent_homes::codex_default_home(home).join(rest);
    }
    // Claude Code's own folder: on Windows USERPROFILE's, whatever HOME says.
    if spec == ".claude" {
        return ghostex_paths::agent_homes::claude_default_home(home);
    }
    if let Some(rest) = spec.strip_prefix(".claude/") {
        return ghostex_paths::agent_homes::claude_default_home(home).join(rest);
    }
    home.join(spec)
}

/// The XDG config home: `$XDG_CONFIG_HOME` when absolute, else `~/.config`.
pub fn xdg_config_home(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"))
}

/// The source-of-truth folder, `~/.agents`.
pub fn source_root(home: &Path) -> PathBuf {
    home.join(SOURCE_DIR_NAME)
}

/// Every known agent plus the Claude Code and Codex profiles found under
/// `~/.claude-profiles` and `~/.codex-profiles`, in catalog order. Callers
/// decide what to do with agents whose `root` does not exist.
pub fn agent_catalog(home: &Path) -> Vec<AgentSpec> {
    let xdg = xdg_config_home(home);
    let mut specs = Vec::with_capacity(ROWS.len() + 8);
    for row in ROWS {
        let root = resolve(home, &xdg, row.root);
        let spec = AgentSpec {
            id: row.id.to_string(),
            display_name: row.display_name.to_string(),
            icon: row.icon.map(str::to_string),
            kind: AgentKind::Agent,
            skills_dir: row.skills_dir.map(|dir| resolve(home, &xdg, dir)),
            universal: row.universal || NATIVE_SOURCE_READERS.contains(&row.id),
            instruction_file: row.instruction_file.map(|file| resolve(home, &xdg, file)),
            instruction_kind: row.instruction_kind,
            hooks_dir: row.hooks.then(|| root.join("hooks")),
            lock_link: row.hooks.then(|| root.join(".skill-lock.json")),
            root,
        };
        specs.push(spec);
        if row.id == "claude-code" {
            specs.extend(profile_specs(
                home,
                "claude-code",
                "Claude Code",
                "claude",
                ".claude-profiles",
                "CLAUDE.md",
                &["settings.json", "CLAUDE.md"],
            ));
        }
        if row.id == "codex" {
            specs.extend(profile_specs(
                home,
                "codex",
                "Codex",
                "codex",
                ".codex-profiles",
                "AGENTS.md",
                &["config.toml", "AGENTS.md"],
            ));
        }
    }
    specs
}

/// Folder names under a profiles root that are never profiles.
const NOT_PROFILE_NAMES: &[&str] = &["skills", "memories", "tmp", "logs", "cache", "node_modules"];

fn profile_specs(
    home: &Path,
    agent_id: &str,
    agent_name: &str,
    icon: &str,
    profiles_dir: &str,
    instruction_name: &str,
    markers: &[&str],
) -> Vec<AgentSpec> {
    let profiles_root = home.join(profiles_dir);
    let Ok(entries) = std::fs::read_dir(&profiles_root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| !NOT_PROFILE_NAMES.contains(&name.as_str()) && !name.ends_with(".bak"))
        .collect();
    names.sort();
    names
        .into_iter()
        .filter(|name| {
            let dir = profiles_root.join(name);
            markers.iter().any(|marker| dir.join(marker).exists())
        })
        .map(|name| {
            let root = profiles_root.join(&name);
            AgentSpec {
                id: format!("{agent_id}:{name}"),
                display_name: format!("{agent_name} {name} profile"),
                icon: Some(icon.to_string()),
                kind: AgentKind::Profile,
                skills_dir: Some(root.join("skills")),
                universal: agent_id == "codex",
                instruction_file: Some(root.join(instruction_name)),
                instruction_kind: InstructionKind::Plain,
                hooks_dir: Some(root.join("hooks")),
                lock_link: Some(root.join(".skill-lock.json")),
                root,
            }
        })
        .collect()
}
