/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import type { GpuiGxserverRpcSuccess } from '../types-and-protocol';
import type { GxserverRpcErrorCode } from '@/packages/shared/gxserver-protocol';

export function uniqueNonEmptyStrings(values: readonly unknown[] | undefined): readonly string[] | undefined {
  if (!Array.isArray(values)) {
    return undefined;
  }
  return [
    ...new Set(
      values.flatMap((value) => {
        const normalized = typeof value === 'string' ? normalizeNonEmptyString(value) : undefined;
        return normalized ? [normalized] : [];
      })
    ),
  ];
}

export function sameStringSet(left: ReadonlySet<string>, right: ReadonlySet<string>): boolean {
  if (left.size !== right.size) {
    return false;
  }
  for (const value of left) {
    if (!right.has(value)) {
      return false;
    }
  }
  return true;
}

export function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function stringFromRecord(record: Record<string, unknown> | undefined, key: string): string | undefined {
  const value = record?.[key];
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : undefined;
}

export function booleanFromRecord(record: Record<string, unknown> | undefined, key: string): boolean | undefined {
  const value = record?.[key];
  return typeof value === 'boolean' ? value : undefined;
}

export function optionalStringField<TKey extends string>(
  key: TKey,
  value: string | undefined
): Partial<Record<TKey, string>> {
  return value ? ({ [key]: value } as Partial<Record<TKey, string>>) : {};
}

export function optionalNumberField<TKey extends string>(
  key: TKey,
  value: number | undefined
): Partial<Record<TKey, number>> {
  return value !== undefined ? ({ [key]: value } as Partial<Record<TKey, number>>) : {};
}

export function normalizeNonEmptyString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim().length > 0 ? value : undefined;
}

export function delayGpuiAgentPromptStep(delayMs: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, delayMs);
  });
}

export async function readJson(response: Response): Promise<unknown> {
  const text = await response.text();
  return text.trim() ? (JSON.parse(text) as unknown) : undefined;
}

export function isGxserverRpcSuccess<TResult>(value: unknown): value is GpuiGxserverRpcSuccess<TResult> {
  return (
    typeof value === 'object' &&
    value !== null &&
    (value as Partial<GpuiGxserverRpcSuccess<TResult>>).ok === true &&
    (value as Partial<GpuiGxserverRpcSuccess<TResult>>).product === 'gxserver' &&
    'result' in value
  );
}

export function gpuiGxserverRpcErrorMessage(value: unknown): string | undefined {
  /*
  CDXC:ServerDaemon 2026-07-11-05:56:
  gxserver domain endpoints return an intentionally user-facing `message` in
  their bounded RPC error envelope. The GPUI-local client must preserve that
  field just like the shared native client does; replacing it with a generic
  transport error hides actionable Git/generation failures and forces blind
  retries. Accept only a bounded plain string from an explicit failed envelope.
  */
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (record.ok !== false || typeof record.message !== 'string') {
    return undefined;
  }
  const message = record.message
    .replace(/[\u0000-\u001f\u007f-\u009f]+/gu, ' ')
    .replace(/\s+/gu, ' ')
    .trim()
    .slice(0, 500);
  return message || undefined;
}

export function gpuiGxserverRpcErrorCode(value: unknown): GxserverRpcErrorCode | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  return typeof record.error === 'string' ? (record.error as GxserverRpcErrorCode) : undefined;
}

export function parseObject(value: unknown): Record<string, unknown> | undefined {
  try {
    const parsed = typeof value === 'string' ? (JSON.parse(value) as unknown) : value;
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : undefined;
  } catch {
    return undefined;
  }
}

export function readGpuiRecordString(record: Record<string, unknown> | undefined, key: string): string | undefined {
  const value = record?.[key];
  return typeof value === 'string' ? value : undefined;
}
