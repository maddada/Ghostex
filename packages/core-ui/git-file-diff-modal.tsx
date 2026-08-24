import { IconColumns2, IconLayoutRows, IconPilcrow, IconTextWrap } from '@tabler/icons-react';
import { useId, useMemo, useState, type ReactNode } from 'react';
import { Button } from '@/packages/components/ui/button';
import { cn } from '@/packages/components/utils';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/packages/components/ui/dialog';
import type { SidebarGitFileDiffDraft } from '../shared/sidebar-git';
import type { SidebarTheme } from '../shared/session-grid-contract';
import { AppTooltip } from './app-tooltip';

export type GitFileDiffModalDraft = SidebarGitFileDiffDraft;

export type GitFileDiffModalProps = {
  draft: GitFileDiffModalDraft;
  isOpen: boolean;
  onClose: () => void;
  theme?: SidebarTheme;
};

type DiffLineKind = 'addition' | 'context' | 'deletion' | 'hunk' | 'metadata' | 'raw';

type DiffLine = {
  content: string;
  kind: DiffLineKind;
  number: number;
};

export type GitDiffViewMode = 'split' | 'unified';

export function GitFileDiffModal({ draft, isOpen, onClose, theme = 'dark-1' }: GitFileDiffModalProps) {
  const descriptionId = useId();
  const titleId = useId();
  const hasStats = draft.additions !== undefined || draft.deletions !== undefined;
  const isDarkTheme = getSidebarThemeVariant(theme) === 'dark';

  /*
   * CDXC:TitlebarGit 2026-05-25-10:16:
   * The standalone file diff modal uses a sticky file header, monospaced patch rows, and addition/deletion coloring while staying inside Ghostex's existing modal host.
   */
  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onClose();
        }
      }}
      open={isOpen}
    >
      <DialogContent
        aria-describedby={descriptionId}
        aria-labelledby={titleId}
        className={cn(
          'ghostex-settings-shadcn settings-modal-dialog git-file-diff-modal-shadcn flex flex-col gap-0 overflow-hidden p-0 font-sans',
          isDarkTheme && 'dark'
        )}
        data-sidebar-theme={theme}
      >
        <DialogHeader className='git-file-diff-modal-header'>
          <DialogTitle className='git-file-diff-modal-title' id={titleId}>
            File diff
          </DialogTitle>
          <DialogDescription className='git-file-diff-modal-description' id={descriptionId}>
            <span className='git-file-diff-modal-path'>{draft.filePath}</span>
            {hasStats ? (
              <span className='git-file-diff-modal-stats'>
                <span className='changed-files-tree-additions'>+{draft.additions ?? 0}</span>
                <span className='changed-files-tree-stat-divider'>/</span>
                <span className='changed-files-tree-deletions'>-{draft.deletions ?? 0}</span>
              </span>
            ) : null}
          </DialogDescription>
        </DialogHeader>
        <div className='git-file-diff-modal-body'>
          <GitFileDiffPanel draft={draft} />
        </div>
      </DialogContent>
    </Dialog>
  );
}

function getSidebarThemeVariant(theme: SidebarTheme): 'dark' | 'light' {
  /**
   * CDXC:SidebarTheme 2026-06-15-01:43:
   * Git diff modals share the app modal theme contract: Dark 1/Dark 2 use
   * dark shadcn mode, while Light removes the dark class and uses light tokens.
   */
  return theme.startsWith('light-') || theme === 'plain-light' ? 'light' : 'dark';
}

type GitFileDiffPanelProps = {
  draft?: GitFileDiffModalDraft;
  hideWhitespace?: boolean;
  isLoading?: boolean;
  lineWrap?: boolean;
  onHideWhitespaceChange?: (hideWhitespace: boolean) => void;
  onLineWrapChange?: (lineWrap: boolean) => void;
  onViewModeChange?: (viewMode: GitDiffViewMode) => void;
  placeholder?: string;
  showToolbar?: boolean;
  viewMode?: GitDiffViewMode;
};

