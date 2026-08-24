import { TOOLTIP_DELAY_MS as BASE_TOOLTIP_DELAY_MS } from '../components/ui/tooltip-config';

/** Sidebar tooltips intentionally wait 300ms longer than the app-wide default. */
export const TOOLTIP_DELAY_MS = BASE_TOOLTIP_DELAY_MS + 300;

/** Session/browser cards and project headers wait another 700ms before opening. */
export const SIDEBAR_ITEM_TOOLTIP_DELAY_MS = TOOLTIP_DELAY_MS + 700;
