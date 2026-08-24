import { Tooltip as TooltipPrimitive } from '@base-ui/react/tooltip';
import * as React from 'react';

import { cn } from '../utils';
import { tooltipSurfaceStyle } from './overlay-surface';
import { TOOLTIP_DELAY_MS, TOOLTIP_MOTION_CLASS_NAME } from './tooltip-config';

function TooltipProvider({
  delayDuration,
  delay = TOOLTIP_DELAY_MS,
  ...props
}: TooltipPrimitive.Provider.Props & {
  delayDuration?: number;
}) {
  return <TooltipPrimitive.Provider data-slot='tooltip-provider' delay={delayDuration ?? delay} {...props} />;
}

function Tooltip({
  onOpenChange,
  ...props
}: Omit<TooltipPrimitive.Root.Props, 'onOpenChange'> & {
  onOpenChange?: (open: boolean) => void;
}) {
  return (
    <TooltipPrimitive.Root
      data-slot='tooltip'
      onOpenChange={onOpenChange ? (open) => onOpenChange(open) : undefined}
      {...props}
    />
  );
}

function TooltipTrigger({ ...props }: TooltipPrimitive.Trigger.Props) {
  return <TooltipPrimitive.Trigger data-slot='tooltip-trigger' {...props} />;
}

function TooltipContent({
  anchor,
  className,
  side = 'bottom',
  sideOffset = 0,
  align = 'center',
  alignOffset = 0,
  collisionPadding,
  children,
  style,
  ...props
}: TooltipPrimitive.Popup.Props &
  Pick<TooltipPrimitive.Positioner.Props, 'align' | 'alignOffset' | 'anchor' | 'side' | 'sideOffset'> & {
    collisionPadding?: number;
  }) {
  return (
    <TooltipPrimitive.Portal>
      {/*
       * CDXC:SidebarTooltips 2026-06-30-02:02:
       * macOS sidebar tooltip surfaces must stay outside pointer hit testing.
       * Hover over the label must not keep it shown, wheel events should pass
       * through to underlying scroll containers, and clicks should land on the
       * controls below the tooltip.
       */}
      <TooltipPrimitive.Positioner
        align={align}
        alignOffset={alignOffset}
        anchor={anchor}
        side={side}
        sideOffset={sideOffset}
        collisionPadding={collisionPadding}
        className='tooltip-positioner pointer-events-none isolate z-50'
      >
        <TooltipPrimitive.Popup
          data-slot='tooltip-content'
          className={cn(
            'pointer-events-none z-50 inline-block w-fit whitespace-pre-line px-3 py-1.5 text-xs [overflow-wrap:anywhere] has-data-[slot=kbd]:pr-1.5 **:data-[slot=kbd]:relative **:data-[slot=kbd]:isolate **:data-[slot=kbd]:z-50 **:data-[slot=kbd]:rounded-none',
            TOOLTIP_MOTION_CLASS_NAME,
            className
          )}
          style={{
            ...tooltipSurfaceStyle,
            maxWidth: 'min(90vw, var(--available-width, 90vw))',
            zIndex: 'var(--ghostex-tooltip-z-index, 1400)',
            ...style,
          }}
          {...props}
        >
          {children}
        </TooltipPrimitive.Popup>
      </TooltipPrimitive.Positioner>
    </TooltipPrimitive.Portal>
  );
}

export { TOOLTIP_DELAY_MS, TOOLTIP_MOTION_CLASS_NAME, Tooltip, TooltipTrigger, TooltipContent, TooltipProvider };
