// @ts-nocheck
import { RangeSetBuilder, StateField, EditorState } from '@codemirror/state';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { syntaxHighlighting } from '@codemirror/language';
import { Decoration, EditorView, GutterMarker, WidgetType, gutterLineClass } from '@codemirror/view';
import { createElement, AlertCircle, Delete } from 'lucide';
import {
  resolveCodeLanguage,
  isFenceMarker,
  getFencedCodeInfo,
  addFenceOpeningLineMarker,
  addCodeLanguageLabel,
  addTopLineCopyButton,
  addTopLinePillLabel,
  addMermaidDiagram,
  addMermaidDiagramBlock,
  addCopyCodeButton,
} from './helpers/codeBlocks';
import { ImageWidget, getImageData, isImageUrl } from './helpers/images';
import { highlightStyle } from './theme';
import { collectSingleTildeStrikePairs, collectStrikethroughRanges } from './helpers/strikeMarkers';
import { collectEmojiRangesFromText } from './helpers/emoji';
import { collectKbdTagRangesFromText, hasKbdTagMarker } from './helpers/kbd';
import { headingLevelFromName, resolvedSyntaxTree } from './helpers/markdownSyntax';
import {
  headingCollapseLiveExtensions,
  headingCollapseSharedExtensions,
  getCollapsedHeadingSections,
  getDetailsBlocks,
  toggleCollapsibleSection,
} from './helpers/headingCollapse';
import {
  addListMarkerDecoration,
  listMarkerData,
  detectListIndentStylesByLine,
  nextOrderedSequenceNumber,
} from './helpers/listMarkers';
import {
  addTableDecorations,
  addTableDecorationsForLineRange,
  isTableDelimiterLine,
  parseTableInfo,
} from './helpers/tables';
import {
  forEachYamlFrontmatterField,
  parseFrontmatter,
  parseSimpleYamlFlowArrayValue,
  isInsideFrontmatter,
  isInsideFrontmatterContent,
  isThematicBreakLine,
} from './helpers/frontmatter';
import { isWikiLinkNode, parseWikiLinkData, getWikiLinkStatus } from './helpers/wikiLinks';
import { getLocalLinkStatus, normalizeLocalLinkTarget, isLikelyLocalLinkTarget } from './helpers/localLinks';
import { mergeConflictSourceExtensions, parseMergeConflicts } from './helpers/mergeConflicts';
import { AlertType, AlertIconWidget, detectAlertInBlockquote } from './helpers/alerts';
import { parseFootnotes, footnoteReferenceKey } from './helpers/footnotes';
import { getLiveRenderedBlocks, type LiveRenderedBlock } from './helpers/liveRenderedBlocks';
import { getMermaidColonBlocks, rangeOverlapsMermaidColonBlock } from './helpers/mermaidColonBlocks';
import { findRawSourceUrlMatches, normalizeSourceHref } from './helpers/rawUrls';
import { trimDecoratedUrlRange } from './helpers/urlDecorationRange';
import { collectInlineFootnoteMarkerRanges } from './helpers/inlineFootnotes';
import {
  collectLatexMathRanges,
  renderLatexMathToHtml,
  resolveFencedDisplayMathInnerLineRange,
  type LatexMathRange,
  type LatexMathMode,
} from './helpers/math';
import { diagnosticDataField } from './helpers/diagnostics';

const markerDeco = Decoration.mark({ class: 'meo-md-marker' });
const activeLineMarkerDeco = Decoration.mark({ class: 'meo-md-marker-active' });
const frontmatterBoundaryMarkerDeco = Decoration.mark({ class: 'meo-md-frontmatter-boundary-marker' });
const linkMarkerDeco = Decoration.mark({ class: 'meo-md-marker meo-md-link-marker' });
const activeLinkMarkerDeco = Decoration.mark({ class: 'meo-md-marker-active meo-md-link-marker-active' });
const linkLabelBracketDeco = Decoration.mark({
  class: 'meo-md-link-label-bracket',
  attributes: {
    style: 'color: var(--meo-color-base02) !important; -webkit-text-fill-color: var(--meo-color-base02) !important;',
  },
});
const activeLinkLabelBracketDeco = Decoration.mark({
  class: 'meo-md-link-label-bracket-active',
  attributes: {
    style: 'color: var(--meo-color-base02) !important; -webkit-text-fill-color: var(--meo-color-base02) !important;',
  },
});
const footnoteMarkerDeco = Decoration.mark({
  class: 'meo-md-footnote-marker',
  attributes: {
    style: 'color: var(--meo-color-base02) !important; -webkit-text-fill-color: var(--meo-color-base02) !important;',
  },
});
const footnoteLiteralDeco = Decoration.mark({ class: 'meo-md-footnote-literal' });
const wikiLinkMarkerDeco = Decoration.mark({ class: 'meo-md-marker meo-md-link-marker meo-md-wiki-marker' });
const activeWikiLinkMarkerDeco = Decoration.mark({
  class: 'meo-md-marker-active meo-md-link-marker-active meo-md-wiki-marker',
});
const emptyWikiLinkMarkerDeco = Decoration.mark({
  class: 'meo-md-marker meo-md-link-marker meo-md-wiki-marker meo-md-wiki-empty-marker',
});
const strikeMarkerDeco = Decoration.mark({ class: 'meo-md-marker meo-md-strike-marker' });
const activeStrikeMarkerDeco = Decoration.mark({ class: 'meo-md-marker-active meo-md-strike-marker-active' });
const codeMarkerDeco = Decoration.mark({ class: 'meo-md-code-marker' });
const activeCodeMarkerDeco = Decoration.mark({ class: 'meo-md-code-marker-active' });
const fenceMarkerDeco = Decoration.mark({ class: 'meo-md-fence-marker' });
const hrMarkerDeco = Decoration.mark({ class: 'meo-md-hr-marker' });
const hiddenLinkUrlDeco = Decoration.mark({ class: 'meo-md-link-url-hidden' });
const linkBoundaryDeco = Decoration.mark({ class: 'meo-md-url-boundary' });
const collapsedHeadingBodyDeco = Decoration.replace({
  inclusiveStart: false,
  inclusiveEnd: false,
});
const collapsedHeadingLineDeco = Decoration.line({ class: 'meo-md-heading-collapsed' });
const tableDelimiterGutterLineClassMarker = new (class extends GutterMarker {
  elementClass = 'meo-md-hide-line-number';
})();
const isTableContentLine = (lineText: string): boolean => lineText.includes('|');

const lineStyleDecos = {
  h1: Decoration.line({ class: 'meo-md-h1' }),
  h2: Decoration.line({ class: 'meo-md-h2' }),
  h3: Decoration.line({ class: 'meo-md-h3' }),
  h4: Decoration.line({ class: 'meo-md-h4' }),
  h5: Decoration.line({ class: 'meo-md-h5' }),
  h6: Decoration.line({ class: 'meo-md-h6' }),
  detailsSummary: Decoration.line({ class: 'meo-md-details-summary-line' }),
  quote: Decoration.line({ class: 'meo-md-quote' }),
  mergeIncomingHeader: Decoration.line({ class: 'meo-merge-line meo-merge-incoming-header' }),
  codeBlock: Decoration.line({ class: 'meo-md-code-block' }),
  footnote: Decoration.line({ class: 'meo-md-footnote-line' }),
  footnoteContinuation: Decoration.line({ class: 'meo-md-footnote-line meo-md-footnote-continuation' }),
  frontmatterContent: Decoration.line({ class: 'meo-md-frontmatter-content' }),
  frontmatterBoundary: Decoration.line({ class: 'meo-md-hr meo-md-frontmatter-boundary' }),
  hrActive: Decoration.line({ class: 'meo-md-hr-active' }),
  hr: Decoration.line({ class: 'meo-md-hr' }),
};

const alertLineDecos: Record<AlertType, ReturnType<typeof Decoration.line>> = {
  NOTE: Decoration.line({ class: 'meo-md-alert meo-md-alert-note' }),
  TIP: Decoration.line({ class: 'meo-md-alert meo-md-alert-tip' }),
  IMPORTANT: Decoration.line({ class: 'meo-md-alert meo-md-alert-important' }),
  WARNING: Decoration.line({ class: 'meo-md-alert meo-md-alert-warning' }),
  CAUTION: Decoration.line({ class: 'meo-md-alert meo-md-alert-caution' }),
};

const alertMarkerDeco = Decoration.mark({ class: 'meo-md-alert-marker' });
const alertLabelActiveDeco = Decoration.mark({ class: 'meo-md-alert-label-active' });
const hiddenAlertMarkerDeco = Decoration.mark({ class: 'meo-md-alert-marker-hidden' });
const frontmatterKeyDeco = Decoration.mark({ class: 'meo-md-frontmatter-key' });
const frontmatterValueDeco = Decoration.mark({ class: 'meo-md-frontmatter-value' });
const mergeConflictMarkerPrefixes = ['<<<<<<<', '|||||||', '=======', '>>>>>>>'];
const fileSchemePrefix = 'file:';
const rawFileUrlBlockedAncestorNames = new Set([
  'Link',
  'Autolink',
  'URL',
  'Image',
  'InlineCode',
  'CodeText',
  'FencedCode',
  'CodeBlock',
  'HTMLTag',
  'HTMLBlock',
  'Table',
]);

const listLineDecoCache = new Map();
const listIndentWidgetCache = new Map();
const frontmatterArrayPillWidgetCache = new Map();

