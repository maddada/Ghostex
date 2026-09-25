/*
 * Inline code that is a file reference, promoted to a clickable chip.
 *
 * Agents write `packages/core-ui/styles/chat.css:913` and `apps/desktop/src/cef/shell.rs:42:8`
 * constantly, and today every one of them is an inert grey span. This module
 * decides which inline-code spans are actually file references, and hands the
 * renderer the pieces it needs to draw one: the path to open, the line/column
 * to open it at, and where that path may be cut if the column is too narrow to
 * show all of it.
 *
 * The bar for saying "yes" is deliberately high. A false positive is worse than
 * a miss: it turns a piece of ordinary prose code — `npm install`, `--flag`,
 * `Array.map`, `origin/main` — into something that looks like a link and then
 * fails to open. So a span only becomes a chip when it carries real path
 * evidence, and everything below is written to say no first.
 *
 * The rule, in order:
 *
 *  1. One token only. Anything with whitespace or a backtick in it is a
 *     command line or a sentence, not a path.
 *  2. Every character has to be path-shaped. Each `/`-separated segment must be
 *     `[A-Za-z0-9._+@~-]+`, which rejects `foo.bar()`, `array[0]`, `a|b`,
 *     `src/**\/*.ts`, `key=value`, `mailto:x`, and `https://…` outright — a
 *     stray `:` that is not a trailing `:line[:column]` is disqualifying, and
 *     so is a backslash outside a Windows path.
 *  3. It has to look like more than a word: either it contains a directory
 *     separator (or announces itself with `/`, `./`, `../`, `~/`, `C:\`, `\\`)
 *     or it carries a trailing `:line`. A bare `README.md` therefore stays
 *     plain inline code — the same call the reference implementation makes,
 *     because agents name files in prose far more often than they mean "open
 *     this". (This is the one clause a fenced block's title is exempt from —
 *     see resolveSessionChatFenceTitleFilePath.)
 *  4. It must not be a host or a version. `example.com/x.html`, `localhost`,
 *     and `1.2.3` are not files.
 *  5. Its basename must name a file: a letter-initial extension (`.ts`, `.rs`,
 *     `.zshrc`) or a conventional extensionless filename (`Makefile`,
 *     `Dockerfile`, `README`). A digit-initial "extension" is a version
 *     number, not a file type, so `release/v1.2` and `p99.9` are refused.
 *
 * Relative paths are handed to the host exactly as the agent wrote them: the
 * chat surface has no cwd of its own, and the host that owns an editor is the
 * one that knows the project root (gpui resolves them against the active
 * project in open_session_chat_file). Resolving here would mean guessing a
 * root, which is precisely the wrong move.
 */

import { IconFile, IconFileCode, IconMarkdown } from '@tabler/icons-react';

import { sessionChatFilePositionSuffix } from '@/packages/core-ui/chat/presentation/file-position';
export {
  splitSessionChatFilePosition,
  sessionChatFilePositionSuffix,
  type SessionChatFilePosition,
} from '@/packages/core-ui/chat/presentation/file-position';
// The decisions themselves live in the shared presentation package so the GPUI
// transcript can reach them without React; this module keeps the React icons
// and the remark passes that feed react-markdown.
import {
  sessionChatBareFilePaths,
  sessionChatFilePathIconName,
  type SessionChatFilePathRef,
} from '@/packages/core-ui/chat/presentation/file-paths';
export {
  resolveSessionChatFenceTitleFilePath,
  resolveSessionChatInlineCodeFilePath,
  type SessionChatFilePathRef,
} from '@/packages/core-ui/chat/presentation/file-paths';

/** Exposes a file chip's unadorned path to the transcript context menu. */
export const SESSION_CHAT_FILE_PATH_ATTRIBUTE = 'data-session-chat-file-path';

/**
 * The chip's visible text, split where it is allowed to be cut.
 *
 * `apps/desktop/src/cef/shell.rs:42:8` becomes parent `apps/desktop/src/cef` and
 * name `/shell.rs:42:8`. The chip shows the whole path, so in a
 * narrow transcript column something has to give when it does not fit — and
 * the part that may go is the tail of the parent. The leading folder says
 * which of the repo's worlds this is, the filename says what it is, and the
 * coordinates say where; the directories in between are the only part a reader
 * can lose and still know what they are looking at.
 *
 * The separator goes with the name, not with the parent, so that a truncated
 * parent still ends in one: `apps/desktop/src/c…/shell.rs` reads as a path with a hole
 * in it, while `apps/desktop/src/c…shell.rs` reads as a typo.
 */
