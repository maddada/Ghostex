// @ts-nocheck
import { ensureSyntaxTree, syntaxTree } from '@codemirror/language';
import { EditorState } from '@codemirror/state';

export function resolvedSyntaxTree(state: EditorState, timeout: number = 50): any {
  return ensureSyntaxTree(state, state.doc.length, timeout) ?? syntaxTree(state);
}

export function headingLevelFromName(name: string): number | null {
  if (!name.startsWith('ATXHeading')) {
    return null;
  }

  const level = Number.parseInt(name.slice('ATXHeading'.length), 10);
  return Number.isInteger(level) && level >= 1 && level <= 6 ? level : null;
}

export interface HeadingInfo {
  level: number;
  text: string;
  line: number;
  from: number;
}

export interface HeadingSection extends HeadingInfo {
  sectionFrom: number;
  sectionTo: number;
  headingFrom: number;
  headingTo: number;
  lineFrom: number;
  lineTo: number;
  collapseFrom: number;
  collapseTo: number;
}

export interface DetailsBlockInfo {
  kind: 'details';
  anchorFrom: number;
  anchorTo: number;
  summaryFrom: number;
  summaryTo: number;
  lineFrom: number;
  lineTo: number;
  sectionFrom: number;
  sectionTo: number;
  bodyFrom: number;
  bodyTo: number;
  closingFrom: number;
  closingTo: number;
  summaryText: string;
  defaultCollapsed: boolean;
}

const detailsOpenTagPattern = /<details\b[^>]*>/i;
const detailsCloseTagPattern = /<\/details\s*>/i;
const summaryTagPattern = /<summary\b[^>]*>([\s\S]*?)<\/summary\s*>/i;

export function extractHeadings(state: EditorState): HeadingInfo[] {
  const headings: HeadingInfo[] = [];
  const tree = resolvedSyntaxTree(state);

  tree.iterate({
    enter(node: any) {
      const headingLevel = headingLevelFromName(node.name);
      if (headingLevel !== null) {
        const line = state.doc.lineAt(node.from);
        let text = state.doc.sliceString(node.from, node.to);
        text = text
          .replace(/^#{1,6}\s+/, '')
          .replace(/\s+#+$/, '')
          .trim();
        headings.push({
          level: headingLevel,
          text,
          line: line.number,
          from: node.from,
        });
      }
    },
  });

  return headings;
}

function hasDetailsOpenAttribute(openTag: string): boolean {
  const attributeText = openTag.replace(/^<details\b/i, '').replace(/>$/, '');
  return /(?:^|[\s/])open(?=[\s=/>]|$)/i.test(attributeText);
}

function normalizeSummaryText(rawText: string | undefined): string {
  const text = String(rawText ?? '')
    .replace(/<[^>]+>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  return text || 'Details';
}

export function extractDetailsBlocks(state: EditorState): DetailsBlockInfo[] {
  const detailsBlocks: DetailsBlockInfo[] = [];
  const pendingBlocks: Array<{
    anchorFrom: number;
    anchorTo: number;
    summaryFrom: number;
    summaryTo: number;
    lineFrom: number;
    lineTo: number;
    summaryText: string;
    defaultCollapsed: boolean;
  }> = [];
  const tree = resolvedSyntaxTree(state);

  tree.iterate({
    enter(node: any) {
      if (node.name !== 'HTMLBlock') {
        return;
      }

      const rawText = state.doc.sliceString(node.from, node.to);
      const openTagMatch = rawText.match(detailsOpenTagPattern);
      if (openTagMatch) {
        const openingLine = state.doc.lineAt(node.from);
        const summaryMatch = rawText.match(summaryTagPattern);
        const openerTo =
          typeof summaryMatch?.index === 'number'
            ? node.from + summaryMatch.index + summaryMatch[0].length
            : openingLine.to;
        const summaryFrom = typeof summaryMatch?.index === 'number' ? node.from + summaryMatch.index : node.from;
        const summaryTo = typeof summaryMatch?.index === 'number' ? openerTo : openingLine.to;
        pendingBlocks.push({
          anchorFrom: node.from,
          anchorTo: openerTo,
          summaryFrom,
          summaryTo,
          lineFrom: openingLine.from,
          lineTo: openingLine.to,
          summaryText: normalizeSummaryText(summaryMatch?.[1]),
          defaultCollapsed: !hasDetailsOpenAttribute(openTagMatch[0]),
        });
      }

      if (!detailsCloseTagPattern.test(rawText)) {
        return;
      }

      const openBlock = pendingBlocks.pop();
      if (!openBlock) {
        return;
      }

      const closingLine = state.doc.lineAt(node.from);

      detailsBlocks.push({
        kind: 'details',
        anchorFrom: openBlock.anchorFrom,
        anchorTo: openBlock.anchorTo,
        summaryFrom: openBlock.summaryFrom,
        summaryTo: openBlock.summaryTo,
        lineFrom: openBlock.lineFrom,
        lineTo: openBlock.lineTo,
        sectionFrom: openBlock.anchorFrom,
        sectionTo: closingLine.to,
        bodyFrom: openBlock.anchorTo,
        bodyTo: node.from,
        closingFrom: node.from,
        closingTo: closingLine.to,
        summaryText: openBlock.summaryText,
        defaultCollapsed: openBlock.defaultCollapsed,
      });
    },
  });

  detailsBlocks.sort((a, b) => a.anchorFrom - b.anchorFrom);
  return detailsBlocks;
}

export function extractHeadingSections(state: EditorState): HeadingSection[] {
  const headings: HeadingSection[] = [];
  const tree = resolvedSyntaxTree(state);

  tree.iterate({
    enter(node: any) {
      const headingLevel = headingLevelFromName(node.name);
      if (headingLevel === null) {
        return;
      }

      const line = state.doc.lineAt(node.from);
      let text = state.doc.sliceString(node.from, node.to);
      text = text
        .replace(/^#{1,6}\s+/, '')
        .replace(/\s+#+$/, '')
        .trim();
      headings.push({
        level: headingLevel,
        text,
        from: node.from,
        line: line.number,
        sectionFrom: node.from,
        sectionTo: state.doc.length,
        headingFrom: node.from,
        headingTo: node.to,
        lineFrom: line.from,
        lineTo: line.to,
        collapseFrom: line.to,
        collapseTo: state.doc.length,
      });
    },
  });

  for (let index = 0; index < headings.length; index += 1) {
    const heading = headings[index];
    for (let nextIndex = index + 1; nextIndex < headings.length; nextIndex += 1) {
      const nextHeading = headings[nextIndex];
      if (nextHeading.level <= heading.level) {
        heading.sectionTo = nextHeading.headingFrom;
        const previousLineNo = Math.max(heading.line, nextHeading.line - 1);
        heading.collapseTo = state.doc.line(previousLineNo).to;
        break;
      }
    }
  }

  return headings;
}
