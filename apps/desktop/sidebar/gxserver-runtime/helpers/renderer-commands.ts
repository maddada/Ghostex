/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_RENDERER_COMMAND_RENAME_TITLE_CONTROL_PATTERN,
  GPUI_RENDERER_COMMAND_RENAME_TITLE_MAX_CHARS,
} from '../constants';
import type { GpuiRendererCommandHandler } from '../types-and-protocol';
import { isObjectRecord, readGpuiRecordString } from './records';
import type { GxserverRendererCommand } from '@/packages/shared/gxserver-protocol';

export async function handleGpuiRendererCommand(
  socket: WebSocket,
  command: GxserverRendererCommand,
  handler: GpuiRendererCommandHandler
): Promise<void> {
  try {
    const result = await handler(command);
    socket.send(
      JSON.stringify({
        commandId: command.commandId,
        ok: true,
        result: isObjectRecord(result) ? result : { ok: true },
        type: 'rendererCommandResult',
      })
    );
  } catch (error) {
    socket.send(
      JSON.stringify({
        commandId: command.commandId,
        error: safeGpuiRendererCommandErrorMessage(error),
        ok: false,
        type: 'rendererCommandResult',
      })
    );
  }
}

export function isGpuiRendererCommand(value: unknown): value is GxserverRendererCommand {
  if (!isObjectRecord(value)) {
    return false;
  }
  return typeof value.action === 'string' && typeof value.commandId === 'string' && isObjectRecord(value.payload);
}

export function safeGpuiRendererCommandErrorMessage(error: unknown): string {
  if (!(error instanceof Error)) {
    return 'Renderer command failed.';
  }
  if (
    error.message === 'Invalid renderer command title.' ||
    error.message === 'No matching project was found.' ||
    error.message === 'No matching session was found.' ||
    error.message === 'Renderer command bridge unavailable.' ||
    error.message === 'Unsupported renderer command.'
  ) {
    return error.message;
  }
  return 'Renderer command failed.';
}

export function normalizeGpuiRendererCommandRenameTitle(payload: Record<string, unknown>): string | undefined {
  const rawTitle = readGpuiRecordString(payload, 'title');
  if (rawTitle === undefined || GPUI_RENDERER_COMMAND_RENAME_TITLE_CONTROL_PATTERN.test(rawTitle)) {
    return undefined;
  }
  const title = rawTitle.trim();
  if (!title || title.length > GPUI_RENDERER_COMMAND_RENAME_TITLE_MAX_CHARS) {
    return undefined;
  }
  return title;
}

export function readGpuiRendererCommandSessionTarget(
  payload: Record<string, unknown>
): Record<string, unknown> | undefined {
  const target = payload.sessionTarget;
  return isObjectRecord(target) && !Array.isArray(target) ? target : undefined;
}

export function parseGpuiRendererCommandGlobalSessionRef(
  globalRef: string | undefined
): { projectId: string; sessionId: string } | undefined {
  const parts = globalRef?.trim().split(':');
  if (parts?.length !== 3 || !parts[1] || !parts[2]) {
    return undefined;
  }
  return {
    projectId: parts[1],
    sessionId: parts[2],
  };
}
