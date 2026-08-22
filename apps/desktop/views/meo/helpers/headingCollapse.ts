// @ts-nocheck
import { RangeSetBuilder, StateEffect, StateField, Transaction, EditorState } from '@codemirror/state';
import { EditorView, GutterMarker, gutter } from '@codemirror/view';
import { createElement, ChevronDown } from 'lucide';
import { extractDetailsBlocks, extractHeadingSections, HeadingSection, DetailsBlockInfo } from './markdownSyntax';

const toggleHeadingCollapseEffect = StateEffect.define<number>();
const expandHeadingCollapseEffect = StateEffect.define<number[]>();
const emptyCollapseOverrides = Object.freeze(new Map<number, boolean>());

interface CollapsibleSection {
  kind: 'heading' | 'details';
  anchor: number;
  lineFrom: number;
  collapseFrom: number;
  collapseTo: number;
  defaultCollapsed: boolean;
  headingSection: HeadingSection | null;
  detailsBlock: DetailsBlockInfo | null;
}

export interface DetailsBlockState extends DetailsBlockInfo {
  collapsed: boolean;
}

function isHeadingSectionCollapsible(state: EditorState, section: HeadingSection): boolean {
  if (!section || section.collapseTo <= section.collapseFrom) {
    return false;
  }
  return state.doc.sliceString(section.collapseFrom, section.collapseTo).trim().length > 0;
}

function createHeadingCollapsibleSection(section: HeadingSection): CollapsibleSection {
  return {
    kind: 'heading',
    anchor: section.lineFrom,
    lineFrom: section.lineFrom,
    collapseFrom: section.collapseFrom,
    collapseTo: section.collapseTo,
    defaultCollapsed: false,
    headingSection: section,
    detailsBlock: null
  };
}

function createDetailsCollapsibleSection(detailsBlock: DetailsBlockInfo): CollapsibleSection {
  return {
    kind: 'details',
    anchor: detailsBlock.anchorFrom,
    lineFrom: detailsBlock.lineFrom,
    collapseFrom: detailsBlock.bodyFrom,
    collapseTo: detailsBlock.bodyTo,
    defaultCollapsed: detailsBlock.defaultCollapsed,
    headingSection: null,
    detailsBlock
  };
}

function getCollapsibleHeadingSections(state: EditorState): HeadingSection[] {
  return extractHeadingSections(state).filter((section) => isHeadingSectionCollapsible(state, section));
}

function getCollapsibleSections(state: EditorState): CollapsibleSection[] {
  const sections = [
    ...getCollapsibleHeadingSections(state).map(createHeadingCollapsibleSection),
    ...extractDetailsBlocks(state).map(createDetailsCollapsibleSection)
  ];

  sections.sort((a, b) => a.lineFrom - b.lineFrom || a.anchor - b.anchor);
  return sections;
}

function getCollapsibleSectionLineMap(state: EditorState): Map<number, CollapsibleSection> {
  return new Map(getCollapsibleSections(state).map((section) => [section.lineFrom, section]));
}

function getCollapsibleSectionAnchorMap(state: EditorState): Map<number, CollapsibleSection> {
  return new Map(getCollapsibleSections(state).map((section) => [section.anchor, section]));
}

function hasHeadingCollapseEffect(transaction: any): boolean {
  return transaction.effects.some(
    (effect: any) =>
      effect.is(toggleHeadingCollapseEffect) ||
      effect.is(expandHeadingCollapseEffect)
  );
}

function hasToggleHeadingCollapseEffect(transaction: any): boolean {
  return transaction.effects.some((effect: any) => effect.is(toggleHeadingCollapseEffect));
}

function mapCollapseOverrides(
  overrides: ReadonlyMap<number, boolean>,
  transaction: Transaction
): Map<number, boolean> {
  if (!overrides.size || !transaction.docChanged) {
    return new Map(overrides);
  }

  const mapped = new Map<number, boolean>();
  for (const [anchor, collapsed] of overrides) {
    mapped.set(transaction.changes.mapPos(anchor, 1), collapsed);
  }
  return mapped;
}

function normalizeCollapseOverrides(
  state: EditorState,
  overrides: Map<number, boolean>
): ReadonlyMap<number, boolean> {
  if (!overrides.size) {
    return emptyCollapseOverrides;
  }

  const sections = getCollapsibleSectionAnchorMap(state);
  const normalizedEntries: Array<[number, boolean]> = [];
  const seen = new Set<number>();
  for (const [anchor, collapsed] of overrides) {
    if (seen.has(anchor)) {
      continue;
    }
    seen.add(anchor);

    const section = sections.get(anchor);
    if (!section || collapsed === section.defaultCollapsed) {
      continue;
    }

    normalizedEntries.push([anchor, collapsed]);
  }

  if (!normalizedEntries.length) {
    return emptyCollapseOverrides;
  }

  normalizedEntries.sort((a, b) => a[0] - b[0]);
  return new Map(normalizedEntries);
}

