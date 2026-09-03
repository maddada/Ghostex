'use client';

import * as React from 'react';
import { Toggle as TogglePrimitive } from '@base-ui/react/toggle';
import { ToggleGroup as ToggleGroupPrimitive } from '@base-ui/react/toggle-group';

import { cn } from '../utils';

/*
 * CDXC:DesignSystem 2026-08-24:
 * The one segmented single-select control for the whole app. It renders the
 * stock shadcn ButtonGroup shape — a single bordered, rounded container whose
 * segments are flat, share one hairline, and only the outer corners are
 * rounded — with the selected segment carrying a highlighted fill.
 *
 * It deliberately does NOT reuse Toggle/ToggleGroup's `toggleVariants`
 * classes: those ship `rounded-none` and per-item borders, which the app's
 * unlayered square-theme and settings sheets rewrite (`.rounded-none` is
 * re-rounded with !important there), so every segment ended up drawing its own
 * rounded box inside the group and reading as unrelated buttons. Owning the
 * geometry here, under private data-slots, keeps one shape everywhere.
 *
 * Any new "pick exactly one of N" strip should use this instead of a row of
 * Buttons or a raw ToggleGroup.
 */

type SegmentedControlSize = 'default' | 'sm';

const SegmentedControlContext = React.createContext<{ size: SegmentedControlSize }>({
  size: 'default',
});

/*
 * The container owns the control height (border-box, matching Button and
 * SelectTrigger of the same size) and the items fill it. Sizing the items
 * instead made every segmented control 2px taller than the buttons and
 * dropdowns sharing its row, because the container border wrapped around the
 * full-height items.
 */
const segmentedControlClass =
  'group/segmented box-border inline-flex w-fit items-stretch overflow-hidden rounded-[8px] border border-border bg-transparent [&>[data-slot=segmented-control-item]+[data-slot=segmented-control-item]]:border-l [&>[data-slot=segmented-control-item]+[data-slot=segmented-control-item]]:border-border';

/*
 * The icon sizing deliberately avoids any `size-*` utility: the settings sheet
 * rounds every element whose class list mentions one, which would put a radius
 * back on each segment and undo the joined-strip shape.
 */
const segmentedControlItemClass =
  'relative inline-flex min-w-0 shrink-0 items-center justify-center gap-1.5 bg-transparent px-3 font-normal whitespace-nowrap text-muted-foreground transition-colors outline-none select-none hover:bg-accent/60 hover:text-foreground focus-visible:z-10 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-50 aria-pressed:bg-foreground/14 aria-pressed:text-foreground [&_svg]:pointer-events-none [&_svg]:h-4 [&_svg]:w-4 [&_svg]:shrink-0';

function SegmentedControl({
  className,
  children,
  onValueChange,
  size = 'default',
  stretch = false,
  value,
  ...props
}: Omit<ToggleGroupPrimitive.Props, 'defaultValue' | 'onValueChange' | 'value'> & {
  /** Stretch the segments to fill the row instead of sizing to their labels. */
  stretch?: boolean;
  size?: SegmentedControlSize;
  value: string;
  onValueChange: (value: string) => void;
}) {
  const contextValue = React.useMemo(() => ({ size }), [size]);
  return (
    <ToggleGroupPrimitive
      data-slot='segmented-control'
      data-size={size}
      className={cn(
        segmentedControlClass,
        size === 'sm' ? 'h-7' : 'h-8',
        stretch && 'w-full [&>[data-slot=segmented-control-item]]:flex-1',
        className
      )}
      onValueChange={(nextValues) => {
        /*
         * Base UI hands back the full selection array and lets a click on the
         * active segment clear it. A segmented control is single-select and
         * always has a value, so an empty array is ignored rather than
         * reported as a change.
         */
        const [nextValue] = nextValues as string[];
        if (nextValue && nextValue !== value) {
          onValueChange(nextValue);
        }
      }}
      value={[value]}
      {...props}
    >
      <SegmentedControlContext.Provider value={contextValue}>{children}</SegmentedControlContext.Provider>
    </ToggleGroupPrimitive>
  );
}

function SegmentedControlItem({ className, children, ...props }: TogglePrimitive.Props) {
  const { size } = React.useContext(SegmentedControlContext);
  return (
    <TogglePrimitive
      data-slot='segmented-control-item'
      data-size={size}
      className={cn(segmentedControlItemClass, 'h-full', size === 'sm' ? 'text-xs' : 'text-[0.8125rem]', className)}
      {...props}
    >
      {children}
    </TogglePrimitive>
  );
}

export { SegmentedControl, SegmentedControlItem };
