import { sessionChatComposerReferences, type SessionChatReferenceKind } from './session-chat-reference-pills';

const BMP_VARIATION_SELECTOR_START = 0xfe00;
const BMP_VARIATION_SELECTOR_COUNT = 16;
const SUPPLEMENTARY_VARIATION_SELECTOR_START = 0xe0100;
const SUPPLEMENTARY_VARIATION_SELECTOR_COUNT = 240;
const REFERENCE_TOKEN_CAPACITY = BMP_VARIATION_SELECTOR_COUNT + SUPPLEMENTARY_VARIATION_SELECTOR_COUNT;

function referenceTokenForIndex(index: number): string {
  if (index < BMP_VARIATION_SELECTOR_COUNT) {
    return String.fromCharCode(BMP_VARIATION_SELECTOR_START + index);
  }
  return String.fromCodePoint(SUPPLEMENTARY_VARIATION_SELECTOR_START + index - BMP_VARIATION_SELECTOR_COUNT);
}

export interface SessionChatMonacoReference {
  kind: SessionChatReferenceKind;
  label: string;
  path: string;
  source: string;
  token: string;
}

export interface SessionChatMonacoReferenceOccurrence extends SessionChatMonacoReference {
  end: number;
  start: number;
}

/**
 * Keeps canonical Markdown outside Monaco while its presentation model uses
 * one intrinsically invisible variation selector for each atomic reference
 * pill. The token stays invisible even between Monaco decoration updates.
 */
export class SessionChatMonacoReferenceModel {
  private nextTokenIndex = 0;
  private readonly referencesByToken = new Map<string, SessionChatMonacoReference>();

  reset(): void {
    this.nextTokenIndex = 0;
    this.referencesByToken.clear();
  }

  canonicalOffsetToModel(presentation: string, canonicalOffset: number): number {
    const target = Math.max(0, canonicalOffset);
    let canonicalCursor = 0;
    for (let modelOffset = 0; modelOffset < presentation.length;) {
      if (target <= canonicalCursor) {
        return modelOffset;
      }
      const token = String.fromCodePoint(presentation.codePointAt(modelOffset) ?? 0);
      const reference = this.referencesByToken.get(token);
      canonicalCursor += reference?.source.length ?? 1;
      if (target <= canonicalCursor) {
        // A canonical caret inside a reference cannot exist in the one-token
        // presentation. Put it after the pill, the useful edge for insertions.
        return modelOffset + (reference ? token.length : 1);
      }
      modelOffset += reference ? token.length : 1;
    }
    return presentation.length;
  }

  expand(presentation: string): string {
    let canonical = '';
    for (const character of presentation) {
      canonical += this.referencesByToken.get(character)?.source ?? character;
    }
    return canonical;
  }

  modelOffsetToCanonical(presentation: string, modelOffset: number): number {
    const end = Math.min(Math.max(0, modelOffset), presentation.length);
    let canonicalOffset = 0;
    for (let index = 0; index < end;) {
      const token = String.fromCodePoint(presentation.codePointAt(index) ?? 0);
      const reference = this.referencesByToken.get(token);
      canonicalOffset += reference?.source.length ?? 1;
      index += reference ? token.length : 1;
    }
    return canonicalOffset;
  }

  occurrences(presentation: string): SessionChatMonacoReferenceOccurrence[] {
    const occurrences: SessionChatMonacoReferenceOccurrence[] = [];
    for (let index = 0; index < presentation.length;) {
      const token = String.fromCodePoint(presentation.codePointAt(index) ?? 0);
      const reference = this.referencesByToken.get(token);
      if (reference) {
        occurrences.push({ ...reference, end: index + token.length, start: index });
      }
      index += reference ? token.length : 1;
    }
    return occurrences;
  }

  virtualizeCanonical(canonical: string, currentPresentation = ''): string {
    const reusableBySource = new Map<string, string[]>();
    for (const reference of this.occurrences(currentPresentation)) {
      const reusable = reusableBySource.get(reference.source) ?? [];
      reusable.push(reference.token);
      reusableBySource.set(reference.source, reusable);
    }
    return this.virtualize(canonical, (source) => reusableBySource.get(source)?.shift());
  }

  virtualizeInsertion(canonical: string): string {
    return this.virtualize(canonical);
  }

  private allocateToken(canonical: string): string {
    while (this.nextTokenIndex < REFERENCE_TOKEN_CAPACITY) {
      const token = referenceTokenForIndex(this.nextTokenIndex);
      this.nextTokenIndex += 1;
      if (!this.referencesByToken.has(token) && !canonical.includes(token)) {
        return token;
      }
    }
    throw new Error('The Monaco reference token range is exhausted.');
  }

  private virtualize(canonical: string, reusableToken?: (source: string) => string | undefined): string {
    const references = sessionChatComposerReferences(canonical);
    if (references.length === 0) {
      return canonical;
    }
    let presentation = '';
    let cursor = 0;
    for (const reference of references) {
      const source = canonical.slice(reference.start, reference.end);
      const token = reusableToken?.(source) ?? this.allocateToken(canonical);
      this.referencesByToken.set(token, {
        kind: reference.kind,
        label: reference.label,
        path: reference.path,
        source,
        token,
      });
      presentation += canonical.slice(cursor, reference.start);
      presentation += token;
      cursor = reference.end;
    }
    return presentation + canonical.slice(cursor);
  }
}
