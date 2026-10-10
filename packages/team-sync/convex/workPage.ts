import { v } from "convex/values";
import { internal } from "./_generated/api";
import type { Doc, Id } from "./_generated/dataModel";
import type { QueryCtx } from "./_generated/server";
import { action, internalMutation, internalQuery, query } from "./_generated/server";
import { normalizeTicket, requireMember } from "./lib/auth";
import { slackApiGet } from "./lib/slackApi";
import { readTeamFlow } from "./teamFlow";
import { threadsForTicket } from "./slackThreads";

/**
 * What the Ghostex Work page reads about a team's tickets: a batched summary per list row (Slack thread count, working thread, validation, team sessions) and one ticket's Slack threads and team sessions for its details.
 *
 * CDXC:WorkMode 2026-10-09 WHY:
 * "Validation" is done when the ticket has a Slack thread in a watch-only channel (the validation channel, e.g. #sprint-tickets-validation, where the flow links every request it forwards) or a session posted the ticket's final result with `ghostex slack post --final`, which the team instructions send after the single validation request. The validation post itself is made by the session, not by Ghostex, so it cannot be seen any other way.
 */

const MAX_TICKETS = 300;
/** A long thread shows its first message and this many latest replies; the rest is one click away in Slack. */
const LATEST_REPLIES = 6;
const NAME_TTL_MS = 7 * 24 * 60 * 60 * 1000;
/** A name Slack would not give (no `channels:read`, a deleted user) is asked again after this long. */
const MISSING_NAME_TTL_MS = 24 * 60 * 60 * 1000;
const MAX_NAME_LOOKUPS = 20;

type ThreadRole = "working" | "watchOnly" | "source";

async function workingRow(ctx: QueryCtx, teamId: Id<"teams">, ticket: string) {
  return await ctx.db
    .query("ticketWorkingThreads")
    .withIndex("by_team_ticket", (q) => q.eq("teamId", teamId).eq("ticket", ticket))
    .unique();
}

async function ticketThreads(ctx: QueryCtx, teamId: Id<"teams">, ticket: string) {
  return await threadsForTicket(ctx, teamId, ticket);
}

async function ticketSessionRows(ctx: QueryCtx, teamId: Id<"teams">, ticket: string) {
  return await ctx.db
    .query("ticketSessions")
    .withIndex("by_team_ticket", (q) => q.eq("teamId", teamId).eq("ticket", ticket))
    .collect();
}

/** One ticket's summary: what a list row and the team-flow tracker need. */
async function summarize(ctx: QueryCtx, teamId: Id<"teams">, ticket: string, watchOnly: Set<string>) {
  const [working, threads, sessions] = await Promise.all([
    workingRow(ctx, teamId, ticket),
    ticketThreads(ctx, teamId, ticket),
    ticketSessionRows(ctx, teamId, ticket),
  ]);
  const workingThread = working ? threads.find((thread) => thread._id === working.threadId) ?? (await ctx.db.get(working.threadId)) : null;
  const validationThread = threads
    .filter((thread) => watchOnly.has(thread.channelId))
    .sort((left, right) => left.createdAt - right.createdAt)[0];
  return {
    threadCount: threads.length,
    workingThread: workingThread
      ? { channelId: workingThread.channelId, threadTs: workingThread.threadTs, permalink: workingThread.permalink ?? null }
      : null,
    validation: validationThread
      ? { source: "watchOnlyThread", channelId: validationThread.channelId, permalink: validationThread.permalink ?? null, at: validationThread.createdAt }
      : working?.finalPostedAt !== undefined
        ? { source: "finalPost", channelId: null, permalink: workingThread?.permalink ?? null, at: working.finalPostedAt }
        : null,
    sessionCount: sessions.filter((row) => row.status === "starting" || row.status === "running").length,
  };
}

/** `{ tickets }` → a summary per ticket the team knows (tickets it has nothing for are left out). */
export const ticketSummaries = query({
  args: { memberToken: v.string(), tickets: v.array(v.string()) },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const settings = await readTeamFlow(ctx, me.teamId);
    const watchOnly = new Set(settings.watchOnlyChannelIds);
    const tickets = [...new Set(args.tickets.filter((ticket) => ticket.trim()).map(normalizeTicket))].slice(0, MAX_TICKETS);
    const summaries: Record<string, Awaited<ReturnType<typeof summarize>>> = {};
    for (const ticket of tickets) {
      const summary = await summarize(ctx, me.teamId, ticket, watchOnly);
      if (summary.threadCount > 0 || summary.workingThread || summary.validation || summary.sessionCount > 0) {
        summaries[ticket] = summary;
      }
    }
    return summaries;
  },
});

type StoredNames = { users: Record<string, string>; channels: Record<string, string>; stale: { kind: "user" | "channel"; slackId: string }[] };

