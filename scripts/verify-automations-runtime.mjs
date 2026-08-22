#!/usr/bin/env node
import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..");
const CLI_PATH = path.join(REPO_ROOT, "server", "target", "debug", "ghostex");
const RUNNING_STATUSES = new Set(["queued", "running"]);
const FINAL_TIMEOUT_MS = 90_000;
const POLL_MS = 1_000;

const args = new Set(process.argv.slice(2));
const shouldRun =
  args.has("--yes") ||
  args.has("--run") ||
  process.env.GHOSTEX_VERIFY_AUTOMATIONS === "1";
const keepArtifacts = args.has("--keep");

if (!shouldRun) {
  console.error(
    [
      "This verifier mutates the Ghostex app state by adding temporary projects and a verifier agent.",
      "Start Ghostex first, then rerun with GHOSTEX_VERIFY_AUTOMATIONS=1 or --yes.",
      "Example:",
      "  bun run start",
      "  GHOSTEX_VERIFY_AUTOMATIONS=1 node scripts/verify-automations-runtime.mjs",
    ].join("\n"),
  );
  process.exit(2);
}

const runSlug = `automation-verifier-${Date.now().toString(36)}`;
const tempRoot = await mkdtemp(path.join(tmpdir(), `${runSlug}-`));
const gitProjectPath = path.join(tempRoot, "git-project");
const nonGitProjectPath = path.join(tempRoot, "non-git-project");
const verifierAgentId = "automation-verifier";
const worktreePathsToRemove = new Set();
const projectPathsToRemove = new Set([gitProjectPath, nonGitProjectPath]);

try {
  await createVerifierProjects();
  await assertDevBridgeReady();
  await saveVerifierAgent();

  const gitProject = await addProject(gitProjectPath, `${runSlug} Git`);
  const nonGitProject = await addProject(nonGitProjectPath, `${runSlug} Non-Git`);

  const localRun = await verifyLocalRun(gitProject);
  const worktreeRun = await verifyWorktreeRun(gitProject);
  const nonGitRun = await verifyNonGitWorktreeFailure(nonGitProject);
  const scheduledRun = await verifyScheduledRun(gitProject);

  await archiveWorktreeRun(gitProject, worktreeRun);

  console.log(
    JSON.stringify(
      {
        ok: true,
        runs: {
          local: summarizeRun(localRun),
          nonGitWorktree: summarizeRun(nonGitRun),
          scheduled: summarizeRun(scheduledRun),
          worktree: summarizeRun(worktreeRun),
        },
        tempRoot,
      },
      null,
      2,
    ),
  );
} finally {
  if (!keepArtifacts) {
    await cleanupVerifierArtifacts();
  } else {
    console.error(`Keeping verifier artifacts under ${tempRoot}`);
  }
}

async function createVerifierProjects() {
  await mkdir(gitProjectPath, { recursive: true });
  await mkdir(nonGitProjectPath, { recursive: true });
  await writeFile(path.join(gitProjectPath, "README.md"), "# Ghostex automation verifier\n");
  await writeFile(path.join(nonGitProjectPath, "README.md"), "# Ghostex automation verifier\n");
  await git(["init"], gitProjectPath);
  await git(["config", "user.email", "ghostex-verifier@example.invalid"], gitProjectPath);
  await git(["config", "user.name", "Ghostex Automation Verifier"], gitProjectPath);
  await git(["add", "README.md"], gitProjectPath);
  await git(["commit", "-m", "Initial verifier commit"], gitProjectPath);
}

async function assertDevBridgeReady() {
  try {
    await cli(["state", "--timeout", "3000"]);
  } catch (error) {
    throw new Error(
      `Ghostex bridge is not reachable. Start it with "bun run start" before running this verifier.\n${error.message}`,
    );
  }
}

async function saveVerifierAgent() {
  const command = [
    "exec /bin/zsh -lc '",
    "while IFS= read -r line; do",
    '  [ \"$line\" = \"Then include a short summary.\" ] && break;',
    "done;",
    'printf \"%s\\n%s\\n\" \"AUTOMATION_RESULT: no_findings\" \"Ghostex automation verifier completed.\";',
    "exit 0",
    "'",
  ].join(" ");
  const result = await cli([
    "save-agent",
    "--agent-id",
    verifierAgentId,
    "--name",
    "Automation Verifier",
    "--command",
    command,
    "--timeout",
    "10000",
  ]);
  assert(result.ok === true, "save-agent did not return ok=true");
}

