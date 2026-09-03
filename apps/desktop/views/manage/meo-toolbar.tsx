import { Decoration, EditorView, type DecorationSet } from '@codemirror/view';
import { IconMessagePlus } from '@tabler/icons-react';
import {
  Bold as MeoBoldIcon,
  Brackets as MeoBracketsIcon,
  CaseSensitive as MeoCaseSensitiveIcon,
  ChevronDown as MeoChevronDownIcon,
  ChevronUp as MeoChevronUpIcon,
  Code as MeoCodeIcon,
  GitCompare as MeoGitCompareIcon,
  Hash as MeoHashIcon,
  Heading as MeoHeadingIcon,
  Heading1 as MeoHeading1Icon,
  Heading2 as MeoHeading2Icon,
  Heading3 as MeoHeading3Icon,
  Heading4 as MeoHeading4Icon,
  Heading5 as MeoHeading5Icon,
  Heading6 as MeoHeading6Icon,
  Image as MeoImageIcon,
  Italic as MeoItalicIcon,
  Keyboard as MeoKeyboardIcon,
  Link as MeoLinkIcon,
  List as MeoListIcon,
  ListOrdered as MeoListOrderedIcon,
  ListTodo as MeoListTodoIcon,
  Minus as MeoMinusIcon,
  PanelLeftRightDashed as MeoPanelLeftRightDashedIcon,
  Quote as MeoQuoteIcon,
  Replace as MeoReplaceIcon,
  ReplaceAll as MeoReplaceAllIcon,
  Search as MeoSearchIcon,
  Strikethrough as MeoStrikethroughIcon,
  Table2 as MeoTable2Icon,
  Terminal as MeoTerminalIcon,
  WholeWord as MeoWholeWordIcon,
  X as MeoXIcon,
} from 'lucide-react';
import { RangeSetBuilder, StateEffect, StateField } from '@codemirror/state';
import { type KeyboardEvent as ReactKeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { MANAGE_MEO_CODE_COLOR, MANAGE_MEO_HEADING_COLOR, MANAGE_MEO_VARIABLE_COLOR } from './constants';
import {
  ManageAnnotation,
  ManageAnnotationPreview,
  ManageCapturedSelection,
  ManageMeoAnnotationDecoration,
  ManageMeoMode,
  ManageMeoSelectionState,
  ManageResolvedAnnotationRange,
  ManageSelectionAnchor,
} from './types';
import { ManageTooltipButton } from './manage-tooltip-button';
import {
  defaultManageSelectionAnchor,
  manageAnnotationColor,
  meoSelectionToolbarPosition,
  normalizeAnnotationQuote,
} from './annotation-store';
import { applyThemeSettings as applyMeoThemeSettings } from '../meo/helpers/theme';

export const MANAGE_MEO_THEME = {
  backgroundColor: '#101112',
  colors: {
    base01: '#e5e7eb',
    base02: '#8b949e',
    base03: '#30363d',
    base04: MANAGE_MEO_HEADING_COLOR,
    base05: '#7dd3fc',
    base06: '#67e8f9',
    base07: '#fde68a',
    base08: '#c084fc',
    base09: '#86efac',
  },
  fonts: {
    liveFont: '',
    sourceFont: '',
    liveFontWeight: '450',
    sourceFontWeight: '450',
    liveFontSize: 14,
    sourceFontSize: 14,
    h1FontSize: 1.5,
    h1FontWeight: '720',
    h2FontSize: 1.35,
    h2FontWeight: '700',
    h3FontSize: 1.18,
    h3FontWeight: '700',
    h4FontSize: 1.08,
    h4FontWeight: '680',
    h5FontSize: 1,
    h5FontWeight: '660',
    h6FontSize: 0.94,
    h6FontWeight: '650',
    liveLineHeight: 1.55,
    sourceLineHeight: 1.55,
  },
  id: 'ghostex-manage-meo',
  name: 'Ghostex Docs Meo',
  syntaxTokens: {
    atom: MANAGE_MEO_VARIABLE_COLOR,
    bool: MANAGE_MEO_VARIABLE_COLOR,
    constant: MANAGE_MEO_VARIABLE_COLOR,
    definedVariable: MANAGE_MEO_VARIABLE_COLOR,
    monospace: MANAGE_MEO_CODE_COLOR,
    regexp: MANAGE_MEO_VARIABLE_COLOR,
    specialVariable: MANAGE_MEO_VARIABLE_COLOR,
    specialString: MANAGE_MEO_VARIABLE_COLOR,
    string: MANAGE_MEO_VARIABLE_COLOR,
    variableName: MANAGE_MEO_VARIABLE_COLOR,
  },
};
export const manageMeoAnnotationEffect = StateEffect.define<ManageMeoAnnotationDecoration[]>();

export const manageMeoAnnotationField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(value, transaction) {
    let nextValue = value.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (effect.is(manageMeoAnnotationEffect)) {
        nextValue = buildManageMeoAnnotationDecorations(effect.value);
      }
    }
    return nextValue;
  },
  provide(field) {
    return EditorView.decorations.from(field);
  },
});