async function storedNames(ctx: QueryCtx, teamId: Id<"teams">, userIds: Set<string>, channelIds: Set<string>): Promise<StoredNames> {
  const now = Date.now();
  const names: StoredNames = { users: {}, channels: {}, stale: [] };
  // Team members are named by their own Ghostex; Slack is asked only for everyone else.
  for (const member of await ctx.db.query("members").withIndex("by_team", (q) => q.eq("teamId", teamId)).collect()) {
    if (member.slackUserId && userIds.has(member.slackUserId)) names.users[member.slackUserId] = member.name;
  }
  const look = async (kind: "user" | "channel", slackId: string) => {
    const row = await ctx.db
      .query("slackNames")
      .withIndex("by_team_kind_id", (q) => q.eq("teamId", teamId).eq("kind", kind).eq("slackId", slackId))
      .unique();
    if (row?.name) (kind === "user" ? names.users : names.channels)[slackId] = row.name;
    const ttl = row?.name ? NAME_TTL_MS : MISSING_NAME_TTL_MS;
    if (!row || row.updatedAt < now - ttl) names.stale.push({ kind, slackId });
  };
  for (const id of userIds) if (!names.users[id]) await look("user", id);
  for (const id of channelIds) await look("channel", id);
  return names;
}

function presentMessage(message: Doc<"slackMessages">) {
  return {
    ts: message.ts,
    userId: message.userId ?? null,
    botId: message.botId ?? null,
    authorName: message.authorName ?? null,
    text: message.text,
    files: message.files.map((file) => ({ id: file.id, name: file.name ?? null, mimetype: file.mimetype ?? null, permalink: file.permalink ?? null })),
    postedAt: message.postedAt,
    editedAt: message.editedAt ?? null,
  };
}

const MENTION = /<@([A-Z0-9]+)(?:\|[^>]*)?>/g;

async function loadTicketDetails(ctx: QueryCtx, args: { memberToken: string; ticket: string }) {
  const me = await requireMember(ctx, args.memberToken);
  const ticket = normalizeTicket(args.ticket);
  const settings = await readTeamFlow(ctx, me.teamId);
  const watchOnly = new Set(settings.watchOnlyChannelIds);
  const summary = await summarize(ctx, me.teamId, ticket, watchOnly);
  const working = await workingRow(ctx, me.teamId, ticket);
  const threads = await ticketThreads(ctx, me.teamId, ticket);
  const role = (thread: Doc<"slackThreads">): ThreadRole =>
    thread._id === working?.threadId ? "working" : watchOnly.has(thread.channelId) ? "watchOnly" : "source";
  const order: Record<ThreadRole, number> = { working: 0, source: 1, watchOnly: 2 };
  threads.sort((left, right) => order[role(left)] - order[role(right)] || left.createdAt - right.createdAt);

  const userIds = new Set<string>();
  const channelIds = new Set<string>();
  const presented = [];
  for (const thread of threads) {
    channelIds.add(thread.channelId);
    const messages = (
      await ctx.db
        .query("slackMessages")
        .withIndex("by_thread_ts", (q) => q.eq("threadId", thread._id))
        .collect()
    ).filter((message) => message.deletedAt === undefined);
    const [first, ...replies] = messages;
    const latest = replies.slice(-LATEST_REPLIES);
    const shown = first ? [first, ...latest] : [];
    for (const message of shown) {
      if (message.userId) userIds.add(message.userId);
      for (const match of message.text.matchAll(MENTION)) userIds.add(match[1]);
    }
    presented.push({
      id: thread._id,
      channelId: thread.channelId,
      threadTs: thread.threadTs,
      permalink: thread.permalink ?? null,
      role: role(thread),
      isWorkingThread: role(thread) === "working",
      replyCount: replies.length,
      hiddenReplyCount: replies.length - latest.length,
      lastMessageAt: thread.lastMessageAt ?? null,
      messages: shown.map(presentMessage),
    });
  }

  const memberNames = new Map<Id<"members">, string>();
  const sessions = [];
  for (const row of (await ticketSessionRows(ctx, me.teamId, ticket)).sort((left, right) => right.updatedAt - left.updatedAt)) {
    if (row.memberId && !memberNames.has(row.memberId)) {
      const member = await ctx.db.get(row.memberId);
      if (member) memberNames.set(row.memberId, member.name);
    }
    if (!row.memberId && row.slackUserId) userIds.add(row.slackUserId);
    sessions.push({
      id: row._id,
      memberId: row.memberId ?? null,
      memberName: row.memberId ? (memberNames.get(row.memberId) ?? null) : null,
      slackUserId: row.slackUserId,
      isMe: row.memberId === me._id,
      runPlace: row.runPlace,
      status: row.status,
      projectId: row.projectId ?? null,
      sessionId: row.sessionId ?? null,
      sessionUrl: row.sessionUrl ?? null,
      branch: row.branch ?? null,
      runnerName: row.runnerName ?? null,
      error: row.error ?? null,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
    });
  }
  return {
    teamId: me.teamId,
    ticket,
    summary,
    threads: presented,
    sessions,
    names: await storedNames(ctx, me.teamId, userIds, channelIds),
  };
}

