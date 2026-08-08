import { TOOLTIP_DELAY_MS as BASE_TOOLTIP_DELAY_MS } from "../components/ui/tooltip-config";

/** Sidebar tooltips intentionally wait 300ms longer than the app-wide default. */
export const TOOLTIP_DELAY_MS = BASE_TOOLTIP_DELAY_MS + 300;
