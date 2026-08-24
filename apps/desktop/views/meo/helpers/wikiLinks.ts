// @ts-nocheck
import { StateField, RangeSetBuilder, EditorState, Transaction } from '@codemirror/state';
import { Decoration, EditorView } from '@codemirror/view';
import { resolvedSyntaxTree } from './markdownSyntax';

const SCHEME_RE = /^[a-z][a-z0-9+.-]*:/i;
const wikiLinkStatusByTarget = new Map<string, boolean>();

let vscodeApi: any = null;
let wikiLinkRequestCounter = 0;
let latestWikiLinkRequestId = '';
let pendingWikiStatusRefresh: number | null = null;
const wikiStatusDebounceMs = 1000;

export const initializeWikiLinkHandling = (vscode: any): void => {
  vscodeApi = vscode;
};

export const isEscapedAt = (text: string, index: number): boolean => {
  let slashCount = 0;
  for (let i = index - 1; i >= 0 && text[i] === '\\'; i -= 1) {
    slashCount += 1;
  }
  return slashCount % 2 === 1;
};

export const collectWikiLinkTargets = (text: string): string[] => {
  const targets = new Set<string>();
  for (let i = 0; i < text.length - 1; i += 1) {
    if (text[i] !== '[' || text[i + 1] !== '[') {
      continue;
    }
    if ((i > 0 && text[i - 1] === '!') || isEscapedAt(text, i)) {
      continue;
    }

    const close = text.indexOf(']]', i + 2);
    if (close < 0) {
      break;
    }
    const content = text.slice(i + 2, close);
    const pipeIndex = content.indexOf('|');
    const targetRaw = (pipeIndex >= 0 ? content.slice(0, pipeIndex) : content).trim();
    const target = normalizeWikiTarget(targetRaw);
    if (target) {
      targets.add(target);
    }
    i = close + 1;
  }
  return Array.from(targets);
};

export interface WikiLinkRefreshContext {
  refreshDecorations: () => void;
}

let refreshContext: WikiLinkRefreshContext | null = null;

export const setWikiLinkRefreshContext = (context: WikiLinkRefreshContext): void => {
  refreshContext = context;
};

export const requestWikiLinkStatuses = (text: string): void => {
  const targets = collectWikiLinkTargets(text);
  if (!targets.length) {
    replaceWikiLinkStatuses([]);
    refreshContext?.refreshDecorations();
    return;
  }

  const requestId = `wiki-${wikiLinkRequestCounter++}`;
  latestWikiLinkRequestId = requestId;
  vscodeApi?.postMessage({ type: 'resolveWikiLinks', requestId, targets });
};

export const scheduleWikiLinkStatusRefresh = (text: string): void => {
  if (pendingWikiStatusRefresh !== null) {
    window.clearTimeout(pendingWikiStatusRefresh);
  }
  pendingWikiStatusRefresh = window.setTimeout(() => {
    pendingWikiStatusRefresh = null;
    requestWikiLinkStatuses(text);
  }, wikiStatusDebounceMs);
};

export const cancelPendingWikiStatusRefresh = (): void => {
  if (pendingWikiStatusRefresh !== null) {
    window.clearTimeout(pendingWikiStatusRefresh);
    pendingWikiStatusRefresh = null;
  }
};

export const handleResolvedWikiLinks = (message: {
  requestId: string;
  results?: Array<{ target: string; exists: boolean }>;
}): boolean => {
  if (message.requestId !== latestWikiLinkRequestId) {
    return false;
  }
  replaceWikiLinkStatuses(message.results ?? []);
  return true;
};

export const wikiLinkScheme = 'meo-wiki:';

function parentNameOf(node: any): string {
  return node?.node?.parent?.name ?? node?.parent?.name ?? '';
}

function isEscapedPosition(state: EditorState, pos: number): boolean {
  if (pos <= 0) {
    return false;
  }
  let slashCount = 0;
  for (let i = pos - 1; i >= 0 && state.doc.sliceString(i, i + 1) === '\\'; i -= 1) {
    slashCount += 1;
  }
  return slashCount % 2 === 1;
}

function trimRange(state: EditorState, from: number, to: number): { from: number; to: number } {
  while (from < to && /\s/.test(state.doc.sliceString(from, from + 1))) {
    from += 1;
  }
  while (to > from && /\s/.test(state.doc.sliceString(to - 1, to))) {
    to -= 1;
  }
  return { from, to };
}

interface WikiLinkBounds {
  openFrom: number;
  openTo: number;
  closeFrom: number;
  closeTo: number;
  contentFrom: number;
  contentTo: number;
}

function wikiLinkBounds(state: EditorState, node: any): WikiLinkBounds | null {
  if (!node || node.name !== 'Link' || parentNameOf(node) === 'Image') {
    return null;
  }

  const fullForm =
    node.from + 1 < node.to &&
    state.doc.sliceString(node.from, node.from + 2) === '[[' &&
    state.doc.sliceString(node.to - 2, node.to) === ']]';
  if (fullForm) {
    if (isEscapedPosition(state, node.from)) {
      return null;
    }
    return {
      openFrom: node.from,
      openTo: node.from + 2,
      closeFrom: node.to - 2,
      closeTo: node.to,
      contentFrom: node.from + 2,
      contentTo: node.to - 2,
    };
  }

  if (node.from < 1 || node.to >= state.doc.length || isEscapedPosition(state, node.from - 1)) {
    return null;
  }
  const wrappedForm =
    state.doc.sliceString(node.from - 1, node.from + 1) === '[[' &&
    state.doc.sliceString(node.to - 1, node.to + 1) === ']]';
  if (!wrappedForm) {
    return null;
  }

  return {
    openFrom: node.from - 1,
    openTo: node.from + 1,
    closeFrom: node.to - 1,
    closeTo: node.to + 1,
    contentFrom: node.from + 1,
    contentTo: node.to - 1,
  };
}

