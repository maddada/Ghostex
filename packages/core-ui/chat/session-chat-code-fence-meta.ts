/*
 * Fence meta: everything an agent writes after the language on a ``` line.
 *
 *     ```ts title="src/main.ts"
 *     ```ts file=src/main.ts
 *     ```ts filename=src/main.ts
 *     ```ts src/main.ts
 *
 * All four name the file the block came from, and agents use all four. The
 * header shows that name instead of the bare language when it is there, which
 * is the difference between "ts" and "the file you are about to edit".
 *
 * mdast keeps the meta string on the code node, but mdast-util-to-hast does not
 * turn it into anything the renderer can read, so remarkSessionChatCodeMeta
 * copies it onto the fence's <code> as a data property first (the same trick
 * remarkSessionChatInlineCode uses to mark inline spans) and
 * sessionChatFenceMeta reads it back off the <pre> node react-markdown hands
 * the block renderer.
 */

import { sessionChatFilePathIcon } from './session-chat-file-paths';

interface MarkdownAstNode {
  children?: MarkdownAstNode[];
  data?: {
    hProperties?: Record<string, unknown>;
  };
  meta?: unknown;
  type?: string;
}

export function remarkSessionChatCodeMeta() {
  return (tree: MarkdownAstNode) => {
    const visit = (node: MarkdownAstNode) => {
      if (node.type === 'code' && typeof node.meta === 'string') {
        const meta = node.meta.trim();
        if (meta !== '') {
          node.data = {
            ...node.data,
            hProperties: { ...node.data?.hProperties, dataCodeMeta: meta },
          };
        }
      }
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}

/** The hast shape of the <pre> react-markdown renders a fence as. */
interface FenceHastNode {
  children?: readonly {
    properties?: { dataCodeMeta?: unknown };
    tagName?: string;
    type?: string;
  }[];
}

/** The meta string of the fence this <pre> node came from, if it had one. */
export function sessionChatFenceMeta(node: unknown): string | null {
  const children = (node as FenceHastNode | undefined)?.children;
  const code = children?.find((child) => child?.type === 'element' && child.tagName === 'code');
  const meta = code?.properties?.dataCodeMeta;
  return typeof meta === 'string' && meta !== '' ? meta : null;
}

/**
 * The filename this fence names, or null when it names none. Decided in the
 * shared presentation package so the GPUI code-block header shows the same
 * name this one does.
 */
export { sessionChatFenceTitle } from '@/packages/core-ui/chat/presentation/file-paths';

/**
 * The same three-glyph vocabulary the inline-code file chips use, so a path
 * named by a fence and the same path named mid-sentence carry one icon.
 */
export function sessionChatFenceTitleIcon(title: string) {
  const separator = Math.max(title.lastIndexOf('/'), title.lastIndexOf('\\'));
  return sessionChatFilePathIcon(title.slice(separator + 1));
}
