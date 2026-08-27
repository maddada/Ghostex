import {
  forwardRef,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type CSSProperties,
  type Ref,
} from 'react';
import { createPortal } from 'react-dom';
import { TOOLTIP_MOTION_CLASS_NAME } from '../components/ui/tooltip-config';
import { cn } from '@/packages/components/utils';
import {
  areSidebarTooltipsSuppressed,
  SIDEBAR_TOOLTIP_DISMISS_EVENT,
  SIDEBAR_TOOLTIP_SUPPRESSION_CHANGED_EVENT,
} from './app-tooltip';
import { SIDEBAR_FIXED_TOOLTIP_DELAY_OFFSET_MS, useSidebarTooltipDelayMs } from './tooltip-delay';

const SIDEBAR_FIXED_TOOLTIP_VIEWPORT_MARGIN_PX = 8;
const SIDEBAR_FIXED_TOOLTIP_TRIGGER_OFFSET_PX = 8;

type SidebarFixedTooltipSide = 'bottom' | 'left' | 'right' | 'top';
type SidebarFixedTooltipAlign = 'center' | 'end' | 'start';

type SidebarFixedTooltipRect = {
  bottom: number;
  height: number;
  left: number;
  right: number;
  top: number;
  width: number;
};

export type SidebarFixedTooltipPosition = {
  left: number;
  maxWidth: number;
  side: SidebarFixedTooltipSide;
  top: number;
};

type SidebarFixedTooltipPositionInput = {
  align?: SidebarFixedTooltipAlign;
  margin?: number;
  offset?: number;
  preferredSide?: SidebarFixedTooltipSide;
  tooltipRect: SidebarFixedTooltipRect;
  triggerRect: SidebarFixedTooltipRect;
  viewportHeight: number;
  viewportWidth: number;
};

type SidebarFixedTooltipButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  tooltip: string;
  tooltipAlign?: SidebarFixedTooltipAlign;
  tooltipSide?: SidebarFixedTooltipSide;
};

let activeSidebarFixedTooltipId: symbol | undefined;
let activeSidebarFixedTooltipClose: (() => void) | undefined;

function assignSidebarFixedTooltipButtonRef(
  ref: Ref<HTMLButtonElement> | undefined,
  value: HTMLButtonElement | null
): void {
  if (!ref) {
    return;
  }

  if (typeof ref === 'function') {
    ref(value);
    return;
  }

  ref.current = value;
}

function clamp(value: number, min: number, max: number): number {
  if (max < min) {
    return min;
  }

  return Math.max(min, Math.min(value, max));
}

function getAvailableSpace({
  margin,
  offset,
  triggerRect,
  viewportHeight,
  viewportWidth,
}: {
  margin: number;
  offset: number;
  triggerRect: SidebarFixedTooltipRect;
  viewportHeight: number;
  viewportWidth: number;
}): Record<SidebarFixedTooltipSide, number> {
  return {
    bottom: viewportHeight - margin - triggerRect.bottom - offset,
    left: triggerRect.left - margin - offset,
    right: viewportWidth - margin - triggerRect.right - offset,
    top: triggerRect.top - margin - offset,
  };
}

function getCandidateSides(preferredSide: SidebarFixedTooltipSide): SidebarFixedTooltipSide[] {
  switch (preferredSide) {
    case 'left':
      return ['left', 'right', 'bottom', 'top'];
    case 'right':
      return ['right', 'left', 'bottom', 'top'];
    case 'top':
      return ['top', 'bottom', 'right', 'left'];
    case 'bottom':
    default:
      return ['bottom', 'top', 'right', 'left'];
  }
}

function getRequiredSpace(side: SidebarFixedTooltipSide, tooltipHeight: number, tooltipWidth: number): number {
  return side === 'left' || side === 'right' ? tooltipWidth : tooltipHeight;
}

function getResolvedTooltipSide({
  availableSpace,
  preferredSide,
  tooltipHeight,
  tooltipWidth,
}: {
  availableSpace: Record<SidebarFixedTooltipSide, number>;
  preferredSide: SidebarFixedTooltipSide;
  tooltipHeight: number;
  tooltipWidth: number;
}): SidebarFixedTooltipSide {
  const candidates = getCandidateSides(preferredSide);
  const fittingSide = candidates.find(
    (side) => getRequiredSpace(side, tooltipHeight, tooltipWidth) <= availableSpace[side]
  );
  if (fittingSide) {
    return fittingSide;
  }

  return candidates.reduce((bestSide, side) => (availableSpace[side] > availableSpace[bestSide] ? side : bestSide));
}

