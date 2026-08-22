// @ts-nocheck
import { EditorState } from '@codemirror/state';
import { parseFrontmatter, isInsideFrontmatter } from './frontmatter';
import { resolvedSyntaxTree } from './markdownSyntax';
import { collectInlineFootnoteMarkerRanges } from './inlineFootnotes';

export interface FootnoteReference {
  from: number;
  to: number;
  containerFrom: number;
  containerTo: number;
  label: string;
  normalizedLabel: string;
  number: number | null;
  definition: FootnoteDefinition | null;
}

export interface FootnoteDefinition {
  label: string;
  normalizedLabel: string;
  markerFrom: number;
  markerTo: number;
  colonFrom: number;
  colonTo: number;
  contentFrom: number;
  contentTo: number;
  lineFrom: number;
  lineTo: number;
  number: number | null;
  firstReferenceFrom: number | null;
  continuationLines: FootnoteContinuationLine[];
  isPrimary: boolean;
}

export interface FootnoteContinuationLine {
  from: number;
  to: number;
  hideIndentFrom: number | null;
  hideIndentTo: number | null;
  extraIndentColumns: number;
}

export interface ParsedFootnotes {
  references: FootnoteReference[];
  definitions: FootnoteDefinition[];
  numberByLabel: Map<string, number>;
  referencesByContainerKey: Map<string, FootnoteReference[]>;
}

interface ProtectedRange {
  from: number;
  to: number;
}

const footnoteCache = new WeakMap<object, ParsedFootnotes>();
const definitionMarkerPattern = /^[ \t]{0,3}\[\^([^\]\r\n]+)\]:(?:[ \t]|$)/;

export function normalizeFootnoteLabel(rawLabel: string): string {
  return String(rawLabel ?? '')
    .trim()
    .replace(/\s+/g, ' ')
    .toLowerCase();
}

export function parseFootnotes(state: EditorState): ParsedFootnotes {
  const cached = footnoteCache.get(state.doc);
  if (cached) {
    return cached;
  }

  const frontmatter = parseFrontmatter(state);
  const tree = resolvedSyntaxTree(state);
  const protectedRanges = collectProtectedRanges(state, tree);
  const definitions = collectDefinitions(state, frontmatter, protectedRanges);
  const definitionMarkerToByFrom = new Map<number, number>();
  const primaryDefinitions = new Map<string, FootnoteDefinition>();

  for (const definition of definitions) {
    definitionMarkerToByFrom.set(definition.markerFrom, definition.markerTo);
    if (!definition.isPrimary) {
      continue;
    }
    primaryDefinitions.set(definition.normalizedLabel, definition);
  }

  const references = collectReferences(state, tree, frontmatter, protectedRanges, primaryDefinitions, definitionMarkerToByFrom);
  const numberByLabel = new Map<string, number>();
  let nextNumber = 1;

  for (const reference of references) {
    if (!reference.definition) {
      continue;
    }

    let footnoteNumber = numberByLabel.get(reference.normalizedLabel);
    if (!footnoteNumber) {
      footnoteNumber = nextNumber;
      nextNumber += 1;
      numberByLabel.set(reference.normalizedLabel, footnoteNumber);
    }

    reference.number = footnoteNumber;
  }

  for (const definition of definitions) {
    if (!definition.isPrimary) {
      continue;
    }
    definition.number = numberByLabel.get(definition.normalizedLabel) ?? null;
  }

  const firstReferenceByLabel = new Map<string, number>();
  for (const reference of references) {
    if (!reference.definition || firstReferenceByLabel.has(reference.normalizedLabel)) {
      continue;
    }
    firstReferenceByLabel.set(reference.normalizedLabel, reference.from);
  }

  for (const definition of definitions) {
    if (!definition.isPrimary) {
      continue;
    }
    definition.firstReferenceFrom = firstReferenceByLabel.get(definition.normalizedLabel) ?? null;
  }

  const referencesByContainerKey = new Map<string, FootnoteReference[]>();
  for (const reference of references) {
    const containerKey = footnoteReferenceKey(reference.containerFrom, reference.containerTo);
    const containerRefs = referencesByContainerKey.get(containerKey);
    if (containerRefs) {
      containerRefs.push(reference);
      continue;
    }
    referencesByContainerKey.set(containerKey, [reference]);
  }

  const parsed: ParsedFootnotes = {
    references,
    definitions,
    numberByLabel,
    referencesByContainerKey
  };

  footnoteCache.set(state.doc, parsed);
  return parsed;
}

export function footnoteReferenceKey(from: number, to: number): string {
  return `${from}:${to}`;
}

function collectProtectedRanges(state: EditorState, tree: any): ProtectedRange[] {
  const ranges: ProtectedRange[] = [];

  tree.iterate({
    enter(node: any) {
      if (node.name === 'FencedCode' || node.name === 'CodeBlock') {
        ranges.push({ from: node.from, to: node.to });
      }
    }
  });

  ranges.sort((a, b) => a.from - b.from);
  return ranges;
}

