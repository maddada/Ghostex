import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, test } from "vitest";
import {
  appendCursorCliResumeFlag,
  getCursorChatSessionIdFromIdentity,
  getCursorChatSessionLookupScript,
  isCursorAgentTranscriptPath,
  isCursorChatSessionId,
} from "./cursor-cli-session";

const testNodeRuntime = process.env.GHOSTEX_TEST_NODE || "node";

type BunSpawnSync = (options: {
  cmd: string[];
  env?: Record<string, string>;
  stderr?: "pipe";
  stdout?: "pipe";
}) => {
  exitCode: number;
  stderr: Uint8Array;
  stdout: Uint8Array;
};

function runNodeScript(
  script: string,
  args: readonly string[],
  env: NodeJS.ProcessEnv = {},
): string {
  /*
  CDXC:CursorCLI 2026-06-10-18:17:
  The Cursor chat lookup uses Node's built-in sqlite module in production through Ghostex's bundled code-server Node. Tests run under Bun, so invoke Node as a subprocess instead of importing `node:sqlite` into the test runner.
  */
  const childEnv = Object.fromEntries(
    Object.entries({ ...process.env, ...env }).filter((entry): entry is [string, string] => entry[1] !== undefined),
  );
  const outputDir = mkdtempSync(join(tmpdir(), "ghostex-node-script-output-"));
  const outputPath = join(outputDir, "stdout.txt");
  const wrappedScript = [
    'const __ghostexFs = require("node:fs");',
    'const __ghostexStdoutPath = process.env.GHOSTEX_NODE_STDOUT_FILE;',
    "const __ghostexStdoutChunks = [];",
    "process.stdout.write = (chunk, encoding, callback) => {",
    "  const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(String(chunk), typeof encoding === 'string' ? encoding : 'utf8');",
    "  __ghostexStdoutChunks.push(buffer);",
    "  if (typeof encoding === 'function') encoding();",
    "  if (typeof callback === 'function') callback();",
    "  return true;",
    "};",
    "process.on('exit', () => {",
    "  if (__ghostexStdoutPath) __ghostexFs.writeFileSync(__ghostexStdoutPath, Buffer.concat(__ghostexStdoutChunks));",
    "});",
    script,
  ].join("\n");
  const command = [testNodeRuntime, "--no-warnings", "-e", wrappedScript, "--", ...args];
  try {
    const bun = (globalThis as typeof globalThis & { Bun?: { spawnSync?: BunSpawnSync } }).Bun;
    const subprocessEnv = { ...childEnv, GHOSTEX_NODE_STDOUT_FILE: outputPath };
    if (bun?.spawnSync) {
      const result = bun.spawnSync({
        cmd: command,
        env: subprocessEnv,
        stderr: "pipe",
        stdout: "pipe",
      });
      if (result.exitCode !== 0) {
        throw new Error(new TextDecoder().decode(result.stderr) || `Node subprocess exited ${result.exitCode}`);
      }
    } else {
      execFileSync(testNodeRuntime, ["--no-warnings", "-e", wrappedScript, "--", ...args], {
        encoding: "utf8",
        env: subprocessEnv,
      });
    }
    return readFileSync(outputPath, "utf8");
  } finally {
    rmSync(outputDir, { force: true, recursive: true });
  }
}

