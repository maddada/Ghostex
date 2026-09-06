import { useEffect, useRef, useState } from 'react';

export type PickerDirection = 'ArrowUp' | 'ArrowDown' | 'ArrowLeft' | 'ArrowRight';
export type PickerControl = PickerDirection | 'Enter' | 'Escape';

const controls: Record<string, PickerControl> = {
  ArrowUp: 'ArrowUp',
  ArrowDown: 'ArrowDown',
  ArrowLeft: 'ArrowLeft',
  ArrowRight: 'ArrowRight',
  Enter: 'Enter',
  Escape: 'Escape',
  h: 'ArrowLeft',
  j: 'ArrowDown',
  k: 'ArrowUp',
  l: 'ArrowRight',
  ',': 'ArrowLeft',
  '.': 'ArrowRight',
};

/**
 * CDXC:SessionChat 2026-09-05 DECISION:
 * User: support H/J/K/L alongside arrows and light up the matching footer key when used.
 * Additional effort shortcuts remain unadvertised. Modifier chords belong to the application's configured hotkeys.
 */
export function modelPickerControlForKey(event: KeyboardEvent): PickerControl | undefined {
  if (event.isComposing || event.altKey || event.ctrlKey || event.metaKey) return;
  return controls[event.key] ?? controls[event.key.toLowerCase()];
}

export function useModelPickerKeyFeedback() {
  const [pressed, setPressed] = useState<ReadonlySet<PickerControl>>(() => new Set());
  const held = useRef(new Map<string, PickerControl>());
  const releases = useRef(new Map<PickerControl, ReturnType<typeof setTimeout>>());

  const releaseAfterDelay = (control: PickerControl) => {
    clearTimeout(releases.current.get(control));
    releases.current.set(
      control,
      setTimeout(() => {
        releases.current.delete(control);
        setPressed((current) => {
          const next = new Set(current);
          next.delete(control);
          return next;
        });
      }, 160)
    );
  };

  useEffect(() => {
    const release = (event: KeyboardEvent) => {
      const key = event.code || event.key;
      const control = held.current.get(key);
      held.current.delete(key);
      if (!control || [...held.current.values()].includes(control)) return;
      // Keep a quick tap visible; held keys stay lit until their keyup arrives.
      releaseAfterDelay(control);
    };
    const reset = () => {
      held.current.clear();
      for (const timer of releases.current.values()) clearTimeout(timer);
      releases.current.clear();
    };
    const blur = () => {
      reset();
      setPressed(new Set());
    };
    window.addEventListener('keyup', release, true);
    window.addEventListener('blur', blur);
    return () => {
      window.removeEventListener('keyup', release, true);
      window.removeEventListener('blur', blur);
      reset();
    };
  }, []);

  const pulse = (control: PickerControl) => {
    clearTimeout(releases.current.get(control));
    releases.current.delete(control);
    setPressed((current) => (current.has(control) ? current : new Set([...current, control])));
    if (![...held.current.values()].includes(control)) releaseAfterDelay(control);
  };
  const press = (event: KeyboardEvent, control: PickerControl) => {
    held.current.set(event.code || event.key, control);
    pulse(control);
  };

  return { pressed, press, pulse };
}

/**
 * CDXC:SessionChat 2026-09-05 DECISION:
 * User: trackpad scrolling navigates models vertically and effort horizontally.
 * CDXC:SessionChat 2026-09-05 WHY:
 * A gesture advances one choice; momentum from that gesture must not race through the remaining cards.
 */
export function useModelPickerWheelNavigation(
  element: HTMLElement | null,
  navigate: (direction: PickerDirection) => void
) {
  const latest = useRef(navigate);
  latest.current = navigate;

  useEffect(() => {
    if (!element) return;
    let lastEvent = 0;
    let x = 0;
    let y = 0;
    let fired: { horizontal: boolean; sign: number } | null = null;
    const wheel = (event: WheelEvent) => {
      if (event.ctrlKey || event.metaKey) return;
      event.preventDefault();
      event.stopPropagation();
      const unit = event.deltaMode === 1 ? 20 : event.deltaMode === 2 ? element.clientHeight : 1;
      const dx = (event.shiftKey && !event.deltaX ? event.deltaY : event.deltaX) * unit;
      const dy = (event.shiftKey ? 0 : event.deltaY) * unit;
      if (!dx && !dy) return;
      const now = performance.now();
      const reversed = fired && Math.sign(fired.horizontal ? dx : dy) === -fired.sign;
      if (now - lastEvent > 180 || reversed) {
        x = 0;
        y = 0;
        fired = null;
      }
      lastEvent = now;
      if (fired) return;
      x += dx;
      y += dy;
      if (Math.max(Math.abs(x), Math.abs(y)) < 28) return;
      const horizontal = Math.abs(x) > Math.abs(y);
      const sign = Math.sign(horizontal ? x : y);
      fired = { horizontal, sign };
      latest.current(horizontal ? (sign > 0 ? 'ArrowRight' : 'ArrowLeft') : sign > 0 ? 'ArrowDown' : 'ArrowUp');
    };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => element.removeEventListener('wheel', wheel);
  }, [element]);
}
