import { useEffect, useRef, useState, type CSSProperties, type RefObject } from 'react';

export const STAGE_WIDTH = 1672;
export const STAGE_HEIGHT = 941;
export const PANEL_COUNT = 5;
/** x of the hairline between the copy column and the preview column, per panel (`gr` in the prototype). */
export const PANEL_DIVIDER_X: readonly number[] = [759, 796, 727, 756, STAGE_WIDTH];
/** x of the Ghostex lockup and the footer, per panel (`h5` in the prototype). */
export const PANEL_LOCKUP_X: readonly number[] = [46, 48, 60, 46, 44];
export const VEIL_LEFT = Math.min(...PANEL_DIVIDER_X);
/**
 * CDXC:Onboarding 2026-09-12 DECISION:
 * User: "the bg behind the right side graphics is too blue please make it less saturated colors for the bg
 * graphic/shader". The prototype ran the DarkVeil shader at full saturation; this keeps its shape and motion
 * but pulls most of the colour out of it. 0 is greyscale, 1 is the shader's own colour.
 */
export const VEIL_SATURATION = 0.32;

export function box(left: number, top: number, width?: number, height?: number): CSSProperties {
  return { position: 'absolute', left, top, width, height };
}

export function prefersReducedMotion(): boolean {
  return typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

export function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(prefersReducedMotion);
  useEffect(() => {
    if (typeof matchMedia === 'undefined') return;
    const query = matchMedia('(prefers-reduced-motion: reduce)');
    const onChange = () => setReduced(query.matches);
    query.addEventListener('change', onChange);
    return () => query.removeEventListener('change', onChange);
  }, []);
  return reduced;
}

/** Steps 0..count-1 every `intervalMs`, then rests `rest` ticks on the last frame before wrapping (`Da`). */
export function useCycle(count: number, intervalMs: number, rest = 3): number {
  const reduced = useReducedMotion();
  const [tick, setTick] = useState(() => (prefersReducedMotion() ? count - 1 : 0));
  useEffect(() => {
    if (reduced) {
      setTick(count - 1);
      return;
    }
    const timer = setInterval(() => setTick((value) => (value + 1) % (count + rest)), intervalMs);
    return () => clearInterval(timer);
  }, [count, intervalMs, rest, reduced]);
  return Math.min(tick, count - 1);
}

/** Milliseconds since mount, wrapping at `loopMs`, sampled every 80 ms (`n5`, made to loop). */
export function useLoopClock(loopMs: number): number {
  const reduced = useReducedMotion();
  const [elapsed, setElapsed] = useState(() => (prefersReducedMotion() ? loopMs - 1 : 0));
  useEffect(() => {
    if (reduced) {
      setElapsed(loopMs - 1);
      return;
    }
    const startedAt = Date.now();
    const timer = setInterval(() => setElapsed((Date.now() - startedAt) % loopMs), 80);
    return () => clearInterval(timer);
  }, [loopMs, reduced]);
  return elapsed;
}

export function useNow(intervalMs = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(timer);
  }, [intervalMs]);
  return now;
}

export function useOutsideClick(ref: RefObject<HTMLElement | null>, onOutside: () => void, active: boolean): void {
  const callback = useRef(onOutside);
  callback.current = onOutside;
  useEffect(() => {
    if (!active) return;
    const onPointerDown = (event: PointerEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) callback.current();
    };
    document.addEventListener('pointerdown', onPointerDown);
    return () => document.removeEventListener('pointerdown', onPointerDown);
  }, [active, ref]);
}

export function clockStamp(date: Date): string {
  return date.toTimeString().slice(0, 8);
}

export function folderBasename(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, '');
  const parts = trimmed.split(/[\\/]/);
  return parts[parts.length - 1] || trimmed;
}
