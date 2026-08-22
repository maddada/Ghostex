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

import { sessionChatFilePathIcon } from "./session-chat-file-paths";

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
      if (node.type === "code" && typeof node.meta === "string") {
        const meta = node.meta.trim();
        if (meta !== "") {
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
  const code = children?.find(
    (child) => child?.type === "element" && child.tagName === "code",
  );
  const meta = code?.properties?.dataCodeMeta;
  return typeof meta === "string" && meta !== "" ? meta : null;
}

const FENCE_TITLE_ATTRIBUTE =
  /(?:^|\s)(?:title|file(?:name)?)=(?:"([^"]+)"|'([^']+)'|(\S+))/i;
/*
 * A bare token only counts as a filename when it reads like one: something
 * before a dot, an extension after it, and nothing but path characters in
 * between. Prose meta such as `showLineNumbers` or `{1,3-5}` must stay out of
 * the header, and so must a version number sitting on its own.
 */
const FENCE_FILENAME_TOKEN = /^[\w@][\w@./-]*\.[A-Za-z0-9]+$/;

/** The filename this fence names, or null when it names none. */
export function sessionChatFenceTitle(meta: string | null): string | null {
  if (meta === null) {
    return null;
  }
  const attribute = FENCE_TITLE_ATTRIBUTE.exec(meta);
  const named = attribute?.[1] ?? attribute?.[2] ?? attribute?.[3];
  if (named) {
    return named;
  }
  return meta.split(/\s+/).find((token) => FENCE_FILENAME_TOKEN.test(token)) ?? null;
}

/**
 * The same three-glyph vocabulary the inline-code file chips use, so a path
 * named by a fence and the same path named mid-sentence carry one icon.
 */
export function sessionChatFenceTitleIcon(title: string) {
  const separator = Math.max(title.lastIndexOf("/"), title.lastIndexOf("\\"));
  return sessionChatFilePathIcon(title.slice(separator + 1));
}
