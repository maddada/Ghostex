import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../components/ui/tooltip';
import { useEffect, useState, type ComponentProps, type ReactElement, type ReactNode } from 'react';
import { useConfiguredSidebarTooltipDelayMs } from './tooltip-delay';

export const SIDEBAR_TOOLTIP_DISMISS_EVENT = 'ghostex-sidebar-tooltip-dismiss';
export const SIDEBAR_TOOLTIP_SUPPRESSION_CHANGED_EVENT = 'ghostex-sidebar-tooltip-suppression-changed';

/**
 * CDXC:Tooltips 2026-09-15 DECISION:
 * User: no tooltip may show while a sidebar context menu is open; one used to open on top of the menu.
 * Suppression is keyed by reason so a drag and an open menu each hold it independently and it lifts only when the last reason clears.
 * It stays a temporary block for these two flows only; native pointer-leave keeps dismissing through the event so the next hover can open a tooltip.
 * SEE-ALSO: packages/core-ui/sidebar-context-menu-portal.tsx (deleted 2026-10-01), packages/core-ui/styles/group-panels.css (deleted 2026-11-06).
 */
export type SidebarTooltipSuppressionReason = 'drag' | 'contextMenu';

const sidebarTooltipSuppressionReasons = new Set<SidebarTooltipSuppressionReason>();

function setSidebarTooltipSuppressionBodyFlag(suppressed: boolean) {
  const body = typeof document === 'undefined' ? undefined : document.body;
  if (!body) {
    return;
  }
  if (suppressed) {
    body.dataset.sidebarTooltipsSuppressed = 'true';
    return;
  }
  delete body.dataset.sidebarTooltipsSuppressed;
}

export function dismissSidebarTooltips() {
  window.dispatchEvent(new Event(SIDEBAR_TOOLTIP_DISMISS_EVENT));
}

/*
 * CDXC:Tooltips 2026-07-23:
 * Scrolling any sidebar scroll area must dismiss visible tooltips immediately:
 * a session row can scroll out from under an open tooltip, leaving the tooltip
 * floating over unrelated rows. Scroll events do not bubble, so a capture-phase
 * window listener is the one place that sees every scroller (main sidebar,
 * inner project session lists, modal bodies). The dismiss dispatch is
 * rate-limited so momentum scrolling does not spam every mounted tooltip's
 * listener each frame; the shared tooltip open delay (300ms) is far above
 * the limit, so nothing can open and survive between dismissals mid-scroll.
 */
const SCROLL_DISMISS_MIN_INTERVAL_MS = 100;

export function useDismissSidebarTooltipsOnScroll() {
  useEffect(() => {
    let lastDismissAt = 0;
    const handleScroll = () => {
      const now = Date.now();
      if (now - lastDismissAt < SCROLL_DISMISS_MIN_INTERVAL_MS) {
        return;
      }
      lastDismissAt = now;
      dismissSidebarTooltips();
    };
    window.addEventListener('scroll', handleScroll, { capture: true, passive: true });
    return () => {
      window.removeEventListener('scroll', handleScroll, { capture: true });
    };
  }, []);
}

export function areSidebarTooltipsSuppressed() {
  return sidebarTooltipSuppressionReasons.size > 0;
}

export function setSidebarTooltipsSuppressed(reason: SidebarTooltipSuppressionReason, suppressed: boolean) {
  const wasSuppressed = areSidebarTooltipsSuppressed();
  if (suppressed) {
    sidebarTooltipSuppressionReasons.add(reason);
  } else {
    sidebarTooltipSuppressionReasons.delete(reason);
  }
  const isSuppressed = areSidebarTooltipsSuppressed();
  setSidebarTooltipSuppressionBodyFlag(isSuppressed);
  if (wasSuppressed === isSuppressed) {
    return;
  }
  /*
   * CDXC:Tooltips 2026-06-02-20:22:
   * Sidebar project/session drag should not spawn hover tooltips under the pointer. Suppress both Radix and local session title tooltips for the duration of sidebar drag operations, and close any tooltip that was already open when the drag started.
   */
  if (isSuppressed) {
    dismissSidebarTooltips();
  }
  window.dispatchEvent(new Event(SIDEBAR_TOOLTIP_SUPPRESSION_CHANGED_EVENT));
}

export function setSidebarTooltipsSuppressedForDrag(suppressed: boolean) {
  setSidebarTooltipsSuppressed('drag', suppressed);
}

