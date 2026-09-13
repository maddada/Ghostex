export type TranscriptMaterializationReason = 'search' | 'selection';
export const TRANSCRIPT_RENDERED_EVENT = 'ghostex-transcript-rendered';

const controllers = new WeakMap<HTMLElement, (reason: TranscriptMaterializationReason, active: boolean) => void>();

export function registerTranscriptMaterialization(
  viewport: HTMLElement,
  update: (reason: TranscriptMaterializationReason, active: boolean) => void
) {
  controllers.set(viewport, update);
  return () => {
    controllers.delete(viewport);
  };
}

export function materializeSessionChatTranscript(
  root: HTMLElement | null,
  reason: TranscriptMaterializationReason,
  active: boolean
) {
  const viewport = root?.querySelector<HTMLElement>('[data-slot="message-scroller-viewport"]');
  if (viewport) controllers.get(viewport)?.(reason, active);
}

export function transcriptRowElements(content: HTMLElement): HTMLElement[] {
  return Array.from(content.children).filter(
    (child): child is HTMLElement => child instanceof HTMLElement && child.hasAttribute('data-message-id')
  );
}

export function captureTranscriptAnchor(viewport: HTMLElement, content: HTMLElement) {
  const top = viewport.getBoundingClientRect().top;
  const row = transcriptRowElements(content).find((element) => element.getBoundingClientRect().bottom > top);
  return row ? { id: row.dataset.messageId!, offset: row.getBoundingClientRect().top - top } : null;
}