function wikiHrefFromTarget(target: string): string {
  if (!target) {
    return '';
  }
  if (SCHEME_RE.test(target)) {
    return target;
  }
  return `${wikiLinkScheme}${encodeURIComponent(target)}`;
}

export function normalizeWikiTarget(target: string | null | undefined): string {
  const normalized = (target ?? '').split('#', 1)[0]?.trim() ?? '';
  if (!normalized || SCHEME_RE.test(normalized)) {
    return '';
  }
  return normalized;
}

export function isWikiLinkNode(state: EditorState, node: any): boolean {
  return Boolean(wikiLinkBounds(state, node));
}

export function wikiMarkerRanges(state: EditorState, node: any): { from: number; to: number }[] | null {
  const bounds = wikiLinkBounds(state, node);
  if (!bounds) {
    return null;
  }
  return [
    { from: bounds.openFrom, to: bounds.openTo },
    { from: bounds.closeFrom, to: bounds.closeTo },
  ];
}

export interface WikiLinkData {
  href: string;
  localTarget: string;
  textFrom: number;
  textTo: number;
  hideFrom: number;
  hideTo: number;
  openFrom: number;
  openTo: number;
  closeFrom: number;
  closeTo: number;
}

export function parseWikiLinkData(state: EditorState, node: any): WikiLinkData | null {
  const bounds = wikiLinkBounds(state, node);
  if (!bounds || bounds.contentFrom > bounds.contentTo) {
    return null;
  }

  const content = state.doc.sliceString(bounds.contentFrom, bounds.contentTo);
  const pipeOffset = content.indexOf('|');
  let target = trimRange(state, bounds.contentFrom, bounds.contentTo);
  let text = trimRange(state, bounds.contentFrom, bounds.contentTo);
  let hideFrom = -1;
  let hideTo = -1;

  if (pipeOffset >= 0) {
    const aliasFrom = bounds.contentFrom + pipeOffset + 1;
    const alias = trimRange(state, aliasFrom, bounds.contentTo);
    target = trimRange(state, bounds.contentFrom, bounds.contentFrom + pipeOffset);
    if (alias.from < alias.to) {
      text = alias;
      hideFrom = bounds.contentFrom;
      hideTo = alias.from;
    } else {
      text = target;
      hideFrom = bounds.contentFrom + pipeOffset;
      hideTo = bounds.contentTo;
    }
  }

  const hasTarget = target.from < target.to;
  const hasText = text.from < text.to;
  const rawTarget = hasTarget ? state.doc.sliceString(target.from, target.to).trim() : '';
  const localTarget = normalizeWikiTarget(rawTarget);

  return {
    href: hasTarget ? wikiHrefFromTarget(rawTarget) : '',
    localTarget,
    textFrom: hasText ? text.from : -1,
    textTo: hasText ? text.to : -1,
    hideFrom,
    hideTo,
    openFrom: bounds.openFrom,
    openTo: bounds.openTo,
    closeFrom: bounds.closeFrom,
    closeTo: bounds.closeTo,
  };
}

export function getWikiLinkStatus(target: string | null | undefined): boolean | null {
  if (!target) {
    return null;
  }
  if (!wikiLinkStatusByTarget.has(target)) {
    return null;
  }
  return wikiLinkStatusByTarget.get(target) === true;
}

export function replaceWikiLinkStatuses(entries: Array<{ target: string; exists: boolean }>): void {
  wikiLinkStatusByTarget.clear();
  for (const entry of entries) {
    if (!entry || typeof entry.target !== 'string') {
      continue;
    }
    const target = entry.target.trim();
    if (!target) {
      continue;
    }
    wikiLinkStatusByTarget.set(target, entry.exists === true);
  }
}

const sourceWikiMarkerDeco = Decoration.mark({ class: 'meo-md-wiki-marker' });

function computeSourceWikiMarkers(state: EditorState): any {
  const builder = new RangeSetBuilder<any>();
  const tree = resolvedSyntaxTree(state);

  tree.iterate({
    enter(node: any) {
      const ranges = wikiMarkerRanges(state, node);
      if (!ranges) {
        return;
      }
      for (const range of ranges) {
        builder.add(range.from, range.to, sourceWikiMarkerDeco);
      }
    },
  });

  return builder.finish();
}

export const sourceWikiMarkerField = StateField.define<any>({
  create(state: EditorState) {
    try {
      return computeSourceWikiMarkers(state);
    } catch {
      return Decoration.none;
    }
  },
  update(markers: any, transaction: Transaction) {
    if (!transaction.docChanged) {
      return markers;
    }
    try {
      return computeSourceWikiMarkers(transaction.state);
    } catch {
      return markers;
    }
  },
  provide: (field: any) => EditorView.decorations.from(field),
});
