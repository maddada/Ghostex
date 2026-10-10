import { ConvexError } from "convex/values";
import type { Doc } from "../_generated/dataModel";
import type { QueryCtx } from "../_generated/server";

/** Bumped when Ghostex needs a newer copy of these functions; `teams:info` reports it. */
export const FUNCTIONS_VERSION = 5;

const INVITE_LIFETIME_MS = 7 * 24 * 60 * 60 * 1000;

export function inviteExpiry(now: number): number {
  return now + INVITE_LIFETIME_MS;
}

export async function sha256Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return hex(new Uint8Array(digest));
}

export function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * A new secret with the given prefix (`gxm_` member token, `gxi_` invite code).
 *
 * CDXC:TeamSync 2026-10-09 WHY:
 * Secrets are made only in actions: queries and mutations run with a seeded random generator so they can be retried deterministically, which is the wrong source for a bearer token.
 */
export function newSecret(prefix: string, byteCount: number): string {
  const bytes = new Uint8Array(byteCount);
  crypto.getRandomValues(bytes);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  const base64url = btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  return `${prefix}${base64url}`;
}

/** The calling member, or a ConvexError when the token is unknown or the member was removed. */
export async function requireMember(ctx: QueryCtx, memberToken: string): Promise<Doc<"members">> {
  const tokenHash = await sha256Hex(memberToken.trim());
  const member = await ctx.db
    .query("members")
    .withIndex("by_token_hash", (q) => q.eq("tokenHash", tokenHash))
    .unique();
  if (!member || member.removedAt !== undefined) {
    throw new ConvexError("This member token is not valid for this team. Join again with an invite link.");
  }
  return member;
}

/** Whether the member is one of the team's owners. */
export function isOwner(member: Doc<"members">): boolean {
  return member.role === "owner";
}

/** The ticket key as stored: trimmed, Linear identifiers upper-cased (`spx-12` → `SPX-12`). */
export function normalizeTicket(ticket: string): string {
  const trimmed = ticket.trim();
  if (trimmed.length === 0) throw new ConvexError("Pass a ticket.");
  return /^[A-Za-z][A-Za-z0-9]*-\d+$/.test(trimmed) ? trimmed.toUpperCase() : trimmed;
}
