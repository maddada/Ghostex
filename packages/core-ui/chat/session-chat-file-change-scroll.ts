/** CDXC:SessionChat 2026-09-10 WHY:
 * Expanding can push the header offscreen when the transcript follows resized content; a pre-collapse-only check missed that movement.
 * Check after React and the scroller's resize frame settle, and leave an already visible header in place.
 */
export function revealSessionChatFileChangeHeader(header: HTMLElement | null): void {
  if (!header) return;
  window.requestAnimationFrame(() => {
    window.requestAnimationFrame(() => {
      if (!header.isConnected) return;
      const viewport = header.closest<HTMLElement>('[data-slot="message-scroller-viewport"]');
      const viewportBounds = viewport?.getBoundingClientRect();
      const top = viewport && viewportBounds ? Math.max(0, viewportBounds.top + viewport.clientTop) : 0;
      const bottom = viewport && viewportBounds
        ? Math.min(window.innerHeight, viewportBounds.top + viewport.clientTop + viewport.clientHeight)
        : window.innerHeight;
      const bounds = header.getBoundingClientRect();
      if (bounds.top < top || bounds.bottom > bottom) {
        header.scrollIntoView({ behavior: 'instant', block: 'center', inline: 'nearest' });
      }
    });
  });
}
