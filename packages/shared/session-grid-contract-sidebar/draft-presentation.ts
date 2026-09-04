export type SidebarSessionDraftPresentation = {
  /**
   * CDXC:Drafts 2026-09-04 DECISION:
   * User: the chat composer holds unsent text for this session. Draws the white
   * composer-draft dot on the leading agent icon; absent means no dot.
   */
  hasComposerDraft?: boolean;
};