function isMergeConflictMarkerLine(state, pos) {
  const line = state.doc.lineAt(pos);
  const lineText = state.doc.sliceString(line.from, line.to).trimStart();
  return mergeConflictMarkerPrefixes.some((prefix) => lineText.startsWith(prefix));
}

class ListIndentWidget extends WidgetType {
  indentColumns: number;

  constructor(indentColumns: number) {
    super();
    this.indentColumns = indentColumns;
  }

  eq(other: WidgetType): boolean {
    return other instanceof ListIndentWidget && other.indentColumns === this.indentColumns;
  }

  toDOM(): HTMLElement {
    const spacer = document.createElement('span');
    spacer.className = 'meo-md-list-indent-spacer';
    spacer.style.width = `${Math.max(0, this.indentColumns)}ch`;
    return spacer;
  }
}

function listIndentWidget(indentColumns) {
  const normalized = Math.max(0, Math.round(indentColumns));
  let widget = listIndentWidgetCache.get(normalized);
  if (widget) {
    return widget;
  }
  widget = new ListIndentWidget(normalized);
  listIndentWidgetCache.set(normalized, widget);
  return widget;
}

class FootnoteBackrefSpacerWidget extends WidgetType {
  footnoteNumber: number;

  constructor(footnoteNumber: number) {
    super();
    this.footnoteNumber = footnoteNumber;
  }

  eq(other: WidgetType): boolean {
    return other instanceof FootnoteBackrefSpacerWidget && other.footnoteNumber === this.footnoteNumber;
  }

  toDOM(): HTMLElement {
    const marker = document.createElement('span');
    marker.className = 'meo-md-footnote-backref meo-md-footnote-backref-spacer';
    marker.textContent = `${this.footnoteNumber}.`;
    marker.setAttribute('aria-hidden', 'true');
    return marker;
  }
}

class FootnoteReferenceSeparatorWidget extends WidgetType {
  eq(other: WidgetType): boolean {
    return other instanceof FootnoteReferenceSeparatorWidget;
  }

  toDOM(): HTMLElement {
    const sep = document.createElement('span');
    sep.className = 'meo-md-footnote-separator';
    sep.textContent = ',';
    sep.setAttribute('aria-hidden', 'true');
    return sep;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

const footnoteReferenceSeparatorWidget = new FootnoteReferenceSeparatorWidget();

function listLineDeco(
  contentOffsetColumns,
  indentColumns,
  guideStepColumns = 2,
  selected = false,
  isTask = false,
  taskHiddenPrefixColumns = 0
) {
  const offset = Math.max(0, contentOffsetColumns);
  const indent = Math.max(0, indentColumns);
  const guideStep = Math.max(2, guideStepColumns);
  const hiddenTaskPrefix = Math.max(0, taskHiddenPrefixColumns);
  const key = `${offset}:${indent}:${guideStep}:${selected ? 1 : 0}:${isTask ? 1 : 0}:${hiddenTaskPrefix}`;
  let deco = listLineDecoCache.get(key);
  if (deco) {
    return deco;
  }

  const classes = ['meo-md-list-line'];
  if (selected) {
    classes.push('meo-md-list-line-selected');
  }
  if (isTask) {
    classes.push('meo-md-list-line-task');
  }

  deco = Decoration.line({
    class: classes.join(' '),
    attributes: {
      style: `--meo-list-hanging-indent:${offset}ch;--meo-list-indent-columns:${indent}ch;--meo-list-guide-step:${guideStep}ch;--meo-task-hidden-prefix-columns:${hiddenTaskPrefix}ch;`,
    },
  });
  listLineDecoCache.set(key, deco);
  return deco;
}

const inlineStyleDecos = {
  em: Decoration.mark({ class: 'meo-md-em' }),
  strong: Decoration.mark({ class: 'meo-md-strong' }),
  strike: Decoration.mark({ class: 'meo-md-strike' }),
  inlineCode: Decoration.mark({ class: 'meo-md-inline-code' }),
};

function addFrontmatterBoundaryDecorations(builder, state, frontmatter, activeLines) {
  if (frontmatter.contentTo > frontmatter.contentFrom) {
    addLineClass(builder, state, frontmatter.contentFrom, frontmatter.contentTo, lineStyleDecos.frontmatterContent);
    forEachYamlFrontmatterField(state, frontmatter, ({ line, keyFrom, keyTo, valueFrom, valueTo }) => {
      addRange(builder, keyFrom, keyTo, frontmatterKeyDeco);
      if (valueFrom !== null && valueFrom < valueTo) {
        const lineIsActive = activeLines.has(line.number);
        const selectionOverlapsValue = overlapsSelection(state, valueFrom, valueTo);
        const parsedArrayValue =
          !lineIsActive && !selectionOverlapsValue
            ? parseSimpleYamlFlowArrayValue(line.text, valueFrom - line.from)
            : null;

        if (parsedArrayValue) {
          builder.push(
            Decoration.replace({
              widget: frontmatterArrayPillsWidget(parsedArrayValue.items.map((item) => item.text)),
              inclusive: false,
            }).range(line.from + parsedArrayValue.fromOffset, line.from + parsedArrayValue.toOffset)
          );
          return;
        }

        addRange(builder, valueFrom, valueTo, frontmatterValueDeco);
      }
    });
  }

  const boundaries = [
    { from: frontmatter.openingFrom, to: frontmatter.openingTo, isOpening: true },
    { from: frontmatter.closingFrom, to: frontmatter.closingTo },
  ];

  for (const boundary of boundaries) {
    addLineClass(builder, state, boundary.from, boundary.to, lineStyleDecos.frontmatterBoundary);
    const lineNo = state.doc.lineAt(boundary.from).number;
    const boundarySelected = overlapsSelection(state, boundary.from, boundary.to);
    if (activeLines.has(lineNo) || boundarySelected) {
      addLineClass(builder, state, boundary.from, boundary.to, lineStyleDecos.hrActive);
      addRange(builder, boundary.from, boundary.to, activeLineMarkerDeco);
    } else {
      if (boundary.isOpening) {
        const line = state.doc.lineAt(boundary.from);
        addTopLinePillLabel(builder, line.to, 'frontmatter');
        addRange(builder, boundary.from, boundary.to, frontmatterBoundaryMarkerDeco);
      }
    }
  }
}

function addThematicBreakDecorations(builder, state, from, to, activeLines) {
  addLineClass(builder, state, from, to, lineStyleDecos.hr);
  const lineNo = state.doc.lineAt(from).number;
  if (activeLines.has(lineNo)) {
    addLineClass(builder, state, from, to, lineStyleDecos.hrActive);
    addRange(builder, from, to, activeLineMarkerDeco);
  } else {
    addRange(builder, from, to, hrMarkerDeco);
  }
}

function addForcedThematicBreakDecorations(builder, state, activeLines, frontmatter, codeBlockLines = null) {
  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    const line = state.doc.line(lineNo);
    if (!isThematicBreakLine(line.text) || isInsideFrontmatter(frontmatter, line.from) || codeBlockLines?.has(lineNo)) {
      continue;
    }
    addThematicBreakDecorations(builder, state, line.from, line.to, activeLines);
  }
}

function getNodeHref(state, node) {
  const href = state.doc.sliceString(node.from, node.to).trim();
  return normalizeSourceHref(href);
}

function addLinkMark(builder, from, to, href) {
  if (!href) {
    return;
  }
  addRange(
    builder,
    from,
    to,
    Decoration.mark({
      class: 'meo-md-link',
      attributes: { 'data-meo-link-href': href },
    })
  );
}

function addTrimmedUrlLinkMark(builder, from, to, rawUrl, href) {
  if (!href) {
    return;
  }
  const range = trimDecoratedUrlRange(from, to, rawUrl, href);
  if (from < range.from) {
    addRange(builder, from, range.from, linkBoundaryDeco);
  }
  if (range.to < to) {
    addRange(builder, range.to, to, linkBoundaryDeco);
  }
  addLinkMark(builder, range.from, range.to, href);
}

function findChildNode(node, name) {
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

class ClearLinkUrlWidget extends WidgetType {
  urlFrom: number;
  urlTo: number;

  constructor(urlFrom: number, urlTo: number) {
    super();
    this.urlFrom = urlFrom;
    this.urlTo = urlTo;
  }

  eq(other: WidgetType): boolean {
    return other instanceof ClearLinkUrlWidget && other.urlFrom === this.urlFrom && other.urlTo === this.urlTo;
  }

  toDOM(): HTMLElement {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'meo-md-link-clear-btn';
    button.title = 'Clear link URL';
    button.setAttribute('aria-label', 'Clear link URL');
    button.appendChild(createElement(Delete, { 'aria-hidden': 'true' }));
    button.addEventListener('pointerdown', (event) => {
      event.preventDefault();
      event.stopPropagation();
    });
    button.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      const view = EditorView.findFromDOM(button);
      if (!view) {
        return;
      }
      view.dispatch({
        changes: { from: this.urlFrom, to: this.urlTo, insert: '' },
        selection: { anchor: this.urlFrom },
      });
      view.focus();
    });
    return button;
  }

  ignoreEvent(): boolean {
    return false;
  }
}

class MissingWikiLinkWidget extends WidgetType {
  eq(other) {
    return other instanceof MissingWikiLinkWidget;
  }

