import { ConvexError, v } from "convex/values";
import { internal } from "./_generated/api";
import type { Id } from "./_generated/dataModel";
import { action, internalQuery } from "./_generated/server";
import { normalizeTicket, requireMember } from "./lib/auth";
import { escapeSlack, postMessage, updateMessage } from "./lib/slackApi";
import type { PostTarget } from "./slackFlowState";

const MAX_TEXT_CHARS = 3_500;
const MAX_REQUIREMENTS = 10;
const MAX_REQUIREMENT_CHARS = 300;

export const memberOfToken = internalQuery({
  args: { memberToken: v.string() },
  handler: async (ctx, args): Promise<Id<"members">> => (await requireMember(ctx, args.memberToken))._id,
});

/**
 * `ghostex slack post --session <ref> "<text>"`: a session posts to its ticket's working thread, as "Claude · SPX-1234". With `final`, the text also goes once to every source thread the ticket's requests came from.
 *
 * CDXC:TeamSync 2026-10-09 DECISION:
 * User: the agent decides when a milestone happened (numbered 1a, 1b, 2a… as the team instructions say) and posts it through a `ghostex slack post` command Ghostex gives it; the source thread gets only the final result (PR, QC package version, video).
 */
export const postForSession = action({
  args: {
    memberToken: v.string(),
    sessionId: v.optional(v.string()),
    tickets: v.array(v.string()),
    text: v.string(),
    final: v.optional(v.boolean()),
  },
  handler: async (ctx, args): Promise<{ ticket: string; posted: { channelId: string; ts: string }[] }> => {
    const text = args.text.trim();
    if (!text) throw new ConvexError("Pass the text to post.");
    if (text.length > MAX_TEXT_CHARS) throw new ConvexError(`Keep a post under ${MAX_TEXT_CHARS} characters.`);
    const memberId: Id<"members"> = await ctx.runQuery(internal.slackPost.memberOfToken, { memberToken: args.memberToken });
    const target: PostTarget | null = await ctx.runQuery(internal.slackFlowState.resolvePostTarget, {
      memberId,
      sessionId: args.sessionId,
      tickets: args.tickets.filter((ticket) => ticket.trim()).map(normalizeTicket),
    });
    if (!target) {
      throw new ConvexError("This session's ticket has no Slack working thread. Working threads are opened by `@Ghostex` in Slack.");
    }
    const username = `Claude · ${target.ticket}`;
    const posted = [await postMessage({ channel: target.workingThread.channelId, threadTs: target.workingThread.threadTs, text, username })];
    if (args.final) {
      for (const source of target.sourceThreads) {
        posted.push(await postMessage({ channel: source.channelId, threadTs: source.threadTs, text, username }));
      }
      await ctx.runMutation(internal.workPage.recordFinalPost, { memberId, ticket: target.ticket });
    }
    return { ticket: target.ticket, posted: posted.map((post) => ({ channelId: post.channel, ts: post.ts })) };
  },
});

type OpeningPost = { channelId: string; ts: string; head: string[]; tags: string };

/** The working thread's opening post a caller's own `slack.request` command opened. */
export const openingPostOf = internalQuery({
  args: { memberId: v.id("members"), commandId: v.string() },
  handler: async (ctx, args): Promise<OpeningPost | null> => {
    const commandId = ctx.db.normalizeId("commands", args.commandId);
    const command = commandId ? await ctx.db.get(commandId) : null;
    if (!command || command.memberId !== args.memberId || command.type !== "slack.request") return null;
    // Only the Slack flow opens working threads; a member-queued command never names one.
    if (command.source !== "slack" || command.createdBy !== undefined) return null;
    const post = (command.payload as { openingPost?: Partial<OpeningPost> | null } | null)?.openingPost;
    if (!post?.channelId || !post.ts || !Array.isArray(post.head)) return null;
    return { channelId: post.channelId, ts: post.ts, head: post.head.map(String), tags: String(post.tags ?? "") };
  },
});

/**
 * Rewrites a new working thread's opening post with the requirements the requester's Ghostex summarised from the source thread (server/src/team_sync/slack_requirements.rs), in place of the quote of the thread's first message.
 */
export const updateOpeningPost = action({
  args: { memberToken: v.string(), commandId: v.string(), requirements: v.array(v.string()) },
  handler: async (ctx, args): Promise<{ channelId: string; ts: string }> => {
    const memberId: Id<"members"> = await ctx.runQuery(internal.slackPost.memberOfToken, { memberToken: args.memberToken });
    const post: OpeningPost | null = await ctx.runQuery(internal.slackPost.openingPostOf, { memberId, commandId: args.commandId });
    if (!post) throw new ConvexError("This command opened no working thread.");
    const requirements = args.requirements
      .map((line) => line.replace(/\s+/g, " ").trim())
      .filter(Boolean)
      .slice(0, MAX_REQUIREMENTS)
      .map((line) => (line.length > MAX_REQUIREMENT_CHARS ? `${line.slice(0, MAX_REQUIREMENT_CHARS - 1)}…` : line));
    if (requirements.length === 0) throw new ConvexError("Pass at least one requirement.");
    const text = [...post.head, "*Requirements*", ...requirements.map((line) => `• ${escapeSlack(line)}`), post.tags]
      .filter(Boolean)
      .join("\n");
    await updateMessage(post.channelId, post.ts, text);
    return { channelId: post.channelId, ts: post.ts };
  },
});
