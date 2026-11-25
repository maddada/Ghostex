//! Managed Ghostex Native Pi TypeScript extension (`ghostex-pi-native-extension.ts`).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// Ghostex-owned TypeScript extension passed to `pi --mode rpc --extension <path>`.
///
/// 1. Gates non-read-only tool calls through `ctx.ui.confirm` with a structured JSON payload so
///    `driver_pi.rs` evaluates Ghostex's live approval level (`Ask`, `Auto-edits`, `Auto`,
///    `Full permissions`, `--read-only`) without restarting Pi.
/// 2. Expands `$skill-name` tokens in user prompts using installed Pi / Agent skills.
/// 3. Caps OpenRouter `max_tokens` / `max_completion_tokens` at 32768 so high-ceiling models do
///    not fail on smaller OpenRouter credit balances.
pub(super) const GHOSTEX_PI_NATIVE_EXTENSION_SOURCE: &str = r#"// Ghostex Native Pi extension (managed by ghostex acp-host; do not edit manually).
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

pub(super) const READ_ONLY_TOOLS = new Set(["read", "grep", "find", "ls", "tool_search"]);
pub(super) const OPENROUTER_MAX_TOKENS_CAP = 32768;

function stripFrontmatter(content: string): string {
  const normalized = content.replace(/\r\n/g, "\n");
  if (!normalized.startsWith("---\n")) return normalized.trim();
  const closingIndex = normalized.indexOf("\n---", 4);
  if (closingIndex === -1) return normalized.trim();
  return normalized.slice(closingIndex + 4).trim();
}

function findSkillFile(skillName: string, cwd: string): string | null {
  const safeName = skillName.trim();
  if (!safeName || safeName.includes("/") || safeName.includes("\\") || safeName.includes("..")) {
    return null;
  }
  const home = os.homedir();
  const agentDir = process.env.PI_CODING_AGENT_DIR || path.join(home, ".pi", "agent");
  const roots = [
    path.join(cwd, ".pi", "skills"),
    path.join(cwd, ".agents", "skills"),
    path.join(agentDir, "skills"),
    path.join(home, ".agents", "skills"),
  ];
  for (const root of roots) {
    const direct = path.join(root, safeName, "SKILL.md");
    if (fs.existsSync(direct)) return direct;
    const flat = path.join(root, `${safeName}.md`);
    if (fs.existsSync(flat)) return flat;
  }
  return null;
}

function expandSkillReferences(text: string, cwd: string): string | null {
  const pattern = /(^|[\s(])\$([a-zA-Z0-9][a-zA-Z0-9_-]*)/g;
  const seen = new Set<string>();
  const blocks: string[] = [];
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(text)) !== null) {
    const name = match[2];
    if (!name || seen.has(name)) continue;
    const skillPath = findSkillFile(name, cwd);
    if (!skillPath) continue;
    try {
      const raw = fs.readFileSync(skillPath, "utf8");
      const body = stripFrontmatter(raw);
      if (!body) continue;
      seen.add(name);
      blocks.push(`<skill name="${name}" location="${skillPath}">\n${body}\n</skill>`);
    } catch {
      // Ignore unreadable skill files.
    }
  }
  if (blocks.length === 0) return null;
  return `${blocks.join("\n\n")}\n\n${text}`;
}

export default function ghostexPiNativeExtension(pi: any) {
  pi.on("tool_call", async (event: any, ctx: any) => {
    const toolName = typeof event?.toolName === "string" ? event.toolName : "";
    if (!toolName || READ_ONLY_TOOLS.has(toolName.toLowerCase())) {
      return undefined;
    }
    if (!ctx?.hasUI || !ctx?.ui?.confirm) {
      return undefined;
    }
    const payload = JSON.stringify({
      ghostexToolApproval: true,
      toolName,
      toolCallId: typeof event?.toolCallId === "string" ? event.toolCallId : undefined,
      input: event?.input ?? {},
    });
    const allowed = await ctx.ui.confirm(`Allow ${toolName}?`, payload, {
      signal: ctx.signal,
    });
    if (!allowed) {
      return {
        block: true,
        reason: `User denied ${toolName}.`,
      };
    }
    return undefined;
  });

  pi.on("input", (event: any, ctx: any) => {
    if (event?.source === "extension" || typeof event?.text !== "string") {
      return { action: "continue" };
    }
    const expanded = expandSkillReferences(event.text, ctx?.cwd || process.cwd());
    if (!expanded) {
      return { action: "continue" };
    }
    return { action: "transform", text: expanded };
  });

  pi.on("before_provider_request", (event: any, ctx: any) => {
    if (ctx?.model?.provider !== "openrouter") {
      return undefined;
    }
    const payload = event?.payload;
    if (!payload || typeof payload !== "object") {
      return undefined;
    }
    let changed = false;
    const next: Record<string, unknown> = { ...payload };
    if (typeof next.max_tokens === "number" && next.max_tokens > OPENROUTER_MAX_TOKENS_CAP) {
      next.max_tokens = OPENROUTER_MAX_TOKENS_CAP;
      changed = true;
    }
    if (
      typeof next.max_completion_tokens === "number" &&
      next.max_completion_tokens > OPENROUTER_MAX_TOKENS_CAP
    ) {
      next.max_completion_tokens = OPENROUTER_MAX_TOKENS_CAP;
      changed = true;
    }
    return changed ? next : undefined;
  });
}
"#;

pub(super) fn materialize_pi_extension(
    paths: &crate::paths::GxserverPaths,
) -> Result<PathBuf, String> {
    let dir = paths.root_dir.join("acp-extensions");
    fs::create_dir_all(&dir)
        .map_err(|err| format!("Failed creating Pi extension directory: {err}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    let target = dir.join("ghostex-pi-native-extension.ts");
    if let Ok(existing) = fs::read_to_string(&target) {
        if existing == GHOSTEX_PI_NATIVE_EXTENSION_SOURCE {
            return Ok(target);
        }
    }
    let staged = dir.join(format!(
        "ghostex-pi-native-extension.{}.tmp.ts",
        uuid::Uuid::new_v4()
    ));
    let mut opts = OpenOptions::new();
    opts.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&staged)
        .map_err(|err| format!("Failed creating staged Pi extension file: {err}"))?;
    file.write_all(GHOSTEX_PI_NATIVE_EXTENSION_SOURCE.as_bytes())
        .and_then(|_| file.flush())
        .map_err(|err| {
            let _ = fs::remove_file(&staged);
            format!("Failed writing staged Pi extension file: {err}")
        })?;
    drop(file);
    fs::rename(&staged, &target).map_err(|err| {
        let _ = fs::remove_file(&staged);
        format!("Failed installing Pi extension file: {err}")
    })?;
    Ok(target)
}