  toDOM(): HTMLElement {
    const badge = document.createElement('span');
    badge.className = 'meo-md-wiki-missing-icon';
    badge.title = 'Wiki link target not found locally';
    badge.setAttribute('aria-label', 'Wiki link target not found locally');
    badge.appendChild(createElement(AlertCircle, { 'aria-hidden': 'true' }));
    return badge;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

class MissingLocalLinkWidget extends WidgetType {
  eq(other) {
    return other instanceof MissingLocalLinkWidget;
  }

  toDOM(): HTMLElement {
    const badge = document.createElement('span');
    badge.className = 'meo-md-local-link-missing-icon';
    badge.title = 'Local file link target not found';
    badge.setAttribute('aria-label', 'Local file link target not found');
    badge.appendChild(createElement(AlertCircle, { 'aria-hidden': 'true' }));
    return badge;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

class FrontmatterArrayPillsWidget extends WidgetType {
  itemLabels: string[];
  cacheKey: string;

  constructor(itemLabels: string[], cacheKey: string) {
    super();
    this.itemLabels = itemLabels;
    this.cacheKey = cacheKey;
  }

  eq(other: WidgetType): boolean {
    return other instanceof FrontmatterArrayPillsWidget && other.cacheKey === this.cacheKey;
  }

  toDOM(): HTMLElement {
    const container = document.createElement('span');
    container.className = 'meo-md-frontmatter-array-pills';
    container.setAttribute('aria-hidden', 'true');
    for (const labelText of this.itemLabels) {
      const pill = document.createElement('span');
      pill.className = 'meo-md-frontmatter-pill';
      pill.textContent = labelText;
      container.appendChild(pill);
    }
    return container;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

function frontmatterArrayPillsWidget(itemLabels) {
  const cacheKey = JSON.stringify(itemLabels);
  let widget = frontmatterArrayPillWidgetCache.get(cacheKey);
  if (widget) {
    return widget;
  }
  widget = new FrontmatterArrayPillsWidget(itemLabels, cacheKey);
  frontmatterArrayPillWidgetCache.set(cacheKey, widget);
  return widget;
}

class DetailsSummaryWidget extends WidgetType {
  anchor: number;
  lineFrom: number;
  summaryText: string;
  collapsed: boolean;

  constructor(anchor: number, lineFrom: number, summaryText: string, collapsed: boolean) {
    super();
    this.anchor = anchor;
    this.lineFrom = lineFrom;
    this.summaryText = summaryText;
    this.collapsed = collapsed;
  }

  eq(other: WidgetType): boolean {
    return (
      other instanceof DetailsSummaryWidget &&
      other.anchor === this.anchor &&
      other.lineFrom === this.lineFrom &&
      other.summaryText === this.summaryText &&
      other.collapsed === this.collapsed
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'meo-md-details-summary';
    button.title = this.collapsed ? 'Expand details' : 'Collapse details';
    button.setAttribute('aria-label', this.collapsed ? 'Expand details' : 'Collapse details');

    const label = document.createElement('span');
    label.className = 'meo-md-details-summary-label';
    label.textContent = this.summaryText;
    button.appendChild(label);

    button.addEventListener('pointerdown', (event) => {
      event.preventDefault();
      event.stopPropagation();
    });

    button.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      toggleCollapsibleSection(view, this.anchor);
    });

    return button;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

class FootnoteReferenceWidget extends WidgetType {
  footnoteNumber: number;
  definitionFrom: number;

  constructor(footnoteNumber: number, definitionFrom: number) {
    super();
    this.footnoteNumber = footnoteNumber;
    this.definitionFrom = definitionFrom;
  }

  eq(other: WidgetType): boolean {
    return (
      other instanceof FootnoteReferenceWidget &&
      other.footnoteNumber === this.footnoteNumber &&
      other.definitionFrom === this.definitionFrom
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'meo-md-footnote-ref';
    button.title = `Jump to footnote ${this.footnoteNumber}`;
    button.setAttribute('aria-label', `Jump to footnote ${this.footnoteNumber}`);

    const number = document.createElement('sup');
    number.textContent = String(this.footnoteNumber);
    button.appendChild(number);

    button.addEventListener('pointerdown', (event) => {
      event.preventDefault();
      event.stopPropagation();
    });

    button.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      view.dispatch({
        selection: { anchor: this.definitionFrom },
        effects: EditorView.scrollIntoView(this.definitionFrom, { y: 'center' }),
      });
      view.focus();
    });

    return button;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

class FootnoteBacklinkWidget extends WidgetType {
  footnoteNumber: number;
  referenceFrom: number;

  constructor(footnoteNumber: number, referenceFrom: number) {
    super();
    this.footnoteNumber = footnoteNumber;
    this.referenceFrom = referenceFrom;
  }

  eq(other: WidgetType): boolean {
    return (
      other instanceof FootnoteBacklinkWidget &&
      other.footnoteNumber === this.footnoteNumber &&
      other.referenceFrom === this.referenceFrom
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'meo-md-footnote-backref';
    button.title = `Jump to footnote reference ${this.footnoteNumber}`;
    button.setAttribute('aria-label', `Jump to footnote reference ${this.footnoteNumber}`);
    button.textContent = `${this.footnoteNumber}.`;

    button.addEventListener('pointerdown', (event) => {
      event.preventDefault();
      event.stopPropagation();
    });

    button.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      view.dispatch({
        selection: { anchor: this.referenceFrom },
        effects: EditorView.scrollIntoView(this.referenceFrom, { y: 'center' }),
      });
      view.focus();
    });

    return button;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

function addMarkdownLinkDecorations(builder, state, node, activeLines) {
  const urlNode = findChildNode(node, 'URL');
  if (!urlNode) {
    return;
  }

  const prefix = state.doc.sliceString(node.from, urlNode.from);
  const closeTextAt = prefix.lastIndexOf('](');
  if (closeTextAt <= 0) {
    return;
  }

  const textFrom = node.from + 1;
  const textTo = node.from + closeTextAt;
  if (textFrom >= textTo) {
    return;
  }
  const href = getNodeHref(state, urlNode);
  addLinkMark(builder, textFrom, textTo, href);

  const localTarget = normalizeLocalLinkTarget(href);
  const localTargetStatus = isLikelyLocalLinkTarget(localTarget) ? getLocalLinkStatus(localTarget) : null;
  if (localTarget && localTargetStatus === false) {
    const iconPos = textFrom < textTo ? textFrom : node.from + 1;
    builder.push(
      Decoration.widget({
        widget: new MissingLocalLinkWidget(),
        side: -1,
      }).range(iconPos)
    );
  }

  if (!href) {
    return;
  }
  const urlLine = state.doc.lineAt(urlNode.from);
  const isActiveLine = activeLines.has(urlLine.number);
  if (!isActiveLine) {
    addRange(builder, urlNode.from, urlNode.to, hiddenLinkUrlDeco);
    return;
  }

  builder.push(
    Decoration.widget({
      widget: new ClearLinkUrlWidget(urlNode.from, urlNode.to),
      side: 1,
    }).range(urlNode.to)
  );
}

function addFootnoteReferenceDecorations(builder, state, reference, activeLines): boolean {
  if (!shouldRenderFootnoteReference(state, reference, activeLines)) {
    return false;
  }

  builder.push(
    Decoration.replace({
      widget: new FootnoteReferenceWidget(reference.number as number, reference.definition!.lineFrom),
      inclusive: false,
    }).range(reference.from, reference.to)
  );

  return true;
}

function shouldRenderFootnoteReference(state, reference, activeLines): boolean {
  const line = state.doc.lineAt(reference.from);
  const editingReference = activeLines.has(line.number) || overlapsSelection(state, reference.from, reference.to);
  return !editingReference && Boolean(reference.number) && Boolean(reference.definition);
}

function addInlineFootnoteMarkerSyntaxDecorations(
  builder,
  containerFrom: number,
  markerRanges: Array<{ label: string; fromOffset: number; toOffset: number }>
) {
  for (const markerRange of markerRanges) {
    const markerFrom = containerFrom + markerRange.fromOffset;
    const markerTo = containerFrom + markerRange.toOffset;
    if (markerTo - markerFrom < 3) {
      continue;
    }

    addRange(builder, markerFrom, markerFrom + 1, footnoteMarkerDeco);
    addRange(builder, markerFrom + 1, markerFrom + 2, footnoteMarkerDeco);
    addRange(builder, markerTo - 1, markerTo, footnoteMarkerDeco);
  }
}

function getEmptyImageLinkUrl(state, node) {
  const urlNode = findChildNode(node, 'URL');
  if (!urlNode) {
    return '';
  }

  const prefix = state.doc.sliceString(node.from, urlNode.from);
  const closeTextAt = prefix.lastIndexOf('](');
  if (closeTextAt < 1) {
    return '';
  }

  const textFrom = node.from + 1;
  const textTo = node.from + closeTextAt;
  if (state.doc.sliceString(textFrom, textTo).trim()) {
    return '';
  }

  const url = state.doc.sliceString(urlNode.from, urlNode.to).trim();
  return isImageUrl(url) ? url : '';
}

function addAutolinkDecorations(builder, state, node) {
  const urlNode = findChildNode(node, 'URL');
  if (!urlNode) {
    return;
  }
  const href = getNodeHref(state, urlNode);
  const rawUrl = state.doc.sliceString(urlNode.from, urlNode.to);
  addTrimmedUrlLinkMark(builder, urlNode.from, urlNode.to, rawUrl, href);
}

function addWikiLinkDecorations(builder, state, node, activeLines) {
  const wikiLink = parseWikiLinkData(state, node);
  if (!wikiLink) {
    return false;
  }

  const hasVisibleText = wikiLink.textFrom >= 0 && wikiLink.textTo > wikiLink.textFrom;
  if (wikiLink.href && hasVisibleText) {
    addLinkMark(builder, wikiLink.textFrom, wikiLink.textTo, wikiLink.href);
  }
  const lineNo = state.doc.lineAt(node.from).number;
  const marker = activeLines.has(lineNo)
    ? activeWikiLinkMarkerDeco
    : hasVisibleText
      ? wikiLinkMarkerDeco
      : emptyWikiLinkMarkerDeco;
  addRange(builder, wikiLink.openFrom, wikiLink.openTo, marker);
  addRange(builder, wikiLink.closeFrom, wikiLink.closeTo, marker);

  if (!activeLines.has(lineNo) && wikiLink.hideTo > wikiLink.hideFrom) {
    addRange(builder, wikiLink.hideFrom, wikiLink.hideTo, hiddenLinkUrlDeco);
  }

  const localTargetStatus = getWikiLinkStatus(wikiLink.localTarget);
  if (wikiLink.localTarget && localTargetStatus === false) {
    const iconPos = hasVisibleText ? wikiLink.textFrom : wikiLink.openTo;
    builder.push(
      Decoration.widget({
        widget: new MissingWikiLinkWidget(),
        side: -1,
      }).range(iconPos)
    );
  }

  return true;
}

function addRange(builder, from, to, deco) {
  if (to <= from) {
    return;
  }
  builder.push(deco.range(from, to));
}

function addLineAwareRange(builder, activeLines, lineNo, from, to, inactiveDeco, activeDeco) {
  addRange(builder, from, to, activeLines.has(lineNo) ? activeDeco : inactiveDeco);
}

function addSingleTildeStrikeDecorations(builder, state, activeLines, existingStrikeRanges, codeBlockLines = null) {
  const pairs = collectSingleTildeStrikePairs(state, existingStrikeRanges);
  for (const pair of pairs) {
    if (codeBlockLines?.has(pair.lineNo)) {
      continue;
    }
    addRange(builder, pair.strikeFrom, pair.strikeTo, inlineStyleDecos.strike);
    addLineAwareRange(
      builder,
      activeLines,
      pair.lineNo,
      pair.openFrom,
      pair.openTo,
      strikeMarkerDeco,
      activeStrikeMarkerDeco
    );
    addLineAwareRange(
      builder,
      activeLines,
      pair.lineNo,
      pair.closeFrom,
      pair.closeTo,
      strikeMarkerDeco,
      activeStrikeMarkerDeco
    );
  }
}

function collectActiveLines(state: EditorState): Set<number> {
  const lines = new Set<number>();
  for (const range of state.selection.ranges) {
    // In live mode, only reveal markdown markers on the focused line.
    const focusLine = state.doc.lineAt(range.head).number;
    lines.add(focusLine);
  }
  return lines;
}

function collectIndentSelectedLines(state: EditorState): Set<number> {
  const lines = new Set<number>();
  for (const range of state.selection.ranges) {
    if (range.empty) {
      continue;
    }
    const from = Math.min(range.from, range.to);
    const to = Math.max(range.from, range.to);
    const startLine = state.doc.lineAt(from).number;
    const endLine = state.doc.lineAt(to - 1).number;
    for (let lineNo = startLine; lineNo <= endLine; lineNo += 1) {
      const lineStart = state.doc.line(lineNo).from;
      if (lineStart >= from && lineStart < to) {
        lines.add(lineNo);
      }
    }
  }
  return lines;
}

function addLineClass(builder, state, from, to, deco) {
  const startLine = state.doc.lineAt(from).number;
  const endLine = state.doc.lineAt(Math.max(from, to - 1)).number;
  for (let lineNo = startLine; lineNo <= endLine; lineNo += 1) {
    const line = state.doc.line(lineNo);
    builder.push(deco.range(line.from));
  }
}

function rangeTouchesActiveLine(state: EditorState, from: number, to: number, activeLines: Set<number>): boolean {
  if (to <= from) {
    return false;
  }

  const startLine = state.doc.lineAt(from).number;
  const endLine = state.doc.lineAt(Math.max(from, to - 1)).number;
  for (let lineNo = startLine; lineNo <= endLine; lineNo += 1) {
    if (activeLines.has(lineNo)) {
      return true;
    }
  }
  return false;
}

function addDetailsBlockDecorations(builder, state, detailsBlocks, activeLines) {
  for (const detailsBlock of detailsBlocks) {
    const openingActive = rangeTouchesActiveLine(state, detailsBlock.anchorFrom, detailsBlock.anchorTo, activeLines);
    const closingActive = rangeTouchesActiveLine(state, detailsBlock.closingFrom, detailsBlock.closingTo, activeLines);
    const editingBoundary = openingActive || closingActive;

    if (!editingBoundary) {
      addLineClass(builder, state, detailsBlock.lineFrom, detailsBlock.lineTo, lineStyleDecos.detailsSummary);

      if (detailsBlock.summaryFrom > detailsBlock.anchorFrom) {
        builder.push(collapsedHeadingBodyDeco.range(detailsBlock.anchorFrom, detailsBlock.summaryFrom));
      }

      builder.push(
        Decoration.replace({
          widget: new DetailsSummaryWidget(
            detailsBlock.anchorFrom,
            detailsBlock.lineFrom,
            detailsBlock.summaryText,
            detailsBlock.collapsed
          ),
        }).range(detailsBlock.summaryFrom, detailsBlock.summaryTo)
      );

      if (detailsBlock.anchorTo > detailsBlock.summaryTo) {
        builder.push(collapsedHeadingBodyDeco.range(detailsBlock.summaryTo, detailsBlock.anchorTo));
      }
    }

    if (!editingBoundary) {
      builder.push(collapsedHeadingBodyDeco.range(detailsBlock.closingFrom, detailsBlock.closingTo));
    }

    if (detailsBlock.collapsed && detailsBlock.bodyTo > detailsBlock.bodyFrom) {
      builder.push(collapsedHeadingBodyDeco.range(detailsBlock.bodyFrom, detailsBlock.bodyTo));
    }
  }
}

function addFootnoteDefinitionDecorations(builder, state, footnotes, activeLines) {
  for (const definition of footnotes.definitions) {
    if (!definition.isPrimary) {
      continue;
    }

    const showRawSyntax =
      rangeTouchesActiveLine(state, definition.lineFrom, definition.lineTo, activeLines) ||
      overlapsSelection(state, definition.lineFrom, definition.lineTo);
    const hasResolvedTarget = definition.number !== null && definition.firstReferenceFrom !== null;

    if (showRawSyntax || !hasResolvedTarget) {
      addRange(builder, definition.colonFrom, definition.colonTo, activeLinkMarkerDeco);
    }
    if (showRawSyntax || !hasResolvedTarget) {
      continue;
    }

    const firstLine = state.doc.lineAt(definition.lineFrom);
    builder.push(
      Decoration.replace({
        widget: new FootnoteBacklinkWidget(definition.number, definition.firstReferenceFrom),
        inclusive: false,
      }).range(definition.markerFrom, definition.markerTo)
    );
    builder.push(lineStyleDecos.footnote.range(firstLine.from));

    for (const continuationLine of definition.continuationLines) {
      builder.push(lineStyleDecos.footnoteContinuation.range(continuationLine.from));
      if (continuationLine.hideIndentFrom !== null && continuationLine.hideIndentTo !== null) {
        builder.push(
          Decoration.replace({
            widget: new FootnoteBackrefSpacerWidget(definition.number),
            inclusive: false,
          }).range(continuationLine.hideIndentFrom, continuationLine.hideIndentTo)
        );
        if (continuationLine.extraIndentColumns > 0) {
          builder.push(
            Decoration.widget({
              widget: listIndentWidget(continuationLine.extraIndentColumns),
              side: 1,
            }).range(continuationLine.hideIndentTo)
          );
        }
      }
    }
  }
}

function addAtxHeadingPrefixMarkers(builder, state, from, activeLines) {
  const line = state.doc.lineAt(from);
  const text = state.doc.sliceString(line.from, line.to);
  const match = /^(#{1,6}[ \t]+)/.exec(text);
  if (!match) {
    return;
  }

  const prefixTo = line.from + match[1].length;
  if (activeLines.has(line.number)) {
    addRange(builder, line.from, prefixTo, activeLineMarkerDeco);
    return;
  }
  addRange(builder, line.from, prefixTo, markerDeco);
}

function addListLineDecorations(builder, state, indentSelectedLines, frontmatter = null, codeBlockLines = null) {
  const stylesByLine = detectListIndentStylesByLine(state);
  const orderedCountsByLevel: Array<number | null> = [];

  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    if (codeBlockLines?.has(lineNo)) {
      orderedCountsByLevel.length = 0;
      continue;
    }
    const line = state.doc.line(lineNo);
    const lineText = state.doc.sliceString(line.from, line.to);
    const style = stylesByLine.get(lineNo);
    const marker = listMarkerData(lineText, null, style);
    if (!marker) {
      orderedCountsByLevel.length = 0;
      continue;
    }

    const level = marker.indentLevel;
    const { expected: orderedDisplayIndex } = nextOrderedSequenceNumber(
      orderedCountsByLevel,
      level,
      marker.orderedNumber
    );

    const inFrontmatterContent = isInsideFrontmatterContent(frontmatter, line.from);
    if (inFrontmatterContent) {
      // Keep front matter list-like values rendered literally (source-style),
      // while still tinting the prefix as a list marker.
      addListMarkerDecoration(builder, state, line.from, orderedDisplayIndex, style, {
        useSourceStyleLiteral: true,
      });
      continue;
    }

    if (marker.fromOffset > 0 && (marker.indentColumns ?? 0) > 0) {
      builder.push(
        Decoration.replace({
          widget: listIndentWidget(marker.indentColumns ?? 0),
          inclusive: false,
        }).range(line.from, line.from + marker.fromOffset)
      );
    }

    builder.push(
      listLineDeco(
        marker.contentOffsetColumns ?? marker.toOffset,
        marker.indentColumns ?? 0,
        style?.columns ?? 2,
        indentSelectedLines.has(lineNo),
        Boolean(marker.isTask),
        marker.taskHiddenPrefixColumns ?? 0
      ).range(line.from)
    );
    addListMarkerDecoration(builder, state, line.from, orderedDisplayIndex, style);
  }
}

function buildDecorations(state) {
  const ranges = [];
  const diagnostics = state.field(diagnosticDataField, false) ?? [];
  const activeLines = collectActiveLines(state);
  const indentSelectedLines = collectIndentSelectedLines(state);
  const tree = resolvedSyntaxTree(state);
  const mermaidColonBlocks = getMermaidColonBlocks(state);
  const footnotes = parseFootnotes(state);
  const collapsedHeadingSections = getCollapsedHeadingSections(state);
  const detailsBlocks = getDetailsBlocks(state);
  const strikeRanges = collectStrikethroughRanges(tree);
  const codeBlockLines = collectCodeBlockLines(state, tree, mermaidColonBlocks);
  const renderedTableRanges = collectRenderedTableRanges(state, getLiveRenderedBlocks(state));
  const parsedTableRanges = [];
  let tableDepth = 0;

  let frontmatter = null;
  try {
    frontmatter = parseFrontmatter(state);
    if (frontmatter) {
      addFrontmatterBoundaryDecorations(ranges, state, frontmatter, activeLines);
    }
  } catch {
    frontmatter = null;
  }
  addForcedThematicBreakDecorations(ranges, state, activeLines, frontmatter, codeBlockLines);
  const mathRanges = collectMathRanges(state, tree, mermaidColonBlocks, renderedTableRanges, frontmatter);

  tree.iterate({
    enter: (node) => {
      if (hasCodeBlockAncestor(node)) {
        if (node.name === 'QuoteMark') {
          const line = state.doc.lineAt(node.from);
          addLineAwareRange(ranges, activeLines, line.number, node.from, node.to, markerDeco, activeLineMarkerDeco);
          return;
        }
        if (!node.name.endsWith('Mark') || !isFenceMarker(state, node.from, node.to)) {
          return;
        }
      }

      if (node.name === 'Table') {
        tableDepth += 1;
      }

      const headingLevel = headingLevelFromName(node.name);
      if (headingLevel !== null) {
        if (tableDepth === 0 && !isInsideFrontmatter(frontmatter, node.from)) {
          addAtxHeadingPrefixMarkers(ranges, state, node.from, activeLines);
          addLineClass(ranges, state, node.from, node.to, lineStyleDecos[`h${headingLevel}`]);
        }
      }

      if (node.name === 'Blockquote') {
        const line = state.doc.lineAt(node.from);
        const lineText = state.doc.sliceString(line.from, line.to).trimStart();
        if (lineText.startsWith('>>>>>>>')) {
          addLineClass(ranges, state, node.from, node.to, lineStyleDecos.mergeIncomingHeader);
          return;
        }
        const alertBlock = detectAlertInBlockquote(state, node);
        if (alertBlock) {
          addAlertBlockDecorations(ranges, state, node, alertBlock, activeLines);
          return;
        }
        addLineClass(ranges, state, node.from, node.to, lineStyleDecos.quote);
      } else if (node.name === 'Table') {
        const tableInfo = parseTableInfo(state, node);
        parsedTableRanges.push({ from: tableInfo.from, to: tableInfo.to });
        addTableDecorations(ranges, state, node, diagnostics);
      } else if (node.name === 'FencedCode' || node.name === 'CodeBlock') {
        addLineClass(ranges, state, node.from, node.to, lineStyleDecos.codeBlock);
        if (node.name === 'FencedCode') {
          addFenceOpeningLineMarker(
            ranges,
            state,
            node.from,
            activeLines,
            addRange,
            activeLineMarkerDeco,
            fenceMarkerDeco
          );

          addCodeLanguageLabel(ranges, state, node, activeLines);

          const codeInfo = getFencedCodeInfo(state, node);
          if (codeInfo === 'mermaid') {
            addMermaidDiagram(ranges, state, node);
            return;
          }
        }
        addCopyCodeButton(ranges, state, node.from, node.to);
      }

      if (node.name === 'Emphasis') {
        addRange(ranges, node.from, node.to, inlineStyleDecos.em);
      } else if (node.name === 'StrongEmphasis') {
        addRange(ranges, node.from, node.to, inlineStyleDecos.strong);
      } else if (node.name === 'Strikethrough') {
        addRange(ranges, node.from, node.to, inlineStyleDecos.strike);
      } else if (node.name === 'InlineCode' || node.name === 'CodeText') {
        addRange(ranges, node.from, node.to, inlineStyleDecos.inlineCode);
      } else if (node.name === 'LinkLabel') {
        const parentName = node.node.parent?.name ?? '';
        if (parentName === 'Link' && node.to - node.from >= 2) {
          const line = state.doc.lineAt(node.from);
          const markerDecoForLine = activeLines.has(line.number) ? activeLinkLabelBracketDeco : linkLabelBracketDeco;
          addRange(ranges, node.from, node.from + 1, markerDecoForLine);
          addRange(ranges, node.to - 1, node.to, markerDecoForLine);
        }
      } else if (node.name === 'Link') {
        const markerRanges = collectInlineFootnoteMarkerRanges(state.doc.sliceString(node.from, node.to));
        if (markerRanges.length) {
          addInlineFootnoteMarkerSyntaxDecorations(ranges, node.from, markerRanges);
        }

        const footnoteReferences = footnotes.referencesByContainerKey.get(footnoteReferenceKey(node.from, node.to));
        if (markerRanges.length) {
          const resolvedReferenceKeys = new Set(
            (footnoteReferences ?? []).map((reference) => footnoteReferenceKey(reference.from, reference.to))
          );
          const renderedFootnoteReferences = [];
          for (const footnoteReference of footnoteReferences ?? []) {
            if (addFootnoteReferenceDecorations(ranges, state, footnoteReference, activeLines)) {
              renderedFootnoteReferences.push(footnoteReference);
            }
          }
          for (let index = 0; index < renderedFootnoteReferences.length - 1; index += 1) {
            const currentReference = renderedFootnoteReferences[index];
            const nextReference = renderedFootnoteReferences[index + 1];
            if (currentReference.to !== nextReference.from) {
              continue;
            }
            ranges.push(
              Decoration.widget({
                widget: footnoteReferenceSeparatorWidget,
                side: 1,
              }).range(currentReference.to)
            );
          }
          for (const markerRange of markerRanges) {
            const markerFrom = node.from + markerRange.fromOffset;
            const markerTo = node.from + markerRange.toOffset;
            if (resolvedReferenceKeys.has(footnoteReferenceKey(markerFrom, markerTo))) {
              continue;
            }
            addRange(ranges, markerFrom + 2, markerTo - 1, footnoteLiteralDeco);
          }
          return;
        }
        if (addWikiLinkDecorations(ranges, state, node, activeLines)) {
          return;
        }
        const emptyImageUrl = getEmptyImageLinkUrl(state, node);
        if (emptyImageUrl) {
          const line = state.doc.lineAt(node.from);
          if (!activeLines.has(line.number)) {
            const linkSelection = overlapsSelection(state, node.from, node.to);
            if (!linkSelection) {
              ranges.push(
                Decoration.replace({
                  widget: new ImageWidget(emptyImageUrl, '', ''),
                  inclusive: false,
                }).range(node.from, node.to)
              );
              return;
            }
          }
        }
        addMarkdownLinkDecorations(ranges, state, node, activeLines);
      } else if (node.name === 'Autolink') {
        addAutolinkDecorations(ranges, state, node);
      } else if (node.name === 'URL') {
        const parentName = node.node.parent?.name ?? '';
        if (parentName !== 'Link' && parentName !== 'Autolink') {
          const href = getNodeHref(state, node);
          const rawUrl = state.doc.sliceString(node.from, node.to);
          addTrimmedUrlLinkMark(ranges, node.from, node.to, rawUrl, href);
        }
      } else if (node.name === 'Image') {
        const line = state.doc.lineAt(node.from);
        const isActiveLine = activeLines.has(line.number);
        const imageSelection = overlapsSelection(state, node.from, node.to);

        if (isActiveLine || imageSelection) {
          const { url, altText, linkUrl } = getImageData(state, node);
          if (url) {
            ranges.push(
              Decoration.widget({
                widget: new ImageWidget(url, altText, linkUrl),
                side: 1,
                block: true,
              }).range(line.to)
            );
          }
          return;
        }

        const { url, altText, linkUrl } = getImageData(state, node);
        if (url) {
          ranges.push(
            Decoration.replace({
              widget: new ImageWidget(url, altText, linkUrl),
              inclusive: false,
            }).range(node.from, node.to)
          );
        }
      }

      if (!node.name.endsWith('Mark')) {
        return;
      }

      const line = state.doc.lineAt(node.from);
      if (isInsideFrontmatterContent(frontmatter, node.from)) {
        return;
      }
      if (isMergeConflictMarkerLine(state, node.from)) {
        // Keep merge conflict markers visible in live mode (e.g. ">>>>>>> branch")
        // even when the Markdown parser tokenizes them as quote markers.
        return;
      }
      if (tableDepth > 0 && node.name === 'HeaderMark') {
        return;
      }
      if (isFenceMarker(state, node.from, node.to)) {
        // Show fence markers on all lines (not just active)
        addLineAwareRange(ranges, activeLines, line.number, node.from, node.to, fenceMarkerDeco, activeLineMarkerDeco);
      } else if (node.name === 'StrikethroughMark') {
        addLineAwareRange(
          ranges,
          activeLines,
          line.number,
          node.from,
          node.to,
          strikeMarkerDeco,
          activeStrikeMarkerDeco
        );
      } else if (node.name === 'CodeMark') {
        addLineAwareRange(ranges, activeLines, line.number, node.from, node.to, codeMarkerDeco, activeCodeMarkerDeco);
      } else if (node.name === 'LinkMark') {
        const parentName = node.node.parent?.name ?? '';
        if (
          parentName === 'Link' &&
          footnotes.referencesByContainerKey.has(
            footnoteReferenceKey(node.node.parent?.from ?? -1, node.node.parent?.to ?? -1)
          )
        ) {
          return;
        }
        // For image links, check if the image node overlaps with selection to show markers
        let useActiveDeco = activeLines.has(line.number);
        if (parentName === 'Image') {
          const { url } = getImageData(state, node.node.parent);
          if (!url) {
            return;
          }
          // Also show active markers if the image is selected
          if (!useActiveDeco && overlapsSelection(state, node.node.parent.from, node.node.parent.to)) {
            useActiveDeco = true;
          }
        } else if (parentName === 'Link') {
          if (isWikiLinkNode(state, node.node.parent)) {
            return;
          }
          const urlNode = findChildNode(node.node.parent, 'URL');
          if (!urlNode) {
            return;
          }
          const href = getNodeHref(state, urlNode);
          if (!href) {
            return;
          }
        }
        addRange(ranges, node.from, node.to, useActiveDeco ? activeLinkMarkerDeco : linkMarkerDeco);
      } else if (
        node.name === 'SuperscriptMark' &&
        node.node.parent?.parent?.name === 'Link' &&
        footnotes.referencesByContainerKey.has(
          footnoteReferenceKey(node.node.parent.parent.from, node.node.parent.parent.to)
        )
      ) {
        // Keep "^" visible for unresolved markers inside partially-resolved
        // adjacent footnote sequences (e.g. "[^4][^5]" where only "[^4]" resolves).
        return;
      } else if (activeLines.has(line.number)) {
        addRange(ranges, node.from, node.to, activeLineMarkerDeco);
      } else {
        addRange(ranges, node.from, node.to, markerDeco);
      }
    },
    leave: (node) => {
      if (node.name === 'Table') {
        tableDepth -= 1;
      }
    },
  });

  addFallbackTableDecorations(ranges, state, tree, parsedTableRanges, mermaidColonBlocks, diagnostics);
  addRawFileUrlDecorations(ranges, state, tree, frontmatter);
  addSingleTildeStrikeDecorations(ranges, state, activeLines, strikeRanges, codeBlockLines);
  addListLineDecorations(ranges, state, indentSelectedLines, frontmatter, codeBlockLines);
  addMathDecorations(ranges, state, mathRanges, activeLines);
  addKbdTagDecorations(ranges, state, activeLines, renderedTableRanges, mathRanges, frontmatter, codeBlockLines);
  addEmojiDecorationsWithMath(ranges, state, mathRanges, codeBlockLines);
  addMermaidColonFenceDecorations(ranges, state, mermaidColonBlocks, activeLines);
  addFootnoteDefinitionDecorations(ranges, state, footnotes, activeLines);
  addDetailsBlockDecorations(ranges, state, detailsBlocks, activeLines);
  for (const section of collapsedHeadingSections) {
    addLineClass(ranges, state, section.lineFrom, section.lineTo, collapsedHeadingLineDeco);
    addRange(ranges, section.collapseFrom, section.collapseTo, collapsedHeadingBodyDeco);
  }

  const result = Decoration.set(ranges, true);
  return filterDecorationsOutsideMergeConflicts(state, result);
}

function hasCodeBlockAncestor(node) {
  let parent = node.node.parent;
  while (parent) {
    if (parent.name === 'FencedCode' || parent.name === 'CodeBlock') {
      return true;
    }
    parent = parent.parent;
  }
  return false;
}

function addAlertBlockDecorations(builder, state, node, alertBlock, activeLines) {
  const startLine = state.doc.lineAt(node.from);
  const endLine = state.doc.lineAt(node.to);
  const lineDeco = alertLineDecos[alertBlock.type];

  for (let lineNo = startLine.number; lineNo <= endLine.number; lineNo += 1) {
    const line = state.doc.line(lineNo);
    builder.push(lineDeco.range(line.from));
  }

  if (!activeLines.has(startLine.number)) {
    builder.push(
      Decoration.widget({
        widget: new AlertIconWidget(alertBlock.type),
        side: -1,
      }).range(startLine.from)
    );
    addRange(builder, alertBlock.directiveFrom, alertBlock.directiveTo, hiddenAlertMarkerDeco);
  } else {
    addRange(builder, alertBlock.directiveFrom, alertBlock.directiveTo, alertMarkerDeco);
    addRange(builder, alertBlock.labelFrom, alertBlock.labelTo, alertLabelActiveDeco);
  }
}

function safeBuildDecorations(state, fallback, context, extra = {}) {
  try {
    return buildDecorations(state);
  } catch (error) {
    console.error('[MEO liveMode] decoration build failed', {
      context,
      docLength: state.doc.length,
      ...extra,
      error,
    });
    return fallback;
  }
}

function mergeConflictRanges(state) {
  return parseMergeConflicts(state).map((conflict) => ({
    from: conflict.blockFrom,
    to: conflict.blockTo,
  }));
}

function pointInsideRanges(pos, ranges) {
  for (const range of ranges) {
    if (pos >= range.from && pos < range.to) {
      return true;
    }
  }
  return false;
}

function rangeOverlapsRanges(from, to, ranges) {
  for (const range of ranges) {
    if (rangesOverlap(from, to, range.from, range.to)) {
      return true;
    }
  }
  return false;
}

function filterDecorationsOutsideMergeConflicts(state, decorations) {
  const conflicts = mergeConflictRanges(state);
  if (!conflicts.length || isEmptyDecorationSet(decorations)) {
    return decorations;
  }

  const filtered = [];
  decorations.between(0, state.doc.length, (from, to, value) => {
    const overlaps = to > from ? rangeOverlapsRanges(from, to, conflicts) : pointInsideRanges(from, conflicts);
    if (!overlaps) {
      filtered.push(value.range(from, to));
    }
  });

  return Decoration.set(filtered, true);
}

function collectCodeBlockLines(state, tree, mermaidColonBlocks) {
  const lines = new Set();
  tree.iterate({
    enter(node) {
      if (node.name !== 'FencedCode' && node.name !== 'CodeBlock') {
        return;
      }

      const startLineNo = state.doc.lineAt(node.from).number;
      const endLineNo = state.doc.lineAt(Math.max(node.to - 1, node.from)).number;
      for (let lineNo = startLineNo; lineNo <= endLineNo; lineNo += 1) {
        lines.add(lineNo);
      }
      return false;
    },
  });

  for (const block of mermaidColonBlocks) {
    for (let lineNo = block.startLine; lineNo <= block.endLine; lineNo += 1) {
      lines.add(lineNo);
    }
  }

  return lines;
}

function addMermaidColonFenceDecorations(builder, state, mermaidColonBlocks, activeLines) {
  for (const block of mermaidColonBlocks) {
    const startLine = state.doc.line(block.startLine);
    const endLine = state.doc.line(block.endLine);

    addLineClass(builder, state, startLine.from, endLine.to, lineStyleDecos.codeBlock);

    addRange(
      builder,
      startLine.from,
      startLine.to,
      activeLines.has(startLine.number) ? activeCodeMarkerDeco : fenceMarkerDeco
    );
    if (!activeLines.has(startLine.number)) {
      addTopLinePillLabel(builder, startLine.to, 'mermaid');
    }

    addRange(
      builder,
      endLine.from,
      endLine.to,
      activeLines.has(endLine.number) ? activeCodeMarkerDeco : fenceMarkerDeco
    );

    addMermaidDiagramBlock(builder, state, {
      startLine: block.startLine,
      endLine: block.endLine,
      diagramText: block.diagramText,
      fullBlockText: block.fullBlockText,
    });
  }
}

const emojiWidgetCache = new Map<string, WidgetType>();

class KbdTagWidget extends WidgetType {
  keyText: string;

  constructor(keyText: string) {
    super();
    this.keyText = keyText;
  }

  eq(other: WidgetType): boolean {
    return other instanceof KbdTagWidget && other.keyText === this.keyText;
  }

  toDOM(): HTMLElement {
    const el = document.createElement('kbd');
    el.className = 'meo-md-kbd';
    el.textContent = this.keyText;
    return el;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

const kbdWidgetCache = new Map<string, WidgetType>();

function getKbdWidget(keyText: string): WidgetType {
  let widget = kbdWidgetCache.get(keyText);
  if (!widget) {
    widget = new KbdTagWidget(keyText);
    kbdWidgetCache.set(keyText, widget);
  }
  return widget;
}

class LatexMathWidget extends WidgetType {
  html: string;
  mode: LatexMathMode;
  fencedDisplay: boolean;
  startLine: number;
  endLine: number;

  constructor(html: string, mode: LatexMathMode, fencedDisplay = false, startLine = 0, endLine = 0) {
    super();
    this.html = html;
    this.mode = mode;
    this.fencedDisplay = fencedDisplay;
    this.startLine = startLine;
    this.endLine = endLine;
  }

  eq(other: WidgetType): boolean {
    return (
      other instanceof LatexMathWidget &&
      other.html === this.html &&
      other.mode === this.mode &&
      other.fencedDisplay === this.fencedDisplay &&
      other.startLine === this.startLine &&
      other.endLine === this.endLine
    );
  }

  toDOM(): HTMLElement {
    const wrapper = document.createElement(this.mode === 'display' ? 'div' : 'span');
    wrapper.className = `meo-md-math meo-md-math-${this.mode}`;
    if (this.startLine > 0) {
      wrapper.dataset.meoRenderedBlockStartLine = String(this.startLine);
    }
    if (this.endLine > 0) {
      wrapper.dataset.meoRenderedBlockEndLine = String(this.endLine);
    }
    if (this.fencedDisplay) {
      wrapper.dataset.meoRenderedBlockKind = 'math';
    }
    if (this.fencedDisplay && this.mode === 'display') {
      wrapper.classList.add('meo-md-math-fenced-display');
      wrapper.addEventListener('pointerdown', (event: PointerEvent) => {
        if (event.button !== 0) {
          return;
        }
        const view = EditorView.findFromDOM(wrapper);
        if (!view) {
          return;
        }

        // Clicking rendered fenced math should move the caret inside the block so
        // live mode reveals the original source lines for editing.
        const lastContentLine = Math.max(this.startLine, this.endLine - 1);
        const targetLineNo = Math.max(1, Math.min(this.startLine + 1, lastContentLine));
        const targetPos = view.state.doc.line(targetLineNo).from;

        event.preventDefault();
        event.stopPropagation();
        view.dispatch({ selection: { anchor: targetPos } });
        view.focus();
      });
    }
    wrapper.innerHTML = this.html;
    return wrapper;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

const MATH_WIDGET_CACHE_LIMIT = 300;
const mathWidgetCache = new Map<string, WidgetType>();

function getMathWidget(
  html: string,
  mode: LatexMathMode,
  fencedDisplay = false,
  startLine = 0,
  endLine = 0
): WidgetType {
  const key = `${mode}:${fencedDisplay ? 1 : 0}:${startLine}:${endLine}:${html}`;
  let widget = mathWidgetCache.get(key);
  if (widget) {
    mathWidgetCache.delete(key);
    mathWidgetCache.set(key, widget);
    return widget;
  }

  widget = new LatexMathWidget(html, mode, fencedDisplay, startLine, endLine);
  mathWidgetCache.set(key, widget);
  if (mathWidgetCache.size > MATH_WIDGET_CACHE_LIMIT) {
    const oldestKey = mathWidgetCache.keys().next().value;
    if (oldestKey !== undefined) {
      mathWidgetCache.delete(oldestKey);
    }
  }

  return widget;
}

function collectRenderedTableRanges(
  state,
  blocks: ReadonlyArray<LiveRenderedBlock>
): Array<{ from: number; to: number }> {
  const ranges: Array<{ from: number; to: number }> = [];
  for (const block of blocks) {
    if (block.kind !== 'table') {
      continue;
    }
    ranges.push({
      from: state.doc.line(block.startLine).from,
      to: state.doc.line(block.endLine).to,
    });
  }
  return ranges;
}

function mergeSimpleRanges(ranges: Array<{ from: number; to: number }>): Array<{ from: number; to: number }> {
  const filtered = ranges
    .filter((range) => Number.isFinite(range.from) && Number.isFinite(range.to) && range.to > range.from)
    .sort((left, right) => left.from - right.from || left.to - right.to);
  if (!filtered.length) {
    return [];
  }

  const merged = [filtered[0]];
  for (let index = 1; index < filtered.length; index += 1) {
    const current = filtered[index];
    const previous = merged[merged.length - 1];
    if (current.from <= previous.to) {
      if (current.to > previous.to) {
        previous.to = current.to;
      }
      continue;
    }
    merged.push({ from: current.from, to: current.to });
  }
  return merged;
}

function collectInlineCodeRanges(tree): Array<{ from: number; to: number }> {
  const ranges: Array<{ from: number; to: number }> = [];
  tree.iterate({
    enter(node) {
      if (node.name === 'InlineCode' || node.name === 'CodeText') {
        ranges.push({ from: node.from, to: node.to });
      }
    },
  });
  return ranges;
}

function collectCodeBlockRanges(state, tree, mermaidColonBlocks): Array<{ from: number; to: number }> {
  const ranges: Array<{ from: number; to: number }> = [];

  tree.iterate({
    enter(node) {
      if (node.name !== 'FencedCode' && node.name !== 'CodeBlock') {
        return;
      }
      ranges.push({ from: node.from, to: node.to });
      return false;
    },
  });

  for (const block of mermaidColonBlocks) {
    ranges.push({
      from: state.doc.line(block.startLine).from,
      to: state.doc.line(block.endLine).to,
    });
  }

  return ranges;
}

function collectMathRanges(state, tree, mermaidColonBlocks, renderedTableRanges, frontmatter = null): LatexMathRange[] {
  const excludedRanges = [
    ...collectInlineCodeRanges(tree),
    ...collectCodeBlockRanges(state, tree, mermaidColonBlocks),
    ...renderedTableRanges,
  ];

  if (frontmatter) {
    excludedRanges.push({ from: frontmatter.openingFrom, to: frontmatter.closingTo });
  }

  const text = state.doc.toString();
  if (text.indexOf('$') === -1) {
    return [];
  }

  return collectLatexMathRanges(text, {
    excludedRanges: mergeSimpleRanges(excludedRanges),
  });
}

function resolveFencedMathRenderSpan(
  state,
  startLineNo: number,
  endLineNo: number
): { innerFrom: number; innerTo: number } | null {
  const innerLineRange = resolveFencedDisplayMathInnerLineRange(startLineNo, endLineNo);
  if (!innerLineRange) {
    return null;
  }

  const innerStartLine = state.doc.line(innerLineRange.innerStartLine);
  const innerEndLine = state.doc.line(innerLineRange.innerEndLine);
  if (innerEndLine.to <= innerStartLine.from) {
    return null;
  }

  return {
    innerFrom: innerStartLine.from,
    innerTo: innerEndLine.to,
  };
}

function addMathDecorations(builder, state, mathRanges: ReadonlyArray<LatexMathRange>, activeLines) {
  for (const mathRange of mathRanges) {
    if (mathRange.to <= mathRange.from) {
      continue;
    }
    const fencedDisplay = mathRange.mode === 'display' && mathRange.fencedDisplay === true;
    const editingBoundary =
      rangeTouchesActiveLine(state, mathRange.from, mathRange.to, activeLines) ||
      overlapsSelection(state, mathRange.from, mathRange.to);

    if (fencedDisplay) {
      const openingLine = state.doc.lineAt(mathRange.from);
      const closingLine = state.doc.lineAt(Math.max(mathRange.to - 1, mathRange.from));
      const startLineNo = openingLine.number;
      const endLineNo = closingLine.number;
      const renderSpan = resolveFencedMathRenderSpan(state, startLineNo, endLineNo);

      addLineClass(builder, state, openingLine.from, closingLine.to, lineStyleDecos.codeBlock);

      if (!activeLines.has(openingLine.number)) {
        addTopLinePillLabel(builder, openingLine.to, 'latex');
      }
      const copyContent = renderSpan ? state.doc.sliceString(renderSpan.innerFrom, renderSpan.innerTo) : '';
      if (copyContent) {
        addTopLineCopyButton(builder, openingLine.to, copyContent);
      }

      addRange(
        builder,
        openingLine.from,
        openingLine.to,
        activeLines.has(openingLine.number) ? activeCodeMarkerDeco : fenceMarkerDeco
      );
      addRange(
        builder,
        closingLine.from,
        closingLine.to,
        activeLines.has(closingLine.number) ? activeCodeMarkerDeco : fenceMarkerDeco
      );

      if (editingBoundary) {
        continue;
      }

      const html = renderLatexMathToHtml(mathRange.content, mathRange.mode);
      if (!html) {
        continue;
      }

      if (!renderSpan) {
        continue;
      }

      builder.push(
        Decoration.replace({
          widget: getMathWidget(html, mathRange.mode, true, startLineNo, endLineNo),
          block: true,
        }).range(renderSpan.innerFrom, renderSpan.innerTo)
      );
      continue;
    }

    if (editingBoundary) {
      continue;
    }

    const html = renderLatexMathToHtml(mathRange.content, mathRange.mode);
    if (!html) {
      continue;
    }

    builder.push(
      Decoration.replace({
        widget: getMathWidget(html, mathRange.mode, false, 0, 0),
        inclusive: false,
      }).range(mathRange.from, mathRange.to)
    );
  }
}

function addKbdTagDecorations(
  builder,
  state,
  activeLines,
  renderedTableRanges,
  mathRanges = [],
  frontmatter = null,
  codeBlockLines = null
) {
  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    if (activeLines.has(lineNo) || codeBlockLines?.has(lineNo)) {
      continue;
    }

    const line = state.doc.line(lineNo);
    if (isInsideFrontmatterContent(frontmatter, line.from)) {
      continue;
    }

    const lineText = state.doc.sliceString(line.from, line.to);
    if (!hasKbdTagMarker(lineText)) {
      continue;
    }
    if (overlapsParsedTableRange(line.from, line.to, renderedTableRanges)) {
      continue;
    }

    const kbdRanges = collectKbdTagRangesFromText(lineText, line.from);
    for (const kbdRange of kbdRanges) {
      if (overlapsSelection(state, kbdRange.from, kbdRange.to)) {
        continue;
      }
      if (overlapsParsedTableRange(kbdRange.from, kbdRange.to, renderedTableRanges)) {
        continue;
      }
      if (overlapsParsedTableRange(kbdRange.from, kbdRange.to, mathRanges)) {
        continue;
      }

      const keyText = kbdRange.content.trim();
      if (!keyText) {
        continue;
      }

      builder.push(
        Decoration.replace({
          widget: getKbdWidget(keyText),
          inclusive: false,
        }).range(kbdRange.from, kbdRange.to)
      );
    }
  }
}

function getEmojiWidget(emoji: string): WidgetType {
  let widget = emojiWidgetCache.get(emoji);
  if (!widget) {
    widget = new (class extends WidgetType {
      toDOM() {
        const span = document.createElement('span');
        span.className = 'meo-md-emoji';
        span.textContent = emoji;
        return span;
      }
      ignoreEvent() {
        return true;
      }
    })();
    emojiWidgetCache.set(emoji, widget);
  }
  return widget;
}

function addEmojiDecorationsWithMath(builder, state, mathRanges, codeBlockLines = null) {
  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    if (codeBlockLines?.has(lineNo)) {
      continue;
    }
    const line = state.doc.line(lineNo);
    const lineText = state.doc.sliceString(line.from, line.to);
    if (overlapsParsedTableRange(line.from, line.to, mathRanges)) {
      continue;
    }

    if (lineText.indexOf(':') === -1) {
      continue;
    }

    const emojiRanges = collectEmojiRangesFromText(lineText, line.from);
    for (const emojiRange of emojiRanges) {
      if (overlapsParsedTableRange(emojiRange.from, emojiRange.to, mathRanges)) {
        continue;
      }
      builder.push(
        Decoration.replace({
          widget: getEmojiWidget(emojiRange.emoji),
          inclusive: false,
        }).range(emojiRange.from, emojiRange.to)
      );
    }
  }
}

const liveDecorationField = StateField.define({
  create(state) {
    return safeBuildDecorations(state, Decoration.none, 'create');
  },
  update(decorations, transaction) {
    // Recompute on every transaction so live mode stays in sync with parser updates
    // that may arrive without direct doc/selection changes.
    const next = safeBuildDecorations(transaction.state, decorations, 'update', {
      docChanged: transaction.docChanged,
      selection: transaction.selection,
    });

    // Guard against transient empty parse results on selection-only transactions.
    if (!transaction.docChanged && isEmptyDecorationSet(next) && !isEmptyDecorationSet(decorations)) {
      return decorations;
    }

    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

function buildLiveLineNumberMarkers(state) {
  const builder = new RangeSetBuilder();
  const conflictLineNumbers = new Set();
  for (const conflict of parseMergeConflicts(state)) {
    for (let lineNo = conflict.startLineNo; lineNo <= conflict.endLineNo; lineNo += 1) {
      conflictLineNumbers.add(lineNo);
    }
  }
  for (const block of getLiveRenderedBlocks(state)) {
    if (block.lineNumberHiddenFrom < 1 || block.lineNumberHiddenTo < block.lineNumberHiddenFrom) {
      continue;
    }
    for (let lineNo = block.lineNumberHiddenFrom; lineNo <= block.lineNumberHiddenTo; lineNo += 1) {
      if (conflictLineNumbers.has(lineNo)) {
        continue;
      }
      const line = state.doc.line(lineNo);
      builder.add(line.from, line.from, tableDelimiterGutterLineClassMarker);
    }
  }
  return builder.finish();
}

function detectTableBlocks(state) {
  const blocks = [];
  for (let lineNo = 2; lineNo <= state.doc.lines; lineNo += 1) {
    const delimiterLine = state.doc.line(lineNo);
    const delimiterText = state.doc.sliceString(delimiterLine.from, delimiterLine.to);
    if (isThematicBreakLine(delimiterText)) continue;
    if (!isTableDelimiterLine(delimiterText)) continue;

    const headerLineNo = lineNo - 1;
    const headerLine = state.doc.line(headerLineNo);
    const headerText = state.doc.sliceString(headerLine.from, headerLine.to);
    if (!isTableContentLine(headerText)) continue;

    let endLineNo = lineNo;
    for (let rowLineNo = lineNo + 1; rowLineNo <= state.doc.lines; rowLineNo += 1) {
      const rowLine = state.doc.line(rowLineNo);
      const rowText = state.doc.sliceString(rowLine.from, rowLine.to);
      if (!isTableContentLine(rowText)) break;
      endLineNo = rowLineNo;
    }

    blocks.push({ startLineNo: headerLineNo, endLineNo });
    lineNo = endLineNo;
  }
  return blocks;
}

function addFallbackTableDecorations(builder, state, tree, parsedTableRanges, mermaidColonBlocks, diagnostics = []) {
  const tableBlocks = detectTableBlocks(state);
  for (const block of tableBlocks) {
    const from = state.doc.line(block.startLineNo).from;
    const to = state.doc.line(block.endLineNo).to;
    if (overlapsParsedTableRange(from, to, parsedTableRanges)) continue;
    if (isInsideCodeBlock(tree, from)) continue;
    if (rangeOverlapsMermaidColonBlock(mermaidColonBlocks, from, to)) continue;
    addTableDecorationsForLineRange(builder, state, block.startLineNo, block.endLineNo, diagnostics);
  }
}

function hasBlockedRawFileUrlAncestor(tree, from, to) {
  const positions = [from, Math.max(from, to - 1)];
  for (const position of positions) {
    let node = tree.resolveInner(position, 1);
    while (node) {
      if (rawFileUrlBlockedAncestorNames.has(node.name)) {
        return true;
      }
      node = node.parent;
    }
  }
  return false;
}

function addRawFileUrlDecorations(builder, state, tree, frontmatter = null) {
  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    const line = state.doc.line(lineNo);
    if (line.text.indexOf(fileSchemePrefix) === -1) {
      continue;
    }

    const matches = findRawSourceUrlMatches(line.text);
    for (const match of matches) {
      if (!match.href.toLowerCase().startsWith(fileSchemePrefix)) {
        continue;
      }
      const from = line.from + match.index;
      const to = from + match.length;
      if (to <= from) {
        continue;
      }
      if (isInsideFrontmatterContent(frontmatter, from)) {
        continue;
      }
      if (hasBlockedRawFileUrlAncestor(tree, from, to)) {
        continue;
      }
      addLinkMark(builder, from, to, match.href);
    }
  }
}

function rangesOverlap(fromA, toA, fromB, toB) {
  return fromA < toB && toA > fromB;
}

function overlapsSelection(state, from, to) {
  return state.selection.ranges.some((range) => rangesOverlap(from, to, range.from, range.to));
}

function overlapsParsedTableRange(from, to, ranges) {
  return ranges.some((range) => rangesOverlap(from, to, range.from, range.to));
}

function isInsideCodeBlock(tree, pos) {
  let node = tree.resolveInner(pos, 1);
  while (node) {
    if (node.name === 'FencedCode' || node.name === 'CodeBlock') return true;
    node = node.parent;
  }
  return false;
}

const liveLineNumberMarkerField = StateField.define({
  create(state) {
    return buildLiveLineNumberMarkers(state);
  },
  update(markers, transaction) {
    if (!transaction.docChanged && transaction.startState.selection.eq(transaction.state.selection)) {
      return markers;
    }
    return buildLiveLineNumberMarkers(transaction.state);
  },
  provide: (field) => gutterLineClass.from(field),
});

export function liveModeExtensions() {
  return [
    markdown({
      base: markdownLanguage,
      addKeymap: false,
      codeLanguages: resolveCodeLanguage,
      extensions: [{ remove: ['SetextHeading'] }],
    }),
    syntaxHighlighting(highlightStyle),
    liveDecorationField,
    liveLineNumberMarkerField,
    ...mergeConflictSourceExtensions(),
    ...headingCollapseSharedExtensions(),
    ...headingCollapseLiveExtensions(),
  ];
}

function isEmptyDecorationSet(set) {
  const cursor = set.iter();
  return cursor.value === null;
}
