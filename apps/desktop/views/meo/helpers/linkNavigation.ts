// @ts-nocheck
import type { EditorState } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { resolvedSyntaxTree } from './markdownSyntax';
import { isWikiLinkNode, parseWikiLinkData } from './wikiLinks';
import { findRawSourceUrlMatches, linkSchemeRe, normalizeSourceHref } from './rawUrls';

const linkReferenceCache = new WeakMap<object, Map<string, string>>();
type LinkLookupOptions = {
  exactTextHit?: boolean;
};

function childNodeByName(node: any, name: string): any | null {
  for (let child = node?.firstChild ?? null; child; child = child.nextSibling) {
    if (child.name === name) {
      return child;
    }
  }
  return null;
}

function normalizeReferenceLabel(value: string): string {
  return value
    .trim()
    .replace(/\s+/g, ' ')
    .toLowerCase();
}

function parseReferenceLabel(text: string): string {
  if (!text.startsWith('[') || !text.endsWith(']')) {
    return '';
  }
  const value = text.slice(1, -1);
  return normalizeReferenceLabel(value);
}

function linkTextLabel(state: EditorState, linkNode: any): string {
  const text = state.doc.sliceString(linkNode.from, linkNode.to);
  const match = text.match(/^\[([^\]]+)\]/);
  return match ? normalizeReferenceLabel(match[1]) : '';
}

function getReferenceLinkLabel(state: EditorState, linkNode: any): string {
  const labelNode = childNodeByName(linkNode, 'LinkLabel');
  if (!labelNode) {
    return linkTextLabel(state, linkNode);
  }
  const parsed = parseReferenceLabel(state.doc.sliceString(labelNode.from, labelNode.to));
  if (!parsed) {
    return linkTextLabel(state, linkNode);
  }
  return parsed;
}

function getReferenceLinkMap(state: EditorState): Map<string, string> {
  const cacheKey = state.doc as unknown as object;
  const cached = linkReferenceCache.get(cacheKey);
  if (cached) {
    return cached;
  }

  const resolved = new Map<string, string>();
  const tree = resolvedSyntaxTree(state);
  tree.iterate({
    enter(node: any) {
      if (node.name !== 'LinkReference') {
        return;
      }
      const labelNode = childNodeByName(node.node, 'LinkLabel');
      const urlNode = childNodeByName(node.node, 'URL');
      if (!labelNode || !urlNode) {
        return;
      }
      const label = parseReferenceLabel(state.doc.sliceString(labelNode.from, labelNode.to));
      const href = normalizeSourceHref(state.doc.sliceString(urlNode.from, urlNode.to));
      if (!label || !href || resolved.has(label)) {
        return;
      }
      resolved.set(label, href);
    }
  });

  linkReferenceCache.set(cacheKey, resolved);
  return resolved;
}

function inlineLinkDestinationFromText(linkText: string): string {
  const match = linkText.match(/\]\(\s*(<[^>\n]+>|[^)\s]+)(?:\s+["'(].*)?\)$/);
  if (!match) {
    return '';
  }
  const rawDestination = match[1].trim();
  const unwrapped = rawDestination.startsWith('<') && rawDestination.endsWith('>')
    ? rawDestination.slice(1, -1)
    : rawDestination;
  return normalizeSourceHref(unwrapped);
}

function hrefFromSourceLinkNode(state: EditorState, linkNode: any): string {
  if (isWikiLinkNode(state, linkNode)) {
    return parseWikiLinkData(state, linkNode)?.href ?? '';
  }
  const urlNode = childNodeByName(linkNode, 'URL');
  if (urlNode) {
    return normalizeSourceHref(state.doc.sliceString(urlNode.from, urlNode.to));
  }
  const inlineDestination = inlineLinkDestinationFromText(state.doc.sliceString(linkNode.from, linkNode.to));
  if (inlineDestination) {
    return inlineDestination;
  }
  const referenceLabel = getReferenceLinkLabel(state, linkNode);
  if (!referenceLabel) {
    return '';
  }
  return getReferenceLinkMap(state).get(referenceLabel) ?? '';
}

function isPosInsideRange(pos: number, from: number, to: number, exactTextHit: boolean): boolean {
  if (exactTextHit) {
    return pos >= from && pos < to;
  }
  return pos >= from && pos <= to;
}

function hrefFromRawSourceUrlAtPos(state: EditorState, pos: number, exactTextHit: boolean = false): string {
  const line = state.doc.lineAt(pos);
  const lineText = state.doc.sliceString(line.from, line.to);
  for (const match of findRawSourceUrlMatches(lineText)) {
    const matchStart = line.from + match.index;
    const matchEnd = matchStart + match.length;
    if (!isPosInsideRange(pos, matchStart, matchEnd, exactTextHit)) {
      continue;
    }
    return match.href;
  }
  return '';
}

function hrefFromSourceSyntaxAtPos(state: EditorState, pos: number, options: LinkLookupOptions = {}): string {
  const exactTextHit = options.exactTextHit === true;
  const tree = resolvedSyntaxTree(state);
  const candidates = [tree.resolveInner(pos, -1), tree.resolveInner(pos, 1)];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    for (let node = candidate; node; node = node.parent) {
      const key = `${node.name}:${node.from}:${node.to}`;
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      if (!isPosInsideRange(pos, node.from, node.to, exactTextHit)) {
        continue;
      }

      if (node.name === 'Link') {
        const href = hrefFromSourceLinkNode(state, node);
        if (href) {
          return href;
        }
        continue;
      }

      if (node.name === 'Autolink' || node.name === 'Image') {
        const urlNode = childNodeByName(node, 'URL');
        const href = urlNode ? normalizeSourceHref(state.doc.sliceString(urlNode.from, urlNode.to)) : '';
        if (href) {
          return href;
        }
        continue;
      }

      if (node.name === 'URL') {
        const href = normalizeSourceHref(state.doc.sliceString(node.from, node.to));
        if (href) {
          return href;
        }
      }
    }
  }

  const rawHref = hrefFromRawSourceUrlAtPos(state, pos, exactTextHit);
  if (rawHref && linkSchemeRe.test(rawHref)) {
    return rawHref;
  }
  return '';
}

export function isPrimaryModifierPointerClick(event: MouseEvent | PointerEvent): boolean {
  if (event.altKey || event.shiftKey) {
    return false;
  }
  return event.metaKey || event.ctrlKey;
}

export function getDecoratedLinkHrefFromTarget(target: EventTarget | null): string {
  const targetElement = target instanceof Element ? target : target instanceof Node ? target.parentElement : null;
  if (!targetElement) {
    return '';
  }
  const linkElement = targetElement.closest('[data-meo-link-href]');
  if (!(linkElement instanceof Element)) {
    return '';
  }
  return linkElement.getAttribute('data-meo-link-href') || '';
}

export function getLinkHrefAtPointer(
  event: MouseEvent | PointerEvent,
  editorView: EditorView,
  options: LinkLookupOptions = {}
): string {
  const decoratedHref = getDecoratedLinkHrefFromTarget(event.target);
  if (decoratedHref) {
    return decoratedHref;
  }

  const pos = editorView.posAtCoords({ x: event.clientX, y: event.clientY });
  if (pos === null) {
    return '';
  }
  return hrefFromSourceSyntaxAtPos(editorView.state, pos, options);
}