export function ManageMeoTopToolbar({
  contentMaxWidthEnabled,
  currentMode,
  findCaseSensitive,
  findOpen,
  findQuery,
  findReplacement,
  findStatus,
  findStatusIsError,
  findWholeWord,
  gitGutterVisible,
  lineNumbersVisible,
  onCloseFind,
  onFindCaseSensitiveChange,
  onFindOpenChange,
  onFindQueryChange,
  onFindReplacementChange,
  onFindWholeWordChange,
  onFormat,
  onModeChange,
  onReplaceAll,
  onReplaceCurrent,
  onRunFind,
  onToggleContentMaxWidth,
  onToggleGitGutter,
  onToggleLineNumbers,
}: {
  contentMaxWidthEnabled: boolean;
  currentMode: ManageMeoMode;
  findCaseSensitive: boolean;
  findOpen: boolean;
  findQuery: string;
  findReplacement: string;
  findStatus: string;
  findStatusIsError: boolean;
  findWholeWord: boolean;
  gitGutterVisible: boolean;
  lineNumbersVisible: boolean;
  onCloseFind: () => void;
  onFindCaseSensitiveChange: (enabled: boolean) => void;
  onFindOpenChange: (open: boolean) => void;
  onFindQueryChange: (query: string) => void;
  onFindReplacementChange: (replacement: string) => void;
  onFindWholeWordChange: (enabled: boolean) => void;
  onFormat: (action: string, level?: number | { cols?: number; rows?: number }) => void;
  onModeChange: (mode: ManageMeoMode) => void;
  onReplaceAll: () => void;
  onReplaceCurrent: () => void;
  onRunFind: (backward?: boolean) => void;
  onToggleContentMaxWidth: () => void;
  onToggleGitGutter: () => void;
  onToggleLineNumbers: () => void;
}) {
  const [tableSize, setTableSize] = useState({ cols: 1, rows: 1 });
  const findInputRef = useRef<HTMLInputElement | null>(null);
  const fullToolbarWidthRef = useRef(0);
  const toolbarRef = useRef<HTMLDivElement | null>(null);
  const [hideOptionalControls, setHideOptionalControls] = useState(false);
  const headingIcons = [
    MeoHeading1Icon,
    MeoHeading2Icon,
    MeoHeading3Icon,
    MeoHeading4Icon,
    MeoHeading5Icon,
    MeoHeading6Icon,
  ];

  const runFindFromKeyboard = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key !== 'Enter') {
      return;
    }
    event.preventDefault();
    onRunFind(event.shiftKey);
  };

  const runReplaceFromKeyboard = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key !== 'Enter') {
      return;
    }
    event.preventDefault();
    onReplaceCurrent();
  };

  useEffect(() => {
    if (!findOpen) {
      return;
    }
    findInputRef.current?.focus();
    findInputRef.current?.select();
  }, [findOpen]);

  useLayoutEffect(() => {
    const toolbar = toolbarRef.current;
    if (!toolbar) {
      return undefined;
    }
    /*
     * CDXC:Docs 2026-06-30-13:45:
     * The three secondary right-side Markdown toolbar buttons should stay visible until the rendered toolbar actually overflows. Measure the full toolbar while those buttons are visible, then restore them only after the available width can fit that measured full row again.
     */
    let animationFrame: number | undefined;
    const measureToolbar = () => {
      animationFrame = undefined;
      const availableWidth = toolbar.clientWidth;
      if (availableWidth <= 0) {
        return;
      }
      if (!hideOptionalControls) {
        const toolbarStyle = window.getComputedStyle(toolbar);
        const toolbarGap = Number.parseFloat(toolbarStyle.columnGap || toolbarStyle.gap || '0') || 0;
        const horizontalPadding =
          (Number.parseFloat(toolbarStyle.paddingLeft) || 0) + (Number.parseFloat(toolbarStyle.paddingRight) || 0);
        const formatGroup = toolbar.querySelector(':scope > .format-group');
        const rightGroup = toolbar.querySelector(':scope > .right-group');
        const modeGroup = toolbar.querySelector(':scope > .mode-group');
        const requiredWidth =
          horizontalPadding +
          (formatGroup instanceof HTMLElement ? formatGroup.scrollWidth : 0) +
          (rightGroup instanceof HTMLElement ? rightGroup.getBoundingClientRect().width : 0) +
          (modeGroup instanceof HTMLElement ? modeGroup.getBoundingClientRect().width : 0) +
          toolbarGap * 2;
        fullToolbarWidthRef.current = requiredWidth;
        setHideOptionalControls(requiredWidth > availableWidth + 1);
        return;
      }
      const fullToolbarWidth = fullToolbarWidthRef.current;
      if (fullToolbarWidth > 0 && availableWidth >= fullToolbarWidth + 6) {
        setHideOptionalControls(false);
      }
    };
    const scheduleMeasure = () => {
      if (animationFrame !== undefined) {
        window.cancelAnimationFrame(animationFrame);
      }
      animationFrame = window.requestAnimationFrame(measureToolbar);
    };
    scheduleMeasure();
    const resizeObserver = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(scheduleMeasure);
    if (resizeObserver) {
      resizeObserver.observe(toolbar);
    } else {
      window.addEventListener('resize', scheduleMeasure);
    }
    return () => {
      if (animationFrame !== undefined) {
        window.cancelAnimationFrame(animationFrame);
      }
      resizeObserver?.disconnect();
      if (!resizeObserver) {
        window.removeEventListener('resize', scheduleMeasure);
      }
    };
  }, [currentMode, hideOptionalControls]);

  return (
    <div
      aria-label='Editor toolbar'
      className='mode-toolbar'
      data-optional-controls-hidden={String(hideOptionalControls)}
      ref={toolbarRef}
      role='toolbar'
    >
      <div aria-label='Formatting' className='format-group' role='group'>
        <div className='heading-wrapper'>
          <ManageTooltipButton
            className='format-button'
            data-action='heading'
            onClick={() => onFormat('heading', 1)}
            tooltip='Heading'
            type='button'
          >
            <MeoHeadingIcon aria-hidden='true' size={18} />
          </ManageTooltipButton>
          <div className='heading-dropdown-wrapper'>
            <div aria-label='Heading levels' className='heading-dropdown' role='menu'>
              {headingIcons.map((HeadingIcon, index) => {
                const level = index + 1;
                return (
                  <ManageTooltipButton
                    className='heading-dropdown-option'
                    data-level={level}
                    key={level}
                    onClick={() => onFormat('heading', level)}
                    tooltip={`Heading ${level}`}
                    type='button'
                  >
                    <HeadingIcon aria-hidden='true' size={18} />
                  </ManageTooltipButton>
                );
              })}
            </div>
          </div>
        </div>
        <ManageTooltipButton
          className='format-button'
          data-action='bulletList'
          onClick={() => onFormat('bulletList')}
          tooltip='Bullet List'
          type='button'
        >
          <MeoListIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          className='format-button'
          data-action='numberedList'
          onClick={() => onFormat('numberedList')}
          tooltip='Numbered List'
          type='button'
        >
          <MeoListOrderedIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          className='format-button'
          data-action='task'
          onClick={() => onFormat('task')}
          tooltip='Task'
          type='button'
        >
          <MeoListTodoIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
        <div className='format-separator' role='separator' />
        <div className='table-wrapper'>
          <ManageTooltipButton
            className='format-button'
            data-action='table'
            onClick={() => onFormat('table', tableSize)}
            tooltip='Table'
            type='button'
          >
            <MeoTable2Icon aria-hidden='true' size={18} />
          </ManageTooltipButton>
          <div className='table-dropdown-wrapper'>
            <div className='table-dropdown'>
              <div className='table-grid'>
                {Array.from({ length: 25 }, (_, index) => {
                  const row = Math.floor(index / 5) + 1;
                  const col = (index % 5) + 1;
                  const isHighlighted = col <= tableSize.cols && row <= tableSize.rows;
                  return (
                    <button
                      aria-label={`${col} by ${row} table`}
                      className={`table-grid-cell${isHighlighted ? ' is-highlighted' : ''}`}
                      data-col={col}
                      data-row={row}
                      key={`${col}-${row}`}
                      onClick={() => onFormat('table', { cols: col, rows: row })}
                      onMouseEnter={() => setTableSize({ cols: col, rows: row })}
                      type='button'
                    />
                  );
                })}
              </div>
              <div className='table-size-label'>
                {tableSize.cols} x {tableSize.rows}
              </div>
            </div>
          </div>
        </div>
        <ManageTooltipButton
          className='format-button'
          data-action='codeBlock'
          onClick={() => onFormat('codeBlock')}
          tooltip='Code Block'
          type='button'
        >
          <MeoCodeIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          className='format-button'
          data-action='link'
          onClick={() => onFormat('link')}
          tooltip='Link'
          type='button'
        >
          <MeoLinkIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          className='format-button'
          data-action='wikiLink'
          onClick={() => onFormat('wikiLink')}
          tooltip='Wiki Link'
          type='button'
        >
          <MeoBracketsIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          className='format-button'
          data-action='image'
          onClick={() => onFormat('image')}
          tooltip='Image'
          type='button'
        >
          <MeoImageIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          className='format-button'
          data-action='quote'
          onClick={() => onFormat('quote')}
          tooltip='Quote'
          type='button'
        >
          <MeoQuoteIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          className='format-button'
          data-action='hr'
          onClick={() => onFormat('hr')}
          tooltip='Horizontal Rule'
          type='button'
        >
          <MeoMinusIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
      </div>
      <div className='right-group'>
        <ManageTooltipButton
          aria-pressed={findOpen}
          className={`format-button toggle-button${findOpen ? ' is-active' : ''}`}
          data-action='find'
          onClick={() => {
            if (findOpen) {
              onCloseFind();
              return;
            }
            onFindOpenChange(true);
          }}
          tooltip='Find and Replace'
          type='button'
        >
          <MeoSearchIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          aria-pressed={contentMaxWidthEnabled}
          className={`format-button toggle-button manage-toolbar-optional-button${contentMaxWidthEnabled ? ' is-active' : ''}`}
          data-action='contentMaxWidth'
          hidden={hideOptionalControls}
          onClick={onToggleContentMaxWidth}
          tooltip={contentMaxWidthEnabled ? 'Use Full Content Width' : 'Constrain Content Width'}
          type='button'
        >
          <MeoPanelLeftRightDashedIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          aria-pressed={lineNumbersVisible}
          className={`format-button toggle-button manage-toolbar-optional-button${lineNumbersVisible ? ' is-active' : ''}`}
          data-action='lineNumbers'
          hidden={hideOptionalControls}
          onClick={onToggleLineNumbers}
          tooltip={lineNumbersVisible ? 'Hide Line Numbers' : 'Show Line Numbers'}
          type='button'
        >
          <MeoHashIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          aria-pressed={gitGutterVisible}
          className={`format-button toggle-button manage-toolbar-optional-button${gitGutterVisible ? ' is-active' : ''}`}
          data-action='gitChangesGutter'
          hidden={hideOptionalControls}
          onClick={onToggleGitGutter}
          tooltip={gitGutterVisible ? 'Hide Git Changes' : 'Show Git Changes'}
          type='button'
        >
          <MeoGitCompareIcon aria-hidden='true' size={18} />
        </ManageTooltipButton>
      </div>
      <div aria-label='Markdown mode' className='mode-group' role='group'>
        <ManageTooltipButton
          aria-label={`Markdown mode: ${currentMode === 'live' ? 'Live' : 'Source'}. Switch to ${
            currentMode === 'live' ? 'Source' : 'Live'
          }.`}
          aria-pressed={currentMode === 'source'}
          className='mode-button manage-mode-toggle is-active'
          data-mode={currentMode}
          onClick={() => onModeChange(currentMode === 'live' ? 'source' : 'live')}
          tooltip={currentMode === 'live' ? 'Switch to Source' : 'Switch to Live'}
          type='button'
        >
          {currentMode === 'live' ? 'Live' : 'Source'}
        </ManageTooltipButton>
      </div>
      <div aria-label='Find and replace' className={`find-panel${findOpen ? ' is-visible' : ''}`} role='search'>
        <div className='find-row'>
          <div className='find-input-wrap'>
            <input
              aria-label='Find'
              className='find-input'
              onChange={(event) => onFindQueryChange(event.currentTarget.value)}
              onKeyDown={runFindFromKeyboard}
              placeholder='Find'
              ref={findInputRef}
              type='text'
              value={findQuery}
            />
            <span className={`find-status${findStatusIsError ? ' is-error' : ''}`}>{findStatus}</span>
          </div>
          <ManageTooltipButton
            aria-label='Whole Word'
            aria-pressed={findWholeWord}
            className={`format-button toggle-button find-option-button${findWholeWord ? ' is-active' : ''}`}
            onClick={() => onFindWholeWordChange(!findWholeWord)}
            tooltip='Whole Word'
            type='button'
          >
            <MeoWholeWordIcon aria-hidden='true' size={16} />
          </ManageTooltipButton>
          <ManageTooltipButton
            aria-label='Case Sensitive'
            aria-pressed={findCaseSensitive}
            className={`format-button toggle-button find-option-button${findCaseSensitive ? ' is-active' : ''}`}
            onClick={() => onFindCaseSensitiveChange(!findCaseSensitive)}
            tooltip='Case Sensitive'
            type='button'
          >
            <MeoCaseSensitiveIcon aria-hidden='true' size={16} />
          </ManageTooltipButton>
          <ManageTooltipButton
            className='format-button'
            onClick={() => onRunFind(true)}
            tooltip='Previous Match'
            type='button'
          >
            <MeoChevronUpIcon aria-hidden='true' size={16} />
          </ManageTooltipButton>
          <ManageTooltipButton
            className='format-button'
            onClick={() => onRunFind(false)}
            tooltip='Next Match'
            type='button'
          >
            <MeoChevronDownIcon aria-hidden='true' size={16} />
          </ManageTooltipButton>
        </div>
        <div className='find-row'>
          <input
            aria-label='Replace'
            className='find-input'
            onChange={(event) => onFindReplacementChange(event.currentTarget.value)}
            onKeyDown={runReplaceFromKeyboard}
            placeholder='Replace'
            type='text'
            value={findReplacement}
          />
          <ManageTooltipButton
            className='format-button'
            onClick={onReplaceCurrent}
            tooltip='Replace Current Match'
            type='button'
          >
            <MeoReplaceIcon aria-hidden='true' size={16} />
          </ManageTooltipButton>
          <ManageTooltipButton
            className='format-button'
            onClick={onReplaceAll}
            tooltip='Replace All Matches'
            type='button'
          >
            <MeoReplaceAllIcon aria-hidden='true' size={16} />
          </ManageTooltipButton>
          <span aria-hidden='true' className='find-button-spacer' />
          <ManageTooltipButton
            aria-label='Close Find'
            className='format-button find-close-button'
            onClick={onCloseFind}
            tooltip='Close Find'
            type='button'
          >
            <MeoXIcon aria-hidden='true' size={16} />
          </ManageTooltipButton>
        </div>
      </div>
    </div>
  );
}

