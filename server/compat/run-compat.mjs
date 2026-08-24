#!/usr/bin/env node
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { existsSync, realpathSync } from 'node:fs';
import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/*
CDXC:GxserverRustPort 2026-06-14-20:01:
Phase 0 needs a reusable black-box compatibility harness before the Rust daemon owns API behavior. Keep TypeScript as the fixture source, normalize dynamic runtime fields, and run the same minimal lifecycle, health, protocol-gate, status, WebSocket, and stop checks against TypeScript or a future Rust binary.
*/

const PRODUCT = 'gxserver';
const PROTOCOL_VERSION = 1;
const PROTOCOL_HEADER = 'x-gxserver-protocol-version';
const LOCAL_HOST = '127.0.0.1';
const DEFAULT_LOCAL_PORT = 58744;
const DEV_PORT_ENV = 'GHOSTEX_GXSERVER_DEV_PORT';
const JSON_BODY_LIMIT_BYTES = 1024 * 1024;
const GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES = 256 * 1024;
const GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES = 512 * 1024;
const COMPAT_USER = 'gxserver-compat';
const COMPAT_SAFE_SYSTEM_PATHS = ['/usr/bin', '/bin', '/usr/sbin', '/sbin'];
const CURRENT_MIGRATION_VERSION = 11;
const EXPECTED_MIGRATIONS = [
  '0001_foundation',
  '0002_domain_state',
  '0003_session_sidebar_order',
  '0004_previous_session_history_quality',
  '0005_session_tags',
  '0006_expand_session_tags',
  '0007_expand_session_tags_in_progress_and_type',
  '0008_remove_retired_session_type_tags',
  '0009_remove_legacy_zmux_chat_projects',
  '0010_portless_persistence_model',
  '0011_session_kind_constraint',
];
const EXPECTED_CAPABILITIES = ['health', 'events', 'localFullApi', 'remoteLimitedApi', 'strictProtocolVersion'];

const compatDir = path.dirname(fileURLToPath(import.meta.url));
const gxserverRsRoot = path.resolve(compatDir, '..');
const repoRoot = path.resolve(gxserverRsRoot, '..');
const fixturesDir = path.join(compatDir, 'fixtures');

const options = parseArgs(process.argv.slice(2));

if (options.help) {
  printUsage();
  process.exit(0);
}

await main(options);