export function GitFileDiffControls({
  hideWhitespace,
  lineWrap,
  onHideWhitespaceChange,
  onLineWrapChange,
  onViewModeChange,
  viewMode,
}: {
  hideWhitespace: boolean;
  lineWrap: boolean;
  onHideWhitespaceChange: (hideWhitespace: boolean) => void;
  onLineWrapChange: (lineWrap: boolean) => void;
  onViewModeChange: (viewMode: GitDiffViewMode) => void;
  viewMode: GitDiffViewMode;
}) {
  const nextViewMode = viewMode === 'split' ? 'unified' : 'split';
  const isLineWrapForced = viewMode === 'split';
  const effectiveLineWrap = isLineWrapForced || lineWrap;
  const viewModeLabel = nextViewMode === 'split' ? 'Switch to split diff' : 'Switch to unified diff';
  const lineWrapLabel = isLineWrapForced
    ? 'Line wrapping is forced on when side by side is used.'
    : effectiveLineWrap
      ? 'Disable line wrapping'
      : 'Enable line wrapping';
  const whitespaceLabel = hideWhitespace ? 'Show whitespace changes' : 'Hide whitespace changes';
  return (
    <div className='git-file-diff-controls' aria-label='Diff display options'>
      <AppTooltip content={viewModeLabel}>
        <Button
          aria-label={viewModeLabel}
          aria-pressed={viewMode === 'split'}
          className='git-file-diff-control-button'
          data-active={viewMode === 'split' ? 'true' : 'false'}
          onClick={() => onViewModeChange(nextViewMode)}
          size='icon-xs'
          type='button'
          variant='outline'
        >
          {viewMode === 'split' ? (
            <IconLayoutRows aria-hidden='true' data-icon='inline-start' />
          ) : (
            <IconColumns2 aria-hidden='true' data-icon='inline-start' />
          )}
        </Button>
      </AppTooltip>
      <AppTooltip content={lineWrapLabel}>
        <span className='git-file-diff-control-tooltip-trigger' tabIndex={isLineWrapForced ? 0 : -1}>
          <Button
            aria-label={lineWrapLabel}
            aria-pressed={effectiveLineWrap}
            className='git-file-diff-control-button'
            data-active={effectiveLineWrap ? 'true' : 'false'}
            disabled={isLineWrapForced}
            onClick={() => onLineWrapChange(!lineWrap)}
            size='icon-xs'
            type='button'
            variant='outline'
          >
            <IconTextWrap aria-hidden='true' data-icon='inline-start' />
          </Button>
        </span>
      </AppTooltip>
      <AppTooltip content={whitespaceLabel}>
        <Button
          aria-label={whitespaceLabel}
          aria-pressed={hideWhitespace}
          className='git-file-diff-control-button'
          data-active={hideWhitespace ? 'true' : 'false'}
          onClick={() => onHideWhitespaceChange(!hideWhitespace)}
          size='icon-xs'
          type='button'
          variant='outline'
        >
          <IconPilcrow aria-hidden='true' data-icon='inline-start' />
        </Button>
      </AppTooltip>
    </div>
  );
}

