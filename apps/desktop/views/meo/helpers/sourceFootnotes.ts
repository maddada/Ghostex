// @ts-nocheck
import { EditorState, RangeSetBuilder, StateField, type Transaction } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView } from '@codemirror/view';
import { resolvedSyntaxTree } from './markdownSyntax';
import { collectInlineFootnoteMarkerRanges } from './inlineFootnotes';

const sourceLinkMarkerDeco = Decoration.mark({
  class: 'meo-md-link-marker',
  attributes: {
    style: 'color: var(--meo-color-base02) !important; -webkit-text-fill-color: var(--meo-color-base02) !important;'
  }
});
const sourceFootnoteMarkerDeco = Decoration.mark({
  class: 'meo-md-footnote-marker',
  attributes: {
    style: 'color: var(--meo-color-base02) !important; -webkit-text-fill-color: var(--meo-color-base02) !important;'
  }
});

function addRange(builder: RangeSetBuilder<Decoration>, from: number, to: number, deco: Decoration): void {
  if (to <= from) {
    return;
  }
  builder.add(from, to, deco);
}

function findChildNode(node: any, name: string): any {
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

function addReferenceLinkMarkerDecorations(builder: RangeSetBuilder<Decoration>, node: any, linkLabelNode: any): void {
  addRange(builder, node.from, node.from + 1, sourceLinkMarkerDeco);
  addRange(builder, linkLabelNode.from - 1, linkLabelNode.from, sourceLinkMarkerDeco);
  addRange(builder, linkLabelNode.from, linkLabelNode.from + 1, sourceLinkMarkerDeco);
  addRange(builder, linkLabelNode.to - 1, linkLabelNode.to, sourceLinkMarkerDeco);
}

function addInlineFootnoteMarkerDecorations(
  builder: RangeSetBuilder<Decoration>,
  nodeFrom: number,
  markerRanges: Array<{ label: string; fromOffset: number; toOffset: number }>
): void {
  for (const markerRange of markerRanges) {
    const markerFrom = nodeFrom + markerRange.fromOffset;
    const markerTo = nodeFrom + markerRange.toOffset;
    if (markerTo - markerFrom < 3) {
      continue;
    }
    addRange(builder, markerFrom, markerFrom + 1, sourceFootnoteMarkerDeco);
    addRange(builder, markerFrom + 1, markerFrom + 2, sourceFootnoteMarkerDeco);
    addRange(builder, markerTo - 1, markerTo, sourceFootnoteMarkerDeco);
  }
}

function computeSourceFootnoteMarkerDecorations(state: EditorState): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const tree = resolvedSyntaxTree(state);

  tree.iterate({
    enter(node: any) {
      if (node.name !== 'Link') {
        return;
      }
      if (findChildNode(node, 'URL')) {
        return;
      }

      const linkLabelNode = findChildNode(node, 'LinkLabel');
      if (
        linkLabelNode &&
        linkLabelNode.from > node.from &&
        linkLabelNode.to > linkLabelNode.from + 1 &&
        linkLabelNode.to <= node.to
      ) {
        addReferenceLinkMarkerDecorations(builder, node, linkLabelNode);
      }

      const rawText = state.doc.sliceString(node.from, node.to);
      const markerRanges = collectInlineFootnoteMarkerRanges(rawText);
      if (!markerRanges.length) {
        return;
      }
      addInlineFootnoteMarkerDecorations(builder, node.from, markerRanges);
    }
  });

  return builder.finish();
}

export const sourceFootnoteMarkerField = StateField.define<DecorationSet>({
  create(state: EditorState) {
    try {
      return computeSourceFootnoteMarkerDecorations(state);
    } catch (error) {
      console.error('[MEO webview] Failed to build source footnote-marker decorations on create.', error);
      return Decoration.none;
    }
  },
  update(decorations: DecorationSet, transaction: Transaction) {
    if (!transaction.docChanged) {
      return decorations;
    }
    try {
      return computeSourceFootnoteMarkerDecorations(transaction.state);
    } catch (error) {
      console.error('[MEO webview] Failed to build source footnote-marker decorations on update.', error);
      return decorations;
    }
  },
  provide: (field: any) => EditorView.decorations.from(field)
});
