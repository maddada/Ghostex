import { v } from "convex/values";
import { internal } from "./_generated/api";
import type { Doc, Id } from "./_generated/dataModel";
import type { QueryCtx } from "./_generated/server";
import { internalMutation, internalQuery } from "./_generated/server";
import { upsertMessage } from "./slackIntake";
import { findThread, threadsForTicket, upsertThread } from "./slackThreads";
import { readTeamFlow } from "./teamFlow";

/** A member whose Ghostex sent no heartbeat (every 2 minutes) for this long counts as offline. */
const OFFLINE_AFTER_MS = 5 * 60 * 1000;

export async function memberForSlackUser(ctx: QueryCtx, teamId: Id<"teams">, slackUserId: string): Promise<Doc<"members"> | null> {
  return (
    (
      await ctx.db
        .query("members")
        .withIndex("by_team_slack_user", (q) => q.eq("teamId", teamId).eq("slackUserId", slackUserId))
        .collect()
    ).find((member) => member.removedAt === undefined) ?? null
  );
}

async function workingThreadOf(ctx: QueryCtx, teamId: Id<"teams">, ticket: string) {
  const row = await ctx.db
    .query("ticketWorkingThreads")
    .withIndex("by_team_ticket", (q) => q.eq("teamId", teamId).eq("ticket", ticket))
    .unique();
  const thread = row ? await ctx.db.get(row.threadId) : null;
  return thread ? { threadId: thread._id, channelId: thread.channelId, threadTs: thread.threadTs, permalink: thread.permalink ?? null } : null;
}

export function isOnline(member: Doc<"members"> | null, now: number): boolean {
  return member !== null && (member.lastSeenAt ?? 0) > now - OFFLINE_AFTER_MS;
}

export type LoadedRequest = {
  request: Doc<"slackRequests">;
  settings: Awaited<ReturnType<typeof readTeamFlow>>;
  requester: { id: Id<"members">; name: string; linearUserId: string | null } | null;
  /** The ticket this thread is already linked to. */
  threadTicket: string | null;
  /** Whether the request was typed in a ticket's working thread. */
  isWorkingThread: boolean;
};

export const loadRequest = internalQuery({
  args: { requestId: v.id("slackRequests") },
  handler: async (ctx, args): Promise<LoadedRequest | null> => {
    const request = await ctx.db.get(args.requestId);
    if (!request) return null;
    const requester = await memberForSlackUser(ctx, request.teamId, request.slackUserId);
    let threadTicket: string | null = null;
    let isWorkingThread = false;
    if (request.threadTs) {
      const thread = await findThread(ctx, request.teamId, request.channelId, request.threadTs);
      if (thread?.ticket) {
        threadTicket = thread.ticket;
        isWorkingThread = (await workingThreadOf(ctx, request.teamId, thread.ticket))?.threadId === thread._id;
      }
    }
    return {
      request,
      settings: await readTeamFlow(ctx, request.teamId),
      requester: requester ? { id: requester._id, name: requester.name, linearUserId: requester.linearUserId ?? null } : null,
      threadTicket,
      isWorkingThread,
    };
  },
});

const storedMessage = v.object({
  ts: v.optional(v.string()),
  thread_ts: v.optional(v.string()),
  user: v.optional(v.string()),
  bot_id: v.optional(v.string()),
  username: v.optional(v.string()),
  text: v.optional(v.string()),
  files: v.optional(
    v.array(
      v.object({
        id: v.optional(v.string()),
        name: v.optional(v.string()),
        mimetype: v.optional(v.string()),
        url_private: v.optional(v.string()),
        permalink: v.optional(v.string()),
      }),
    ),
  ),
  edited: v.optional(v.object({ ts: v.optional(v.string()) })),
});

/** Saves what step 1 read, so the Work page shows the whole thread. */
export const storeThread = internalMutation({
  args: {
    teamId: v.id("teams"),
    channelId: v.string(),
    threadTs: v.string(),
    permalink: v.optional(v.string()),
    messages: v.array(storedMessage),
  },
  handler: async (ctx, args) => {
    const team = await ctx.db.get(args.teamId);
    if (!team) return null;
    const thread = await upsertThread(ctx, args.teamId, {
      channelId: args.channelId,
      threadTs: args.threadTs,
      permalink: args.permalink,
    });
    for (const message of args.messages) await upsertMessage(ctx, team, thread, message);
    return null;
  },
});

