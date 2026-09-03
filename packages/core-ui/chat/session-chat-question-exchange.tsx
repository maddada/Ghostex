/*
CDXC:AgentScreenDetection 2026-08-22:
Answered agent questions (AskUserQuestion / request_user_input /
cursor_ask_question / Hermes' clarify / omp's ask) rendered as a first-class
card in the chat log instead of a generic tool row with raw JSON.
The tool call's input carries the questions and options; the tool result's text
carries the user's answers in the harness envelope
  The user answered: "q"="a", "q2"="a2". Read the answers carefully …
  Your questions have been answered: "q"="a". You can now continue …
The question texts are known exactly from the input, so answers are recovered
by locating each `"question"="` marker in order — the envelope does not escape
quotes, so quote-aware parsing is impossible and marker slicing is the only
reliable read. Anything that fails to parse falls back to the raw result text
inside the card, and a pair that is not recognizably an answered question at
all keeps the generic tool row (the caller decides that via
`answeredSessionChatQuestionExchange` returning null).
*/

import { IconCheck, IconChevronRight } from '@tabler/icons-react';
import { useState } from 'react';
import type { SessionChatQuestion } from '../../shared/session-chat';
import { cn } from '@/packages/components/utils';
import { SessionChatChoiceRows } from './session-chat-choice-rows';
import type { SessionChatToolPair } from './session-chat-tool-fold';

export interface SessionChatQuestionExchangeAnswer {
  /** Options whose labels the answer text matched, in answer order. */
  selectedIndices: number[];
  /** Free text beyond the matched labels ("Other" answers, added notes). */
  otherText: string | null;
  /** The user closed the question dialog without answering. */
  dismissed: boolean;
}

export interface SessionChatQuestionExchange {
  questions: SessionChatQuestion[];
  /**
   * One entry per question; null when that question went unanswered (skipped).
   * The whole field is null when the result text did not parse per-question —
   * `fallbackText` then carries the answer as one blob.
   */
  answers: (SessionChatQuestionExchangeAnswer | null)[] | null;
  fallbackText: string | null;
}

/** Upstream `isAskUserQuestionTool` mirror (session_chat.rs). */
export function isSessionChatQuestionToolName(name: string): boolean {
  const normalized = name.replace(/[^a-zA-Z0-9]/g, '').toLowerCase();
  return (
    normalized === 'askuserquestion' ||
    normalized === 'askquestion' ||
    normalized === 'requestuserinput' ||
    normalized === 'cursoraskquestion' ||
    // Hermes Agent / oh-my-pi.
    normalized === 'clarify' ||
    normalized === 'ask'
  );
}

/**
 * Hermes' clarify tool hard-caps `choices` at 4 before the terminal panel
 * renders, so the answered card mirrors what was actually offered.
 */
const HERMES_CLARIFY_MAX_CHOICES = 4;

/**
 * One parsed question plus the model-supplied id omp's `ask` echoes in its
 * multi-question result lines (`id: value`). Only the exchange parser needs
 * the id, so it rides beside the shared-contract question, not on it.
 */
interface ParsedQuestionWithId {
  id: string | null;
  question: SessionChatQuestion;
}