export function ManageMeoSelectionFormatToolbar({
  anchor,
  onAnnotate,
  onFormat,
  selectionState,
}: {
  anchor: ManageSelectionAnchor;
  onAnnotate: () => void;
  onFormat: (action: string, level?: number | { cols?: number; rows?: number }) => void;
  selectionState: ManageMeoSelectionState;
}) {
  const position = meoSelectionToolbarPosition(selectionState, anchor);
  const formatAction = (action: string) => {
    onFormat(action);
  };
  return createPortal(
    <div
      aria-label='Inline markdown formatting'
      className={`selection-inline-menu is-visible${position.isBelow ? ' is-below' : ''}`}
      onPointerDown={(event) => event.preventDefault()}
      role='toolbar'
      style={{ left: position.left, top: position.top }}
    >
      <ManageTooltipButton
        aria-label='Annotations'
        className='selection-inline-button manage-selection-inline-mode-button'
        onClick={onAnnotate}
        tooltip='Annotations'
        type='button'
      >
        <IconMessagePlus aria-hidden='true' size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton
        aria-label='Bold'
        className='selection-inline-button'
        data-action='bold'
        onClick={() => formatAction('bold')}
        tooltip='Bold'
        type='button'
      >
        <MeoBoldIcon aria-hidden='true' size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton
        aria-label='Italic'
        className='selection-inline-button'
        data-action='italic'
        onClick={() => formatAction('italic')}
        tooltip='Italic'
        type='button'
      >
        <MeoItalicIcon aria-hidden='true' size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton
        aria-label='Lineover'
        className='selection-inline-button'
        data-action='lineover'
        onClick={() => formatAction('lineover')}
        tooltip='Lineover'
        type='button'
      >
        <MeoStrikethroughIcon aria-hidden='true' size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton
        aria-label='Inline Code'
        className='selection-inline-button'
        data-action='inlineCode'
        onClick={() => formatAction('inlineCode')}
        tooltip='Inline Code'
        type='button'
      >
        <MeoTerminalIcon aria-hidden='true' size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton
        aria-label='Link'
        className='selection-inline-button'
        data-action='link'
        onClick={() => formatAction('link')}
        tooltip='Link'
        type='button'
      >
        <MeoLinkIcon aria-hidden='true' size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton
        aria-label='Wiki Link'
        className='selection-inline-button'
        data-action='wikiLink'
        onClick={() => formatAction('wikiLink')}
        tooltip='Wiki Link'
        type='button'
      >
        <MeoBracketsIcon aria-hidden='true' size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton
        aria-label='Kbd'
        className='selection-inline-button'
        data-action='kbd'
        onClick={() => formatAction('kbd')}
        tooltip='Kbd'
        type='button'
      >
        <MeoKeyboardIcon aria-hidden='true' size={16} />
      </ManageTooltipButton>
      <div aria-label='Suggested replacements' className='selection-inline-suggestions' hidden role='group' />
    </div>,
    document.body
  );
}

