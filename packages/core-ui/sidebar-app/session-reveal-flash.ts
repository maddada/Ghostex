const pendingFlashes = new WeakMap<HTMLElement, () => void>();

/**
 * CDXC:Sessions 2026-09-09 DECISION:
 * User: revealing a session with the titlebar button should flash its outline twice, like blink blink.
 * Wait for the reveal scroll to settle so both flashes are visible.
 */
export function flashRevealedSession(row: HTMLElement): void {
  pendingFlashes.get(row)?.();
  const ancestors: HTMLElement[] = [];
  for (let ancestor = row.parentElement; ancestor; ancestor = ancestor.parentElement) {
    ancestors.push(ancestor);
  }
  let positions = ancestors.map((ancestor) => ancestor.scrollTop);
  let stableFrames = 0;
  let frameId = 0;
  let flash: Animation | undefined;
  const cancel = () => {
    window.cancelAnimationFrame(frameId);
    flash?.cancel();
    pendingFlashes.delete(row);
  };
  pendingFlashes.set(row, cancel);

  const waitForScroll = () => {
    if (!row.isConnected) {
      cancel();
      return;
    }
    const nextPositions = ancestors.map((ancestor) => ancestor.scrollTop);
    stableFrames = nextPositions.every((position, index) => position === positions[index]) ? stableFrames + 1 : 0;
    positions = nextPositions;
    if (stableFrames < 3) {
      frameId = window.requestAnimationFrame(waitForScroll);
      return;
    }
    flash = row.animate(
      [
        { outlineColor: 'transparent', offset: 0 },
        { outlineColor: 'var(--session-focus-visible-outline)', offset: 0.2 },
        { outlineColor: 'var(--session-focus-visible-outline)', offset: 0.55 },
        { outlineColor: 'transparent', offset: 1 },
      ],
      { duration: 550, iterations: 2, easing: 'ease-in-out' }
    );
    flash.onfinish = () => pendingFlashes.delete(row);
  };
  frameId = window.requestAnimationFrame(waitForScroll);
}
