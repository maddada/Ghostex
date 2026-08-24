import * as React from 'react';

/**
 * CDXC:ReactDropdowns 2026-05-30-08:58:
 * Expanded React dropdown surfaces should show the same visible border as
 * sidebar tooltips, so menus, selects, popovers, and tooltips share the
 * tooltip border token instead of each primitive inventing its own outline.
 */
const overlayTooltipBorderStyle: React.CSSProperties = {
  border: '1px solid var(--ghostex-tooltip-border, rgba(255, 255, 255, 0.12))',
};

const tooltipSurfaceStyle: React.CSSProperties = {
  background: 'var(--ghostex-tooltip-background, rgba(24, 24, 24, 0.98))',
  ...overlayTooltipBorderStyle,
  borderRadius: 'var(--ghostex-tooltip-radius, 5px)',
  boxShadow: 'var(--ghostex-tooltip-shadow, 0 12px 30px rgba(0, 0, 0, 0.35))',
  color: 'var(--ghostex-tooltip-foreground, rgba(255, 255, 255, 0.78))',
  font: 'var(--ghostex-tooltip-font, 500 12px/1.35 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif)',
};

export { overlayTooltipBorderStyle, tooltipSurfaceStyle };