export function sessionChatFilePathChipLabel(ref: SessionChatFilePathRef): {
  name: string;
  parent: string;
} {
  // Split the path as written rather than by its normalized segments, so a
  // Windows path keeps its backslashes both on screen and in the split.
  const separatorIndex = Math.max(ref.path.lastIndexOf('/'), ref.path.lastIndexOf('\\'));
  return {
    name: `${separatorIndex < 0 ? ref.path : ref.path.slice(separatorIndex)}${sessionChatFilePositionSuffix(ref.position)}`,
    parent: separatorIndex < 0 ? '' : ref.path.slice(0, separatorIndex),
  };
}

/** The path plus its coordinates, as the tooltip and the title attribute show it. */
export function sessionChatFilePathTitle(ref: SessionChatFilePathRef): string {
  return `${ref.path}${sessionChatFilePositionSuffix(ref.position)}`;
}

/*
 * Ghostex has no file-type icon set of its own — the Docs tree keeps a private
 * three-way switch and nothing else — so rather than pull in a new icon
 * dependency for this, the chip picks from @tabler/icons-react, which is
 * already the house set. Three glyphs is the whole vocabulary: prose, source,
 * and everything else; which of the three a basename gets is decided in the
 * shared module so the GPUI transcript draws the same glyph.
 */
const FILE_PATH_ICONS = { file: IconFile, 'file-code': IconFileCode, markdown: IconMarkdown } as const;

export function sessionChatFilePathIcon(basename: string): typeof IconFile {
  return FILE_PATH_ICONS[sessionChatFilePathIconName(basename)];
}

/*
 * Marks every inline-code node so the shared `code` renderer can tell an inline
 * span from a fence's body — react-markdown hands both to the same component,
 * and only the inline ones may become chips. Spans inside a link are skipped:
 * the link already decides where that text goes.
 */
interface MarkdownAstNode {
  children?: MarkdownAstNode[];
  data?: {
    hProperties?: Record<string, unknown>;
  };
  type?: string;
  url?: string;
  value?: unknown;
}

const NON_PROSE_CONTAINERS = new Set(['code', 'definition', 'html', 'inlineCode', 'link', 'linkReference']);

function taggedInlineCode(value: string): MarkdownAstNode {
  return {
    type: 'inlineCode',
    value,
    data: { hProperties: { dataInlineCode: '' } },
  };
}

/**
 * Promotes plain file paths in text somebody typed into the composer to the
 * same tagged inline-code node an agent-authored `path/to/file.ts` span uses.
 * The renderer therefore draws the exact same FileChip and invokes the exact
 * same host open-file route for both roles; this pass only supplies the AST
 * node that ordinary prose otherwise lacks.
 *
 * Links and existing code are left alone. Candidate discovery only splits on
 * whitespace and sentence punctuation; resolveSessionChatInlineCodeFilePath
 * still makes every implicit path/not-path decision.
 *
 * CDXC:SessionChat 2026-09-06 DECISION:
 * User: @file mentions must follow all the same chat rules as [File #N](path) references, including images and other files.
 * Explicit mentions become link nodes so image previews, file opening, positions, and context menus all use the attachment renderer; the @ marker is never part of the destination.
 */
export function remarkSessionChatBareFilePaths() {
  return (tree: MarkdownAstNode): void => {
    const visit = (node: MarkdownAstNode): void => {
      const children = node.children;
      if (!children) return;

      const rebuilt: MarkdownAstNode[] = [];
      let changed = false;
      for (const child of children) {
        if (child.type !== 'text' || typeof child.value !== 'string') {
          if (!NON_PROSE_CONTAINERS.has(child.type ?? '')) {
            visit(child);
          }
          rebuilt.push(child);
          continue;
        }

        const value = child.value;
        let cursor = 0;
        for (const found of sessionChatBareFilePaths(value)) {
          changed = true;
          if (found.start > cursor) {
            rebuilt.push({ type: 'text', value: value.slice(cursor, found.start) });
          }
          rebuilt.push(
            found.mention
              ? { type: 'link', url: found.path, children: [{ type: 'text', value: found.path }] }
              : taggedInlineCode(found.path)
          );
          cursor = found.end;
        }
        if (cursor < value.length) {
          rebuilt.push({ type: 'text', value: value.slice(cursor) });
        }
      }
      if (changed) node.children = rebuilt;
    };
    visit(tree);
  };
}

export function remarkSessionChatInlineCode() {
  return (tree: MarkdownAstNode) => {
    const visit = (node: MarkdownAstNode, insideLink: boolean) => {
      if (node.type === 'inlineCode' && !insideLink) {
        node.data = {
          ...node.data,
          hProperties: { ...node.data?.hProperties, dataInlineCode: '' },
        };
      }
      const childInsideLink = insideLink || node.type === 'link' || node.type === 'linkReference';
      node.children?.forEach((child) => visit(child, childInsideLink));
    };
    visit(tree, false);
  };
}