export function applyManageMeoTheme(): void {
  const rootStyle = document.documentElement.style;
  rootStyle.setProperty(
    '--vscode-editor-font-family',
    'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
  );
  rootStyle.setProperty('--vscode-editor-font-size', '14px');
  rootStyle.setProperty('--vscode-editor-font-weight', '450');
  rootStyle.setProperty('--vscode-editor-background', '#101112');
  rootStyle.setProperty('--vscode-editor-foreground', '#e5e7eb');
  rootStyle.setProperty('--vscode-sideBar-background', '#17191c');
  rootStyle.setProperty('--vscode-panel-border', 'rgba(255, 255, 255, 0.10)');
  rootStyle.setProperty('--vscode-editor-selectionBackground', 'rgba(125, 211, 252, 0.28)');
  rootStyle.setProperty('--vscode-editorWidget-background', '#17191c');
  applyMeoThemeSettings(MANAGE_MEO_THEME);
}

export function createManageMeoAnnotationDecorations(
  text: string,
  annotations: readonly ManageAnnotation[]
): ManageMeoAnnotationDecoration[] {
  return collectManageAnnotationRanges(text, annotations).map((range) => ({
    from: range.from,
    labelId: range.annotation.labelId,
    to: range.to,
    type: range.annotation.type,
  }));
}