function parseQuestionsWithIds(input: unknown, toolName?: string): ParsedQuestionWithId[] | null {
  if (typeof input !== 'object' || input === null) {
    return null;
  }
  const isHermesClarify =
    typeof toolName === 'string' && toolName.replace(/[^a-zA-Z0-9]/g, '').toLowerCase() === 'clarify';
  const isCursorAskQuestion =
    typeof toolName === 'string' && toolName.replace(/[^a-zA-Z0-9]/g, '').toLowerCase() === 'askquestion';
  const rawQuestions = (input as Record<string, unknown>).questions;
  const candidates = Array.isArray(rawQuestions) && rawQuestions.length > 0 ? rawQuestions : [input];
  const questions: ParsedQuestionWithId[] = [];
  for (const raw of candidates) {
    // Hermes tolerates bare-string batch entries (["Q1?", "Q2?"]).
    const record =
      typeof raw === 'object' && raw !== null
        ? (raw as Record<string, unknown>)
        : typeof raw === 'string' && raw.trim().length > 0
          ? { question: raw.trim() }
          : null;
    if (record === null) {
      continue;
    }
    const text =
      typeof record.question === 'string' ? record.question : typeof record.prompt === 'string' ? record.prompt : '';
    let options = parseQuestionOptions(record.options ?? record.choices);
    if (isHermesClarify) {
      options = options.slice(0, HERMES_CLARIFY_MAX_CHOICES);
    }
    if (text.length > 0 || options.length > 0) {
      // `multi_select` is Hermes' spelling, `multi` is omp's; Hermes honors
      // it only when choices exist.
      const multiSelect =
        (isCursorAskQuestion || record.multiSelect === true || record.multi_select === true || record.multi === true) &&
        !(isHermesClarify && options.length === 0);
      questions.push({
        id: typeof record.id === 'string' && record.id.length > 0 ? record.id : null,
        question: {
          question: text,
          ...(typeof record.header === 'string' ? { header: record.header } : {}),
          multiSelect,
          ...(typeof record.allowCustom === 'boolean' ? { allowCustom: record.allowCustom } : {}),
          ...(typeof toolName === 'string' && toolName.length > 0 ? { toolName } : {}),
          ...(typeof record.recommended === 'number' && Number.isInteger(record.recommended) && record.recommended >= 0
            ? { recommended: record.recommended }
            : {}),
          options,
        },
      });
    }
  }
  return questions.length > 0 ? questions : null;
}

/**
 * TS mirror of `parse_session_chat_questions`: the canonical `{questions:
 * [...]}` input shape (also Hermes' clarify and omp's ask), plus the flat
 * single-question shape Pi's cursor_ask_question sends (`{question, options,
 * allowCustom}`, with `prompt`/`choices` accepted as aliases the way the
 * bridge normalizes them).
 */
export function parseSessionChatQuestionsInput(input: unknown, toolName?: string): SessionChatQuestion[] | null {
  const parsed = parseQuestionsWithIds(input, toolName);
  return parsed ? parsed.map((entry) => entry.question) : null;
}

function parseQuestionOptions(raw: unknown): SessionChatQuestion['options'] {
  if (!Array.isArray(raw)) {
    return [];
  }
  const options: SessionChatQuestion['options'] = [];
  for (const option of raw) {
    if (typeof option === 'string') {
      options.push({ label: option });
      continue;
    }
    if (typeof option !== 'object' || option === null) {
      continue;
    }
    const record = option as Record<string, unknown>;
    // Pi options carry `{label, value}`; label is what its select renders,
    // with `value` as the fallback the bridge also uses.
    const label =
      typeof record.label === 'string' ? record.label : typeof record.value === 'string' ? record.value : null;
    if (label === null) {
      continue;
    }
    options.push({
      label,
      ...(typeof record.description === 'string' ? { description: record.description } : {}),
    });
  }
  return options;
}

const RESULT_PREFIXES = ['The user answered: ', 'Your questions have been answered: '];
/*
Pi's cursor_ask_question result envelope: a single question answers as
`User answered: <label or custom text>` (one line) and a cancel as the exact
sentence below. The multi-question form (`User answered:` + `- id: value`
lines) keeps question ids we do not track, so it falls back to the raw blob.
*/
const PI_RESULT_PREFIX = 'User answered: ';
const PI_CANCELLED_TEXT = 'User cancelled the question.';
/*
omp's `ask` result envelope. A single question answers as newline-joined parts
(`User selected: <labels>`, `User provided custom input: <text>` — multi-line
bodies continue two-space indented — and `User added note: <text>`), or one of
the two closing sentences. The multi-question form is `User answers:` plus one
`<id>: <value>` line per question, where value is a bare label, `[a, b]` for
multi-select, `"text"` for custom input, or `(cancelled)`, each optionally
suffixed by ` (auto-selected after timeout)` and ` (note: …)`.
*/
const OMP_SELECTED_PREFIX = 'User selected: ';
const OMP_CUSTOM_PREFIX = 'User provided custom input:';
const OMP_NOTE_PREFIX = 'User added note:';
const OMP_CANCELLED_TEXT = 'User cancelled the selection';
const OMP_NO_SELECTION_TEXT = 'User did not select any options';
const OMP_MULTI_HEADER = 'User answers:';
const OMP_TIMEOUT_SUFFIX = ' (auto-selected after timeout)';
/** Substring-matched so the exact closing-sentence wording cannot break it. */
const RESULT_SUFFIX_MARKERS = ['". Read the answers carefully', '". You can now continue'];
const DISMISSED_PREFIX = '[User dismissed';