/** Links the source thread to the ticket; returns whether Ghostex already posted the working thread's link there. */
export const linkThread = internalMutation({
  args: {
    teamId: v.id("teams"),
    channelId: v.string(),
    threadTs: v.string(),
    permalink: v.optional(v.string()),
    ticket: v.string(),
  },
  handler: async (ctx, args): Promise<{ linkPostedAt: number | null }> => {
    const thread = await upsertThread(ctx, args.teamId, {
      channelId: args.channelId,
      threadTs: args.threadTs,
      permalink: args.permalink,
      ticket: args.ticket,
    });
    return { linkPostedAt: thread.linkPostedAt ?? null };
  },
});

export const markLinkPosted = internalMutation({
  args: { teamId: v.id("teams"), channelId: v.string(), threadTs: v.string() },
  handler: async (ctx, args) => {
    const thread = await findThread(ctx, args.teamId, args.channelId, args.threadTs);
    if (thread) await ctx.db.patch(thread._id, { linkPostedAt: Date.now() });
    return null;
  },
});

export const getWorkingThread = internalQuery({
  args: { teamId: v.id("teams"), ticket: v.string() },
  handler: async (ctx, args) => await workingThreadOf(ctx, args.teamId, args.ticket),
});

/**
 * Records the working thread Ghostex just opened. When another request opened one for the same ticket in the meantime, that one wins and `created` is false.
 */
export const recordWorkingThread = internalMutation({
  args: { teamId: v.id("teams"), ticket: v.string(), channelId: v.string(), threadTs: v.string(), permalink: v.optional(v.string()) },
  handler: async (ctx, args) => {
    const existing = await workingThreadOf(ctx, args.teamId, args.ticket);
    if (existing) return { ...existing, created: false };
    const thread = await upsertThread(ctx, args.teamId, {
      channelId: args.channelId,
      threadTs: args.threadTs,
      permalink: args.permalink,
      ticket: args.ticket,
    });
    // The opening post is the working thread's own message; mark it as having the link so it never gets a "working on this" reply.
    await ctx.db.patch(thread._id, { linkPostedAt: Date.now() });
    await ctx.db.insert("ticketWorkingThreads", {
      teamId: args.teamId,
      ticket: args.ticket,
      threadId: thread._id,
      createdAt: Date.now(),
    });
    return { threadId: thread._id, channelId: args.channelId, threadTs: args.threadTs, permalink: args.permalink ?? null, created: true };
  },
});

export type QueuedWork =
  | { action: "start"; commandId: Id<"commands">; assigned: boolean; online: boolean; memberName: string | null }
  | { action: "message"; commandId: Id<"commands">; assigned: boolean; online: boolean; memberName: string | null }
  /** The ticket's cloud session is still starting, so there is no session to send to yet. */
  | { action: "cloudStarting"; memberName: string | null };

/**
 * Step 4's decision, made in one transaction so two requests for a ticket never start two sessions: the ticket's session exists → a `message` command to the Ghostex that runs it; none → a `start` command to the requester's Ghostex and the ticket's session row.
 *
 * CDXC:TeamSync 2026-10-09 DECISION:
 * User: one working session per ticket; "the session exists → send it your message", none → start it in the cloud or locally. A command waits in Convex while its Ghostex is off and starts when it is back. A cloud session gets the message too: the Ghostex that started it sends it through its cloud runner (server/src/cloud_runner.rs), since only that requester's Claude login can reach it.
 */