export function GitFileDiffPanel({
  draft,
  hideWhitespace,
  isLoading = false,
  lineWrap,
  onHideWhitespaceChange,
  onLineWrapChange,
  onViewModeChange,
  placeholder = 'Select a file to preview its diff.',
  showToolbar = true,
  viewMode,
}: GitFileDiffPanelProps) {
  const [internalViewMode, setInternalViewMode] = useState<GitDiffViewMode>('unified');
  const [internalHideWhitespace, setInternalHideWhitespace] = useState(false);
  const [internalLineWrap, setInternalLineWrap] = useState(false);
  const resolvedViewMode = viewMode ?? internalViewMode;
  const resolvedHideWhitespace = hideWhitespace ?? internalHideWhitespace;
  const resolvedLineWrap = lineWrap ?? internalLineWrap;
  const effectiveLineWrap = resolvedViewMode === 'split' || resolvedLineWrap;
  const setViewMode = onViewModeChange ?? setInternalViewMode;
  const setHideWhitespace = onHideWhitespaceChange ?? setInternalHideWhitespace;
  const setLineWrap = onLineWrapChange ?? setInternalLineWrap;
  const lines = useMemo(() => parseDiffLines(draft?.patch ?? ''), [draft?.patch]);
  const visibleLines = useMemo(
    () => (resolvedHideWhitespace ? lines.filter((line) => !isWhitespaceOnlyChangeLine(line)) : lines),
    [resolvedHideWhitespace, lines]
  );

  /*
   * CDXC:TitlebarGit 2026-06-05-20:59:
   * Commit review now embeds file diffs in the right side of the widened review modal instead of opening a second modal. Keep diff display controls behind a single overflow menu so Unified, Split, and Hide whitespace do not compete with the selected file path for header space.
   *
   * CDXC:TitlebarGit 2026-06-08-04:07:
   * Diff display controls use a direct icon-control pattern:
   * one tooltip button toggles unified/split, one toggles line wrapping, and one
   * toggles whitespace-only changes. The commit modal can host those controls
   * in its file header while the standalone file diff keeps the same controls
   * above its patch surface.
   *
   * CDXC:TitlebarGit 2026-06-08-04:47:
   * Side-by-side diff mode must force line wrapping so each half stays readable
   * in the commit and standalone diff panes. Keep the wrap button disabled in
   * split view and explain the forced state in its tooltip.
   */
  if (isLoading) {
    return <div className='git-file-diff-placeholder'>Loading diff...</div>;
  }
  if (!draft) {
    return <div className='git-file-diff-placeholder'>{placeholder}</div>;
  }

  return (
    <div className='git-file-diff-panel' data-has-toolbar={showToolbar ? 'true' : 'false'}>
      {showToolbar ? (
        <GitFileDiffControls
          hideWhitespace={resolvedHideWhitespace}
          lineWrap={effectiveLineWrap}
          onHideWhitespaceChange={setHideWhitespace}
          onLineWrapChange={setLineWrap}
          onViewModeChange={setViewMode}
          viewMode={resolvedViewMode}
        />
      ) : null}
      <div
        className='git-file-diff-surface'
        data-view-mode={resolvedViewMode}
        data-wrap-lines={effectiveLineWrap ? 'true' : 'false'}
        role='document'
      >
        {resolvedViewMode === 'split'
          ? visibleLines.map((line) => <SplitDiffLine key={line.number} line={line} />)
          : visibleLines.map((line) => <UnifiedDiffLine key={line.number} line={line} />)}
      </div>
    </div>
  );
}

function UnifiedDiffLine({ line }: { line: DiffLine }) {
  return (
    <div className='git-file-diff-line' data-kind={line.kind}>
      <span aria-hidden='true' className='git-file-diff-line-number'>
        {line.number}
      </span>
      <pre className='git-file-diff-line-content'>{renderDiffLineContent(line, line.content)}</pre>
    </div>
  );
}

function SplitDiffLine({ line }: { line: DiffLine }) {
  if (line.kind === 'metadata' || line.kind === 'hunk' || line.kind === 'raw') {
    return <UnifiedDiffLine line={line} />;
  }
  const leftContent = line.kind === 'deletion' || line.kind === 'context' ? line.content : '';
  const rightContent = line.kind === 'addition' ? line.content : line.kind === 'context' ? line.content : '';
  return (
    <div className='git-file-diff-split-line' data-kind={line.kind}>
      <span aria-hidden='true' className='git-file-diff-line-number'>
        {line.number}
      </span>
      <pre className='git-file-diff-line-content git-file-diff-split-cell'>
        {renderDiffLineContent(line, leftContent)}
      </pre>
      <span aria-hidden='true' className='git-file-diff-line-number'>
        {line.number}
      </span>
      <pre className='git-file-diff-line-content git-file-diff-split-cell'>
        {renderDiffLineContent(line, rightContent)}
      </pre>
    </div>
  );
}

type DiffSyntaxTokenKind =
  | 'attribute'
  | 'comment'
  | 'function'
  | 'keyword'
  | 'literal'
  | 'marker'
  | 'number'
  | 'operator'
  | 'plain'
  | 'string'
  | 'type';

type DiffSyntaxToken = {
  kind: DiffSyntaxTokenKind;
  text: string;
};

