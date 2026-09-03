/**
 * CDXC:Navigation 2026-08-19:
 * The React half of the titlebar Back/Forward pair, used by every React titlebar
 * (the web app today). The gpui desktop titlebar is native GPUI and paints its
 * own pair from the same controller state pushed over its Rust bridge — see
 * `apps/desktop/src/navigation_history`.
 *
 * The buttons are genuinely `disabled` when the trail has nothing in that
 * direction, so keyboard and assistive tech agree with the dimmed pixels.
 *
 * They carry NO hover tooltip: the arrows are self-explanatory, and a tooltip on
 * a control this small and this frequently clicked is noise. The destination
 * text still reaches assistive tech through `aria-label`, which is silent for
 * everyone else.
 */

import { useSyncExternalStore } from 'react';
import type { NavigationHistoryController } from './navigation-history-controller';

const ARROW_LEFT_PATH = 'M15 6l-6 6 6 6';
const ARROW_RIGHT_PATH = 'M9 6l6 6-6 6';

function ChevronIcon({ path }: { path: string }) {
  return (
    <svg aria-hidden='true' viewBox='0 0 24 24'>
      <path d={path} />
    </svg>
  );
}

export type NavigationHistoryButtonsProps = {
  controller: NavigationHistoryController;
  /** Class applied to the pair's container. */
  className?: string;
  /** Class applied to each button, so hosts reuse their own titlebar button look. */
  buttonClassName?: string;
};

export function NavigationHistoryButtons({ buttonClassName, className, controller }: NavigationHistoryButtonsProps) {
  const state = useSyncExternalStore(controller.subscribe, controller.getState, controller.getState);

  return (
    <div className={className} data-navigation-history='buttons'>
      <button
        aria-label={state.backTooltip}
        className={buttonClassName}
        disabled={!state.canGoBack}
        onClick={() => void controller.navigate('back')}
        type='button'
      >
        <ChevronIcon path={ARROW_LEFT_PATH} />
      </button>
      <button
        aria-label={state.forwardTooltip}
        className={buttonClassName}
        disabled={!state.canGoForward}
        onClick={() => void controller.navigate('forward')}
        type='button'
      >
        <ChevronIcon path={ARROW_RIGHT_PATH} />
      </button>
    </div>
  );
}
