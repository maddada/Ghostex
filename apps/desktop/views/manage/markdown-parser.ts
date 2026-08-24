import { ManageMarkdownAlertKind, ManageMarkdownBlock } from './types';

export const MANAGE_MARKDOWN_HTML_BLOCK_TAGS = new Set([
  'article',
  'aside',
  'blockquote',
  'details',
  'div',
  'figure',
  'footer',
  'form',
  'h1',
  'h2',
  'h3',
  'h4',
  'h5',
  'h6',
  'header',
  'hr',
  'main',
  'nav',
  'ol',
  'p',
  'pre',
  'section',
  'table',
  'ul',
]);

export function parseManageMarkdownToBlocks(markdown: string): ManageMarkdownBlock[] {
  const body = extractManageMarkdownBody(markdown);
  const lines = body.split('\n');
  const blocks: ManageMarkdownBlock[] = [];
  let index = 0;
  let order = 0;

  const pushBlock = (
    type: ManageMarkdownBlock['type'],
    content: string,
    startLine: number,
    extra: Partial<ManageMarkdownBlock> = {}
  ) => {
    blocks.push({
      content,
      id: `manage-md-block-${order}-${startLine}`,
      order,
      startLine,
      type,
      ...extra,
    });
    order += 1;
  };

  while (index < lines.length) {
    const line = lines[index] ?? '';
    const startLine = index + 1;
    if (!line.trim()) {
      index += 1;
      continue;
    }

    const heading = line.match(/^(#{1,6})\s+(.+?)\s*#*\s*$/u);
    if (heading) {
      pushBlock('heading', heading[2] ?? '', startLine, { level: heading[1]?.length ?? 1 });
      index += 1;
      continue;
    }

    if (/^\s{0,3}(?:([-*_])(?:\s*\1){2,})\s*$/u.test(line)) {
      pushBlock('hr', '', startLine);
      index += 1;
      continue;
    }

    const directive = line.match(/^\s*:::\s*([A-Za-z][\w-]*)\s*$/u);
    if (directive) {
      const contentLines: string[] = [];
      index += 1;
      while (index < lines.length && !/^\s*:::\s*$/u.test(lines[index] ?? '')) {
        contentLines.push(lines[index] ?? '');
        index += 1;
      }
      if (index < lines.length) {
        index += 1;
      }
      pushBlock('directive', contentLines.join('\n').trim(), startLine, {
        directiveKind: directive[1]?.toLocaleLowerCase(),
      });
      continue;
    }

    const fence = line.match(/^\s{0,3}(`{3,}|~{3,})(.*)$/u);
    if (fence) {
      const marker = fence[1] ?? '```';
      const markerChar = marker[0] ?? '`';
      const markerLength = marker.length;
      const language = (fence[2] ?? '').trim().split(/\s+/u)[0] ?? '';
      const contentLines: string[] = [];
      index += 1;
      while (index < lines.length) {
        const close = (lines[index] ?? '').match(/^\s{0,3}(`{3,}|~{3,})\s*$/u);
        if (close && close[1]?.[0] === markerChar && close[1].length >= markerLength) {
          index += 1;
          break;
        }
        contentLines.push(lines[index] ?? '');
        index += 1;
      }
      pushBlock('code', contentLines.join('\n'), startLine, { language });
      continue;
    }

    if (/^\s{0,3}>\s?/u.test(line)) {
      const quoteLines: string[] = [];
      while (index < lines.length && /^\s{0,3}>\s?/u.test(lines[index] ?? '')) {
        quoteLines.push((lines[index] ?? '').replace(/^\s{0,3}>\s?/u, ''));
        index += 1;
      }
      const alert = quoteLines[0]?.trim().match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*$/iu);
      if (alert) {
        pushBlock('blockquote', quoteLines.slice(1).join('\n').trim(), startLine, {
          alertKind: alert[1]?.toLocaleLowerCase() as ManageMarkdownAlertKind,
        });
      } else {
        pushBlock('blockquote', quoteLines.join('\n').trim(), startLine);
      }
      continue;
    }

    if (isManageMarkdownTableStart(lines, index)) {
      const tableLines = [line, lines[index + 1] ?? ''];
      index += 2;
      while (index < lines.length && lineHasUnescapedPipe(lines[index] ?? '')) {
        tableLines.push(lines[index] ?? '');
        index += 1;
      }
      pushBlock('table', tableLines.join('\n'), startLine);
      continue;
    }

    const list = line.match(/^(\s*)([-*+]|\d+[.)])\s+(\[[ xX]\]\s+)?(.*)$/u);
    if (list) {
      const marker = list[2] ?? '-';
      const checkbox = list[3];
      const contentLines = [list[4] ?? ''];
      const indentLength = expandManageMarkdownIndent(list[1] ?? '').length;
      index += 1;
      while (index < lines.length) {
        const nextLine = lines[index] ?? '';
        if (!nextLine.trim() || isManageMarkdownBlockStart(lines, index)) {
          break;
        }
        if (expandManageMarkdownIndent(nextLine).length > indentLength) {
          contentLines.push(nextLine.trim());
          index += 1;
          continue;
        }
        break;
      }
      const orderedStartMatch = marker.match(/^(\d+)/u);
      pushBlock('list-item', contentLines.join('\n').trim(), startLine, {
        checked: checkbox ? /\[[xX]\]/u.test(checkbox) : undefined,
        level: Math.floor(indentLength / 2),
        ordered: Boolean(orderedStartMatch),
        orderedStart: orderedStartMatch ? Number(orderedStartMatch[1]) : undefined,
      });
      continue;
    }

    const htmlTag = line.match(/^\s{0,3}<([A-Za-z][\w-]*)(?:\s|>|\/>)/u)?.[1]?.toLocaleLowerCase();
    if (htmlTag && MANAGE_MARKDOWN_HTML_BLOCK_TAGS.has(htmlTag)) {
      const htmlLines = [line];
      index += 1;
      if (!line.includes(`</${htmlTag}>`) && !/\/>\s*$/u.test(line)) {
        while (index < lines.length) {
          const nextLine = lines[index] ?? '';
          if (!nextLine.trim()) {
            break;
          }
          htmlLines.push(nextLine);
          index += 1;
          if (nextLine.includes(`</${htmlTag}>`)) {
            break;
          }
        }
      }
      pushBlock('html', htmlLines.join('\n'), startLine);
      continue;
    }

    const paragraphLines = [line.trim()];
    index += 1;
    while (index < lines.length && lines[index]?.trim() && !isManageMarkdownBlockStart(lines, index)) {
      paragraphLines.push((lines[index] ?? '').trim());
      index += 1;
    }
    pushBlock('paragraph', paragraphLines.join(' '), startLine);
  }

  return blocks;
}

export function extractManageMarkdownBody(markdown: string): string {
  const normalized = markdown.replace(/\r\n?/gu, '\n');
  const frontmatter = normalized.match(/^---[ \t]*\n[\s\S]*?\n---[ \t]*(?:\n|$)/u);
  return frontmatter ? normalized.slice(frontmatter[0].length) : normalized;
}

export function isManageMarkdownBlockStart(lines: string[], index: number): boolean {
  const line = lines[index] ?? '';
  if (!line.trim()) {
    return false;
  }
  return (
    /^(#{1,6})\s+/u.test(line) ||
    /^\s{0,3}(?:([-*_])(?:\s*\1){2,})\s*$/u.test(line) ||
    /^\s*:::\s*([A-Za-z][\w-]*)\s*$/u.test(line) ||
    /^\s{0,3}(`{3,}|~{3,})/u.test(line) ||
    /^\s{0,3}>\s?/u.test(line) ||
    /^(\s*)([-*+]|\d+[.)])\s+/u.test(line) ||
    isManageMarkdownTableStart(lines, index) ||
    Boolean(line.match(/^\s{0,3}<([A-Za-z][\w-]*)(?:\s|>|\/>)/u)?.[1])
  );
}

export function isManageMarkdownTableStart(lines: string[], index: number): boolean {
  const line = lines[index] ?? '';
  const divider = lines[index + 1] ?? '';
  return lineHasUnescapedPipe(line) && isManageMarkdownTableDivider(divider);
}

export function isManageMarkdownTableDivider(line: string): boolean {
  return /^\s*\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)+\|?\s*$/u.test(line);
}

export function lineHasUnescapedPipe(line: string): boolean {
  return /(^|[^\\])\|/u.test(line);
}

export function expandManageMarkdownIndent(value: string): string {
  return value.replace(/\t/gu, '    ');
}

export function computeManageOrderedListIndices(blocks: ManageMarkdownBlock[]): Map<string, number> {
  const indices = new Map<string, number>();
  const counters = new Map<number, number>();
  for (const block of blocks) {
    if (block.type !== 'list-item') {
      counters.clear();
      continue;
    }
    const level = block.level ?? 0;
    for (const counterLevel of Array.from(counters.keys())) {
      if (counterLevel > level) {
        counters.delete(counterLevel);
      }
    }
    if (!block.ordered) {
      counters.delete(level);
      continue;
    }
    const nextIndex = counters.has(level) ? (counters.get(level) ?? 0) + 1 : (block.orderedStart ?? 1);
    counters.set(level, nextIndex);
    indices.set(block.id, nextIndex);
  }
  return indices;
}

export function parseManageMarkdownTableContent(content: string): { headers: string[]; rows: string[][] } {
  const lines = content.split('\n').filter((line) => line.trim());
  const parseRow = (line: string): string[] =>
    line
      .replace(/^\s*\|/u, '')
      .replace(/\|\s*$/u, '')
      .split(/(?<!\\)\|/u)
      .map((cell) => cell.trim().replace(/\\\|/gu, '|'));
  const headers = lines[0] ? parseRow(lines[0]) : [];
  const rows = lines.slice(2).map(parseRow);
  return { headers, rows };
}
