// Terminal-surface "Queued: N" button (plan 016, section 5).
//
// The Ghostex prompt queue is edited in the chat view, but it keeps draining
// while the user watches the terminal, so the terminal surface needs to say how
// many prompts are still waiting and offer one click back to the place they can
// be edited. Top-left, mirroring the floating top-right
// SessionChatHostActionsCluster, hidden entirely at count 0.
//
// The count comes from the presentation projection's `queuedPromptCount` (the
// same field the sidebar badge reads), which gxserver republishes on every
// queue mutation and every scheduler delivery. That matters because the SERVER
// drains the queue: a prompt can leave it with no client action at all, so a
// count derived from this client's own calls would go stale on its own.

import { IconStackPush } from "@tabler/icons-react";
import { AppTooltip } from "@/sidebar/app-tooltip";

export function SessionChatQueuedPromptsButton({
  count,
  failedCount = 0,
  onOpenChat,
}: {
  count: number;
  failedCount?: number;
  onOpenChat(): void;
}) {
  if (count < 1) {
    return null;
  }
  const label = `${count} queued prompt${count === 1 ? "" : "s"}`;
  /*
  A `failed` row stops the queue draining until the user retries or deletes it,
  so this button has to say "stopped", not just "waiting". #ff6b6b is the same
  error red the sidebar badge switches to (sidebar/styles/session-cards.css).
  Only the glyph and the wording change: the button box is untouched, so the
  terminal chrome around it never moves.
  */
  const hasFailed = failedCount > 0;
  const tooltip = hasFailed
    ? `${label} — delivery failed, open Chat View to retry`
    : `${label} — open Chat View to edit them`;
  return (
    <div className="pointer-events-none absolute left-[13px] top-[13px] z-20 flex">
      <AppTooltip content={tooltip}>
        {/*
          data-slot opts out of the unlayered legacy `button:where(:not([data-slot]))`
          chrome in theme.css, exactly as the host action cluster does.
        */}
        <button
          aria-label={
            hasFailed
              ? `${label}. Delivery failed. Open Chat View.`
              : `${label}. Open Chat View.`
          }
          className="pointer-events-auto flex h-[28.125px] shrink-0 items-center gap-1.5 border border-[#2a2a2a] bg-[#101010] px-2.5 text-[11px] leading-none text-[#a6a6a6] shadow-[0_10px_22px_rgba(0,0,0,0.32)] transition-colors hover:bg-[#343434]"
          data-slot="session-chat-queued-prompts"
          onClick={(event) => {
            // This button hides its own container: switching to chat marks the
            // terminal layer aria-hidden, and leaving focus inside an
            // aria-hidden subtree strands assistive technology on an element it
            // is told does not exist. Release focus before the swap.
            event.currentTarget.blur();
            onOpenChat();
          }}
          type="button"
        >
          <IconStackPush
            aria-hidden="true"
            color={hasFailed ? "#ff6b6b" : undefined}
            size={14}
            stroke={2}
          />
          <span className={hasFailed ? "text-[#ff6b6b]" : undefined}>Queued: {count}</span>
        </button>
      </AppTooltip>
    </div>
  );
}