const DIFF_SYNTAX_KEYWORDS = new Set([
  'as',
  'async',
  'await',
  'break',
  'case',
  'catch',
  'class',
  'const',
  'continue',
  'default',
  'defer',
  'do',
  'else',
  'enum',
  'export',
  'extension',
  'final',
  'for',
  'from',
  'func',
  'function',
  'guard',
  'if',
  'implements',
  'import',
  'in',
  'interface',
  'let',
  'mut',
  'new',
  'override',
  'package',
  'private',
  'protected',
  'protocol',
  'public',
  'return',
  'static',
  'struct',
  'super',
  'switch',
  'throws',
  'throw',
  'try',
  'type',
  'var',
  'where',
  'while',
]);

const DIFF_SYNTAX_LITERALS = new Set(['false', 'nil', 'null', 'None', 'self', 'Self', 'this', 'true', 'undefined']);

const DIFF_SYNTAX_PRIMITIVE_TYPES = new Set([
  'Array',
  'Bool',
  'Boolean',
  'Date',
  'Dictionary',
  'Double',
  'Float',
  'Int',
  'Map',
  'Number',
  'Object',
  'Set',
  'String',
  'Void',
]);

/*
 * CDXC:TitlebarGit 2026-06-08-04:20:
 * Commit review diffs need visible syntax coloring in both the inline commit
 * diff and standalone file diff without replacing the existing split/unified
 * row model. Tokenize each rendered patch row so the diff layout, wrapping,
 * and line-number behavior stay shared across both surfaces.
 */
function renderDiffLineContent(line: DiffLine, content: string): ReactNode {
  if (!content) {
    return ' ';
  }
  if (line.kind !== 'addition' && line.kind !== 'context' && line.kind !== 'deletion') {
    return content;
  }
  return tokenizeDiffLineContent(line.kind, content).map((token, index) => (
    <span className={`git-file-diff-token git-file-diff-token-${token.kind}`} key={`${index}:${token.kind}`}>
      {token.text}
    </span>
  ));
}

function tokenizeDiffLineContent(kind: DiffLineKind, content: string): DiffSyntaxToken[] {
  const tokens: DiffSyntaxToken[] = [];
  let code = content;
  if (
    (kind === 'addition' || kind === 'context' || kind === 'deletion') &&
    (content.startsWith('+') || content.startsWith('-') || content.startsWith(' '))
  ) {
    tokens.push({ kind: 'marker', text: content.slice(0, 1) });
    code = content.slice(1);
  }
  tokens.push(...tokenizeCodeContent(code));
  return tokens.length > 0 ? tokens : [{ kind: 'plain', text: ' ' }];
}

function tokenizeCodeContent(code: string): DiffSyntaxToken[] {
  const tokens: DiffSyntaxToken[] = [];
  let index = 0;
  while (index < code.length) {
    const char = code[index];
    if (isWhitespace(char)) {
      const start = index;
      while (index < code.length && isWhitespace(code[index])) {
        index += 1;
      }
      tokens.push({ kind: 'plain', text: code.slice(start, index) });
      continue;
    }
    if (char === '/' && code[index + 1] === '/') {
      tokens.push({ kind: 'comment', text: code.slice(index) });
      break;
    }
    if (char === '/' && code[index + 1] === '*') {
      const endIndex = code.indexOf('*/', index + 2);
      const cursor = endIndex >= 0 ? endIndex + 2 : code.length;
      tokens.push({ kind: 'comment', text: code.slice(index, cursor) });
      index = cursor;
      continue;
    }
    if (char === '#' && (index === 0 || isWhitespace(code[index - 1]))) {
      tokens.push({ kind: 'comment', text: code.slice(index) });
      break;
    }
    if (char === '"' || char === "'" || char === '`') {
      const cursor = readQuotedString(code, index, char);
      tokens.push({ kind: 'string', text: code.slice(index, cursor) });
      index = cursor;
      continue;
    }
    if (char === '@' && isIdentifierStart(code[index + 1] ?? '')) {
      const cursor = readIdentifier(code, index + 1);
      tokens.push({ kind: 'attribute', text: code.slice(index, cursor) });
      index = cursor;
      continue;
    }
    if (isDigit(char)) {
      const start = index;
      while (index < code.length && /[0-9a-fA-F._xXoObB]/.test(code[index])) {
        index += 1;
      }
      tokens.push({ kind: 'number', text: code.slice(start, index) });
      continue;
    }
    if (isIdentifierStart(char)) {
      const cursor = readIdentifier(code, index);
      const word = code.slice(index, cursor);
      tokens.push({ kind: classifyIdentifierToken(code, word, cursor), text: word });
      index = cursor;
      continue;
    }
    const operatorMatch = /^[{}\[\]().,:;?<>+=*\/%!&|^~-]+/.exec(code.slice(index));
    if (operatorMatch) {
      tokens.push({ kind: 'operator', text: operatorMatch[0] });
      index += operatorMatch[0].length;
      continue;
    }
    tokens.push({ kind: 'plain', text: char });
    index += 1;
  }
  return tokens;
}

