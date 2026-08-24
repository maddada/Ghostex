// @ts-nocheck
import { EditorState } from '@codemirror/state';

export interface MermaidColonBlock {
  startLine: number;
  endLine: number;
  from: number;
  to: number;
  fullBlockText: string;
  diagramText: string;
}

const standardFenceLineRe = /^[ \t]{0,3}([`~]{3,})/;
const mermaidColonOpenLineRe = /^[ \t]{0,3}(:{3,})\s*mermaid\s*$/i;

const mermaidColonBlockCache = new WeakMap<EditorState, MermaidColonBlock[]>();

export function rangeOverlapsMermaidColonBlock(
  blocks: ReadonlyArray<{ from: number; to: number }>,
  from: number,
  to: number
): boolean {
  return blocks.some((block) => from < block.to && to > block.from);
}

function parseStandardFenceLine(lineText: string): { char: '`' | '~'; length: number } | null {
  const match = standardFenceLineRe.exec(lineText);
  if (!match) {
    return null;
  }

  const marker = match[1];
  const char = marker[0] as '`' | '~';
  if (char !== '`' && char !== '~') {
    return null;
  }

  return { char, length: marker.length };
}

function parseMermaidColonOpenLine(lineText: string): number | null {
  const match = mermaidColonOpenLineRe.exec(lineText);
  if (!match) {
    return null;
  }
  return match[1].length;
}

function isMermaidColonCloseLine(lineText: string, colonCount: number): boolean {
  return colonCount >= 3 && new RegExp(`^[ \\t]{0,3}:{${colonCount},}\\s*$`).test(lineText);
}

export function getMermaidColonBlocks(state: EditorState): MermaidColonBlock[] {
  const cached = mermaidColonBlockCache.get(state);
  if (cached) {
    return cached;
  }

  const blocks: MermaidColonBlock[] = [];
  let standardFence: { char: '`' | '~'; length: number } | null = null;
  let pending: {
    startLine: number;
    colonCount: number;
    contentLines: string[];
  } | null = null;

  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    const line = state.doc.line(lineNo);
    const lineText = state.doc.sliceString(line.from, line.to);

    if (pending) {
      if (isMermaidColonCloseLine(lineText, pending.colonCount)) {
        const startLine = pending.startLine;
        const endLine = lineNo;
        const from = state.doc.line(startLine).from;
        const to = line.to;

        blocks.push({
          startLine,
          endLine,
          from,
          to,
          fullBlockText: state.doc.sliceString(from, to),
          diagramText: pending.contentLines.join('\n'),
        });
        pending = null;
        continue;
      }

      pending.contentLines.push(lineText);
      continue;
    }

    const standardFenceLine = parseStandardFenceLine(lineText);
    if (standardFenceLine) {
      if (!standardFence) {
        standardFence = standardFenceLine;
      } else if (standardFence.char === standardFenceLine.char && standardFenceLine.length >= standardFence.length) {
        standardFence = null;
      }
      continue;
    }

    if (standardFence) {
      continue;
    }

    const mermaidOpenColonCount = parseMermaidColonOpenLine(lineText);
    if (!mermaidOpenColonCount) {
      continue;
    }

    pending = {
      startLine: lineNo,
      colonCount: mermaidOpenColonCount,
      contentLines: [],
    };
  }

  mermaidColonBlockCache.set(state, blocks);
  return blocks;
}
