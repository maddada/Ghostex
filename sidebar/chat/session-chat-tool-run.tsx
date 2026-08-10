// Compact work rows for tool calls, results, and file edits.

import {
  IconChevronRight,
  IconFileText,
  IconPencil,
  IconTerminal2,
  IconTool,
  IconWorldSearch,
} from "@tabler/icons-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import type {
  SessionChatToolCallBlock,
  SessionChatToolResultBlock,
} from "../../shared/session-chat";
import { cn } from "../../lib/utils";
import {
  diffFromSessionChatText,
  diffFromSessionChatToolCall,
  type SessionChatDiffLine,
} from "./session-chat-diff";
import {
  centerSessionChatExpansion,
  SessionChatExpansion,
} from "./session-chat-expansion";
import { pairSessionChatToolBlocks } from "./session-chat-tool-fold";
import {
  formatSessionChatToolInput,
  summarizeSessionChatToolInput,
} from "./session-chat-tool-summary";

export const SESSION_CHAT_MAX_TOOL_RESULT_CHARS = 4000;

type ToolBlock = SessionChatToolCallBlock | SessionChatToolResultBlock;

export interface SessionChatToolRunProps {
  blocks: readonly ToolBlock[];
  /** Global expand toggle; each row remains independently collapsible. */
  expandSignal?: boolean;
}

function clipBody(text: string): string {
  return text.length > SESSION_CHAT_MAX_TOOL_RESULT_CHARS
    ? `${text.slice(0, SESSION_CHAT_MAX_TOOL_RESULT_CHARS)}…`
    : text;
}

function toolIcon(name: string): ReactNode {
  const normalized = name.toLowerCase();
  if (/edit|write|patch|replace/.test(normalized)) {
    return <IconPencil aria-hidden="true" stroke={1.8} />;
  }
  if (/read|file|glob|list/.test(normalized)) {
    return <IconFileText aria-hidden="true" stroke={1.8} />;
  }
  if (/exec|command|shell|terminal|bash/.test(normalized)) {
    return <IconTerminal2 aria-hidden="true" stroke={1.8} />;
  }
  if (/web|search|browser|fetch|url/.test(normalized)) {
    return <IconWorldSearch aria-hidden="true" stroke={1.8} />;
  }
  return <IconTool aria-hidden="true" stroke={1.8} />;
}

function isCommandTool(name: string): boolean {
  return /exec|command|shell|terminal|bash/.test(name.toLowerCase());
}

function DiffView({ lines }: { lines: readonly SessionChatDiffLine[] }) {
  return (
    <div className="ghostex-chat-file-edit">
      <div className="ghostex-chat-diff">
        {lines.map((line, index) => (
          <div
            className={cn("ghostex-chat-diff-line", `is-${line.kind}`)}
            key={index}
          >
            <span className="ghostex-chat-diff-sign">
              {line.kind === "add" ? "+" : line.kind === "del" ? "-" : " "}
            </span>
            <span>{line.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function ToolBody({
  error,
  label,
  text,
}: {
  error?: boolean;
  label?: string;
  text: string;
}) {
  return (
    <div className="ghostex-chat-tool-body-group">
      {label ? <div className="ghostex-chat-tool-body-label">{label}</div> : null}
      <pre className={cn("ghostex-chat-tool-body", error && "is-error")}>
        {clipBody(text)}
      </pre>
    </div>
  );
}

function ToolLine({
  call,
  expandSignal,
  result,
}: {
  call?: SessionChatToolCallBlock;
  expandSignal: boolean;
  result?: SessionChatToolResultBlock;
}) {
  const [open, setOpen] = useState(expandSignal);
  const triggerRef = useRef<HTMLButtonElement>(null);
  useEffect(() => setOpen(expandSignal), [expandSignal]);

  const name = call?.name ?? "Result";
  const commandTool = isCommandTool(name);
  const inputPreview = call ? summarizeSessionChatToolInput(call.input) : "";
  const resultPreview = result?.output.split("\n")[0]?.trim().slice(0, 120) ?? "";
  const preview = commandTool ? "" : inputPreview || resultPreview;
  const callDiff = call ? diffFromSessionChatToolCall(call.name, call.input) : null;
  const resultDiff = result ? diffFromSessionChatText(result.output) : null;
  const diff = callDiff ?? resultDiff;
  const inputDetail = call ? formatSessionChatToolInput(call.input) : "";
  const inputAddsInfo = Boolean(
    call &&
      (commandTool || inputDetail.replace(/\s+/g, " ").trim() !== preview),
  );
  const hasResultBody = Boolean(result?.output && resultDiff === null);
  const hasDetail = diff !== null || inputAddsInfo || hasResultBody;

  return (
    <div
      className={cn("ghostex-chat-work-row", result?.isError && "is-error")}
      data-open={open}
    >
      <button
        aria-expanded={hasDetail ? open : undefined}
        className="ghostex-chat-work-trigger"
        disabled={!hasDetail}
        onClick={() => {
          if (hasDetail) {
            if (!open) {
              centerSessionChatExpansion(triggerRef.current);
            }
            setOpen((current) => !current);
          }
        }}
        ref={triggerRef}
        type="button"
      >
        <span className="ghostex-chat-work-icon">{toolIcon(name)}</span>
        <span className="ghostex-chat-work-heading">{name}</span>
        {preview ? <span className="ghostex-chat-work-preview">{preview}</span> : null}
        {hasDetail ? (
          <IconChevronRight
            aria-hidden="true"
            className={cn("ghostex-chat-work-chevron", open && "rotate-90")}
            stroke={2}
          />
        ) : null}
      </button>
      {hasDetail && open ? (
        <SessionChatExpansion
          className="ghostex-chat-work-detail"
          label={`Collapse ${name}`}
          onCollapse={() => setOpen(false)}
        >
          {inputAddsInfo && (!diff || commandTool) ? (
            <ToolBody
              label={commandTool ? "Command" : result ? "Input" : undefined}
              text={inputDetail}
            />
          ) : null}
          {diff ? <DiffView lines={diff} /> : null}
          {!diff && hasResultBody && result ? (
            <ToolBody
              error={result.isError}
              label={call ? "Result" : undefined}
              text={result.output}
            />
          ) : null}
        </SessionChatExpansion>
      ) : null}
    </div>
  );
}

export function SessionChatToolRun({ blocks, expandSignal = false }: SessionChatToolRunProps) {
  const pairs = pairSessionChatToolBlocks(blocks);
  return (
    <div className="ghostex-chat-tool-run">
      {pairs.map((pair, index) => (
        <ToolLine
          call={pair.call}
          expandSignal={expandSignal}
          key={index}
          result={pair.result}
        />
      ))}
    </div>
  );
}
