import { IconCheck, IconDots, IconPencil, IconPlus } from '@tabler/icons-react';
import { PointerSensor } from '@dnd-kit/dom';
import { move } from '@dnd-kit/helpers';
import { useDragDropMonitor, type DragDropEventHandlers } from '@dnd-kit/react';
import { useSortable } from '@dnd-kit/react/sortable';
import { useEffect, useEffectEvent, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { isSidebarCommandIcon, type SidebarCommandIcon } from '../shared/sidebar-command-icons';
import { openAppModal } from './app-modal-host-bridge';
import { AppTooltip, dismissSidebarTooltips } from './app-tooltip';
import { SidebarCommandIconGlyph } from './sidebar-command-icon';
import { SidebarContextMenuPortal } from './sidebar-context-menu-portal';
import { createSpaceDragData, getSidebarSpaceDragData } from './sidebar-dnd';
import { getSidebarReorderActivationConstraints } from './sidebar-reorder-activation';
import { DEFAULT_SIDEBAR_SPACE_ICON, type SidebarSpace, type SidebarSpacesState } from './spaces';
import type { WebviewApi } from './webview-api';

/*
CDXC:SidebarSpaces 2026-08-27:
One gxserver section's horizontal Space switcher. "All Projects" is a built-in
first button that is always visible and never overflows; the user's Spaces
follow it in their manual order, and whatever does not fit moves into the More
menu. More renders only when there is overflow. Its New Space action is a
convenience alongside the same action in project/group Space menus.

Which Spaces fit is decided from REAL measurements, never from a name-length
guess: the row renders every button for one pre-paint layout pass, records each
button's rendered width plus the track's computed column gap, and only then
splits the list. A ResizeObserver on the row re-runs the split when the sidebar
is resized, and a signature over the Space ids, names, and icons re-runs the
measuring pass when the buttons themselves change.
*/

type SpaceFilterRowProps = {
  /** True while the owning sidebar section is collapsed; the row hides with it. */
  collapsed: boolean;
  onReorderSpaces: (orderedSpaceIds: string[]) => void;
  onSelectSpace: (spaceId: string | undefined) => void;
  /** Present only for a remote gxserver section, and carried into the editor modal. */
  remoteMachineId?: string;
  sectionKey: string;
  selectedSpaceId?: string;
  spaces: SidebarSpacesState;
  vscode: WebviewApi;
};

type SpaceRowMeasurement = {
  allProjectsWidth: number;
  gap: number;
  moreWidth: number;
  signature: string;
  widthBySpaceId: Record<string, number>;
};

type SpaceRowLayout = {
  overflowSpaceIds: string[];
  visibleSpaceIds: string[];
};

type ContextMenuPosition = {
  x: number;
  y: number;
};

/*
 * CDXC:SidebarSpaces 2026-08-27:
 * Pointer-only, for the same reason the collection header reorder is (see the
 * CDXC:CollectionReorder note in project-collection-section.tsx): dnd-kit's
 * KeyboardSensor starts a drag from Space/Enter on any focused draggable, and a
 * stranded keyboard drag keeps the shared sidebar manager out of its idle state,
 * which silently kills every other drag in the sidebar. Space buttons are
 * ordinary focusable buttons, so they would be exactly such a trap.
 */
const spaceSensors = [
  PointerSensor.configure({
    activationConstraints: getSidebarReorderActivationConstraints,
  }),
];

/*
 * CDXC:SidebarSpaces 2026-08-27:
 * dnd-kit sortable ids are global to the app, and the same Space row renders
 * once per gxserver section, so ids are section-scoped exactly like the remote
 * project-collection instances are.
 */
export function createSidebarSpaceSortableId(sectionKey: string, spaceId: string): string {
  return `space:${sectionKey}:${spaceId}`;
}

/*
 * CDXC:SidebarSpaces 2026-08-28:
 * The row is icon-only, so "All Projects" needs a glyph of its own. It comes
 * from the same icon set the Space icon picker offers, which keeps the built-in
 * button visually part of the row instead of a differently-drawn special case.
 */
const ALL_PROJECTS_SPACE_ICON: SidebarCommandIcon = 'layoutDashboard';
const ALL_PROJECTS_SPACE_LABEL = 'All Projects';

/*
 * CDXC:SidebarSpaceSwipe 2026-08-28:
 * Trackpad swipes navigate one Space per physical gesture. Horizontal delta
 * must dominate vertical delta before the sidebar prevents default scrolling,
 * and momentum stays locked so one input stream cannot skip several Spaces.
 * DOM wheel events carry no NSEvent phase/momentumPhase, so the renderer alone
 * cannot tell "second physical swipe" apart from "the rest of the first". On
 * the desktop app the boundary comes from the native side: the AppKit
 * sendEvent observer reports each finger scroll-gesture begin
 * (NSEventPhaseBegan — fired once when fingers land and start scrolling,
 * never by momentum) over the sidebar, and that signal is the only thing that
 * releases the lock. One swipe therefore switches exactly one Space no matter
 * how long, slow, or uneven it is, and a distinct re-swipe during the first
 * swipe's momentum switches again immediately with no pointer movement.
 * Hosts without the native signal (the web app) segment the stream by
 * inter-event silence instead: finger and momentum events flow at frame
 * cadence without gaps, so a gap at or past the stream-gap threshold means
 * fingers left and landed again. Delta magnitudes are deliberately never used
 * for unlocking — a swipe that slows and speeds up must stay one gesture.
 * The old row/list move out first and their newly filtered contents enter from
 * the opposite side; reduced-motion users switch immediately. All Projects is
 * the first destination, and navigation stops at either end instead of wrapping
 * unexpectedly.
 */
const SPACE_SWIPE_THRESHOLD_PX = 44;
const SPACE_SWIPE_GESTURE_END_DELAY_MS = 120;
const SPACE_SWIPE_MEANINGFUL_DELTA_MIN_PX = 6;
/*
 * Gap segmentation for hosts without the native gesture-begin signal: an
 * unbroken finger/momentum stream never pauses longer than a few dropped
 * frames (8–16ms cadence), measured on event.timeStamp so late delivery under
 * renderer load cannot fabricate a gap. 64ms of stream silence can only be
 * fingers off the pad.
 */
const SPACE_SWIPE_STREAM_GAP_MS = 64;
/*
 * The native gesture-begin signal and the wheel deltas of the same physical
 * swipe travel different pipes (AppKit observer → Rust → CEF script vs. the
 * compositor), so a first swipe from idle can lock-and-switch off its racing
 * deltas before its own begin lands. A begin that arrives this soon after the
 * lock was taken belongs to the gesture that just locked and must not release
 * it; a genuine re-swipe's begin always trails the previous lock by far more
 * (lift, land, move — plus the previous swipe's own duration past its
 * threshold).
 */
const SPACE_SWIPE_NATIVE_BEGIN_LOCK_GRACE_MS = 80;

export const SIDEBAR_NATIVE_SCROLL_GESTURE_BEGAN_EVENT = 'ghostex-sidebar-native-scroll-gesture-began';

let sidebarNativeScrollGestureReportingActive = false;

/*
 * Called by the desktop sidebar entry when Rust's AppKit observer reports a
 * finger scroll-gesture begin (NSEventPhaseBegan) inside the sidebar frame.
 * The first report proves the host delivers the native signal, which then
 * owns gesture segmentation outright; the stream-gap heuristic stays off so
 * a mid-swipe stall can never be misread as a second swipe.
 */
export function reportSidebarNativeScrollGestureBegan() {
  sidebarNativeScrollGestureReportingActive = true;
  window.dispatchEvent(new Event(SIDEBAR_NATIVE_SCROLL_GESTURE_BEGAN_EVENT));
}
const SPACE_SWIPE_EXIT_DURATION_MS = 85;
const SPACE_SWIPE_ENTER_DURATION_MS = 165;

type SpaceSwipeDirection = 'next' | 'previous';

function resolveSidebarSpaceSwipeDestination(
  spaces: SidebarSpacesState,
  selectedSpaceId: string | undefined,
  direction: SpaceSwipeDirection
): { spaceId: string | undefined } | undefined {
  const destinations = [undefined, ...spaces.order.filter((spaceId) => spaces.spaces[spaceId] !== undefined)];
  const currentIndex = selectedSpaceId ? destinations.indexOf(selectedSpaceId) : 0;
  const resolvedCurrentIndex = currentIndex >= 0 ? currentIndex : 0;
  const nextIndex = direction === 'next' ? resolvedCurrentIndex + 1 : resolvedCurrentIndex - 1;
  if (nextIndex < 0 || nextIndex >= destinations.length) {
    return undefined;
  }
  return { spaceId: destinations[nextIndex] };
}

function normalizedWheelDelta(delta: number, deltaMode: number, pageSize: number): number {
  if (deltaMode === WheelEvent.DOM_DELTA_LINE) {
    return delta * 16;
  }
  if (deltaMode === WheelEvent.DOM_DELTA_PAGE) {
    return delta * pageSize;
  }
  return delta;
}

async function waitForAnimations(animations: readonly Animation[]): Promise<void> {
  await Promise.all(animations.map((animation) => animation.finished.catch(() => undefined)));
}

export function resolveSidebarSpaceIcon(icon: string): SidebarCommandIcon {
  return isSidebarCommandIcon(icon) ? icon : DEFAULT_SIDEBAR_SPACE_ICON;
}

/**
 * Split the ordered user Spaces into the ones that fit beside the pinned All
 * Projects button and the permanent More control, then apply the promotion rule:
 * a selected Space that would sit in More takes the LAST visible slot, pushing
 * as many trailing Spaces into More as its own width needs. All Projects is
 * never displaced.
 */
export function createSidebarSpaceRowLayout({
  containerWidth,
  measurement,
  orderedSpaceIds,
  selectedSpaceId,
}: {
  containerWidth: number;
  measurement: SpaceRowMeasurement;
  orderedSpaceIds: readonly string[];
  selectedSpaceId: string | undefined;
}): SpaceRowLayout {
  const { allProjectsWidth, gap, moreWidth, widthBySpaceId } = measurement;
  const allSpacesFit =
    orderedSpaceIds.every((spaceId) => widthBySpaceId[spaceId] !== undefined) &&
    allProjectsWidth + orderedSpaceIds.reduce((total, spaceId) => total + gap + (widthBySpaceId[spaceId] ?? 0), 0) <=
      containerWidth;
  if (allSpacesFit) {
    return { overflowSpaceIds: [], visibleSpaceIds: [...orderedSpaceIds] };
  }

  let budget = containerWidth - allProjectsWidth - moreWidth - gap;
  const visibleSpaceIds: string[] = [];
  for (const spaceId of orderedSpaceIds) {
    const width = widthBySpaceId[spaceId];
    if (width === undefined || budget - (width + gap) < 0) {
      break;
    }
    budget -= width + gap;
    visibleSpaceIds.push(spaceId);
  }

  if (selectedSpaceId && orderedSpaceIds.includes(selectedSpaceId) && !visibleSpaceIds.includes(selectedSpaceId)) {
    const selectedWidth = widthBySpaceId[selectedSpaceId] ?? 0;
    while (visibleSpaceIds.length > 0 && budget - (selectedWidth + gap) < 0) {
      const displacedSpaceId = visibleSpaceIds.pop();
      budget += (widthBySpaceId[displacedSpaceId ?? ''] ?? 0) + gap;
    }
    visibleSpaceIds.push(selectedSpaceId);
  }

  const visibleSpaceIdSet = new Set(visibleSpaceIds);
  return {
    overflowSpaceIds: orderedSpaceIds.filter((spaceId) => !visibleSpaceIdSet.has(spaceId)),
    visibleSpaceIds,
  };
}

/**
 * Project a reorder of the VISIBLE Space buttons back onto the full order. The
 * visible buttons occupy a set of positions in the stored order (promotion can
 * make that set non-contiguous), so the reordered visible ids are written back
 * into exactly those positions and every overflowed Space keeps its slot.
 */
export function applySidebarSpaceRowReorder(
  orderedSpaceIds: readonly string[],
  visibleSpaceIds: readonly string[],
  reorderedVisibleSpaceIds: readonly string[]
): string[] {
  const visibleSpaceIdSet = new Set(visibleSpaceIds);
  const nextOrder = [...orderedSpaceIds];
  let visibleIndex = 0;
  for (const [index, spaceId] of nextOrder.entries()) {
    if (!visibleSpaceIdSet.has(spaceId)) {
      continue;
    }
    const nextSpaceId = reorderedVisibleSpaceIds[visibleIndex];
    visibleIndex += 1;
    if (nextSpaceId) {
      nextOrder[index] = nextSpaceId;
    }
  }
  return nextOrder;
}

export function SpaceFilterRow({
  collapsed,
  onReorderSpaces,
  onSelectSpace,
  remoteMachineId,
  sectionKey,
  selectedSpaceId,
  spaces,
  vscode,
}: SpaceFilterRowProps) {
  const [rowElement, setRowElement] = useState<HTMLDivElement | null>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const allProjectsButtonRef = useRef<HTMLButtonElement>(null);
  const moreButtonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const swipeAnimationsRef = useRef<Animation[]>([]);
  const swipeAnimationGenerationRef = useRef(0);
  const swipeDeltaXRef = useRef(0);
  const swipeDirectionSignRef = useRef(0);
  const swipeGestureLockedRef = useRef(false);
  const swipeGestureEndTimerRef = useRef<number | undefined>(undefined);
  const swipeLastStreamEventAtRef = useRef(0);
  const swipeLockTakenAtMsRef = useRef(0);
  const [containerWidth, setContainerWidth] = useState(0);
  const [measurement, setMeasurement] = useState<SpaceRowMeasurement>();
  const [isMeasuring, setIsMeasuring] = useState(true);
  const [moreMenuPosition, setMoreMenuPosition] = useState<ContextMenuPosition>();
  const [editMenu, setEditMenu] = useState<{ position: ContextMenuPosition; spaceId: string }>();

  const orderedSpaces = useMemo(
    () => spaces.order.flatMap((spaceId) => (spaces.spaces[spaceId] ? [spaces.spaces[spaceId]] : [])),
    [spaces]
  );
  const signature = useMemo(
    () => orderedSpaces.map((space) => [space.spaceId, space.name, space.icon].join(' ')).join('|'),
    [orderedSpaces]
  );

  /*
   * CDXC:SidebarSpaces 2026-08-29:
   * Only the filtered CONTENT animates on a Space switch. The switcher row
   * itself is pinned chrome (it sticks with the Projects header while the
   * list scrolls) and must hold perfectly still through swipe and click
   * switches alike, so it is deliberately not part of this set.
   */
  const getSwipeAnimationElements = (): HTMLElement[] => {
    const elements: HTMLElement[] = [];
    for (const element of document.querySelectorAll<HTMLElement>('[data-sidebar-space-content-section]')) {
      if (element.dataset.sidebarSpaceContentSection === sectionKey) {
        elements.push(element);
      }
    }
    return elements;
  };

  const cancelSwipeAnimations = () => {
    for (const animation of swipeAnimationsRef.current) {
      animation.cancel();
    }
    swipeAnimationsRef.current = [];
  };

  const animateSwipeBoundary = (direction: SpaceSwipeDirection) => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      return;
    }
    cancelSwipeAnimations();
    const offset = direction === 'next' ? -5 : 5;
    swipeAnimationsRef.current = getSwipeAnimationElements().map((element) =>
      element.animate(
        [
          { transform: 'translateX(0)' },
          { transform: `translateX(${offset}px)`, offset: 0.45 },
          { transform: 'translateX(0)' },
        ],
        { duration: 150, easing: 'cubic-bezier(0.22, 1, 0.36, 1)' }
      )
    );
  };

  const navigateBySwipe = useEffectEvent(async (direction: SpaceSwipeDirection) => {
    const destination = resolveSidebarSpaceSwipeDestination(spaces, selectedSpaceId, direction);
    if (!destination) {
      animateSwipeBoundary(direction);
      return;
    }

    dismissSidebarTooltips();
    const animationGeneration = swipeAnimationGenerationRef.current + 1;
    swipeAnimationGenerationRef.current = animationGeneration;
    const animationElements = getSwipeAnimationElements();
    const shouldReduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (shouldReduceMotion || animationElements.length === 0) {
      onSelectSpace(destination.spaceId);
      return;
    }

    cancelSwipeAnimations();
    const exitOffset = direction === 'next' ? -12 : 12;
    const exitAnimations = animationElements.map((element) =>
      element.animate(
        [
          { opacity: 1, transform: 'translateX(0)' },
          { opacity: 0, transform: `translateX(${exitOffset}px)` },
        ],
        {
          duration: SPACE_SWIPE_EXIT_DURATION_MS,
          easing: 'cubic-bezier(0.4, 0, 1, 1)',
          fill: 'forwards',
        }
      )
    );
    swipeAnimationsRef.current = exitAnimations;
    await waitForAnimations(exitAnimations);
    if (swipeAnimationGenerationRef.current !== animationGeneration) {
      return;
    }

    onSelectSpace(destination.spaceId);
    await new Promise<void>((resolve) => window.requestAnimationFrame(() => resolve()));
    if (swipeAnimationGenerationRef.current !== animationGeneration) {
      return;
    }

    const enterOffset = direction === 'next' ? 16 : -16;
    const enterAnimations = animationElements.map((element) =>
      element.animate(
        [
          { opacity: 0, transform: `translateX(${enterOffset}px)` },
          { opacity: 1, transform: 'translateX(0)' },
        ],
        {
          duration: SPACE_SWIPE_ENTER_DURATION_MS,
          easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
          fill: 'forwards',
        }
      )
    );
    for (const animation of exitAnimations) {
      animation.cancel();
    }
    swipeAnimationsRef.current = enterAnimations;
    await waitForAnimations(enterAnimations);
    if (swipeAnimationGenerationRef.current === animationGeneration) {
      cancelSwipeAnimations();
    }
  });

  useEffect(() => {
    if (collapsed || spaces.order.length === 0) {
      return undefined;
    }

    const resetGesture = () => {
      if (swipeGestureEndTimerRef.current !== undefined) {
        window.clearTimeout(swipeGestureEndTimerRef.current);
      }
      swipeDeltaXRef.current = 0;
      swipeDirectionSignRef.current = 0;
      swipeGestureLockedRef.current = false;
      swipeGestureEndTimerRef.current = undefined;
    };
    const scheduleGestureReset = () => {
      if (swipeGestureEndTimerRef.current !== undefined) {
        window.clearTimeout(swipeGestureEndTimerRef.current);
      }
      swipeGestureEndTimerRef.current = window.setTimeout(resetGesture, SPACE_SWIPE_GESTURE_END_DELAY_MS);
    };
    /*
     * Machine tabs guarantee that this scroll viewport contains exactly one
     * active machine surface and therefore one mounted Space row. Bind the
     * gesture to the whole owning viewport instead of individual project nodes,
     * so headers, empty-state padding, and every row are equally swipeable.
     */
    const scrollViewport = rowElement?.closest<HTMLElement>('.session-groups-content');
    const belongsToActiveScroller = (target: Element): boolean => scrollViewport?.contains(target) === true;
    const handleWheel = (event: WheelEvent) => {
      if (event.ctrlKey || event.shiftKey || document.body.dataset.sidebarTooltipsSuppressed === 'true') {
        return;
      }
      /*
       * CDXC:SidebarSpaceSwipe 2026-08-28:
       * A Space switch can unmount the project row that was under a stationary
       * cursor. CEF may keep the next wheel gesture targeted at that detached
       * node until pointer movement forces a hit test, which made the second
       * swipe appear locked. Resolve the live element under the wheel event's
       * coordinates every time; retain event.target only for environments that
       * cannot return a point target.
       */
      const eventTarget = event.target instanceof Element ? event.target : undefined;
      const pointTarget = document.elementFromPoint(event.clientX, event.clientY);
      const targetBelongsToActiveScroller =
        (pointTarget ? belongsToActiveScroller(pointTarget) : false) ||
        (eventTarget ? belongsToActiveScroller(eventTarget) : false) ||
        scrollViewport?.matches(':hover') === true;
      if (!targetBelongsToActiveScroller) {
        return;
      }
      /*
       * Heuristic stream segmentation, only while no native gesture-begin
       * signal has ever arrived (see reportSidebarNativeScrollGestureBegan).
       * It runs on every wheel event over the scroller — any delta at all,
       * horizontal-dominant or not, means the pad is emitting, so a slow
       * mid-swipe crawl (sub-2px deltas) or a momentum tail keeps the stream
       * alive and only genuine finger lift-and-land silence starts a new
       * gesture. The timestamp survives gesture resets: it describes the raw
       * stream, not the gesture built on top of it.
       */
      const gapSinceLastStreamEventMs = event.timeStamp - swipeLastStreamEventAtRef.current;
      swipeLastStreamEventAtRef.current = event.timeStamp;
      if (!sidebarNativeScrollGestureReportingActive && gapSinceLastStreamEventMs >= SPACE_SWIPE_STREAM_GAP_MS) {
        resetGesture();
      }
      const pageSize = rowElement?.clientWidth ?? window.innerWidth;
      const deltaX = normalizedWheelDelta(event.deltaX, event.deltaMode, pageSize);
      const deltaY = normalizedWheelDelta(event.deltaY, event.deltaMode, window.innerHeight);
      if (Math.abs(deltaX) < 2 || Math.abs(deltaX) <= Math.abs(deltaY) * 1.25) {
        return;
      }

      event.preventDefault();
      /*
       * The silence timer is the heuristic hosts' trailing gesture end. It is
       * re-armed by every horizontal-dominant event — a slow crawl included —
       * so it can only fire once the pad has truly gone quiet, never in the
       * middle of an unevenly paced swipe. Under native reporting it stays
       * unarmed: NSEventPhaseBegan is the only gesture boundary there.
       */
      if (!sidebarNativeScrollGestureReportingActive) {
        scheduleGestureReset();
      }
      const deltaMagnitude = Math.abs(deltaX);
      if (deltaMagnitude < SPACE_SWIPE_MEANINGFUL_DELTA_MIN_PX) {
        return;
      }
      if (swipeGestureLockedRef.current) {
        return;
      }
      const directionSign = Math.sign(deltaX);
      if (swipeDirectionSignRef.current !== 0 && swipeDirectionSignRef.current !== directionSign) {
        swipeDeltaXRef.current = 0;
      }
      swipeDirectionSignRef.current = directionSign;
      swipeDeltaXRef.current += deltaX;
      if (Math.abs(swipeDeltaXRef.current) < SPACE_SWIPE_THRESHOLD_PX) {
        return;
      }

      swipeGestureLockedRef.current = true;
      swipeLockTakenAtMsRef.current = performance.now();
      const direction: SpaceSwipeDirection = swipeDeltaXRef.current > 0 ? 'next' : 'previous';
      void navigateBySwipe(direction);
    };

    /*
     * Fingers landed on the pad and started a scroll somewhere over the
     * sidebar. Whatever gesture state exists belongs to the previous physical
     * swipe (its momentum can no longer emit anything — touching the pad
     * halts it), so release the lock and let this gesture accumulate toward
     * its own single switch — unless the lock was taken within the grace
     * window, in which case this begin raced behind its own gesture's deltas
     * and releasing would let that same swipe switch twice.
     */
    const handleNativeScrollGestureBegan = () => {
      if (
        swipeGestureLockedRef.current &&
        performance.now() - swipeLockTakenAtMsRef.current < SPACE_SWIPE_NATIVE_BEGIN_LOCK_GRACE_MS
      ) {
        return;
      }
      resetGesture();
    };

    window.addEventListener('wheel', handleWheel, { capture: true, passive: false });
    window.addEventListener(SIDEBAR_NATIVE_SCROLL_GESTURE_BEGAN_EVENT, handleNativeScrollGestureBegan);
    return () => {
      window.removeEventListener('wheel', handleWheel, { capture: true });
      window.removeEventListener(SIDEBAR_NATIVE_SCROLL_GESTURE_BEGAN_EVENT, handleNativeScrollGestureBegan);
      if (swipeGestureEndTimerRef.current !== undefined) {
        window.clearTimeout(swipeGestureEndTimerRef.current);
      }
      resetGesture();
    };
  }, [collapsed, rowElement, spaces.order.length]);

  useEffect(
    () => () => {
      swipeAnimationGenerationRef.current += 1;
      cancelSwipeAnimations();
    },
    []
  );

  useLayoutEffect(() => {
    if (!rowElement) {
      return undefined;
    }
    const updateContainerWidth = () => setContainerWidth(trackRef.current?.clientWidth ?? rowElement.clientWidth);
    updateContainerWidth();
    const resizeObserver = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(updateContainerWidth);
    resizeObserver?.observe(rowElement);
    return () => resizeObserver?.disconnect();
  }, [rowElement]);

  useLayoutEffect(() => {
    const track = trackRef.current;
    if (!track || measurement?.signature === signature) {
      return;
    }
    if (!isMeasuring) {
      setIsMeasuring(true);
      return;
    }
    /*
     * Widths are read straight off the rendered buttons in this track rather
     * than through per-button ref registration: a ref callback that changes
     * identity on every render would also re-register the dnd-kit droppable on
     * every render, which is exactly the kind of churn that breaks an in-flight
     * drag.
     */
    const widthBySpaceId: Record<string, number> = {};
    for (const element of track.querySelectorAll<HTMLElement>('[data-sidebar-space-id]')) {
      const spaceId = element.dataset.sidebarSpaceId;
      if (spaceId) {
        widthBySpaceId[spaceId] = element.getBoundingClientRect().width;
      }
    }
    const parsedGap = Number.parseFloat(window.getComputedStyle(track).columnGap);
    setMeasurement({
      allProjectsWidth: allProjectsButtonRef.current?.getBoundingClientRect().width ?? 0,
      gap: Number.isFinite(parsedGap) ? parsedGap : 0,
      moreWidth: moreButtonRef.current?.getBoundingClientRect().width ?? 0,
      signature,
      widthBySpaceId,
    });
    setIsMeasuring(false);
    /*
     * `rowElement` is a dependency because a collapsed section unmounts the row
     * entirely: without it the measuring pass would be skipped for good when the
     * section starts collapsed and is expanded later.
     */
  }, [isMeasuring, measurement, rowElement, signature]);

  const layout = useMemo<SpaceRowLayout>(() => {
    if (isMeasuring || !measurement || measurement.signature !== signature || containerWidth <= 0) {
      return { overflowSpaceIds: [], visibleSpaceIds: spaces.order };
    }
    return createSidebarSpaceRowLayout({
      containerWidth,
      measurement,
      orderedSpaceIds: spaces.order,
      selectedSpaceId,
    });
  }, [containerWidth, isMeasuring, measurement, selectedSpaceId, signature, spaces.order]);

  const layoutRef = useRef(layout);
  layoutRef.current = layout;
  const orderRef = useRef(spaces.order);
  orderRef.current = spaces.order;

  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * The drop is committed from the shared sidebar drag manager rather than from
   * a nested provider, and only for this section's own Space drags. dnd-kit's
   * `move` resolves the new position from the drop target it reported, which
   * exists here precisely because Space buttons use the default drag feedback:
   * the collection/project surfaces use feedback "none", which never yields a
   * rect-overlap target and is why those paths resolve drops from the pointer.
   */
  const handleSpaceDragEnd = useEffectEvent((event: Parameters<NonNullable<DragDropEventHandlers['onDragEnd']>>[0]) => {
    const sourceData = getSidebarSpaceDragData(event.operation.source);
    if (!sourceData || sourceData.sectionKey !== sectionKey || event.canceled) {
      return;
    }
    const visibleSpaceIds = layoutRef.current.visibleSpaceIds;
    const sortableIds = visibleSpaceIds.map((spaceId) => createSidebarSpaceSortableId(sectionKey, spaceId));
    const sortableIdPrefix = createSidebarSpaceSortableId(sectionKey, '');
    const reorderedVisibleSpaceIds = move(sortableIds, event).map((sortableId) =>
      sortableId.slice(sortableIdPrefix.length)
    );
    const nextOrder = applySidebarSpaceRowReorder(orderRef.current, visibleSpaceIds, reorderedVisibleSpaceIds);
    if (nextOrder.some((spaceId, index) => spaceId !== orderRef.current[index])) {
      onReorderSpaces(nextOrder);
    }
  });
  useDragDropMonitor(useMemo(() => ({ onDragEnd: handleSpaceDragEnd }), [handleSpaceDragEnd]));

  const openSpaceEditor = (space?: SidebarSpace) => {
    openAppModal({
      ...(space
        ? {
            spaceColor: space.color,
            spaceIcon: space.icon,
            spaceId: space.spaceId,
            spaceName: space.name,
          }
        : {}),
      ...(remoteMachineId ? { remoteMachineId } : {}),
      mode: space ? 'edit' : 'create',
      modal: 'sidebarSpaceEditor',
      sectionKey,
      type: 'open',
    });
  };

  const dismissMenus = () => {
    setMoreMenuPosition(undefined);
    setEditMenu(undefined);
  };

  if (collapsed) {
    return null;
  }

  const visibleSpaces = layout.visibleSpaceIds.flatMap((spaceId) => {
    const space = spaces.spaces[spaceId];
    return space ? [space] : [];
  });
  const overflowSpaces = layout.overflowSpaceIds.flatMap((spaceId) => {
    const space = spaces.spaces[spaceId];
    return space ? [space] : [];
  });
  const editedSpace = editMenu ? spaces.spaces[editMenu.spaceId] : undefined;
  const shouldShowMoreButton = isMeasuring || layout.overflowSpaceIds.length > 0 || moreMenuPosition !== undefined;

  return (
    <div className='sidebar-space-filter-row' data-sidebar-space-section={sectionKey} ref={setRowElement}>
      <div className='sidebar-space-filter-track' data-measuring={String(isMeasuring)} ref={trackRef}>
        <AppTooltip content={ALL_PROJECTS_SPACE_LABEL}>
          <button
            aria-label={ALL_PROJECTS_SPACE_LABEL}
            aria-pressed={selectedSpaceId === undefined}
            className='sidebar-space-filter-button sidebar-space-filter-all'
            data-selected={String(selectedSpaceId === undefined)}
            onClick={() => onSelectSpace(undefined)}
            ref={allProjectsButtonRef}
            type='button'
          >
            <SidebarCommandIconGlyph className='sidebar-space-filter-icon' icon={ALL_PROJECTS_SPACE_ICON} size={16} />
          </button>
        </AppTooltip>
        {visibleSpaces.map((space, index) => (
          <SpaceFilterButton
            index={index}
            key={space.spaceId}
            onContextMenu={(position) => {
              setMoreMenuPosition(undefined);
              setEditMenu({ position, spaceId: space.spaceId });
            }}
            onSelect={() => onSelectSpace(space.spaceId)}
            sectionKey={sectionKey}
            selected={selectedSpaceId === space.spaceId}
            space={space}
          />
        ))}
        {shouldShowMoreButton ? (
          <AppTooltip content='More Spaces'>
            <button
              aria-label='More Spaces'
              className='sidebar-space-filter-button sidebar-space-filter-more'
              onClick={(event) => {
                const bounds = event.currentTarget.getBoundingClientRect();
                setEditMenu(undefined);
                setMoreMenuPosition({ x: bounds.left, y: bounds.bottom + 4 });
              }}
              ref={moreButtonRef}
              type='button'
            >
              <IconDots aria-hidden='true' size={16} stroke={2} />
            </button>
          </AppTooltip>
        ) : null}
      </div>
      {moreMenuPosition ? (
        <SidebarContextMenuPortal
          menuRef={menuRef}
          menuStyle={{ left: `${moreMenuPosition.x}px`, top: `${moreMenuPosition.y}px`, width: '218px' }}
          onDismiss={dismissMenus}
          vscode={vscode}
        >
          <button
            className='session-context-menu-item'
            onClick={() => {
              dismissMenus();
              openSpaceEditor();
            }}
            role='menuitem'
            type='button'
          >
            <IconPlus aria-hidden='true' className='session-context-menu-icon' size={14} stroke={2} />
            New Space
          </button>
          {overflowSpaces.length > 0 ? <div className='session-context-menu-divider' role='separator' /> : null}
          {overflowSpaces.map((space) => (
            <button
              aria-checked={selectedSpaceId === space.spaceId}
              className='session-context-menu-item'
              key={space.spaceId}
              onClick={() => {
                dismissMenus();
                onSelectSpace(space.spaceId);
              }}
              role='menuitemradio'
              type='button'
            >
              <SidebarCommandIconGlyph
                className='session-context-menu-icon'
                color={space.color}
                icon={resolveSidebarSpaceIcon(space.icon)}
                size={14}
              />
              <span className='sidebar-space-filter-menu-name'>{space.name}</span>
              {selectedSpaceId === space.spaceId ? (
                <IconCheck aria-hidden='true' className='session-context-menu-trailing-icon' size={14} />
              ) : null}
            </button>
          ))}
        </SidebarContextMenuPortal>
      ) : null}
      {editMenu && editedSpace ? (
        <SidebarContextMenuPortal
          menuRef={menuRef}
          menuStyle={{ left: `${editMenu.position.x}px`, top: `${editMenu.position.y}px`, width: '218px' }}
          onDismiss={dismissMenus}
          vscode={vscode}
        >
          <button
            className='session-context-menu-item'
            onClick={() => {
              dismissMenus();
              openSpaceEditor(editedSpace);
            }}
            role='menuitem'
            type='button'
          >
            <IconPencil aria-hidden='true' className='session-context-menu-icon' size={14} />
            Edit Space…
          </button>
        </SidebarContextMenuPortal>
      ) : null}
    </div>
  );
}