function collapseOverridesEqual(
  a: ReadonlyMap<number, boolean>,
  b: ReadonlyMap<number, boolean>
): boolean {
  if (a === b) {
    return true;
  }
  if (a.size !== b.size) {
    return false;
  }

  const aEntries = a.entries();
  const bEntries = b.entries();
  while (true) {
    const nextA = aEntries.next();
    const nextB = bEntries.next();
    if (nextA.done || nextB.done) {
      return nextA.done === nextB.done;
    }
    if (nextA.value[0] !== nextB.value[0] || nextA.value[1] !== nextB.value[1]) {
      return false;
    }
  }
}

function sortedNumbersFromSet(values: Set<number>): readonly number[] {
  if (!values.size) {
    return [];
  }
  return Array.from(values).sort((a, b) => a - b);
}

function getEffectiveCollapsedState(
  overrides: ReadonlyMap<number, boolean>,
  section: CollapsibleSection
): boolean {
  return overrides.get(section.anchor) ?? section.defaultCollapsed;
}

function setCollapseOverride(
  overrides: Map<number, boolean>,
  sections: Map<number, CollapsibleSection>,
  anchor: number,
  collapsed: boolean
): void {
  const section = sections.get(anchor);
  if (!section) {
    return;
  }

  if (collapsed === section.defaultCollapsed) {
    overrides.delete(anchor);
    return;
  }

  overrides.set(anchor, collapsed);
}

function toggleCollapseOverride(
  overrides: Map<number, boolean>,
  sections: Map<number, CollapsibleSection>,
  anchor: number
): void {
  const section = sections.get(anchor);
  if (!section) {
    return;
  }

  const nextCollapsed = !getEffectiveCollapsedState(overrides, section);
  setCollapseOverride(overrides, sections, anchor, nextCollapsed);
}

const headingCollapseStateField = StateField.define<ReadonlyMap<number, boolean>>({
  create(): ReadonlyMap<number, boolean> {
    return emptyCollapseOverrides;
  },
  update(
    collapsedHeadings: ReadonlyMap<number, boolean>,
    transaction: Transaction
  ): ReadonlyMap<number, boolean> {
    const hasEffectChange = hasHeadingCollapseEffect(transaction);
    if (!transaction.docChanged && !hasEffectChange) {
      return collapsedHeadings;
    }

    let next = mapCollapseOverrides(collapsedHeadings, transaction);
    const sections = getCollapsibleSectionAnchorMap(transaction.state);

    if (hasEffectChange) {
      for (const effect of transaction.effects) {
        if (effect.is(toggleHeadingCollapseEffect)) {
          toggleCollapseOverride(next, sections, effect.value);
          continue;
        }
        if (effect.is(expandHeadingCollapseEffect)) {
          for (const anchor of effect.value) {
            setCollapseOverride(next, sections, anchor, false);
          }
        }
      }
    }

    const normalized = normalizeCollapseOverrides(transaction.state, next);
    return collapseOverridesEqual(normalized, collapsedHeadings) ? collapsedHeadings : normalized;
  }
});

const headingCollapseSharedExtension = Object.freeze([headingCollapseStateField]);

function getCollapseOverrides(state: EditorState): ReadonlyMap<number, boolean> {
  return state.field(headingCollapseStateField, false) ?? emptyCollapseOverrides;
}

function isSectionCollapsed(state: EditorState, section: CollapsibleSection): boolean {
  return getEffectiveCollapsedState(getCollapseOverrides(state), section);
}

function isSectionCollapsedByAnchor(state: EditorState, anchor: number, defaultCollapsed: boolean): boolean {
  return getCollapseOverrides(state).get(anchor) ?? defaultCollapsed;
}

function getCollapsedSections(state: EditorState): CollapsibleSection[] {
  return getCollapsibleSections(state).filter((section) => isSectionCollapsed(state, section));
}

export function getCollapsedHeadingSections(state: EditorState): HeadingSection[] {
  return getCollapsedSections(state)
    .filter((section) => section.kind === 'heading' && section.headingSection)
    .map((section) => section.headingSection as HeadingSection);
}

export function getDetailsBlocks(state: EditorState): DetailsBlockState[] {
  return extractDetailsBlocks(state).map((detailsBlock) => ({
    ...detailsBlock,
    collapsed: isSectionCollapsedByAnchor(state, detailsBlock.anchorFrom, detailsBlock.defaultCollapsed)
  }));
}

export function toggleCollapsibleSection(view: EditorView, anchor: number): boolean {
  const section = getCollapsibleSectionAnchorMap(view.state).get(anchor);
  if (!section) {
    return false;
  }

  const isCollapsed = isSectionCollapsed(view.state, section);
  const transactionSpec: any = {
    effects: toggleHeadingCollapseEffect.of(section.anchor),
    annotations: Transaction.addToHistory.of(false)
  };
  if (!isCollapsed && section.kind === 'heading') {
    transactionSpec.selection = { anchor: section.lineFrom };
  }

  view.dispatch(transactionSpec);
  view.focus();
  return true;
}