describe("cursor-cli-session", () => {
  test("should validate Cursor chat UUID identities", () => {
    expect(isCursorChatSessionId("c62d8f4d-e93b-4932-817e-1eefd9188de4")).toBe(true);
    expect(isCursorChatSessionId("Cursor and Antigravity CLI Agent Detection")).toBe(false);
    expect(getCursorChatSessionIdFromIdentity("C62D8F4D-E93B-4932-817E-1EEFD9188DE4")).toBe(
      "c62d8f4d-e93b-4932-817e-1eefd9188de4",
    );
    expect(
      getCursorChatSessionIdFromIdentity(
        "/Users/madda/.cursor/projects/Users-madda-dev-active-zmux/agent-transcripts/C62D8F4D-E93B-4932-817E-1EEFD9188DE4/C62D8F4D-E93B-4932-817E-1EEFD9188DE4.jsonl",
      ),
    ).toBe("c62d8f4d-e93b-4932-817e-1eefd9188de4");
  });

  test("should append --resume after the configured launch command", () => {
    expect(appendCursorCliResumeFlag("cursor-agent --yolo", "c62d8f4d-e93b-4932-817e-1eefd9188de4")).toBe(
      'cursor-agent --yolo --resume "c62d8f4d-e93b-4932-817e-1eefd9188de4"',
    );
  });

  test("should identify Cursor transcript paths", () => {
    expect(
      isCursorAgentTranscriptPath(
        "/Users/madda/.cursor/projects/Users-madda-dev-active-zmux/agent-transcripts/9a81fa27-6dcf-49dd-be3f-802169e708e2/9a81fa27-6dcf-49dd-be3f-802169e708e2.jsonl",
      ),
    ).toBe(true);
    expect(
      isCursorAgentTranscriptPath(
        "/Users/madda/.codex/sessions/2026/05/27/rollout-2026-05-27T09-05-02-019e67d2-6482-7461-b1bf-32cb31d27f0d.jsonl",
      ),
    ).toBe(false);
  });

  test("should resolve the latest matching chat id for a project title", () => {
    expect(runNodeScript('console.log("ok")', []).trim()).toBe("ok");
    const tempHome = mkdtempSync(join(tmpdir(), "ghostex-cursor-home-"));
    const projectPath = join(tempHome, "project");
    const projectHash = createHash("md5").update(projectPath).digest("hex");
    const chatsRoot = join(tempHome, ".cursor", "chats", projectHash);
    const olderChatId = "11111111-1111-4111-8111-111111111111";
    const newerChatId = "22222222-2222-4222-8222-222222222222";
    const title = "Ghostex Cursor Resume Lookup";

    const writeChat = (chatId: string, createdAt: number) => {
      const chatDir = join(chatsRoot, chatId);
      mkdirSync(chatDir, { recursive: true });
      const dbPath = join(chatDir, "store.db");
      const meta = JSON.stringify({
        agentId: chatId,
        createdAt,
        name: title,
      });
      runNodeScript(
        [
          'const { DatabaseSync } = require("node:sqlite");',
          "const [dbPath, meta] = process.argv.slice(1);",
          "const db = new DatabaseSync(dbPath);",
          "try {",
          '  db.exec("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT)");',
          '  db.prepare("INSERT INTO meta VALUES (?, ?)").run("0", meta);',
          "} finally {",
          "  db.close();",
          "}",
        ].join("\n"),
        [dbPath, meta],
        { HOME: tempHome },
      );
    };

    mkdirSync(chatsRoot, { recursive: true });
    writeChat(olderChatId, 100);
    writeChat(newerChatId, 200);

    try {
      const fixtureRows = JSON.parse(
        runNodeScript(
          [
            'const fs = require("node:fs");',
            'const path = require("node:path");',
            'const { DatabaseSync } = require("node:sqlite");',
            "const [chatsRoot] = process.argv.slice(1);",
            "const rows = [];",
            "for (const chatId of fs.readdirSync(chatsRoot).sort()) {",
            '  const db = new DatabaseSync(path.join(chatsRoot, chatId, "store.db"), { readOnly: true });',
            "  try {",
            '    rows.push(...db.prepare("select value from meta").all().map((row) => JSON.parse(row.value)));',
            "  } finally {",
            "    db.close();",
            "  }",
            "}",
            "console.log(JSON.stringify(rows));",
          ].join("\n"),
          [chatsRoot],
          { HOME: tempHome },
        ),
      ) as Array<{ agentId?: string; name?: string }>;
      expect(fixtureRows.map((row) => row.agentId)).toEqual([olderChatId, newerChatId]);
      expect(fixtureRows.every((row) => row.name === title)).toBe(true);
      expect(getCursorChatSessionLookupScript()).toContain("DatabaseSync");
      const resolvedChatId = runNodeScript(
        getCursorChatSessionLookupScript(),
        [projectPath, title],
        { HOME: tempHome },
      ).trim();
      expect(resolvedChatId).toBe(newerChatId);
    } finally {
      rmSync(tempHome, { force: true, recursive: true });
    }
  });
});
