/*
CDXC:SessionChatQuestionExchange 2026-08-22:
Answered agent questions (AskUserQuestion / request_user_input) rendered as a
first-class card in the chat log instead of a generic tool row with raw JSON.
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

import { IconCheck, IconChevronRight } from "@tabler/icons-react";
import { useState } from "react";
import type { SessionChatQuestion } from "../../shared/session-chat";
import { cn } from "@/packages/components/utils";
import { SessionChatChoiceRows } from "./session-chat-choice-rows";
import type { SessionChatToolPair } from "./session-chat-tool-fold";

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
  const normalized = name.replace(/[^a-zA-Z0-9]/g, "").toLowerCase();
  return normalized === "askuserquestion" || normalized === "requestuserinput";
}

/** TS mirror of `parse_session_chat_questions` (the canonical input shape). */
export function parseSessionChatQuestionsInput(
  input: unknown,
): SessionChatQuestion[] | null {
  if (typeof input !== "object" || input === null) {
    return null;
  }
  const rawQuestions = (input as Record<string, unknown>).questions;
  if (!Array.isArray(rawQuestions) || rawQuestions.length === 0) {
    return null;
  }
  const questions: SessionChatQuestion[] = [];
  for (const raw of rawQuestions) {
    if (typeof raw !== "object" || raw === null) {
      continue;
    }
    const record = raw as Record<string, unknown>;
    const text = typeof record.question === "string" ? record.question : "";
    const options = parseQuestionOptions(record.options);
    if (text.length > 0 || options.length > 0) {
      questions.push({
        question: text,
        ...(typeof record.header === "string" ? { header: record.header } : {}),
        multiSelect: record.multiSelect === true,
        options,
      });
    }
  }
  return questions.length > 0 ? questions : null;
}

function parseQuestionOptions(raw: unknown): SessionChatQuestion["options"] {
  if (!Array.isArray(raw)) {
    return [];
  }
  const options: SessionChatQuestion["options"] = [];
  for (const option of raw) {
    if (typeof option === "string") {
      options.push({ label: option });
      continue;
    }
    if (typeof option !== "object" || option === null) {
      continue;
    }
    const record = option as Record<string, unknown>;
    if (typeof record.label !== "string") {
      continue;
    }
    options.push({
      label: record.label,
      ...(typeof record.description === "string"
        ? { description: record.description }
        : {}),
    });
  }
  return options;
}

const RESULT_PREFIXES = [
  "The user answered: ",
  "Your questions have been answered: ",
];
/** Substring-matched so the exact closing-sentence wording cannot break it. */
const RESULT_SUFFIX_MARKERS = [
  '". Read the answers carefully',
  '". You can now continue',
];
const DISMISSED_PREFIX = "[User dismissed";