export function buildManageMeoAnnotationDecorations(
  decorations: readonly ManageMeoAnnotationDecoration[]
): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const orderedDecorations = decorations
    .filter((decoration) => decoration.from >= 0 && decoration.to > decoration.from)
    .sort((left, right) => left.from - right.from || left.to - right.to);
  for (const decoration of orderedDecorations) {
    builder.add(
      decoration.from,
      decoration.to,
      Decoration.mark({
        attributes: {
          'data-type': decoration.type,
          ...(decoration.labelId ? { 'data-label-id': decoration.labelId } : {}),
          style: `--manage-annotation-color: ${manageAnnotationColor(decoration)};`,
        },
        class: `annotation-highlight manage-annotation-highlight ${decoration.type === 'redline' ? 'deletion' : 'comment'}`,
      })
    );
  }
  return builder.finish();
}

export function collectManageAnnotationRanges(
  text: string,
  annotations: readonly ManageAnnotation[]
): ManageResolvedAnnotationRange[] {
  const ranges: ManageResolvedAnnotationRange[] = [];
  for (const annotation of annotations) {
    if (annotation.scope !== 'selection') {
      continue;
    }
    for (const match of findManageAnnotationTextMatches(text, annotation.quote)) {
      ranges.push({
        annotation,
        from: match.from,
        labelId: annotation.labelId,
        to: match.to,
        type: annotation.type,
      });
    }
  }
  return ranges;
}

