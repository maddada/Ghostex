/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Banner for state the agent paints only on its TERMINAL SCREEN — an expired
login, a workspace-trust dialog, a usage-limit banner, a stream error, the CLI
having exited — plus the send watchdog's report that a message could not be
proven delivered. A transcript projection can never show any of it, so gxserver
classifies the screen capture it already reads for the option pills and hands
the result over as `terminalNotice`.

It wears the interactive card's visual language (shell / panel / action row)
tinted by severity, and sits directly above that card in the composer stack.
The `kind` is an OPEN set: nothing here branches on it, so an unknown kind from
a newer daemon still renders as title + detail + actions.

Dismissal is local and per-detection: hiding a notice remembers `kind` +
`detectedAt`, so the same detection stays hidden while a NEW one (a fresh
`detectedAt`) shows again. The server keeps re-sending an unresolved notice, so
a dismissal is a "I know, hide it" — never a resolution.
*/

import {
  IconAlertTriangle,
  IconChevronDown,
  IconInfoCircle,
  IconTerminal2,
  IconX,
} from "@tabler/icons-react";
import { useLayoutEffect, useRef, useState } from "react";
import type { SessionChatTerminalNotice } from "../../shared/session-chat";
import { cn } from "../../lib/utils";
import { Button } from "../../components/ui/button";

const SEND_FAILED_NOTICE =
  "Couldn't deliver those keys — switch to Terminal View to act there.";
const READ_ONLY_HINT = "Input is held by another device.";

export function sessionChatTerminalNoticeDismissKey(
  notice: SessionChatTerminalNotice | null,
): string | null {
  return notice ? `${notice.kind}:${notice.detectedAt}` : null;
}

export interface SessionChatTerminalNoticeCardProps {
  notice: SessionChatTerminalNotice | null;
  /** False while another device holds input: `sendKeys` actions go read-only. */
  canSend: boolean;
  /**
   * Writes an action's raw bytes verbatim through answerSessionChatPrompt's
   * approval lane — the same path the interactive card's Allow/Deny uses.
   */
  onSendKeys: (send: string) => Promise<void>;
  /** Host switch-back; `switchToTerminal` actions hide when the host has none. */
  onSwitchToTerminal?: () => void;
}

/** An action this build knows how to run, with its payload already proven. */
type RenderableNoticeAction =
  | { id: string; label: string; kind: "switchToTerminal" }
  | { id: string; label: string; kind: "sendKeys"; send: string };

interface SeverityStyle {
  shell: string;
  icon: string;
}

const SEVERITY_STYLES: Record<SessionChatTerminalNotice["severity"], SeverityStyle> = {
  error: { icon: "text-destructive", shell: "border-destructive/40 bg-destructive/10" },
  info: { icon: "text-muted-foreground", shell: "border-input bg-muted/20" },
  warning: { icon: "text-amber-400", shell: "border-amber-500/40 bg-amber-500/10" },
};

/**
 * Severity is a CLOSED set in this build's type but an open one on the wire: a
 * newer daemon can send a level this build has never heard of. Resolving it
 * through a runtime lookup keeps that notice on the muted `info` surface —
 * title, detail and actions all intact — instead of rendering it unstyled.
 */
function severityStyle(severity: string): SeverityStyle {
  const styles: Record<string, SeverityStyle | undefined> = SEVERITY_STYLES;
  return styles[severity] ?? SEVERITY_STYLES.info;
}

/** Severity-tinted surface, shaped like the interactive card's shell. */
function NoticeShell({
  children,
  kind,
  severity,
}: {
  children: React.ReactNode;
  kind: string;
  severity: SessionChatTerminalNotice["severity"];
}) {
  return (
    <div
      className={cn(
        "min-w-0 overflow-hidden rounded-2xl border",
        severityStyle(severity).shell,
      )}
      data-kind={kind}
      data-severity={severity}
      role="status"
    >
      {children}
    </div>
  );
}

