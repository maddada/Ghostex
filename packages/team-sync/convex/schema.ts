import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

/**
 * CDXC:TeamSync 2026-10-09 DECISION:
 * User: each Work workspace uses its team's own Convex project, never one Ghostex runs. One person creates it and deploys these functions; teammates join with an invite link. Slack and Linear send their events here because it is always on, and a command reaches the requester's Ghostex the moment it is online.
 *
 * CDXC:TeamSync 2026-10-09 WHY:
 * There is no external auth provider: every function takes a member token, a random secret Ghostex keeps in its private credentials file, and this database stores only its SHA-256. Invite codes are stored the same way and work once.
 */
export default defineSchema({
  teams: defineTable({
    name: v.string(),
    /** Slack's `team_id` for the Slack workspace whose events this team receives. */
    slackTeamId: v.optional(v.string()),
    functionsVersion: v.number(),
    createdAt: v.number(),
  }).index("by_slack_team", ["slackTeamId"]),

  members: defineTable({
    teamId: v.id("teams"),
    name: v.string(),
    role: v.union(v.literal("owner"), v.literal("member")),
    tokenHash: v.string(),
    /** The member's Slack user id (`U…`), which is how a Slack command finds its requester. */
    slackUserId: v.optional(v.string()),
    linearUserId: v.optional(v.string()),
    joinedAt: v.number(),
    lastSeenAt: v.optional(v.number()),
    removedAt: v.optional(v.number()),
  })
    .index("by_token_hash", ["tokenHash"])
    .index("by_team", ["teamId"])
    .index("by_team_slack_user", ["teamId", "slackUserId"]),

  invites: defineTable({
    teamId: v.id("teams"),
    codeHash: v.string(),
    createdBy: v.id("members"),
    createdAt: v.number(),
    expiresAt: v.number(),
    usedAt: v.optional(v.number()),
    usedBy: v.optional(v.id("members")),
  }).index("by_code_hash", ["codeHash"]),

  /** Work addressed to one member's Ghostex (a Slack command, a teammate's request). */
  commands: defineTable({
    teamId: v.id("teams"),
    /** Who must run it; absent while a Slack user is not linked to any member yet. */
    memberId: v.optional(v.id("members")),
    type: v.string(),
    payload: v.any(),
    source: v.union(v.literal("slack"), v.literal("linear"), v.literal("ghostex")),
    status: v.union(
      v.literal("unassigned"),
      v.literal("pending"),
      v.literal("claimed"),
      v.literal("done"),
      v.literal("failed"),
      v.literal("cancelled"),
    ),
    /** Slack user id of the requester, kept so an unassigned command can be handed over later. */
    slackUserId: v.optional(v.string()),
    createdBy: v.optional(v.id("members")),
    createdAt: v.number(),
    claimedAt: v.optional(v.number()),
    /** The claiming Ghostex's server id, so it can pick its own claim back up after a restart. */
    claimedBy: v.optional(v.string()),
    completedAt: v.optional(v.number()),
    result: v.optional(v.any()),
    error: v.optional(v.string()),
  })
    .index("by_member", ["memberId"])
    .index("by_member_status", ["memberId", "status"])
    .index("by_team_slack_user_status", ["teamId", "slackUserId", "status"]),

  slackThreads: defineTable({
    teamId: v.id("teams"),
    channelId: v.string(),
    threadTs: v.string(),
    permalink: v.optional(v.string()),
    /** The ticket this thread belongs to: a Linear identifier (`SPX-1234`) or a GitHub issue (`owner/repo#12`). */
    ticket: v.optional(v.string()),
    createdAt: v.number(),
    updatedAt: v.number(),
    lastMessageAt: v.optional(v.number()),
    /** When Ghostex replied in this (source) thread with the working thread's link; it does that once. */
    linkPostedAt: v.optional(v.number()),
  })
    .index("by_team_channel_thread", ["teamId", "channelId", "threadTs"])
    .index("by_team_ticket", ["teamId", "ticket"]),

  /**
   * Every ticket a (source) thread is linked to; `slackThreads.ticket` holds only the latest one.
   *
   * CDXC:TeamSync 2026-10-09 DECISION:
   * User: a source thread that asks for several tickets ("Both, one thread each") gets every ticket's final result, not only the last one's.
   */
  slackThreadTickets: defineTable({
    teamId: v.id("teams"),
    threadId: v.id("slackThreads"),
    ticket: v.string(),
    createdAt: v.number(),
  })
    .index("by_team_ticket", ["teamId", "ticket"])
    .index("by_thread", ["threadId"]),

  slackMessages: defineTable({
    teamId: v.id("teams"),
    threadId: v.id("slackThreads"),
    ts: v.string(),
    userId: v.optional(v.string()),
    botId: v.optional(v.string()),
    authorName: v.optional(v.string()),
    text: v.string(),
    files: v.array(
      v.object({
        id: v.string(),
        name: v.optional(v.string()),
        mimetype: v.optional(v.string()),
        urlPrivate: v.optional(v.string()),
        permalink: v.optional(v.string()),
      }),
    ),
    postedAt: v.number(),
    editedAt: v.optional(v.number()),
    deletedAt: v.optional(v.number()),
  }).index("by_thread_ts", ["threadId", "ts"]),

  /**
   * CDXC:TeamSync 2026-10-09 DECISION:
   * User: one working thread per ticket in the working channel. A row here is that thread; the table has at most one row per ticket.
   */
  ticketWorkingThreads: defineTable({
    teamId: v.id("teams"),
    ticket: v.string(),
    threadId: v.id("slackThreads"),
    createdAt: v.number(),
    /** When a session first posted the ticket's final result (`ghostex slack post --final`), which comes after its validation request. */
    finalPostedAt: v.optional(v.number()),
  }).index("by_team_ticket", ["teamId", "ticket"]),

  /**
   * The team's flow settings (one row per team): the working channel, watch-only channels, which repo and Linear team new work from each channel goes to, where new work runs, and the team instructions every session started from Slack gets.
   *
   * CDXC:TeamSync 2026-10-09 DECISION:
   * User: the rules (one ticket, one working thread, one session) are built in; the team-specific details are team-wide settings plus a team instructions file "added to every session Ghostex starts".
   */
  teamFlowSettings: defineTable({
    teamId: v.id("teams"),
    workingChannelId: v.optional(v.string()),
    watchOnlyChannelIds: v.array(v.string()),
    channelRepos: v.array(
      v.object({
        channelId: v.string(),
        /** `owner/name` on GitHub. */
        repo: v.optional(v.string()),
        /** The Ghostex project's name, when the repo alone is ambiguous. */
        project: v.optional(v.string()),
        linearTeamKey: v.optional(v.string()),
        /** The Linear project (release) tickets created from this channel go to: its name, id or link. */
        linearProject: v.optional(v.string()),
      }),
    ),
    /** The Linear team new tickets go to when the channel names none. */
    linearTeamKey: v.optional(v.string()),
    defaultRunPlace: v.union(v.literal("cloud"), v.literal("local")),
    /** Tagged with the requester on every new working thread. */
    qcOwnerSlackUserId: v.optional(v.string()),
    instructions: v.optional(v.string()),
    /** The team's primary tracker (Linear tickets & projects, or GitHub issues & projects); absent = Linear when the team has a Linear key, otherwise GitHub. */
    tracker: v.optional(v.union(v.literal("linear"), v.literal("github"))),
    updatedAt: v.number(),
    updatedBy: v.optional(v.id("members")),
  }).index("by_team", ["teamId"]),

  /**
   * The Linear API keys the Slack flow reads and creates tickets with: the team's key (no `memberId`) and at most one key per member. Only internal functions read `key`; every query, mutation and action answers with whether a key is set, never with the key.
   *
   * CDXC:TeamSync 2026-10-09 DECISION:
   * User: "let's just use 1 key from the owner but also allow the user to override by setting their own key". The team key is set by an owner; a member who turns on "Create my Slack tickets with my own Linear key" has their workspace's Linear key stored here, and tickets they request from Slack are created with it so Linear shows them as the creator.
   *
   * CDXC:TeamSync 2026-10-09 WHY:
   * Convex has no per-row secrecy from the deployment's admins, so the team's Convex admins can technically read these keys; Settings says so next to the switch.
   */
  linearKeys: defineTable({
    teamId: v.id("teams"),
    /** Absent for the team's key. */
    memberId: v.optional(v.id("members")),
    key: v.string(),
    /** Whose Linear account the key acts as, as Linear named it when it was saved. */
    linearUserName: v.optional(v.string()),
    setBy: v.id("members"),
    updatedAt: v.number(),
  }).index("by_team_member", ["teamId", "memberId"]),

  /**
   * The team's flow steps (one row per team): the steps the Work page draws as a tracker on each ticket. A team without a row uses Ghostex's default flow.
   *
   * SEE-ALSO: server/src/work_mode/team_flow.rs (the rules, the default steps and the same validation), teamFlowSteps.ts.
   */
  teamFlowSteps: defineTable({
    teamId: v.id("teams"),
    steps: v.array(
      v.object({
        id: v.string(),
        label: v.string(),
        rule: v.object({
          kind: v.string(),
          label: v.optional(v.string()),
          states: v.optional(v.array(v.string())),
        }),
      }),
    ),
    updatedAt: v.number(),
    updatedBy: v.id("members"),
  }).index("by_team", ["teamId"]),

  /** One `@Ghostex` mention or `/ghostex` command, from receipt until its work is queued. */
  slackRequests: defineTable({
    teamId: v.id("teams"),
    kind: v.union(v.literal("mention"), v.literal("slashCommand")),
    slackUserId: v.string(),
    channelId: v.string(),
    threadTs: v.optional(v.string()),
    messageTs: v.optional(v.string()),
    text: v.string(),
    mode: v.optional(v.union(v.literal("cloud"), v.literal("local"))),
    prompt: v.string(),
    responseUrl: v.optional(v.string()),
    status: v.union(
      v.literal("received"),
      v.literal("waitingForChoice"),
      v.literal("done"),
      v.literal("failed"),
    ),
    /** The tickets the requester is asked to pick from. */
    candidates: v.optional(v.array(v.string())),
    /** Per ticket: what the flow did (created the ticket, opened the working thread, queued or forwarded). */
    outcome: v.optional(v.any()),
    error: v.optional(v.string()),
    createdAt: v.number(),
    updatedAt: v.number(),
  }),

  /**
   * CDXC:TeamSync 2026-10-09 DECISION:
   * User: one working session per ticket. A row here is that session from the moment its start is queued; a later request for the ticket is sent to it instead of starting another.
   */
  ticketSessions: defineTable({
    teamId: v.id("teams"),
    ticket: v.string(),
    /** Whose Ghostex runs it; absent while the requester's Slack user is not linked to a member. */
    memberId: v.optional(v.id("members")),
    /** The Slack requester; absent for a session started from the Work page (`workCloudSessions.ts`) by a member with no Slack user. */
    slackUserId: v.optional(v.string()),
    runPlace: v.union(v.literal("cloud"), v.literal("local")),
    status: v.union(v.literal("starting"), v.literal("running"), v.literal("failed"), v.literal("cancelled")),
    /** The Slack command that started it; absent for a session started from the Work page. */
    commandId: v.optional(v.id("commands")),
    projectId: v.optional(v.string()),
    sessionId: v.optional(v.string()),
    sessionUrl: v.optional(v.string()),
    branch: v.optional(v.string()),
    runnerName: v.optional(v.string()),
    error: v.optional(v.string()),
    createdAt: v.number(),
    updatedAt: v.number(),
  })
    .index("by_team_ticket", ["teamId", "ticket"])
    .index("by_member_session", ["memberId", "sessionId"]),

  /** Slack display names the Work page shows for user and channel ids, asked from Slack once and refreshed after a week. */
  slackNames: defineTable({
    teamId: v.id("teams"),
    kind: v.union(v.literal("user"), v.literal("channel")),
    slackId: v.string(),
    name: v.string(),
    updatedAt: v.number(),
  }).index("by_team_kind_id", ["teamId", "kind", "slackId"]),

  /** Calls the dev mocks (`devMocks.ts`) received; empty unless GHOSTEX_DEV_MOCKS is set. */
  devMockCalls: defineTable({
    service: v.union(v.literal("slack"), v.literal("linear")),
    method: v.string(),
    body: v.string(),
    at: v.number(),
  }),

  /**
   * Every signed Slack delivery, deduplicated by Slack's event id (Slack retries for 3 seconds). The body is kept as JSON text because Slack's payloads may hold keys Convex documents can't (`$…`).
   */
  slackEvents: defineTable({
    teamId: v.optional(v.id("teams")),
    eventId: v.string(),
    eventType: v.string(),
    payloadJson: v.string(),
    receivedAt: v.number(),
  }).index("by_event_id", ["eventId"]),

  linearEvents: defineTable({
    teamId: v.optional(v.id("teams")),
    deliveryId: v.string(),
    eventType: v.string(),
    action: v.string(),
    payloadJson: v.string(),
    receivedAt: v.number(),
  }).index("by_delivery", ["deliveryId"]),
});