function SpaceFilterButton({
  index,
  onContextMenu,
  onSelect,
  sectionKey,
  selected,
  space,
}: {
  index: number;
  onContextMenu: (position: ContextMenuPosition) => void;
  onSelect: () => void;
  sectionKey: string;
  selected: boolean;
  space: SidebarSpace;
}) {
  /*
   * CDXC:SidebarSpaces 2026-08-27:
   * The button is both the drag handle and the sortable element, and the
   * accepted type is section-scoped so a Space can only ever be dropped among
   * its own gxserver's Spaces. All Projects and More render outside this
   * component, so neither is sortable and neither can be displaced by a drop.
   */
  const sortable = useSortable({
    accept: `space:${sectionKey}`,
    data: createSpaceDragData(sectionKey, space.spaceId),
    id: createSidebarSpaceSortableId(sectionKey, space.spaceId),
    index,
    sensors: spaceSensors,
    type: `space:${sectionKey}`,
  });
  const style = { '--sidebar-space-color': space.color } as CSSProperties;

  return (
    <AppTooltip content={space.name}>
      <button
        aria-label={space.name}
        aria-pressed={selected}
        className='sidebar-space-filter-button sidebar-space-filter-space'
        data-dragging={String(sortable.isDragging)}
        data-selected={String(selected)}
        data-sidebar-space-id={space.spaceId}
        onClick={onSelect}
        onContextMenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
          onContextMenu({ x: event.clientX, y: event.clientY });
        }}
        ref={sortable.ref}
        style={style}
        type='button'
      >
        <SidebarCommandIconGlyph
          className='sidebar-space-filter-icon'
          color={space.color}
          icon={resolveSidebarSpaceIcon(space.icon)}
          size={16}
        />
      </button>
    </AppTooltip>
  );
}
