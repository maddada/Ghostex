import { describe, expect, test } from 'bun:test';

import {
  disconnectRemoteCodeView,
  finishRemoteCodeViewLaunch,
  planRemotePromptEditorShortcut,
  resolveRemotePromptEditorRequest,
  selectRemoteCodeViewTarget,
  type RemoteCodeViewTarget,
} from './remote-code-view';

const target = (projectId: string, projectPath: string, connectionGeneration = 7): RemoteCodeViewTarget => ({
  kind: 'remote',
  machineId: 'build-box',
  projectId,
  projectPath,
  connectionGeneration,
  componentPlatform: 'linux-arm64',
});

describe('remote Code view lifecycle', () => {
  test('keeps the exact remote folder and reuses one connection across projects', () => {
    const first = target('one', '/srv/work/one');
    const launching = selectRemoteCodeViewTarget({ status: 'idle' }, first).state;
    const ready = finishRemoteCodeViewLaunch(launching, first, {
      ok: true,
      runtimeOrigin: 'http://127.0.0.1:43123',
      promptEditorIpcReady: true,
    });
    const second = target('two', '/srv/Other Project/two');
    const switched = selectRemoteCodeViewTarget(ready, second);

    expect(switched.cleanup).toBe(false);
    expect(switched.launch).toBeNull();
    expect(switched.state).toEqual({ ...ready, target: second });
  });

  test('cleans up and relaunches for a new connection generation', () => {
    const first = target('one', '/srv/work/one');
    const current = {
      status: 'ready' as const,
      target: first,
      runtimeOrigin: 'http://127.0.0.1:43123',
      promptEditorIpcReady: true,
    };
    const reconnected = selectRemoteCodeViewTarget(current, target('one', '/srv/work/one', 8));

    expect(reconnected.cleanup).toBe(true);
    expect(reconnected.launch?.target.connectionGeneration).toBe(8);
  });

  test('ignores stale launch results and disconnects only the owning generation', () => {
    const oldTarget = target('one', '/srv/work/one');
    const newTarget = target('one', '/srv/work/one', 8);
    const current = selectRemoteCodeViewTarget({ status: 'launching', target: oldTarget }, newTarget).state;

    expect(
      finishRemoteCodeViewLaunch(current, oldTarget, {
        ok: false,
        error: 'stale',
      })
    ).toBe(current);
    expect(disconnectRemoteCodeView(current, 'build-box', 7).cleanup).toBe(false);
    expect(disconnectRemoteCodeView(current, 'build-box', 8)).toEqual({
      cleanup: true,
      launch: null,
      state: { status: 'idle' },
    });
  });

  test('queues remote Ctrl+G until the exact runtime owns ready IPC', () => {
    const codeView = target('one', '/srv/Remote Project/one');
    const request = {
      codeView,
      sessionId: 'G7abc',
    };
    const plan = planRemotePromptEditorShortcut({ status: 'idle' }, request);

    expect(plan).toMatchObject({
      activateCodeView: true,
      attachCapability: 'code-server',
      delivery: { status: 'waiting', request },
      deliverControlG: false,
      target: { codeView, sessionId: 'G7abc' },
    });
    expect(plan.codeView.launch?.target).toBe(codeView);

    const ready = finishRemoteCodeViewLaunch(plan.codeView.state, codeView, {
      ok: true,
      runtimeOrigin: 'http://127.0.0.1:43123',
      promptEditorIpcReady: true,
    });
    expect(resolveRemotePromptEditorRequest(ready, request, request)).toEqual({
      status: 'deliver',
      request,
    });
  });

  test('cancels the queued shortcut on reconnect, project switch, close, or runtime failure', () => {
    const request = {
      codeView: target('one', '/srv/work/one'),
      sessionId: 'G7abc',
    };
    const failed = {
      status: 'failed' as const,
      target: request.codeView,
      error: 'launch failed',
    };

    expect(resolveRemotePromptEditorRequest(failed, request, request).status).toBe('cancelled');
    expect(resolveRemotePromptEditorRequest({ status: 'idle' }, request, request)).toMatchObject({
      status: 'cancelled',
      reason: 'stale-runtime',
    });
    expect(resolveRemotePromptEditorRequest(failed, request, null)).toMatchObject({
      status: 'cancelled',
      reason: 'session-closed',
    });
    expect(
      resolveRemotePromptEditorRequest(failed, request, {
        ...request,
        codeView: target('one', '/srv/work/one', 8),
      })
    ).toMatchObject({ status: 'cancelled', reason: 'stale-request' });
    expect(
      resolveRemotePromptEditorRequest(failed, request, {
        ...request,
        codeView: target('two', '/srv/work/two'),
      })
    ).toMatchObject({ status: 'cancelled', reason: 'stale-request' });
  });

  test('does not mark a runtime ready without IPC ownership', () => {
    const codeView = target('one', '/srv/work/one');
    const launching = selectRemoteCodeViewTarget({ status: 'idle' }, codeView).state;

    expect(
      finishRemoteCodeViewLaunch(launching, codeView, {
        ok: true,
        runtimeOrigin: '   ',
        promptEditorIpcReady: true,
      })
    ).toEqual({
      status: 'failed',
      target: codeView,
      error: 'Remote Code runtime did not claim IPC ownership.',
    });
    expect(
      finishRemoteCodeViewLaunch(launching, codeView, {
        ok: true,
        runtimeOrigin: 'http://127.0.0.1:43123',
        promptEditorIpcReady: false,
      })
    ).toEqual({
      status: 'failed',
      target: codeView,
      error: 'Remote Code runtime did not claim IPC ownership.',
    });
    expect(
      resolveRemotePromptEditorRequest(
        {
          status: 'ready',
          target: codeView,
          runtimeOrigin: 'http://127.0.0.1:43123',
          promptEditorIpcReady: false,
        },
        { codeView, sessionId: 'G7abc' },
        { codeView, sessionId: 'G7abc' }
      )
    ).toMatchObject({ status: 'cancelled', reason: 'stale-runtime' });
  });
});