export function SessionChatTerminalNoticeCard({
  canSend,
  notice,
  onSendKeys,
  onSwitchToTerminal,
}: SessionChatTerminalNoticeCardProps) {
  const [dismissedKey, setDismissedKey] = useState<string | null>(null);
  const [tailOpen, setTailOpen] = useState(false);
  const [sending, setSending] = useState(false);
  const [sendFailed, setSendFailed] = useState(false);
  const sendingRef = useRef(false);

  const noticeKey = sessionChatTerminalNoticeDismissKey(notice);

  // Every fresh detection starts clean: tail collapsed, no stale send state.
  // The dismissed key is deliberately NOT reset here — it holds the identity of
  // the detection the user hid, and only a different identity outlives it.
  useLayoutEffect(() => {
    sendingRef.current = false;
    setSending(false);
    setSendFailed(false);
    setTailOpen(false);
  }, [noticeKey]);

  if (!notice || noticeKey === null || noticeKey === dismissedKey) {
    return null;
  }

  // Actions are normalized here so the render below never has to re-prove that
  // a `sendKeys` action carries bytes. An action kind this build does not know
  // is DROPPED rather than guessed at — the title/detail still stand on their
  // own, and a button whose behaviour we cannot name would lie about what it
  // does.
  const actions: RenderableNoticeAction[] = [];
  for (const action of notice.actions ?? []) {
    if (action.kind === "switchToTerminal") {
      if (onSwitchToTerminal) {
        actions.push({ id: action.id, kind: "switchToTerminal", label: action.label });
      }
    } else if (action.kind === "sendKeys" && action.send !== undefined) {
      // A `sendKeys` action without bytes has nothing to write; an inert button
      // would claim an ability the notice never carried.
      actions.push({
        id: action.id,
        kind: "sendKeys",
        label: action.label,
        send: action.send,
      });
    }
  }

  const runSendKeys = (send: string): void => {
    if (sendingRef.current || !canSend) {
      return;
    }
    sendingRef.current = true;
    setSending(true);
    setSendFailed(false);
    void onSendKeys(send)
      .catch(() => {
        // The keystrokes never reached the TUI: say so instead of pretending
        // the notice was handled.
        setSendFailed(true);
      })
      .finally(() => {
        sendingRef.current = false;
        setSending(false);
      });
  };

  const SeverityIcon = notice.severity === "info" ? IconInfoCircle : IconAlertTriangle;

  return (
    <NoticeShell kind={notice.kind} severity={notice.severity}>
      <div className="flex items-start gap-2 px-3 py-2.5">
        <SeverityIcon
          aria-hidden="true"
          className={cn("mt-0.5 size-4 shrink-0", severityStyle(notice.severity).icon)}
          stroke={2}
        />
        <div className="min-w-0 flex-1">
          <p className="text-sm leading-snug font-medium text-foreground">{notice.title}</p>
          {notice.detail ? (
            <p className="mt-1 text-xs leading-snug text-muted-foreground">
              {notice.detail}
            </p>
          ) : null}
          {notice.screenTail ? (
            <div className="mt-2">
              <button
                className="group/tail inline-flex items-center gap-1 rounded-md text-[11px] font-medium text-muted-foreground outline-none transition-colors duration-150 hover:text-foreground"
                // The sidebar's legacy bare-button base paints a 1px app border
                // on every unnamed button; naming the slot opts this row out.
                data-slot="session-chat-notice-tail-toggle"
                onClick={() => setTailOpen((value) => !value)}
                type="button"
              >
                {tailOpen ? "Hide terminal output" : "Show terminal output"}
                <IconChevronDown
                  aria-hidden="true"
                  className={cn(
                    "size-3.5 shrink-0 transition-transform duration-150",
                    tailOpen && "rotate-180",
                  )}
                  stroke={2}
                />
              </button>
              {tailOpen ? (
                <div className="mt-2 min-w-0 rounded-lg border border-border/65 bg-background/70 p-3">
                  <pre className="max-h-40 min-w-0 overflow-auto font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-foreground [overflow-wrap:anywhere]">
                    {notice.screenTail}
                  </pre>
                </div>
              ) : null}
            </div>
          ) : null}
          {sendFailed ? (
            <p className="mt-2 text-[11px] leading-snug text-destructive/80">
              {SEND_FAILED_NOTICE}
            </p>
          ) : null}
          {actions.length > 0 ? (
            <div className="mt-3 flex flex-wrap items-center gap-2">
              {actions.map((action) =>
                action.kind === "switchToTerminal" ? (
                  <Button
                    key={action.id}
                    onClick={onSwitchToTerminal}
                    size="xs"
                    variant="outline"
                  >
                    <IconTerminal2 aria-hidden="true" stroke={2} />
                    {action.label}
                  </Button>
                ) : (
                  <Button
                    disabled={!canSend || sending}
                    key={action.id}
                    onClick={() => runSendKeys(action.send)}
                    size="xs"
                    variant="outline"
                    {...(canSend ? {} : { title: READ_ONLY_HINT })}
                  >
                    {action.label}
                  </Button>
                ),
              )}
            </div>
          ) : null}
        </div>
        <Button
          aria-label="Dismiss"
          onClick={() => setDismissedKey(noticeKey)}
          size="icon-xs"
          variant="ghost"
        >
          <IconX aria-hidden="true" stroke={2} />
        </Button>
      </div>
    </NoticeShell>
  );
}
