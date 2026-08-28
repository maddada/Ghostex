import { sessionChatComposerReferences, type SessionChatReferenceKind } from './session-chat-reference-pills';

const FIRST_REFERENCE_TOKEN = 0xe000;
const LAST_REFERENCE_TOKEN = 0xf8ff;

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
 * one private-use character for each atomic reference pill.
 */
export class SessionChatMonacoReferenceModel {
  private nextTokenCodePoint = FIRST_REFERENCE_TOKEN;
  private readonly referencesByToken = new Map<string, SessionChatMonacoReference>();

  canonicalOffsetToModel(presentation: string, canonicalOffset: number): number {
    const target = Math.max(0, canonicalOffset);
    let canonicalCursor = 0;
    for (let modelOffset = 0; modelOffset < presentation.length; modelOffset += 1) {
      if (target <= canonicalCursor) {
        return modelOffset;
      }
      const reference = this.referencesByToken.get(presentation[modelOffset] ?? '');
      canonicalCursor += reference?.source.length ?? 1;
      if (target <= canonicalCursor) {
        // A canonical caret inside a reference cannot exist in the one-token
        // presentation. Put it after the pill, the useful edge for insertions.
        return modelOffset + 1;
      }
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
    for (let index = 0; index < end; index += 1) {
      canonicalOffset += this.referencesByToken.get(presentation[index] ?? '')?.source.length ?? 1;
    }
    return canonicalOffset;
  }

  occurrences(presentation: string): SessionChatMonacoReferenceOccurrence[] {
    const occurrences: SessionChatMonacoReferenceOccurrence[] = [];
    for (let index = 0; index < presentation.length; index += 1) {
      const reference = this.referencesByToken.get(presentation[index] ?? '');
      if (reference) {
        occurrences.push({ ...reference, end: index + 1, start: index });
      }
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
    while (this.nextTokenCodePoint <= LAST_REFERENCE_TOKEN) {
      const token = String.fromCharCode(this.nextTokenCodePoint);
      this.nextTokenCodePoint += 1;
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