async function main(runOptions) {
  if (!['phase0', 'phase3', 'phase4', 'phase5', 'phase6', 'phase7'].includes(runOptions.suite)) {
    throw new Error(`Unsupported suite: ${runOptions.suite}`);
  }

  const target = resolveTarget(runOptions);
  if (!(await isTcpPortAvailable(runOptions.port))) {
    if (runOptions.skipIfPortBusy) {
      console.log(`SKIP gxserver-rs compat ${runOptions.suite}: ${LOCAL_HOST}:${runOptions.port} is already in use.`);
      return;
    }
    throw new Error(
      `${LOCAL_HOST}:${runOptions.port} is already in use; stop the current gxserver on the selected port before running the compatibility harness.`
    );
  }

  const homeDir = await mkdtemp(path.join(tmpdir(), 'gxserver-rs-phase0-home-'));
  const paths = getGxserverPaths(homeDir);
  const childOutput = { stderr: '', stdout: '' };
  let child;
  let stoppedByControlEndpoint = false;
  let targetEnv;
  let version;

  const observations = {
    schemaVersion: 1,
    suite: runOptions.suite,
    tests: [],
  };

  try {
    await prepareCompatSandbox(homeDir, runOptions);
    targetEnv = createTargetEnv(homeDir, runOptions);
    assertCompatTargetEnv(targetEnv, homeDir);
    version = await readTargetVersion(target, runOptions.timeoutMs, targetEnv);

    child = spawn(target.command, target.foregroundArgs, {
      cwd: target.cwd,
      env: targetEnv,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    collectChildOutput(child, childOutput);

    const token = (await waitForFileText(paths.authTokenFile, runOptions.timeoutMs, child, childOutput)).trim();
    assert.match(token, /^[A-Za-z0-9_-]{32,}$/u);
    await waitForServerReady(token, runOptions.timeoutMs, child, childOutput);

    const minimalHealth = await requestJson('/api/health', { method: 'GET' });
    assert.equal(minimalHealth.status, 200);
    assert.deepEqual(minimalHealth.body, {
      ok: true,
      product: PRODUCT,
      protocolVersion: PROTOCOL_VERSION,
      version,
    });
    recordObservation(
      observations,
      'minimalHealth',
      normalizeExchange(
        {
          request: { method: 'GET', path: '/api/health' },
          response: minimalHealth,
        },
        homeDir
      )
    );

    const unauthorizedRpc = await requestJson('/api/listSessions', { method: 'POST' });
    assert.equal(unauthorizedRpc.status, 401);
    assertErrorEnvelope(unauthorizedRpc.body, 'unauthorized');
    recordObservation(
      observations,
      'unauthorizedRpc',
      normalizeExchange(
        {
          request: { method: 'POST', path: '/api/listSessions' },
          response: unauthorizedRpc,
        },
        homeDir
      )
    );

    const methodGate = await requestJson('/api/listSessions', {
      method: 'GET',
      protocolVersion: PROTOCOL_VERSION,
      token,
    });
    assert.equal(methodGate.status, 405);
    assertErrorEnvelope(methodGate.body, 'methodNotAllowed');
    recordObservation(
      observations,
      'methodGate',
      normalizeExchange(
        {
          request: { method: 'GET', path: '/api/listSessions', protocolVersion: PROTOCOL_VERSION },
          response: methodGate,
        },
        homeDir
      )
    );

    const missingProtocol = await requestJson('/api/listSessions', { method: 'POST', token });
    assert.equal(missingProtocol.status, 426);
    assertErrorEnvelope(missingProtocol.body, 'protocolMismatch');
    assert.match(missingProtocol.body.message, /Update Ghostex and gxserver/u);
    recordObservation(
      observations,
      'missingProtocol',
      normalizeExchange(
        {
          request: { method: 'POST', path: '/api/listSessions', token: '<bearer>' },
          response: missingProtocol,
        },
        homeDir
      )
    );

    const wrongProtocol = await requestJson('/api/listSessions', {
      method: 'POST',
      protocolVersion: 999,
      token,
    });
    assert.equal(wrongProtocol.status, 426);
    assertErrorEnvelope(wrongProtocol.body, 'protocolMismatch');
    recordObservation(
      observations,
      'wrongProtocol',
      normalizeExchange(
        {
          request: { method: 'POST', path: '/api/listSessions', protocolVersion: 999, token: '<bearer>' },
          response: wrongProtocol,
        },
        homeDir
      )
    );

    const bodyProtocolRpc = await requestJson('/api/listSessions', {
      body: { params: {}, protocolVersion: PROTOCOL_VERSION },
      method: 'POST',
      token,
    });
    assert.equal(bodyProtocolRpc.status, 200);
    assertSuccessEnvelope(bodyProtocolRpc.body);
    assert.deepEqual(bodyProtocolRpc.body.result.sessions, []);
    recordObservation(
      observations,
      'bodyProtocolRpc',
      normalizeExchange(
        {
          request: {
            body: { params: {}, protocolVersion: PROTOCOL_VERSION },
            method: 'POST',
            path: '/api/listSessions',
            token: '<bearer>',
          },
          response: bodyProtocolRpc,
        },
        homeDir
      )
    );

    const oversizedBody = {
      params: { padding: 'x'.repeat(JSON_BODY_LIMIT_BYTES) },
      protocolVersion: PROTOCOL_VERSION,
    };
    const oversizedRpc = await requestJson('/api/listSessions', {
      body: oversizedBody,
      method: 'POST',
      protocolVersion: PROTOCOL_VERSION,
      token,
    });
    assert.equal(oversizedRpc.status, 413);
    assertErrorEnvelope(oversizedRpc.body, 'badRequest');
    assert.match(oversizedRpc.body.message, /JSON RPC limit/u);
    recordObservation(
      observations,
      'jsonBodyLimit',
      normalizeExchange(
        {
          request: {
            body: { params: { padding: `<${JSON_BODY_LIMIT_BYTES} chars>` }, protocolVersion: PROTOCOL_VERSION },
            method: 'POST',
            path: '/api/listSessions',
            protocolVersion: PROTOCOL_VERSION,
            token: '<bearer>',
          },
          response: oversizedRpc,
        },
        homeDir
      )
    );

    const authenticatedHealth = await requestJson('/api/health/server', {
      method: 'GET',
      protocolVersion: PROTOCOL_VERSION,
      token,
    });
    assert.equal(authenticatedHealth.status, 200);
    assertAuthenticatedHealth(authenticatedHealth.body, version);
    await assertRuntimeFiles(paths, token);
    recordObservation(
      observations,
      'authenticatedHealth',
      normalizeExchange(
        {
          request: { method: 'GET', path: '/api/health/server', protocolVersion: PROTOCOL_VERSION, token: '<bearer>' },
          response: authenticatedHealth,
        },
        homeDir
      )
    );

    const eventStreamReady = await readEventStreamReady(token);
    assert.equal(eventStreamReady.rawEndsWithNewline, true);
    assert.equal(eventStreamReady.body.type, 'eventStreamReady');
    assert.equal(eventStreamReady.body.protocolVersion, PROTOCOL_VERSION);
    assert.equal(eventStreamReady.body.serverId, authenticatedHealth.body.serverId);
    recordObservation(observations, 'eventStreamReady', normalizeValue(eventStreamReady, homeDir));

    const cliStatus = await runTargetCommand(target, target.statusArgs, targetEnv, runOptions.timeoutMs);
    assert.equal(cliStatus.exitCode, 0, cliStatus.stderr);
    const statusBody = JSON.parse(cliStatus.stdout);
    assert.equal(statusBody.ok, true);
    assert.equal(statusBody.product, PRODUCT);
    assert.equal(statusBody.state, 'running');
    assert.equal(statusBody.health.port, runOptions.port);
    recordObservation(
      observations,
      'cliStatusRunning',
      normalizeValue(
        {
          exitCode: cliStatus.exitCode,
          stdoutJson: statusBody,
        },
        homeDir
      )
    );

    if (runOptions.suite === 'phase3') {
      await runPhase3DomainChecks({ homeDir, observations, token });
    }
    if (runOptions.suite === 'phase4') {
      await runPhase4EventChecks({ homeDir, observations, token });
    }
    if (runOptions.suite === 'phase5') {
      await runPhase5ZmxChecks({ homeDir, observations, token });
    }
    if (runOptions.suite === 'phase6') {
      await runPhase6AgentChecks({ homeDir, observations, paths, token });
    }
    if (runOptions.suite === 'phase7') {
      await runPhase7TypedOperationChecks({ homeDir, observations, paths, runOptions, token });
    }

    const controlStop = await requestJson('/api/control/stop', {
      body: { protocolVersion: PROTOCOL_VERSION },
      method: 'POST',
      token,
    });
    assert.equal(controlStop.status, 200);
    assertSuccessEnvelope(controlStop.body);
    assert.deepEqual(controlStop.body.result, {});
    recordObservation(
      observations,
      'controlStop',
      normalizeExchange(
        {
          request: {
            body: { protocolVersion: PROTOCOL_VERSION },
            method: 'POST',
            path: '/api/control/stop',
            token: '<bearer>',
          },
          response: controlStop,
        },
        homeDir
      )
    );

    stoppedByControlEndpoint = true;
    await waitForProcessExit(child, runOptions.timeoutMs, childOutput);
    await assertRuntimeMetadataRemoved(paths);

    const stoppedStatus = await runTargetCommand(target, target.statusArgs, targetEnv, runOptions.timeoutMs);
    assert.equal(stoppedStatus.exitCode, 0, stoppedStatus.stderr);
    const stoppedStatusBody = JSON.parse(stoppedStatus.stdout);
    assert.equal(stoppedStatusBody.ok, true);
    assert.equal(stoppedStatusBody.product, PRODUCT);
    assert.notEqual(stoppedStatusBody.state, 'running');
    recordObservation(
      observations,
      'cliStatusStopped',
      normalizeValue(
        {
          exitCode: stoppedStatus.exitCode,
          stdoutJson: stoppedStatusBody,
        },
        homeDir
      )
    );

    await assertCompatSandboxContained(homeDir, runOptions);
    await updateOrCompareFixture(runOptions, observations);
    console.log(`gxserver-rs compat ${runOptions.suite} passed for ${target.name}.`);
  } finally {
    if (child && child.exitCode === null) {
      if (!stoppedByControlEndpoint) {
        child.kill('SIGTERM');
      }
      await waitForProcessExit(child, 2_000, childOutput).catch(async () => {
        child.kill('SIGKILL');
        await waitForProcessExit(child, 2_000, childOutput).catch(() => undefined);
      });
    }
    if (runOptions.keepHome) {
      console.log(`Kept isolated HOME at ${homeDir}`);
    } else {
      await rm(homeDir, { force: true, recursive: true });
    }
  }
}

/*
CDXC:GxserverRustPort 2026-06-14-22:52:
Phase 3 compatibility must exercise durable project/session state and read-only presentation inventory through public RPC endpoints, not Rust internals. Keep the fixture metadata-only and path-normalized so it can compare TypeScript and Rust without leaking user workspace names or terminal content.
*/
async function runPhase3DomainChecks({ homeDir, observations, token }) {
  const workspaceDir = path.join(homeDir, 'workspace');
  const projectDir = path.join(workspaceDir, 'phase3-project');
  const addedProjectDir = path.join(workspaceDir, 'added-project');
  await mkdir(projectDir, { recursive: true });
  await mkdir(addedProjectDir, { recursive: true });

  const createProject = await requestJson('/api/createProject', {
    body: {
      params: {
        identityIcon: { color: 'blue', kind: 'emoji', value: 'G' },
        isPinned: true,
        name: 'Phase 3 Compat',
        path: projectDir,
        runtimeSettings: { defaultSurface: 'workspace' },
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(createProject.status, 200);
  assertSuccessEnvelope(createProject.body);
  const project = createProject.body.result.project;
  assert.match(project.projectId, /^P\d[a-z0-9]{3}$/u);
  assert.equal(project.name, 'Phase 3 Compat');
  assert.equal(project.path, projectDir);
  recordObservation(
    observations,
    'phase3CreateProject',
    normalizeExchange(
      {
        request: {
          body: {
            params: {
              identityIcon: { color: 'blue', kind: 'emoji', value: 'G' },
              isPinned: true,
              name: 'Phase 3 Compat',
              path: projectDir,
              runtimeSettings: { defaultSurface: 'workspace' },
            },
            protocolVersion: PROTOCOL_VERSION,
          },
          method: 'POST',
          path: '/api/createProject',
          token: '<bearer>',
        },
        response: createProject,
      },
      homeDir
    )
  );

  const updateProject = await requestJson('/api/updateProject', {
    body: {
      params: {
        customAgentOrder: ['codex'],
        customAgents: [{ id: 'codex', name: 'Codex' }],
        isFavorite: true,
        name: 'Phase 3 Compat Updated',
        projectId: project.projectId,
        worktree: { branch: 'main', rootPath: projectDir },
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(updateProject.status, 200);
  assertSuccessEnvelope(updateProject.body);
  assert.equal(updateProject.body.result.project.name, 'Phase 3 Compat Updated');
  assert.equal(updateProject.body.result.project.isFavorite, true);
  recordObservation(
    observations,
    'phase3UpdateProject',
    normalizeExchange(
      {
        request: {
          body: {
            params: {
              customAgentOrder: ['codex'],
              customAgents: [{ id: 'codex', name: 'Codex' }],
              isFavorite: true,
              name: 'Phase 3 Compat Updated',
              projectId: project.projectId,
              worktree: { branch: 'main', rootPath: projectDir },
            },
            protocolVersion: PROTOCOL_VERSION,
          },
          method: 'POST',
          path: '/api/updateProject',
          token: '<bearer>',
        },
        response: updateProject,
      },
      homeDir
    )
  );

  const addProjectPath = await requestJson('/api/addProjectPath', {
    body: {
      params: { path: addedProjectDir },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(addProjectPath.status, 200);
  assertSuccessEnvelope(addProjectPath.body);
  const addedProject = addProjectPath.body.result.project;
  assert.match(addedProject.projectId, /^P\d[a-z0-9]{3}$/u);
  assert.equal(addedProject.path, addedProjectDir);
  recordObservation(
    observations,
    'phase3AddProjectPath',
    normalizeExchange(
      {
        request: {
          body: { params: { path: addedProjectDir }, protocolVersion: PROTOCOL_VERSION },
          method: 'POST',
          path: '/api/addProjectPath',
          token: '<bearer>',
        },
        response: addProjectPath,
      },
      homeDir
    )
  );

  const terminalSession = await requestJson('/api/createSession', {
    body: {
      params: {
        cwd: projectDir,
        kind: 'terminal',
        launchSettings: { surface: 'workspace' },
        lifecycleState: 'running',
        projectId: project.projectId,
        providerState: { lifecycleState: 'exists', provider: 'zmx' },
        runtimeSettings: { terminalTitle: 'Shell Title' },
        sessionTag: 'research',
        sidebarOrder: 2000,
        title: 'Terminal One',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(terminalSession.status, 200);
  assertSuccessEnvelope(terminalSession.body);
  const terminal = terminalSession.body.result.session;
  assert.match(terminal.sessionId, /^G\d[a-z0-9]{3}$/u);
  assert.equal(terminal.projectId, project.projectId);
  assert.equal(terminal.title, 'Terminal One');
  recordObservation(
    observations,
    'phase3CreateSession',
    normalizeExchange(
      {
        request: {
          body: {
            params: {
              cwd: projectDir,
              kind: 'terminal',
              launchSettings: { surface: 'workspace' },
              lifecycleState: 'running',
              projectId: project.projectId,
              providerState: { lifecycleState: 'exists', provider: 'zmx' },
              runtimeSettings: { terminalTitle: 'Shell Title' },
              sessionTag: 'research',
              sidebarOrder: 2000,
              title: 'Terminal One',
            },
            protocolVersion: PROTOCOL_VERSION,
          },
          method: 'POST',
          path: '/api/createSession',
          token: '<bearer>',
        },
        response: terminalSession,
      },
      homeDir
    )
  );

  const agentSession = await requestJson('/api/createAgentSession', {
    body: {
      params: {
        agentId: 'codex',
        cwd: projectDir,
        launchSettings: { surface: 'workspace' },
        projectId: project.projectId,
        runtimeSettings: {
          agentName: 'Codex',
          agentSessionId: 'agent-session-1',
          firstUserMessage: 'Summarize the project.',
        },
        sidebarOrder: 1000,
        title: 'Codex Agent',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(agentSession.status, 200);
  assertSuccessEnvelope(agentSession.body);
  const agent = agentSession.body.result.session;
  assert.match(agent.sessionId, /^G\d[a-z0-9]{3}$/u);
  assert.equal(agent.kind, 'agent');
  assert.equal(agent.agentId, 'codex');
  recordObservation(
    observations,
    'phase3CreateAgentSession',
    normalizeExchange(
      {
        request: {
          body: {
            params: {
              agentId: 'codex',
              cwd: projectDir,
              launchSettings: { surface: 'workspace' },
              projectId: project.projectId,
              runtimeSettings: {
                agentName: 'Codex',
                agentSessionId: 'agent-session-1',
                firstUserMessage: 'Summarize the project.',
              },
              sidebarOrder: 1000,
              title: 'Codex Agent',
            },
            protocolVersion: PROTOCOL_VERSION,
          },
          method: 'POST',
          path: '/api/createAgentSession',
          token: '<bearer>',
        },
        response: agentSession,
      },
      homeDir
    )
  );

  const updateSession = await requestJson('/api/updateSession', {
    body: {
      params: {
        isPinned: true,
        lifecycleState: 'sleeping',
        projectId: project.projectId,
        runtimeSettings: { terminalTitle: 'Shell Title Updated' },
        sessionId: terminal.sessionId,
        title: 'Terminal One Updated',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(updateSession.status, 200);
  assertSuccessEnvelope(updateSession.body);
  assert.equal(updateSession.body.result.session.title, 'Terminal One Updated');
  recordObservation(
    observations,
    'phase3UpdateSession',
    normalizeExchange(
      {
        request: {
          body: {
            params: {
              isPinned: true,
              lifecycleState: 'sleeping',
              projectId: project.projectId,
              runtimeSettings: { terminalTitle: 'Shell Title Updated' },
              sessionId: terminal.sessionId,
              title: 'Terminal One Updated',
            },
            protocolVersion: PROTOCOL_VERSION,
          },
          method: 'POST',
          path: '/api/updateSession',
          token: '<bearer>',
        },
        response: updateSession,
      },
      homeDir
    )
  );

  const updateOrder = await requestJson('/api/updateSessionOrder', {
    body: {
      params: {
        projectId: project.projectId,
        sessionIds: [agent.sessionId, terminal.sessionId],
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(updateOrder.status, 200);
  assertSuccessEnvelope(updateOrder.body);
  assert.equal(updateOrder.body.result.sessions.length, 2);
  recordObservation(
    observations,
    'phase3UpdateSessionOrder',
    normalizeExchange(
      {
        request: {
          body: {
            params: {
              projectId: project.projectId,
              sessionIds: [agent.sessionId, terminal.sessionId],
            },
            protocolVersion: PROTOCOL_VERSION,
          },
          method: 'POST',
          path: '/api/updateSessionOrder',
          token: '<bearer>',
        },
        response: updateOrder,
      },
      homeDir
    )
  );

  const listProjects = await requestJson('/api/listProjects', {
    body: { params: {}, protocolVersion: PROTOCOL_VERSION },
    method: 'POST',
    token,
  });
  assert.equal(listProjects.status, 200);
  assertSuccessEnvelope(listProjects.body);
  assert.equal(listProjects.body.result.projects.length, 2);
  recordObservation(
    observations,
    'phase3ListProjects',
    normalizeExchange(
      {
        request: {
          body: { params: {}, protocolVersion: PROTOCOL_VERSION },
          method: 'POST',
          path: '/api/listProjects',
          token: '<bearer>',
        },
        response: listProjects,
      },
      homeDir
    )
  );

  const listSessions = await requestJson('/api/listSessions', {
    body: { params: { projectId: project.projectId }, protocolVersion: PROTOCOL_VERSION },
    method: 'POST',
    token,
  });
  assert.equal(listSessions.status, 200);
  assertSuccessEnvelope(listSessions.body);
  assert.equal(listSessions.body.result.sessions.length, 2);
  recordObservation(
    observations,
    'phase3ListSessions',
    normalizeExchange(
      {
        request: {
          body: { params: { projectId: project.projectId }, protocolVersion: PROTOCOL_VERSION },
          method: 'POST',
          path: '/api/listSessions',
          token: '<bearer>',
        },
        response: listSessions,
      },
      homeDir
    )
  );

  const projectStatus = await requestJson('/api/readProjectStatus', {
    body: { params: { projectId: project.projectId }, protocolVersion: PROTOCOL_VERSION },
    method: 'POST',
    token,
  });
  assert.equal(projectStatus.status, 200);
  assertSuccessEnvelope(projectStatus.body);
  assert.equal(projectStatus.body.result.sessions.length, 2);
  recordObservation(
    observations,
    'phase3ReadProjectStatus',
    normalizeExchange(
      {
        request: {
          body: { params: { projectId: project.projectId }, protocolVersion: PROTOCOL_VERSION },
          method: 'POST',
          path: '/api/readProjectStatus',
          token: '<bearer>',
        },
        response: projectStatus,
      },
      homeDir
    )
  );

  const snapshot = await requestJson('/api/readPresentationSnapshot', {
    body: { params: {}, protocolVersion: PROTOCOL_VERSION },
    method: 'POST',
    token,
  });
  assert.equal(snapshot.status, 200);
  assertSuccessEnvelope(snapshot.body);
  assert.equal(snapshot.body.result.snapshot.projects.length, 2);
  assert.equal(snapshot.body.result.snapshot.sessions.length, 2);
  recordObservation(
    observations,
    'phase3ReadPresentationSnapshot',
    normalizeExchange(
      {
        request: {
          body: { params: {}, protocolVersion: PROTOCOL_VERSION },
          method: 'POST',
          path: '/api/readPresentationSnapshot',
          token: '<bearer>',
        },
        response: snapshot,
      },
      homeDir
    )
  );

  const search = await requestJson('/api/searchSessions', {
    body: {
      params: { limit: 10, projectId: project.projectId, query: 'Terminal One Updated' },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(search.status, 200);
  assertSuccessEnvelope(search.body);
  assert.equal(search.body.result.results.length, 1);
  recordObservation(
    observations,
    'phase3SearchSessions',
    normalizeExchange(
      {
        request: {
          body: {
            params: { limit: 10, projectId: project.projectId, query: 'Terminal One Updated' },
            protocolVersion: PROTOCOL_VERSION,
          },
          method: 'POST',
          path: '/api/searchSessions',
          token: '<bearer>',
        },
        response: search,
      },
      homeDir
    )
  );

  const removeSession = await requestJson('/api/removeSession', {
    body: {
      params: { projectId: project.projectId, sessionId: terminal.sessionId },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(removeSession.status, 200);
  assertSuccessEnvelope(removeSession.body);
  recordObservation(
    observations,
    'phase3RemoveSession',
    normalizeExchange(
      {
        request: {
          body: {
            params: { projectId: project.projectId, sessionId: terminal.sessionId },
            protocolVersion: PROTOCOL_VERSION,
          },
          method: 'POST',
          path: '/api/removeSession',
          token: '<bearer>',
        },
        response: removeSession,
      },
      homeDir
    )
  );

  const removeProject = await requestJson('/api/removeProject', {
    body: {
      params: { projectId: addedProject.projectId },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(removeProject.status, 200);
  assertSuccessEnvelope(removeProject.body);
  recordObservation(
    observations,
    'phase3RemoveProject',
    normalizeExchange(
      {
        request: {
          body: { params: { projectId: addedProject.projectId }, protocolVersion: PROTOCOL_VERSION },
          method: 'POST',
          path: '/api/removeProject',
          token: '<bearer>',
        },
        response: removeProject,
      },
      homeDir
    )
  );
}

/*
CDXC:GxserverRustPort 2026-06-15-09:55:
Phase 4 compatibility exercises the event hub through the public WebSocket and HTTP contracts. Keep the suite on the explicit dev port so packaged Ghostex can keep 58744, and compare only metadata-safe presentation and renderer-command envelopes.
*/
async function runPhase4EventChecks({ homeDir, observations, token }) {
  const workspaceDir = path.join(homeDir, 'workspace');
  const projectDir = path.join(workspaceDir, 'phase4-project');
  await mkdir(projectDir, { recursive: true });

  const socket = await openEventSocket(token);
  try {
    const snapshotPromise = nextWebSocketEvent(socket, 'presentationSnapshot');
    socket.send(
      JSON.stringify({
        clientId: 'phase4-client',
        lastRevision: 0,
        type: 'subscribePresentation',
      })
    );
    const snapshot = await snapshotPromise;
    assert.equal(snapshot.clientId, 'phase4-client');
    assert.equal(snapshot.revision, snapshot.snapshot.revision);
    assert.deepEqual(snapshot.snapshot.projects, []);
    recordObservation(observations, 'phase4PresentationSnapshot', normalizeValue(snapshot, homeDir));

    const handledPromise = nextWebSocketEvent(socket, 'apiRequestHandled');
    const listSessions = await requestJson('/api/listSessions', {
      body: { params: {}, protocolVersion: PROTOCOL_VERSION },
      method: 'POST',
      token,
    });
    assert.equal(listSessions.status, 200);
    const handled = await handledPromise;
    assert.equal(handled.path, '/api/listSessions');
    recordObservation(observations, 'phase4ApiRequestHandled', normalizeValue(handled, homeDir));

    const rendererUnavailable = await requestJson('/api/dispatchRendererCommand', {
      body: {
        params: {
          action: 'toggleSidebarCollapsed',
          payload: {},
        },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(rendererUnavailable.status, 503);
    assertErrorEnvelope(rendererUnavailable.body, 'dependencyUnavailable');
    recordObservation(
      observations,
      'phase4RendererUnavailable',
      normalizeExchange(
        {
          request: {
            body: {
              params: {
                action: 'toggleSidebarCollapsed',
                payload: {},
              },
              protocolVersion: PROTOCOL_VERSION,
            },
            method: 'POST',
            path: '/api/dispatchRendererCommand',
            token: '<bearer>',
          },
          response: rendererUnavailable,
        },
        homeDir
      )
    );

    const rendererSnapshotPromise = nextWebSocketEvent(socket, 'presentationSnapshot');
    socket.send(
      JSON.stringify({
        clientId: 'phase4-renderer',
        rendererCommands: true,
        type: 'subscribePresentation',
      })
    );
    await rendererSnapshotPromise;

    const commandPromise = nextWebSocketEvent(socket, 'rendererCommand');
    const rendererResponsePromise = requestJson('/api/dispatchRendererCommand', {
      body: {
        params: {
          action: 'toggleSidebarCollapsed',
          payload: { source: 'phase4' },
          timeoutMs: 1000,
        },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    const commandEvent = await commandPromise;
    assert.equal(commandEvent.command.action, 'toggleSidebarCollapsed');
    assert.deepEqual(commandEvent.command.payload, { source: 'phase4' });
    assert.equal(typeof commandEvent.command.commandId, 'string');
    socket.send(
      JSON.stringify({
        commandId: commandEvent.command.commandId,
        ok: true,
        result: { ok: true, toggled: true },
        type: 'rendererCommandResult',
      })
    );
    const rendererResponse = await rendererResponsePromise;
    assert.equal(rendererResponse.status, 200);
    assert.deepEqual(rendererResponse.body.result, { ok: true, toggled: true });
    recordObservation(
      observations,
      'phase4RendererCommand',
      normalizeValue(
        {
          event: commandEvent,
          response: rendererResponse,
        },
        homeDir
      )
    );

    /*
    CDXC:GxserverPresentationEvents 2026-06-22-04:30:
    Renderer command subscriptions are ordered like TypeScript's WebSocket set. A later renderer-capable subscription must not steal commands from the first open renderer client, otherwise native command ownership can flip when secondary clients subscribe.
    */
    const secondRendererSocket = await openEventSocket(token);
    try {
      const secondRendererSnapshotPromise = nextWebSocketEvent(secondRendererSocket, 'presentationSnapshot');
      secondRendererSocket.send(
        JSON.stringify({
          clientId: 'phase4-renderer-later',
          rendererCommands: true,
          type: 'subscribePresentation',
        })
      );
      await secondRendererSnapshotPromise;

      const firstRendererCommandPromise = observeWebSocketEvent(socket, 'rendererCommand', 500);
      const laterRendererCommandPromise = observeWebSocketEvent(secondRendererSocket, 'rendererCommand', 500);
      const orderedRendererResponsePromise = requestJson('/api/dispatchRendererCommand', {
        body: {
          params: {
            action: 'toggleSidebarCollapsed',
            payload: { source: 'phase4-ordered-renderer' },
            timeoutMs: 3000,
          },
          protocolVersion: PROTOCOL_VERSION,
        },
        method: 'POST',
        token,
      });
      const firstRendererCommand = await firstRendererCommandPromise;
      if (firstRendererCommand.event) {
        socket.send(
          JSON.stringify({
            commandId: firstRendererCommand.event.command.commandId,
            ok: true,
            result: { ok: true, ordered: true },
            type: 'rendererCommandResult',
          })
        );
      }
      const laterRendererCommand = await laterRendererCommandPromise;
      if (!firstRendererCommand.event && laterRendererCommand.event) {
        secondRendererSocket.send(
          JSON.stringify({
            commandId: laterRendererCommand.event.command.commandId,
            ok: true,
            result: { ok: true, ordered: true },
            type: 'rendererCommandResult',
          })
        );
      }
      const orderedRendererResponse = await orderedRendererResponsePromise;
      assert.ok(
        firstRendererCommand.event,
        firstRendererCommand.error?.message ?? 'First renderer subscriber did not receive the command.'
      );
      assert.ok(
        !laterRendererCommand.event,
        'Later renderer subscriber unexpectedly received the command before the first subscriber.'
      );
      assert.equal(orderedRendererResponse.status, 200);
      assert.deepEqual(orderedRendererResponse.body.result, { ok: true, ordered: true });
    } finally {
      secondRendererSocket.close();
    }

    const projectAddedPromise = nextWebSocketEvent(socket, 'presentationDelta');
    const createProject = await requestJson('/api/createProject', {
      body: {
        params: {
          name: 'Phase 4 Events',
          path: projectDir,
        },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(createProject.status, 200);
    assertSuccessEnvelope(createProject.body);
    const project = createProject.body.result.project;
    const projectAdded = await projectAddedPromise;
    assert.equal(projectAdded.delta.type, 'projectAdded');
    assert.equal(projectAdded.delta.project.projectId, project.projectId);
    assert.equal(projectAdded.delta.domainProject.projectId, project.projectId);
    recordObservation(observations, 'phase4ProjectAddedDelta', normalizeValue(projectAdded, homeDir));

    const sessionChangedPromise = nextWebSocketEvent(socket, 'presentationDelta');
    const createSession = await requestJson('/api/createSession', {
      body: {
        params: {
          kind: 'terminal',
          lifecycleState: 'running',
          projectId: project.projectId,
          providerState: { lifecycleState: 'exists', provider: 'zmx' },
          title: 'Phase 4 Session',
        },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(createSession.status, 200);
    const session = createSession.body.result.session;
    const sessionChanged = await sessionChangedPromise;
    assert.equal(sessionChanged.delta.type, 'sessionPresentationChanged');
    assert.equal(sessionChanged.delta.session.sessionId, session.sessionId);
    recordObservation(observations, 'phase4SessionChangedDelta', normalizeValue(sessionChanged, homeDir));

    const projectRemovedPromise = nextWebSocketEvent(socket, 'presentationDelta');
    const removeProject = await requestJson('/api/removeProject', {
      body: {
        params: { projectId: project.projectId },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(removeProject.status, 200);
    const projectRemoved = await projectRemovedPromise;
    assert.deepEqual(projectRemoved.delta, {
      projectId: project.projectId,
      type: 'projectRemoved',
    });
    recordObservation(observations, 'phase4ProjectRemovedDelta', normalizeValue(projectRemoved, homeDir));
  } finally {
    socket.close();
  }

  const revisionProjectDir = path.join(workspaceDir, 'phase4-revision-project');
  await mkdir(revisionProjectDir, { recursive: true });
  const revisionProject = await requestJson('/api/createProject', {
    body: {
      params: { name: 'Phase 4 Revision', path: revisionProjectDir },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(revisionProject.status, 200);
  const revisionProjectId = revisionProject.body.result.project.projectId;
  const snapshotBefore = await requestJson('/api/readPresentationSnapshot', {
    body: { params: {}, protocolVersion: PROTOCOL_VERSION },
    method: 'POST',
    token,
  });
  assert.equal(snapshotBefore.status, 200);
  const revisionBefore = Number(snapshotBefore.body.result.snapshot.revision);
  const revisionSession = await requestJson('/api/createSession', {
    body: {
      params: {
        lifecycleState: 'running',
        projectId: revisionProjectId,
        providerState: { lifecycleState: 'exists', provider: 'zmx' },
        title: 'Revision Without Clients',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(revisionSession.status, 200);
  const snapshotAfter = await requestJson('/api/readPresentationSnapshot', {
    body: { params: {}, protocolVersion: PROTOCOL_VERSION },
    method: 'POST',
    token,
  });
  assert.equal(snapshotAfter.status, 200);
  const revisionAfter = Number(snapshotAfter.body.result.snapshot.revision);
  assert.ok(revisionAfter > revisionBefore);
  recordObservation(
    observations,
    'phase4RevisionWithoutClients',
    normalizeValue(
      {
        revisionAdvanced: true,
      },
      homeDir
    )
  );
}

/*
CDXC:GxserverRustPort 2026-06-15-18:06:
Phase 5 compatibility must exercise real public lifecycle and session-I/O endpoints on the explicit dev port without stopping the packaged daemon on 58744. Keep observations metadata-only and normalize repo/home paths because zmx command strings include Ghostex-managed artifact paths and per-run auth-token locations.
*/
async function runPhase5ZmxChecks({ homeDir, observations, token }) {
  const workspaceDir = path.join(homeDir, 'workspace');
  const projectDir = path.join(workspaceDir, 'phase5-project');
  await mkdir(projectDir, { recursive: true });
  const liveSessions = [];
  const rememberLive = (projectId, sessionId) => {
    liveSessions.push({ projectId, sessionId });
  };
  const forgetLive = (projectId, sessionId) => {
    const index = liveSessions.findIndex(
      (session) => session.projectId === projectId && session.sessionId === sessionId
    );
    if (index >= 0) {
      liveSessions.splice(index, 1);
    }
  };

  const createProject = await requestJson('/api/createProject', {
    body: {
      params: { name: 'Phase 5 Zmx', path: projectDir },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(createProject.status, 200);
  const project = createProject.body.result.project;

  const createSession = await requestJson('/api/createSession', {
    body: {
      params: {
        lifecycleState: 'unknown',
        projectId: project.projectId,
        title: 'Phase 5 Terminal',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(createSession.status, 200);
  const session = createSession.body.result.session;

  try {
    const attach = await requestJson('/api/attachSessionMetadata', {
      body: {
        params: {
          projectId: project.projectId,
          promptEditor: 'monaco',
          sessionId: session.sessionId,
        },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(attach.status, 200);
    assertSuccessEnvelope(attach.body);
    assert.equal(attach.body.result.attach.provider, 'zmx');
    assert.match(
      attach.body.result.attach.attachCommand,
      new RegExp(`^${escapeRegExp(resolveCompatShell())} -l?c `, 'u')
    );
    assert.match(attach.body.result.attach.attachCommand, /--prompt-editor=monaco/u);
    recordObservation(
      observations,
      'phase5AttachMetadata',
      normalizeValue(
        {
          attachCommand: attach.body.result.attach.attachCommand,
          persistenceSessionCreated: attach.body.result.attach.persistenceSessionCreated,
          providerState: attach.body.result.attach.providerState.lifecycleState,
          startupTextDisposition: attach.body.result.attach.startupTextDisposition,
          zmxName: attach.body.result.attach.zmxName,
        },
        homeDir
      )
    );

    const start = await requestJson('/api/startSessionProvider', {
      body: {
        params: { projectId: project.projectId, sessionId: session.sessionId },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(start.status, 200);
    assertSuccessEnvelope(start.body);
    assert.equal(start.body.result.started, true);
    assert.equal(start.body.result.providerState.lifecycleState, 'exists');
    assert.equal(start.body.result.session.lifecycleState, 'running');
    rememberLive(project.projectId, session.sessionId);
    recordObservation(
      observations,
      'phase5StartPlainProvider',
      normalizeValue(
        {
          providerState: start.body.result.providerState.lifecycleState,
          started: start.body.result.started,
          startupTextDisposition: start.body.result.startupTextDisposition,
          zmxName: start.body.result.zmxName,
        },
        homeDir
      )
    );

    const probe = await requestJson('/api/probeSessionProvider', {
      body: {
        params: { projectId: project.projectId, sessionId: session.sessionId },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(probe.status, 200);
    assert.equal(probe.body.result.providerState.lifecycleState, 'exists');
    recordObservation(
      observations,
      'phase5ProbeExists',
      normalizeValue(
        {
          providerState: probe.body.result.providerState.lifecycleState,
          sessionLifecycleState: probe.body.result.session.lifecycleState,
          zmxName: probe.body.result.providerState.zmxName,
        },
        homeDir
      )
    );

    const read = await requestJson('/api/readSessionText', {
      body: {
        params: { projectId: project.projectId, sessionId: session.sessionId },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(read.status, 200);
    assert.equal(read.body.result.limitBytes, GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES);
    assert.equal(typeof read.body.result.text, 'string');
    recordObservation(
      observations,
      'phase5ReadSessionText',
      normalizeValue(
        {
          limitBytes: read.body.result.limitBytes,
          provider: read.body.result.provider,
          source: read.body.result.source,
          textIsString: typeof read.body.result.text === 'string',
          truncated: read.body.result.truncated,
          ...(read.body.result.truncatedReason ? { truncatedReason: read.body.result.truncatedReason } : {}),
          zmxName: read.body.result.zmxName,
        },
        homeDir
      )
    );

    const sendText = await requestJson('/api/sendSessionText', {
      body: {
        params: {
          projectId: project.projectId,
          sessionId: session.sessionId,
          text: 'printf phase5-text',
        },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(sendText.status, 200);
    assert.equal(sendText.body.result.textBytes, Buffer.byteLength('printf phase5-text'));
    assert.equal(sendText.body.result.textLength, 'printf phase5-text'.length);
    recordObservation(
      observations,
      'phase5SendSessionText',
      normalizeValue(
        {
          exitCode: sendText.body.result.exitCode,
          provider: sendText.body.result.provider,
          textBytes: sendText.body.result.textBytes,
          textLength: sendText.body.result.textLength,
          zmxName: sendText.body.result.zmxName,
        },
        homeDir
      )
    );

    const sendEnter = await requestJson('/api/sendSessionEnter', {
      body: {
        params: { projectId: project.projectId, sessionId: session.sessionId },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(sendEnter.status, 200);
    assert.equal(sendEnter.body.result.textBytes, 1);
    recordObservation(
      observations,
      'phase5SendSessionEnter',
      normalizeValue(
        {
          exitCode: sendEnter.body.result.exitCode,
          textBytes: sendEnter.body.result.textBytes,
          textLength: sendEnter.body.result.textLength,
        },
        homeDir
      )
    );

    const sendMessage = await requestJson('/api/sendSessionMessage', {
      body: {
        params: {
          projectId: project.projectId,
          sessionId: session.sessionId,
          submit: false,
          text: 'printf phase5-message',
        },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(sendMessage.status, 200);
    assert.equal(sendMessage.body.result.submit, false);
    recordObservation(
      observations,
      'phase5SendSessionMessage',
      normalizeValue(
        {
          exitCode: sendMessage.body.result.exitCode,
          submit: sendMessage.body.result.submit,
          textBytes: sendMessage.body.result.textBytes,
          textLength: sendMessage.body.result.textLength,
        },
        homeDir
      )
    );

    const focus = await requestJson('/api/focusSession', {
      body: {
        params: { projectId: project.projectId, sessionId: session.sessionId },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(focus.status, 503);
    assertErrorEnvelope(focus.body, 'dependencyUnavailable');
    recordObservation(
      observations,
      'phase5FocusWithoutRenderer',
      normalizeExchange(
        {
          request: {
            body: {
              params: { projectId: project.projectId, sessionId: session.sessionId },
              protocolVersion: PROTOCOL_VERSION,
            },
            method: 'POST',
            path: '/api/focusSession',
            token: '<bearer>',
          },
          response: focus,
        },
        homeDir
      )
    );

    const targetlessMessage = await requestJson('/api/sendSessionMessage', {
      body: {
        params: { agentId: 'codex', text: 'phase5 visible message' },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(targetlessMessage.status, 503);
    assertErrorEnvelope(targetlessMessage.body, 'dependencyUnavailable');
    recordObservation(
      observations,
      'phase5TargetlessMessageWithoutRenderer',
      normalizeExchange(
        {
          request: {
            body: {
              params: { agentId: 'codex', text: '<message>' },
              protocolVersion: PROTOCOL_VERSION,
            },
            method: 'POST',
            path: '/api/sendSessionMessage',
            token: '<bearer>',
          },
          response: targetlessMessage,
        },
        homeDir
      )
    );

    const sleep = await requestJson('/api/sleepSession', {
      body: {
        params: { projectId: project.projectId, sessionId: session.sessionId },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(sleep.status, 200);
    assert.equal(sleep.body.result.kill.killed, true);
    assert.equal(sleep.body.result.session.lifecycleState, 'sleeping');
    forgetLive(project.projectId, session.sessionId);
    recordObservation(
      observations,
      'phase5SleepSession',
      normalizeValue(
        {
          killed: sleep.body.result.kill.killed,
          providerState: sleep.body.result.session.providerState.lifecycleState,
          sessionLifecycleState: sleep.body.result.session.lifecycleState,
          zmxName: sleep.body.result.kill.zmxName,
        },
        homeDir
      )
    );

    const wake = await requestJson('/api/wakeSession', {
      body: {
        params: { projectId: project.projectId, sessionId: session.sessionId, startupText: '' },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(wake.status, 200);
    assert.equal(wake.body.result.attach.providerState.lifecycleState, 'missing');
    assert.equal(wake.body.result.session.lifecycleState, 'running');
    recordObservation(
      observations,
      'phase5WakeSession',
      normalizeValue(
        {
          providerState: wake.body.result.attach.providerState.lifecycleState,
          sessionLifecycleState: wake.body.result.session.lifecycleState,
          startupTextDisposition: wake.body.result.attach.startupTextDisposition,
          zmxName: wake.body.result.attach.zmxName,
        },
        homeDir
      )
    );

    const restart = await requestJson('/api/startSessionProvider', {
      body: {
        params: {
          projectId: project.projectId,
          sessionId: session.sessionId,
          startupText: 'printf phase5-startup',
        },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(restart.status, 200);
    assert.equal(restart.body.result.started, true);
    assert.equal(restart.body.result.startupTextDisposition, 'queueAfterTerminalReady');
    rememberLive(project.projectId, session.sessionId);
    recordObservation(
      observations,
      'phase5RestartWithStartupText',
      normalizeValue(
        {
          providerState: restart.body.result.providerState.lifecycleState,
          started: restart.body.result.started,
          startupTextDisposition: restart.body.result.startupTextDisposition,
        },
        homeDir
      )
    );

    const transition = await requestJson('/api/transitionSession', {
      body: {
        params: { action: 'close', projectId: project.projectId, sessionId: session.sessionId },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(transition.status, 200);
    assert.equal(transition.body.result.action, 'close');
    assert.equal(transition.body.result.transition.kill.killed, true);
    assert.equal(transition.body.result.session.lifecycleState, 'stopped');
    forgetLive(project.projectId, session.sessionId);
    recordObservation(
      observations,
      'phase5TransitionClose',
      normalizeValue(
        {
          action: transition.body.result.action,
          killed: transition.body.result.transition.kill.killed,
          providerState: transition.body.result.session.providerState.lifecycleState,
          sessionLifecycleState: transition.body.result.session.lifecycleState,
        },
        homeDir
      )
    );

    const oversized = await requestJson('/api/sendSessionText', {
      body: {
        params: {
          projectId: project.projectId,
          sessionId: session.sessionId,
          text: 'x'.repeat(GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES + 1),
        },
        protocolVersion: PROTOCOL_VERSION,
      },
      method: 'POST',
      token,
    });
    assert.equal(oversized.status, 400);
    assertErrorEnvelope(oversized.body, 'badRequest');
    recordObservation(
      observations,
      'phase5OversizedSendRejected',
      normalizeExchange(
        {
          request: {
            body: {
              params: {
                projectId: project.projectId,
                sessionId: session.sessionId,
                text: `<${GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES + 1} chars>`,
              },
              protocolVersion: PROTOCOL_VERSION,
            },
            method: 'POST',
            path: '/api/sendSessionText',
            token: '<bearer>',
          },
          response: oversized,
        },
        homeDir
      )
    );
  } finally {
    for (const live of [...liveSessions]) {
      await requestJson('/api/killSession', {
        body: {
          params: { projectId: live.projectId, sessionId: live.sessionId },
          protocolVersion: PROTOCOL_VERSION,
        },
        method: 'POST',
        token,
      }).catch(() => undefined);
      forgetLive(live.projectId, live.sessionId);
    }
  }
}

/*
CDXC:GxserverRustPort 2026-06-16-10:00:
Phase 6 compatibility covers agent settings, launch/resume planning, rename/title/status ingestion, hook setup surfaces, and log privacy through public RPCs on the explicit dev port. Record stable metadata-only projections so fixtures do not include prompts, terminal titles, hook payload bodies, or local absolute paths.
*/
async function runPhase6AgentChecks({ homeDir, observations, paths, token }) {
  const workspaceDir = path.join(homeDir, 'workspace');
  const projectDir = path.join(workspaceDir, 'phase6-project');
  await mkdir(projectDir, { recursive: true });

  const readSettings = await requestJson('/api/readAgentSettings', {
    body: { params: {}, protocolVersion: PROTOCOL_VERSION },
    method: 'POST',
    token,
  });
  assert.equal(readSettings.status, 200);
  assert.equal(readSettings.body.result.isPersisted, false);
  assert.equal(readSettings.body.result.settings.agentAcceptAllEnabled, true);
  assert.equal(readSettings.body.result.settings.defaultPromptAgentId, 'codex');
  recordObservation(
    observations,
    'phase6ReadDefaultAgentSettings',
    normalizeValue(
      {
        isPersisted: readSettings.body.result.isPersisted,
        settings: readSettings.body.result.settings,
      },
      homeDir
    )
  );

  const updateSettingsOff = await requestJson('/api/updateAgentSettings', {
    body: {
      params: { agentAcceptAllEnabled: false, defaultPromptAgentId: ' claude ' },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(updateSettingsOff.status, 200);
  assert.equal(updateSettingsOff.body.result.settings.agentAcceptAllEnabled, false);
  assert.equal(updateSettingsOff.body.result.settings.defaultPromptAgentId, 'claude');
  recordObservation(
    observations,
    'phase6UpdateAgentSettingsOff',
    normalizeValue(updateSettingsOff.body.result, homeDir)
  );

  const createProject = await requestJson('/api/createProject', {
    body: {
      params: {
        customAgents: [{ agentId: 'codex', command: 'codex', name: 'Codex' }],
        name: 'Phase 6 Agents',
        path: projectDir,
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(createProject.status, 200);
  const project = createProject.body.result.project;

  const launchPlanOff = await requestJson('/api/readAgentLaunchPlan', {
    body: {
      params: { agentId: 'codex', projectId: project.projectId },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(launchPlanOff.status, 200);
  assert.equal(launchPlanOff.body.result.plan.command, 'codex');
  assert.equal(launchPlanOff.body.result.plan.startupTextDisposition, 'queueAfterTerminalReady');
  recordObservation(
    observations,
    'phase6LaunchPlanSettingsOff',
    normalizeValue(launchPlanOff.body.result.plan, homeDir)
  );

  const updateSettingsOn = await requestJson('/api/updateAgentSettings', {
    body: {
      params: { agentAcceptAllEnabled: true },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(updateSettingsOn.status, 200);

  const launchPlanOn = await requestJson('/api/readAgentLaunchPlan', {
    body: {
      params: { agentId: 'codex', projectId: project.projectId },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(launchPlanOn.status, 200);
  assert.equal(launchPlanOn.body.result.plan.command, 'codex --yolo');
  recordObservation(observations, 'phase6LaunchPlanSettingsOn', normalizeValue(launchPlanOn.body.result.plan, homeDir));

  const terminalSession = await requestJson('/api/createSession', {
    body: {
      params: {
        kind: 'terminal',
        lifecycleState: 'running',
        projectId: project.projectId,
        title: 'Phase 6 Terminal',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(terminalSession.status, 200);
  const terminal = terminalSession.body.result.session;

  const renameTerminal = await requestJson('/api/requestSessionRename', {
    body: {
      params: {
        projectId: project.projectId,
        sessionId: terminal.sessionId,
        title: 'Phase 6 Terminal Renamed',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(renameTerminal.status, 200);
  assert.equal(renameTerminal.body.result.changed, true);
  assert.equal(renameTerminal.body.result.pendingAgentMetadata, false);
  assert.equal(renameTerminal.body.result.session.title, 'Phase 6 Terminal Renamed');
  recordObservation(
    observations,
    'phase6RenameTerminal',
    normalizeValue(
      {
        changed: renameTerminal.body.result.changed,
        pendingAgentMetadata: renameTerminal.body.result.pendingAgentMetadata,
        reason: renameTerminal.body.result.reason,
        title: renameTerminal.body.result.session.title,
        titleSource: renameTerminal.body.result.session.runtimeSettings.titleSource,
      },
      homeDir
    )
  );

  const forkRejected = await requestJson('/api/forkSession', {
    body: {
      params: { projectId: project.projectId, sessionId: terminal.sessionId },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(forkRejected.status, 400);
  assertErrorEnvelope(forkRejected.body, 'badRequest');
  recordObservation(
    observations,
    'phase6ForkTerminalRejected',
    normalizeExchange(
      {
        request: {
          body: {
            params: { projectId: project.projectId, sessionId: terminal.sessionId },
            protocolVersion: PROTOCOL_VERSION,
          },
          method: 'POST',
          path: '/api/forkSession',
          token: '<bearer>',
        },
        response: forkRejected,
      },
      homeDir
    )
  );

  const agentSessionId = '12345678-1234-1234-1234-123456789abc';
  const createAgent = await requestJson('/api/createSession', {
    body: {
      params: {
        agentId: 'codex',
        kind: 'agent',
        launchSettings: {
          agentLaunchPlan: {
            agentCommand: 'codex',
            command: 'codex --yolo',
            startupText: ' codex --yolo\r',
            startupTextDisposition: 'queueAfterTerminalReady',
          },
        },
        lifecycleState: 'running',
        projectId: project.projectId,
        runtimeSettings: {
          agentCommand: 'codex',
          agentName: 'codex',
          agentSessionId,
          launchAgentId: 'codex',
          titleSource: 'placeholder',
        },
        title: 'Codex Session',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(createAgent.status, 200);
  const agent = createAgent.body.result.session;

  const resumePlan = await requestJson('/api/readAgentResumePlan', {
    body: {
      params: { projectId: project.projectId, sessionId: agent.sessionId },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(resumePlan.status, 200);
  assert.equal(resumePlan.body.result.plan.agentId, 'codex');
  assert.equal(resumePlan.body.result.plan.runtimeCommand, 'codex --yolo');
  assert.equal(resumePlan.body.result.plan.startupTextDisposition, 'queueAfterTerminalReady');
  recordObservation(
    observations,
    'phase6ResumePlan',
    normalizeValue(
      {
        agentId: resumePlan.body.result.plan.agentId,
        hasCopyCommand: typeof resumePlan.body.result.plan.copyCommand === 'string',
        hasPrimaryCommand: typeof resumePlan.body.result.plan.primaryCommand === 'string',
        runtimeCommand: resumePlan.body.result.plan.runtimeCommand,
        startupTextDisposition: resumePlan.body.result.plan.startupTextDisposition,
      },
      homeDir
    )
  );

  const renameAgent = await requestJson('/api/requestSessionRename', {
    body: {
      params: {
        agentName: 'codex',
        projectId: project.projectId,
        sessionId: agent.sessionId,
        title: 'Phase 6 Agent Rename',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(renameAgent.status, 200);
  assert.equal(renameAgent.body.result.pendingAgentMetadata, true);
  recordObservation(
    observations,
    'phase6RenameAgentPending',
    normalizeValue(
      {
        changed: renameAgent.body.result.changed,
        pendingAgentMetadata: renameAgent.body.result.pendingAgentMetadata,
        reason: renameAgent.body.result.reason,
        pendingStatus: renameAgent.body.result.session.runtimeSettings.pendingAgentTitleRequestStatus,
        shouldSendAgentRenameCommand: renameAgent.body.result.shouldSendAgentRenameCommand,
      },
      homeDir
    )
  );

  const genericAgent = await requestJson('/api/createSession', {
    body: {
      params: {
        agentId: 'codex',
        kind: 'agent',
        lifecycleState: 'running',
        projectId: project.projectId,
        runtimeSettings: {
          agentCommand: 'codex',
          agentName: 'codex',
          launchAgentId: 'codex',
          titleSource: 'placeholder',
        },
        title: 'Codex Session',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(genericAgent.status, 200);
  let generic = genericAgent.body.result.session;

  const ingestedAgentSessionId = 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb';
  const ingestState = await requestJson('/api/ingestSessionStateEvent', {
    body: {
      params: {
        agentName: 'codex',
        agentSessionId: ingestedAgentSessionId,
        firstUserMessage: 'phase6 first prompt must not be logged',
        projectId: project.projectId,
        sessionId: generic.sessionId,
        title: 'Phase 6 Ingested Title',
        titleSource: 'user',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(ingestState.status, 200);
  generic = ingestState.body.result.session;
  assert.equal(generic.runtimeSettings.agentSessionId, ingestedAgentSessionId);
  recordObservation(
    observations,
    'phase6IngestSessionState',
    normalizeValue(
      {
        changed: ingestState.body.result.changed,
        reason: ingestState.body.result.reason,
        agentId: generic.agentId,
        agentSessionIdPresent: typeof generic.runtimeSettings.agentSessionId === 'string',
        title: generic.title,
        titleSource: generic.runtimeSettings.titleSource,
      },
      homeDir
    )
  );

  const capturedId = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa';
  const terminalTitle = await requestJson('/api/ingestTerminalTitleEvent', {
    body: {
      params: {
        agentName: 'codex',
        projectId: project.projectId,
        rawTitle: capturedId,
        sessionId: generic.sessionId,
        sessionPersistenceProvider: 'zmx',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(terminalTitle.status, 200);
  assert.equal(terminalTitle.body.result.agentSessionId, capturedId);
  generic = terminalTitle.body.result.session;
  recordObservation(
    observations,
    'phase6TerminalTitleCapture',
    normalizeValue(
      {
        agentSessionIdCaptured: terminalTitle.body.result.agentSessionId === capturedId,
        activity: terminalTitle.body.result.activity.activity,
        changed: terminalTitle.body.result.changed,
        previousActivity: terminalTitle.body.result.previousActivity,
        reason: terminalTitle.body.result.reason,
      },
      homeDir
    )
  );

  const activityWorking = await requestJson('/api/updateAgentActivity', {
    body: {
      params: {
        activity: 'working',
        agentName: 'codex',
        nowMs: 1781604000000,
        projectId: project.projectId,
        sessionId: generic.sessionId,
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(activityWorking.status, 200);
  assert.equal(activityWorking.body.result.activity.activity, 'working');
  generic = activityWorking.body.result.session;
  recordObservation(
    observations,
    'phase6UpdateAgentActivity',
    normalizeValue(
      {
        activity: activityWorking.body.result.activity.activity,
        enteredAttention: activityWorking.body.result.enteredAttention,
        previousActivity: activityWorking.body.result.previousActivity,
        lastActiveAt: generic.lastActiveAt,
      },
      homeDir
    )
  );

  const hookAttention = await requestJson('/api/ingestAgentHookEvent', {
    body: {
      params: {
        agentName: 'codex',
        eventName: 'PermissionRequest',
        projectId: project.projectId,
        sessionId: generic.sessionId,
        statusUpdatedAt: '2026-06-16T10:01:00.000Z',
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(hookAttention.status, 200);
  assert.equal(hookAttention.body.result.activity.activity, 'attention');
  generic = hookAttention.body.result.session;
  recordObservation(
    observations,
    'phase6IngestAgentHookEvent',
    normalizeValue(
      {
        activity: hookAttention.body.result.activity.activity,
        changed: hookAttention.body.result.changed,
        enteredAttention: hookAttention.body.result.enteredAttention,
        previousActivity: hookAttention.body.result.previousActivity,
        reason: hookAttention.body.result.reason,
        sessionActivity: generic.runtimeSettings.agentActivity.activity,
      },
      homeDir
    )
  );

  const currentRuntimeSettings = { ...generic.runtimeSettings, gxserverFirstPromptAutoTitleStatus: 'running' };
  const markAutoTitleRunning = await requestJson('/api/updateSession', {
    body: {
      params: {
        projectId: project.projectId,
        runtimeSettings: currentRuntimeSettings,
        sessionId: generic.sessionId,
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(markAutoTitleRunning.status, 200);
  const cancelAutoTitle = await requestJson('/api/cancelFirstPromptAutoTitle', {
    body: {
      params: {
        projectId: project.projectId,
        reason: 'escape',
        sessionId: generic.sessionId,
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(cancelAutoTitle.status, 200);
  assert.equal(cancelAutoTitle.body.result.changed, true);
  assert.equal(cancelAutoTitle.body.result.previousStatus, 'running');
  recordObservation(
    observations,
    'phase6CancelFirstPromptAutoTitle',
    normalizeValue(
      {
        changed: cancelAutoTitle.body.result.changed,
        previousStatus: cancelAutoTitle.body.result.previousStatus,
        reason: cancelAutoTitle.body.result.reason,
        status: cancelAutoTitle.body.result.session.runtimeSettings.gxserverFirstPromptAutoTitleStatus,
      },
      homeDir
    )
  );

  const hookStatus = await requestJson('/api/readAgentHookStatus', {
    body: {
      params: { agentIds: ['qoder'], autoUpgradeInstalled: false },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(hookStatus.status, 200);
  assert.equal(hookStatus.body.result.type, 'agentHookStatus');
  assert.equal(hookStatus.body.result.agents.length, 1);
  recordObservation(
    observations,
    'phase6ReadAgentHookStatus',
    normalizeValue(
      {
        agentId: hookStatus.body.result.agents[0].agentId,
        cliCommand: hookStatus.body.result.agents[0].cliCommand,
        hasNotifyHookPath: typeof hookStatus.body.result.notifyHookPath === 'string',
        status: hookStatus.body.result.agents[0].status,
        type: hookStatus.body.result.type,
      },
      homeDir
    )
  );

  const hookInstall = await requestJson('/api/installAgentHooks', {
    body: {
      params: { agentIds: ['qoder'] },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(hookInstall.status, 200);
  assert.ok(hookInstall.body.result.installedPaths.length >= 1);
  recordObservation(
    observations,
    'phase6InstallAgentHooks',
    normalizeValue(
      {
        installedPathCount: hookInstall.body.result.installedPaths.length,
        notifyHookInstalled: hookInstall.body.result.installedPaths.includes(hookInstall.body.result.notifyHookPath),
        type: hookInstall.body.result.type,
      },
      homeDir
    )
  );

  let logText = '';
  try {
    logText = await readFile(paths.logFile, 'utf8');
  } catch {}
  assert.equal(logText.includes('phase6 first prompt must not be logged'), false);
  assert.equal(logText.includes('Phase 6 Ingested Title'), false);
  assert.equal(logText.includes(capturedId), false);
  recordObservation(observations, 'phase6LogPrivacy', {
    rawPromptLogged: false,
    rawTitleLogged: false,
    rawAgentSessionIdLogged: false,
  });
}

/*
CDXC:GxserverRustPort 2026-06-16-00:49:
Phase 7 compatibility exercises typed Git/GitHub/worktree/Beads operations and repository clone jobs through public RPCs on the explicit dev port. Use isolated repositories and stubbed clone/GitHub tools so the suite never reaches the network, never shells against arbitrary user paths, and can compare TypeScript and Rust without touching the packaged daemon on 58744.

CDXC:GxserverCompatContainment 2026-06-22-09:47:
Compat fixture generation must be safe to run on developer machines. Build the target process environment from an explicit allowlist, keep HOME/TMP/XDG/Git config under the temp home, and make command shims fail closed so old TypeScript fixtures and new Rust comparisons cannot inherit real tokens, proxy settings, SSH sockets, user profile paths, or network-capable Git/GitHub/agent commands.

CDXC:GxserverCompatLogs 2026-06-22-10:01:
Area 36 privacy applies to persistent compat artifacts too. Tool invocation JSONL should keep only metadata booleans and counts, never raw argv, cwd, clone destinations, paths, URLs, command text, environment values, or secrets.
*/
async function prepareCompatSandbox(homeDir, runOptions) {
  const sandboxPaths = getCompatSandboxPaths(homeDir);
  await mkdir(sandboxPaths.tmpDir, { recursive: true });
  await mkdir(sandboxPaths.xdgCacheHome, { recursive: true });
  await mkdir(sandboxPaths.xdgConfigHome, { recursive: true });
  await mkdir(sandboxPaths.xdgDataHome, { recursive: true });
  await mkdir(sandboxPaths.gitTemplateDir, { recursive: true });
  await mkdir(sandboxPaths.toolStubDir, { recursive: true });
  await writeFile(sandboxPaths.gitConfigFile, '', { mode: 0o600 });
  await prepareCompatToolStubs(homeDir, runOptions, sandboxPaths);
  runOptions.sandboxPaths = sandboxPaths;
}

async function prepareCompatToolStubs(homeDir, runOptions, sandboxPaths) {
  const realGit = process.env.GXSERVER_COMPAT_REAL_GIT || '/usr/bin/git';
  if (!path.isAbsolute(realGit)) {
    throw new Error('GXSERVER_COMPAT_REAL_GIT must be an absolute path when set.');
  }
  const nodeShebang = `#!${process.execPath}`;
  await writeFile(
    path.join(sandboxPaths.toolStubDir, 'git'),
    `${nodeShebang}
const cp = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const homeDir = fs.realpathSync.native(${JSON.stringify(homeDir)});
const invocationLog = ${JSON.stringify(sandboxPaths.invocationLogFile)};
const realGit = process.env.GXSERVER_COMPAT_REAL_GIT || ${JSON.stringify(realGit)};
const args = process.argv.slice(2);
function isInside(parent, candidate) {
  const relative = path.relative(parent, candidate);
  return relative === "" || (!!relative && !relative.startsWith("..") && !path.isAbsolute(relative));
}
function fail(message) {
  process.stderr.write(message + "\\n");
  process.exit(2);
}
function record(extra = {}) {
  fs.appendFileSync(invocationLog, JSON.stringify({
    argCount: args.length,
    cwdInsideHome: isInside(homeDir, process.cwd()),
    hasNetworkLookingArg: args.some((arg) => /^(?:https?|ssh):\\/\\//i.test(String(arg)) || /^git@[^:]+:/i.test(String(arg))),
    tool: "git",
    ...extra,
  }) + "\\n");
}
const cwd = fs.realpathSync.native(process.cwd());
if (!isInside(homeDir, cwd)) {
  fail("compat git cwd escaped temp HOME");
}
if (args.some((arg, index) => index > 0 && /^(?:https?|ssh):\\/\\//i.test(arg))) {
  if (args[0] !== "clone") {
    fail("compat git blocked network-looking argument outside clone");
  }
}
if (args[0] === "clone") {
  const destination = args[args.length - 1];
  const absolute = path.resolve(cwd, destination);
  if (!isInside(homeDir, absolute) || absolute === homeDir) {
    fail("compat git clone destination escaped temp HOME");
  }
  record({ destinationInsideHome: isInside(homeDir, absolute), intercepted: true });
  fs.mkdirSync(absolute, { recursive: true });
  fs.writeFileSync(path.join(absolute, "README.md"), "compat clone\\n");
  process.stdout.write("compat clone ok\\n");
  process.exit(0);
}
const allowed = new Set([
  "--version",
  "commit\\u0000--no-verify\\u0000-F\\u0000-",
  "status\\u0000--short\\u0000--branch",
  "worktree\\u0000list\\u0000--porcelain",
]);
const joined = args.join("\\u0000");
if (!allowed.has(joined)) {
  fail("compat git blocked unsupported args: " + args.join(" "));
}
record({ delegated: true });
const result = cp.spawnSync(realGit, args, {
  env: process.env,
  stdio: ["inherit", "inherit", "inherit"],
});
process.exit(result.status ?? 1);
`,
    { mode: 0o755 }
  );
  await writeFile(
    path.join(sandboxPaths.toolStubDir, 'gh'),
    `${nodeShebang}
const fs = require("node:fs");
const path = require("node:path");
const homeDir = fs.realpathSync.native(${JSON.stringify(homeDir)});
const invocationLog = ${JSON.stringify(sandboxPaths.invocationLogFile)};
const args = process.argv.slice(2);
function isInside(parent, candidate) {
  const relative = path.relative(parent, path.resolve(candidate));
  return relative === "" || (!!relative && !relative.startsWith("..") && !path.isAbsolute(relative));
}
const cwd = fs.realpathSync.native(process.cwd());
if (!isInside(homeDir, cwd)) {
  process.stderr.write("compat gh cwd escaped temp HOME\\n");
  process.exit(2);
}
fs.appendFileSync(invocationLog, JSON.stringify({
  argCount: args.length,
  cwdInsideHome: isInside(homeDir, cwd),
  hasNetworkLookingArg: args.some((arg) => /^(?:https?|ssh):\\/\\//i.test(String(arg)) || /^git@[^:]+:/i.test(String(arg))),
  tool: "gh",
}) + "\\n");
if (args.join(" ") === "--version") {
  process.stdout.write("gh version 2.0.0 (compat)\\n");
  process.exit(0);
}
if (args.join(" ") === "pr view --json number,state,title,url") {
  process.stdout.write(JSON.stringify({ number: 7, state: "OPEN", title: "Compat PR", url: "https://example.invalid/pr/7" }) + "\\n");
  process.exit(0);
}
if (args.join(" ") === "pr create --fill") {
  process.stdout.write("https://example.invalid/pr/8\\n");
  process.exit(0);
}
process.stderr.write("unsupported gh args: " + args.join(" ") + "\\n");
process.exit(2);
`,
    { mode: 0o755 }
  );
  for (const agentCommand of ['claude', 'codex', 'cursor-agent', 'grok']) {
    await writeFile(
      path.join(sandboxPaths.toolStubDir, agentCommand),
      `${nodeShebang}
const fs = require("node:fs");
const path = require("node:path");
const homeDir = fs.realpathSync.native(${JSON.stringify(homeDir)});
const invocationLog = ${JSON.stringify(sandboxPaths.invocationLogFile)};
const args = process.argv.slice(2);
function isInside(parent, candidate) {
  const relative = path.relative(parent, path.resolve(candidate));
  return relative === "" || (!!relative && !relative.startsWith("..") && !path.isAbsolute(relative));
}
fs.appendFileSync(invocationLog, JSON.stringify({
  argCount: args.length,
  cwdInsideHome: isInside(homeDir, process.cwd()),
  tool: ${JSON.stringify(agentCommand)},
}) + "\\n");
process.stdout.write("Compat Generated Title\\n");
`,
      { mode: 0o755 }
    );
  }
  runOptions.toolStubDir = sandboxPaths.toolStubDir;
  runOptions.realGit = realGit;
}

async function runPhase7TypedOperationChecks({ homeDir, observations, paths, runOptions, token }) {
  const workspaceDir = path.join(homeDir, 'workspace');
  const projectDir = path.join(workspaceDir, 'phase7-project');
  const cloneParentDir = path.join(workspaceDir, 'clone-parent');
  await mkdir(projectDir, { recursive: true });
  await mkdir(cloneParentDir, { recursive: true });
  await writeFile(path.join(projectDir, 'file.txt'), 'one\ntwo\n');
  await runGit(['init'], projectDir, homeDir, runOptions);
  await runGit(['config', 'user.email', 'compat@example.invalid'], projectDir, homeDir, runOptions);
  await runGit(['config', 'user.name', 'Compat'], projectDir, homeDir, runOptions);

  const createProject = await requestJson('/api/createProject', {
    body: {
      params: {
        gitConfig: {},
        name: 'Phase 7 Project',
        path: projectDir,
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(createProject.status, 200);
  const project = createProject.body.result.project;

  const gitStatus = await requestJson('/api/runGitAction', {
    body: {
      params: { action: 'status', projectId: project.projectId },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(gitStatus.status, 200);
  assert.equal(gitStatus.body.result.action, 'status');
  assert.equal(gitStatus.body.result.command.executable, 'git');
  recordObservation(
    observations,
    'phase7RunGitStatus',
    normalizeValue(
      {
        action: gitStatus.body.result.action,
        args: gitStatus.body.result.command.args,
        exitCode: gitStatus.body.result.exitCode,
        stdoutHasFile: gitStatus.body.result.stdout.includes('file.txt'),
      },
      homeDir
    )
  );

  const lineCount = await requestJson('/api/runGitAction', {
    body: {
      params: { action: 'countFileLines', filePaths: ['file.txt'], projectPath: projectDir },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(lineCount.status, 200);
  assert.equal(lineCount.body.result.stdout, '2');
  assert.equal(lineCount.body.result.command, undefined);
  recordObservation(observations, 'phase7RunGitCountFileLines', normalizeValue(lineCount.body.result, homeDir));

  const commitPlan = await requestJson('/api/runGitAction', {
    body: {
      params: {
        action: 'commit',
        messageBody: 'private body',
        messageSubject: 'private subject',
        noVerify: true,
        projectId: project.projectId,
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(commitPlan.status, 200);
  assert.deepEqual(commitPlan.body.result.command.args, ['commit', '--no-verify', '-F', '<stdin>']);
  recordObservation(
    observations,
    'phase7RunGitCommitRedaction',
    normalizeValue(
      {
        action: commitPlan.body.result.action,
        command: commitPlan.body.result.command,
        exitCodeType: typeof commitPlan.body.result.exitCode,
        stderrLoggedSubject: commitPlan.body.result.stderr.includes('private subject'),
        stdoutLoggedSubject: commitPlan.body.result.stdout.includes('private subject'),
      },
      homeDir
    )
  );

  const ghVersion = await requestJson('/api/runGitHubAction', {
    body: {
      params: { action: 'version', projectId: project.projectId },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(ghVersion.status, 200);
  assert.equal(ghVersion.body.result.stdout, 'gh version 2.0.0 (compat)');
  recordObservation(
    observations,
    'phase7RunGitHubVersion',
    normalizeValue(
      {
        action: ghVersion.body.result.action,
        args: ghVersion.body.result.command.args,
        exitCode: ghVersion.body.result.exitCode,
        stdout: ghVersion.body.result.stdout,
      },
      homeDir
    )
  );

  const worktreeExists = await requestJson('/api/runWorktreeAction', {
    body: {
      params: {
        action: 'pathExists',
        projectId: project.projectId,
        worktreePath: path.join(workspaceDir, 'phase7-project-copy'),
      },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(worktreeExists.status, 200);
  assert.equal(worktreeExists.body.result.stdout, 'false');
  recordObservation(observations, 'phase7RunWorktreePathExists', normalizeValue(worktreeExists.body.result, homeDir));

  const worktreeList = await requestJson('/api/runWorktreeAction', {
    body: {
      params: { action: 'list', projectId: project.projectId },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(worktreeList.status, 200);
  assert.equal(worktreeList.body.result.action, 'list');
  recordObservation(
    observations,
    'phase7RunWorktreeList',
    normalizeValue(
      {
        action: worktreeList.body.result.action,
        exitCode: worktreeList.body.result.exitCode,
        worktreeCount: Array.isArray(worktreeList.body.result.worktrees)
          ? worktreeList.body.result.worktrees.length
          : 0,
      },
      homeDir
    )
  );

  const setupNoop = await requestJson('/api/runProjectSetupCommand', {
    body: {
      params: { action: 'worktreeSetupCommand', projectId: project.projectId },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(setupNoop.status, 200);
  assert.equal(setupNoop.body.result.command, undefined);
  recordObservation(observations, 'phase7RunProjectSetupNoop', normalizeValue(setupNoop.body.result, homeDir));

  const beadsStorage = await requestJson('/api/runBeadsAction', {
    body: {
      params: { action: 'storageExists', projectBoardScope: true, projectId: project.projectId },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(beadsStorage.status, 200);
  assert.equal(beadsStorage.body.result.stdout, 'false');
  recordObservation(observations, 'phase7RunBeadsStorageExists', normalizeValue(beadsStorage.body.result, homeDir));

  const scopeRejected = await requestJson('/api/runGitAction', {
    body: {
      params: { action: 'status', projectPath: path.join(workspaceDir, 'unregistered') },
      protocolVersion: PROTOCOL_VERSION,
    },
    method: 'POST',
    token,
  });
  assert.equal(scopeRejected.status, 404);
  assertErrorEnvelope(scopeRejected.body, 'notFound');
  recordObservation(
    observations,
    'phase7TypedScopeRejected',
    normalizeExchange(
      {
        request: {
          body: {
            params: { action: 'status', projectPath: path.join(workspaceDir, 'unregistered') },
            protocolVersion: PROTOCOL_VERSION,
          },
          method: 'POST',
          path: '/api/runGitAction',
          token: '<bearer>',
        },
        response: scopeRejected,
      },
      homeDir
    )
  );

  const cloneParams = {
    branchName: 'main',
    cloneMainOnly: true,
    destinationFolderName: 'phase7-clone',
    parentPath: cloneParentDir,
    repositoryInput: 'gh repo clone factory-ai/ghostex',
    shallowClone: true,
  };
  const clonePreview = await requestJson('/api/previewRepositoryClone', {
    body: { params: cloneParams, protocolVersion: PROTOCOL_VERSION },
    method: 'POST',
    token,
  });
  assert.equal(clonePreview.status, 200);
  assert.equal(clonePreview.body.result.preview.cloneUrl, 'https://github.com/factory-ai/ghostex.git');
  assert.equal(clonePreview.body.result.preview.destinationExists, false);
  recordObservation(
    observations,
    'phase7PreviewRepositoryClone',
    normalizeValue(
      {
        branchName: clonePreview.body.result.preview.branchName,
        cloneMainOnly: clonePreview.body.result.preview.cloneMainOnly,
        cloneUrl: clonePreview.body.result.preview.cloneUrl,
        destinationExists: clonePreview.body.result.preview.destinationExists,
        destinationFolderName: clonePreview.body.result.preview.destinationFolderName,
        repositoryName: clonePreview.body.result.preview.repositoryName,
        shallowClone: clonePreview.body.result.preview.shallowClone,
      },
      homeDir
    )
  );

  const cloneStart = await requestJson('/api/startRepositoryClone', {
    body: { params: cloneParams, protocolVersion: PROTOCOL_VERSION },
    method: 'POST',
    token,
  });
  assert.equal(cloneStart.status, 200);
  assert.equal(cloneStart.body.result.job.state, 'running');
  const cloneJob = await waitForRepositoryCloneJob(cloneStart.body.result.job.jobId, token);
  assert.equal(cloneJob.state, 'completed');
  assert.equal(cloneJob.projectPath, path.join(cloneParentDir, 'phase7-clone'));
  recordObservation(
    observations,
    'phase7StartRepositoryClone',
    normalizeValue(
      {
        completed: cloneJob.state === 'completed',
        exitCode: cloneJob.exitCode,
        hasProject: Boolean(cloneJob.project?.projectId),
        message: cloneJob.message,
        projectPath: cloneJob.projectPath,
        stdout: cloneJob.stdout,
      },
      homeDir
    )
  );

  const existingPreview = await requestJson('/api/previewRepositoryClone', {
    body: { params: cloneParams, protocolVersion: PROTOCOL_VERSION },
    method: 'POST',
    token,
  });
  assert.equal(existingPreview.status, 200);
  assert.equal(existingPreview.body.result.preview.destinationExists, true);
  const existingStart = await requestJson('/api/startRepositoryClone', {
    body: { params: cloneParams, protocolVersion: PROTOCOL_VERSION },
    method: 'POST',
    token,
  });
  assert.equal(existingStart.status, 400);
  assertErrorEnvelope(existingStart.body, 'badRequest');
  recordObservation(
    observations,
    'phase7CloneExistingRejected',
    normalizeExchange(
      {
        request: {
          body: { params: cloneParams, protocolVersion: PROTOCOL_VERSION },
          method: 'POST',
          path: '/api/startRepositoryClone',
          token: '<bearer>',
        },
        response: existingStart,
      },
      homeDir
    )
  );

  let logText = '';
  try {
    logText = await readFile(paths.logFile, 'utf8');
  } catch {}
  assert.equal(logText.includes('private subject'), false);
  assert.equal(logText.includes('factory-ai/ghostex'), false);
  assert.equal(logText.includes('phase7-clone'), false);
  assert.equal(logText.includes(cloneParentDir), false);
  recordObservation(observations, 'phase7LogPrivacy', {
    clonePathLogged: false,
    cloneUrlLogged: false,
    commitSubjectLogged: false,
  });
}

async function runGit(args, cwd, homeDir, runOptions) {
  const result = await runCommand(process.env.GXSERVER_COMPAT_REAL_GIT || '/usr/bin/git', args, {
    cwd,
    env: createGitSetupEnv(homeDir, runOptions),
    timeoutMs: 20_000,
  });
  assert.equal(result.exitCode, 0, result.stderr);
  return result;
}

async function waitForRepositoryCloneJob(jobId, token) {
  const deadline = Date.now() + 10_000;
  let latest;
  while (Date.now() < deadline) {
    const response = await requestJson('/api/readRepositoryCloneJob', {
      body: { params: { jobId }, protocolVersion: PROTOCOL_VERSION },
      method: 'POST',
      token,
    });
    assert.equal(response.status, 200);
    latest = response.body.result.job;
    if (latest.state !== 'running') {
      return latest;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out waiting for repository clone job. Latest: ${JSON.stringify(latest)}`);
}

function parseArgs(args) {
  const options = {
    bin: undefined,
    help: false,
    keepHome: false,
    suite: 'phase0',
    skipIfPortBusy: false,
    target: 'ts',
    timeoutMs: 7_500,
    updateFixtures: false,
    port: DEFAULT_LOCAL_PORT,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    switch (arg) {
      case '--bin':
        options.bin = readArgValue(args, ++index, arg);
        break;
      case '--help':
      case '-h':
        options.help = true;
        break;
      case '--keep-home':
        options.keepHome = true;
        break;
      case '--suite':
        options.suite = readArgValue(args, ++index, arg);
        break;
      case '--skip-if-port-busy':
        options.skipIfPortBusy = true;
        break;
      case '--port':
        options.port = parsePort(readArgValue(args, ++index, arg));
        break;
      case '--target':
        options.target = readArgValue(args, ++index, arg);
        break;
      case '--timeout-ms':
        options.timeoutMs = Number(readArgValue(args, ++index, arg));
        if (!Number.isFinite(options.timeoutMs) || options.timeoutMs <= 0) {
          throw new Error('--timeout-ms must be a positive number.');
        }
        break;
      case '--update-fixtures':
        options.updateFixtures = true;
        break;
      default:
        throw new Error(`Unknown argument: ${arg}`);
    }
  }
  return options;
}

function parsePort(value) {
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error('--port must be an integer from 1 to 65535.');
  }
  return port;
}

function readArgValue(args, index, flag) {
  const value = args[index];
  if (!value || value.startsWith('--')) {
    throw new Error(`${flag} requires a value.`);
  }
  return value;
}

function printUsage() {
  console.log(`Usage: node gxserver-rs/compat/run-compat.mjs [options]

Options:
  --target ts|rust          Daemon target to run. Defaults to ts.
  --suite phase0|phase3|phase4|phase5|phase6|phase7
                            Compatibility suite. Defaults to phase0.
  --bin <path>             Rust binary path when --target rust is used.
  --port <port>            Explicit local development/compatibility port. Defaults to 58744.
  --update-fixtures        Replace phase0-observed-ts.json from the current TypeScript target.
  --skip-if-port-busy      Exit successfully without running if the selected local port is occupied.
  --keep-home              Keep the isolated HOME for debugging.
  --timeout-ms <ms>        Poll and process timeout. Defaults to 7500.
`);
}

function resolveTarget(runOptions) {
  if (runOptions.target === 'ts') {
    const cliPath = path.join(repoRoot, 'gxserver', 'dist', 'src', 'cli.js');
    if (!existsSync(cliPath)) {
      throw new Error(`Missing ${cliPath}. Run: npm --prefix gxserver run build`);
    }
    return {
      command: process.execPath,
      cwd: repoRoot,
      foregroundArgs: [cliPath, '--foreground'],
      name: 'typescript',
      statusArgs: [cliPath, 'status', '--json'],
      versionArgs: [cliPath, '--version'],
    };
  }

  if (runOptions.target === 'rust') {
    const binaryPath =
      runOptions.bin ??
      process.env.GHOSTEX_GXSERVER_RUST_BIN ??
      path.join(gxserverRsRoot, 'target', 'debug', 'gxserver');
    if (!existsSync(binaryPath)) {
      throw new Error(`Missing Rust gxserver binary at ${binaryPath}. Pass --bin or set GHOSTEX_GXSERVER_RUST_BIN.`);
    }
    return {
      command: binaryPath,
      cwd: repoRoot,
      foregroundArgs: ['--foreground'],
      name: 'rust',
      statusArgs: ['status', '--json'],
      versionArgs: ['--version'],
    };
  }

  throw new Error(`Unsupported target: ${runOptions.target}`);
}

async function readTargetVersion(target, timeoutMs, env) {
  const result = await runCommand(target.command, target.versionArgs, { cwd: target.cwd, env, timeoutMs });
  assert.equal(result.exitCode, 0, result.stderr);
  const version = result.stdout.trim();
  assert.match(version, /^\d+\.\d+\.\d+(?:[-+][A-Za-z0-9.-]+)?$/u);
  return version;
}

async function runTargetCommand(target, args, env, timeoutMs) {
  return await runCommand(target.command, args, {
    cwd: target.cwd,
    env,
    timeoutMs,
  });
}

function createTargetEnv(homeDir, runOptions) {
  /*
  CDXC:GxserverRustPort 2026-06-14-21:58:
  Compatibility runs may target an explicitly selected loopback port while the packaged daemon owns 58744. Pass the port through a dev-scoped environment variable only when --port is explicit so product defaults remain unchanged and startup never silently falls back to another daemon.
  */
  const sandboxPaths = runOptions.sandboxPaths ?? getCompatSandboxPaths(homeDir);
  const pathEntries = [...(runOptions.toolStubDir ? [runOptions.toolStubDir] : []), ...COMPAT_SAFE_SYSTEM_PATHS];
  return {
    ...(runOptions.port !== DEFAULT_LOCAL_PORT ? { [DEV_PORT_ENV]: String(runOptions.port) } : {}),
    ...(runOptions.realGit ? { GXSERVER_COMPAT_REAL_GIT: runOptions.realGit } : {}),
    GCM_INTERACTIVE: 'never',
    GIT_ASKPASS: '/bin/false',
    GIT_CONFIG_GLOBAL: sandboxPaths.gitConfigFile,
    GIT_CONFIG_NOSYSTEM: '1',
    GIT_TEMPLATE_DIR: sandboxPaths.gitTemplateDir,
    GIT_TERMINAL_PROMPT: '0',
    GHOSTEX_SOURCE_ROOT: repoRoot,
    HOME: homeDir,
    LANG: 'C.UTF-8',
    LC_ALL: 'C.UTF-8',
    LOGNAME: COMPAT_USER,
    NO_PROXY: `${LOCAL_HOST},localhost`,
    PATH: pathEntries.join(path.delimiter),
    SHELL: resolveCompatShell(),
    SSH_ASKPASS: '/bin/false',
    TMPDIR: sandboxPaths.tmpDir,
    USER: COMPAT_USER,
    XDG_CACHE_HOME: sandboxPaths.xdgCacheHome,
    XDG_CONFIG_HOME: sandboxPaths.xdgConfigHome,
    XDG_DATA_HOME: sandboxPaths.xdgDataHome,
  };
}

function createGitSetupEnv(homeDir, runOptions) {
  const targetEnv = createTargetEnv(homeDir, runOptions);
  return {
    ...targetEnv,
    PATH: COMPAT_SAFE_SYSTEM_PATHS.join(path.delimiter),
  };
}

function resolveCompatShell() {
  /*
  CDXC:GxserverUbuntu 2026-06-23-07:52:
  The compat harness must exercise the same server code on macOS and Ubuntu. Keep zsh when present for mac parity, but use bash/sh on Linux sandboxes so tests do not fail before gxserver can prove platform-neutral behavior.
  */
  const candidates = (
    process.platform === 'darwin'
      ? ['/bin/zsh', process.env.SHELL, '/usr/bin/zsh', '/bin/bash', '/usr/bin/bash']
      : [process.env.SHELL, '/bin/bash', '/usr/bin/bash', '/bin/zsh', '/usr/bin/zsh']
  )
    .concat(['/bin/sh', '/usr/bin/sh'])
    .filter(Boolean);
  for (const candidate of candidates) {
    if (['bash', 'sh', 'zsh'].includes(path.basename(candidate)) && existsSync(candidate)) {
      return candidate;
    }
  }
  return '/bin/sh';
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
}

async function runCommand(command, args, { cwd, env = process.env, timeoutMs }) {
  const child = spawn(command, args, {
    cwd,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stdout = '';
  let stderr = '';
  child.stdout.on('data', (chunk) => {
    stdout += String(chunk);
  });
  child.stderr.on('data', (chunk) => {
    stderr += String(chunk);
  });
  let timeout;
  try {
    const [exitCode, signal] = await Promise.race([
      once(child, 'exit'),
      new Promise((_, reject) => {
        timeout = setTimeout(() => {
          child.kill('SIGTERM');
          reject(new Error(`Timed out running ${command} ${args.join(' ')}`));
        }, timeoutMs);
      }),
    ]);
    return {
      exitCode: exitCode ?? (signal ? 1 : 0),
      stderr,
      stdout,
    };
  } finally {
    if (timeout) {
      clearTimeout(timeout);
    }
  }
}

function collectChildOutput(child, output) {
  child.stdout.on('data', (chunk) => {
    output.stdout = appendBounded(output.stdout, String(chunk));
  });
  child.stderr.on('data', (chunk) => {
    output.stderr = appendBounded(output.stderr, String(chunk));
  });
}

function appendBounded(current, chunk) {
  return `${current}${chunk}`.slice(-16_000);
}

function getGxserverPaths(homeDir) {
  const rootDir = path.join(homeDir, '.ghostex', 'gxserver');
  const authDir = path.join(rootDir, 'auth');
  const runtimeDir = path.join(rootDir, 'runtime');
  const logsDir = path.join(homeDir, '.ghostex', 'logs');
  return {
    authDir,
    authTokenFile: path.join(authDir, 'token'),
    configFile: path.join(rootDir, 'config.json'),
    identityFile: path.join(rootDir, 'identity.json'),
    logFile: path.join(logsDir, 'gxserver.jsonl'),
    logsDir,
    rootDir,
    runtimeMetadataFile: path.join(runtimeDir, 'server.json'),
    stateDbFile: path.join(rootDir, 'state.db'),
    zmxDir: path.join(rootDir, 'zmx'),
  };
}

function getCompatSandboxPaths(homeDir) {
  const compatDir = path.join(homeDir, 'compat-sandbox');
  return {
    gitConfigFile: path.join(compatDir, 'gitconfig'),
    gitTemplateDir: path.join(compatDir, 'git-template'),
    invocationLogFile: path.join(compatDir, 'tool-invocations.jsonl'),
    tmpDir: path.join(compatDir, 'tmp'),
    toolStubDir: path.join(compatDir, 'bin'),
    xdgCacheHome: path.join(compatDir, 'xdg-cache'),
    xdgConfigHome: path.join(compatDir, 'xdg-config'),
    xdgDataHome: path.join(compatDir, 'xdg-data'),
  };
}

function assertCompatTargetEnv(env, homeDir) {
  assert.equal(env.HOME, homeDir);
  assert.equal(env.TMPDIR, path.join(homeDir, 'compat-sandbox', 'tmp'));
  assert.equal(env.GIT_TERMINAL_PROMPT, '0');
  assert.equal(env.GIT_CONFIG_NOSYSTEM, '1');
  assert.equal(env.GHOSTEX_SOURCE_ROOT, repoRoot);
  for (const forbiddenKey of [
    'ANTHROPIC_API_KEY',
    'AWS_ACCESS_KEY_ID',
    'AWS_SECRET_ACCESS_KEY',
    'GH_TOKEN',
    'GITHUB_TOKEN',
    'HTTP_PROXY',
    'HTTPS_PROXY',
    'NETRC',
    'OPENAI_API_KEY',
    'SSH_AUTH_SOCK',
  ]) {
    assert.equal(Object.hasOwn(env, forbiddenKey), false, `compat target env leaked ${forbiddenKey}`);
  }
  const realHome = process.env.HOME;
  if (realHome && realHome !== homeDir) {
    for (const [key, value] of Object.entries(env)) {
      if (key === 'GHOSTEX_SOURCE_ROOT') {
        continue;
      }
      assert.equal(String(value).includes(realHome), false, `compat target env ${key} leaked real HOME`);
    }
  }
}

async function assertCompatSandboxContained(homeDir, runOptions) {
  const sandboxPaths = runOptions.sandboxPaths ?? getCompatSandboxPaths(homeDir);
  let text = '';
  try {
    text = await readFile(sandboxPaths.invocationLogFile, 'utf8');
  } catch {
    return;
  }
  for (const line of text.split('\n')) {
    if (!line.trim()) {
      continue;
    }
    const invocation = JSON.parse(line);
    assert.equal(Object.hasOwn(invocation, 'args'), false, 'compat invocation log persisted raw args');
    assert.equal(Object.hasOwn(invocation, 'cwd'), false, 'compat invocation log persisted cwd');
    assert.equal(Object.hasOwn(invocation, 'destination'), false, 'compat invocation log persisted destination');
    assert.equal(invocation.cwdInsideHome, true, `compat ${invocation.tool} cwd escaped temp HOME`);
    if (Object.hasOwn(invocation, 'destinationInsideHome')) {
      assert.equal(invocation.destinationInsideHome, true, 'compat git clone destination escaped temp HOME');
    }
    if (invocation.tool === 'git' && invocation.delegated === true) {
      assert.equal(
        invocation.hasNetworkLookingArg,
        false,
        'compat delegated git command included a network-looking argument'
      );
    }
  }
}

function isPathInside(parentPath, candidatePath) {
  const relative = path.relative(resolveExistingPath(parentPath), resolveExistingPath(candidatePath));
  return relative === '' || (!!relative && !relative.startsWith('..') && !path.isAbsolute(relative));
}

function resolveExistingPath(inputPath) {
  try {
    return realpathSync.native(inputPath);
  } catch {
    return path.resolve(inputPath);
  }
}

function hasNetworkLookingArg(args) {
  return (
    Array.isArray(args) &&
    args.some((arg) => /^(?:https?|ssh):\/\//iu.test(String(arg)) || /^git@[^:]+:/iu.test(String(arg)))
  );
}

async function waitForFileText(filePath, timeoutMs, child, output) {
  return await waitFor(
    async () => {
      assertChildStillUseful(child, output);
      try {
        return await readFile(filePath, 'utf8');
      } catch {
        return undefined;
      }
    },
    timeoutMs,
    `Timed out waiting for ${filePath}.`
  );
}

async function waitForServerReady(token, timeoutMs, child, output) {
  await waitFor(
    async () => {
      assertChildStillUseful(child, output);
      try {
        const response = await requestJson('/api/health/server', {
          method: 'GET',
          protocolVersion: PROTOCOL_VERSION,
          token,
        });
        return response.status === 200 ? true : undefined;
      } catch {
        return undefined;
      }
    },
    timeoutMs,
    'Timed out waiting for authenticated gxserver health.'
  );
}

async function waitFor(callback, timeoutMs, errorMessage) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const value = await callback();
      if (value !== undefined) {
        return value;
      }
    } catch (error) {
      lastError = error;
    }
    await delay(50);
  }
  if (lastError) {
    throw lastError;
  }
  throw new Error(errorMessage);
}

function assertChildStillUseful(child, output) {
  if (child.exitCode !== null) {
    throw new Error(
      `gxserver foreground exited early with code ${child.exitCode}.\nstdout:\n${output.stdout}\nstderr:\n${output.stderr}`
    );
  }
}

async function delay(ms) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function requestJson(pathname, requestOptions) {
  const headers = {};
  if (requestOptions.token) {
    headers.authorization = `Bearer ${requestOptions.token}`;
  }
  if (requestOptions.protocolVersion !== undefined) {
    headers[PROTOCOL_HEADER] = String(requestOptions.protocolVersion);
  }
  if (requestOptions.body !== undefined) {
    headers['content-type'] = 'application/json';
  }
  const response = await fetch(compatHttpUrl(pathname), {
    body: requestOptions.body === undefined ? undefined : JSON.stringify(requestOptions.body),
    headers,
    method: requestOptions.method,
  });
  const text = await response.text();
  return {
    body: text.trim() ? JSON.parse(text) : undefined,
    headers: Object.fromEntries(response.headers.entries()),
    status: response.status,
  };
}

function compatHttpUrl(pathname) {
  assert.equal(typeof pathname, 'string');
  assert.equal(pathname.startsWith('/'), true, 'compat HTTP requests must use local absolute paths');
  assert.equal(pathname.startsWith('//'), false, 'compat HTTP requests must not use protocol-relative URLs');
  const url = new URL(pathname, `http://${LOCAL_HOST}:${options.port}`);
  assert.equal(url.protocol, 'http:');
  assert.equal(url.hostname, LOCAL_HOST);
  assert.equal(url.port, String(options.port));
  return url;
}

function compatWebSocketUrl(pathname, query) {
  const url = compatHttpUrl(pathname);
  url.protocol = 'ws:';
  for (const [key, value] of Object.entries(query)) {
    url.searchParams.set(key, String(value));
  }
  return url;
}

function assertErrorEnvelope(body, error) {
  assert.equal(body.ok, false);
  assert.equal(body.product, PRODUCT);
  assert.equal(body.error, error);
  assert.equal(body.protocolVersion, PROTOCOL_VERSION);
  assert.equal(typeof body.message, 'string');
  assert.equal(typeof body.requestId, 'string');
}

function assertSuccessEnvelope(body) {
  assert.equal(body.ok, true);
  assert.equal(body.product, PRODUCT);
  assert.equal(body.protocolVersion, PROTOCOL_VERSION);
  assert.equal(typeof body.requestId, 'string');
  assert.equal(typeof body.result, 'object');
}

function assertAuthenticatedHealth(body, version) {
  assert.equal(body.ok, true);
  assert.equal(body.product, PRODUCT);
  assert.equal(body.protocolVersion, PROTOCOL_VERSION);
  assert.equal(body.version, version);
  assert.equal(body.port, options.port);
  assert.equal(typeof body.pid, 'number');
  assert.match(body.serverId, /^S\d+[a-z0-9]+$/u);
  assert.match(body.startedAt, /^\d{4}-\d{2}-\d{2}T/u);
  assert.equal(typeof body.buildIdentity, 'string');
  assert.deepEqual(body.capabilities, EXPECTED_CAPABILITIES);
  assert.equal(body.listeners.local.enabled, true);
  assert.equal(body.listeners.local.host, LOCAL_HOST);
  assert.equal(body.listeners.local.kind, 'local');
  assert.equal(body.listeners.local.port, options.port);
  assert.equal(body.listeners.remote.enabled, false);
  assert.equal(body.listeners.remote.host, '0.0.0.0');
  assert.equal(body.listeners.remote.kind, 'remote');
  assert.equal(body.listeners.remote.port, 58745);
  assert.equal(body.migration.currentVersion, CURRENT_MIGRATION_VERSION);
  assert.deepEqual(body.migration.appliedMigrations, EXPECTED_MIGRATIONS);
  assert.equal(Array.isArray(body.tools), true);
}

async function assertRuntimeFiles(paths, token) {
  assert.equal((await readFile(paths.authTokenFile, 'utf8')).trim(), token);
  assert.equal(pathMode(await stat(paths.authDir)), 0o700);
  assert.equal(pathMode(await stat(paths.authTokenFile)), 0o600);
  await assertFileExists(paths.configFile);
  await assertFileExists(paths.identityFile);
  await assertFileExists(paths.logsDir);
  await assertFileExists(paths.runtimeMetadataFile);
  await assertFileExists(paths.stateDbFile);
  await assertFileExists(paths.zmxDir);
  const runtimeMetadata = JSON.parse(await readFile(paths.runtimeMetadataFile, 'utf8'));
  assert.equal(runtimeMetadata.port, options.port);
  assert.equal(runtimeMetadata.protocolVersion, PROTOCOL_VERSION);
}

async function assertFileExists(filePath) {
  await stat(filePath);
}

function pathMode(stats) {
  return stats.mode & 0o777;
}

async function assertRuntimeMetadataRemoved(paths) {
  try {
    await stat(paths.runtimeMetadataFile);
  } catch (error) {
    if (error && error.code === 'ENOENT') {
      return;
    }
    throw error;
  }
  throw new Error(`${paths.runtimeMetadataFile} still exists after control stop.`);
}

async function readEventStreamReady(token) {
  if (typeof WebSocket === 'undefined') {
    throw new Error('Global WebSocket is unavailable. Use Node 22 or newer.');
  }
  const url = compatWebSocketUrl('/api/events', {
    authToken: token,
    protocolVersion: PROTOCOL_VERSION,
  });
  const socket = new WebSocket(url);
  try {
    const text = await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error('Timed out waiting for eventStreamReady.')), 5_000);
      socket.addEventListener(
        'message',
        async (event) => {
          clearTimeout(timeout);
          try {
            resolve(await webSocketDataToText(event.data));
          } catch (error) {
            reject(error);
          }
        },
        { once: true }
      );
      socket.addEventListener(
        'error',
        () => {
          clearTimeout(timeout);
          reject(new Error('gxserver event WebSocket failed.'));
        },
        { once: true }
      );
    });
    return {
      body: JSON.parse(text.trim()),
      rawEndsWithNewline: text.endsWith('\n'),
    };
  } finally {
    socket.close();
  }
}

async function openEventSocket(token) {
  if (typeof WebSocket === 'undefined') {
    throw new Error('Global WebSocket is unavailable. Use Node 22 or newer.');
  }
  const url = compatWebSocketUrl('/api/events', {
    authToken: token,
    protocolVersion: PROTOCOL_VERSION,
  });
  const socket = new WebSocket(url);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', () => reject(new Error('gxserver event WebSocket failed.')), { once: true });
  });
  const ready = await nextWebSocketEvent(socket, 'eventStreamReady');
  assert.equal(ready.type, 'eventStreamReady');
  assert.equal(ready.protocolVersion, PROTOCOL_VERSION);
  return socket;
}

async function nextWebSocketEvent(socket, type, timeoutMs = 5_000) {
  return await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      socket.removeEventListener('message', onMessage);
      reject(new Error(`Timed out waiting for WebSocket event ${type}.`));
    }, timeoutMs);
    async function onMessage(event) {
      let parsed;
      try {
        parsed = JSON.parse((await webSocketDataToText(event.data)).trim());
      } catch (error) {
        clearTimeout(timeout);
        socket.removeEventListener('message', onMessage);
        reject(error);
        return;
      }
      if (parsed.type !== type) {
        return;
      }
      clearTimeout(timeout);
      socket.removeEventListener('message', onMessage);
      resolve(parsed);
    }
    socket.addEventListener('message', onMessage);
    socket.addEventListener(
      'error',
      () => {
        clearTimeout(timeout);
        socket.removeEventListener('message', onMessage);
        reject(new Error('gxserver event WebSocket failed.'));
      },
      { once: true }
    );
  });
}

async function observeWebSocketEvent(socket, type, timeoutMs) {
  try {
    return { event: await nextWebSocketEvent(socket, type, timeoutMs) };
  } catch (error) {
    return { error };
  }
}

async function webSocketDataToText(data) {
  if (typeof data === 'string') {
    return data;
  }
  if (data instanceof ArrayBuffer) {
    return Buffer.from(data).toString('utf8');
  }
  if (ArrayBuffer.isView(data)) {
    return Buffer.from(data.buffer, data.byteOffset, data.byteLength).toString('utf8');
  }
  if (data && typeof data.arrayBuffer === 'function') {
    return Buffer.from(await data.arrayBuffer()).toString('utf8');
  }
  return String(data);
}

async function waitForProcessExit(child, timeoutMs, output) {
  if (child.exitCode !== null) {
    return;
  }
  let timeout;
  try {
    await Promise.race([
      once(child, 'exit'),
      new Promise((_, reject) => {
        timeout = setTimeout(() => {
          reject(
            new Error(
              `Timed out waiting for gxserver foreground exit.\nstdout:\n${output.stdout}\nstderr:\n${output.stderr}`
            )
          );
        }, timeoutMs);
      }),
    ]);
  } finally {
    if (timeout) {
      clearTimeout(timeout);
    }
  }
}

async function isTcpPortAvailable(port) {
  const server = createServer();
  return await new Promise((resolve) => {
    server.once('error', () => resolve(false));
    server.listen(port, LOCAL_HOST, () => {
      server.close(() => resolve(true));
    });
  });
}

function recordObservation(observations, name, value) {
  observations.tests.push({ name, value });
}

function normalizeExchange(exchange, homeDir) {
  return normalizeValue(exchange, homeDir);
}

function normalizeValue(value, homeDir, key = '') {
  if (Array.isArray(value)) {
    if (key === 'tools') {
      return value.map(normalizeToolStatus);
    }
    const normalized = value.map((item) => normalizeValue(item, homeDir));
    if (['groups', 'projects', 'results', 'sessions', 'sessionIds'].includes(key)) {
      return normalized.sort((left, right) => JSON.stringify(left).localeCompare(JSON.stringify(right)));
    }
    return normalized;
  }
  if (value && typeof value === 'object') {
    if (key === 'headers') {
      return normalizeHeaders(value);
    }
    const normalized = {};
    for (const [entryKey, entryValue] of Object.entries(value)) {
      if (isVolatileCompatKey(entryKey)) {
        continue;
      }
      normalized[entryKey] = normalizeValue(entryValue, homeDir, entryKey);
    }
    return normalized;
  }
  if (typeof value === 'string') {
    const text = value.replaceAll(homeDir, '<home>').replaceAll(repoRoot, '<repo>');
    if (key === 'requestId') {
      return '<requestId>';
    }
    if (key === 'commandId') {
      return '<commandId>';
    }
    if (key === 'serverId') {
      return '<serverId>';
    }
    if (key === 'startedAt' || key === 'createdAt' || key === 'updatedAt' || key === 'completedAt') {
      return '<timestamp>';
    }
    if (key === 'date') {
      return '<httpDate>';
    }
    if (key === 'buildIdentity') {
      return '<buildIdentity>';
    }
    return normalizeDynamicIds(text);
  }
  if (typeof value === 'number') {
    if (key === 'pid') {
      return '<pid>';
    }
    if (key === 'port' && value === options.port) {
      return '<localPort>';
    }
    return value;
  }
  return value;
}

function isVolatileCompatKey(key) {
  return key === 'revision' || key === 'titleObservation' || key === 'tooltip' || key === 'zmxTitleObservation';
}

function normalizeHeaders(headers) {
  return {
    ...(typeof headers['content-type'] === 'string' ? { 'content-type': headers['content-type'] } : {}),
  };
}

function normalizeDynamicIds(value) {
  return value
    .replaceAll(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z/gu, '<timestamp>')
    .replaceAll(/\bS\d[a-z0-9]:P\d[a-z0-9]{3}:G\d[a-z0-9]{3}\b/gu, '<globalSessionRef>')
    .replaceAll(/\bS\d[a-z0-9]-P\d[a-z0-9]{3}-G\d[a-z0-9]{3}\b/gu, '<zmxName>')
    .replaceAll(/\bP\d[a-z0-9]{3}\b/gu, '<projectId>')
    .replaceAll(/\bG\d[a-z0-9]{3}\b/gu, '<sessionId>')
    .replaceAll(/\bS\d[a-z0-9]\b/gu, '<serverId>');
}

function normalizeToolStatus(tool) {
  return {
    capability: typeof tool.capability === 'string' ? tool.capability : '<missing>',
    tool: typeof tool.tool === 'string' ? tool.tool : '<missing>',
  };
}

async function updateOrCompareFixture(runOptions, observations) {
  const observedTsFixturePath = observedFixturePath(runOptions.suite);
  if (runOptions.updateFixtures) {
    if (runOptions.target !== 'ts') {
      throw new Error('--update-fixtures is only allowed with --target ts.');
    }
    await mkdir(fixturesDir, { recursive: true });
    await writeFile(
      observedTsFixturePath,
      `${JSON.stringify(
        {
          generatedAt: new Date().toISOString(),
          notes: [
            `Generated by gxserver-rs/compat/run-compat.mjs --target ts --suite ${runOptions.suite} --update-fixtures.`,
            'Dynamic fields are normalized so Rust can compare against the TypeScript contract.',
          ],
          schemaVersion: observations.schemaVersion,
          sourceTarget: 'typescript',
          suite: observations.suite,
          observations: observations.tests,
        },
        null,
        2
      )}\n`
    );
    console.log(`Updated ${observedTsFixturePath}`);
    return;
  }

  const fixture = JSON.parse(await readFile(observedTsFixturePath, 'utf8'));
  assert.deepEqual(observations.tests, fixture.observations);
}

function observedFixturePath(suite) {
  return path.join(fixturesDir, `${suite}-observed-ts.json`);
}