export const queueTicketWork = internalMutation({
  args: {
    requestId: v.id("slackRequests"),
    ticket: v.string(),
    runPlace: v.union(v.literal("cloud"), v.literal("local")),
    startPayload: v.any(),
    messagePayload: v.any(),
  },
  handler: async (ctx, args): Promise<QueuedWork> => {
    const request = await ctx.db.get(args.requestId);
    if (!request) throw new Error("The Slack request is gone.");
    const now = Date.now();
    const sessions = await ctx.db
      .query("ticketSessions")
      .withIndex("by_team_ticket", (q) => q.eq("teamId", request.teamId).eq("ticket", args.ticket))
      .collect();
    const live = sessions
      .filter((session) => session.status === "starting" || session.status === "running")
      .sort((left, right) => right.createdAt - left.createdAt)[0];
    if (live) {
      const owner = live.memberId ? await ctx.db.get(live.memberId) : null;
      if (live.runPlace === "cloud" && !live.sessionUrl) {
        return { action: "cloudStarting", memberName: owner?.name ?? null };
      }
      const session =
        live.runPlace === "cloud"
          ? { runPlace: "cloud", sessionUrl: live.sessionUrl }
          : live.sessionId
            ? { runPlace: "local", projectId: live.projectId ?? null, sessionId: live.sessionId }
            : null;
      const commandId = await ctx.db.insert("commands", {
        teamId: request.teamId,
        memberId: live.memberId,
        type: "slack.request",
        payload: { ...args.messagePayload, session },
        source: "slack",
        status: live.memberId ? "pending" : "unassigned",
        slackUserId: live.slackUserId,
        createdAt: now,
      });
      return { action: "message", commandId, assigned: live.memberId !== undefined, online: isOnline(owner, now), memberName: owner?.name ?? null };
    }
    const requester = await memberForSlackUser(ctx, request.teamId, request.slackUserId);
    const commandId = await ctx.db.insert("commands", {
      teamId: request.teamId,
      memberId: requester?._id,
      type: "slack.request",
      payload: args.startPayload,
      source: "slack",
      status: requester ? "pending" : "unassigned",
      slackUserId: request.slackUserId,
      createdAt: now,
    });
    await ctx.db.insert("ticketSessions", {
      teamId: request.teamId,
      ticket: args.ticket,
      memberId: requester?._id,
      slackUserId: request.slackUserId,
      runPlace: args.runPlace,
      status: "starting",
      commandId,
      createdAt: now,
      updatedAt: now,
    });
    return { action: "start", commandId, assigned: requester !== null, online: isOnline(requester, now), memberName: requester?.name ?? null };
  },
});

export const finishRequest = internalMutation({
  args: {
    requestId: v.id("slackRequests"),
    status: v.union(v.literal("waitingForChoice"), v.literal("done"), v.literal("failed")),
    candidates: v.optional(v.array(v.string())),
    outcome: v.optional(v.any()),
    error: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    await ctx.db.patch(args.requestId, {
      status: args.status,
      candidates: args.candidates,
      outcome: args.outcome,
      error: args.error,
      updatedAt: Date.now(),
    });
    return null;
  },
});

async function sessionRowForCommand(ctx: QueryCtx, command: Doc<"commands">) {
  const ticket = (command.payload as { ticket?: { key?: string } } | null)?.ticket?.key;
  if (!ticket) return null;
  return (
    (
      await ctx.db
        .query("ticketSessions")
        .withIndex("by_team_ticket", (q) => q.eq("teamId", command.teamId).eq("ticket", ticket))
        .collect()
    ).find((row) => row.commandId === command._id) ?? null
  );
}

/** What `reportCommand` needs: the finished command, its ticket session row and who ran it. */
export const commandReport = internalQuery({
  args: { commandId: v.id("commands") },
  handler: async (ctx, args) => {
    const command = await ctx.db.get(args.commandId);
    if (!command) return null;
    const member = command.memberId ? await ctx.db.get(command.memberId) : null;
    const settings = await readTeamFlow(ctx, command.teamId);
    return {
      command: {
        id: command._id,
        status: command.status,
        payload: command.payload,
        result: command.result ?? null,
        error: command.error ?? null,
      },
      memberName: member?.name ?? null,
      qcOwnerSlackUserId: settings.qcOwnerSlackUserId,
    };
  },
});

/** Records the session a `start` command made (or the error), so later requests for the ticket reach it. */
export const recordSessionResult = internalMutation({
  args: { commandId: v.id("commands") },
  handler: async (ctx, args) => {
    const command = await ctx.db.get(args.commandId);
    if (!command) return null;
    const row = await sessionRowForCommand(ctx, command);
    if (!row) return null;
    const now = Date.now();
    if (command.status !== "done") {
      await ctx.db.patch(row._id, {
        status: command.status === "cancelled" ? "cancelled" : "failed",
        error: command.error,
        updatedAt: now,
      });
      return null;
    }
    const result = (command.result ?? {}) as Record<string, unknown>;
    const text = (key: string) => (typeof result[key] === "string" && result[key] ? (result[key] as string) : undefined);
    await ctx.db.patch(row._id, {
      status: "running",
      memberId: row.memberId ?? command.memberId,
      runPlace: text("runPlace") === "local" ? "local" : text("runPlace") === "cloud" ? "cloud" : row.runPlace,
      projectId: text("projectId"),
      sessionId: text("sessionId"),
      sessionUrl: text("sessionUrl"),
      branch: text("branch"),
      runnerName: text("runner"),
      updatedAt: now,
    });
    return null;
  },
});

