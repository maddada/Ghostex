import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import type {
  SessionChatAgentFleet,
  SessionChatQueuedPrompt,
} from "../../shared/session-chat";
import { SessionChatComposer } from "./session-chat-composer";
import { moveSessionChatQueueRow } from "./session-chat-queue";
import type { SessionChatQueueController } from "./use-session-chat";

/*
CDXC:SessionChatAgentFleet 2026-08-23:
The sub-agent strip, mounted on the REAL composer with a REAL prompt
queue, because the thing worth looking at is the boundary between them:

  - the fleet strip is OUTSIDE the composer container — work the agent owns
  - the queue strip is INSIDE it — input the user still owns

Two containers, two weights, stacked in that order. A reader should be able to
tell at a glance which rows they can act on, and here they actually can: the
queue rows drag, edit, send-now and delete, and Tab in the input queues a
prompt, all against local state instead of a daemon.

Everything is local — no gxserver, no transport, no daemon. The queue
controller below is the same shape useSessionChat hands the composer, with
every capability on, so the strip renders every control a live session would.

Monaco is deliberately not configured: with no `monacoVsBaseUrl` the composer
mounts its plain-textarea backend, which is the same path the mobile bundle
takes. The queue and fleet rows are identical either way.

The clocks tick for real: they interpolate from the fleet's `detectedAt`, a
fixed timestamp here, exactly as they do against a live daemon.
*/

const AT = "2026-08-23T10:00:00.000Z";

/*
Realistic tasks, NOT pre-ellipsized ones. The CLI truncates to whatever its own
column allows and we can only ever show what it painted, but seeding the story
with an already-cut string hides how much room the row actually gives the task.
The last row keeps a terminal-truncated value on purpose, because that is what
a narrow agent pane really sends.
*/
const FLEET: SessionChatAgentFleet = {
  agents: [
    {
      elapsedSeconds: 756,
      name: "general-purpose",
      task: "Fixing tool-row alignment in the transcript",
    },
    {
      elapsedSeconds: 195,
      name: "general-purpose",
      nested: 1,
      task: "Launching board_gxserver.rs split",
    },
    // A shorter name, so the shared name column is visibly doing its job.
    { elapsedSeconds: 12, name: "explore", task: "Reviewing the diff…" },
  ],
  detectedAt: AT,
};

function queued(
  id: string,
  text: string,
  extra: Partial<SessionChatQueuedPrompt> = {},
): SessionChatQueuedPrompt {
  return { createdAt: AT, id, state: "queued", text, updatedAt: AT, ...extra };
}

const QUEUE_SEED: SessionChatQueuedPrompt[] = [
  queued("1", "When those land, run the release preflight."),
  queued("2", "Then summarise what changed in three bullets."),
];

function SessionChatAgentFleetStripStory({
  agents,
  isWorking,
  queued: queuedCount,
  theme,
}: {
  agents: number;
  isWorking: boolean;
  queued: number;
  theme: "dark" | "light";
}) {
  const [prompts, setPrompts] = useState<SessionChatQueuedPrompt[]>(() =>
    QUEUE_SEED.slice(0, queuedCount),
  );
  const [sent, setSent] = useState<string[]>([]);

  const fleet: SessionChatAgentFleet | null =
    agents === 0 ? null : { ...FLEET, agents: FLEET.agents.slice(0, agents) };

  // The shape useSessionChat builds from a live daemon, with every capability
  // on so no control is hidden behind a missing endpoint.
  const queue: SessionChatQueueController = {
    capabilities: {
      canEdit: true,
      canQueue: true,
      canRemove: true,
      canReorder: true,
      canRetry: true,
      canSendNow: true,
      canSyncDraft: false,
      supported: true,
    },
    prompts,
    async queuePrompt(text) {
      setPrompts((current) => [
        ...current,
        queued(`local-${current.length + 1}-${text.length}`, text),
      ]);
    },
    async removePrompt(promptId) {
      const row = prompts.find((entry) => entry.id === promptId) ?? null;
      setPrompts((current) => current.filter((entry) => entry.id !== promptId));
      return row;
    },
    async reorder(promptIds) {
      setPrompts((current) => {
        let next = [...current];
        promptIds.forEach((id, target) => {
          const from = next.findIndex((entry) => entry.id === id);
          if (from >= 0) {
            next = moveSessionChatQueueRow(next, from, target);
          }
        });
        return next;
      });
    },
    async retryPrompt(promptId) {
      setPrompts((current) =>
        current.map((entry) =>
          entry.id === promptId ? queued(entry.id, entry.text) : entry,
        ),
      );
    },
    async sendNow(promptId) {
      const row = prompts.find((entry) => entry.id === promptId);
      if (row) {
        setSent((current) => [...current, row.text]);
      }
      setPrompts((current) => current.filter((entry) => entry.id !== promptId));
    },
  };

  return (
    <div
      className="ghostex-session-chat-scope flex h-screen flex-col justify-end bg-background p-4 text-foreground"
      data-chat-theme={theme}
    >
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-2">
        {/* Standing in for the transcript, so a send has somewhere to land. */}
        {sent.length > 0 ? (
          <div className="text-xs text-muted-foreground">
            Sent: {sent.map((text) => `“${text}”`).join(", ")}
          </div>
        ) : null}
        <SessionChatComposer
          agentFleet={fleet}
          isWorking={isWorking}
          onInterrupt={() => undefined}
          onSend={(text) => {
            setSent((current) => [...current, text]);
          }}
          queue={queue}
          sendOnEnter
          sessionKey="story-agent-fleet"
        />
      </div>
    </div>
  );
}

const meta = {
  args: { agents: 3, isWorking: true, queued: 2, theme: "dark" },
  argTypes: {
    agents: { control: { max: 3, min: 0, step: 1, type: "range" } },
    queued: { control: { max: 2, min: 0, step: 1, type: "range" } },
    theme: { control: "inline-radio", options: ["dark", "light"] },
  },
  component: SessionChatAgentFleetStripStory,
  parameters: { layout: "fullscreen" },
  title: "Chat/Sub-agents strip",
} satisfies Meta<typeof SessionChatAgentFleetStripStory>;

export default meta;

type Story = StoryObj<typeof meta>;

/** Three sub-agents over two queued prompts: both strips, in their own boxes. */
export const Dark: Story = { args: { theme: "dark" } };

export const Light: Story = { args: { theme: "light" } };

/** A single sub-agent, no queue — the quietest the strip ever gets. */
export const OneAgent: Story = { args: { agents: 1, queued: 0 } };

/** No fleet: the strip renders NOTHING, leaving the composer where it was. */
export const NoFleet: Story = { args: { agents: 0 } };

/**
 * One queued row and nothing in flight: nothing to reorder, so no handle
 * column, so no empty gutter indenting the prompt away from the input text.
 * With two rows (the default story) the handles are back and the gutter is
 * theirs.
 */
export const SingleQueuedPrompt: Story = { args: { agents: 1, queued: 1 } };