async function addProject(projectPath, name) {
  const result = await cli(["add-project", projectPath, "--name", name, "--timeout", "15000"]);
  assert(result.ok === true, `add-project failed for ${projectPath}`);
  const project = findProjectByPath(result.state, projectPath);
  assert(project, `added project was not present in CLI state: ${projectPath}`);
  return project;
}

async function verifyLocalRun(project) {
  const automation = createDefinition(project, {
    id: `${runSlug}-local`,
    name: "Verifier Local",
    executionMode: { kind: "local" },
  });
  await saveAutomation(project, automation);
  const startedAt = Date.now();
  await cli([
    "automation-run-now",
    automation.id,
    "--path",
    project.path,
    "--timeout",
    "20000",
  ]);
  const run = await waitForAutomationRun(project.path, automation.id, startedAt);
  assert(run.status === "no_findings", `local run finished with ${run.status}`);
  assert(typeof run.sessionId === "string" && run.sessionId.length > 0, "local run did not store a session id");
  return run;
}

async function verifyWorktreeRun(project) {
  const automation = createDefinition(project, {
    id: `${runSlug}-worktree`,
    name: "Verifier Worktree",
    executionMode: { kind: "worktree" },
  });
  await saveAutomation(project, automation);
  const startedAt = Date.now();
  await cli([
    "automation-run-now",
    automation.id,
    "--path",
    project.path,
    "--timeout",
    "30000",
  ]);
  const run = await waitForAutomationRun(project.path, automation.id, startedAt);
  assert(run.status === "no_findings", `worktree run finished with ${run.status}`);
  assert(run.worktree?.path, "worktree run did not store worktree metadata");
  worktreePathsToRemove.add(run.worktree.path);
  projectPathsToRemove.add(run.worktree.path);
  const worktreeList = await git(["worktree", "list", "--porcelain"], project.path);
  assert(
    worktreeList.stdout.includes(run.worktree.path),
    `git worktree list does not include ${run.worktree.path}`,
  );
  return run;
}

async function verifyNonGitWorktreeFailure(project) {
  const automation = createDefinition(project, {
    id: `${runSlug}-non-git-worktree`,
    name: "Verifier Non-Git Worktree",
    executionMode: { kind: "worktree" },
  });
  await saveAutomation(project, automation);
  const startedAt = Date.now();
  await cli([
    "automation-run-now",
    automation.id,
    "--path",
    project.path,
    "--timeout",
    "20000",
  ]);
  const run = await waitForAutomationRun(project.path, automation.id, startedAt);
  assert(
    run.status === "failed" || run.status === "needs_attention",
    `non-Git worktree run should fail or need attention, received ${run.status}`,
  );
  assert(!run.sessionId, "non-Git worktree failure silently launched a local session");
  assert(!run.worktree, "non-Git worktree failure should not store worktree metadata");
  return run;
}

async function verifyScheduledRun(project) {
  const automation = createDefinition(project, {
    enabled: true,
    id: `${runSlug}-scheduled`,
    name: "Verifier Scheduled",
    executionMode: { kind: "local" },
    nextRunAt: new Date(Date.now() + 2_500).toISOString(),
  });
  const startedAt = Date.now();
  await saveAutomation(project, automation);
  const run = await waitForAutomationRun(project.path, automation.id, startedAt);
  assert(run.status === "no_findings", `scheduled run finished with ${run.status}`);
  return run;
}

async function archiveWorktreeRun(project, run) {
  if (!run.worktree?.path) {
    return;
  }
  await cli([
    "automation-archive-run",
    "--path",
    project.path,
    "--run-id",
    run.id,
    "--remove-worktree",
    "true",
    "--timeout",
    "30000",
  ]);
  const worktreeList = await git(["worktree", "list", "--porcelain"], project.path);
  assert(
    !worktreeList.stdout.includes(run.worktree.path),
    `archive cleanup did not remove worktree ${run.worktree.path}`,
  );
  worktreePathsToRemove.delete(run.worktree.path);
}

