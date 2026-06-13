import test from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync, type ChildProcessWithoutNullStreams } from "node:child_process";
import http from "node:http";
import { once } from "node:events";
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { WebSocket } from "ws";
import {
  GXSERVER_LOCAL_API_PORT,
  GXSERVER_PROTOCOL_VERSION,
  type GxserverAuthToken,
  type GxserverRuntimeMetadata,
} from "../protocol/index.js";
import { GXSERVER_PROTOCOL_HEADER } from "../src/api.js";
import { ensureGxserverAuthToken } from "../src/auth.js";
import { GxserverEventHub } from "../src/events.js";
import { LEGACY_MACOS_STATE_IMPORT_ID } from "../src/legacy-macos-state-migration.js";
import { getGxserverStatus } from "../src/lifecycle.js";
import { createGxserverLogger } from "../src/logger.js";
import { getGxserverPaths } from "../src/paths.js";
import type { GxserverPaths } from "../src/paths.js";
import {
  createGxserverHttpServer,
  GXSERVER_JSON_BODY_LIMIT_BYTES,
  type GxserverApiRuntime,
} from "../src/server.js";
import {
  GXSERVER_DOMAIN_STATE_JSON_LIMIT_CHARS,
  GXSERVER_DOMAIN_STATE_JSON_MAX_DEPTH,
} from "../src/domain-state.js";
import {
  createGxserverMigrationStatus,
  initializeGxserverStorage,
  openGxserverDatabase,
  readGxserverConfig,
  type GxserverConfig,
  writeGxserverConfig,
} from "../src/storage.js";
import {
  GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES,
  GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES,
  type GxserverZmxCommandRunner,
} from "../src/zmx-lifecycle.js";

test("minimal health is unauthenticated and non-health APIs require auth", async () => {
  await withApiServer("local", async ({ baseUrl }) => {
    const health = await requestJson(baseUrl, "/api/health", { method: "GET" });
    assert.equal(health.status, 200);
    assert.deepEqual(health.body, {
      ok: true,
      product: "gxserver",
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      version: "0.1.0-test",
    });

    const protectedResponse = await requestJson(baseUrl, "/api/listSessions", { method: "POST" });
    assert.equal(protectedResponse.status, 401);
    assert.equal(protectedResponse.body.error, "unauthorized");
  });
});

test("agent settings API owns inherited Accept All launch policy", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    const project = (
      await requestJson(baseUrl, "/api/createProject", {
        body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      })
    ).body.result.project;

    const initialSettings = await requestJson(baseUrl, "/api/readAgentSettings", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(initialSettings.status, 200);
    assert.equal(initialSettings.body.result.isPersisted, false);
    assert.equal(initialSettings.body.result.settings.agentAcceptAllEnabled, true);

    const initialPlan = await requestJson(baseUrl, "/api/readAgentLaunchPlan", {
      body: {
        params: { agentId: "codex", projectId: project.projectId },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(initialPlan.body.result.plan.command, "codex --yolo");

    const updatedSettings = await requestJson(baseUrl, "/api/updateAgentSettings", {
      body: {
        params: { agentAcceptAllEnabled: false },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(updatedSettings.status, 200);
    assert.equal(updatedSettings.body.result.settings.agentAcceptAllEnabled, false);

    const persistedSettings = await requestJson(baseUrl, "/api/readAgentSettings", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(persistedSettings.body.result.isPersisted, true);
    assert.equal(persistedSettings.body.result.settings.agentAcceptAllEnabled, false);

    const updatedPlan = await requestJson(baseUrl, "/api/readAgentLaunchPlan", {
      body: {
        params: { agentId: "codex", projectId: project.projectId },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(updatedPlan.body.result.plan.command, "codex");
  });
});

test("forkSession creates and starts a gxserver-owned Codex fork", async () => {
  const zmxCalls: string[] = [];
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    const project = (
      await requestJson(baseUrl, "/api/createProject", {
        body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      })
    ).body.result.project;
    const source = (
      await requestJson(baseUrl, "/api/createAgentSession", {
        body: {
          params: {
            agentId: "codex",
            projectId: project.projectId,
            runtimeSettings: {
              agentName: "codex",
              agentSessionId: "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56",
              titleSource: "user",
            },
            title: "Existing Codex task",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      })
    ).body.result.session;

    const forkResponse = await requestJson(baseUrl, "/api/forkSession", {
      body: {
        params: { projectId: project.projectId, sessionId: source.sessionId },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });

    assert.equal(forkResponse.status, 200);
    const fork = forkResponse.body.result.fork;
    assert.equal(fork.session.agentId, "codex");
    assert.notEqual(fork.session.sessionId, source.sessionId);
    assert.equal(fork.session.hiddenMetadata.restoredFromSessionId, source.sessionId);
    assert.equal(fork.session.launchSettings.forkedFromSessionId, source.sessionId);
    assert.equal(fork.session.runtimeSettings.forkedFromSessionId, source.sessionId);
    assert.equal(fork.provider.providerState.lifecycleState, "exists");
    assert.equal(fork.provider.session.providerState.lifecycleState, "exists");
    assert.equal(fork.session.providerState.lifecycleState, "exists");
    assert.equal(
      fork.session.launchSettings.agentLaunchPlan.command,
      'codex --yolo fork "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56"',
    );
    const readFork = await requestJson(baseUrl, "/api/readProjectStatus", {
      body: {
        params: { projectId: project.projectId },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const persistedFork = readFork.body.result.sessions.find(
      (session: { sessionId: string }) => session.sessionId === fork.session.sessionId,
    );
    assert.equal(persistedFork?.providerState.lifecycleState, "exists");
    assert.ok(zmxCalls.some((script) => script.includes('codex --yolo fork "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56"')));
  }, {
    zmxLifecycle: fakeZmxLifecycle(zmxCalls, () => 1),
  });
});

test("foreground gxserver process uses temporary HOME, writes daemon state, and stops only the control plane", async (t) => {
  /*
  CDXC:GxserverVerification 2026-05-30-18:37:
  Cutover verification needs at least one real gxserver foreground-process fixture with an isolated HOME, not only in-process HTTP handlers. The smoke proves auth token creation, SQLite/log initialization, authenticated health, and control-plane stop without touching the user's normal daemon state.
  */
  if (!(await isFixedLocalPortAvailable())) {
    t.skip(`127.0.0.1:${GXSERVER_LOCAL_API_PORT} is already in use; skipping the real foreground gxserver process fixture.`);
    return;
  }

  const homeDir = await mkdtemp(path.join(tmpdir(), "gxserver-foreground-home-"));
  const paths = getGxserverPaths(homeDir);
  const cliPath = path.resolve("dist/src/cli.js");
  const child = spawn(process.execPath, [cliPath, "--foreground"], {
    cwd: path.resolve("."),
    env: { ...process.env, HOME: homeDir },
  });
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => {
    stdout += String(chunk);
  });
  child.stderr.on("data", (chunk) => {
    stderr += String(chunk);
  });

  try {
    const token = (await waitForFileText(paths.authTokenFile, 5_000)).trim() as GxserverAuthToken;
    const health = await waitForHealth(token, 5_000);
    assert.equal(health.serverId.startsWith("S"), true);
    assert.equal(health.migration.currentVersion, 9);
    assert.equal(health.listeners.local.port, GXSERVER_LOCAL_API_PORT);

    const stop = await requestJson(`http://127.0.0.1:${GXSERVER_LOCAL_API_PORT}`, "/api/control/stop", {
      body: { protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      token,
    });
    assert.equal(stop.status, 200);
    await waitForProcessExit(child, 5_000);

    const logs = await readFile(paths.logFile, "utf8");
    assert.match(logs, /"event":"serverStarted"/);
    assert.match(await readFile(paths.runtimeMetadataFile, "utf8").catch(() => ""), /^$/);
    assert.equal(stdout.trim(), "");
  } finally {
    if (child.exitCode === null) {
      child.kill("SIGTERM");
      await waitForProcessExit(child, 2_000).catch(() => child.kill("SIGKILL"));
    }
    assert.equal(stderr.trim(), "", stderr);
    await rm(homeDir, { force: true, recursive: true });
  }
});

test("foreground gxserver closes local listener when remote listener bind fails", async (t) => {
  /*
  CDXC:GxserverVerification 2026-05-30-20:09:
  Remote listener startup can fail after the local listener is bound. The foreground-process regression must prove gxserver exits without runtime metadata and releases the local listener so CLI status cannot report a half-started daemon.

  CDXC:GxserverVerification 2026-05-30-23:34:
  This regression uses the fixed local listener because config.json is not allowed to move the local API away from 127.0.0.1:58744. Skip when the user's daemon already owns the fixed port instead of creating a test-only alternate local port.
  */
  if (!(await isFixedLocalPortAvailable())) {
    t.skip(`127.0.0.1:${GXSERVER_LOCAL_API_PORT} is already in use; skipping the real remote-bind cleanup fixture.`);
    return;
  }

  const remoteBlocker = http.createServer();
  remoteBlocker.listen(0, "127.0.0.1");
  await once(remoteBlocker, "listening");
  const remoteAddress = remoteBlocker.address();
  if (typeof remoteAddress !== "object" || remoteAddress === null || !("port" in remoteAddress)) {
    throw new Error("Expected remote port blocker to listen on a TCP port.");
  }

  const homeDir = await mkdtemp(path.join(tmpdir(), "gxserver-remote-bind-failure-home-"));
  const paths = getGxserverPaths(homeDir);
  const config = await readGxserverConfig(paths);
  await writeGxserverConfig(paths, {
    ...config,
    listeners: {
      local: {
        ...config.listeners.local,
      },
      remote: {
        ...config.listeners.remote,
        enabled: true,
        host: "127.0.0.1",
        port: remoteAddress.port,
      },
    },
  });

  const cliPath = path.resolve("dist/src/cli.js");
  const child = spawn(process.execPath, [cliPath, "--foreground"], {
    cwd: path.resolve("."),
    env: { ...process.env, HOME: homeDir },
  });
  let stderr = "";
  child.stderr.on("data", (chunk) => {
    stderr += String(chunk);
  });

  try {
    await waitForProcessExit(child, 5_000);
    assert.notEqual(child.exitCode, 0);
    assert.match(stderr, new RegExp(`Port ${remoteAddress.port} is already in use`));
    assert.equal(await isTcpPortAvailable(GXSERVER_LOCAL_API_PORT), true);
    assert.match(await readFile(paths.runtimeMetadataFile, "utf8").catch(() => ""), /^$/);
    assert.equal((await getGxserverStatus({ homeDir, version: "0.1.0-test" })).state, "stopped");
  } finally {
    if (child.exitCode === null) {
      child.kill("SIGTERM");
      await waitForProcessExit(child, 2_000).catch(() => child.kill("SIGKILL"));
    }
    await closeServer(remoteBlocker);
    await rm(homeDir, { force: true, recursive: true });
  }
});

test("RPC endpoints require POST and the exact gxserver protocol version", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const preflight = await requestJson(baseUrl, "/api/attachSessionMetadata", {
      method: "OPTIONS",
      origin: "null",
      requestPrivateNetwork: true,
    });
    assert.equal(preflight.status, 204);
    assert.equal(preflight.body, undefined);
    assert.equal(preflight.headers.get("access-control-allow-origin"), "null");
    assert.equal(preflight.headers.get("access-control-allow-private-network"), "true");
    assert.match(preflight.headers.get("access-control-allow-headers") ?? "", /authorization/);
    assert.match(preflight.headers.get("access-control-allow-headers") ?? "", /x-gxserver-protocol-version/);

    const getResponse = await requestJson(baseUrl, "/api/listSessions", {
      method: "GET",
      token,
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
    });
    assert.equal(getResponse.status, 405);
    assert.equal(getResponse.body.error, "methodNotAllowed");

    const missingProtocol = await requestJson(baseUrl, "/api/listSessions", {
      method: "POST",
      token,
    });
    assert.equal(missingProtocol.status, 426);
    assert.equal(missingProtocol.body.error, "protocolMismatch");
    assert.match(missingProtocol.body.message, /Update Ghostex and gxserver/);

    const wrongProtocol = await requestJson(baseUrl, "/api/listSessions", {
      method: "POST",
      protocolVersion: 999,
      token,
    });
    assert.equal(wrongProtocol.status, 426);
    assert.equal(wrongProtocol.body.error, "protocolMismatch");

    const bodyProtocol = await requestJson(baseUrl, "/api/listSessions", {
      body: { protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(bodyProtocol.status, 200);
    assert.deepEqual(bodyProtocol.body.result.sessions, []);
  });
});

test("CORS and private-network headers are limited to trusted browser origins", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const trustedDev = await requestJson(baseUrl, "/api/listSessions", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      origin: "http://localhost:5173",
      token,
    });
    assert.equal(trustedDev.status, 200);
    assert.equal(trustedDev.headers.get("access-control-allow-origin"), "http://localhost:5173");
    assert.equal(trustedDev.headers.get("access-control-allow-private-network"), null);

    const cli = await requestJson(baseUrl, "/api/listSessions", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(cli.status, 200);
    assert.equal(cli.headers.get("access-control-allow-origin"), null);
    assert.equal(cli.headers.get("access-control-allow-private-network"), null);

    const disallowedActual = await requestJson(baseUrl, "/api/listSessions", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      origin: "https://evil.example",
      token,
    });
    assert.equal(disallowedActual.status, 200);
    assert.equal(disallowedActual.headers.get("access-control-allow-origin"), null);
    assert.equal(disallowedActual.headers.get("access-control-allow-private-network"), null);

    const disallowedPreflight = await requestJson(baseUrl, "/api/attachSessionMetadata", {
      method: "OPTIONS",
      origin: "https://evil.example",
      requestPrivateNetwork: true,
    });
    assert.equal(disallowedPreflight.status, 204);
    assert.equal(disallowedPreflight.headers.get("access-control-allow-origin"), null);
    assert.equal(disallowedPreflight.headers.get("access-control-allow-private-network"), null);
    assert.equal(disallowedPreflight.headers.get("access-control-allow-headers"), null);
  });
});

test("configured CORS origins can opt in trusted gxserver browser clients", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl }) => {
      const preflight = await requestJson(baseUrl, "/api/listSessions", {
        method: "OPTIONS",
        origin: "https://trusted.example",
        requestPrivateNetwork: true,
      });
      assert.equal(preflight.status, 204);
      assert.equal(preflight.headers.get("access-control-allow-origin"), "https://trusted.example");
      assert.equal(preflight.headers.get("access-control-allow-private-network"), "true");
    },
    {
      configureConfig: (config) => ({
        ...config,
        cors: {
          allowedOrigins: [...config.cors.allowedOrigins, "https://trusted.example"],
        },
      }),
    },
  );
});

test("RPC JSON body parser accepts normal bodies and rejects oversized bodies", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const normal = await requestJson(baseUrl, "/api/listSessions", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(normal.status, 200);
    assert.deepEqual(normal.body.result.sessions, []);

    const oversizedBody = JSON.stringify({
      params: { padding: "x".repeat(GXSERVER_JSON_BODY_LIMIT_BYTES) },
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
    });
    assert.equal(Buffer.byteLength(oversizedBody) > GXSERVER_JSON_BODY_LIMIT_BYTES, true);

    const oversized = await requestRawJson(baseUrl, "/api/listSessions", {
      bodyText: oversizedBody,
      method: "POST",
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      token,
    });
    assert.equal(oversized.status, 413);
    assert.equal(oversized.body.error, "badRequest");
    assert.match(oversized.body.message, /JSON RPC limit/);
  });
});

test("local listener has full API while remote listener blocks dangerous local-only operations", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const localRunProcess = await requestJson(baseUrl, "/api/runProcess", {
      body: {},
      method: "POST",
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      token,
    });
    assert.equal(localRunProcess.status, 404);
    assert.equal(localRunProcess.body.error, "notFound");
  });

  await withApiServer("remote", async ({ baseUrl, token }) => {
    const remoteRunProcess = await requestJson(baseUrl, "/api/runProcess", {
      body: {},
      method: "POST",
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      token,
    });
    assert.equal(remoteRunProcess.status, 404);
    assert.equal(remoteRunProcess.body.error, "notFound");

    const remoteQueryLogs = await requestJson(baseUrl, "/api/queryLogs", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      token,
    });
    assert.equal(remoteQueryLogs.status, 403);
    assert.equal(remoteQueryLogs.body.error, "forbidden");

    const remoteResolveGitRoot = await requestJson(baseUrl, "/api/resolveGitRootForPath", {
      body: { params: { path: "/tmp" }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      token,
    });
    assert.equal(remoteResolveGitRoot.status, 403);
    assert.equal(remoteResolveGitRoot.body.error, "forbidden");

    const remoteListSessions = await requestJson(baseUrl, "/api/listSessions", {
      body: {},
      method: "POST",
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      token,
    });
    assert.equal(remoteListSessions.status, 200);
    assert.deepEqual(remoteListSessions.body.result.sessions, []);
  });
});

