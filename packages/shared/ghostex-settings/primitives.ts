import { type ghostexSettings } from "./types";

export function clampNumber(value: number, min: number, max: number, fallback: number): number {
  if (!Number.isFinite(value)) {
    return fallback;
  }

  return Math.min(max, Math.max(min, value));
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

export function readBoolean(
  source: Record<string, unknown>,
  key: keyof ghostexSettings,
  fallback: boolean,
): boolean {
  const value = source[key];
  return typeof value === "boolean" ? value : fallback;
}

export function readNumber(
  source: Record<string, unknown>,
  key: keyof ghostexSettings,
  fallback: number,
): number {
  const value = source[key];
  return typeof value === "number" ? value : fallback;
}

export function readString(
  source: Record<string, unknown>,
  key: keyof ghostexSettings,
  fallback: string,
): string {
  const value = source[key];
  return typeof value === "string" ? value : fallback;
}

export function readLooseString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}