export function findManageAnnotationTextMatches(text: string, quote: string): Array<{ from: number; to: number }> {
  const normalizedQuote = normalizeAnnotationQuote(quote);
  if (!normalizedQuote) {
    return [];
  }
  const normalizedText = buildManageNormalizedTextIndex(text);
  const matches: Array<{ from: number; to: number }> = [];
  let fromIndex = 0;
  while (fromIndex < normalizedText.text.length) {
    const matchIndex = normalizedText.text.indexOf(normalizedQuote, fromIndex);
    if (matchIndex < 0) {
      break;
    }
    const start = normalizedText.positions[matchIndex];
    const end = normalizedText.positions[matchIndex + normalizedQuote.length - 1];
    if (typeof start === 'number' && typeof end === 'number' && end >= start) {
      matches.push({ from: start, to: end + 1 });
    }
    fromIndex = matchIndex + normalizedQuote.length;
  }
  return matches;
}

export function buildManageNormalizedTextIndex(text: string): { positions: number[]; text: string } {
  const positions: number[] = [];
  let normalized = '';
  let previousWasWhitespace = true;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index] ?? '';
    if (/\s/u.test(character)) {
      if (!previousWasWhitespace) {
        normalized += ' ';
        positions.push(index);
        previousWasWhitespace = true;
      }
      continue;
    }
    normalized += character;
    positions.push(index);
    previousWasWhitespace = false;
  }
  if (normalized.endsWith(' ')) {
    normalized = normalized.slice(0, -1);
    positions.pop();
  }
  return { positions, text: normalized };
}