type AppTooltipProps = ComponentProps<typeof Tooltip> & {
  align?: ComponentProps<typeof TooltipContent>['align'];
  alignOffset?: ComponentProps<typeof TooltipContent>['alignOffset'];
  anchor?: ComponentProps<typeof TooltipContent>['anchor'];
  children: ReactElement;
  collisionPadding?: ComponentProps<typeof TooltipContent>['collisionPadding'];
  content: ReactNode;
  contentClassName?: string;
  delay?: ComponentProps<typeof TooltipTrigger>['delay'];
  side?: ComponentProps<typeof TooltipContent>['side'];
  contentStyle?: ComponentProps<typeof TooltipContent>['style'];
  sideOffset?: number;
};

/**
 * CDXC:Tooltips 2026-05-06-18:58
 * User-facing tooltips must render through the shadcn/Radix tooltip instead of
 * native title attributes. Action tooltip copy should describe the action
 * directly and omit project or group names when the surrounding UI already
 * supplies that context.
 */
export function AppTooltip({
  align,
  alignOffset,
  anchor,
  children,
  collisionPadding,
  content,
  contentClassName,
  delay,
  side,
  contentStyle,
  sideOffset = 8,
  ...tooltipProps
}: AppTooltipProps) {
  const sidebarDelayMs = useConfiguredSidebarTooltipDelayMs();
  const { defaultOpen, onOpenChange, open: controlledOpen, ...tooltipRootProps } = tooltipProps;
  const isControlled = controlledOpen !== undefined;
  const [uncontrolledOpen, setUncontrolledOpen] = useState(Boolean(defaultOpen));
  const open = isControlled ? controlledOpen : uncontrolledOpen;

  const setOpen = (nextOpen: boolean) => {
    if (nextOpen && areSidebarTooltipsSuppressed()) {
      return;
    }
    if (!isControlled) {
      setUncontrolledOpen(nextOpen);
    }
    onOpenChange?.(nextOpen);
  };

  useEffect(() => {
    const handleDismiss = () => setOpen(false);
    const handleSuppressionChanged = () => {
      if (areSidebarTooltipsSuppressed()) {
        setOpen(false);
      }
    };
    window.addEventListener(SIDEBAR_TOOLTIP_DISMISS_EVENT, handleDismiss);
    window.addEventListener(SIDEBAR_TOOLTIP_SUPPRESSION_CHANGED_EVENT, handleSuppressionChanged);
    return () => {
      window.removeEventListener(SIDEBAR_TOOLTIP_DISMISS_EVENT, handleDismiss);
      window.removeEventListener(SIDEBAR_TOOLTIP_SUPPRESSION_CHANGED_EVENT, handleSuppressionChanged);
    };
  });

  if (content === undefined || content === null || content === '') {
    return children;
  }

  /*
   * CDXC:Tooltips 2026-05-25-07:16:
   * Native sidebar tooltips must disappear when the sidebar stops owning pointer
   * hover because WKWebView can miss normal trigger leave events during app
   * switching, external clicks, or fast exits into another native surface. Keep
   * AppTooltip controllable through a shared dismiss event so all Radix tooltip
   * instances close immediately and stay closed until the trigger opens again.
   *
   * CDXC:Tooltips 2026-06-13-02:59:
   * The macOS titlebar uses the same AppTooltip wrapper as the sidebar, but its
   * compact chrome sometimes needs side-positioned labels. Forward side to
   * TooltipContent without changing the sidebar's default bottom placement.
   *
   * CDXC:Tooltips 2026-06-15-16:40:
   * Titlebar button hover labels need a small vertical alignment nudge while
   * preserving the shared tooltip primitive. Forward alignOffset so the
   * titlebar wrapper can adjust placement through Base UI's positioner instead
   * of CSS transforms.
   */
  const tooltip = (
    <Tooltip {...tooltipRootProps} onOpenChange={setOpen} open={open}>
      <TooltipTrigger delay={sidebarDelayMs ?? delay} render={children} />
      <TooltipContent
        align={align}
        alignOffset={alignOffset}
        anchor={anchor}
        className={contentClassName}
        collisionPadding={collisionPadding}
        side={side}
        sideOffset={sideOffset}
        style={contentStyle}
      >
        {content}
      </TooltipContent>
    </Tooltip>
  );
  return sidebarDelayMs === undefined ? (
    tooltip
  ) : (
    <TooltipProvider delay={sidebarDelayMs} timeout={0}>
      {tooltip}
    </TooltipProvider>
  );
}

export { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger };
