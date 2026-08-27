import { TOOLTIP_DELAY_MS as BASE_TOOLTIP_DELAY_MS } from '../components/ui/tooltip-config';
import { TooltipProvider } from '../components/ui/tooltip';
import { createContext, createElement, useContext, type ReactNode } from 'react';
import { DEFAULT_SIDEBAR_TOOLTIP_DELAY_MS } from '../shared/ghostex-settings';

/** Sidebar tooltips intentionally wait 300ms longer than the app-wide default. */
export const TOOLTIP_DELAY_MS = DEFAULT_SIDEBAR_TOOLTIP_DELAY_MS;

/** Session/browser cards and project headers wait another 700ms before opening. */
export const SIDEBAR_ITEM_TOOLTIP_DELAY_MS = TOOLTIP_DELAY_MS + 700;

/** Fixed action-button labels historically use the shorter app-wide delay. */
export const SIDEBAR_FIXED_TOOLTIP_DELAY_OFFSET_MS = BASE_TOOLTIP_DELAY_MS - TOOLTIP_DELAY_MS;

const SidebarTooltipDelayContext = createContext(TOOLTIP_DELAY_MS);

export function SidebarTooltipDelayProvider({ children, delayMs }: { children: ReactNode; delayMs: number }) {
  return createElement(
    SidebarTooltipDelayContext.Provider,
    { value: delayMs },
    createElement(TooltipProvider, { delayDuration: delayMs }, children)
  );
}

export function useSidebarTooltipDelayMs(offsetMs = 0): number {
  return Math.max(0, useContext(SidebarTooltipDelayContext) + offsetMs);
}

export function useSidebarItemTooltipDelayMs(): number {
  return useSidebarTooltipDelayMs(SIDEBAR_ITEM_TOOLTIP_DELAY_MS - TOOLTIP_DELAY_MS);
}