type TicketDetailsData = Awaited<ReturnType<typeof loadTicketDetails>>;

export const ticketDetailsData = internalQuery({
  args: { memberToken: v.string(), ticket: v.string() },
  handler: async (ctx, args): Promise<TicketDetailsData> => await loadTicketDetails(ctx, args),
});

export const rememberNames = internalMutation({
  args: {
    teamId: v.id("teams"),
    names: v.array(v.object({ kind: v.union(v.literal("user"), v.literal("channel")), slackId: v.string(), name: v.string() })),
  },
  handler: async (ctx, args) => {
    const now = Date.now();
    for (const entry of args.names) {
      const row = await ctx.db
        .query("slackNames")
        .withIndex("by_team_kind_id", (q) => q.eq("teamId", args.teamId).eq("kind", entry.kind).eq("slackId", entry.slackId))
        .unique();
      if (row) await ctx.db.patch(row._id, { name: entry.name, updatedAt: now });
      else await ctx.db.insert("slackNames", { teamId: args.teamId, ...entry, updatedAt: now });
    }
    return null;
  },
});

/** One name from Slack; an empty string when Slack would not say (the row is asked again later). */
async function askSlackName(kind: "user" | "channel", slackId: string): Promise<string> {
  try {
    if (kind === "user") {
      const result = await slackApiGet("users.info", { user: slackId });
      const user = result.user as { real_name?: string; name?: string; profile?: { display_name?: string; real_name?: string } } | undefined;
      return user?.profile?.display_name || user?.profile?.real_name || user?.real_name || user?.name || "";
    }
    // `conversations.info` needs `channels:read` (and `groups:read` for private channels), which apps made from an older manifest lack.
    const result = await slackApiGet("conversations.info", { channel: slackId });
    const channel = result.channel as { name?: string } | undefined;
    return channel?.name ?? "";
  } catch {
    return "";
  }
}

/** Slack's markup with mentions named: `<@U1>` → `@Rana`, `<#C1>` → `#bugs`. Links stay as `<url|label>` for the page to draw. */
function nameMentions(text: string, users: Record<string, string>, channels: Record<string, string>): string {
  return text
    .replace(MENTION, (_, id: string) => `@${users[id] ?? id}`)
    .replace(/<#([A-Z0-9]+)\|([^>]*)>/g, (_, id: string, name: string) => `#${name || channels[id] || id}`)
    .replace(/<#([A-Z0-9]+)>/g, (_, id: string) => `#${channels[id] ?? id}`)
    .replace(/<!(here|channel|everyone)(?:\|[^>]*)?>/g, "@$1");
}

/** `{ ticket }` → the ticket's Slack threads (working thread first, each with its first message and latest replies, authors and channels named) and the sessions the team runs on it. */
export const ticketDetails = action({
  args: { memberToken: v.string(), ticket: v.string() },
  handler: async (ctx, args) => {
    const data: TicketDetailsData = await ctx.runQuery(internal.workPage.ticketDetailsData, args);
    const { users, channels, stale } = data.names;
    const asked = stale.slice(0, MAX_NAME_LOOKUPS);
    if (asked.length > 0 && process.env.SLACK_BOT_TOKEN) {
      const found = await Promise.all(asked.map(async (entry) => ({ ...entry, name: await askSlackName(entry.kind, entry.slackId) })));
      for (const entry of found) {
        if (entry.name) (entry.kind === "user" ? users : channels)[entry.slackId] = entry.name;
      }
      await ctx.runMutation(internal.workPage.rememberNames, { teamId: data.teamId, names: found });
    }
    return {
      ticket: data.ticket,
      summary: data.summary,
      threads: data.threads.map((thread) => ({
        ...thread,
        channelName: channels[thread.channelId] ?? null,
        messages: thread.messages.map((message) => ({
          ...message,
          authorName: message.authorName ?? (message.userId ? (users[message.userId] ?? null) : null),
          isApp: message.botId !== null,
          text: nameMentions(message.text, users, channels),
        })),
      })),
      sessions: data.sessions.map((session) => ({
        ...session,
        memberName: session.memberName ?? (session.slackUserId ? users[session.slackUserId] : undefined) ?? null,
      })),
    };
  },
});

/** Called once a session's `--final` post went out: the ticket's validation step is done. */
export const recordFinalPost = internalMutation({
  args: { memberId: v.id("members"), ticket: v.string() },
  handler: async (ctx, args) => {
    const member = await ctx.db.get(args.memberId);
    if (!member) return null;
    const row = await workingRow(ctx, member.teamId, normalizeTicket(args.ticket));
    if (row && row.finalPostedAt === undefined) await ctx.db.patch(row._id, { finalPostedAt: Date.now() });
    return null;
  },
});
