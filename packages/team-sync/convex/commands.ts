import { ConvexError, v } from "convex/values";
import { internal } from "./_generated/api";
import type { Doc, Id } from "./_generated/dataModel";
import { mutation, query } from "./_generated/server";
import { requireMember } from "./lib/auth";

/** A claim older than this is treated as abandoned (the Ghostex that took it died mid-command). */
const ABANDONED_CLAIM_MS = 15 * 60 * 1000;
const OPEN_COMMAND_LIMIT = 50;

function present(command: Doc<"commands">) {
  return {
    id: command._id,
    type: command.type,
    payload: command.payload,
    source: command.source,
    status: command.status,
    slackUserId: command.slackUserId ?? null,
    createdAt: command.createdAt,
    claimedAt: command.claimedAt ?? null,
    claimedBy: command.claimedBy ?? null,
  };
}

/**
 * The caller's open commands (pending, plus claimed ones that may need picking back up), oldest first. Ghostex subscribes to this.
 */
export const listOpen = query({
  args: { memberToken: v.string() },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const byStatus = (status: "pending" | "claimed") =>
      ctx.db
        .query("commands")
        .withIndex("by_member_status", (q) => q.eq("memberId", me._id).eq("status", status))
        .take(OPEN_COMMAND_LIMIT);
    const open = [...(await byStatus("pending")), ...(await byStatus("claimed"))];
    open.sort((left, right) => left.createdAt - right.createdAt);
    return open.slice(0, OPEN_COMMAND_LIMIT).map(present);
  },
});

/** Recent commands of any status, newest first, for `ghostex team status`. */
export const listRecent = query({
  args: { memberToken: v.string(), limit: v.optional(v.number()) },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const limit = Math.min(Math.max(args.limit ?? 20, 1), 100);
    const rows = await ctx.db
      .query("commands")
      .withIndex("by_member", (q) => q.eq("memberId", me._id))
      .order("desc")
      .take(limit);
    return rows.map((command) => ({
      ...present(command),
      completedAt: command.completedAt ?? null,
      result: command.result ?? null,
      error: command.error ?? null,
    }));
  },
});

async function ownCommand(
  ctx: Parameters<typeof requireMember>[0],
  me: Doc<"members">,
  commandId: Id<"commands">,
): Promise<Doc<"commands">> {
  const command = await ctx.db.get(commandId);
  if (!command || command.memberId !== me._id) {
    throw new ConvexError("No such command for this member.");
  }
  return command;
}

/**
 * Takes a command so this Ghostex alone runs it. Returns the command when the claim succeeded, `null` when another Ghostex of the same member already holds it.
 */
export const claim = mutation({
  args: { memberToken: v.string(), commandId: v.id("commands"), clientId: v.string() },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const command = await ownCommand(ctx, me, args.commandId);
    const now = Date.now();
    const claimable =
      command.status === "pending" ||
      (command.status === "claimed" &&
        (command.claimedBy === args.clientId || (command.claimedAt ?? 0) < now - ABANDONED_CLAIM_MS));
    if (!claimable) return null;
    await ctx.db.patch(command._id, { status: "claimed", claimedAt: now, claimedBy: args.clientId });
    await ctx.db.patch(me._id, { lastSeenAt: now });
    return present({ ...command, status: "claimed", claimedAt: now, claimedBy: args.clientId });
  },
});

/** Finishes a claimed command with its result or error. */
export const complete = mutation({
  args: {
    memberToken: v.string(),
    commandId: v.id("commands"),
    clientId: v.string(),
    ok: v.boolean(),
    result: v.optional(v.any()),
    error: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const command = await ownCommand(ctx, me, args.commandId);
    if (command.status !== "claimed" || command.claimedBy !== args.clientId) {
      throw new ConvexError("This Ghostex does not hold that command.");
    }
    await ctx.db.patch(command._id, {
      status: args.ok ? "done" : "failed",
      completedAt: Date.now(),
      result: args.result,
      error: args.ok ? undefined : (args.error ?? "Failed."),
    });
    if (command.type === "slack.request") {
      await ctx.scheduler.runAfter(0, internal.slackFlowReport.reportCommand, { commandId: command._id });
    }
    return null;
  },
});

/**
 * Command types a member may queue directly. `slack.request` is queued only by the Slack flow's internal mutations (queueTicketWork, queueIssueCreation) with source "slack", because a Ghostex runs it as its own member's request.
 */
const MEMBER_COMMAND_TYPES = new Set(["ping"]);

/** Queues a command for the caller or a teammate (Ghostex-originated work, and `ghostex team ping`). */
export const enqueue = mutation({
  args: {
    memberToken: v.string(),
    to: v.optional(v.id("members")),
    type: v.string(),
    payload: v.optional(v.any()),
  },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const memberId = args.to ?? me._id;
    const target = await ctx.db.get(memberId);
    if (!target || target.teamId !== me.teamId || target.removedAt !== undefined) {
      throw new ConvexError("No such teammate.");
    }
    const type = args.type.trim();
    if (!type) throw new ConvexError("Pass a command type.");
    if (!MEMBER_COMMAND_TYPES.has(type)) throw new ConvexError(`Teammates cannot queue "${type}" commands.`);
    return await ctx.db.insert("commands", {
      teamId: me.teamId,
      memberId,
      type,
      payload: args.payload ?? null,
      source: "ghostex",
      status: "pending",
      createdBy: me._id,
      createdAt: Date.now(),
    });
  },
});

/** Cancels a command that has not finished; the addressee or its creator may. */
export const cancel = mutation({
  args: { memberToken: v.string(), commandId: v.id("commands") },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const command = await ctx.db.get(args.commandId);
    if (!command || (command.memberId !== me._id && command.createdBy !== me._id)) {
      throw new ConvexError("No such command for this member.");
    }
    if (command.status === "done" || command.status === "failed") return null;
    await ctx.db.patch(command._id, { status: "cancelled", completedAt: Date.now() });
    return null;
  },
});