function collectCollapsedHeadingAnchorsForSelection(state: EditorState): readonly number[] {
  const collapsedSections = getCollapsedSections(state);
  if (!collapsedSections.length) {
    return [];
  }

  const matches = new Set<number>();
  for (const selectionRange of state.selection.ranges) {
    const from = Math.min(selectionRange.from, selectionRange.to);
    const to = Math.max(selectionRange.from, selectionRange.to);
    for (const section of collapsedSections) {
      if (selectionRange.empty) {
        if (from > section.collapseFrom && from < section.collapseTo) {
          matches.add(section.anchor);
        }
        continue;
      }
      if (from < section.collapseTo && to > section.collapseFrom) {
        matches.add(section.anchor);
      }
    }
  }

  return sortedNumbersFromSet(matches);
}

export function headingCollapseSharedExtensions(): readonly any[] {
  return headingCollapseSharedExtension;
}

class HeadingFoldGutterMarker extends GutterMarker {
  collapsed: boolean;

  constructor(collapsed: boolean) {
    super();
    this.collapsed = collapsed;
  }

  eq(other: HeadingFoldGutterMarker): boolean {
    return other instanceof HeadingFoldGutterMarker && other.collapsed === this.collapsed;
  }

  toDOM(): HTMLElement {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'meo-md-fold-toggle';
    button.title = this.collapsed ? 'Expand section' : 'Collapse section';
    button.setAttribute('aria-label', this.collapsed ? 'Expand section' : 'Collapse section');
    if (!this.collapsed) {
      button.classList.add('is-expanded');
    }

    const chevron = createElement(ChevronDown, {
      class: 'meo-md-fold-chevron',
      'aria-hidden': 'true',
      width: 14,
      height: 14
    });
    button.appendChild(chevron);
    return button;
  }
}

class HeadingFoldGutterSpacerMarker extends GutterMarker {
  toDOM(): HTMLElement {
    const spacer = document.createElement('span');
    spacer.className = 'meo-md-fold-toggle meo-md-fold-toggle-spacer';
    spacer.setAttribute('aria-hidden', 'true');
    return spacer;
  }
}

const collapsedHeadingFoldMarker = new HeadingFoldGutterMarker(true);
const expandedHeadingFoldMarker = new HeadingFoldGutterMarker(false);
const headingFoldGutterSpacerMarker = new HeadingFoldGutterSpacerMarker();

function buildHeadingFoldGutterMarkers(state: EditorState): any {
  const builder = new RangeSetBuilder<any>();
  for (const section of getCollapsibleSections(state)) {
    builder.add(
      section.lineFrom,
      section.lineFrom,
      isSectionCollapsed(state, section) ? collapsedHeadingFoldMarker : expandedHeadingFoldMarker
    );
  }
  return builder.finish();
}

const liveHeadingFoldGutterField = StateField.define<any>({
  create(state: EditorState) {
    return buildHeadingFoldGutterMarkers(state);
  },
  update(markers: any, transaction: any) {
    if (!transaction.docChanged && !hasHeadingCollapseEffect(transaction)) {
      return markers;
    }
    return buildHeadingFoldGutterMarkers(transaction.state);
  }
});

const liveHeadingFoldGutterExtension = gutter({
  class: 'meo-md-fold-gutter',
  renderEmptyElements: true,
  initialSpacer() {
    return headingFoldGutterSpacerMarker;
  },
  markers(view: EditorView) {
    return view.state.field(liveHeadingFoldGutterField);
  },
  domEventHandlers: {
    mousedown(view: EditorView, line: any, event: MouseEvent) {
      const target = event.target instanceof Element ? event.target : null;
      if (!target?.closest('.meo-md-fold-toggle')) {
        return false;
      }

      const section = getCollapsibleSectionLineMap(view.state).get(line.from);
      event.preventDefault();
      event.stopPropagation();
      if (section) {
        toggleCollapsibleSection(view, section.anchor);
      }
      return true;
    }
  }
});

const liveHeadingAutoExpandSelectionExtension = EditorView.updateListener.of((update: any) => {
  if (update.transactions.some(hasToggleHeadingCollapseEffect)) {
    return;
  }

  const collapsedSections = getCollapsedSections(update.state);
  if (!collapsedSections.length) {
    return;
  }

  const expandAnchors = collectCollapsedHeadingAnchorsForSelection(update.state);
  if (!expandAnchors.length) {
    return;
  }

  update.view.dispatch({
    effects: expandHeadingCollapseEffect.of(expandAnchors as number[]),
    annotations: Transaction.addToHistory.of(false)
  });
});

const headingCollapseLiveExtension = Object.freeze([
  liveHeadingFoldGutterField,
  liveHeadingFoldGutterExtension,
  liveHeadingAutoExpandSelectionExtension
]);

export function headingCollapseLiveExtensions(): readonly any[] {
  return headingCollapseLiveExtension;
}

const emptyFoldGutterMarkers = new RangeSetBuilder<any>().finish();

const sourceHeadingFoldSpacerOnlyExtension = gutter({
  class: 'meo-md-fold-gutter',
  initialSpacer() {
    return headingFoldGutterSpacerMarker;
  },
  markers() {
    return emptyFoldGutterMarkers;
  }
});

export function headingCollapseSourceSpacerExtensions(): any[] {
  return [sourceHeadingFoldSpacerOnlyExtension];
}