/** `"…"` body between the known prefix and closing sentence, or null. */
function stripAnswerEnvelope(output: string): string | null {
  const prefix = RESULT_PREFIXES.find((candidate) =>
    output.startsWith(candidate),
  );
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
function matchAnswerToOptions(
  question: SessionChatQuestion,
  text: string,
): SessionChatQuestionExchangeAnswer {
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
      const boundary =
        remaining.length === option.label.length ||
        remaining.startsWith(`${option.label}, `);
      if (
        boundary &&
        remaining.startsWith(option.label) &&
        option.label.length > bestLength
      ) {
        best = index;
        bestLength = option.label.length;
      }
    });
    if (best < 0) {
      break;
    }
    selectedIndices.push(best);
    remaining = remaining.slice(bestLength);
    if (remaining.startsWith(", ")) {
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

function parseAnswers(
  questions: SessionChatQuestion[],
  output: string,
): (SessionChatQuestionExchangeAnswer | null)[] | null {
  const body = stripAnswerEnvelope(output.trim());
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
  const answers: (SessionChatQuestionExchangeAnswer | null)[] = questions.map(
    () => null,
  );
  found.forEach((entry, position) => {
    const next = found[position + 1];
    let raw = next ? body.slice(entry.end, next.start) : body.slice(entry.end);
    raw = raw.trimEnd();
    if (raw.endsWith(",")) {
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
export function answeredSessionChatQuestionExchange(
  pair: SessionChatToolPair,
): SessionChatQuestionExchange | null {
  if (!pair.call || !pair.result || pair.result.isError === true) {
    return null;
  }
  if (!isSessionChatQuestionToolName(pair.call.name)) {
    return null;
  }
  const questions = parseSessionChatQuestionsInput(pair.call.input);
  if (!questions) {
    return null;
  }
  const output = pair.result.output.trim();
  if (output.length === 0) {
    return null;
  }
  const answers = parseAnswers(questions, output);
  return {
    questions,
    answers,
    fallbackText: answers === null ? output : null,
  };
}

function MicroLabel({ text }: { text: string }) {
  return (
    <span className="text-[11px] font-semibold tracking-widest text-muted-foreground uppercase">
      {text}
    </span>
  );
}

/** A chosen option, in the selected choice-row's visual language. */
function SelectedAnswerRow({
  description,
  label,
}: {
  description?: string | undefined;
  label: string;
}) {
  return (
    <div className="flex w-full items-start gap-3 rounded-lg border border-primary/30 bg-primary/10 px-3 py-2">
      <IconCheck
        aria-hidden="true"
        className="ghostex-chat-glyph-semantic mt-0.5 text-primary"
      />
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-sm leading-snug font-medium text-foreground">
          {label}
        </span>
        {description && description !== label ? (
          <span className="text-xs leading-snug text-muted-foreground">
            {description}
          </span>
        ) : null}
      </span>
    </div>
  );
}

/** The user's own words: an "Other" answer or notes beyond the option labels. */
function CustomAnswerRow({ label, text }: { label: string; text: string }) {
  return (
    <div className="flex w-full items-start gap-3 rounded-lg border border-primary/30 bg-primary/10 px-3 py-2">
      <IconCheck
        aria-hidden="true"
        className="ghostex-chat-glyph-semantic mt-0.5 text-primary"
      />
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-[10px] font-semibold tracking-widest text-muted-foreground uppercase">
          {label}
        </span>
        <span className="text-sm leading-snug whitespace-pre-wrap text-foreground [overflow-wrap:anywhere]">
          {text}
        </span>
      </span>
    </div>
  );
}

function MutedAnswerRow({ text }: { text: string }) {
  return (
    <div className="rounded-lg bg-foreground/[0.045] px-3 py-2 text-xs text-muted-foreground">
      {text}
    </div>
  );
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
  const unansweredLabel = answer?.dismissed
    ? "Dismissed without answering"
    : hasParsedAnswers
      ? "Skipped"
      : null;
  const showUnanswered =
    selectedOptions.length === 0 &&
    !answer?.otherText &&
    unansweredLabel !== null;

  return (
    <div className={cn("min-w-0 px-4 py-3.5 sm:px-5", index > 0 && "border-t border-border/65")}>
      <div className="flex items-center gap-3">
        <MicroLabel text={question.header ?? "Question"} />
        {total > 1 ? (
          <span className="flex h-5 shrink-0 items-center rounded-md bg-muted/60 px-1.5 text-[10px] font-medium text-muted-foreground tabular-nums">
            {index + 1}/{total}
          </span>
        ) : null}
      </div>
      {question.question.length > 0 ? (
        <p className="mt-1.5 text-sm text-foreground/90">{question.question}</p>
      ) : null}
      {selectedOptions.length > 0 || answer?.otherText || showUnanswered ? (
        <div className="mt-3 space-y-1.5">
          {selectedOptions.map((option, selectionIndex) => (
            <SelectedAnswerRow
              description={option.description}
              key={`${selectionIndex}:${option.label}`}
              label={option.label}
            />
          ))}
          {answer?.otherText ? (
            <CustomAnswerRow
              label={selectedOptions.length > 0 ? "Added note" : "Custom answer"}
              text={answer.otherText}
            />
          ) : null}
          {showUnanswered && unansweredLabel ? (
            <MutedAnswerRow text={unansweredLabel} />
          ) : null}
        </div>
      ) : null}
      {question.options.length > 0 ? (
        <div className="mt-2">
          <button
            aria-expanded={showOptions}
            className="flex items-center gap-1 rounded-md px-1 py-0.5 text-xs text-muted-foreground transition-colors duration-150 hover:text-foreground"
            data-slot="session-chat-question-options-toggle"
            onClick={() => setShowOptions((value) => !value)}
            type="button"
          >
            {/* One disclosure metaphor across the surface: a right chevron
                that turns a quarter, never a down chevron that flips. */}
            <IconChevronRight
              aria-hidden="true"
              className={cn(
                "ghostex-chat-disclosure-chevron",
                showOptions && "is-open",
              )}
            />
            {showOptions
              ? "Hide options"
              : `Show all ${question.options.length} options`}
          </button>
          {showOptions ? (
            <div className="mt-2">
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

export function SessionChatQuestionExchangeCard({
  exchange,
}: {
  exchange: SessionChatQuestionExchange;
}) {
  return (
    <div
      className="ghostex-chat-question-exchange min-w-0 overflow-hidden rounded-2xl border border-border/65 bg-card"
      data-slot="session-chat-question-exchange"
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
        <div className="border-t border-border/65 px-4 py-3.5 sm:px-5">
          <MicroLabel text="Answer" />
          <p className="mt-1.5 text-sm leading-snug whitespace-pre-wrap text-foreground [overflow-wrap:anywhere]">
            {exchange.fallbackText}
          </p>
        </div>
      ) : null}
    </div>
  );
}
