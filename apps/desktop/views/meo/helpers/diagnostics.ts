// @ts-nocheck
import { RangeSetBuilder, StateEffect, StateField, type Transaction } from '@codemirror/state';
import { Decoration, EditorView, type DecorationSet } from '@codemirror/view';

export type EditorDiagnostic = {
  from: number;
  to: number;
  severity: 0 | 1 | 2 | 3;
  message: string;
  source?: string;
  code?: string;
};

export const setDiagnosticsEffect = StateEffect.define<EditorDiagnostic[]>();

const severityClasses = [
  'meo-diagnostic-error',
  'meo-diagnostic-warning',
  'meo-diagnostic-info',
  'meo-diagnostic-hint',
];

function diagnosticTitle(diagnostic: EditorDiagnostic): string {
  const parts: string[] = [];
  if (diagnostic.source) {
    parts.push(diagnostic.source);
  }
  if (diagnostic.code) {
    parts.push(diagnostic.code);
  }
  const prefix = parts.length ? `${parts.join(' ')}: ` : '';
  return `${prefix}${diagnostic.message}`;
}

function buildDiagnosticDecorations(docLength: number, diagnostics: EditorDiagnostic[]): DecorationSet {
  if (!Array.isArray(diagnostics) || diagnostics.length === 0) {
    return Decoration.none;
  }

  const builder = new RangeSetBuilder<Decoration>();
  const sortedDiagnostics = [...diagnostics].sort((left, right) =>
    left.from === right.from ? left.to - right.to : left.from - right.from
  );
  for (const diagnostic of sortedDiagnostics) {
    const from = Math.max(0, Math.min(Math.floor(diagnostic.from), docLength));
    const to = Math.max(from, Math.min(Math.floor(diagnostic.to), docLength));
    if (to <= from) {
      continue;
    }

    const severityClass = severityClasses[diagnostic.severity] ?? severityClasses[0];
    builder.add(
      from,
      to,
      Decoration.mark({
        class: `meo-diagnostic ${severityClass}`,
        attributes: {
          title: diagnosticTitle(diagnostic),
        },
      })
    );
  }

  return builder.finish();
}

function normalizeDiagnostics(docLength: number, diagnostics: EditorDiagnostic[]): EditorDiagnostic[] {
  if (!Array.isArray(diagnostics) || diagnostics.length === 0) {
    return [];
  }

  const result: EditorDiagnostic[] = [];
  for (const diagnostic of diagnostics) {
    const from = Math.max(0, Math.min(Math.floor(diagnostic.from), docLength));
    const to = Math.max(from, Math.min(Math.floor(diagnostic.to), docLength));
    if (to <= from) {
      continue;
    }
    result.push({ ...diagnostic, from, to });
  }
  return result;
}

export const diagnosticDataField = StateField.define<EditorDiagnostic[]>({
  create() {
    return [];
  },
  update(diagnostics: EditorDiagnostic[], tr: Transaction): EditorDiagnostic[] {
    for (const effect of tr.effects) {
      if (effect.is(setDiagnosticsEffect)) {
        return normalizeDiagnostics(tr.state.doc.length, effect.value);
      }
    }
    if (!tr.docChanged || diagnostics.length === 0) {
      return diagnostics;
    }
    return normalizeDiagnostics(
      tr.state.doc.length,
      diagnostics.map((diagnostic) => ({
        ...diagnostic,
        from: tr.changes.mapPos(diagnostic.from),
        to: tr.changes.mapPos(diagnostic.to),
      }))
    );
  },
});

export const diagnosticField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(decorations: DecorationSet, tr: Transaction): DecorationSet {
    for (const effect of tr.effects) {
      if (effect.is(setDiagnosticsEffect)) {
        return buildDiagnosticDecorations(tr.state.doc.length, effect.value);
      }
    }
    if (tr.docChanged) {
      return decorations.map(tr.changes);
    }
    return decorations;
  },
  provide(field) {
    return EditorView.decorations.from(field);
  },
});
