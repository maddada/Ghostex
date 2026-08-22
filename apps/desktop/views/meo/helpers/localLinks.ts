// @ts-nocheck
const SCHEME_RE = /^[a-z][a-z0-9+.-]*:/i;
const HOSTNAME_RE = /^[a-z0-9-]+(?:\.[a-z0-9-]+)+$/i;

const localLinkStatusByTarget = new Map<string, boolean>();
let vscodeApi: any = null;
let localLinkRequestCounter = 0;
let latestLocalLinkRequestId = '';
let pendingLocalLinkStatusRefresh: number | null = null;
const localLinkStatusDebounceMs = 1000;

export interface LocalLinkRefreshContext {
  refreshDecorations: () => void;
}

let refreshContext: LocalLinkRefreshContext | null = null;

export const initializeLocalLinkHandling = (vscode: any): void => {
  vscodeApi = vscode;
};

export const setLocalLinkRefreshContext = (context: LocalLinkRefreshContext): void => {
  refreshContext = context;
};

export const scheduleLocalLinkStatusRefresh = (text: string): void => {
  if (pendingLocalLinkStatusRefresh !== null) {
    window.clearTimeout(pendingLocalLinkStatusRefresh);
  }
  pendingLocalLinkStatusRefresh = window.setTimeout(() => {
    pendingLocalLinkStatusRefresh = null;
    requestLocalLinkStatuses(text);
  }, localLinkStatusDebounceMs);
};

export const cancelPendingLocalLinkStatusRefresh = (): void => {
  if (pendingLocalLinkStatusRefresh !== null) {
    window.clearTimeout(pendingLocalLinkStatusRefresh);
    pendingLocalLinkStatusRefresh = null;
  }
};

export const requestLocalLinkStatuses = (text: string): void => {
  const targets = collectLocalLinkTargets(text);
  if (!targets.length) {
    replaceLocalLinkStatuses([]);
    refreshContext?.refreshDecorations();
    return;
  }

  const requestId = `local-link-${localLinkRequestCounter++}`;
  latestLocalLinkRequestId = requestId;
  vscodeApi?.postMessage({ type: 'resolveLocalLinks', requestId, targets });
};

export const handleResolvedLocalLinks = (message: { requestId: string; results?: Array<{ target: string; exists: boolean }> }): boolean => {
  if (message.requestId !== latestLocalLinkRequestId) {
    return false;
  }
  replaceLocalLinkStatuses(message.results ?? []);
  return true;
};

export function replaceLocalLinkStatuses(entries: Array<{ target: string; exists: boolean }>): void {
  localLinkStatusByTarget.clear();
  for (const entry of entries) {
    if (!entry || typeof entry.target !== 'string') {
      continue;
    }
    const target = normalizeLocalLinkTarget(entry.target);
    if (!target) {
      continue;
    }
    localLinkStatusByTarget.set(target, entry.exists === true);
  }
}

export function getLocalLinkStatus(target: string | null | undefined): boolean | null {
  const normalized = normalizeLocalLinkTarget(target ?? '');
  if (!normalized || !localLinkStatusByTarget.has(normalized)) {
    return null;
  }
  return localLinkStatusByTarget.get(normalized) === true;
}

export function normalizeLocalLinkTarget(rawTarget: string): string {
  let target = `${rawTarget ?? ''}`.trim();
  if (!target) {
    return '';
  }
  if (target.startsWith('<') && target.endsWith('>')) {
    target = target.slice(1, -1).trim();
  }
  return target;
}

export function isLikelyLocalLinkTarget(rawTarget: string): boolean {
  const normalized = normalizeLocalLinkTarget(rawTarget);
  if (!normalized || normalized.startsWith('#')) {
    return false;
  }
  if (/^\/\//.test(normalized)) {
    return false;
  }
  if (/^file:/i.test(normalized)) {
    return true;
  }

  const [targetPath = ''] = normalized.split(/[?#]/, 1);
  if (!targetPath) {
    return false;
  }
  if (SCHEME_RE.test(targetPath)) {
    return false;
  }

  const pathLike = targetPath.replace(/\\/g, '/');
  if (!pathLike) {
    return false;
  }
  if (
    pathLike.startsWith('./') ||
    pathLike.startsWith('../') ||
    pathLike.startsWith('/') ||
    pathLike.startsWith('~')
  ) {
    return true;
  }
  if (/^[a-zA-Z]:\//.test(pathLike)) {
    return true;
  }
  if (pathLike.toLowerCase().startsWith('www.')) {
    return false;
  }

  const firstSegment = pathLike.split('/', 1)[0] ?? '';
  const hostPart = firstSegment.includes(':') ? firstSegment.split(':', 1)[0] : firstSegment;
  if (hostPart === 'localhost') {
    return false;
  }
  if (/^\d{1,3}(?:\.\d{1,3}){3}$/.test(hostPart)) {
    return false;
  }
  if (HOSTNAME_RE.test(hostPart)) {
    return false;
  }

  return true;
}

export const collectLocalLinkTargets = (text: string): string[] => {
  const targets = new Set<string>();

  for (let i = 0; i < text.length; i += 1) {
    if (text[i] !== '[' || isEscapedAt(text, i)) {
      continue;
    }
    if (i > 0 && text[i - 1] === '!') {
      continue;
    }

    const closeBracket = findClosingBracket(text, i + 1);
    if (closeBracket < 0) {
      continue;
    }

    let cursor = closeBracket + 1;
    while (cursor < text.length && (text[cursor] === ' ' || text[cursor] === '\t')) {
      cursor += 1;
    }
    if (text[cursor] !== '(') {
      continue;
    }

    const destination = consumeLinkDestination(text, cursor + 1);
    if (destination.nextIndex < 0) {
      continue;
    }
    const normalized = normalizeLocalLinkTarget(destination.target);
    if (normalized && isLikelyLocalLinkTarget(normalized)) {
      targets.add(normalized);
    }
    i = destination.nextIndex;
  }

  return Array.from(targets);
};

function isEscapedAt(text: string, index: number): boolean {
  let slashCount = 0;
  for (let i = index - 1; i >= 0 && text[i] === '\\'; i -= 1) {
    slashCount += 1;
  }
  return (slashCount % 2) === 1;
}

function findClosingBracket(text: string, from: number): number {
  for (let i = from; i < text.length; i += 1) {
    if (text[i] === ']' && !isEscapedAt(text, i)) {
      return i;
    }
  }
  return -1;
}

function consumeLinkDestination(text: string, from: number): { target: string; nextIndex: number } {
  let depth = 1;
  for (let i = from; i < text.length; i += 1) {
    const char = text[i];
    if (char === '\n' || char === '\r') {
      return { target: '', nextIndex: -1 };
    }
    if (char === '(' && !isEscapedAt(text, i)) {
      depth += 1;
      continue;
    }
    if (char === ')' && !isEscapedAt(text, i)) {
      depth -= 1;
      if (depth === 0) {
        const inside = text.slice(from, i);
        return { target: parseDestinationToken(inside), nextIndex: i };
      }
    }
  }
  return { target: '', nextIndex: -1 };
}

function parseDestinationToken(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) {
    return '';
  }

  if (trimmed.startsWith('<')) {
    const close = trimmed.indexOf('>');
    if (close > 1) {
      return trimmed.slice(1, close);
    }
    return '';
  }

  let end = trimmed.length;
  for (let i = 0; i < trimmed.length; i += 1) {
    if (/\s/.test(trimmed[i])) {
      end = i;
      break;
    }
  }
  return trimmed.slice(0, end);
}
