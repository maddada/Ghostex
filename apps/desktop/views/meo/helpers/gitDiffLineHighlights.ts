// @ts-nocheck
import { Decoration, DecorationSet, EditorView } from '@codemirror/view';
import { StateField, StateEffect, Transaction, RangeSetBuilder, EditorState } from '@codemirror/state';
import { gitDiffLineFlagsField, setGitBaselineEffect } from './gitDiffGutter';

const setGitDiffHighlightsEnabled = StateEffect.define<boolean>();

let gitDiffHighlightsEnabled = false;

function hasRenderableGitDiffLines(lineFlags: unknown): boolean {
  if (!Array.isArray(lineFlags)) {
    return false;
  }
  for (let i = 0; i < lineFlags.length; i += 1) {
    const flags = lineFlags[i] as { added?: boolean; modified?: boolean } | undefined;
    if (flags?.added || flags?.modified) {
      return true;
    }
  }
  return false;
}

function buildGitDiffLineHighlights(state: EditorState): DecorationSet {
  if (!gitDiffHighlightsEnabled) {
    return Decoration.none;
  }

  const lineFlags = state.field(gitDiffLineFlagsField, false);
  if (!hasRenderableGitDiffLines(lineFlags)) {
    return Decoration.none;
  }

  const builder = new RangeSetBuilder<Decoration>();
  const doc = state.doc;

  for (let i = 1; i <= doc.lines; i++) {
    const flags = lineFlags[i - 1];
    if (!flags) {
      continue;
    }

    const line = doc.line(i);
    let decoration: Decoration | null = null;

    if (flags.added) {
      decoration = Decoration.line({ class: 'meo-diff-added-line' });
    } else if (flags.modified) {
      decoration = Decoration.line({ class: 'meo-diff-changed-line' });
    }

    if (decoration) {
      builder.add(line.from, line.from, decoration);
    }
  }

  return builder.finish();
}

const gitDiffLineHighlightsField = StateField.define<DecorationSet>({
  create(state: EditorState): DecorationSet {
    return buildGitDiffLineHighlights(state);
  },
  update(highlights: DecorationSet, tr: Transaction): DecorationSet {
    let baselineChanged = false;
    let highlightsEnabledChanged = false;
    for (const effect of tr.effects) {
      if (effect.is(setGitBaselineEffect)) {
        baselineChanged = true;
      }
      if (effect.is(setGitDiffHighlightsEnabled)) {
        gitDiffHighlightsEnabled = effect.value;
        highlightsEnabledChanged = true;
      }
    }
    if (!tr.docChanged && !baselineChanged && !highlightsEnabledChanged) {
      return highlights.map(tr.changes);
    }

    return buildGitDiffLineHighlights(tr.state);
  },
  provide: (f) => EditorView.decorations.from(f),
});

function resolveEditorView(target: unknown): EditorView | null {
  if (!target || typeof target !== 'object') {
    return null;
  }
  const maybeView = target as { dispatch?: unknown; state?: unknown };
  if (typeof maybeView.dispatch === 'function' && maybeView.state) {
    return maybeView as EditorView;
  }
  const wrapped = target as { view?: { dispatch?: unknown; state?: unknown } };
  if (wrapped.view && typeof wrapped.view.dispatch === 'function' && wrapped.view.state) {
    return wrapped.view as EditorView;
  }
  return null;
}

export function setGitDiffLineHighlightsEnabled(target: unknown, enabled: boolean): void {
  // Keep desired highlight visibility in module state even when source-only extensions
  // are not active (e.g. toggled while in Live mode).
  gitDiffHighlightsEnabled = enabled;
  const view = resolveEditorView(target);
  if (!view) {
    return;
  }
  view.dispatch({
    effects: setGitDiffHighlightsEnabled.of(enabled),
  });
}

export { gitDiffLineHighlightsField };
