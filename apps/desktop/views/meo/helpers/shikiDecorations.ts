// @ts-nocheck
import { RangeSetBuilder, StateEffect, Prec } from '@codemirror/state';
import { Decoration, ViewPlugin, EditorView, type DecorationSet, type ViewUpdate } from '@codemirror/view';
import { syntaxTree } from '@codemirror/language';
import { getFencedCodeInfo } from './codeBlocks';
import {
  resolveShikiLang,
  getShikiTokens,
  requestShikiTokens,
  isShikiThemeReady,
  isShikiEnabled,
  subscribeShikiRefresh,
  getShikiThemeMeta,
  type ShikiToken
} from './shikiHighlighter';

const shikiRefreshEffect = StateEffect.define<null>();

const FONT_STYLE_ITALIC = 1;
const FONT_STYLE_BOLD = 2;
const FONT_STYLE_UNDERLINE = 4;

function tokenStyle(token: ShikiToken): string {
  let style = token.color ? `color:${token.color}` : '';
  const fontStyle = token.fontStyle ?? 0;
  if (fontStyle & FONT_STYLE_ITALIC) {
    style += ';font-style:italic';
  }
  if (fontStyle & FONT_STYLE_BOLD) {
    style += ';font-weight:bold';
  }
  if (fontStyle & FONT_STYLE_UNDERLINE) {
    style += ';text-decoration:underline';
  }
  return style;
}

function addBlockDecorations(
  view: EditorView,
  node: { name: string; from: number; to: number },
  builder: RangeSetBuilder<Decoration>,
  markCache: Map<string, Decoration>
): void {
  const { state } = view;
  const info = node.name === 'FencedCode' ? getFencedCodeInfo(state, node) : null;
  const lang = resolveShikiLang(info);
  if (!lang) {
    return;
  }

  const startLine = state.doc.lineAt(node.from);
  const endLine = state.doc.lineAt(Math.max(node.to - 1, node.from));
  if (endLine.number - startLine.number < 2) {
    return;
  }

  const contentFrom = state.doc.line(startLine.number + 1).from;
  const contentTo = state.doc.line(endLine.number - 1).to;
  if (contentFrom >= contentTo) {
    return;
  }

  const code = state.doc.sliceString(contentFrom, contentTo);
  const tokens = getShikiTokens(lang, code);
  if (!tokens) {
    requestShikiTokens(lang, code);
    return;
  }

  const meta = getShikiThemeMeta();
  const bracketColors = meta.bracketColors;
  const numBracketColors = bracketColors.length;

  const addMark = (from: number, to: number, style: string): void => {
    if (from >= to || !style) {
      return;
    }
    let deco = markCache.get(style);
    if (!deco) {
      deco = Decoration.mark({ attributes: { style } });
      markCache.set(style, deco);
    }
    builder.add(from, to, deco);
  };

  let depth = 0;

  for (const line of tokens) {
    for (const token of line) {
      const content = token.content;
      if (!content) {
        continue;
      }
      const tokenFrom = contentFrom + token.offset;
      if (tokenFrom < contentFrom || tokenFrom + content.length > contentTo) {
        continue;
      }
      const baseStyle = tokenStyle(token);
      const inStringOrComment = token.isStringComment === true;

      if (numBracketColors === 0 || inStringOrComment) {
        if (content.trim()) {
          addMark(tokenFrom, tokenFrom + content.length, baseStyle);
        }
        continue;
      }

      let spanStart = 0;
      for (let i = 0; i < content.length; i += 1) {
        const ch = content[i];
        const isOpen = ch === '(' || ch === '[' || ch === '{';
        const isClose = ch === ')' || ch === ']' || ch === '}';
        if (!isOpen && !isClose) {
          continue;
        }
        if (i > spanStart && content.slice(spanStart, i).trim()) {
          addMark(tokenFrom + spanStart, tokenFrom + i, baseStyle);
        }
        let bracketColor: string;
        if (isOpen) {
          bracketColor = bracketColors[depth % numBracketColors];
          depth += 1;
        } else if (depth === 0) {
          bracketColor = meta.unexpectedBracket;
        } else {
          depth -= 1;
          bracketColor = bracketColors[depth % numBracketColors];
        }
        addMark(tokenFrom + i, tokenFrom + i + 1, `color:${bracketColor}`);
        spanStart = i + 1;
      }
      if (content.length > spanStart && content.slice(spanStart).trim()) {
        addMark(tokenFrom + spanStart, tokenFrom + content.length, baseStyle);
      }
    }
  }
}

function buildDecorations(view: EditorView): DecorationSet {
  if (!isShikiEnabled() || !isShikiThemeReady()) {
    return Decoration.none;
  }
  const builder = new RangeSetBuilder<Decoration>();
  const markCache = new Map<string, Decoration>();
  try {
    syntaxTree(view.state).iterate({
      enter(node) {
        if (node.name === 'FencedCode' || node.name === 'CodeBlock') {
          addBlockDecorations(view, node, builder, markCache);
          return false;
        }
        return undefined;
      }
    });
  } catch {
    return Decoration.none;
  }
  return builder.finish();
}

const shikiPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    private readonly unsubscribe: () => void;

    constructor(view: EditorView) {
      this.decorations = buildDecorations(view);
      this.unsubscribe = subscribeShikiRefresh(() => {
        view.dispatch({ effects: shikiRefreshEffect.of(null) });
      });
    }

    update(update: ViewUpdate): void {
      const refreshed = update.transactions.some((transaction) =>
        transaction.effects.some((effect) => effect.is(shikiRefreshEffect))
      );
      if (update.docChanged || refreshed) {
        this.decorations = buildDecorations(update.view);
      }
    }

    destroy(): void {
      this.unsubscribe();
    }
  },
  {
    decorations: (plugin) => plugin.decorations
  }
);

export const shikiCodeHighlight = Prec.high(shikiPlugin);