export function getSidebarFixedTooltipPosition({
  align = 'center',
  margin = SIDEBAR_FIXED_TOOLTIP_VIEWPORT_MARGIN_PX,
  offset = SIDEBAR_FIXED_TOOLTIP_TRIGGER_OFFSET_PX,
  preferredSide = 'bottom',
  tooltipRect,
  triggerRect,
  viewportHeight,
  viewportWidth,
}: SidebarFixedTooltipPositionInput): SidebarFixedTooltipPosition {
  const maxWidth = Math.max(0, viewportWidth - margin * 2);
  const tooltipWidth = Math.min(tooltipRect.width, maxWidth);
  const tooltipHeight = tooltipRect.height;
  const triggerCenterX = triggerRect.left + triggerRect.width / 2;
  const triggerCenterY = triggerRect.top + triggerRect.height / 2;
  const availableSpace = getAvailableSpace({
    margin,
    offset,
    triggerRect,
    viewportHeight,
    viewportWidth,
  });
  const side = getResolvedTooltipSide({
    availableSpace,
    preferredSide,
    tooltipHeight,
    tooltipWidth,
  });

  if (side === 'left' || side === 'right') {
    const preferredLeft = side === 'left' ? triggerRect.left - offset - tooltipWidth : triggerRect.right + offset;
    return {
      left: clamp(preferredLeft, margin, viewportWidth - margin - tooltipWidth),
      maxWidth,
      side,
      top: clamp(triggerCenterY - tooltipHeight / 2, margin, viewportHeight - margin - tooltipHeight),
    };
  }

  const preferredLeft =
    align === 'start'
      ? triggerRect.left
      : align === 'end'
        ? triggerRect.right - tooltipWidth
        : triggerCenterX - tooltipWidth / 2;

  return {
    left: clamp(preferredLeft, margin, viewportWidth - margin - tooltipWidth),
    maxWidth,
    side,
    top:
      side === 'top'
        ? clamp(triggerRect.top - offset - tooltipHeight, margin, viewportHeight - margin - tooltipHeight)
        : clamp(triggerRect.bottom + offset, margin, viewportHeight - margin - tooltipHeight),
  };
}

/**
 * CDXC:SidebarTooltips 2026-06-25-15:48:
 * Sidebar action tooltips must render through a fixed document-body popup instead of CSS pseudo-elements so scroll masks, the Recent Projects footer boundary, and section overflow cannot clip the label.
 * Resolve the popup side from measured viewport space so the last remote-machine or project header action can flip above its trigger while normal actions still prefer the requested side.
 */
