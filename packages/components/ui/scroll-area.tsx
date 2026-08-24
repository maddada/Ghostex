import * as React from 'react';
import { ScrollArea as ScrollAreaPrimitive } from '@base-ui/react/scroll-area';

import { cn } from '../utils';

function ScrollArea({ className, children, ...props }: ScrollAreaPrimitive.Root.Props) {
  return (
    <ScrollAreaPrimitive.Root data-slot='scroll-area' className={cn('relative', className)} {...props}>
      {/*
       * CDXC:ScrollFades 2026-06-19-14:16:
       * Shared ScrollArea viewports are used by Project Board details,
       * Agents Hub, and modal bodies. Apply the Codex-style mask at the
       * scrolling viewport so fixed outer chrome and custom scrollbars stay
       * outside the fade.
       */}
      <ScrollAreaPrimitive.Viewport
        data-slot='scroll-area-viewport'
        className='vertical-scroll-fade-mask size-full rounded-none transition-[color,box-shadow] outline-none focus-visible:ring-[3px] focus-visible:ring-ring/20 focus-visible:outline-1'
      >
        {children}
      </ScrollAreaPrimitive.Viewport>
      <ScrollBar />
      <ScrollAreaPrimitive.Corner />
    </ScrollAreaPrimitive.Root>
  );
}

function ScrollBar({ className, orientation = 'vertical', ...props }: ScrollAreaPrimitive.Scrollbar.Props) {
  return (
    <ScrollAreaPrimitive.Scrollbar
      data-slot='scroll-area-scrollbar'
      data-orientation={orientation}
      orientation={orientation}
      className={cn(
        'flex touch-none p-px transition-colors select-none data-horizontal:h-2.5 data-horizontal:flex-col data-horizontal:border-t data-horizontal:border-t-transparent data-vertical:h-full data-vertical:w-2.5 data-vertical:border-l data-vertical:border-l-transparent',
        className
      )}
      {...props}
    >
      <ScrollAreaPrimitive.Thumb data-slot='scroll-area-thumb' className='relative flex-1 rounded-none bg-border' />
    </ScrollAreaPrimitive.Scrollbar>
  );
}

export { ScrollArea, ScrollBar };