/** `"…"` body between the known prefix and closing sentence, or null. */
function stripAnswerEnvelope(output: string): string | null {
  const prefix = RESULT_PREFIXES.find((candidate) => output.startsWith(candidate));
  if (!prefix) {
    return null;
  }
  let body = output.slice(prefix.length);
  for (const marker of RESULT_SUFFIX_MARKERS) {
    const at = body.lastIndexOf(marker);
    if (at >= 0) {
      // Keep the closing quote of the last answer.
      body = body.slice(0, at + 1);
      break;
    }
  }
  return body;
}

/**
 * Match an answer's text back onto the question's options: consume full option
 * labels (longest first) separated by ", " from the front; whatever remains is
 * the user's own words. Answers echo option labels verbatim, so exact prefix
 * matching is safe; multi-select answers are labels joined by ", " with any
 * free-text answer appended the same way.
 */
function matchAnswerToOptions(question: SessionChatQuestion, text: string): SessionChatQuestionExchangeAnswer {
  if (text.startsWith(DISMISSED_PREFIX)) {
    return { selectedIndices: [], otherText: null, dismissed: true };
  }
  const selectedIndices: number[] = [];
  let remaining = text;
  for (;;) {
    let best = -1;
    let bestLength = -1;
    question.options.forEach((option, index) => {
      if (selectedIndices.includes(index) || option.label.length === 0) {
        return;
      }
      const boundary = remaining.length === option.label.length || remaining.startsWith(`${option.label}, `);
      if (boundary && remaining.startsWith(option.label) && option.label.length > bestLength) {
        best = index;
        bestLength = option.label.length;
      }
    });
    if (best < 0) {
      break;
    }
    selectedIndices.push(best);
    remaining = remaining.slice(bestLength);
    if (remaining.startsWith(', ')) {
      remaining = remaining.slice(2);
    }
    if (remaining.length === 0) {
      break;
    }
  }
  const other = remaining.trim();
  return {
    selectedIndices,
    otherText: other.length > 0 ? other : null,
    dismissed: false,
  };
}

/** Peel omp's trailing ` (note: …)` and ` (auto-selected after timeout)` decorations. */
function stripOmpDecorations(raw: string): { note: string | null; text: string } {
  let text = raw;
  let note: string | null = null;
  // `formatQuestionResult` appends the timeout suffix first and the note last.
  const noteMatch = / \(note: ([\s\S]*)\)$/.exec(text);
  if (noteMatch) {
    note = noteMatch[1] ?? null;
    text = text.slice(0, -noteMatch[0].length);
  }
  if (text.endsWith(OMP_TIMEOUT_SUFFIX)) {
    text = text.slice(0, -OMP_TIMEOUT_SUFFIX.length);
  }
  return { note, text };
}

