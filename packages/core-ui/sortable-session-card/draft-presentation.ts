import type { SidebarSessionItem } from '../../shared/session-grid-contract';

export function sidebarSessionComposerDraftPresentation(session: Pick<SidebarSessionItem, 'hasComposerDraft'>): {
  hasComposerDraft: boolean;
} {
  return { hasComposerDraft: session.hasComposerDraft === true };
}