test("project and session domain-state APIs create, update, and list shared domain state", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const createdProject = await requestJson(baseUrl, "/api/createProject", {
      body: {
        params: {
          customAgentOrder: ["codex-pro"],
          customAgents: [{ agentId: "codex-pro", command: "codex --profile pro", name: "Codex Pro" }],
          customCommandOrder: ["setup"],
          customCommands: [{ command: "npm install", commandId: "setup", name: "Setup" }],
          isFavorite: true,
          isPinned: true,
          launchSettings: { acceptAll: true },
          name: "Ghostex",
          path: "/repo/ghostex",
          previousSessionHistory: [{ historyId: "hist-1", primaryTitle: "Old run" }],
          runtimeSettings: { defaultPromptAgentId: "codex" },
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(createdProject.status, 200);
    const project = createdProject.body.result.project;
    assert.match(project.projectId, /^P[0-9][a-z0-9]{3}$/);
    assert.equal(project.isPinned, true);
    assert.equal(project.customAgents[0].agentId, "codex-pro");

    const originalSession = await requestJson(baseUrl, "/api/createAgentSession", {
      body: {
        params: {
          agentId: "codex",
          projectId: project.projectId,
          providerState: { lifecycleState: "exists" },
          title: "Original title",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(originalSession.status, 200);
    const original = originalSession.body.result.session;
    assert.match(original.sessionId, /^G[0-9][a-z0-9]{3}$/);
    assert.equal(original.zmxName, `S7k-${project.projectId}-${original.sessionId}`);

    const restoredSession = await requestJson(baseUrl, "/api/createSession", {
      body: {
        params: {
          isPinned: true,
          projectId: project.projectId,
          restoredFromHistoryId: "hist-1",
          restoredFromSessionId: original.sessionId,
          title: "Restored title stays user-owned",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(restoredSession.status, 200);
    const restored = restoredSession.body.result.session;
    assert.notEqual(restored.sessionId, original.sessionId);
    assert.equal(restored.title, "Restored title stays user-owned");
    assert.equal(restored.hiddenMetadata.restoredFromSessionId, original.sessionId);
    assert.equal(JSON.stringify(restored).includes("restored from"), false);

    const updatedSession = await requestJson(baseUrl, "/api/updateSession", {
      body: {
        params: {
          isFavorite: true,
          lifecycleState: "sleeping",
          projectId: project.projectId,
          runtimeSettings: { delayedSendMs: 250 },
          sessionId: restored.sessionId,
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(updatedSession.status, 200);
    assert.equal(updatedSession.body.result.session.title, "Restored title stays user-owned");
    assert.equal(updatedSession.body.result.session.isFavorite, true);
    assert.equal(updatedSession.body.result.session.runtimeSettings.delayedSendMs, 250);

    const projectStatus = await requestJson(baseUrl, "/api/readProjectStatus", {
      body: {
        params: { projectId: project.projectId },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(projectStatus.status, 200);
    assert.equal(projectStatus.body.result.project.runtimeSettings.defaultPromptAgentId, "codex");
    assert.equal("layout" in projectStatus.body.result.project, false);
    assert.equal(projectStatus.body.result.sessions.length, 2);

    const presentation = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
      body: {
        params: {},
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(presentation.status, 200);
    assert.equal(presentation.body.result.snapshot.projects.length, 1);
    assert.equal(presentation.body.result.snapshot.sessions.length, 2);
    assert.equal(presentation.body.result.snapshot.sessions[0].visibleInSidebarByDefault, true);

    const presentationSearch = await requestJson(baseUrl, "/api/searchSessions", {
      body: {
        params: {
          includeActive: true,
          includePrevious: true,
          query: "Restored",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(presentationSearch.status, 200);
    assert.deepEqual(presentationSearch.body.result.results.map((result: { sessionId: string }) => result.sessionId), [
      restored.sessionId,
    ]);

    const stoppedRestored = await requestJson(baseUrl, "/api/updateSession", {
      body: {
        params: {
          lifecycleState: "stopped",
          projectId: project.projectId,
          sessionId: restored.sessionId,
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(stoppedRestored.status, 200);

    const previousBeforeRemove = await requestJson(baseUrl, "/api/listPreviousSessions", {
      body: {
        params: {
          query: "Restored",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(previousBeforeRemove.status, 200);
    assert.deepEqual(
      previousBeforeRemove.body.result.results.map((result: { sessionId: string }) => result.sessionId),
      [restored.sessionId],
    );

    /*
    CDXC:PreviousSessions 2026-06-02-11:24:
    Previous Sessions delete/restore cleanup is a gxserver mutation. Removing the stopped G-session must make it disappear from listPreviousSessions instead of relying on native modal-local filtering.
    */
    const removedSession = await requestJson(baseUrl, "/api/removeSession", {
      body: {
        params: {
          projectId: project.projectId,
          reason: "previous-session-delete",
          sessionId: restored.sessionId,
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(removedSession.status, 200);
    assert.equal(removedSession.body.result.session.sessionId, restored.sessionId);

    const previousAfterRemove = await requestJson(baseUrl, "/api/listPreviousSessions", {
      body: {
        params: {
          query: "Restored",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(previousAfterRemove.status, 200);
    assert.deepEqual(previousAfterRemove.body.result.results, []);

    const removedProject = await requestJson(baseUrl, "/api/removeProject", {
      body: {
        params: { projectId: project.projectId },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(removedProject.status, 200);
    assert.equal(removedProject.body.result.project.projectId, project.projectId);

    const projectsAfterRemove = await requestJson(baseUrl, "/api/listProjects", {
      body: {
        params: {},
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(projectsAfterRemove.status, 200);
    assert.deepEqual(projectsAfterRemove.body.result.projects, []);

    const sessionsAfterRemove = await requestJson(baseUrl, "/api/listSessions", {
      body: {
        params: {},
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(sessionsAfterRemove.status, 200);
    assert.deepEqual(sessionsAfterRemove.body.result.sessions, []);

    const removedPresentation = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
      body: {
        params: {},
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(removedPresentation.status, 200);
    assert.deepEqual(removedPresentation.body.result.snapshot.projects, []);
    assert.deepEqual(removedPresentation.body.result.snapshot.sessions, []);

    const missingProjectStatus = await requestJson(baseUrl, "/api/readProjectStatus", {
      body: {
        params: { projectId: project.projectId },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(missingProjectStatus.status, 404);
    assert.equal(missingProjectStatus.body.error, "notFound");
  });
});

test("terminal title event API stores gxserver-decided canonical titles", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const createdProject = await requestJson(baseUrl, "/api/createProject", {
      body: {
        params: { name: "Ghostex", path: "/repo/ghostex" },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const project = createdProject.body.result.project;
    const createdSession = await requestJson(baseUrl, "/api/createSession", {
      body: {
        params: {
          projectId: project.projectId,
          runtimeSettings: { titleSource: "placeholder" },
          title: "Search by Text",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const session = createdSession.body.result.session;

    const ignored = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
      body: {
        params: {
          projectId: project.projectId,
          rawTitle: "Search by Text",
          sessionId: session.sessionId,
          sessionPersistenceProvider: "zmx",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(ignored.status, 200);
    assert.equal(ignored.body.result.activity.activity, "idle");
    assert.equal(ignored.body.result.enteredAttention, false);
    assert.equal(ignored.body.result.changed, false);
    assert.equal(ignored.body.result.projection.isTemporaryTitle, true);
    assert.equal(ignored.body.result.session.title, "Search by Text");

    const updated = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
      body: {
        params: {
          projectId: project.projectId,
          rawTitle: "Find previous Codex work",
          sessionId: session.sessionId,
          sessionPersistenceProvider: "zmx",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(updated.status, 200);
    assert.equal(updated.body.result.activity.activity, "idle");
    assert.equal(updated.body.result.changed, true);
    /*
    CDXC:AgentDetection 2026-06-11-22:44:
    A user task title can contain an agent provider name. gxserver should store the title, but it must not promote the row to an agent unless the title is an explicit program-title signal or hook/session metadata identifies the agent.
    */
    assert.equal(updated.body.result.session.kind, "terminal");
    assert.equal(updated.body.result.session.agentId, undefined);
    assert.equal(updated.body.result.session.runtimeSettings.agentName, undefined);
    assert.equal(updated.body.result.session.title, "Find previous Codex work");
    assert.equal(updated.body.result.session.runtimeSettings.titleSource, "terminal-auto");
    assert.equal(updated.body.result.projection.primaryTitle, "Find previous Codex work");
  });
});

test("terminal title event API promotes Claude Code title observations to agent sessions", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const createdProject = await requestJson(baseUrl, "/api/createProject", {
      body: {
        params: { name: "Ghostex", path: "/repo/ghostex" },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const project = createdProject.body.result.project;
    const createdSession = await requestJson(baseUrl, "/api/createSession", {
      body: {
        params: {
          kind: "terminal",
          projectId: project.projectId,
          providerState: { lifecycleState: "exists" },
          surface: "workspace",
          title: "Terminal Session",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const session = createdSession.body.result.session;

    const promoted = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
      body: {
        params: {
          projectId: project.projectId,
          rawTitle: "✳ Claude Code",
          sessionId: session.sessionId,
          sessionPersistenceProvider: "zmx",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });

    /*
    CDXC:ClaudeSessionIdentity 2026-06-11-21:43:
    macOS forwards raw terminal titles without local agent classification. gxserver must promote recognized Claude Code program titles into the shared session model so presentation consumers stop showing the row as a generic terminal.
    */
    assert.equal(promoted.status, 200);
    assert.equal(promoted.body.result.changed, true);
    assert.equal(promoted.body.result.activity.activity, "idle");
    assert.equal(promoted.body.result.enteredAttention, false);
    assert.equal(promoted.body.result.session.kind, "agent");
    assert.equal(promoted.body.result.session.agentId, "claude");
    assert.equal(promoted.body.result.session.runtimeSettings.agentName, "claude");

    const presentation = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    const presentationSession = presentation.body.result.snapshot.sessions.find(
      (candidate: { sessionId: string }) => candidate.sessionId === session.sessionId,
    );
    assert.equal(presentationSession?.agentId, "claude");
    assert.equal(presentationSession?.agentName, "claude");
    assert.equal(presentationSession?.kind, "agent");
  });
});

test("presentation and list APIs sync legacy session-state sidecars without macOS app", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    const project = (
      await requestJson(baseUrl, "/api/createProject", {
        body: {
          params: { name: "Ghostex", path: paths.rootDir },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      })
    ).body.result.project;
    const session = (
      await requestJson(baseUrl, "/api/createSession", {
        body: {
          params: {
            kind: "terminal",
            lifecycleState: "running",
            projectId: project.projectId,
            providerState: { lifecycleState: "exists", provider: "zmx" },
            runtimeSettings: { titleSource: "placeholder" },
            surface: "workspace",
            title: "Terminal Session",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      })
    ).body.result.session;
    const sidecarDir = path.join(path.dirname(paths.rootDir), "session-state", project.projectId);
    const sidecarPath = path.join(sidecarDir, `${session.sessionId}.env`);
    await mkdir(sidecarDir, { recursive: true });
    const workingAt = new Date(Date.now() + 1_000).toISOString();
    const idleAt = new Date(Date.now() + 2_000).toISOString();
    /*
    CDXC:MobileSessionStatus 2026-06-11-23:52:
    Mobile clients call gxserver through `ghostex sessions --json`; they must observe legacy hook sidecar status even when no macOS sidebar poller is running. Keep this test writing the sidecar directly so it covers daemon-owned ingestion rather than a renderer callback.
    */
    await writeFile(
      sidecarPath,
      [
        "agent=codex",
        "agentSessionId=019e7af5-c610-7f62-a129-db7bb510b48d",
        "status=working",
        `statusUpdatedAt=${workingAt}`,
        `lastActivityAt=${workingAt}`,
      ].join("\n"),
    );

    const presentation = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    const presentationSession = presentation.body.result.snapshot.sessions.find(
      (candidate: { sessionId: string }) => candidate.sessionId === session.sessionId,
    );
    assert.equal(presentationSession?.activity, "working");
    assert.equal(presentationSession?.agentId, "codex");

    await writeFile(
      sidecarPath,
      [
        "agent=codex",
        "agentSessionId=019e7af5-c610-7f62-a129-db7bb510b48d",
        "status=idle",
        `statusUpdatedAt=${idleAt}`,
        `lastActivityAt=${idleAt}`,
      ].join("\n"),
    );
    const listed = await requestJson(baseUrl, "/api/listSessions", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    const listedSession = listed.body.result.sessions.find(
      (candidate: { sessionId: string }) => candidate.sessionId === session.sessionId,
    );
    assert.equal(listedSession.runtimeSettings.agentActivity.activity, "idle");
    assert.equal(listedSession.agentId, "codex");
  });
});

test("terminal title event API does not publish presentation deltas for same-title status bookkeeping", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const createdProject = await requestJson(baseUrl, "/api/createProject", {
      body: {
        params: { name: "Ghostex", path: "/repo/ghostex" },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const project = createdProject.body.result.project;
    const createdSession = await requestJson(baseUrl, "/api/createSession", {
      body: {
        params: {
          projectId: project.projectId,
          runtimeSettings: { titleSource: "terminal-auto" },
          title: "Search by Text",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const session = createdSession.body.result.session;
    const socket = new WebSocket(toWebSocketUrl(baseUrl, "/api/events"), {
      headers: authHeaders(token, GXSERVER_PROTOCOL_VERSION),
    });
    const readyEventPromise = nextWebSocketEvent(socket, "eventStreamReady");
    await waitFor(once(socket, "open"), "WebSocket open");
    assert.equal((await readyEventPromise).type, "eventStreamReady");

    try {
      /*
      CDXC:GxserverSessionTitles 2026-06-08-09:30:
      Repeated terminal-title observations that only create idle status bookkeeping must not publish an immediate presentation delta, because macOS sidebar hydration from that delta can steal first responder from the terminal that owns typing focus.
      */
      const observedEventsPromise = collectWebSocketEvents(socket, 600);
      const ignored = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
        body: {
          params: {
            projectId: project.projectId,
            rawTitle: "Search by Text",
            sessionId: session.sessionId,
            sessionPersistenceProvider: "zmx",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(ignored.status, 200);
      assert.equal(ignored.body.result.changed, false);
      assert.equal(ignored.body.result.activity.activity, "idle");
      assert.equal(ignored.body.result.session.runtimeSettings.agentActivity.lastTitle, "Search by Text");
      const presentationEvents = (await observedEventsPromise).filter((event) => event.type === "presentationDelta");
      assert.deepEqual(presentationEvents, []);
    } finally {
      socket.close();
    }
  });
});

test("session state event API resolves resumed Codex sessions from shared history", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const codexSessionId = "019e7af5-c610-7f62-a129-db7bb510b48d";
    const createdProject = await requestJson(baseUrl, "/api/createProject", {
      body: {
        params: {
          name: "Ghostex",
          path: "/repo/ghostex",
          previousSessionHistory: [
            {
              agentSessionId: codexSessionId,
              closedAt: "2026-05-31T12:04:13.807Z",
              primaryTitle: "Shorter native tabs bar",
              sessionRecord: {
                agentName: "codex",
                agentSessionId: codexSessionId,
                title: "Shorter native tabs bar",
                titleSource: "terminal-auto",
              },
            },
          ],
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const project = createdProject.body.result.project;
    const createdSession = await requestJson(baseUrl, "/api/createSession", {
      body: {
        params: {
          projectId: project.projectId,
          runtimeSettings: { titleSource: "placeholder" },
          title: "Terminal Session",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const session = createdSession.body.result.session;

    const ingested = await requestJson(baseUrl, "/api/ingestSessionStateEvent", {
      body: {
        params: {
          projectId: project.projectId,
          sessionId: session.sessionId,
          startupText: `cd '/repo/ghostex' && codex resume "${codexSessionId}"`,
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });

    assert.equal(ingested.status, 200);
    assert.equal(ingested.body.result.changed, true);
    assert.equal(ingested.body.result.reason, "previous-session-record-title");
    assert.equal(ingested.body.result.session.agentId, "codex");
    assert.equal(ingested.body.result.session.kind, "agent");
    assert.equal(ingested.body.result.session.runtimeSettings.agentSessionId, codexSessionId);
    assert.equal(ingested.body.result.session.runtimeSettings.titleSource, "terminal-auto");
    assert.equal(ingested.body.result.session.title, "Shorter native tabs bar");
    assert.equal(ingested.body.result.projection.primaryTitle, "Shorter native tabs bar");
  });
});

test("session state event API logs passive Codex identity conflicts without raw private data", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    await writeNativeSidebarSettings(paths.homeDir, { debuggingMode: true });
    await delay(1_100);
    const currentCodexSessionId = "019e7af5-c610-7f62-a129-db7bb510b48d";
    const incomingCodexSessionId = "019e7c39-7ba7-7ac3-b79c-02757e299516";
    const createdProject = await requestJson(baseUrl, "/api/createProject", {
      body: {
        params: { name: "Ghostex", path: "/repo/ghostex" },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const project = createdProject.body.result.project;
    const createdSession = await requestJson(baseUrl, "/api/createSession", {
      body: {
        params: {
          agentId: "codex",
          kind: "agent",
          projectId: project.projectId,
          runtimeSettings: {
            agentName: "codex",
            agentSessionId: currentCodexSessionId,
            titleSource: "terminal-auto",
          },
          title: "Current Codex Thread",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const session = createdSession.body.result.session;

    const ingested = await requestJson(baseUrl, "/api/ingestSessionStateEvent", {
      body: {
        params: {
          agentName: "codex",
          agentSessionId: incomingCodexSessionId,
          agentSessionPath: "/Users/person/.codex/sessions/private-thread.jsonl",
          firstUserMessage: "private prompt text",
          projectId: project.projectId,
          sessionId: session.sessionId,
          title: "Wrong Codex Thread",
          titleSource: "terminal-auto",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });

    assert.equal(ingested.status, 200);
    assert.equal(ingested.body.result.changed, false);
    assert.equal(ingested.body.result.reason, "passive-session-identity-conflict");
    assert.equal(ingested.body.result.session.runtimeSettings.agentSessionId, currentCodexSessionId);
    assert.equal(ingested.body.result.session.runtimeSettings.firstUserMessage, undefined);
    assert.equal(ingested.body.result.session.title, "Current Codex Thread");
    const { entry: conflict, text: logs } = await waitForLogEvent(
      paths.logFile,
      "sessionIdentity.updateBlocked",
      1_000,
    );
    assert.ok(conflict, "identity conflict log exists");
    assert.equal(conflict.level, "debug");
    assert.equal(conflict.details.reason, "passive-agent-session-id-replacement");
    assert.equal(conflict.details.source, "passive");
    assert.equal(conflict.details.currentAgentSessionIdPresent, true);
    assert.equal(typeof conflict.details.currentAgentSessionIdHash, "string");
    assert.equal(typeof conflict.details.incomingAgentSessionIdHash, "string");
    const { entry: rejected } = await waitForLogEvent(
      paths.logFile,
      "sessionIdentity.passiveEventRejected",
      1_000,
    );
    assert.equal(rejected.level, "warn");
    assert.equal(rejected.details.reason, "passive-agent-session-id-replacement");
    assert.equal(rejected.details.source, "session-state-event");
    assert.equal(rejected.details.identitySource, "passive");
    assert.equal(rejected.details.hasFirstUserMessage, true);
    assert.equal(rejected.details.hasTitle, true);
    assert.doesNotMatch(logs, new RegExp(currentCodexSessionId, "u"));
    assert.doesNotMatch(logs, new RegExp(incomingCodexSessionId, "u"));
    assert.doesNotMatch(logs, /private-thread|private prompt text|Current Codex Thread|Wrong Codex Thread|\/Users\/person/u);
  });
});

test("agent hook event API rejects wrong Codex thread attention before status changes", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    const currentCodexSessionId = "019e7af5-c610-7f62-a129-db7bb510b48d";
    const incomingCodexSessionId = "019e7c39-7ba7-7ac3-b79c-02757e299516";
    const project = (
      await requestJson(baseUrl, "/api/createProject", {
        body: {
          params: { name: "Ghostex", path: paths.rootDir },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      })
    ).body.result.project;
    const target = (
      await requestJson(baseUrl, "/api/createSession", {
        body: {
          params: {
            agentId: "codex",
            kind: "agent",
            projectId: project.projectId,
            runtimeSettings: {
              agentActivity: { activity: "idle", isAcknowledged: true },
              agentName: "codex",
              agentSessionId: currentCodexSessionId,
              titleSource: "terminal-auto",
            },
            title: "Target Codex Thread",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      })
    ).body.result.session;
    await requestJson(baseUrl, "/api/createSession", {
      body: {
        params: {
          agentId: "codex",
          kind: "agent",
          projectId: project.projectId,
          runtimeSettings: {
            agentName: "codex",
            agentSessionId: incomingCodexSessionId,
            titleSource: "terminal-auto",
          },
          title: "Other Codex Thread",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });

    const ingested = await requestJson(baseUrl, "/api/ingestAgentHookEvent", {
      body: {
        params: {
          agentName: "codex",
          agentSessionId: incomingCodexSessionId,
          eventName: "Stop",
          firstUserMessage: "private prompt text",
          projectId: project.projectId,
          rawEventName: "Stop",
          sessionId: target.sessionId,
          status: "attention",
          statusUpdatedAt: "2026-06-09T18:08:19.857Z",
          title: "Wrong Codex Thread",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });

    assert.equal(ingested.status, 200);
    assert.equal(ingested.body.result.changed, false);
    assert.equal(ingested.body.result.reason, "passive-session-identity-conflict");
    assert.equal(ingested.body.result.enteredAttention, false);
    assert.equal(ingested.body.result.activity.activity, "idle");
    assert.equal(ingested.body.result.session.runtimeSettings.agentActivity.activity, "idle");
    assert.equal(ingested.body.result.session.runtimeSettings.agentSessionId, currentCodexSessionId);
    assert.equal(ingested.body.result.session.runtimeSettings.firstUserMessage, undefined);
    assert.equal(ingested.body.result.session.title, "Target Codex Thread");

    const { entry: rejected, text: logs } = await waitForLogEvent(
      paths.logFile,
      "sessionIdentity.passiveEventRejected",
      1_000,
    );
    assert.equal(rejected.level, "warn");
    assert.equal(rejected.details.source, "agent-hook-event");
    assert.equal(rejected.details.activity, "idle");
    assert.equal(rejected.details.hasExplicitStatus, true);
    assert.equal(rejected.details.hasFirstUserMessage, true);
    assert.equal(rejected.details.hasTitle, true);
    assert.doesNotMatch(logs, new RegExp(currentCodexSessionId, "u"));
    assert.doesNotMatch(logs, new RegExp(incomingCodexSessionId, "u"));
    assert.doesNotMatch(logs, /private prompt text|Wrong Codex Thread|Target Codex Thread/u);
  });
});

test("session state event API runs first-prompt auto-title through gxserver", async () => {
  const zmxCalls: string[] = [];
  const sendInputs: string[] = [];
  await withApiServer(
    "local",
    async ({ baseUrl, token }) => {
      const createdProject = await requestJson(baseUrl, "/api/createProject", {
        body: {
          params: { name: "Ghostex", path: "/repo/ghostex" },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      const project = createdProject.body.result.project;
      const createdSession = await requestJson(baseUrl, "/api/createSession", {
        body: {
          params: {
            projectId: project.projectId,
            runtimeSettings: { titleSource: "placeholder" },
            title: "Terminal",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      const session = createdSession.body.result.session;
      const ingested = await requestJson(baseUrl, "/api/ingestSessionStateEvent", {
        body: {
          params: {
            agentName: "codex",
            agentSessionId: "019e8fd3-f8ad-7932-baeb-d9f6f912c57b",
            firstPromptTitleGenerationAgent: "cursor",
            firstPromptTitleGenerationCommand: "cursor-agent",
            firstUserMessage: "Please implement the gxserver session title flow",
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(ingested.status, 200);
      assert.equal(ingested.body.result.session.kind, "agent");
      assert.equal(ingested.body.result.session.runtimeSettings.firstPromptTitleGenerationAgent, "cursor");
      assert.equal(ingested.body.result.session.runtimeSettings.firstPromptTitleGenerationCommand, "cursor-agent");
      assert.equal(ingested.body.result.session.runtimeSettings.gxserverFirstPromptAutoTitleStatus, "running");

      const titledSession = await waitForSession(
        baseUrl,
        token,
        project.projectId,
        session.sessionId,
        (candidate) => candidate.runtimeSettings?.gxserverFirstPromptAutoTitleStatus === "applied",
      );
      assert.equal(titledSession.title, "Server Title Flow");
      assert.equal(titledSession.runtimeSettings.titleSource, "generated");
      assert.equal(titledSession.runtimeSettings.autoTitleFromFirstPrompt, true);
      /*
      CDXC:GxserverSessionTitle 2026-06-05-12:43:
      gxserver-generated titles are staged without carriage-return bytes. Native macOS submits the staged command with sendTerminalEnter after observing the generated-title presentation transition, so agent prompt editors receive a real Enter action instead of typed newline input.
      */
      assert.deepEqual(sendInputs, ["/rename Server Title Flow"]);
      assert.ok(zmxCalls.some((script) => script.includes('exec "$zmx_bin" send "$zmx_session"')));
    },
    {
      firstPromptTitleGeneration: {
        generateTitle: async () => "Server Title Flow",
      },
      zmxLifecycle: fakeZmxLifecycle(zmxCalls, () => 0, { stdinInputs: sendInputs }),
    },
  );
});

test("Claude hook working events run first-prompt auto-title through gxserver", async () => {
  const zmxCalls: string[] = [];
  const sendInputs: string[] = [];
  let titleGenerationCalled = false;
  await withApiServer(
    "local",
    async ({ baseUrl, token }) => {
      const createdProject = await requestJson(baseUrl, "/api/createProject", {
        body: {
          params: { name: "Ghostex", path: "/repo/ghostex" },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      const project = createdProject.body.result.project;
      const createdSession = await requestJson(baseUrl, "/api/createSession", {
        body: {
          params: {
            projectId: project.projectId,
            runtimeSettings: { titleSource: "placeholder" },
            title: "Claude Code",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      const session = createdSession.body.result.session;

      /*
      CDXC:GxserverSessionTitle 2026-06-12-07:08:
      Claude Code UserPromptSubmit hooks are the first reliable signal that a newly launched Claude session is working with user text. When the row is still generically named, gxserver must claim the first-prompt title flow, stage only `/rename`, and let Claude generate its own title after native submits the command.
      */
      const ingested = await requestJson(baseUrl, "/api/ingestAgentHookEvent", {
        body: {
          params: {
            agentName: "claude",
            agentSessionId: "9970b270-b39f-4d63-a764-fa8d88083995",
            eventName: "UserPromptSubmit",
            firstUserMessage: "Please wire Claude hook naming",
            projectId: project.projectId,
            rawEventName: "UserPromptSubmit",
            sessionId: session.sessionId,
            status: "working",
            statusUpdatedAt: "2026-06-12T03:08:00.000Z",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(ingested.status, 200);
      assert.equal(ingested.body.result.reason, "first-prompt-auto-title-claimed");
      assert.equal(ingested.body.result.activity.activity, "working");
      assert.equal(ingested.body.result.session.kind, "agent");
      assert.equal(ingested.body.result.session.agentId, "claude");
      assert.equal(ingested.body.result.session.runtimeSettings.gxserverFirstPromptAutoTitleStatus, "running");

      const renamedSession = await waitForSession(
        baseUrl,
        token,
        project.projectId,
        session.sessionId,
        (candidate) => candidate.runtimeSettings?.gxserverFirstPromptAutoTitleStatus === "applied",
      );
      assert.equal(renamedSession.title, "Claude Code");
      assert.equal(renamedSession.runtimeSettings.titleSource, "placeholder");
      assert.equal(renamedSession.runtimeSettings.autoTitleFromFirstPrompt, true);
      assert.equal(renamedSession.runtimeSettings.gxserverFirstPromptAutoTitleShouldSubmitStagedCommand, true);
      assert.equal(titleGenerationCalled, false);
      assert.deepEqual(sendInputs, ["/rename"]);
      assert.ok(zmxCalls.some((script) => script.includes('exec "$zmx_bin" send "$zmx_session"')));
    },
    {
      firstPromptTitleGeneration: {
        generateTitle: async () => {
          titleGenerationCalled = true;
          throw new Error("Claude bare rename should not call title generation");
        },
      },
      zmxLifecycle: fakeZmxLifecycle(zmxCalls, () => 0, { stdinInputs: sendInputs }),
    },
  );
});

test("first-prompt auto-title cancellation prevents the pending rename command", async () => {
  const zmxCalls: string[] = [];
  const sendInputs: string[] = [];
  let releaseTitle: ((title: string) => void) | undefined;
  let titleGenerationStarted: (() => void) | undefined;
  const titleStarted = new Promise<void>((resolve) => {
    titleGenerationStarted = resolve;
  });
  const titleResult = new Promise<string>((resolve) => {
    releaseTitle = resolve;
  });

  await withApiServer(
    "local",
    async ({ baseUrl, token }) => {
      const createdProject = await requestJson(baseUrl, "/api/createProject", {
        body: {
          params: { name: "Ghostex", path: "/repo/ghostex" },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      const project = createdProject.body.result.project;
      const createdSession = await requestJson(baseUrl, "/api/createSession", {
        body: {
          params: {
            projectId: project.projectId,
            runtimeSettings: { titleSource: "placeholder" },
            title: "Terminal",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      const session = createdSession.body.result.session;
      const ingested = await requestJson(baseUrl, "/api/ingestSessionStateEvent", {
        body: {
          params: {
            agentName: "codex",
            agentSessionId: "019e8fd3-f8ad-7932-baeb-d9f6f912c57c",
            firstUserMessage: "Please cancel this generated title before rename",
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(ingested.status, 200);
      assert.equal(ingested.body.result.session.runtimeSettings.gxserverFirstPromptAutoTitleStatus, "running");
      await titleStarted;

      const cancelled = await requestJson(baseUrl, "/api/cancelFirstPromptAutoTitle", {
        body: {
          params: {
            projectId: project.projectId,
            reason: "escape",
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(cancelled.status, 200);
      assert.equal(cancelled.body.result.changed, true);
      assert.equal(cancelled.body.result.session.runtimeSettings.gxserverFirstPromptAutoTitleStatus, "cancelled");

      releaseTitle?.("Cancelled Title");
      const cancelledSession = await waitForSession(
        baseUrl,
        token,
        project.projectId,
        session.sessionId,
        (candidate) => candidate.runtimeSettings?.gxserverFirstPromptAutoTitleStatus === "cancelled",
      );
      assert.equal(cancelledSession.title, "Terminal");
      await new Promise((resolve) => setTimeout(resolve, 50));
      assert.deepEqual(sendInputs, []);
      assert.equal(zmxCalls.some((script) => script.includes('exec "$zmx_bin" send "$zmx_session"')), false);
    },
    {
      firstPromptTitleGeneration: {
        generateTitle: async () => {
          titleGenerationStarted?.();
          return titleResult;
        },
      },
      zmxLifecycle: fakeZmxLifecycle(zmxCalls, () => 0, { stdinInputs: sendInputs }),
    },
  );
});

test("cancelled first-prompt auto-title retries after a later prompt", async () => {
  const zmxCalls: string[] = [];
  const sendInputs: string[] = [];
  const prompts: string[] = [];
  let releaseFirstTitle: ((title: string) => void) | undefined;
  let firstTitleGenerationStarted: (() => void) | undefined;
  const firstTitleStarted = new Promise<void>((resolve) => {
    firstTitleGenerationStarted = resolve;
  });
  const firstTitleResult = new Promise<string>((resolve) => {
    releaseFirstTitle = resolve;
  });

  await withApiServer(
    "local",
    async ({ baseUrl, token }) => {
      const createdProject = await requestJson(baseUrl, "/api/createProject", {
        body: {
          params: { name: "Ghostex", path: "/repo/ghostex" },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      const project = createdProject.body.result.project;
      const createdSession = await requestJson(baseUrl, "/api/createSession", {
        body: {
          params: {
            projectId: project.projectId,
            runtimeSettings: { titleSource: "placeholder" },
            title: "Terminal",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      const session = createdSession.body.result.session;
      const firstPrompt = "Please cancel this generated title before rename";
      const firstIngest = await requestJson(baseUrl, "/api/ingestSessionStateEvent", {
        body: {
          params: {
            agentName: "codex",
            agentSessionId: "019e8fd3-f8ad-7932-baeb-d9f6f912c57d",
            firstUserMessage: firstPrompt,
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(firstIngest.status, 200);
      assert.equal(firstIngest.body.result.session.runtimeSettings.gxserverFirstPromptAutoTitleStatus, "running");
      await firstTitleStarted;

      const cancelled = await requestJson(baseUrl, "/api/cancelFirstPromptAutoTitle", {
        body: {
          params: {
            projectId: project.projectId,
            reason: "escape",
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(cancelled.status, 200);
      assert.equal(cancelled.body.result.session.runtimeSettings.gxserverFirstPromptAutoTitleStatus, "cancelled");

      releaseFirstTitle?.("Cancelled Title");
      await new Promise((resolve) => setTimeout(resolve, 50));
      assert.deepEqual(sendInputs, []);

      const staleRetry = await requestJson(baseUrl, "/api/ingestSessionStateEvent", {
        body: {
          params: {
            agentName: "codex",
            agentSessionId: "019e8fd3-f8ad-7932-baeb-d9f6f912c57d",
            firstUserMessage: firstPrompt,
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(staleRetry.status, 200);
      assert.equal(staleRetry.body.result.session.runtimeSettings.gxserverFirstPromptAutoTitleStatus, "cancelled");

      const laterPrompt = "Now explain the auto sleep defaults";
      const retried = await requestJson(baseUrl, "/api/ingestSessionStateEvent", {
        body: {
          params: {
            agentName: "codex",
            agentSessionId: "019e8fd3-f8ad-7932-baeb-d9f6f912c57d",
            firstUserMessage: laterPrompt,
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(retried.status, 200);
      assert.equal(retried.body.result.session.runtimeSettings.gxserverFirstPromptAutoTitleStatus, "running");

      const titledSession = await waitForSession(
        baseUrl,
        token,
        project.projectId,
        session.sessionId,
        (candidate) => candidate.runtimeSettings?.gxserverFirstPromptAutoTitleStatus === "applied",
      );
      assert.equal(titledSession.title, "Auto Sleep Defaults");
      assert.equal(titledSession.runtimeSettings.firstUserMessage, laterPrompt);
      assert.deepEqual(prompts, ["cancel this generated title before rename", laterPrompt]);
      assert.deepEqual(sendInputs, ["/rename Auto Sleep Defaults"]);
      assert.ok(zmxCalls.some((script) => script.includes('exec "$zmx_bin" send "$zmx_session"')));
    },
    {
      firstPromptTitleGeneration: {
        generateTitle: async ({ prompt }) => {
          prompts.push(prompt);
          if (prompts.length === 1) {
            firstTitleGenerationStarted?.();
            return firstTitleResult;
          }
          return "Auto Sleep Defaults";
        },
      },
      zmxLifecycle: fakeZmxLifecycle(zmxCalls, () => 0, { stdinInputs: sendInputs }),
    },
  );
});

test("domain-state APIs reject oversized and too-deep project/session JSON before SQLite persistence", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const oversizedProjectBody = {
      params: {
        name: "Oversized project",
        runtimeSettings: { promptCache: "x".repeat(GXSERVER_DOMAIN_STATE_JSON_LIMIT_CHARS) },
      },
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
    };
    assert.equal(Buffer.byteLength(JSON.stringify(oversizedProjectBody)) < GXSERVER_JSON_BODY_LIMIT_BYTES, true);

    const oversizedProject = await requestJson(baseUrl, "/api/createProject", {
      body: oversizedProjectBody,
      method: "POST",
      token,
    });
    assert.equal(oversizedProject.status, 400);
    assert.equal(oversizedProject.body.error, "badRequest");
    assert.match(oversizedProject.body.message, /runtimeSettings exceeds .*JSON size limit/);

    const projectsAfterRejectedCreate = await requestJson(baseUrl, "/api/listProjects", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.deepEqual(projectsAfterRejectedCreate.body.result.projects, []);

    const createdProject = await requestJson(baseUrl, "/api/createProject", {
      body: { params: { name: "Ghostex" }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(createdProject.status, 200);
    const project = createdProject.body.result.project;

    const tooDeepHistory = await requestJson(baseUrl, "/api/updateProject", {
      body: {
        params: {
          previousSessionHistory: [{ historyId: "hist-deep", payload: nestedJson(GXSERVER_DOMAIN_STATE_JSON_MAX_DEPTH + 1) }],
          projectId: project.projectId,
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(tooDeepHistory.status, 400);
    assert.equal(tooDeepHistory.body.error, "badRequest");
    assert.match(tooDeepHistory.body.message, /previousSessionHistory exceeds .*JSON depth limit/);

    const projectStatus = await requestJson(baseUrl, "/api/readProjectStatus", {
      body: { params: { projectId: project.projectId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.deepEqual(projectStatus.body.result.project.previousSessionHistory, []);

    const oversizedSessionBody = {
      params: {
        launchSettings: { firstUserMessage: "x".repeat(GXSERVER_DOMAIN_STATE_JSON_LIMIT_CHARS) },
        projectId: project.projectId,
        title: "Oversized session",
      },
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
    };
    assert.equal(Buffer.byteLength(JSON.stringify(oversizedSessionBody)) < GXSERVER_JSON_BODY_LIMIT_BYTES, true);

    const oversizedSession = await requestJson(baseUrl, "/api/createSession", {
      body: oversizedSessionBody,
      method: "POST",
      token,
    });
    assert.equal(oversizedSession.status, 400);
    assert.equal(oversizedSession.body.error, "badRequest");
    assert.match(oversizedSession.body.message, /launchSettings exceeds .*JSON size limit/);

    const createdSession = await requestJson(baseUrl, "/api/createSession", {
      body: {
        params: { projectId: project.projectId, runtimeSettings: { delayedSendMs: 250 }, title: "Bounded session" },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(createdSession.status, 200);
    const session = createdSession.body.result.session;

    const tooDeepSession = await requestJson(baseUrl, "/api/updateSession", {
      body: {
        params: {
          projectId: project.projectId,
          runtimeSettings: nestedJson(GXSERVER_DOMAIN_STATE_JSON_MAX_DEPTH + 1),
          sessionId: session.sessionId,
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(tooDeepSession.status, 400);
    assert.equal(tooDeepSession.body.error, "badRequest");
    assert.match(tooDeepSession.body.message, /runtimeSettings exceeds .*JSON depth limit/);

    const sessions = await requestJson(baseUrl, "/api/listSessions", {
      body: { params: { projectId: project.projectId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(sessions.body.result.sessions.length, 1);
    assert.deepEqual(sessions.body.result.sessions[0].runtimeSettings, { delayedSendMs: 250 });
  });
});

test("domain-state APIs surface corrupt SQLite JSON columns instead of emptying state", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    const createdProject = await requestJson(baseUrl, "/api/createProject", {
      body: {
        params: { name: "Ghostex", runtimeSettings: { defaultPromptAgentId: "codex" } },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(createdProject.status, 200);
    const project = createdProject.body.result.project;

    const createdSession = await requestJson(baseUrl, "/api/createSession", {
      body: {
        params: { projectId: project.projectId, providerState: { marker: "durable" }, title: "Durable session" },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(createdSession.status, 200);
    const session = createdSession.body.result.session;

    const db = openGxserverDatabase(paths);
    try {
      db.prepare("UPDATE projects SET runtimeSettingsJson = ? WHERE projectId = ?").run("{not-json", project.projectId);
    } finally {
      db.close();
    }

    const projectRead = await requestJson(baseUrl, "/api/readProjectStatus", {
      body: { params: { projectId: project.projectId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(projectRead.status, 409);
    assert.equal(projectRead.body.error, "corruptState");
    assert.match(projectRead.body.message, /project .* column runtimeSettingsJson/);

    const projectUpdate = await requestJson(baseUrl, "/api/updateProject", {
      body: {
        params: { name: "Should not persist", projectId: project.projectId },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(projectUpdate.status, 409);
    assert.equal(projectUpdate.body.error, "corruptState");

    const repairedProjectDb = openGxserverDatabase(paths);
    try {
      const storedProject = repairedProjectDb
        .prepare<[string], { name: string; runtimeSettingsJson: string }>(
          "SELECT name, runtimeSettingsJson FROM projects WHERE projectId = ?",
        )
        .get(project.projectId);
      assert.equal(storedProject?.name, "Ghostex");
      assert.equal(storedProject?.runtimeSettingsJson, "{not-json");
      repairedProjectDb
        .prepare("UPDATE projects SET runtimeSettingsJson = ? WHERE projectId = ?")
        .run(JSON.stringify({ defaultPromptAgentId: "codex" }), project.projectId);
      repairedProjectDb
        .prepare("UPDATE sessions SET providerStateJson = ? WHERE projectId = ? AND sessionId = ?")
        .run(JSON.stringify("wrong-shape"), project.projectId, session.sessionId);
    } finally {
      repairedProjectDb.close();
    }

    const sessionRead = await requestJson(baseUrl, "/api/readProjectStatus", {
      body: { params: { projectId: project.projectId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(sessionRead.status, 409);
    assert.equal(sessionRead.body.error, "corruptState");
    assert.match(sessionRead.body.message, /session .* column providerStateJson/);

    const sessionUpdate = await requestJson(baseUrl, "/api/updateSession", {
      body: {
        params: { projectId: project.projectId, sessionId: session.sessionId, title: "Should not persist" },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(sessionUpdate.status, 409);
    assert.equal(sessionUpdate.body.error, "corruptState");

    const finalDb = openGxserverDatabase(paths);
    try {
      const storedSession = finalDb
        .prepare<[string, string], { providerStateJson: string; title: string }>(
          "SELECT providerStateJson, title FROM sessions WHERE projectId = ? AND sessionId = ?",
        )
        .get(project.projectId, session.sessionId);
      assert.equal(storedSession?.title, "Durable session");
      assert.equal(storedSession?.providerStateJson, JSON.stringify("wrong-shape"));
    } finally {
      finalDb.close();
    }
  });
});

test("zmx lifecycle APIs attach existing sessions without replay and create missing sessions with cwd", async () => {
  const calls: string[] = [];
  let probeExitCode = 0;
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const cwd = paths.rootDir;
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: cwd }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createAgentSession", {
          body: {
            params: {
              agentId: "codex",
              launchSettings: { acceptAllMode: "enabled" },
              projectId: project.projectId,
              runtimeSettings: {
                agentSessionId: "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56",
                titleSource: "user",
              },
              title: "Existing agent",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;
      assert.equal(session.launchSettings.agentLaunchPlan.command, "codex --yolo");

      const launchPlan = await requestJson(baseUrl, "/api/readAgentLaunchPlan", {
        body: {
          params: { agentId: "codex", projectId: project.projectId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(launchPlan.status, 200);
      assert.equal(launchPlan.body.result.plan.command, "codex --yolo");

      const resumePlan = await requestJson(baseUrl, "/api/readAgentResumePlan", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(resumePlan.status, 200);
      assert.match(resumePlan.body.result.plan.primaryCommand, /CODEX_RESUME_SESSION_ID/);
      assert.match(resumePlan.body.result.plan.primaryCommand, /--exact/);
      assert.match(
        resumePlan.body.result.plan.primaryCommand,
        /codex --yolo resume "\$CODEX_RESUME_SESSION_ID"/,
      );
      assert.equal(
        resumePlan.body.result.plan.displayCommand,
        'codex --yolo resume "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56"',
      );
      assert.equal(
        resumePlan.body.result.plan.copyCommand,
        'codex --yolo resume "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56"',
      );

      probeExitCode = 0;
      const existing = await requestJson(baseUrl, "/api/attachSessionMetadata", {
        body: {
          params: { projectId: project.projectId, promptEditor: "monaco", sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(existing.status, 200);
      assert.equal(existing.body.result.attach.providerState.lifecycleState, "exists");
      assert.equal(existing.body.result.attach.persistenceSessionCreated, false);
      assert.equal(existing.body.result.attach.startupTextDisposition, "discardExistingProvider");
      assert.equal(existing.body.result.attach.startupText, undefined);
      assert.match(existing.body.result.attach.attachCommand, /^\/bin\/zsh -lc '/);
      assert.match(existing.body.result.attach.attachCommand, /--prompt-editor=monaco/);

      probeExitCode = 1;
      const missing = await requestJson(baseUrl, "/api/wakeSession", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId, startupText: "" },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(missing.status, 200);
      assert.equal(missing.body.result.attach.providerState.lifecycleState, "missing");
      assert.equal(missing.body.result.attach.persistenceSessionCreated, true);
      assert.equal(missing.body.result.attach.startupTextDisposition, "queueAfterTerminalReady");
      assert.match(missing.body.result.attach.startupText, /codex --yolo resume "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56"/);
      assert.equal(missing.body.result.session.lifecycleState, "running");
      assert.ok(calls.some((script) => script.includes("list --short")));
    },
    {
      zmxLifecycle: fakeZmxLifecycle(calls, () => probeExitCode),
    },
  );
});

test("createSession suppresses fresh terminal title attention and logs safe diagnostics", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    await writeNativeSidebarSettings(paths.homeDir, { debuggingMode: true });
    await delay(1_100);
    const project = (
      await requestJson(baseUrl, "/api/createProject", {
        body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      })
    ).body.result.project;

    const beforeCreateMs = Date.now();
    const created = await requestJson(baseUrl, "/api/createSession", {
      body: {
        params: {
          kind: "terminal",
          lifecycleState: "running",
          projectId: project.projectId,
          providerState: { lifecycleState: "exists", provider: "zmx" },
          title: "Fresh private terminal",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(created.status, 200);
    const session = created.body.result.session;
    assert.equal(session.runtimeSettings.agentActivity.activity, "idle");
    assert.equal(session.runtimeSettings.agentActivity.hasSeenWorking, false);
    assert.equal(session.runtimeSettings.agentActivity.isAcknowledged, true);
    const createSuppressedUntil = Date.parse(session.runtimeSettings.agentActivity.suppressedUntil);
    assert.equal(Number.isFinite(createSuppressedUntil), true);
    assert.ok(createSuppressedUntil - beforeCreateMs > 10_000);

    const title = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
      body: {
        params: {
          agentName: "codex",
          projectId: project.projectId,
          rawTitle: "[ ! ] Action Required /Users/person/private?token=secret https://example.test/path?token=secret",
          sessionId: session.sessionId,
          sessionPersistenceProvider: "zmx",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(title.status, 200);
    assert.equal(title.body.result.activity.activity, "idle");
    assert.equal(title.body.result.enteredAttention, false);
    assert.equal(title.body.result.session.runtimeSettings.agentActivity.activity, "idle");

    const { entry: createLog } = await waitForLogEvent(
      paths.logFile,
      "sessionActivity.createSuppressionStarted",
      1_000,
    );
    assert.equal(createLog.level, "info");
    assert.equal(createLog.details.changed, true);
    assert.equal(createLog.details.lifecycleState, "running");
    assert.equal(createLog.details.previousActivity, "idle");
    assert.equal(createLog.details.previousHadSuppression, false);
    assert.equal(createLog.details.sessionKind, "terminal");
    assert.equal(typeof createLog.details.suppressedUntil, "string");
    assert.equal(typeof createLog.details.suppressionMs, "number");

    const { entry: titleLog, text: logs } = await waitForLogEvent(
      paths.logFile,
      "sessionTitle.terminalTitleEvent",
      1_000,
    );
    assert.equal(titleLog.details.activitySuppressedByWindow, true);
    assert.equal(typeof titleLog.details.activitySuppressionRemainingMs, "number");
    assert.equal(titleLog.details.statusUpdateStored, false);
    assert.doesNotMatch(logs, /Fresh private terminal|Action Required|\/Users\/person|example\.test|token=secret/u);
  });
});

test("createAgentSession suppresses startup title activity", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    await writeNativeSidebarSettings(paths.homeDir, { debuggingMode: true });
    await delay(1_100);
    const project = (
      await requestJson(baseUrl, "/api/createProject", {
        body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      })
    ).body.result.project;

    const beforeCreateMs = Date.now();
    const created = await requestJson(baseUrl, "/api/createAgentSession", {
      body: {
        params: {
          agentId: "codex",
          projectId: project.projectId,
          title: "Agent Detection Sync",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(created.status, 200);
    const session = created.body.result.session;
    assert.equal(session.kind, "agent");
    assert.equal(session.runtimeSettings.agentActivity.activity, "idle");
    assert.equal(session.runtimeSettings.agentActivity.hasSeenWorking, false);
    assert.equal(session.runtimeSettings.agentActivity.isAcknowledged, true);
    const suppressedUntil = Date.parse(session.runtimeSettings.agentActivity.suppressedUntil);
    assert.equal(Number.isFinite(suppressedUntil), true);
    assert.ok(suppressedUntil - beforeCreateMs > 10_000);

    const { entry: createLog } = await waitForLogEvent(
      paths.logFile,
      "sessionActivity.createSuppressionStarted",
      1_000,
    );
    assert.equal(createLog.details.changed, true);
    assert.equal(createLog.details.sessionKind, "agent");
  });
});

test("wakeSession suppresses wake-time title attention and logs safe diagnostics", async () => {
  const calls: string[] = [];
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      await writeNativeSidebarSettings(paths.homeDir, { debuggingMode: true });
      await delay(1_100);
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createAgentSession", {
          body: {
            params: {
              agentId: "codex",
              lifecycleState: "sleeping",
              projectId: project.projectId,
              providerState: { lifecycleState: "missing", provider: "zmx" },
              runtimeSettings: {
                agentActivity: {
                  activity: "working",
                  agentName: "codex",
                  hasSeenWorking: true,
                  isAcknowledged: false,
                  lastChangedAt: "2026-06-10T06:50:00.000Z",
                  workingStartedAt: "2026-06-10T06:50:00.000Z",
                },
                titleSource: "user",
              },
              title: "Sleeping private session",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const beforeWakeMs = Date.now();
      const wake = await requestJson(baseUrl, "/api/wakeSession", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId, startupText: "" },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(wake.status, 200);
      assert.equal(wake.body.result.session.lifecycleState, "running");
      assert.equal(wake.body.result.session.runtimeSettings.agentActivity.activity, "idle");
      assert.equal(wake.body.result.session.runtimeSettings.agentActivity.hasSeenWorking, false);
      assert.equal(wake.body.result.session.runtimeSettings.agentActivity.isAcknowledged, true);
      const suppressedUntil = Date.parse(wake.body.result.session.runtimeSettings.agentActivity.suppressedUntil);
      assert.equal(Number.isFinite(suppressedUntil), true);
      assert.ok(suppressedUntil - beforeWakeMs > 10_000);

      const title = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
        body: {
          params: {
            agentName: "codex",
            projectId: project.projectId,
            rawTitle: "[ ! ] Action Required /Users/person/private?token=secret https://example.test/path?token=secret",
            sessionId: session.sessionId,
            sessionPersistenceProvider: "zmx",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(title.status, 200);
      assert.equal(title.body.result.activity.activity, "idle");
      assert.equal(title.body.result.enteredAttention, false);
      assert.equal(title.body.result.session.runtimeSettings.agentActivity.activity, "idle");

      const { entry: wakeLog } = await waitForLogEvent(
        paths.logFile,
        "sessionActivity.wakeSuppressionStarted",
        1_000,
      );
      assert.equal(wakeLog.level, "info");
      assert.equal(wakeLog.details.changed, true);
      assert.equal(wakeLog.details.previousActivity, "working");
      assert.equal(wakeLog.details.previousHadSuppression, false);
      assert.equal(wakeLog.details.provider, "zmx");
      assert.equal(typeof wakeLog.details.suppressedUntil, "string");
      assert.equal(typeof wakeLog.details.suppressionMs, "number");

      const { entry: titleLog, text: logs } = await waitForLogEvent(
        paths.logFile,
        "sessionTitle.terminalTitleEvent",
        1_000,
      );
      assert.equal(titleLog.details.activitySuppressedByWindow, true);
      assert.equal(typeof titleLog.details.activitySuppressionRemainingMs, "number");
      assert.equal(titleLog.details.statusUpdateStored, false);
      assert.doesNotMatch(logs, /Sleeping private session|Action Required|\/Users\/person|example\.test|token=secret/u);
    },
    {
      zmxLifecycle: fakeZmxLifecycle(calls, () => 1),
    },
  );
});

test("zmx lifecycle API starts missing providers through detached zmx run without replaying existing sessions", async () => {
  const calls: string[] = [];
  let probeExitCode = 1;
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createAgentSession", {
          body: {
            params: {
              agentId: "codex",
              launchSettings: { acceptAllMode: "enabled" },
              projectId: project.projectId,
              title: "Remote agent",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const start = await requestJson(baseUrl, "/api/startSessionProvider", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(start.status, 200);
      assert.equal(start.body.result.started, true);
      assert.equal(start.body.result.startupTextDisposition, "queueAfterTerminalReady");
      assert.equal(start.body.result.session.lifecycleState, "running");
      assert.equal(start.body.result.providerState.lifecycleState, "exists");
      const runScript = calls.find((script) => script.includes('run "$zmx_session" -d --initial-command /bin/zsh -lic "$zmx_startup_command"'));
      assert.ok(runScript);
      assert.match(runScript, /zmx_startup_text=' codex --yolo'/);
      assert.match(runScript, /zmx_startup_command=' codex --yolo[\s\S]*exec \/bin\/zsh -li'/);
      assert.match(runScript, /export GHOSTEX_GLOBAL_SESSION_REF=/);
      assert.match(runScript, /cd "\$zmx_cwd" \|\| exit/);

      const runCallCount = calls.filter((script) => script.includes('run "$zmx_session" -d')).length;
      probeExitCode = 0;
      const existing = await requestJson(baseUrl, "/api/startSessionProvider", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(existing.status, 200);
      assert.equal(existing.body.result.started, false);
      assert.equal(existing.body.result.startupTextDisposition, "discardExistingProvider");
      assert.equal(calls.filter((script) => script.includes('run "$zmx_session" -d')).length, runCallCount);
    },
    {
      zmxLifecycle: fakeZmxLifecycle(calls, () => probeExitCode, {
        runExitCode: () => 0,
      }),
    },
  );
});

test("zmx session interaction APIs read/send through zmx and report missing renderer for visible UI commands", async () => {
  const calls: string[] = [];
  const sendInputs: string[] = [];
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: { params: { projectId: project.projectId, title: "Talk to me" }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.session;

      const read = await requestJson(baseUrl, "/api/readSessionText", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(read.status, 200);
      assert.equal(read.body.result.text, "first line\nsecond line");
      assert.equal(read.body.result.truncated, false);
      assert.equal(read.body.result.limitBytes, GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES);
      assert.equal(read.body.result.zmxName, `S7k-${project.projectId}-${session.sessionId}`);

      const send = await requestJson(baseUrl, "/api/sendSessionText", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId, text: "hello 'agent'" },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(send.status, 200);
      assert.equal(send.body.result.textLength, "hello 'agent'".length);
      assert.equal(send.body.result.textBytes, Buffer.byteLength("hello 'agent'", "utf8"));

      const enter = await requestJson(baseUrl, "/api/sendSessionEnter", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(enter.status, 200);
      assert.equal(enter.body.result.textLength, 1);
      assert.equal(enter.body.result.textBytes, 1);

      const message = await requestJson(baseUrl, "/api/sendSessionMessage", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId, text: "ship it", submit: true },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(message.status, 200);
      assert.equal(message.body.result.submit, true);

      const focus = await requestJson(baseUrl, "/api/focusSession", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(focus.status, 503);
      assert.equal(focus.body.error, "dependencyUnavailable");
      assert.match(focus.body.message, /No macOS renderer is connected/);

      const agentMessage = await requestJson(baseUrl, "/api/sendSessionMessage", {
        body: {
          params: { agentId: "codex", text: "new visible session" },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(agentMessage.status, 503);
      assert.equal(agentMessage.body.error, "dependencyUnavailable");
      assert.match(agentMessage.body.message, /No macOS renderer is connected/);
      assert.ok(calls.some((script) => script.includes('exec "$zmx_bin" history "$zmx_session"')));
      assert.ok(calls.some((script) => script.includes('exec "$zmx_bin" send "$zmx_session"')));
      assert.equal(calls.some((script) => script.includes("hello 'agent'") || script.includes("ship it")), false);
      assert.deepEqual(sendInputs, ["hello 'agent'", "\r", "ship it\r"]);
    },
    {
      zmxLifecycle: {
        requireZmx: async () => ({
          executablePath: "/fake/bundled/zmx",
          source: "devSubmodule",
          tool: "zmx",
        }),
        runZsh: async (script, options) => {
          calls.push(script);
          if (script.includes('exec "$zmx_bin" history "$zmx_session"')) {
            assert.equal(options?.stdoutLimitBytes, GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES);
            return { exitCode: 0, stderr: "", stdout: "first line\nsecond line" };
          }
          if (script.includes('exec "$zmx_bin" send "$zmx_session"')) {
            sendInputs.push(options?.stdin ?? "");
            return { exitCode: 0, stderr: "", stdout: "" };
          }
          return { exitCode: 64, stderr: "unexpected zmx command", stdout: "" };
        },
      },
    },
  );
});

test("zmx session interaction APIs cap history and reject oversized send payloads", async () => {
  const calls: string[] = [];
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: { params: { projectId: project.projectId, title: "Bounded I/O" }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.session;

      const read = await requestJson(baseUrl, "/api/readSessionText", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(read.status, 200);
      assert.equal(read.body.result.text.length, GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES);
      assert.equal(read.body.result.truncated, true);
      assert.equal(read.body.result.truncatedReason, "historyOutputLimitExceeded");
      assert.equal(read.body.result.limitBytes, GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES);

      const oversized = await requestJson(baseUrl, "/api/sendSessionText", {
        body: {
          params: {
            projectId: project.projectId,
            sessionId: session.sessionId,
            text: "x".repeat(GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES + 1),
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(oversized.status, 400);
      assert.equal(oversized.body.error, "badRequest");
      assert.match(oversized.body.message, /zmx send limit/);
      assert.equal(calls.filter((script) => script.includes('exec "$zmx_bin" send "$zmx_session"')).length, 0);
    },
    {
      zmxLifecycle: {
        requireZmx: async () => ({
          executablePath: "/fake/bundled/zmx",
          source: "devSubmodule",
          tool: "zmx",
        }),
        runZsh: async (script, options) => {
          calls.push(script);
          assert.equal(options?.stdoutLimitBytes, GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES);
          return {
            exitCode: 125,
            stderr: `zmx command stdout exceeded ${GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES} bytes`,
            stdout: "h".repeat(GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES),
            stdoutLimitBytes: GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES,
            stdoutTruncated: true,
          };
        },
      },
    },
  );
});

test("gxserver restart preserves session state and does not replay startup text into existing zmx", async () => {
  /*
  CDXC:GxserverVerification 2026-05-30-18:37:
  Restarting gxserver must preserve durable project/session/zmx metadata while leaving provider sessions alive. Reattached existing zmx sessions discard queued resume text so app restart cannot replay agent commands into an already-running backend.
  */
  const homeDir = await mkdtemp(path.join(tmpdir(), "gxserver-api-restart-"));
  const calls: string[] = [];
  try {
    let first = await startApiServerFixture(homeDir, "local", {
      zmxLifecycle: fakeZmxLifecycle(calls, () => 1),
    });
    let projectId = "";
    let sessionId = "";
    try {
      const createdProject = await requestJson(first.baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: first.paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token: first.token,
      });
      assert.equal(createdProject.status, 200);
      const project = createdProject.body.result.project;
      projectId = project.projectId;
      const createdSession = await requestJson(first.baseUrl, "/api/createAgentSession", {
          body: {
            params: {
              agentId: "codex",
              projectId,
              runtimeSettings: { agentSessionId: "existing-thread" },
              title: "Restarted agent",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token: first.token,
      });
      assert.equal(createdSession.status, 200);
      const session = createdSession.body.result.session;
      sessionId = session.sessionId;
      assert.equal(session.zmxName, `S7k-${projectId}-${sessionId}`);
    } finally {
      await first.close();
    }

    const second = await startApiServerFixture(homeDir, "local", {
      zmxLifecycle: fakeZmxLifecycle(calls, () => 0),
    });
    try {
      const attach = await requestJson(second.baseUrl, "/api/attachSessionMetadata", {
        body: {
          params: { projectId, sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token: second.token,
      });
      assert.equal(attach.status, 200);
      assert.equal(attach.body.result.attach.providerState.lifecycleState, "exists");
      assert.equal(attach.body.result.attach.persistenceSessionCreated, false);
      assert.equal(attach.body.result.attach.startupTextDisposition, "discardExistingProvider");
      assert.equal(attach.body.result.attach.startupText, undefined);
      assert.equal(attach.body.result.attach.session.zmxName, `S7k-${projectId}-${sessionId}`);
      assert.ok(calls.some((script) => script.includes("list --short")));
      assert.equal(calls.some((script) => script.includes('kill "$zmx_session" --force')), false);
    } finally {
      await second.close();
    }
  } finally {
    await rm(homeDir, { force: true, recursive: true });
  }
});

test("missing bundled zmx hard-fails attach metadata instead of generating a fallback command", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: { params: { projectId: project.projectId, title: "No zmx" }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.session;

      const response = await requestJson(baseUrl, "/api/attachSessionMetadata", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });

      assert.equal(response.status, 500);
      assert.equal(response.body.error, "internalError");
      assert.match(response.body.message, /bundled zmx missing/);
      assert.equal(JSON.stringify(response.body).includes("attachCommand"), false);
    },
    {
      zmxLifecycle: {
        requireZmx: async () => {
          throw new Error("bundled zmx missing");
        },
        runZsh: async () => ({ exitCode: 0, stderr: "", stdout: "" }),
      },
    },
  );
});

test("agent activity API updates semantic activity and last active state", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createAgentSession", {
          body: {
            params: { agentId: "codex", projectId: project.projectId, title: "Activity agent" },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const launch = await requestJson(baseUrl, "/api/updateAgentActivity", {
        body: {
          params: {
            event: "launch",
            nowMs: Date.parse("2026-05-30T12:00:00.000Z"),
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(launch.status, 200);
      assert.equal(launch.body.result.session.runtimeSettings.agentActivity.activity, "idle");
      assert.equal(
        launch.body.result.session.runtimeSettings.agentActivity.suppressedUntil,
        "2026-05-30T12:00:12.000Z",
      );

      const working = await requestJson(baseUrl, "/api/updateAgentActivity", {
        body: {
          params: {
            activity: "working",
            nowMs: Date.parse("2026-05-30T12:00:13.000Z"),
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(working.status, 200);
      assert.equal(working.body.result.activity.activity, "working");
      assert.equal(working.body.result.enteredAttention, false);
      assert.equal(working.body.result.session.runtimeSettings.agentActivity.activity, "working");
      assert.equal(working.body.result.session.lastActiveAt, "2026-05-30T12:00:13.000Z");
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0),
    },
  );
});

test("escape activity suppresses done attention and logs safe diagnostics", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      await writeNativeSidebarSettings(paths.homeDir, { debuggingMode: true });
      await delay(1_100);
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createAgentSession", {
          body: {
            params: {
              agentId: "codex",
              projectId: project.projectId,
              runtimeSettings: { titleSource: "user" },
              title: "Private Escape Agent",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const workingNowMs = Date.now();
      const working = await requestJson(baseUrl, "/api/updateAgentActivity", {
        body: {
          params: {
            activity: "working",
            agentName: "codex",
            nowMs: workingNowMs,
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(working.status, 200);
      assert.equal(working.body.result.activity.activity, "working");

      const escapeNowMs = Date.now();
      const escape = await requestJson(baseUrl, "/api/updateAgentActivity", {
        body: {
          params: {
            agentName: "codex",
            event: "escape",
            nowMs: escapeNowMs,
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(escape.status, 200);
      assert.equal(escape.body.result.activity.activity, "working");
      assert.equal(
        escape.body.result.activity.attentionSuppressedUntil,
        new Date(escapeNowMs + 5_000).toISOString(),
      );
      assert.equal(escape.body.result.session.runtimeSettings.agentActivity.activity, "working");

      const title = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
        body: {
          params: {
            agentName: "codex",
            projectId: project.projectId,
            rawTitle: "[ ! ] Action Required /Users/person/private?token=secret https://example.test/path?token=secret",
            sessionId: session.sessionId,
            sessionPersistenceProvider: "zmx",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(title.status, 200);
      assert.equal(title.body.result.activity.activity, "idle");
      assert.equal(title.body.result.enteredAttention, false);
      assert.equal(title.body.result.session.runtimeSettings.agentActivity.activity, "idle");
      assert.equal(
        title.body.result.session.runtimeSettings.agentActivity.attentionSuppressedUntil,
        new Date(escapeNowMs + 5_000).toISOString(),
      );

      const hook = await requestJson(baseUrl, "/api/ingestAgentHookEvent", {
        body: {
          params: {
            agentName: "codex",
            eventName: "Stop",
            projectId: project.projectId,
            rawEventName: "Stop",
            sessionId: session.sessionId,
            status: "attention",
            statusUpdatedAt: new Date(Date.now()).toISOString(),
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(hook.status, 200);
      assert.equal(hook.body.result.activity.activity, "idle");
      assert.equal(hook.body.result.enteredAttention, false);

      const { entry: escapeLog } = await waitForLogEvent(
        paths.logFile,
        "sessionActivity.escapeSuppressionStarted",
        1_000,
      );
      assert.equal(escapeLog.level, "info");
      assert.equal(escapeLog.details.changed, true);
      assert.equal(escapeLog.details.previousActivity, "working");
      assert.equal(escapeLog.details.previousHadAttentionSuppression, false);
      assert.equal(escapeLog.details.resultActivity, "working");
      assert.equal(escapeLog.details.source, "terminal-escape");
      assert.equal(typeof escapeLog.details.suppressedUntil, "string");
      assert.equal(typeof escapeLog.details.suppressionMs, "number");

      const { entry: titleLog } = await waitForLogEvent(
        paths.logFile,
        "sessionTitle.terminalTitleEvent",
        1_000,
      );
      assert.equal(titleLog.details.attentionSuppressedByWindow, true);
      assert.equal(typeof titleLog.details.attentionSuppressionRemainingMs, "number");
      assert.equal(titleLog.details.statusUpdateStored, true);

      const logs = await readFile(paths.logFile, "utf8");
      assert.doesNotMatch(logs, /Private Escape Agent|Action Required|\/Users\/person|example\.test|token=secret|https:\/\/example\.test|rawEventName/u);
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0),
    },
  );
});

test("agent hook event API owns working state across suppression and plain terminal titles", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createAgentSession", {
          body: {
            params: {
              agentId: "codex",
              projectId: project.projectId,
              runtimeSettings: { titleSource: "user" },
              title: "User title",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      await requestJson(baseUrl, "/api/updateAgentActivity", {
        body: {
          params: {
            event: "launch",
            nowMs: Date.parse("2026-06-07T06:47:25.000Z"),
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });

      const working = await requestJson(baseUrl, "/api/ingestAgentHookEvent", {
        body: {
          params: {
            agentName: "codex",
            eventName: "UserPromptSubmit",
            projectId: project.projectId,
            rawEventName: "UserPromptSubmit",
            sessionId: session.sessionId,
            status: "working",
            statusUpdatedAt: "2026-06-07T06:47:30.000Z",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(working.status, 200);
      assert.equal(working.body.result.activity.activity, "working");
      assert.equal(working.body.result.session.runtimeSettings.agentActivity.activity, "working");
      assert.equal(working.body.result.session.runtimeSettings.agentActivity.workingSource, "explicit");

      const title = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
        body: {
          params: {
            agentName: "codex",
            projectId: project.projectId,
            rawTitle: "Monaco Ctrl+G Switch",
            sessionId: session.sessionId,
            sessionPersistenceProvider: "zmx",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(title.status, 200);
      assert.equal(title.body.result.activity.activity, "working");
      assert.equal(title.body.result.session.runtimeSettings.agentActivity.activity, "working");

      const stopped = await requestJson(baseUrl, "/api/ingestAgentHookEvent", {
        body: {
          params: {
            agentName: "codex",
            eventName: "Stop",
            projectId: project.projectId,
            rawEventName: "Stop",
            sessionId: session.sessionId,
            status: "attention",
            statusUpdatedAt: "2026-06-07T06:47:31.000Z",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(stopped.status, 200);
      assert.equal(stopped.body.result.activity.activity, "idle");
      assert.equal(stopped.body.result.enteredAttention, false);

      const staleWorking = await requestJson(baseUrl, "/api/ingestAgentHookEvent", {
        body: {
          params: {
            agentName: "codex",
            eventName: "UserPromptSubmit",
            projectId: project.projectId,
            rawEventName: "UserPromptSubmit",
            sessionId: session.sessionId,
            status: "working",
            statusUpdatedAt: "2026-06-07T06:47:30.500Z",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(staleWorking.status, 200);
      assert.equal(staleWorking.body.result.reason, "stale-activity-event");
      assert.equal(staleWorking.body.result.session.runtimeSettings.agentActivity.activity, "idle");

      const presentation = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
        body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      const presentationSession = presentation.body.result.snapshot.sessions.find(
        (candidate: { sessionId: string }) => candidate.sessionId === session.sessionId,
      );
      assert.equal(presentationSession?.activity, "idle");
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0),
    },
  );
});

test("agent hook event API treats Claude Stop as idle even with stale attention status", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: {
            params: {
              kind: "terminal",
              projectId: project.projectId,
              runtimeSettings: { titleSource: "placeholder" },
              surface: "workspace",
              title: "Terminal Session",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const working = await requestJson(baseUrl, "/api/ingestAgentHookEvent", {
        body: {
          params: {
            agentName: "claude",
            agentSessionId: "9970b270-b39f-4d63-a764-fa8d88083995",
            eventName: "UserPromptSubmit",
            projectId: project.projectId,
            rawEventName: "UserPromptSubmit",
            sessionId: session.sessionId,
            status: "working",
            statusUpdatedAt: "2026-06-11T17:43:00.000Z",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(working.status, 200);
      assert.equal(working.body.result.activity.activity, "working");
      assert.equal(working.body.result.session.kind, "agent");
      assert.equal(working.body.result.session.agentId, "claude");

      const stopped = await requestJson(baseUrl, "/api/ingestAgentHookEvent", {
        body: {
          params: {
            agentName: "claude",
            agentSessionId: "9970b270-b39f-4d63-a764-fa8d88083995",
            eventName: "Stop",
            projectId: project.projectId,
            rawEventName: "Stop",
            sessionId: session.sessionId,
            status: "attention",
            statusUpdatedAt: "2026-06-11T17:43:05.000Z",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });

      /*
      CDXC:ClaudeSessionStatus 2026-06-11-21:43:
      Claude Stop settles the active turn. gxserver must override stale hook sidecar status that says `attention` so all clients render idle instead of a false Done state.
      */
      assert.equal(stopped.status, 200);
      assert.equal(stopped.body.result.activity.activity, "idle");
      assert.equal(stopped.body.result.enteredAttention, false);
      assert.equal(stopped.body.result.session.runtimeSettings.agentActivity.activity, "idle");
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0),
    },
  );
});

test("agent hook event API normalizes provider turn boundaries centrally", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const createTerminalSession = async () => (
        await requestJson(baseUrl, "/api/createSession", {
          body: {
            params: {
              kind: "terminal",
              projectId: project.projectId,
              runtimeSettings: { titleSource: "placeholder" },
              surface: "workspace",
              title: "Terminal Session",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;
      const ingestHook = async (params: Record<string, unknown>) => requestJson(baseUrl, "/api/ingestAgentHookEvent", {
        body: {
          params: {
            projectId: project.projectId,
            statusUpdatedAt: "2026-06-11T18:19:00.000Z",
            ...params,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });

      /*
      CDXC:AgentHookStatus 2026-06-11-22:19:
      Cursor response hooks, Rovo permission hooks, and Copilot Notification-as-stop hooks must be normalized in gxserver. Clients should not carry provider-specific Done/attention rules, and stale sidecar status text must not override known provider event semantics.
      */
      const cursorSession = await createTerminalSession();
      const cursorWorking = await ingestHook({
        agentName: "cursor",
        agentSessionId: "cursor-session-1",
        eventName: "beforeSubmitPrompt",
        rawEventName: "beforeSubmitPrompt",
        sessionId: cursorSession.sessionId,
        status: "working",
      });
      assert.equal(cursorWorking.status, 200);
      assert.equal(cursorWorking.body.result.activity.activity, "working");
      const cursorDone = await ingestHook({
        agentName: "cursor",
        agentSessionId: "cursor-session-1",
        eventName: "afterAgentResponse",
        rawEventName: "afterAgentResponse",
        sessionId: cursorSession.sessionId,
        status: "attention",
      });
      assert.equal(cursorDone.status, 200);
      assert.equal(cursorDone.body.result.activity.activity, "idle");

      const rovoSession = await createTerminalSession();
      const rovoPermission = await ingestHook({
        agentName: "rovo",
        agentSessionId: "rovo-session-1",
        eventName: "on_tool_permission",
        rawEventName: "on_tool_permission",
        sessionId: rovoSession.sessionId,
        status: "attention",
      });
      assert.equal(rovoPermission.status, 200);
      assert.equal(rovoPermission.body.result.activity.activity, "working");

      const copilotSession = await createTerminalSession();
      const copilotNotification = await ingestHook({
        agentName: "github copilot",
        agentSessionId: "copilot-session-1",
        eventName: "Notification",
        rawEventName: "Notification",
        sessionId: copilotSession.sessionId,
        status: "attention",
      });
      assert.equal(copilotNotification.status, 200);
      assert.equal(copilotNotification.body.result.activity.activity, "idle");
      assert.equal(copilotNotification.body.result.session.agentId, "copilot");
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0),
    },
  );
});

test("agent hook, session-state, and activity APIs reject cross-agent events for locked launches", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: {
            params: {
              agentId: "codex",
              kind: "agent",
              launchSettings: {
                agentLaunchPlan: {
                  agentCommand: "codex",
                  command: "codex fork 019e7af5-c610-7f62-a129-db7bb510b48d",
                },
                forkedFromSessionId: "G4c7u",
              },
              projectId: project.projectId,
              runtimeSettings: {
                agentActivity: { activity: "idle" },
                agentCommand: "codex",
                agentName: "codex",
                forkedFromSessionId: "G4c7u",
                launchAgentId: "codex",
                titleSource: "user",
              },
              title: "Locked Codex fork",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const cursorHook = await requestJson(baseUrl, "/api/ingestAgentHookEvent", {
        body: {
          params: {
            agentName: "cursor",
            agentSessionPath: "/Users/person/.cursor/private-session.jsonl",
            eventName: "beforeSubmitPrompt",
            firstUserMessage: "private prompt text must not be logged",
            projectId: project.projectId,
            rawEventName: "beforeSubmitPrompt",
            sessionId: session.sessionId,
            status: "working",
            statusUpdatedAt: "2026-06-09T16:37:47.476Z",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(cursorHook.status, 200);
      assert.equal(cursorHook.body.result.reason, "agent-hook-agent-mismatch");
      assert.equal(cursorHook.body.result.changed, false);
      assert.equal(cursorHook.body.result.session.agentId, "codex");
      assert.equal(cursorHook.body.result.session.runtimeSettings.agentName, "codex");
      assert.equal(cursorHook.body.result.session.runtimeSettings.agentActivity.activity, "idle");

      const mismatchLog = await waitForLogEvent(paths.logFile, "sessionIdentity.launchAgentMismatch", 2_000);
      assert.equal(mismatchLog.entry.level, "warn");
      assert.equal(mismatchLog.entry.details.source, "agent-hook-event");
      assert.equal(mismatchLog.entry.details.lockedAgentId, "codex");
      assert.equal(mismatchLog.entry.details.incomingAgentId, "cursor");
      assert.equal(mismatchLog.entry.details.hasAgentSessionPath, true);
      assert.equal(mismatchLog.text.includes("/Users/person"), false);
      assert.equal(mismatchLog.text.includes("private prompt text"), false);
      assert.equal(mismatchLog.text.includes("beforeSubmitPrompt"), false);

      const cursorState = await requestJson(baseUrl, "/api/ingestSessionStateEvent", {
        body: {
          params: {
            agentName: "cursor",
            agentSessionPath: "/Users/person/.cursor/private-session.jsonl",
            firstUserMessage: "another private prompt text",
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(cursorState.status, 200);
      assert.equal(cursorState.body.result.reason, "session-state-agent-mismatch");
      assert.equal(cursorState.body.result.changed, false);
      assert.equal(cursorState.body.result.session.agentId, "codex");
      assert.equal(cursorState.body.result.session.runtimeSettings.agentSessionPath, undefined);

      const cursorActivity = await requestJson(baseUrl, "/api/updateAgentActivity", {
        body: {
          params: {
            activity: "working",
            agentName: "cursor",
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(cursorActivity.status, 200);
      assert.equal(cursorActivity.body.result.activity.activity, "idle");
      assert.equal(cursorActivity.body.result.session.runtimeSettings.agentActivity.activity, "idle");

      const codexHook = await requestJson(baseUrl, "/api/ingestAgentHookEvent", {
        body: {
          params: {
            agentName: "codex",
            eventName: "UserPromptSubmit",
            projectId: project.projectId,
            rawEventName: "UserPromptSubmit",
            sessionId: session.sessionId,
            status: "working",
            statusUpdatedAt: "2026-06-09T16:37:50.000Z",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(codexHook.status, 200);
      assert.equal(codexHook.body.result.activity.activity, "working");
      assert.equal(codexHook.body.result.activity.workingSource, "explicit");
      assert.equal(codexHook.body.result.session.agentId, "codex");
      assert.equal(codexHook.body.result.session.runtimeSettings.agentActivity.agentName, "codex");
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0),
    },
  );
});

test("presentation snapshot repairs stale agent identity from live zmx process evidence", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: {
            params: {
              agentId: "claude",
              kind: "agent",
              launchSettings: { startupText: " gx f\r", surface: "workspace" },
              lifecycleState: "running",
              projectId: project.projectId,
              providerState: { lifecycleState: "exists", provider: "zmx" },
              runtimeSettings: {
                agentActivity: { activity: "idle", agentName: "claude" },
                agentName: "claude",
                agentSessionId: "303d77cf-4871-48da-871f-47782e834307",
                agentSessionPath: "/Users/madda/.claude-profiles/personal/projects/repo/303d77cf-4871-48da-871f-47782e834307.jsonl",
                sessionPersistenceProvider: "zmx",
              },
              surface: "workspace",
              title: "Fix Native Tab Clicks",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const snapshotResponse = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
        body: {
          params: {},
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(snapshotResponse.status, 200);
      const repaired = snapshotResponse.body.result.snapshot.sessions.find(
        (candidate: { sessionId: string }) => candidate.sessionId === session.sessionId,
      );
      assert.equal(repaired.agentId, "codex");
      assert.equal(repaired.agentSessionId, "019eb8d0-d27b-7f30-b6d7-7a04ab8fae78");

      const listed = await requestJson(baseUrl, "/api/listSessions", {
        body: {
          params: { projectId: project.projectId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      const stored = listed.body.result.sessions.find(
        (candidate: { sessionId: string }) => candidate.sessionId === session.sessionId,
      );
      assert.equal(stored.agentId, "codex");
      assert.equal(stored.runtimeSettings.agentName, "codex");
      assert.equal(stored.runtimeSettings.agentSessionId, "019eb8d0-d27b-7f30-b6d7-7a04ab8fae78");
      assert.equal(stored.runtimeSettings.agentSessionPath, undefined);
      assert.equal(stored.runtimeSettings.agentActivity, undefined);
    },
    {
      zmxLifecycle: {
        readSessionProcessIdentities: async ({ sessionNames }) => {
          const sessionName = sessionNames[0];
          return sessionName ? new Map([[sessionName, {
            agentId: "codex",
            agentSessionId: "019eb8d0-d27b-7f30-b6d7-7a04ab8fae78",
          }]]) : new Map();
        },
        requireZmx: async () => ({
          executablePath: "/fake/bundled/zmx",
          source: "devSubmodule",
          tool: "zmx",
        }),
      },
    },
  );
});

test("terminal title API clears explicit working on same-title Codex spinner stop", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createAgentSession", {
          body: {
            params: {
              agentId: "codex",
              projectId: project.projectId,
              runtimeSettings: { titleSource: "user" },
              title: "Ghostex 4.0.0 Beta",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      /*
      CDXC:SessionStatus 2026-06-07-09:22:
      The terminal-title API must preserve hook-owned explicit working across unrelated titles, but a same-title Codex spinner stop is trusted completion evidence because Codex may not send a hook stop event for every completed turn.
      */
      const explicit = await requestJson(baseUrl, "/api/updateAgentActivity", {
        body: {
          params: {
            activity: "working",
            agentName: "codex",
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(explicit.status, 200);
      assert.equal(explicit.body.result.activity.workingSource, "explicit");

      const spinner = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
        body: {
          params: {
            agentName: "codex",
            projectId: project.projectId,
            rawTitle: "⠏ Ghostex 4.0.0 Beta",
            sessionId: session.sessionId,
            sessionPersistenceProvider: "zmx",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(spinner.status, 200);
      assert.equal(spinner.body.result.activity.activity, "working");
      assert.equal(spinner.body.result.activity.workingSource, "explicit");

      const unrelated = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
        body: {
          params: {
            agentName: "codex",
            projectId: project.projectId,
            rawTitle: "Monaco Ctrl+G Switch",
            sessionId: session.sessionId,
            sessionPersistenceProvider: "zmx",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(unrelated.status, 200);
      assert.equal(unrelated.body.result.activity.activity, "working");
      assert.equal(unrelated.body.result.activity.workingSource, "explicit");
      assert.equal(unrelated.body.result.activity.lastTitle, "⠏ Ghostex 4.0.0 Beta");

      const stopped = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
        body: {
          params: {
            agentName: "codex",
            projectId: project.projectId,
            rawTitle: "Ghostex 4.0.0 Beta",
            sessionId: session.sessionId,
            sessionPersistenceProvider: "zmx",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(stopped.status, 200);
      assert.equal(stopped.body.result.activity.activity, "idle");
      assert.equal(stopped.body.result.activity.workingSource, undefined);
      assert.equal(stopped.body.result.session.runtimeSettings.agentActivity.activity, "idle");
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0),
    },
  );
});

test("terminal title API keeps launch-suppressed title-only Codex spinner idle", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createAgentSession", {
          body: {
            params: {
              agentId: "codex",
              projectId: project.projectId,
              title: "Agent Detection Sync",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const spinner = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
        body: {
          params: {
            agentName: "codex",
            projectId: project.projectId,
            rawTitle: "⠦ Agent Detection Sync",
            sessionId: session.sessionId,
            sessionPersistenceProvider: "zmx",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(spinner.status, 200);
      assert.equal(spinner.body.result.activity.activity, "idle");
      assert.equal(spinner.body.result.enteredAttention, false);
      assert.equal(spinner.body.result.session.runtimeSettings.agentActivity.activity, "idle");
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0),
    },
  );
});

test("terminal title API clears missed Codex Stop from trusted metadata title", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createAgentSession", {
          body: {
            params: {
              agentId: "codex",
              projectId: project.projectId,
              runtimeSettings: {
                titleMetadataSource: "agent-metadata",
                titleSource: "terminal-auto",
              },
              title: "Agents Hub Visibility Fix",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      /*
      CDXC:SessionStatus 2026-06-12-04:06:
      A Codex Stop hook can be missed after UserPromptSubmit starts explicit working. If gxserver later observes the same trusted metadata title without spinner state, the terminal-title API should settle the row instead of leaving every client in working forever.
      */
      const working = await requestJson(baseUrl, "/api/updateAgentActivity", {
        body: {
          params: {
            activity: "working",
            agentName: "codex",
            nowMs: Date.parse("2026-06-12T00:02:06.814Z"),
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(working.status, 200);
      assert.equal(working.body.result.activity.activity, "working");
      assert.equal(working.body.result.activity.workingSource, "explicit");

      const settled = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
        body: {
          params: {
            agentName: "codex",
            projectId: project.projectId,
            rawTitle: "Agents Hub Visibility Fix",
            sessionId: session.sessionId,
            sessionPersistenceProvider: "zmx",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(settled.status, 200);
      assert.equal(settled.body.result.activity.activity, "attention");
      assert.equal(settled.body.result.activity.workingSource, undefined);
      assert.equal(settled.body.result.session.runtimeSettings.agentActivity.activity, "attention");
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0),
    },
  );
});

test("missing cwd blocks restore when the provider session is missing", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: {
            params: { name: "Ghostex", path: path.join(paths.rootDir, "deleted-project") },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: { params: { projectId: project.projectId, title: "Dead cwd" }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.session;

      const response = await requestJson(baseUrl, "/api/attachSessionMetadata", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId, startupText: "codex resume abc\n" },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });

      assert.equal(response.status, 200);
      assert.equal(response.body.result.attach.providerState.lifecycleState, "missing");
      assert.equal(response.body.result.attach.restoreBlocked.reason, "missingCwd");
      assert.equal(response.body.result.attach.attachCommand, undefined);
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 1),
    },
  );
});

test("explicit sleep and close kill the zmx provider session", async () => {
  const calls: string[] = [];
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: { params: { projectId: project.projectId, title: "Kill me" }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.session;

      const sleep = await requestJson(baseUrl, "/api/sleepSession", {
        body: { params: { projectId: project.projectId, sessionId: session.sessionId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      assert.equal(sleep.status, 200);
      assert.equal(sleep.body.result.kill.killed, true);
      assert.equal(sleep.body.result.session.lifecycleState, "sleeping");

      const close = await requestJson(baseUrl, "/api/killSession", {
        body: { params: { projectId: project.projectId, sessionId: session.sessionId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      assert.equal(close.status, 200);
      assert.equal(close.body.result.kill.killed, true);
      assert.equal(close.body.result.session.lifecycleState, "stopped");
      assert.equal(calls.filter((script) => script.includes('kill "$zmx_session" --force')).length, 2);
    },
    {
      zmxLifecycle: fakeZmxLifecycle(calls, () => 0),
    },
  );
});

test("stopped sessions cannot be revived by late metadata updates", async () => {
  const calls: string[] = [];
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createAgentSession", {
          body: {
            params: {
              agentId: "codex",
              projectId: project.projectId,
              providerState: { lifecycleState: "exists" },
              runtimeSettings: { titleSource: "placeholder" },
              title: "Worktree: Codex",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const close = await requestJson(baseUrl, "/api/transitionSession", {
        body: {
          params: { action: "close", projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(close.status, 200);
      assert.equal(close.body.result.session.lifecycleState, "stopped");
      assert.equal(close.body.result.session.providerState.lifecycleState, "missing");

      const rejectedLifecycle = await requestJson(baseUrl, "/api/updateSession", {
        body: {
          params: {
            lifecycleState: "running",
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(rejectedLifecycle.status, 400);
      assert.equal(rejectedLifecycle.body.error, "badRequest");
      assert.match(rejectedLifecycle.body.message, /cannot change a stopped session to running/u);

      const rejectedProvider = await requestJson(baseUrl, "/api/updateSession", {
        body: {
          params: {
            projectId: project.projectId,
            providerState: { lifecycleState: "exists" },
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(rejectedProvider.status, 400);
      assert.equal(rejectedProvider.body.error, "badRequest");
      assert.match(rejectedProvider.body.message, /cannot mark a stopped session provider as exists/u);

      const lateActivity = await requestJson(baseUrl, "/api/updateAgentActivity", {
        body: {
          params: {
            activity: "working",
            nowMs: Date.parse("2026-06-05T08:25:00.000Z"),
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(lateActivity.status, 200);
      assert.equal(lateActivity.body.result.session.lifecycleState, "stopped");

      const lateTitle = await requestJson(baseUrl, "/api/ingestTerminalTitleEvent", {
        body: {
          params: {
            projectId: project.projectId,
            rawTitle: "Late title after close",
            sessionId: session.sessionId,
            sessionPersistenceProvider: "zmx",
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(lateTitle.status, 200);
      assert.equal(lateTitle.body.result.session.lifecycleState, "stopped");

      const flags = await requestJson(baseUrl, "/api/updateSession", {
        body: {
          params: {
            isFavorite: true,
            projectId: project.projectId,
            sessionId: session.sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(flags.status, 200);
      assert.equal(flags.body.result.session.isFavorite, true);
      assert.equal(flags.body.result.session.lifecycleState, "stopped");

      const presentation = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
        body: {
          params: {},
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(presentation.status, 200);
      const projected = presentation.body.result.snapshot.sessions.find(
        (candidate: { projectId: string; sessionId: string }) =>
          candidate.projectId === project.projectId && candidate.sessionId === session.sessionId,
      );
      assert.equal(projected.lifecycleState, "stopped");
      assert.equal(projected.visibleInSidebarByDefault, false);
      assert.equal(calls.filter((script) => script.includes('kill "$zmx_session" --force')).length, 1);
    },
    {
      zmxLifecycle: fakeZmxLifecycle(calls, () => 0),
    },
  );
});

test("transitionSession mutates lifecycle without owning client focus target", async () => {
  const calls: string[] = [];
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const sessions: Array<{ sessionId: string }> = [];
      for (const title of ["Closing", "Missing Backend", "Live Backend"]) {
        sessions.push(
          (
            await requestJson(baseUrl, "/api/createSession", {
              body: {
                params: {
                  lifecycleState: "running",
                  projectId: project.projectId,
                  providerState: { lifecycleState: "exists" },
                  title,
                },
                protocolVersion: GXSERVER_PROTOCOL_VERSION,
              },
              method: "POST",
              token,
            })
          ).body.result.session,
        );
      }

      const transition = await requestJson(baseUrl, "/api/transitionSession", {
        body: {
          params: {
            action: "close",
            projectId: project.projectId,
            sessionId: sessions[0].sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });

      assert.equal(transition.status, 200);
      assert.equal(transition.body.result.action, "close");
      assert.equal(transition.body.result.session.lifecycleState, "stopped");
      assert.equal(transition.body.result.transition.kill.killed, true);
      /*
      CDXC:ProjectSidebarOwnership 2026-06-02-13:01:
      gxserver transition owns close/sleep lifecycle only. The macOS app computes next selected row/tab from local visual order, so the API must not return a focus target or probe sibling sessions for focus eligibility.
      */
      assert.equal("focusTarget" in transition.body.result, false);
      assert.equal(calls.filter((script) => script.includes('kill "$zmx_session" --force')).length, 1);
      assert.equal(calls.filter((script) => script.includes("list --short")).length, 0);
    },
    {
      zmxLifecycle: fakeZmxLifecycle(calls, () => 0),
    },
  );
});

test("transitionSession sleep keeps the sidebar row without owning pane-tab focus", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const sessions: Array<{ sessionId: string }> = [];
      for (const title of ["Left", "Sleep Me", "Already Sleeping", "Right"]) {
        sessions.push(
          (
            await requestJson(baseUrl, "/api/createSession", {
              body: {
                params: {
                  lifecycleState: title === "Already Sleeping" ? "sleeping" : "running",
                  projectId: project.projectId,
                  providerState: { lifecycleState: "exists" },
                  title,
                },
                protocolVersion: GXSERVER_PROTOCOL_VERSION,
              },
              method: "POST",
              token,
            })
          ).body.result.session,
        );
      }

      const transition = await requestJson(baseUrl, "/api/transitionSession", {
        body: {
          params: {
            action: "sleep",
            projectId: project.projectId,
            sessionId: sessions[1].sessionId,
          },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });

      assert.equal(transition.status, 200);
      assert.equal(transition.body.result.action, "sleep");
      assert.equal(transition.body.result.session.lifecycleState, "sleeping");
      assert.equal("focusTarget" in transition.body.result, false);

      const list = await requestJson(baseUrl, "/api/listSessions", {
        body: { params: { projectId: project.projectId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      const slept = list.body.result.sessions.find((session: Record<string, unknown>) => session.sessionId === sessions[1].sessionId);
      assert.equal(slept.lifecycleState, "sleeping");
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0),
    },
  );
});

test("sleep failure keeps stored zmx provider route unknown instead of missing", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: {
            params: {
              projectId: project.projectId,
              providerState: { lifecycleState: "exists" },
              title: "Sleep failure stays attachable",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const sleep = await requestJson(baseUrl, "/api/sleepSession", {
        body: { params: { projectId: project.projectId, sessionId: session.sessionId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });

      assert.equal(sleep.status, 200);
      assert.equal(sleep.body.result.kill.killed, false);
      assert.equal(sleep.body.result.kill.exitCode, 42);
      assert.match(sleep.body.result.kill.error, /zmx kill failed/);
      assert.equal(sleep.body.result.session.lifecycleState, "unknown");
      assert.equal(sleep.body.result.session.providerState.lifecycleState, "unknown");
      assert.equal(sleep.body.result.session.providerState.zmxName, session.zmxName);
      assert.match(sleep.body.result.session.providerState.killError, /zmx kill failed/);

      const list = await requestJson(baseUrl, "/api/listSessions", {
        body: { params: { projectId: project.projectId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      const stored = list.body.result.sessions.find((candidate: Record<string, unknown>) => candidate.sessionId === session.sessionId);
      assert.equal(stored.lifecycleState, "unknown");
      assert.equal(stored.providerState.lifecycleState, "unknown");
      assert.equal(stored.providerState.zmxName, session.zmxName);

      const logs = await readFile(paths.logFile, "utf8");
      assert.match(logs, /"event":"zmx.kill.failed"/);
      assert.match(logs, /"reason":"sleepSession"/);
      assert.match(logs, /zmx kill failed/);
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0, { killExitCode: () => 42 }),
    },
  );
});

test("kill failure keeps stored zmx provider route unknown instead of stopped and missing", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: {
            params: {
              projectId: project.projectId,
              providerState: { lifecycleState: "exists" },
              title: "Kill failure stays attachable",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const close = await requestJson(baseUrl, "/api/killSession", {
        body: { params: { projectId: project.projectId, sessionId: session.sessionId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });

      assert.equal(close.status, 200);
      assert.equal(close.body.result.kill.killed, false);
      assert.equal(close.body.result.kill.exitCode, 43);
      assert.match(close.body.result.kill.error, /zmx kill failed/);
      assert.equal(close.body.result.session.lifecycleState, "unknown");
      assert.equal(close.body.result.session.providerState.lifecycleState, "unknown");
      assert.equal(close.body.result.session.providerState.zmxName, session.zmxName);
      assert.match(close.body.result.session.providerState.probeError, /zmx kill failed/);

      const list = await requestJson(baseUrl, "/api/listSessions", {
        body: { params: { projectId: project.projectId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      const stored = list.body.result.sessions.find((candidate: Record<string, unknown>) => candidate.sessionId === session.sessionId);
      assert.equal(stored.lifecycleState, "unknown");
      assert.equal(stored.providerState.lifecycleState, "unknown");
      assert.equal(stored.providerState.zmxName, session.zmxName);

      const logs = await readFile(paths.logFile, "utf8");
      assert.match(logs, /"event":"zmx.kill.failed"/);
      assert.match(logs, /"reason":"killSession"/);
      assert.match(logs, /zmx kill failed/);
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0, { killExitCode: () => 43 }),
    },
  );
});

test("migrated zmx sessions use the canonical server-project-session zmx name", async () => {
  const calls: string[] = [];
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: {
            params: {
              projectId: project.projectId,
              providerState: { legacyProvider: "zmx", legacyProviderSessionName: "legacy-zmx-live" },
              title: "Migrated live zmx",
            },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const attach = await requestJson(baseUrl, "/api/attachSessionMetadata", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(attach.status, 200);
      const expectedZmxName = `S7k-${project.projectId}-${session.sessionId}`;
      assert.equal(attach.body.result.attach.zmxName, expectedZmxName);
      assert.match(attach.body.result.attach.attachCommand, new RegExp(expectedZmxName));

      const sleep = await requestJson(baseUrl, "/api/sleepSession", {
        body: { params: { projectId: project.projectId, sessionId: session.sessionId }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      assert.equal(sleep.status, 200);
      assert.equal(calls.some((script) => script.includes(`zmx_session='${expectedZmxName}'`)), true);
    },
    {
      zmxLifecycle: fakeZmxLifecycle(calls, () => 0),
    },
  );
});

test("attach metadata promotes existing client-created providers into the macOS presentation snapshot", async () => {
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: {
            params: { projectId: project.projectId, title: "TUI-created terminal" },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;
      assert.equal(session.lifecycleState, "unknown");

      const beforeAttach = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
        body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      assert.equal(beforeAttach.status, 200);
      assert.equal(
        beforeAttach.body.result.snapshot.sessions.some(
          (candidate: { sessionId: string }) => candidate.sessionId === session.sessionId,
        ),
        false,
      );

      const attach = await requestJson(baseUrl, "/api/attachSessionMetadata", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(attach.status, 200);
      assert.equal(attach.body.result.attach.providerState.lifecycleState, "exists");
      assert.equal(attach.body.result.attach.session.lifecycleState, "running");
      assert.doesNotMatch(attach.body.result.attach.attachCommand, /--prompt-editor=monaco/);

      const afterAttach = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
        body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      assert.equal(afterAttach.status, 200);
      const presentationSession = afterAttach.body.result.snapshot.sessions.find(
        (candidate: { sessionId: string }) => candidate.sessionId === session.sessionId,
      );
      assert.equal(presentationSession?.lifecycleState, "running");
      assert.equal(presentationSession?.visibleInSidebarByDefault, true);
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => 0),
    },
  );
});

test("startSessionProvider promotes missing plain terminal providers into presentation", async () => {
  const calls: string[] = [];
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: {
            params: { projectId: project.projectId, title: "TUI Button Labels" },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const attachBeforeStart = await requestJson(baseUrl, "/api/attachSessionMetadata", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(attachBeforeStart.status, 200);
      assert.equal(attachBeforeStart.body.result.attach.providerState.lifecycleState, "missing");
      assert.equal(attachBeforeStart.body.result.attach.session.lifecycleState, "unknown");

      const start = await requestJson(baseUrl, "/api/startSessionProvider", {
        body: {
          params: { projectId: project.projectId, sessionId: session.sessionId },
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        },
        method: "POST",
        token,
      });
      assert.equal(start.status, 200);
      assert.equal(start.body.result.started, true);
      assert.equal(start.body.result.startupTextDisposition, "none");
      assert.equal(start.body.result.providerState.lifecycleState, "exists");
      assert.equal(start.body.result.session.lifecycleState, "running");
      const runScript = calls.find((script) =>
        script.includes('run "$zmx_session" -d --initial-command /bin/zsh -lic "$zmx_shell_command"')
      );
      assert.ok(runScript);
      assert.match(runScript, /ghostex_prompt_editor_wrapper="\$ghostex_prompt_editor_home\/state\/prompt-editor"/);
      assert.equal(calls.some((script) => script.includes("gxserver startSessionProvider requires startup text")), false);

      const presentation = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
        body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      assert.equal(presentation.status, 200);
      const presentationSession = presentation.body.result.snapshot.sessions.find(
        (candidate: { sessionId: string }) => candidate.sessionId === session.sessionId,
      );
      assert.equal(presentationSession?.lifecycleState, "running");
      assert.equal(presentationSession?.visibleInSidebarByDefault, true);
    },
    {
      zmxLifecycle: fakeZmxLifecycle(calls, () => 1, {
        runExitCode: () => 0,
      }),
    },
  );
});

test("provider probe API caches exists, missing, and unknown separately from native pane state", async () => {
  let probeExitCode = 0;
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: { params: { name: "Ghostex", path: paths.rootDir }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.project;
      const session = (
        await requestJson(baseUrl, "/api/createSession", {
          body: { params: { projectId: project.projectId, title: "Probe me" }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
          method: "POST",
          token,
        })
      ).body.result.session;

      for (const [exitCode, expected] of [
        [0, "exists"],
        [1, "missing"],
        [127, "unknown"],
      ] as const) {
        probeExitCode = exitCode;
        const response = await requestJson(baseUrl, "/api/probeSessionProvider", {
          body: {
            params: { projectId: project.projectId, sessionId: session.sessionId },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        });
        assert.equal(response.status, 200);
        assert.equal(response.body.result.providerState.lifecycleState, expected);
        assert.equal(response.body.result.session.providerState.lifecycleState, expected);
      }
    },
    {
      zmxLifecycle: fakeZmxLifecycle([], () => probeExitCode),
    },
  );
});

test("control-plane stop preserves zmx sessions", async () => {
  const calls: string[] = [];
  await withApiServer(
    "local",
    async ({ baseUrl, token }) => {
      const response = await requestJson(baseUrl, "/api/control/stop", {
        body: { protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      assert.equal(response.status, 200);
      assert.deepEqual(calls, []);
    },
    {
      zmxLifecycle: fakeZmxLifecycle(calls, () => {
        throw new Error("control-plane stop must not probe or kill zmx");
      }),
    },
  );
});

test("stop-all kills tracked zmx sessions before stopping the control plane", async () => {
  const calls: string[] = [];
  await withApiServer(
    "local",
    async ({ baseUrl, paths, token }) => {
      /*
      CDXC:GxserverCli 2026-06-02-18:45:
      stop-all is the user-requested destructive companion to control-plane
      stop. It must kill tracked zmx sessions and persist stopped state before
      shutdown, while ordinary stop keeps preserving zmx.
      */
      const project = (
        await requestJson(baseUrl, "/api/createProject", {
          body: {
            params: { name: "Ghostex", path: paths.rootDir },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.project;
      const first = (
        await requestJson(baseUrl, "/api/createSession", {
          body: {
            params: { projectId: project.projectId, title: "First" },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;
      const second = (
        await requestJson(baseUrl, "/api/createSession", {
          body: {
            params: { projectId: project.projectId, title: "Second" },
            protocolVersion: GXSERVER_PROTOCOL_VERSION,
          },
          method: "POST",
          token,
        })
      ).body.result.session;

      const response = await requestJson(baseUrl, "/api/control/stopAll", {
        body: { protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      assert.equal(response.status, 200);
      assert.deepEqual(response.body.result, {
        attemptedSessions: 2,
        failedSessions: 0,
        killedSessions: 2,
        skippedSessions: 0,
        uniqueZmxSessions: 2,
      });
      assert.equal(calls.filter((script) => script.includes('kill "$zmx_session" --force')).length, 2);

      const listed = await requestJson(baseUrl, "/api/listSessions", {
        body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
        method: "POST",
        token,
      });
      assert.equal(listed.status, 200);
      const sessions = listed.body.result.sessions;
      assert.equal(sessions.find((session: any) => session.sessionId === first.sessionId)?.lifecycleState, "stopped");
      assert.equal(sessions.find((session: any) => session.sessionId === second.sessionId)?.lifecycleState, "stopped");
    },
    {
      zmxLifecycle: fakeZmxLifecycle(calls, () => 0),
    },
  );
});

test("remote project add validates server-side paths and typed operations stay scoped", async () => {
  await withApiServer("remote", async ({ baseUrl, paths, token }) => {
    const repoPath = path.join(paths.rootDir, "registered-repo");
    const otherPath = path.join(paths.rootDir, "unregistered-repo");
    await mkdir(repoPath, { recursive: true });
    await mkdir(otherPath, { recursive: true });

    const missingProject = await requestJson(baseUrl, "/api/addProjectPath", {
      body: {
        params: { path: path.join(paths.rootDir, "missing-repo") },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(missingProject.status, 404);

    const addedProject = await requestJson(baseUrl, "/api/addProjectPath", {
      body: {
        params: { path: repoPath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(addedProject.status, 200);
    assert.equal(addedProject.body.result.project.path, repoPath);
    assert.equal(addedProject.body.result.project.name, "registered-repo");

    const status = await requestJson(baseUrl, "/api/runGitAction", {
      body: {
        params: { action: "status", projectId: addedProject.body.result.project.projectId },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(status.status, 200);
    assert.equal(status.body.result.action, "status");
    assert.deepEqual(status.body.result.command.args, ["status", "--short", "--branch"]);

    const updatedProject = await requestJson(baseUrl, "/api/updateProject", {
      body: {
        params: {
          gitConfig: { worktreeCommand: "printf setup-done > setup-result.txt" },
          projectId: addedProject.body.result.project.projectId,
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(updatedProject.status, 200);

    const setupCommand = await requestJson(baseUrl, "/api/runProjectSetupCommand", {
      body: {
        params: {
          action: "worktreeSetupCommand",
          projectId: addedProject.body.result.project.projectId,
          setupCommandProjectId: addedProject.body.result.project.projectId,
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    /*
    CDXC:GxserverTypedOperations 2026-06-03-04:08:
    Remote worktree setup may execute only the command stored on registered
    project metadata. The API exposes a typed setup endpoint instead of
    accepting arbitrary process args, and redacts the stored command text from
    command metadata.
    */
    assert.equal(setupCommand.status, 200);
    assert.equal(setupCommand.body.result.action, "worktreeSetupCommand");
    assert.deepEqual(setupCommand.body.result.command.args, ["-lc", "<worktree setup command>"]);
    assert.equal(await readFile(path.join(repoPath, "setup-result.txt"), "utf8"), "setup-done");

    const unregistered = await requestJson(baseUrl, "/api/runGitAction", {
      body: {
        params: { action: "status", projectPath: otherPath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(unregistered.status, 403);
    assert.equal(unregistered.body.error, "forbidden");

    const genericRunProcess = await requestJson(baseUrl, "/api/runProcess", {
      body: { params: { args: ["status"], executable: "git" }, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    /*
    CDXC:GxserverTypedOperations 2026-06-02-15:33:
    Generic process execution is not a gxserver API. Git, worktree, Beads, clone, and GitHub workflows must stay on typed endpoints so native clients cannot bypass project scoping or reintroduce a broad backend shell bridge.
    */
    assert.equal(genericRunProcess.status, 404);
    assert.equal(genericRunProcess.body.error, "notFound");
  });
});

test("remote project directory browse mirrors picker directory filtering without generic filesystem access", async () => {
  await withApiServer("remote", async ({ baseUrl, paths, token }) => {
    /*
    CDXC:RemoteProjectPicker 2026-06-02-23:22:
    Remote Add Project needs T3 Code-style directory browsing through the remote gxserver. The endpoint is remote-allowed because SSH already authenticated the daemon tunnel, but it returns only directory entries for picker navigation and leaves the broad filesystem endpoint blocked.
    */
    const parentPath = path.join(paths.rootDir, "picker-parent");
    await mkdir(path.join(parentPath, "alpha"), { recursive: true });
    await mkdir(path.join(parentPath, "alpine"), { recursive: true });
    await mkdir(path.join(parentPath, "beta"), { recursive: true });
    await mkdir(path.join(parentPath, ".hidden"), { recursive: true });
    await writeFile(path.join(parentPath, "alphabet.txt"), "not a directory\n", "utf8");

    const filtered = await requestJson(baseUrl, "/api/browseProjectDirectories", {
      body: {
        params: { partialPath: path.join(parentPath, "al"), limit: 5 },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(filtered.status, 200);
    assert.equal(filtered.body.result.parentPath, parentPath);
    assert.deepEqual(
      filtered.body.result.entries.map((entry: any) => entry.name),
      ["alpha", "alpine"],
    );

    const hidden = await requestJson(baseUrl, "/api/browseProjectDirectories", {
      body: {
        params: { partialPath: `${parentPath}${path.sep}.h` },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(hidden.status, 200);
    assert.deepEqual(
      hidden.body.result.entries.map((entry: any) => entry.name),
      [".hidden"],
    );

    const relative = await requestJson(baseUrl, "/api/browseProjectDirectories", {
      body: {
        params: { cwd: parentPath, partialPath: "./a" },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(relative.status, 200);
    assert.deepEqual(
      relative.body.result.entries.map((entry: any) => entry.name),
      ["alpha", "alpine"],
    );

    const genericBrowse = await requestJson(baseUrl, "/api/browseFilesystem", {
      body: {
        params: { partialPath: parentPath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(genericBrowse.status, 403);
    assert.equal(genericBrowse.body.error, "forbidden");
  });
});

test("project path registration is idempotent and session creation resolves stale client ids by cwd", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    /*
    CDXC:GxserverVerification 2026-05-31-17:47:
    macOS, CLI/TUI, and mobile clients must not have to invent daemon project ids. Adding the same filesystem project twice returns the existing P-id, and createSession resolves cwd/projectPath before honoring stale client-local ids such as `project-*`.
    */
    const repoPath = path.join(paths.rootDir, "registered-repo");
    await mkdir(repoPath, { recursive: true });

    const firstAdd = await requestJson(baseUrl, "/api/addProjectPath", {
      body: {
        params: { name: "Registered Repo", path: repoPath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(firstAdd.status, 200);
    const projectId = firstAdd.body.result.project.projectId;
    assert.match(projectId, /^P[0-9][a-z0-9]{3}$/u);

    const secondAdd = await requestJson(baseUrl, "/api/addProjectPath", {
      body: {
        params: { path: repoPath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(secondAdd.status, 200);
    assert.equal(secondAdd.body.result.project.projectId, projectId);

    const created = await requestJson(baseUrl, "/api/createSession", {
      body: {
        params: {
          cwd: repoPath,
          projectId: "project-vanvq8",
          title: "Terminal",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(created.status, 200);
    assert.equal(created.body.result.session.projectId, projectId);
    assert.equal(created.body.result.session.cwd, repoPath);
  });
});

test("project path registration attaches linked worktrees to an existing registered main project", async (t) => {
  if (spawnSync("git", ["--version"], { encoding: "utf8" }).status !== 0) {
    t.skip("git is unavailable");
    return;
  }

  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    /*
    CDXC:WorktreeProjectRegistration 2026-06-01-20:59:
    gxserver owns Add Project worktree detection. Registering a linked worktree path should return a project whose worktree metadata points at the already registered main project P-id, preserving idempotent path registration for repeat adds.
    */
    const repoPath = path.join(paths.rootDir, "registered-main");
    const worktreePath = path.join(paths.rootDir, "registered-main-feature");
    await mkdir(repoPath, { recursive: true });
    runGitForTest(repoPath, ["init"]);
    runGitForTest(repoPath, ["config", "user.email", "ghostex@example.invalid"]);
    runGitForTest(repoPath, ["config", "user.name", "Ghostex Test"]);
    await writeFile(path.join(repoPath, "README.md"), "main\n", "utf8");
    runGitForTest(repoPath, ["add", "README.md"]);
    runGitForTest(repoPath, ["commit", "-m", "Initial commit"]);
    runGitForTest(repoPath, ["worktree", "add", "-b", "feature/existing-worktree", worktreePath]);

    const mainAdd = await requestJson(baseUrl, "/api/addProjectPath", {
      body: {
        params: { name: "Registered Main", path: repoPath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(mainAdd.status, 200);
    const mainProject = mainAdd.body.result.project;

    const worktreeAdd = await requestJson(baseUrl, "/api/addProjectPath", {
      body: {
        params: { path: worktreePath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(worktreeAdd.status, 200);
    const worktreeProject = worktreeAdd.body.result.project;
    assert.equal(worktreeProject.path, worktreePath);
    assert.equal(worktreeProject.worktree.parentProjectId, mainProject.projectId);
    assert.equal(worktreeProject.worktree.parentProjectName, "Registered Main");
    assert.equal(worktreeProject.worktree.parentProjectPath, repoPath);
    assert.equal(worktreeProject.worktree.branch, "feature/existing-worktree");

    const secondWorktreeAdd = await requestJson(baseUrl, "/api/addProjectPath", {
      body: {
        params: { path: worktreePath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(secondWorktreeAdd.status, 200);
    assert.equal(secondWorktreeAdd.body.result.project.projectId, worktreeProject.projectId);
    assert.equal(secondWorktreeAdd.body.result.project.worktree.parentProjectId, mainProject.projectId);
  });
});

test("/api/resolveGitRootForPath resolves arbitrary local directories without registering projects", async (t) => {
  if (spawnSync("git", ["--version"], { encoding: "utf8" }).status !== 0) {
    t.skip("git is unavailable");
    return;
  }

  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    /*
    CDXC:GxserverVerification 2026-06-02-12:14:
    CLI/open-file routing asks gxserver for a repository root before the target path is a registered project. The endpoint must return that local fact without mutating project inventory, while the remote listener remains blocked by the endpoint permission test.
    */
    const repoPath = path.join(paths.rootDir, "open-path-repo");
    const nestedPath = path.join(repoPath, "src", "feature");
    const outsidePath = path.join(paths.rootDir, "outside-repo");
    await mkdir(nestedPath, { recursive: true });
    await mkdir(outsidePath, { recursive: true });
    runGitForTest(repoPath, ["init"]);

    const resolved = await requestJson(baseUrl, "/api/resolveGitRootForPath", {
      body: {
        params: { path: nestedPath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(resolved.status, 200);
    assert.equal(resolved.body.result.gitRoot, await realpath(repoPath));

    const projectsAfterResolve = await requestJson(baseUrl, "/api/listProjects", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(projectsAfterResolve.status, 200);
    assert.deepEqual(projectsAfterResolve.body.result.projects, []);

    const outside = await requestJson(baseUrl, "/api/resolveGitRootForPath", {
      body: {
        params: { path: outsidePath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(outside.status, 200);
    assert.deepEqual(outside.body.result, {});
  });
});

test("repository clone preview reports existing default destination and start rejects it", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    const parentPath = path.join(paths.rootDir, "clone-parent");
    await mkdir(path.join(parentPath, "opencode"), { recursive: true });

    const preview = await requestJson(baseUrl, "/api/previewRepositoryClone", {
      body: {
        params: {
          folderPath: parentPath,
          repositoryInput: "gh repo clone anomalyco/opencode",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(preview.status, 200);
    assert.equal(preview.body.result.preview.defaultFolderName, "opencode");
    assert.equal(preview.body.result.preview.destinationFolderName, "opencode");
    assert.equal(preview.body.result.preview.destinationExists, true);
    assert.equal(preview.body.result.preview.destinationExistsKind, "directory");

    const rejectedStart = await requestJson(baseUrl, "/api/startRepositoryClone", {
      body: {
        params: {
          folderPath: parentPath,
          repositoryInput: "gh repo clone anomalyco/opencode",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(rejectedStart.status, 400);
    assert.equal(rejectedStart.body.error, "badRequest");
    assert.match(rejectedStart.body.message, /already exists/);

    const renamedPreview = await requestJson(baseUrl, "/api/previewRepositoryClone", {
      body: {
        params: {
          folderPath: parentPath,
          newFolderName: "opencode-copy",
          repositoryInput: "gh repo clone anomalyco/opencode",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(renamedPreview.status, 200);
    assert.equal(renamedPreview.body.result.preview.destinationFolderName, "opencode-copy");
    assert.equal(renamedPreview.body.result.preview.destinationExists, false);
  });
});

test("/api/queryLogs enforces auth, protocol, method, and returns filtered logs", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    await writeNativeSidebarSettings(paths.homeDir, { debuggingMode: true });
    await createGxserverLogger(paths).log({
      client: "cli",
      event: "agent.detected",
      level: "info",
      projectId: "P3a91",
      serverId: "S7k",
      sessionId: "G8v20",
      ts: "2026-05-30T10:00:00.000Z",
    });
    await createGxserverLogger(paths).log({
      client: "api",
      event: "zmx.kill.failed",
      level: "error",
      projectId: "P3a91",
      serverId: "S7k",
      sessionId: "G8v20",
      ts: "2026-05-30T10:01:00.000Z",
    });

    const wrongMethod = await requestJson(baseUrl, "/api/queryLogs", {
      method: "GET",
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      token,
    });
    assert.equal(wrongMethod.status, 405);
    assert.equal(wrongMethod.body.error, "methodNotAllowed");

    const unauthorized = await requestJson(baseUrl, "/api/queryLogs", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
    });
    assert.equal(unauthorized.status, 401);
    assert.equal(unauthorized.body.error, "unauthorized");

    const missingProtocol = await requestJson(baseUrl, "/api/queryLogs", {
      body: { params: {} },
      method: "POST",
      token,
    });
    assert.equal(missingProtocol.status, 426);
    assert.equal(missingProtocol.body.error, "protocolMismatch");

    const filtered = await requestJson(baseUrl, "/api/queryLogs", {
      body: {
        params: {
          eventPrefix: "agent.",
          limit: 1,
          order: "desc",
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(filtered.status, 200);
    assert.equal(filtered.body.result.entries.length, 1);
    assert.equal(filtered.body.result.entries[0].event, "agent.detected");
    assert.equal(filtered.body.result.malformedLineCount, 0);

    const badParams = await requestJson(baseUrl, "/api/queryLogs", {
      body: {
        params: { limit: 0 },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(badParams.status, 400);
    assert.equal(badParams.body.error, "badRequest");
  });
});

test("authenticated health includes listener, tool, and migration status", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const health = await requestJson(baseUrl, "/api/health/server", {
      method: "GET",
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      token,
    });
    assert.equal(health.status, 200);
    assert.equal(health.body.buildIdentity, "gxserver:0.1.0-test:source");
    assert.equal(health.body.serverId, "S7k");
    assert.equal(health.body.listeners.local.port, GXSERVER_LOCAL_API_PORT);
    assert.equal(health.body.listeners.remote.enabled, false);
    assert.equal(health.body.migration.currentVersion, 9);
    assert.equal(health.body.migration.stateImports.legacyMacosState.id, LEGACY_MACOS_STATE_IMPORT_ID);
    assert.equal(health.body.migration.stateImports.legacyMacosState.status, "notRun");
    assert.equal(Array.isArray(health.body.tools), true);
  });
});

test("WebSocket events require auth and protocol version, then stream JSON server events", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const unauthorized = await websocketRejected(baseUrl, "/api/events");
    assert.equal(unauthorized.statusCode, 401);

    const wrongProtocol = await websocketRejected(baseUrl, "/api/events", {
      protocolVersion: 999,
      token,
    });
    assert.equal(wrongProtocol.statusCode, 426);

    const socket = new WebSocket(toWebSocketUrl(baseUrl, "/api/events"), {
      headers: authHeaders(token, GXSERVER_PROTOCOL_VERSION),
    });
    const readyMessage = onceMessage(socket);
    await waitFor(once(socket, "open"), "WebSocket open");
    const ready = JSON.parse(String(await readyMessage)) as Record<string, unknown>;
    assert.equal(ready.type, "eventStreamReady");
    assert.equal(ready.protocolVersion, GXSERVER_PROTOCOL_VERSION);

    const browserSocket = new WebSocket(
      `${toWebSocketUrl(baseUrl, "/api/events")}?protocolVersion=${GXSERVER_PROTOCOL_VERSION}&authToken=${encodeURIComponent(token)}`,
    );
    const browserReadyMessage = onceMessage(browserSocket);
    await waitFor(once(browserSocket, "open"), "browser WebSocket open");
    const browserReady = JSON.parse(String(await browserReadyMessage)) as Record<string, unknown>;
    assert.equal(browserReady.type, "eventStreamReady");
    browserSocket.close();

    const handledMessage = onceMessage(socket);
    const response = await requestJson(baseUrl, "/api/listSessions", {
      body: {},
      method: "POST",
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      token,
    });
    assert.equal(response.status, 200);
    const handled = JSON.parse(String(await handledMessage)) as Record<string, unknown>;
    assert.equal(handled.type, "apiRequestHandled");
    assert.equal(handled.path, "/api/listSessions");
    socket.close();
  });
});

test("renderer command endpoint dispatches through subscribed macOS event clients", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const socket = new WebSocket(toWebSocketUrl(baseUrl, "/api/events"), {
      headers: authHeaders(token, GXSERVER_PROTOCOL_VERSION),
    });
    const readyMessage = onceMessage(socket);
    await waitFor(once(socket, "open"), "WebSocket open");
    assert.equal((JSON.parse(String(await readyMessage)) as Record<string, unknown>).type, "eventStreamReady");
    /*
    CDXC:GxserverRendererCommands 2026-06-13-02:24:
    Renderer-only commands use gxserver's authenticated event stream instead of the retired native CLI bridge. The HTTP request waits for a matching renderer result so callers get the same structured command outcome through the daemon contract.
    */
    socket.send(JSON.stringify({
      clientId: "macos-renderer-test",
      rendererCommands: true,
      type: "subscribePresentation",
    }));

    const commandEventPromise = nextWebSocketEvent(socket, "rendererCommand");
    const responsePromise = requestJson(baseUrl, "/api/dispatchRendererCommand", {
      body: {
        params: {
          action: "toggleSidebarCollapsed",
          payload: { source: "test" },
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    const commandEvent = await commandEventPromise;
    const command = commandEvent.command as Record<string, unknown>;
    assert.equal(command.action, "toggleSidebarCollapsed");
    assert.deepEqual(command.payload, { source: "test" });
    assert.equal(typeof command.commandId, "string");

    socket.send(JSON.stringify({
      commandId: command.commandId,
      ok: true,
      result: { ok: true, toggled: true },
      type: "rendererCommandResult",
    }));
    const response = await responsePromise;
    assert.equal(response.status, 200);
    assert.deepEqual(response.body.result, { ok: true, toggled: true });
    socket.close();
  });
});

test("renderer command endpoint reports unavailable when no renderer is connected", async () => {
  await withApiServer("local", async ({ baseUrl, token }) => {
    const response = await requestJson(baseUrl, "/api/dispatchRendererCommand", {
      body: {
        params: {
          action: "toggleSidebarCollapsed",
          payload: {},
        },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(response.status, 503);
    assert.equal(response.body.error, "dependencyUnavailable");
  });
});

test("project mutations publish complete presentation deltas for connected clients", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    /*
    CDXC:GxserverPresentationProjects 2026-06-02-15:04:
    Add/remove project mutations must notify connected clients with enough
    presentation state to update the sidebar without a native project-list
    refetch. Project add/update deltas carry the gxserver domain project cache;
    project removal publishes projectRemoved so clients can suppress stale rows.
    */
    const socket = new WebSocket(toWebSocketUrl(baseUrl, "/api/events"), {
      headers: authHeaders(token, GXSERVER_PROTOCOL_VERSION),
    });
    const readyEventPromise = nextWebSocketEvent(socket, "eventStreamReady");
    await waitFor(once(socket, "open"), "WebSocket open");
    assert.equal((await readyEventPromise).type, "eventStreamReady");

    const projectPath = path.join(paths.rootDir, "presentation-delta-project");
    await mkdir(projectPath, { recursive: true });
    const addedEventPromise = nextWebSocketEvent(socket, "presentationDelta");
    const addedProjectResponse = await requestJson(baseUrl, "/api/addProjectPath", {
      body: {
        params: { name: "Presentation Delta Project", path: projectPath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(addedProjectResponse.status, 200);
    const projectId = addedProjectResponse.body.result.project.projectId;

    const addedEvent = await addedEventPromise;
    assert.equal(addedEvent.type, "presentationDelta");
    const addedDelta = addedEvent.delta as Record<string, unknown>;
    assert.equal(addedDelta.type, "projectAdded");
    assert.equal((addedDelta.project as Record<string, unknown>).projectId, projectId);
    assert.equal((addedDelta.project as Record<string, unknown>).path, projectPath);
    assert.equal((addedDelta.domainProject as Record<string, unknown>).projectId, projectId);
    assert.equal((addedDelta.domainProject as Record<string, unknown>).path, projectPath);

    const removedEventPromise = nextWebSocketEvent(socket, "presentationDelta");
    const removedProjectResponse = await requestJson(baseUrl, "/api/removeProject", {
      body: {
        params: { projectId },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(removedProjectResponse.status, 200);

    const removedEvent = await removedEventPromise;
    assert.equal(removedEvent.type, "presentationDelta");
    assert.deepEqual(removedEvent.delta, {
      projectId,
      type: "projectRemoved",
    });
    socket.close();
  });
});

test("session mutations advance presentation revision without connected clients", async () => {
  await withApiServer("local", async ({ baseUrl, paths, token }) => {
    /*
    CDXC:GxserverPresentationSessions 2026-06-02-19:31:
    Shared session mutations must update gxserver presentation revision even when no WebSocket client is connected. Reconnecting macOS sidebars then receive a current snapshot/revision instead of compensating with native-owned project/session refetches.
    */
    const projectPath = path.join(paths.rootDir, "session-revision-project");
    await mkdir(projectPath, { recursive: true });
    const createdProject = await requestJson(baseUrl, "/api/addProjectPath", {
      body: {
        params: { name: "Session Revision Project", path: projectPath },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(createdProject.status, 200);
    const projectId = createdProject.body.result.project.projectId;

    const snapshotBeforeSession = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(snapshotBeforeSession.status, 200);
    const snapshotBeforeSessionBody = snapshotBeforeSession.body.result.snapshot;
    /*
    CDXC:GxserverPresentationProjects 2026-06-06-23:16:
    Add Project must create a presentation project and empty active group before any terminal session exists. This is the daemon-side invariant that keeps 3.6-to-4.0 upgrades and post-startup project additions from rendering an empty sidebar while the project row already exists in SQLite.
    */
    assert.equal(
      snapshotBeforeSessionBody.projects.some((project: Record<string, unknown>) => project.projectId === projectId),
      true,
    );
    const projectGroupBeforeSession = snapshotBeforeSessionBody.groups.find(
      (group: Record<string, unknown>) => group.projectId === projectId,
    );
    assert.equal(projectGroupBeforeSession?.["groupId"], `${projectId}:active`);
    assert.deepEqual(projectGroupBeforeSession?.["sessionIds"], []);
    const revisionBeforeSession = Number(snapshotBeforeSessionBody.revision);

    const createdSession = await requestJson(baseUrl, "/api/createAgentSession", {
      body: {
        params: { agentId: "codex", projectId, title: "Revision Session" },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    assert.equal(createdSession.status, 200);
    const sessionId = createdSession.body.result.session.sessionId;

    const snapshotAfterSession = await requestJson(baseUrl, "/api/readPresentationSnapshot", {
      body: { params: {}, protocolVersion: GXSERVER_PROTOCOL_VERSION },
      method: "POST",
      token,
    });
    assert.equal(snapshotAfterSession.status, 200);
    assert.equal(snapshotAfterSession.body.result.snapshot.sessions[0]?.sessionId, sessionId);
    assert.ok(Number(snapshotAfterSession.body.result.snapshot.revision) > revisionBeforeSession);
  });
});

interface ServerFixture {
  baseUrl: string;
  paths: GxserverPaths;
  token: GxserverAuthToken;
}

interface RunningServerFixture extends ServerFixture {
  close: () => Promise<void>;
}

async function withApiServer(
  listenerKind: "local" | "remote",
  run: (fixture: ServerFixture) => Promise<void>,
  options: {
    configureConfig?: (config: GxserverConfig) => GxserverConfig;
    firstPromptTitleGeneration?: GxserverApiRuntime["firstPromptTitleGeneration"];
    zmxLifecycle?: GxserverApiRuntime["zmxLifecycle"];
  } = {},
): Promise<void> {
  const homeDir = await mkdtemp(path.join(tmpdir(), "gxserver-api-"));
  let fixture: RunningServerFixture | undefined;
  try {
    fixture = await startApiServerFixture(homeDir, listenerKind, options);
    await run(fixture);
  } finally {
    await fixture?.close();
    await rm(homeDir, { force: true, recursive: true });
  }
}

async function startApiServerFixture(
  homeDir: string,
  listenerKind: "local" | "remote",
  options: {
    configureConfig?: (config: GxserverConfig) => GxserverConfig;
    firstPromptTitleGeneration?: GxserverApiRuntime["firstPromptTitleGeneration"];
    zmxLifecycle?: GxserverApiRuntime["zmxLifecycle"];
  } = {},
): Promise<RunningServerFixture> {
  const paths = getGxserverPaths(homeDir);
  const storage = await initializeGxserverStorage(paths);
  let config = await readGxserverConfig(paths);
  if (options.configureConfig) {
    config = options.configureConfig(config);
    await writeGxserverConfig(paths, config);
    config = await readGxserverConfig(paths);
  }
  const auth = await ensureGxserverAuthToken(paths);
  const metadata: GxserverRuntimeMetadata = {
    buildIdentity: "gxserver:0.1.0-test:source",
    pid: process.pid,
    port: GXSERVER_LOCAL_API_PORT,
    protocolVersion: GXSERVER_PROTOCOL_VERSION,
    serverId: "S7k",
    startedAt: "2026-05-30T10:26:00.000Z",
    version: "0.1.0-test",
  };
  const eventHub = new GxserverEventHub(metadata.serverId);
  const runtime: GxserverApiRuntime = {
    authToken: auth.token,
    buildIdentity: metadata.buildIdentity,
    config,
    eventHub,
    logger: createGxserverLogger(paths),
    metadata,
    migration: createGxserverMigrationStatus(storage, undefined, {
      legacyMacosState: {
        id: LEGACY_MACOS_STATE_IMPORT_ID,
        status: "notRun",
      },
    }),
    paths,
    shutdown: () => undefined,
    version: "0.1.0-test",
    ...(options.firstPromptTitleGeneration ? { firstPromptTitleGeneration: options.firstPromptTitleGeneration } : {}),
    ...(options.zmxLifecycle ? { zmxLifecycle: options.zmxLifecycle } : {}),
  };
  const server = createGxserverHttpServer(runtime, listenerKind);
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  if (typeof address !== "object" || address === null || !("port" in address)) {
    throw new Error("Expected gxserver test HTTP server to listen on a TCP port.");
  }
  return {
    baseUrl: `http://127.0.0.1:${address.port}`,
    close: async () => {
      await eventHub.close();
      await closeServer(server);
    },
    paths,
    token: auth.token,
  };
}

function fakeZmxLifecycle(
  calls: string[],
  probeExitCode: () => number,
  options: {
    killExitCode?: () => number;
    runExitCode?: () => number;
    stdinInputs?: string[];
  } = {},
): NonNullable<GxserverApiRuntime["zmxLifecycle"]> {
  const runZsh: GxserverZmxCommandRunner = async (script, commandOptions) => {
    calls.push(script);
    if (commandOptions?.stdin !== undefined) {
      options.stdinInputs?.push(commandOptions.stdin);
    }
    if (script.includes('run "$zmx_session" -d')) {
      const exitCode = options.runExitCode?.() ?? 0;
      return { exitCode, stderr: exitCode === 0 ? "" : `zmx run failed with exit ${exitCode}`, stdout: "" };
    }
    if (script.includes('kill "$zmx_session" --force')) {
      const exitCode = options.killExitCode?.() ?? 0;
      return { exitCode, stderr: exitCode === 0 ? "" : `zmx kill failed with exit ${exitCode}`, stdout: "" };
    }
    const exitCode = probeExitCode();
    return {
      exitCode,
      stderr: exitCode === 127 ? "zmx probe failed" : "",
      stdout: "",
    };
  };
  return {
    requireZmx: async () => ({
      executablePath: "/fake/bundled/zmx",
      source: "devSubmodule",
      tool: "zmx",
    }),
    runZsh,
  };
}

function nestedJson(depth: number): Record<string, unknown> {
  let value: Record<string, unknown> = { leaf: true };
  for (let index = 0; index < depth; index += 1) {
    value = { child: value };
  }
  return value;
}

async function requestJson(
  baseUrl: string,
  pathname: string,
  options: {
    body?: unknown;
    method: "GET" | "OPTIONS" | "POST";
    origin?: string;
    protocolVersion?: number;
    requestPrivateNetwork?: boolean;
    token?: GxserverAuthToken;
  },
): Promise<{ body: Record<string, any>; headers: Headers; status: number }> {
  const headers: Record<string, string> = {};
  if (options.origin) {
    headers.origin = options.origin;
  }
  if (options.token) {
    headers.authorization = `Bearer ${options.token}`;
  }
  if (options.protocolVersion !== undefined) {
    headers[GXSERVER_PROTOCOL_HEADER] = String(options.protocolVersion);
  }
  if (options.body !== undefined) {
    headers["content-type"] = "application/json";
  }
  if (options.requestPrivateNetwork) {
    headers["access-control-request-private-network"] = "true";
  }
  const response = await fetch(`${baseUrl}${pathname}`, {
    body: options.body === undefined ? undefined : JSON.stringify(options.body),
    headers,
    method: options.method,
  });
  const text = await response.text();
  return {
    body: (text.trim() ? JSON.parse(text) : undefined) as Record<string, any>,
    headers: response.headers,
    status: response.status,
  };
}

async function waitForSession(
  baseUrl: string,
  token: GxserverAuthToken,
  projectId: string,
  sessionId: string,
  predicate: (session: Record<string, any>) => boolean,
): Promise<Record<string, any>> {
  const deadline = Date.now() + 2_000;
  let latest: Record<string, any> | undefined;
  while (Date.now() < deadline) {
    const response = await requestJson(baseUrl, "/api/readProjectStatus", {
      body: {
        params: { projectId },
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      },
      method: "POST",
      token,
    });
    latest = response.body.result.sessions.find((session: Record<string, any>) => session.sessionId === sessionId);
    if (latest && predicate(latest)) {
      return latest;
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error(`Timed out waiting for session ${sessionId}. Latest: ${JSON.stringify(latest)}`);
}

async function requestRawJson(
  baseUrl: string,
  pathname: string,
  options: {
    bodyText: string;
    method: "POST";
    protocolVersion?: number;
    token?: GxserverAuthToken;
  },
): Promise<{ body: Record<string, any>; headers: http.IncomingHttpHeaders; status: number }> {
  const url = new URL(pathname, baseUrl);
  const headers: Record<string, string> = {
    "content-type": "application/json",
    "transfer-encoding": "chunked",
  };
  if (options.token) {
    headers.authorization = `Bearer ${options.token}`;
  }
  if (options.protocolVersion !== undefined) {
    headers[GXSERVER_PROTOCOL_HEADER] = String(options.protocolVersion);
  }

  return await new Promise((resolve, reject) => {
    const request = http.request(
      url,
      {
        headers,
        method: options.method,
      },
      (response) => {
        let text = "";
        response.setEncoding("utf8");
        response.on("data", (chunk) => {
          text += chunk;
        });
        response.on("end", () => {
          resolve({
            body: (text.trim() ? JSON.parse(text) : undefined) as Record<string, any>,
            headers: response.headers,
            status: response.statusCode ?? 0,
          });
        });
      },
    );
    request.on("error", reject);
    const splitAt = Math.floor(options.bodyText.length / 2);
    request.write(options.bodyText.slice(0, splitAt));
    request.end(options.bodyText.slice(splitAt));
  });
}

async function websocketRejected(
  baseUrl: string,
  pathname: string,
  options: { protocolVersion?: number; token?: GxserverAuthToken } = {},
): Promise<{ statusCode: number }> {
  const socket = new WebSocket(toWebSocketUrl(baseUrl, pathname), {
    headers: authHeaders(options.token, options.protocolVersion),
  });
  const [, response] = (await waitFor(
    once(socket, "unexpected-response"),
    "WebSocket rejection",
  )) as [unknown, http.IncomingMessage];
  return { statusCode: response.statusCode ?? 0 };
}

function authHeaders(token?: GxserverAuthToken, protocolVersion?: number): Record<string, string> {
  return {
    ...(token ? { authorization: `Bearer ${token}` } : {}),
    ...(protocolVersion !== undefined ? { [GXSERVER_PROTOCOL_HEADER]: String(protocolVersion) } : {}),
  };
}

function toWebSocketUrl(baseUrl: string, pathname: string): string {
  return `${baseUrl.replace(/^http:/, "ws:")}${pathname}`;
}

async function isFixedLocalPortAvailable(): Promise<boolean> {
  return await isTcpPortAvailable(GXSERVER_LOCAL_API_PORT);
}

async function isTcpPortAvailable(port: number): Promise<boolean> {
  const probe = http.createServer();
  return await new Promise((resolve) => {
    probe.once("error", () => resolve(false));
    probe.listen(port, "127.0.0.1", () => {
      probe.close(() => resolve(true));
    });
  });
}

async function waitForFileText(filePath: string, timeoutMs: number): Promise<string> {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;
  while (Date.now() < deadline) {
    try {
      return await readFile(filePath, "utf8");
    } catch (error) {
      lastError = error;
      await delay(50);
    }
  }
  throw lastError instanceof Error ? lastError : new Error(`Timed out waiting for ${filePath}.`);
}

async function waitForLogEvent(
  filePath: string,
  event: string,
  timeoutMs: number,
): Promise<{ entry: Record<string, any>; text: string }> {
  const deadline = Date.now() + timeoutMs;
  let text = "";
  while (Date.now() < deadline) {
    try {
      text = await readFile(filePath, "utf8");
      const entry = text
        .trim()
        .split("\n")
        .filter(Boolean)
        .map((line) => JSON.parse(line) as Record<string, any>)
        .find((candidate) => candidate.event === event);
      if (entry) {
        return { entry, text };
      }
    } catch {
      // Keep polling until the asynchronous logger append lands.
    }
    await delay(50);
  }
  throw new Error(`Timed out waiting for log event ${event} in ${filePath}. Last text length: ${text.length}.`);
}

async function waitForHealth(
  token: GxserverAuthToken,
  timeoutMs: number,
): Promise<Record<string, any>> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const health = await requestJson(`http://127.0.0.1:${GXSERVER_LOCAL_API_PORT}`, "/api/health/server", {
        method: "GET",
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
        token,
      });
      if (health.status === 200) {
        return health.body;
      }
    } catch {
      // Poll until the foreground process binds the fixed local listener.
    }
    await delay(50);
  }
  throw new Error("Timed out waiting for foreground gxserver health.");
}

async function waitForProcessExit(child: ChildProcessWithoutNullStreams, timeoutMs: number): Promise<void> {
  if (child.exitCode !== null) {
    return;
  }
  let timeout: NodeJS.Timeout | undefined;
  try {
    await Promise.race([
      once(child, "exit"),
      new Promise<never>((_, reject) => {
        timeout = setTimeout(() => reject(new Error("Timed out waiting for gxserver foreground process exit.")), timeoutMs);
      }),
    ]);
  } finally {
    if (timeout) {
      clearTimeout(timeout);
    }
  }
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function writeNativeSidebarSettings(homeDir: string, settings: Record<string, unknown>): Promise<void> {
  const stateDir = path.join(homeDir, ".ghostex", "state");
  await mkdir(stateDir, { recursive: true });
  await writeFile(path.join(stateDir, "native-sidebar-settings.json"), JSON.stringify(settings), "utf8");
}

function runGitForTest(cwd: string, args: readonly string[]): void {
  const result = spawnSync("git", [...args], { cwd, encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(result.stderr.trim() || result.stdout.trim() || `git ${args.join(" ")} failed`);
  }
}

async function onceMessage(socket: WebSocket): Promise<WebSocket.RawData> {
  const [message] = (await waitFor(once(socket, "message"), "WebSocket message")) as [WebSocket.RawData];
  return message;
}

async function collectWebSocketEvents(socket: WebSocket, durationMs: number): Promise<Record<string, unknown>[]> {
  const events: Record<string, unknown>[] = [];
  const onMessage = (message: WebSocket.RawData) => {
    events.push(JSON.parse(String(message)) as Record<string, unknown>);
  };
  socket.on("message", onMessage);
  try {
    await delay(durationMs);
  } finally {
    socket.off("message", onMessage);
  }
  return events;
}

async function nextWebSocketEvent(socket: WebSocket, type: string): Promise<Record<string, unknown>> {
  const seenTypes: string[] = [];
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const event = JSON.parse(String(await onceMessage(socket))) as Record<string, unknown>;
    seenTypes.push(typeof event.type === "string" ? event.type : "<unknown>");
    if (event.type === type) {
      return event;
    }
  }
  throw new Error(`Timed out waiting for WebSocket event ${type}; saw ${seenTypes.join(", ") || "no events"}.`);
}

async function waitFor<T>(promise: Promise<T>, label: string): Promise<T> {
  let timeout: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        timeout = setTimeout(() => reject(new Error(`Timed out waiting for ${label}.`)), 1000);
      }),
    ]);
  } finally {
    if (timeout) {
      clearTimeout(timeout);
    }
  }
}

function closeServer(server: http.Server): Promise<void> {
  return new Promise((resolve, reject) => {
    server.close((error) => {
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
}