function parseOmpSingleAnswer(
  question: SessionChatQuestion,
  trimmed: string
): SessionChatQuestionExchangeAnswer | null {
  if (trimmed === OMP_CANCELLED_TEXT) {
    return { selectedIndices: [], otherText: null, dismissed: true };
  }
  if (trimmed === OMP_NO_SELECTION_TEXT) {
    return { selectedIndices: [], otherText: null, dismissed: false };
  }
  const lines = trimmed.split('\n');
  const opensAsOmp =
    lines[0] !== undefined &&
    (lines[0].startsWith(OMP_SELECTED_PREFIX) ||
      lines[0].startsWith(OMP_CUSTOM_PREFIX) ||
      lines[0].startsWith(OMP_NOTE_PREFIX));
  if (!opensAsOmp) {
    return null;
  }
  let selectedIndices: number[] = [];
  const extras: string[] = [];
  let index = 0;
  while (index < lines.length) {
    const line = lines[index] ?? '';
    if (line.startsWith(OMP_SELECTED_PREFIX)) {
      let labels = line.slice(OMP_SELECTED_PREFIX.length);
      if (labels.endsWith(OMP_TIMEOUT_SUFFIX)) {
        labels = labels.slice(0, -OMP_TIMEOUT_SUFFIX.length);
      }
      const matched = matchAnswerToOptions(question, labels);
      selectedIndices = matched.selectedIndices;
      if (matched.otherText) {
        extras.push(matched.otherText);
      }
      index += 1;
      continue;
    }
    if (line.startsWith(OMP_CUSTOM_PREFIX) || line.startsWith(OMP_NOTE_PREFIX)) {
      const prefix = line.startsWith(OMP_CUSTOM_PREFIX) ? OMP_CUSTOM_PREFIX : OMP_NOTE_PREFIX;
      let body = line.slice(prefix.length).replace(/^ /, '');
      index += 1;
      if (body.length === 0) {
        // Multi-line body: the following lines are two-space indented.
        const bodyLines: string[] = [];
        while (index < lines.length && (lines[index] ?? '').startsWith('  ')) {
          bodyLines.push((lines[index] ?? '').slice(2));
          index += 1;
        }
        body = bodyLines.join('\n');
      }
      if (body.length > 0) {
        extras.push(body);
      }
      continue;
    }
    index += 1;
  }
  return { selectedIndices, otherText: extras.length > 0 ? extras.join('\n') : null, dismissed: false };
}

function parseOmpAnswerValue(question: SessionChatQuestion, raw: string): SessionChatQuestionExchangeAnswer {
  const { note, text } = stripOmpDecorations(raw);
  if (text === '(cancelled)') {
    return { selectedIndices: [], otherText: note, dismissed: true };
  }
  if (text.length >= 2 && text.startsWith('"') && text.endsWith('"')) {
    const custom = text.slice(1, -1);
    return {
      selectedIndices: [],
      otherText: [custom, ...(note !== null ? [note] : [])].join('\n') || null,
      dismissed: false,
    };
  }
  const listBody = text.startsWith('[') && text.endsWith(']') ? text.slice(1, -1) : null;
  const matched = matchAnswerToOptions(question, listBody ?? text);
  const other = [...(matched.otherText !== null ? [matched.otherText] : []), ...(note !== null ? [note] : [])];
  return {
    selectedIndices: matched.selectedIndices,
    otherText: other.length > 0 ? other.join('\n') : null,
    dismissed: false,
  };
}

function parseOmpMultiAnswers(
  entries: ParsedQuestionWithId[],
  trimmed: string
): (SessionChatQuestionExchangeAnswer | null)[] | null {
  if (!trimmed.startsWith(OMP_MULTI_HEADER)) {
    return null;
  }
  const body = trimmed.slice(OMP_MULTI_HEADER.length);
  // Locate each question's `\n<id>: ` marker in order; custom inputs are
  // quoted but not escaped, so marker slicing is the only reliable read.
  const found: { end: number; index: number; start: number }[] = [];
  let from = 0;
  entries.forEach((entry, index) => {
    if (entry.id === null) {
      return;
    }
    const marker = `\n${entry.id}: `;
    const at = body.indexOf(marker, from);
    if (at >= 0) {
      found.push({ end: at + marker.length, index, start: at });
      from = at + marker.length;
    }
  });
  if (found.length === 0) {
    return null;
  }
  const answers: (SessionChatQuestionExchangeAnswer | null)[] = entries.map(() => null);
  found.forEach((entry, position) => {
    const next = found[position + 1];
    const raw = (next ? body.slice(entry.end, next.start) : body.slice(entry.end)).trimEnd();
    const question = entries[entry.index]?.question;
    if (question) {
      answers[entry.index] = parseOmpAnswerValue(question, raw);
    }
  });
  return answers;
}

