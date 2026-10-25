import { v } from "convex/values";
import type { Doc, Id } from "./_generated/dataModel";
import type { MutationCtx, QueryCtx } from "./_generated/server";
import { query } from "./_generated/server";
import { normalizeTicket, requireMember } from "./lib/auth";

const MESSAGES_PER_THREAD = 200;

export async function findThread(
  ctx: QueryCtx,
  teamId: Id<"teams">,
  channelId: string,
  threadTs: string,
): Promise<Doc<"slackThreads"> | null> {
  return await ctx.db
    .query("slackThreads")
    .withIndex("by_team_channel_thread", (q) =>
      q.eq("teamId", teamId).eq("channelId", channelId).eq("threadTs", threadTs),
    )
    .unique();
}

/** Records that a thread belongs to a ticket, next to the ticket it already has. */
async function linkThreadTicket(ctx: MutationCtx, teamId: Id<"teams">, threadId: Id<"slackThreads">, ticket: string) {
  const links = await ctx.db
    .query("slackThreadTickets")
    .withIndex("by_thread", (q) => q.eq("threadId", threadId))
    .collect();
  if (links.some((link) => link.ticket === ticket)) return;
  await ctx.db.insert("slackThreadTickets", { teamId, threadId, ticket, createdAt: Date.now() });
}

/** Every thread linked to a ticket: the ones whose (latest) ticket it is, and the ones linked to it next to another ticket. */
export async function threadsForTicket(ctx: QueryCtx, teamId: Id<"teams">, ticket: string): Promise<Doc<"slackThreads">[]> {
  const threads = await ctx.db
    .query("slackThreads")
    .withIndex("by_team_ticket", (q) => q.eq("teamId", teamId).eq("ticket", ticket))
    .collect();
  const links = await ctx.db
    .query("slackThreadTickets")
    .withIndex("by_team_ticket", (q) => q.eq("teamId", teamId).eq("ticket", ticket))
    .collect();
  for (const link of links) {
    if (threads.some((thread) => thread._id === link.threadId)) continue;
    const thread = await ctx.db.get(link.threadId);
    if (thread) threads.push(thread);
  }
  return threads;
}

/** The thread row for a Slack thread, created when Ghostex first sees it. */
export async function upsertThread(
  ctx: MutationCtx,
  teamId: Id<"teams">,
  fields: { channelId: string; threadTs: string; permalink?: string; ticket?: string },
): Promise<Doc<"slackThreads">> {
  const now = Date.now();
  const existing = await findThread(ctx, teamId, fields.channelId, fields.threadTs);
  if (existing) {
    const patch: Partial<Doc<"slackThreads">> = { updatedAt: now };
    if (fields.permalink) patch.permalink = fields.permalink;
    if (fields.ticket) patch.ticket = fields.ticket;
    await ctx.db.patch(existing._id, patch);
    if (fields.ticket) await linkThreadTicket(ctx, teamId, existing._id, fields.ticket);
    return { ...existing, ...patch };
  }
  const id = await ctx.db.insert("slackThreads", {
    teamId,
    channelId: fields.channelId,
    threadTs: fields.threadTs,
    permalink: fields.permalink,
    ticket: fields.ticket,
    createdAt: now,
    updatedAt: now,
  });
  if (fields.ticket) await linkThreadTicket(ctx, teamId, id, fields.ticket);
  return (await ctx.db.get(id))!;
}

async function workingThreadRow(ctx: QueryCtx, teamId: Id<"teams">, ticket: string) {
  return await ctx.db
    .query("ticketWorkingThreads")
    .withIndex("by_team_ticket", (q) => q.eq("teamId", teamId).eq("ticket", ticket))
    .unique();
}

async function presentThread(ctx: QueryCtx, thread: Doc<"slackThreads">, isWorkingThread: boolean) {
  const messages = await ctx.db
    .query("slackMessages")
    .withIndex("by_thread_ts", (q) => q.eq("threadId", thread._id))
    .take(MESSAGES_PER_THREAD);
  return {
    id: thread._id,
    channelId: thread.channelId,
    threadTs: thread.threadTs,
    permalink: thread.permalink ?? null,
    ticket: thread.ticket ?? null,
    isWorkingThread,
    lastMessageAt: thread.lastMessageAt ?? null,
    messages: messages
      .filter((message) => message.deletedAt === undefined)
      .map((message) => ({
        ts: message.ts,
        userId: message.userId ?? null,
        botId: message.botId ?? null,
        authorName: message.authorName ?? null,
        text: message.text,
        files: message.files,
        postedAt: message.postedAt,
        editedAt: message.editedAt ?? null,
      })),
  };
}

/** Every Slack thread linked to a ticket, working thread first, each with its messages. */
export const listForTicket = query({
  args: { memberToken: v.string(), ticket: v.string() },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const ticket = normalizeTicket(args.ticket);
    const working = await workingThreadRow(ctx, me.teamId, ticket);
    const threads = await threadsForTicket(ctx, me.teamId, ticket);
    threads.sort((left, right) => {
      const leftWorking = left._id === working?.threadId ? 0 : 1;
      const rightWorking = right._id === working?.threadId ? 0 : 1;
      return leftWorking - rightWorking || left.createdAt - right.createdAt;
    });
    return await Promise.all(
      threads.map((thread) => presentThread(ctx, thread, thread._id === working?.threadId)),
    );
  },
});

/** The ticket's working thread, or `null`. */
export const getWorkingThread = query({
  args: { memberToken: v.string(), ticket: v.string() },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const working = await workingThreadRow(ctx, me.teamId, normalizeTicket(args.ticket));
    if (!working) return null;
    const thread = await ctx.db.get(working.threadId);
    return thread ? await presentThread(ctx, thread, true) : null;
  },
});