function collectDefinitions(
  state: EditorState,
  frontmatter: ReturnType<typeof parseFrontmatter>,
  protectedRanges: ProtectedRange[]
): FootnoteDefinition[] {
  const definitions: FootnoteDefinition[] = [];
  const seenLabels = new Set<string>();

  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    const line = state.doc.line(lineNo);
    if (isInsideFrontmatter(frontmatter, line.from) || isInsideProtectedRange(line.from, protectedRanges)) {
      continue;
    }

    const markerMatch = definitionMarkerPattern.exec(line.text);
    if (!markerMatch) {
      continue;
    }

    const label = markerMatch[1];
    const normalizedLabel = normalizeFootnoteLabel(label);
    if (!normalizedLabel) {
      continue;
    }

    const markerText = markerMatch[0];
    const colonOffset = markerText.lastIndexOf(':');
    let endLineNo = lineNo;
    const continuationLines: FootnoteContinuationLine[] = [];

    while (endLineNo < state.doc.lines) {
      const nextLine = state.doc.line(endLineNo + 1);
      if (isInsideFrontmatter(frontmatter, nextLine.from) || isInsideProtectedRange(nextLine.from, protectedRanges)) {
        break;
      }
      if (definitionMarkerPattern.test(nextLine.text)) {
        break;
      }
      if (!nextLine.text.trim()) {
        continuationLines.push({
          from: nextLine.from,
          to: nextLine.to,
          hideIndentFrom: null,
          hideIndentTo: null,
          extraIndentColumns: 0
        });
        endLineNo += 1;
        continue;
      }

      const continuationIndent = measureContinuationIndent(nextLine.text);
      if (!continuationIndent) {
        break;
      }

      continuationLines.push({
        from: nextLine.from,
        to: nextLine.to,
        hideIndentFrom: nextLine.from,
        hideIndentTo: nextLine.from + continuationIndent.chars,
        extraIndentColumns: continuationIndent.extraIndentColumns
      });
      endLineNo += 1;
    }

    const contentFrom = line.from + markerText.length;
    const endLine = state.doc.line(endLineNo);
    const definition: FootnoteDefinition = {
      label,
      normalizedLabel,
      markerFrom: line.from + markerText.indexOf('[^'),
      markerTo: line.from + markerText.length,
      colonFrom: line.from + colonOffset,
      colonTo: line.from + colonOffset + 1,
      contentFrom,
      contentTo: line.to,
      lineFrom: line.from,
      lineTo: endLine.to,
      number: null,
      firstReferenceFrom: null,
      continuationLines,
      isPrimary: !seenLabels.has(normalizedLabel)
    };

    definitions.push(definition);
    seenLabels.add(normalizedLabel);
    lineNo = endLineNo;
  }

  return definitions;
}

function collectReferences(
  state: EditorState,
  tree: any,
  frontmatter: ReturnType<typeof parseFrontmatter>,
  protectedRanges: ProtectedRange[],
  definitions: Map<string, FootnoteDefinition>,
  definitionMarkerToByFrom: Map<number, number>
): FootnoteReference[] {
  const references: FootnoteReference[] = [];

  tree.iterate({
    enter(node: any) {
      if (node.name !== 'Link') {
        return;
      }
      if (isInsideFrontmatter(frontmatter, node.from) || isInsideProtectedRange(node.from, protectedRanges)) {
        return;
      }

      if (findChildNode(node, 'URL')) {
        return;
      }
      if (isDefinitionMarkerNode(node.from, node.to, definitionMarkerToByFrom)) {
        return;
      }

      const rawText = state.doc.sliceString(node.from, node.to);
      const markerRanges = collectInlineFootnoteMarkerRanges(rawText);
      if (!markerRanges.length) {
        return;
      }

      for (const markerRange of markerRanges) {
        const normalizedLabel = normalizeFootnoteLabel(markerRange.label);
        if (!normalizedLabel) {
          continue;
        }
        const definition = definitions.get(normalizedLabel);
        if (!definition) {
          continue;
        }

        references.push({
          from: node.from + markerRange.fromOffset,
          to: node.from + markerRange.toOffset,
          containerFrom: node.from,
          containerTo: node.to,
          label: markerRange.label,
          normalizedLabel,
          number: null,
          definition
        });
      }
    }
  });

  return references;
}

function isDefinitionMarkerNode(from: number, to: number, definitionMarkerToByFrom: Map<number, number>): boolean {
  const markerTo = definitionMarkerToByFrom.get(from);
  return markerTo !== undefined && to <= markerTo;
}

function measureContinuationIndent(lineText: string): { chars: number; extraIndentColumns: number } | null {
  let visibleIndent = 0;
  let chars = 0;

  while (chars < lineText.length) {
    const ch = lineText[chars];
    if (ch === ' ') {
      visibleIndent += 1;
      chars += 1;
    } else if (ch === '\t') {
      visibleIndent += 4 - (visibleIndent % 4);
      chars += 1;
    } else {
      break;
    }
  }

  if (visibleIndent < 2) {
    return null;
  }

  return {
    chars,
    extraIndentColumns: Math.max(0, visibleIndent - 2)
  };
}

function isInsideProtectedRange(pos: number, ranges: ProtectedRange[]): boolean {
  for (const range of ranges) {
    if (pos < range.from) {
      return false;
    }
    if (pos >= range.from && pos < range.to) {
      return true;
    }
  }
  return false;
}

function findChildNode(node: any, name: string): any {
  const syntaxNode = node?.node ?? node;
  if (!syntaxNode?.firstChild) {
    return null;
  }

  for (let child = syntaxNode.firstChild; child; child = child.nextSibling) {
    if (child.name === name) {
      return child;
    }
  }

  return null;
}