/*
Hermes' clarify results are JSON (rendered verbatim into the tool output):
`{"question", "choices_offered", "user_response"}` for a single question —
`user_response` is a bare label, custom text, or a list for multi-select —
and `{"responses": [{id?, question, choices_offered, user_response}, …],
"timed_out"?}` for a batch, in question order with `""` marking a skip.
*/
function hermesResponseToAnswer(
  question: SessionChatQuestion,
  response: unknown,
  timedOut: boolean
): SessionChatQuestionExchangeAnswer {
  if (Array.isArray(response)) {
    const selectedIndices: number[] = [];
    const extras: string[] = [];
    for (const item of response) {
      if (typeof item !== 'string' || item.length === 0) {
        continue;
      }
      const optionIndex = question.options.findIndex(
        (option, index) => option.label === item && !selectedIndices.includes(index)
      );
      if (optionIndex >= 0) {
        selectedIndices.push(optionIndex);
      } else {
        extras.push(item);
      }
    }
    return { selectedIndices, otherText: extras.length > 0 ? extras.join(', ') : null, dismissed: false };
  }
  const text = typeof response === 'string' ? response.trim() : '';
  if (text.length === 0) {
    return { selectedIndices: [], otherText: null, dismissed: timedOut };
  }
  return matchAnswerToOptions(question, text);
}

function parseHermesAnswers(
  entries: ParsedQuestionWithId[],
  trimmed: string
): (SessionChatQuestionExchangeAnswer | null)[] | null {
  if (!trimmed.startsWith('{')) {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return null;
  }
  if (typeof parsed !== 'object' || parsed === null) {
    return null;
  }
  const record = parsed as Record<string, unknown>;
  const timedOut = record.timed_out === true;
  if (Array.isArray(record.responses)) {
    const answers: (SessionChatQuestionExchangeAnswer | null)[] = entries.map(() => null);
    record.responses.forEach((row, index) => {
      const question = entries[index]?.question;
      if (!question || typeof row !== 'object' || row === null) {
        return;
      }
      answers[index] = hermesResponseToAnswer(question, (row as Record<string, unknown>).user_response, timedOut);
    });
    return answers.some((answer) => answer !== null) ? answers : null;
  }
  const firstQuestion = entries[0]?.question;
  if ('user_response' in record && entries.length === 1 && firstQuestion) {
    return [hermesResponseToAnswer(firstQuestion, record.user_response, false)];
  }
  return null;
}

function parseAnswers(
  entries: ParsedQuestionWithId[],
  output: string
): (SessionChatQuestionExchangeAnswer | null)[] | null {
  const questions = entries.map((entry) => entry.question);
  const trimmed = output.trim();
  const firstQuestion = questions[0];
  if (questions.length === 1 && firstQuestion) {
    if (trimmed === PI_CANCELLED_TEXT) {
      return [{ selectedIndices: [], otherText: null, dismissed: true }];
    }
    if (trimmed.startsWith(PI_RESULT_PREFIX) && !trimmed.includes('\n')) {
      return [matchAnswerToOptions(firstQuestion, trimmed.slice(PI_RESULT_PREFIX.length))];
    }
    const ompSingle = parseOmpSingleAnswer(firstQuestion, trimmed);
    if (ompSingle !== null) {
      return [ompSingle];
    }
  }
  const ompMulti = parseOmpMultiAnswers(entries, trimmed);
  if (ompMulti !== null) {
    return ompMulti;
  }
  const hermes = parseHermesAnswers(entries, trimmed);
  if (hermes !== null) {
    return hermes;
  }
  const body = stripAnswerEnvelope(trimmed);
  if (body === null) {
    return null;
  }
  // Locate each question's `"question"="` marker in order; a question whose
  // marker is missing was skipped and keeps a null entry.
  const found: { index: number; end: number; start: number }[] = [];
  let from = 0;
  questions.forEach((question, index) => {
    if (question.question.length === 0) {
      return;
    }
    const marker = `"${question.question}"="`;
    const at = body.indexOf(marker, from);
    if (at >= 0) {
      found.push({ index, start: at, end: at + marker.length });
      from = at + marker.length;
    }
  });
  if (found.length === 0) {
    return null;
  }
  const answers: (SessionChatQuestionExchangeAnswer | null)[] = questions.map(() => null);
  found.forEach((entry, position) => {
    const next = found[position + 1];
    let raw = next ? body.slice(entry.end, next.start) : body.slice(entry.end);
    raw = raw.trimEnd();
    if (raw.endsWith(',')) {
      raw = raw.slice(0, -1);
    }
    if (raw.endsWith('"')) {
      raw = raw.slice(0, -1);
    }
    const question = questions[entry.index];
    if (question) {
      answers[entry.index] = matchAnswerToOptions(question, raw);
    }
  });
  return answers;
}