/** A button in one of Ghostex's private notes. Answers Slack at once; the work happens in scheduled functions. */
export const receiveInteraction = internalMutation({
  args: { payloadJson: v.string() },
  handler: async (ctx, args) => {
    const payload = JSON.parse(args.payloadJson) as {
      type?: string;
      user?: { id?: string };
      response_url?: string;
      actions?: { action_id?: string; value?: string }[];
    };
    if (payload.type !== "block_actions") return { handled: false };
    const action = payload.actions?.[0];
    const user = payload.user?.id;
    const responseUrl = payload.response_url;
    if (!action?.action_id || !user) return { handled: false };
    const replace = async (text: string) => {
      if (responseUrl) await ctx.scheduler.runAfter(0, internal.slackFlowReport.replaceNote, { responseUrl, text });
    };
    if (action.action_id.startsWith("gx_pick")) {
      const choice = JSON.parse(action.value ?? "{}") as { r?: string; t?: string };
      const requestId = ctx.db.normalizeId("slackRequests", choice.r ?? "");
      const request = requestId ? await ctx.db.get(requestId) : null;
      if (!request || request.slackUserId !== user || request.status !== "waitingForChoice") {
        await replace("This choice was already made.");
        return { handled: true };
      }
      const candidates = request.candidates ?? [];
      const chosen = choice.t === "*" ? candidates : candidates.filter((ticket) => ticket === choice.t);
      if (chosen.length === 0) return { handled: false };
      await ctx.db.patch(request._id, { status: "received", updatedAt: Date.now() });
      await ctx.scheduler.runAfter(0, internal.slackFlow.handleRequest, { requestId: request._id, chosenTickets: chosen });
      await replace(`Working on ${chosen.join(" and ")}.`);
      return { handled: true };
    }
    if (action.action_id === "gx_offline_ok") {
      await replace("OK. Claude starts as soon as your Ghostex is back.");
      return { handled: true };
    }
    if (action.action_id === "gx_offline_cancel") {
      const commandId = ctx.db.normalizeId("commands", action.value ?? "");
      const command = commandId ? await ctx.db.get(commandId) : null;
      if (!command || command.slackUserId !== user || !(command.status === "pending" || command.status === "unassigned")) {
        await replace("It already started, so there's nothing to cancel.");
        return { handled: true };
      }
      await ctx.db.patch(command._id, { status: "cancelled", completedAt: Date.now() });
      const row = await sessionRowForCommand(ctx, command);
      if (row) await ctx.db.patch(row._id, { status: "cancelled", updatedAt: Date.now() });
      await replace("Cancelled. The ticket and its working thread stay.");
      return { handled: true };
    }
    // Link buttons (Open session, Open working thread) also send a click; nothing to do.
    return { handled: true };
  },
});

export type PostTarget = {
  ticket: string;
  workingThread: { channelId: string; threadTs: string };
  sourceThreads: { channelId: string; threadTs: string }[];
};

/**
 * Where `ghostex slack post` goes: the ticket of the session Slack started (by its session id), else the first ticket the session links to that has a working thread.
 */
export const resolvePostTarget = internalQuery({
  args: { memberId: v.id("members"), sessionId: v.optional(v.string()), tickets: v.array(v.string()) },
  handler: async (ctx, args): Promise<PostTarget | null> => {
    const member = await ctx.db.get(args.memberId);
    if (!member) return null;
    const started = args.sessionId
      ? await ctx.db
          .query("ticketSessions")
          .withIndex("by_member_session", (q) => q.eq("memberId", args.memberId).eq("sessionId", args.sessionId))
          .first()
      : null;
    const candidates = [...(started ? [started.ticket] : []), ...args.tickets];
    for (const ticket of candidates) {
      const working = await workingThreadOf(ctx, member.teamId, ticket);
      if (!working) continue;
      const threads = await threadsForTicket(ctx, member.teamId, ticket);
      return {
        ticket,
        workingThread: { channelId: working.channelId, threadTs: working.threadTs },
        sourceThreads: threads
          .filter((thread) => thread._id !== working.threadId && thread.linkPostedAt !== undefined)
          .map((thread) => ({ channelId: thread.channelId, threadTs: thread.threadTs })),
      };
    }
    return null;
  },
});
