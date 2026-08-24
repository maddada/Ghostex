// @ts-nocheck
import { EditorState, RangeSetBuilder, StateField } from '@codemirror/state';
import { Decoration, EditorView } from '@codemirror/view';
import { resolvedSyntaxTree } from './markdownSyntax';
import { findRawSourceUrlMatches } from './rawUrls';

const sourceFileLinkDeco = Decoration.mark({ class: 'meo-md-source-file-link' });
const fileSchemePrefix = 'file:';
const blockedAncestorNames = new Set([
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

function addRange(builder: RangeSetBuilder<any>, from: number, to: number): void {
  if (to <= from) {
    return;
  }
  builder.add(from, to, sourceFileLinkDeco);
}

function hasBlockedAncestor(tree: any, from: number, to: number): boolean {
  const positions = [from, Math.max(from, to - 1)];
  for (const position of positions) {
    let node = tree.resolveInner(position, 1);
    while (node) {
      if (blockedAncestorNames.has(node.name)) {
        return true;
      }
      node = node.parent;
    }
  }
  return false;
}

function computeSourceFileLinkDecorations(state: EditorState): any {
  const builder = new RangeSetBuilder<any>();
  const tree = resolvedSyntaxTree(state);

  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    const line = state.doc.line(lineNo);
    const text = line.text;
    if (text.indexOf(fileSchemePrefix) === -1) {
      continue;
    }

    const matches = findRawSourceUrlMatches(text);
    for (const match of matches) {
      if (!match.href.toLowerCase().startsWith(fileSchemePrefix)) {
        continue;
      }
      const from = line.from + match.index;
      const to = from + match.length;
      if (to <= from) {
        continue;
      }
      if (hasBlockedAncestor(tree, from, to)) {
        continue;
      }
      addRange(builder, from, to);
    }
  }

  return builder.finish();
}

export const sourceFileLinkField = StateField.define<any>({
  create(state: EditorState) {
    try {
      return computeSourceFileLinkDecorations(state);
    } catch (error) {
      console.error('[MEO webview] Failed to build source file-link decorations on create.', error);
      return Decoration.none;
    }
  },
  update(markers: any, transaction: any) {
    try {
      return computeSourceFileLinkDecorations(transaction.state);
    } catch (error) {
      console.error('[MEO webview] Failed to build source file-link decorations on update.', error);
      return markers;
    }
  },
  provide: (field: any) => EditorView.decorations.from(field),
});
