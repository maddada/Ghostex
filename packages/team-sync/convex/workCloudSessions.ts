import { ConvexError, v } from "convex/values";
import { mutation } from "./_generated/server";
import { normalizeTicket, requireMember } from "./lib/auth";

/**
 * A cloud session a member started from the Ghostex Work page or `ghostex work-mode start --cloud` (server/src/work_mode/cloud_work.rs), added to the ticket's sessions like a Slack request's: teammates see it on the ticket, and a later Slack request for the ticket is sent to it (`slackFlowState.queueTicketWork`).
 */
export const record = mutation({
  args: {
    memberToken: v.string(),
    ticket: v.string(),
    sessionUrl: v.string(),
    branch: v.optional(v.string()),
    projectId: v.optional(v.string()),
    runnerName: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const ticket = normalizeTicket(args.ticket);
    const sessionUrl = args.sessionUrl.trim();
    if (!/^https:\/\/\S+$/.test(sessionUrl)) throw new ConvexError("A cloud session's link must be an https URL.");
    const now = Date.now();
    const existing = (
      await ctx.db
        .query("ticketSessions")
        .withIndex("by_team_ticket", (q) => q.eq("teamId", me.teamId).eq("ticket", ticket))
        .collect()
    ).find((row) => row.sessionUrl === sessionUrl);
    if (existing) {
      await ctx.db.patch(existing._id, { updatedAt: now });
      return existing._id;
    }
    return await ctx.db.insert("ticketSessions", {
      teamId: me.teamId,
      ticket,
      memberId: me._id,
      slackUserId: me.slackUserId,
      runPlace: "cloud",
      status: "running",
      sessionUrl,
      branch: args.branch,
      projectId: args.projectId,
      runnerName: args.runnerName,
      createdAt: now,
      updatedAt: now,
    });
  },
});