export const SidebarFixedTooltipButton = forwardRef<HTMLButtonElement, SidebarFixedTooltipButtonProps>(
  function SidebarFixedTooltipButton(
    {
      children,
      disabled,
      onBlur,
      onFocus,
      onMouseEnter,
      onMouseLeave,
      tooltip,
      tooltipAlign = 'center',
      tooltipSide = 'bottom',
      ...buttonProps
    },
    forwardedRef
  ) {
    const buttonRef = useRef<HTMLButtonElement>(null);
    const disabledRef = useRef(disabled);
    const tooltipRef = useRef<HTMLDivElement>(null);
    const tooltipTextRef = useRef(tooltip);
    const tooltipId = useId();
    const instanceIdRef = useRef(Symbol('sidebarFixedTooltip'));
    const openTimeoutIdRef = useRef<number | undefined>(undefined);
    const tooltipDelayMs = useSidebarTooltipDelayMs(SIDEBAR_FIXED_TOOLTIP_DELAY_OFFSET_MS);
    const [isTooltipOpen, setIsTooltipOpen] = useState(false);
    const [tooltipPosition, setTooltipPosition] = useState<SidebarFixedTooltipPosition>();
    disabledRef.current = disabled;
    tooltipTextRef.current = tooltip;

    const setButtonRef = (button: HTMLButtonElement | null) => {
      buttonRef.current = button;
      assignSidebarFixedTooltipButtonRef(forwardedRef, button);
    };

    const clearOpenTimeout = () => {
      if (openTimeoutIdRef.current === undefined) {
        return;
      }
      window.clearTimeout(openTimeoutIdRef.current);
      openTimeoutIdRef.current = undefined;
    };

    const closeTooltip = () => {
      clearOpenTimeout();
      if (activeSidebarFixedTooltipId === instanceIdRef.current) {
        activeSidebarFixedTooltipId = undefined;
        activeSidebarFixedTooltipClose = undefined;
      }
      setIsTooltipOpen(false);
      setTooltipPosition(undefined);
    };

    const openTooltip = ({ delayed }: { delayed: boolean }) => {
      clearOpenTimeout();
      if (disabled || !tooltip || areSidebarTooltipsSuppressed()) {
        closeTooltip();
        return;
      }

      const commitOpen = () => {
        if (disabledRef.current || !tooltipTextRef.current || areSidebarTooltipsSuppressed()) {
          closeTooltip();
          return;
        }
        if (activeSidebarFixedTooltipId !== instanceIdRef.current) {
          activeSidebarFixedTooltipClose?.();
        }
        activeSidebarFixedTooltipId = instanceIdRef.current;
        activeSidebarFixedTooltipClose = closeTooltip;
        setIsTooltipOpen(true);
        openTimeoutIdRef.current = undefined;
      };

      if (delayed) {
        openTimeoutIdRef.current = window.setTimeout(commitOpen, tooltipDelayMs);
        return;
      }
      commitOpen();
    };

    useEffect(() => {
      const handleSidebarTooltipDismiss = () => closeTooltip();
      const handleSidebarTooltipSuppressionChanged = () => {
        if (areSidebarTooltipsSuppressed()) {
          closeTooltip();
        }
      };

      window.addEventListener(SIDEBAR_TOOLTIP_DISMISS_EVENT, handleSidebarTooltipDismiss);
      window.addEventListener(SIDEBAR_TOOLTIP_SUPPRESSION_CHANGED_EVENT, handleSidebarTooltipSuppressionChanged);

      return () => {
        window.removeEventListener(SIDEBAR_TOOLTIP_DISMISS_EVENT, handleSidebarTooltipDismiss);
        window.removeEventListener(SIDEBAR_TOOLTIP_SUPPRESSION_CHANGED_EVENT, handleSidebarTooltipSuppressionChanged);
        if (activeSidebarFixedTooltipId === instanceIdRef.current) {
          activeSidebarFixedTooltipId = undefined;
          activeSidebarFixedTooltipClose = undefined;
        }
        clearOpenTimeout();
      };
    }, []);

    useEffect(() => {
      clearOpenTimeout();
      if (disabled || !tooltip) {
        closeTooltip();
      }
    }, [disabled, tooltip]);

    useLayoutEffect(() => {
      if (!isTooltipOpen) {
        return undefined;
      }

      const updateTooltipPosition = () => {
        const button = buttonRef.current;
        const tooltipElement = tooltipRef.current;
        if (!button || !tooltipElement) {
          return;
        }

        const nextPosition = getSidebarFixedTooltipPosition({
          align: tooltipAlign,
          preferredSide: tooltipSide,
          tooltipRect: tooltipElement.getBoundingClientRect(),
          triggerRect: button.getBoundingClientRect(),
          viewportHeight: window.innerHeight,
          viewportWidth: window.innerWidth,
        });

        setTooltipPosition((previousPosition) => {
          if (
            previousPosition?.left === nextPosition.left &&
            previousPosition.maxWidth === nextPosition.maxWidth &&
            previousPosition.side === nextPosition.side &&
            previousPosition.top === nextPosition.top
          ) {
            return previousPosition;
          }

          return nextPosition;
        });
      };

      updateTooltipPosition();
      window.addEventListener('resize', updateTooltipPosition);
      window.addEventListener('scroll', updateTooltipPosition, true);

      const resizeObserver =
        typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(updateTooltipPosition);
      if (buttonRef.current) {
        resizeObserver?.observe(buttonRef.current);
      }
      if (tooltipRef.current) {
        resizeObserver?.observe(tooltipRef.current);
      }

      return () => {
        window.removeEventListener('resize', updateTooltipPosition);
        window.removeEventListener('scroll', updateTooltipPosition, true);
        resizeObserver?.disconnect();
      };
    }, [isTooltipOpen, tooltip, tooltipAlign, tooltipSide]);

    return (
      <>
        <button
          {...buttonProps}
          aria-describedby={isTooltipOpen ? tooltipId : buttonProps['aria-describedby']}
          disabled={disabled}
          onBlur={(event) => {
            onBlur?.(event);
            closeTooltip();
          }}
          onFocus={(event) => {
            onFocus?.(event);
            openTooltip({ delayed: false });
          }}
          onMouseEnter={(event) => {
            onMouseEnter?.(event);
            openTooltip({ delayed: true });
          }}
          onMouseLeave={(event) => {
            onMouseLeave?.(event);
            closeTooltip();
          }}
          ref={setButtonRef}
        >
          {children}
        </button>
        {isTooltipOpen && tooltip && typeof document !== 'undefined'
          ? createPortal(
              <div
                className={cn('sidebar-fixed-tooltip-popup', TOOLTIP_MOTION_CLASS_NAME)}
                data-side={tooltipPosition?.side ?? tooltipSide}
                data-state='delayed-open'
                id={tooltipId}
                ref={tooltipRef}
                role='tooltip'
                style={
                  {
                    '--sidebar-fixed-tooltip-left': tooltipPosition ? `${tooltipPosition.left}px` : '0px',
                    '--sidebar-fixed-tooltip-max-width': tooltipPosition
                      ? `${tooltipPosition.maxWidth}px`
                      : `calc(100vw - ${SIDEBAR_FIXED_TOOLTIP_VIEWPORT_MARGIN_PX * 2}px)`,
                    '--sidebar-fixed-tooltip-top': tooltipPosition ? `${tooltipPosition.top}px` : '0px',
                  } as CSSProperties
                }
              >
                {tooltip}
              </div>,
              document.body
            )
          : null}
      </>
    );
  }
);
