// @ts-nocheck
import { EditorState, RangeSetBuilder, StateField, type Transaction } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView } from '@codemirror/view';
import { resolvedSyntaxTree } from './markdownSyntax';
import { normalizeSourceHref } from './rawUrls';
import { trimDecoratedUrlRange } from './urlDecorationRange';

const sourceUrlBoundaryDeco = Decoration.mark({ class: 'meo-md-url-boundary' });

function addRange(builder: RangeSetBuilder<Decoration>, from: number, to: number): void {
  if (to <= from) {
    return;
  }
  builder.add(from, to, sourceUrlBoundaryDeco);
}

function computeSourceUrlBoundaryDecorations(state: EditorState): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const tree = resolvedSyntaxTree(state);

  tree.iterate({
    enter(node: any) {
      if (node.name !== 'URL') {
        return;
      }

      const rawUrl = state.doc.sliceString(node.from, node.to);
      const href = normalizeSourceHref(rawUrl);
      if (!href) {
        return;
      }

      // Markdown URL tokens can include decorative surrounding/trailing quotes.
      // Keep those characters out of URL coloring in source mode.
      const range = trimDecoratedUrlRange(node.from, node.to, rawUrl, href);
      if (node.from < range.from) {
        addRange(builder, node.from, range.from);
      }
      if (range.to < node.to) {
        addRange(builder, range.to, node.to);
      }
    },
  });

  return builder.finish();
}

export const sourceUrlBoundaryField = StateField.define<DecorationSet>({
  create(state: EditorState) {
    try {
      return computeSourceUrlBoundaryDecorations(state);
    } catch (error) {
      console.error('[MEO webview] Failed to build source URL boundary decorations on create.', error);
      return Decoration.none;
    }
  },
  update(decorations: DecorationSet, transaction: Transaction) {
    if (!transaction.docChanged) {
      return decorations;
    }
    try {
      return computeSourceUrlBoundaryDecorations(transaction.state);
    } catch (error) {
      console.error('[MEO webview] Failed to build source URL boundary decorations on update.', error);
      return decorations;
    }
  },
  provide: (field: any) => EditorView.decorations.from(field),
});
