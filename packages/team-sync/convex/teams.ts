import { ConvexError, v } from "convex/values";
import { internal } from "./_generated/api";
import { internalAction, internalMutation, internalQuery, mutation, query } from "./_generated/server";
import { FUNCTIONS_VERSION, newSecret, requireMember, sha256Hex } from "./lib/auth";
import { linearKeyStatus } from "./linearKeys";

/**
 * Creates the team and its owner, and returns the owner's member token.
 *
 * Internal, so only someone holding this deployment's admin key can run it: `ghostex team deploy` runs it with the person's Convex CLI login (`npx convex run teams:createTeam`) right after deploying.
 */
export const createTeam = internalAction({
  args: { name: v.string(), ownerName: v.string() },
  handler: async (ctx, args): Promise<{ teamId: string; memberId: string; memberToken: string }> => {
    const memberToken = newSecret("gxm_", 32);
    const created: { teamId: string; memberId: string } = await ctx.runMutation(
      internal.teams.insertTeamWithOwner,
      { name: args.name, ownerName: args.ownerName, tokenHash: await sha256Hex(memberToken) },
    );
    return { ...created, memberToken };
  },
});

export const insertTeamWithOwner = internalMutation({
  args: { name: v.string(), ownerName: v.string(), tokenHash: v.string() },
  handler: async (ctx, args) => {
    const name = args.name.trim();
    const ownerName = args.ownerName.trim();
    if (!name || !ownerName) throw new ConvexError("Pass a team name and your name.");
    if (await ctx.db.query("teams").first()) {
      throw new ConvexError("This deployment already has a team. Ask its owner for an invite link.");
    }
    const now = Date.now();
    const teamId = await ctx.db.insert("teams", {
      name,
      functionsVersion: FUNCTIONS_VERSION,
      createdAt: now,
    });
    const memberId = await ctx.db.insert("members", {
      teamId,
      name: ownerName,
      role: "owner",
      tokenHash: args.tokenHash,
      joinedAt: now,
    });
    return { teamId, memberId };
  },
});

/** Records the functions version after a deploy, so `teams:info` can tell Ghostex it is current. */
export const recordFunctionsVersion = internalMutation({
  args: {},
  handler: async (ctx) => {
    for (const team of await ctx.db.query("teams").collect()) {
      await ctx.db.patch(team._id, { functionsVersion: FUNCTIONS_VERSION });
    }
    return FUNCTIONS_VERSION;
  },
});

/** Whether this deployment already has a team (asked by `ghostex team deploy` before creating one). */
export const hasTeam = internalQuery({
  args: {},
  handler: async (ctx) => (await ctx.db.query("teams").first()) !== null,
});

/** The team, the caller and the teammates, for `ghostex team status` and the Connections card. */
export const info = query({
  args: { memberToken: v.string() },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const team = await ctx.db.get(me.teamId);
    if (!team) throw new ConvexError("This member's team no longer exists.");
    const linearKeys = await linearKeyStatus(ctx, me);
    const members = await ctx.db
      .query("members")
      .withIndex("by_team", (q) => q.eq("teamId", team._id))
      .collect();
    return {
      team: {
        id: team._id,
        name: team.name,
        slackTeamId: team.slackTeamId ?? null,
        functionsVersion: team.functionsVersion,
      },
      deployedFunctionsVersion: FUNCTIONS_VERSION,
      // Which secrets this deployment holds (never their values), for the Settings Team rows.
      secrets: {
        slackBotToken: Boolean(process.env.SLACK_BOT_TOKEN),
        slackSigningSecret: Boolean(process.env.SLACK_SIGNING_SECRET),
        linearApiKey: linearKeys.team !== null,
      },
      linearKeys,
      me: {
        id: me._id,
        name: me.name,
        role: me.role,
        slackUserId: me.slackUserId ?? null,
        linearUserId: me.linearUserId ?? null,
      },
      members: members
        .filter((member) => member.removedAt === undefined)
        .map((member) => ({
          id: member._id,
          name: member.name,
          role: member.role,
          slackUserId: member.slackUserId ?? null,
          joinedAt: member.joinedAt,
          lastSeenAt: member.lastSeenAt ?? null,
        })),
    };
  },
});

/**
 * Sets who the caller is in Slack and Linear. Linking a Slack user takes the one-time code Ghostex sent that Slack user privately (slackFlow.ts `slackLinkCommand`) and hands that user's waiting commands to the caller.
 */
export const setIdentity = mutation({
  args: {
    memberToken: v.string(),
    name: v.optional(v.string()),
    slackUserId: v.optional(v.union(v.string(), v.null())),
    /** The one-time code from Ghostex's private Slack reply; needed to link a Slack user. */
    slackLinkCode: v.optional(v.string()),
    linearUserId: v.optional(v.union(v.string(), v.null())),
  },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    const patch: { name?: string; slackUserId?: string; linearUserId?: string } = {};
    if (args.name !== undefined && args.name.trim()) patch.name = args.name.trim();
    if (args.linearUserId !== undefined) patch.linearUserId = args.linearUserId?.trim() || undefined;
    let handedOver = 0;
    if (args.slackUserId !== undefined) {
      const slackUserId = args.slackUserId?.trim() || undefined;
      if (slackUserId && slackUserId !== me.slackUserId) {
        // Proof the caller is this Slack user (CDXC:TeamSync 2026-10-25 in slackFlow.ts): a code sent
        // only to them, unused and unexpired. A refused link throws, which rolls this back too, so a
        // mistyped user id does not burn the code.
        const code = args.slackLinkCode?.trim();
        const codeHash = code ? await sha256Hex(code) : null;
        const claim = codeHash
          ? await ctx.db
              .query("slackLinkCodes")
              .withIndex("by_code_hash", (q) => q.eq("codeHash", codeHash))
              .unique()
          : null;
        if (
          claim === null ||
          claim.teamId !== me.teamId ||
          claim.slackUserId !== slackUserId ||
          claim.usedAt !== undefined ||
          claim.expiresAt < Date.now()
        ) {
          throw new ConvexError(
            "Link your Slack user with the one-time code Ghostex sent you privately in Slack. Send @Ghostex any message there to get a new one.",
          );
        }
        await ctx.db.patch(claim._id, { usedAt: Date.now() });
        const taken = await ctx.db
          .query("members")
          .withIndex("by_team_slack_user", (q) => q.eq("teamId", me.teamId).eq("slackUserId", slackUserId))
          .collect();
        if (taken.some((member) => member._id !== me._id && member.removedAt === undefined)) {
          throw new ConvexError(`Slack user ${slackUserId} is already linked to another teammate.`);
        }
        const waiting = await ctx.db
          .query("commands")
          .withIndex("by_team_slack_user_status", (q) =>
            q.eq("teamId", me.teamId).eq("slackUserId", slackUserId).eq("status", "unassigned"),
          )
          .collect();
        for (const command of waiting) {
          await ctx.db.patch(command._id, { memberId: me._id, status: "pending" });
        }
        handedOver = waiting.length;
      }
      patch.slackUserId = slackUserId;
    }
    await ctx.db.patch(me._id, patch);
    return { memberId: me._id, handedOverCommands: handedOver };
  },
});

/** Marks the caller's Ghostex as online now. */
export const heartbeat = mutation({
  args: { memberToken: v.string() },
  handler: async (ctx, args) => {
    const me = await requireMember(ctx, args.memberToken);
    await ctx.db.patch(me._id, { lastSeenAt: Date.now() });
    return null;
  },
});