function classifyIdentifierToken(code: string, word: string, cursor: number): DiffSyntaxTokenKind {
  if (DIFF_SYNTAX_KEYWORDS.has(word)) {
    return 'keyword';
  }
  if (DIFF_SYNTAX_LITERALS.has(word)) {
    return 'literal';
  }
  if (DIFF_SYNTAX_PRIMITIVE_TYPES.has(word) || /^[A-Z]/.test(word)) {
    return 'type';
  }
  if (nextNonWhitespaceCharacter(code, cursor) === '(') {
    return 'function';
  }
  return 'plain';
}

function readIdentifier(code: string, start: number): number {
  let index = start;
  while (index < code.length && isIdentifierPart(code[index])) {
    index += 1;
  }
  return index;
}

function readQuotedString(code: string, start: number, quote: string): number {
  let index = start + 1;
  let isEscaped = false;
  while (index < code.length) {
    const char = code[index];
    if (isEscaped) {
      isEscaped = false;
    } else if (char === '\\') {
      isEscaped = true;
    } else if (char === quote) {
      index += 1;
      break;
    }
    index += 1;
  }
  return index;
}

function nextNonWhitespaceCharacter(code: string, start: number): string | undefined {
  let index = start;
  while (index < code.length) {
    if (!isWhitespace(code[index])) {
      return code[index];
    }
    index += 1;
  }
  return undefined;
}

function isWhitespace(char: string | undefined): boolean {
  return char === ' ' || char === '\t';
}

function isDigit(char: string | undefined): boolean {
  return char !== undefined && char >= '0' && char <= '9';
}

function isIdentifierStart(char: string | undefined): boolean {
  return char !== undefined && /[A-Za-z_$]/.test(char);
}

function isIdentifierPart(char: string | undefined): boolean {
  return char !== undefined && /[A-Za-z0-9_$]/.test(char);
}

function parseDiffLines(patch: string): DiffLine[] {
  const rawLines = patch.trimEnd().split('\n');
  const lines = rawLines.some((line) => line.length > 0) ? rawLines : ['No diff is available for this file.'];
  return lines.map((content, index) => ({
    content,
    kind: classifyDiffLine(content),
    number: index + 1,
  }));
}

function classifyDiffLine(line: string): DiffLineKind {
  if (line.startsWith('@@')) {
    return 'hunk';
  }
  if (
    line.startsWith('diff --git') ||
    line.startsWith('index ') ||
    line.startsWith('new file mode') ||
    line.startsWith('deleted file mode') ||
    line.startsWith('similarity index') ||
    line.startsWith('rename from ') ||
    line.startsWith('rename to ') ||
    line.startsWith('--- ') ||
    line.startsWith('+++ ')
  ) {
    return 'metadata';
  }
  if (line.startsWith('+')) {
    return 'addition';
  }
  if (line.startsWith('-')) {
    return 'deletion';
  }
  if (line.startsWith('No diff is available')) {
    return 'raw';
  }
  return 'context';
}

function isWhitespaceOnlyChangeLine(line: DiffLine): boolean {
  if (line.kind !== 'addition' && line.kind !== 'deletion') {
    return false;
  }
  return line.content.slice(1).trim().length === 0;
}