/**
 * The one entry point: an answered question tool pair becomes an exchange the
 * card can render; anything else (pending question, error result, unparseable
 * input) returns null and keeps the generic tool row.
 */
export function answeredSessionChatQuestionExchange(pair: SessionChatToolPair): SessionChatQuestionExchange | null {
  if (!pair.call || !pair.result || pair.result.isError === true) {
    return null;
  }
  if (!isSessionChatQuestionToolName(pair.call.name)) {
    return null;
  }
  const entries = parseQuestionsWithIds(pair.call.input, pair.call.name);
  if (!entries) {
    return null;
  }
  const output = pair.result.output.trim();
  if (output.length === 0) {
    return null;
  }
  const answers = parseAnswers(entries, output);
  return {
    questions: entries.map((entry) => entry.question),
    answers,
    fallbackText: answers === null ? output : null,
  };
}

function MicroLabel({ text }: { text: string }) {
  return <span className='text-[11px] font-semibold tracking-widest text-muted-foreground uppercase'>{text}</span>;
}

/** A chosen option, in the selected choice-row's visual language. */
function SelectedAnswerRow({ description, label }: { description?: string | undefined; label: string }) {
  return (
    <div className='flex w-full items-start gap-3 rounded-lg border border-primary/30 bg-primary/10 px-3 py-2'>
      <IconCheck aria-hidden='true' className='ghostex-chat-glyph-semantic mt-0.5 text-primary' />
      <span className='flex min-w-0 flex-1 flex-col gap-0.5'>
        <span className='text-sm leading-snug font-medium text-foreground'>{label}</span>
        {description && description !== label ? (
          <span className='text-xs leading-snug text-muted-foreground'>{description}</span>
        ) : null}
      </span>
    </div>
  );
}

/** The user's own words: an "Other" answer or notes beyond the option labels. */
function CustomAnswerRow({ label, text }: { label: string; text: string }) {
  return (
    <div className='flex w-full items-start gap-3 rounded-lg border border-primary/30 bg-primary/10 px-3 py-2'>
      <IconCheck aria-hidden='true' className='ghostex-chat-glyph-semantic mt-0.5 text-primary' />
      <span className='flex min-w-0 flex-1 flex-col gap-0.5'>
        <span className='text-[10px] font-semibold tracking-widest text-muted-foreground uppercase'>{label}</span>
        <span className='text-sm leading-snug whitespace-pre-wrap text-foreground [overflow-wrap:anywhere]'>
          {text}
        </span>
      </span>
    </div>
  );
}

function MutedAnswerRow({ text }: { text: string }) {
  return <div className='rounded-lg bg-foreground/[0.045] px-3 py-2 text-xs text-muted-foreground'>{text}</div>;
}