export function syncManageMeoAnnotationReviewState(
  view: EditorView,
  annotations: readonly ManageAnnotation[],
  onSelectionCapture: (selection: ManageCapturedSelection) => void,
  onSelectionClear: () => void,
  onAnnotationPreviewChange: (preview: ManageAnnotationPreview | undefined) => void
): void {
  const selection = view.state.selection.main;
  const documentLength = view.state.doc.length;
  if (!selection.empty) {
    const from = Math.max(0, Math.min(Math.floor(Math.min(selection.from, selection.to)), documentLength));
    const to = Math.max(from, Math.min(Math.floor(Math.max(selection.from, selection.to)), documentLength));
    const text = view.state.doc.sliceString(from, to);
    if (!normalizeAnnotationQuote(text)) {
      onSelectionClear();
      onAnnotationPreviewChange(undefined);
      return;
    }
    onAnnotationPreviewChange(undefined);
    onSelectionCapture({
      anchor: manageEditorRangeAnchor(view, from, to) ?? defaultManageSelectionAnchor(),
      text,
    });
    return;
  }

  onSelectionClear();
  const caretPosition = Math.max(0, Math.min(Math.floor(selection.from), documentLength));
  const activeRange = findManageAnnotationRangeAtPosition(view.state.doc.toString(), annotations, caretPosition);
  if (!activeRange) {
    onAnnotationPreviewChange(undefined);
    return;
  }
  onAnnotationPreviewChange({
    anchor: manageEditorRangeAnchor(view, activeRange.from, activeRange.to) ?? defaultManageSelectionAnchor(),
    annotation: activeRange.annotation,
  });
}

