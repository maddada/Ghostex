import { quoteShellDoubleArg } from "./shell-quote";

/**
 * CDXC:CursorCLI 2026-05-20-08:20:
 * Cursor Agent resumes by chat UUID (`cursor-agent --resume <id>`), not by
 * sidebar title. Ghostex-created sessions store the UUID from `create-chat`;
 * externally started sessions resolve the latest matching chat `name` from the
 * project-scoped Cursor SQLite store under `~/.cursor/chats/<md5(projectPath)>`.
 */

export const CURSOR_CHAT_SESSION_ID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/iu;

export function isCursorChatSessionId(value: string | undefined): value is string {
  const normalizedValue = value?.trim();
  return Boolean(normalizedValue && CURSOR_CHAT_SESSION_ID_PATTERN.test(normalizedValue));
}

export function getCursorChatSessionIdFromIdentity(value: string | undefined): string | undefined {
  const normalizedValue = value?.trim();
  if (!normalizedValue) {
    return undefined;
  }
  if (CURSOR_CHAT_SESSION_ID_PATTERN.test(normalizedValue)) {
    return normalizedValue.toLowerCase();
  }
  const transcriptMatch = normalizedValue.match(
    /(?:^|[/\\])agent-transcripts(?:[/\\])([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})(?:[/\\])/iu,
  );
  return transcriptMatch?.[1]?.toLowerCase();
}

/**
 * CDXC:CursorCLI 2026-05-27-09:06:
 * Cursor Agent hooks persist transcript paths under Cursor's project-scoped
 * `agent-transcripts` directory. Treat that path shape as authoritative Cursor
 * identity when older records have a stale inherited agent name.
 */
export function isCursorAgentTranscriptPath(value: string | undefined): boolean {
  const normalizedValue = value?.trim();
  return Boolean(
    normalizedValue &&
      /(?:^|[/\\])\.cursor(?:[/\\])projects(?:[/\\]).+(?:[/\\])agent-transcripts(?:[/\\])/iu.test(
        normalizedValue,
      ),
  );
}

/**
 * CDXC:CursorCLI 2026-05-20-08:20:
 * Accept-all and other launch flags stay on the configured base command; append
 * `--resume` after them so Ghostex-owned sessions always attach to the chat id
 * created at launch time.
 */
export function appendCursorCliResumeFlag(agentCommand: string, chatId: string): string {
  const normalizedCommand = agentCommand.trim();
  const normalizedChatId = chatId.trim();
  if (!normalizedCommand || !isCursorChatSessionId(normalizedChatId)) {
    return normalizedCommand;
  }
  return `${normalizedCommand} --resume ${quoteShellDoubleArg(normalizedChatId)}`;
}

/**
 * CDXC:CursorCLI 2026-06-10-18:17:
 * Native full-reload and copy-resume run this script through Ghostex's bundled
 * Node runtime, not Python or system sqlite. It scans the current project's
 * Cursor chat store and returns the newest chat whose `name` matches.
 */
export function getCursorChatSessionLookupScript(): string {
  return `const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { DatabaseSync } = require("node:sqlite");

const [projectPath = "", title = ""] = process.argv.slice(1).map((value) => String(value || "").trim());
if (!projectPath || !title) {
  process.exit(1);
}

const projectHash = crypto.createHash("md5").update(projectPath).digest("hex");
const chatsDir = path.join(os.homedir(), ".cursor", "chats", projectHash);
let chatDirs;
try {
  chatDirs = fs.readdirSync(chatsDir, { withFileTypes: true });
} catch {
  process.exit(1);
}

function parseMetaValue(raw) {
  const value = String(raw || "").trim();
  if (value.startsWith("{")) {
    return JSON.parse(value);
  }
  return JSON.parse(Buffer.from(value, "hex").toString("utf8"));
}

const matches = [];
for (const chatDir of chatDirs) {
  if (!chatDir.isDirectory()) {
    continue;
  }
  const dbPath = path.join(chatsDir, chatDir.name, "store.db");
  if (!fs.existsSync(dbPath)) {
    continue;
  }
  let db;
  let rows;
  try {
    db = new DatabaseSync(dbPath, { readOnly: true });
    rows = db.prepare("select value from meta").all();
  } catch {
    rows = [];
  } finally {
    try {
      db?.close();
    } catch {}
  }
  for (const row of rows) {
    let meta;
    try {
      meta = parseMetaValue(row.value);
    } catch {
      continue;
    }
    if (String(meta.name || "").trim() !== title) {
      continue;
    }
    const chatId = String(meta.agentId || chatDir.name).trim();
    if (!chatId) {
      continue;
    }
    const createdAt = Number(meta.createdAt || 0);
    matches.push({ chatId, createdAt: Number.isFinite(createdAt) ? createdAt : 0 });
  }
}

if (!matches.length) {
  process.exit(1);
}

matches.sort((left, right) => left.createdAt - right.createdAt);
process.stdout.write(matches[matches.length - 1].chatId);
`;
}
