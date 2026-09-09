import { IconBrowser, IconCopy, IconExternalLink, IconFolder } from '@tabler/icons-react';
import { ContextMenuItem } from '@/packages/components/ui/context-menu';
import { classifySessionChatLinkHref, useSessionChatHostLinks } from './session-chat-links';

/**
 * CDXC:SessionChat 2026-09-09 DECISION:
 * User: reference pills offer Copy Path and Locate File, while URLs offer Copy URL and opening in the embedded or external browser.
 */
export function SessionChatReferenceMenuItems({ href }: { href: string }) {
  const links = useSessionChatHostLinks();
  const target = classifySessionChatLinkHref(href);
  const copy = (text: string): void => {
    void navigator.clipboard.writeText(text).catch((error: unknown) => {
      console.error('[session-chat] reference clipboard write failed', error);
    });
  };
  if (target.kind === 'url') {
    return (
      <>
        <ContextMenuItem onClick={() => copy(target.url)}>
          <IconCopy aria-hidden='true' />
          Copy URL
        </ContextMenuItem>
        {links?.openUrl ? (
          <>
            <ContextMenuItem onClick={() => links.openUrl?.(target.url, { external: false, forceEmbedded: true })}>
              <IconBrowser aria-hidden='true' />
              Open in Embedded Browser
            </ContextMenuItem>
            <ContextMenuItem onClick={() => links.openUrl?.(target.url, { external: true })}>
              <IconExternalLink aria-hidden='true' />
              Open in External Browser
            </ContextMenuItem>
          </>
        ) : null}
      </>
    );
  }
  if (target.kind !== 'file') return null;
  return (
    <>
      <ContextMenuItem onClick={() => copy(target.path)}>
        <IconCopy aria-hidden='true' />
        Copy Path
      </ContextMenuItem>
      {links?.locateFile ? (
        <ContextMenuItem onClick={() => links.locateFile?.(target.path)}>
          <IconFolder aria-hidden='true' />
          Locate File
        </ContextMenuItem>
      ) : null}
    </>
  );
}