async function saveAutomation(project, automation) {
  const result = await cli([
    "automation-save",
    "--path",
    project.path,
    "--definition-json",
    JSON.stringify(automation),
    "--timeout",
    "20000",
  ]);
  assert(result.ok === true, `automation-save failed for ${automation.id}`);
  const saved = result.automationState?.automations?.find((candidate) => candidate.id === automation.id);
  assert(saved, `automation-save did not return ${automation.id}`);
}

async function waitForAutomationRun(projectPath, automationId, startedAtMs) {
  const deadline = Date.now() + FINAL_TIMEOUT_MS;
  let lastRun;
  while (Date.now() < deadline) {
    const state = await cli(["automation-state", "--path", projectPath, "--timeout", "10000"]);
    const run = latestRun(state.automationState?.runs ?? [], automationId, startedAtMs);
    if (run) {
      lastRun = run;
      if (!RUNNING_STATUSES.has(run.status)) {
        return run;
      }
    }
    await sleep(POLL_MS);
  }
  throw new Error(
    `Timed out waiting for automation ${automationId}; last status was ${lastRun?.status ?? "<none>"}`,
  );
}

function createDefinition(project, overrides) {
  const now = new Date().toISOString();
  return {
    agentId: verifierAgentId,
    createdAt: now,
    enabled: false,
    executionMode: { kind: "local" },
    id: `${runSlug}-automation`,
    name: "Verifier Automation",
    prompt: `Run the Ghostex automation verifier for ${project.name}.`,
    projectIds: [project.projectId],
    schedule: { everyMs: 60 * 60 * 1000, kind: "interval" },
    updatedAt: now,
    ...overrides,
  };
}

function latestRun(runs, automationId, startedAtMs) {
  return runs
    .filter((run) => run.automationId === automationId)
    .filter((run) => Date.parse(run.createdAt) >= startedAtMs - 1_000)
    .sort((left, right) => Date.parse(right.createdAt) - Date.parse(left.createdAt))[0];
}

function summarizeRun(run) {
  return {
    completedAt: run.completedAt,
    id: run.id,
    sessionId: run.sessionId,
    status: run.status,
    worktree: run.worktree,
  };
}

function findProjectByPath(state, projectPath) {
  const normalized = normalizePath(projectPath);
  return state?.projects?.find((project) => normalizePath(project.path) === normalized);
}

function normalizePath(value) {
  return String(value ?? "").replace(/\/+$/u, "");
}

async function cleanupVerifierArtifacts() {
  for (const worktreePath of [...worktreePathsToRemove]) {
    try {
      if (existsSync(worktreePath)) {
        await git(["worktree", "remove", "--force", worktreePath], gitProjectPath);
      }
    } catch (error) {
      console.error(`Could not remove verifier worktree ${worktreePath}: ${error.message}`);
    }
  }
  for (const projectPath of [...projectPathsToRemove].reverse()) {
    try {
      if (!(await hasProject(projectPath))) {
        continue;
      }
      await cli(["remove-project", "--path", projectPath, "--timeout", "15000"]);
    } catch (error) {
      console.error(`Could not remove verifier project ${projectPath}: ${error.message}`);
    }
  }
  await rm(tempRoot, { force: true, recursive: true });
}

async function cli(cliArgs) {
  const result = await execFileAsync(CLI_PATH, cliArgs, {
    cwd: REPO_ROOT,
    env: {
      ...process.env,
      GHOSTEX_APP_VARIANT: "prod",
    },
    maxBuffer: 20 * 1024 * 1024,
    timeout: 120_000,
  });
  try {
    return JSON.parse(result.stdout);
  } catch {
    throw new Error(`CLI did not return JSON for ${cliArgs.join(" ")}:\n${result.stdout}`);
  }
}

async function hasProject(projectPath) {
  try {
    const state = await cli(["state", "--timeout", "10000"]);
    return Boolean(findProjectByPath(state.state, projectPath));
  } catch {
    return false;
  }
}

async function git(gitArgs, cwd) {
  return execFileAsync("git", gitArgs, {
    cwd,
    maxBuffer: 10 * 1024 * 1024,
    timeout: 60_000,
  });
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