function QuestionSection({
  answer,
  hasParsedAnswers,
  index,
  question,
  total,
}: {
  answer: SessionChatQuestionExchangeAnswer | null;
  /** False when the whole result fell back to one blob (no per-question data). */
  hasParsedAnswers: boolean;
  index: number;
  question: SessionChatQuestion;
  total: number;
}) {
  const [showOptions, setShowOptions] = useState(false);
  const selectedIndices = answer?.selectedIndices ?? [];
  const selectedOptions = selectedIndices
    .map((optionIndex) => question.options[optionIndex])
    .filter((option) => option !== undefined);
  const unansweredLabel = answer?.dismissed ? 'Dismissed without answering' : hasParsedAnswers ? 'Skipped' : null;
  const showUnanswered = selectedOptions.length === 0 && !answer?.otherText && unansweredLabel !== null;

  return (
    <div className={cn('min-w-0 px-4 py-3.5 sm:px-5', index > 0 && 'border-t border-border/65')}>
      <div className='flex items-center gap-3'>
        <MicroLabel text={question.header ?? 'Question'} />
        {total > 1 ? (
          <span className='flex h-5 shrink-0 items-center rounded-md bg-muted/60 px-1.5 text-[10px] font-medium text-muted-foreground tabular-nums'>
            {index + 1}/{total}
          </span>
        ) : null}
      </div>
      {question.question.length > 0 ? <p className='mt-1.5 text-sm text-foreground/90'>{question.question}</p> : null}
      {selectedOptions.length > 0 || answer?.otherText || showUnanswered ? (
        <div className='mt-3 space-y-1.5'>
          {selectedOptions.map((option, selectionIndex) => (
            <SelectedAnswerRow
              description={option.description}
              key={`${selectionIndex}:${option.label}`}
              label={option.label}
            />
          ))}
          {answer?.otherText ? (
            <CustomAnswerRow
              label={selectedOptions.length > 0 ? 'Added note' : 'Custom answer'}
              text={answer.otherText}
            />
          ) : null}
          {showUnanswered && unansweredLabel ? <MutedAnswerRow text={unansweredLabel} /> : null}
        </div>
      ) : null}
      {question.options.length > 0 ? (
        <div className='mt-2'>
          <button
            aria-expanded={showOptions}
            className='flex items-center gap-1 rounded-md px-1 py-0.5 text-xs text-muted-foreground transition-colors duration-150 hover:text-foreground'
            data-slot='session-chat-question-options-toggle'
            onClick={() => setShowOptions((value) => !value)}
            type='button'
          >
            {/* One disclosure metaphor across the surface: a right chevron
                that turns a quarter, never a down chevron that flips. */}
            <IconChevronRight
              aria-hidden='true'
              className={cn('ghostex-chat-disclosure-chevron', showOptions && 'is-open')}
            />
            {showOptions ? 'Hide options' : `Show all ${question.options.length} options`}
          </button>
          {showOptions ? (
            <div className='mt-2'>
              <SessionChatChoiceRows
                onSelect={() => {}}
                options={question.options}
                readOnly
                selected={selectedIndices}
              />
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

export function SessionChatQuestionExchangeCard({ exchange }: { exchange: SessionChatQuestionExchange }) {
  return (
    <div
      className='ghostex-chat-question-exchange min-w-0 overflow-hidden rounded-2xl border border-border/65 bg-card'
      data-slot='session-chat-question-exchange'
    >
      {exchange.questions.map((question, index) => (
        <QuestionSection
          answer={exchange.answers?.[index] ?? null}
          hasParsedAnswers={exchange.answers !== null}
          index={index}
          key={`${index}:${question.question}`}
          question={question}
          total={exchange.questions.length}
        />
      ))}
      {exchange.fallbackText ? (
        <div className='border-t border-border/65 px-4 py-3.5 sm:px-5'>
          <MicroLabel text='Answer' />
          <p className='mt-1.5 text-sm leading-snug whitespace-pre-wrap text-foreground [overflow-wrap:anywhere]'>
            {exchange.fallbackText}
          </p>
        </div>
      ) : null}
    </div>
  );
}