export function findManageAnnotationRangeAtPosition(
  text: string,
  annotations: readonly ManageAnnotation[],
  position: number
): ManageResolvedAnnotationRange | undefined {
  return collectManageAnnotationRanges(text, annotations)
    .filter((range) => position >= range.from && position < range.to)
    .sort((left, right) => left.to - left.from - (right.to - right.from) || left.from - right.from)[0];
}

export function manageEditorRangeAnchor(view: EditorView, from: number, to: number): ManageSelectionAnchor | undefined {
  const documentLength = view.state.doc.length;
  const rangeFrom = Math.max(0, Math.min(Math.floor(from), documentLength));
  const rangeTo = Math.max(rangeFrom, Math.min(Math.floor(to), documentLength));
  if (rangeTo <= rangeFrom) {
    return undefined;
  }
  const rects = manageEditorRangeRects(view, rangeFrom, rangeTo);
  if (rects.length > 0) {
    const left = Math.min(...rects.map((rect) => rect.left));
    const right = Math.max(...rects.map((rect) => rect.right));
    const top = Math.min(...rects.map((rect) => rect.top));
    return {
      left: Math.min(Math.max((left + right) / 2, 12), window.innerWidth - 12),
      top: Math.min(Math.max(top, 12), window.innerHeight - 12),
    };
  }
  const coords = view.coordsAtPos(rangeFrom);
  if (!coords) {
    return undefined;
  }
  return {
    left: Math.min(Math.max((coords.left + coords.right) / 2, 12), window.innerWidth - 12),
    top: Math.min(Math.max(coords.top, 12), window.innerHeight - 12),
  };
}

export function manageEditorRangeRects(view: EditorView, from: number, to: number): DOMRect[] {
  try {
    const start = view.domAtPos(from);
    const end = view.domAtPos(to);
    const range = document.createRange();
    range.setStart(start.node, start.offset);
    range.setEnd(end.node, end.offset);
    const rects = Array.from(range.getClientRects()).filter((rect) => rect.width > 0 && rect.height > 0);
    range.detach();
    return rects;
  } catch {
    return [];
  }
}
