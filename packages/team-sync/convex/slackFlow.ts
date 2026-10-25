import { v } from "convex/values";
import { internal } from "./_generated/api";
import type { ActionCtx } from "./_generated/server";
import { internalAction } from "./_generated/server";
import {
  createLinearIssue,
  findLinearProject,
  linearIssue,
  linearTeams,
  type LinearIssue,
  type LinearTeam,
} from "./lib/linearApi";
import {
  addReaction,
  downloadFile,
  escapeSlack,
  permalink,
  postMessage,
  postPrivateNote,
  threadReplies,
  userNames,
  type SlackApiMessage,
} from "./lib/slackApi";
import { newSecret, sha256Hex } from "./lib/auth";
import { buildThreadContext, messageUserIds, type ThreadContext } from "./lib/threadContext";
import { findLinearProjectLinks, findTickets, parseTicketKey, titleFromText, type TicketRef } from "./lib/tickets";
import type { LoadedRequest, QueuedWork } from "./slackFlowState";

/**
 * CDXC:TeamSync 2026-10-09 DECISION:
 * User: `@Ghostex cloud|local …` is "smart" in five steps, always in this order: read the thread; find the ticket (one → use it, several → ask, none → create it in Linear); find the ticket's working thread or open it; start or reuse the ticket's one session; report back (the source thread gets the link once and later only the final result). It never starts work without a ticket, never opens a second working thread for a ticket, and never starts a second session where one exists. Steps 1–3 and the decision in step 4 run here in Convex, which is always on; the session itself starts on the requester's Ghostex (server/src/team_sync/slack_request.rs).
 *
 * CDXC:TeamSync 2026-10-09 DECISION:
 * User: a request in a watch-only channel (e.g. #sprint-tickets-validation) is forwarded to the ticket's working thread with the requester's name, the link and the words; Ghostex only reacts 👀 there and posts nothing else.
 *
 * SEE-ALSO: server/src/team_sync/slack_request.rs (the command this queues), slackFlowReport.ts (step 5), slackPost.ts (`ghostex slack post`).
 */

const MAX_IMAGES = 5;
const MAX_IMAGE_BYTES = 8 * 1024 * 1024;

type Note = (text: string, blocks?: unknown[]) => Promise<void>;

type FlowContext = {
  loaded: LoadedRequest;
  thread: ThreadContext;
  names: Map<string, string>;
  requesterName: string;
  /** The requester's own message (or the thread, for `/ghostex` there is none). */
  sourcePermalink: string | null;
  threadPermalink: string | null;
  teams: LinearTeam[];
  /** The key that finds and reads tickets, and the key that creates them (`linearKeys:forFlow`). */
  readKey: string | null;
  createKey: string | null;
  /** A GitHub issue the requester's Ghostex created for this request. */
  createdKey: string | null;
  note: Note;
  /** Working threads whose link the source thread still has to get. */
  sourceLinks: { label: string; permalink: string | null; channelId: string }[];
};

function storableMessage(message: SlackApiMessage) {
  return {
    ts: message.ts,
    thread_ts: message.thread_ts,
    user: message.user,
    bot_id: message.bot_id,
    username: message.username,
    text: message.text,
    files: (message.files ?? []).map((file) => ({
      id: file.id,
      name: file.name,
      mimetype: file.mimetype,
      url_private: file.url_private,
      permalink: file.permalink,
    })),
    edited: message.edited ? { ts: message.edited.ts } : undefined,
  };
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Runs the flow for one request. With `chosenTickets`, the requester already picked from several. */
export const handleRequest = internalAction({
  args: {
    requestId: v.id("slackRequests"),
    chosenTickets: v.optional(v.array(v.string())),
    /** A GitHub issue the requester's Ghostex just created for this request (slackGithubIssue.ts). */
    createdTicket: v.optional(v.string()),
  },
  handler: async (ctx, args): Promise<null> => {
    const loaded: LoadedRequest | null = await ctx.runQuery(internal.slackFlowState.loadRequest, {
      requestId: args.requestId,
    });
    if (!loaded) return null;
    const { request } = loaded;
    const note: Note = (text, blocks) =>
      postPrivateNote({
        channel: request.channelId,
        user: request.slackUserId,
        threadTs: request.threadTs,
        text,
        blocks,
        responseUrl: request.responseUrl,
      });
    try {
      const outcome = await runFlow(ctx, loaded, args.chosenTickets ?? null, note, args.createdTicket ?? null);
      await ctx.runMutation(internal.slackFlowState.finishRequest, {
        requestId: request._id,
        status: outcome.status,
        candidates: outcome.status === "waitingForChoice" ? outcome.candidates : undefined,
        outcome: outcome.status === "done" ? outcome.tickets : undefined,
      });
    } catch (error) {
      const message = errorText(error);
      await ctx.runMutation(internal.slackFlowState.finishRequest, {
        requestId: request._id,
        status: "failed",
        error: message,
      });
      await note(`Ghostex couldn't handle this: ${message}`).catch(() => undefined);
    }
    return null;
  },
});

type FlowOutcome =
  | { status: "waitingForChoice"; candidates: string[] }
  | { status: "done"; tickets: Record<string, unknown>[] };

async function runFlow(
  ctx: ActionCtx,
  loaded: LoadedRequest,
  chosenTickets: string[] | null,
  note: Note,
  createdKey: string | null,
): Promise<FlowOutcome> {
  const { request, settings } = loaded;
  const watchOnly = settings.watchOnlyChannelIds.includes(request.channelId);

  // Step 1: read the thread (a `/ghostex` at the top of a channel has none: the prompt is all there is).
  const messages = request.threadTs ? await threadReplies(request.channelId, request.threadTs) : [];
  const names = await userNames([...messageUserIds(messages), request.slackUserId]);
  const thread = buildThreadContext(messages, names, request.prompt);
  const requesterName = names.get(request.slackUserId) ?? loaded.requester?.name ?? request.slackUserId;
  let sourcePermalink: string | null = null;
  let threadPermalink: string | null = null;
  if (request.threadTs) {
    threadPermalink = (await permalink(request.channelId, request.threadTs).catch(() => undefined)) ?? null;
    sourcePermalink =
      request.messageTs && request.messageTs !== request.threadTs
        ? ((await permalink(request.channelId, request.messageTs).catch(() => undefined)) ?? threadPermalink)
        : threadPermalink;
    await ctx.runMutation(internal.slackFlowState.storeThread, {
      teamId: request.teamId,
      channelId: request.channelId,
      threadTs: request.threadTs,
      permalink: threadPermalink ?? undefined,
      messages: messages.map(storableMessage),
    });
  }
  const { readKey, createKey } = await ctx.runQuery(internal.linearKeys.forFlow, {
    teamId: request.teamId,
    memberId: loaded.requester?.id,
  });
  // CDXC:WorkMode 2026-10-09 DECISION:
  // User: the workspace's primary tracker is "Linear Tickets & Projects or Github Issues & Projects". With GitHub, the flow finds GitHub issue (and PR) links in the thread and, with none, the requester's Ghostex creates the GitHub issue with `gh` when it claims the command (Convex has no GitHub token).
  const githubPrimary = (settings.tracker ?? (readKey ? "linear" : "github")) === "github";
  const teams = readKey && !githubPrimary ? await linearTeams(readKey) : [];
  const flow: FlowContext = {
    loaded,
    thread,
    names,
    requesterName,
    sourcePermalink,
    threadPermalink,
    teams,
    readKey,
    createKey,
    createdKey,
    note,
    sourceLinks: [],
  };

  // Step 2: find the ticket.
  let tickets: TicketRef[];
  if (chosenTickets) {
    tickets = chosenTickets.map(parseTicketKey);
  } else if (loaded.isWorkingThread && loaded.threadTicket) {
    tickets = [parseTicketKey(loaded.threadTicket)];
  } else {
    tickets = findTickets(thread.searchText, teams.map((team) => team.key)).filter((ticket) => !githubPrimary || ticket.kind === "github");
    if (tickets.length === 0 && loaded.threadTicket) tickets = [parseTicketKey(loaded.threadTicket)];
  }
  if (!chosenTickets && tickets.length > 1) {
    await askWhichTicket(flow, tickets);
    return { status: "waitingForChoice", candidates: tickets.map((ticket) => ticket.key) };
  }
  let createdTicket: LinearIssue | null = null;
  if (tickets.length === 0) {
    if (watchOnly) {
      await note("I couldn't find a ticket in this thread, so there's nothing to forward.");
      return { status: "done", tickets: [] };
    }
    if (githubPrimary) return await queueGithubIssue(ctx, flow);
    createdTicket = await createTicketFromThread(flow);
    tickets = [{ kind: "linear", key: createdTicket.identifier }];
  }

  const outcomes: Record<string, unknown>[] = [];
  for (const ticket of tickets) {
    outcomes.push(await workOnTicket(ctx, flow, ticket, createdTicket, watchOnly));
  }
  if (flow.sourceLinks.length > 0 && request.threadTs) {
    const [first] = flow.sourceLinks;
    const text =
      flow.sourceLinks.length === 1
        ? `Working on this in ${first.permalink ? `<${first.permalink}|the working thread>` : "the working thread"} in <#${first.channelId}>. The result will be posted here.\n${first.label}`
        : `Working on this in one working thread per ticket in <#${first.channelId}>. The results will be posted here.\n${flow.sourceLinks
            .map((link) => `• ${link.label}${link.permalink ? ` · <${link.permalink}|working thread>` : ""}`)
            .join("\n")}`;
    await postMessage({ channel: request.channelId, threadTs: request.threadTs, text });
    await ctx.runMutation(internal.slackFlowState.markLinkPosted, {
      teamId: request.teamId,
      channelId: request.channelId,
      threadTs: request.threadTs,
    });
  }
  return { status: "done", tickets: outcomes };
}

function ticketUrl(ticket: TicketRef, issue: LinearIssue | null): string | null {
  if (issue?.url) return issue.url;
  if (ticket.kind === "github") return `https://github.com/${ticket.repo}/${ticket.pullRequest ? "pull" : "issues"}/${ticket.number}`;
  return null;
}

function ticketLabel(ticket: TicketRef, issue: LinearIssue | null): string {
  const url = ticketUrl(ticket, issue);
  const key = url ? `<${url}|${ticket.key}>` : ticket.key;
  return issue?.title ? `${key} · ${escapeSlack(issue.title)}` : key;
}

/** Several tickets: a private note with one button per ticket, plus "both, one thread each". */
async function askWhichTicket(flow: FlowContext, tickets: TicketRef[]): Promise<void> {
  const request = flow.loaded.request;
  const titles = await Promise.all(
    tickets.map(async (ticket) =>
      (ticket.kind === "linear" && flow.readKey ? (await linearIssue(flow.readKey, ticket.key))?.title : null) ?? null,
    ),
  );
  const button = (text: string, value: string, index: number) => ({
    type: "button",
    action_id: `gx_pick_${index}`,
    text: { type: "plain_text", text: text.length > 75 ? `${text.slice(0, 74)}…` : text },
    value: JSON.stringify({ r: request._id, t: value }),
  });
  const question = `This thread mentions ${tickets.length} tickets. Which one is this for?`;
  await flow.note(question, [
    { type: "section", text: { type: "mrkdwn", text: question } },
    {
      type: "actions",
      elements: [
        ...tickets.map((ticket, index) => button(titles[index] ? `${ticket.key} · ${titles[index]}` : ticket.key, ticket.key, index)),
        button(tickets.length === 2 ? "Both, one thread each" : "All, one thread each", "*", tickets.length),
      ],
    },
  ]);
}

/**
 * No ticket in the thread: create it in Linear from the thread, in the Linear team the channel maps to, and in the Linear project (release) the thread links or the channel maps to.
 *
 * CDXC:TeamSync 2026-10-09 DECISION:
 * User: the command never works without a Linear ticket; when the thread has none, Ghostex creates it automatically (project/team from the channel mapping) and links the Slack thread to it. When the thread or the channel mapping names a Linear project, the new ticket goes into it.
 *
 * CDXC:TeamSync 2026-10-09 DECISION:
 * User: "let's just use 1 key from the owner but also allow the user to override by setting their own key". The ticket is created with the requester's own key when they stored one (Linear shows them as its creator), else with the team's key.
 */
async function createTicketFromThread(flow: FlowContext): Promise<LinearIssue> {
  const { request, settings, requester } = flow.loaded;
  const { createKey } = flow;
  if (!createKey) {
    throw new Error(
      "there's no ticket in this thread, and this team has no Linear key to create one. Ask a team owner to set the team's Linear key in Ghostex (Settings → Workspaces → Linear for the team).",
    );
  }
  const mapping = settings.channelRepos.find((entry) => entry.channelId === request.channelId);
  // A project linked in the thread is the requester's own choice, so it wins over the channel's.
  const projectRef = findLinearProjectLinks(flow.thread.searchText)[0] ?? mapping?.linearProject ?? null;
  let project = projectRef ? await findLinearProject(createKey, projectRef) : null;
  if (projectRef && !project) {
    await flow.note(`Linear has no project ${projectRef}, so the new ticket has no project.`);
  }
  const teamKey = mapping?.linearTeamKey ?? settings.linearTeamKey;
  const team = teamKey
    ? flow.teams.find((candidate) => candidate.key.toUpperCase() === teamKey.toUpperCase())
    : flow.teams.find((candidate) => project?.teamIds[0] === candidate.id);
  if (!team) {
    if (teamKey) throw new Error(`Linear has no team with the key ${teamKey}.`);
    throw new Error(
      "there's no ticket in this thread and no Linear team for new tickets. Run `ghostex team flow set --linear-team <KEY>` (or map this channel with `ghostex team flow map`).",
    );
  }
  if (project && !project.teamIds.includes(team.id)) {
    await flow.note(`The Linear project ${project.name} isn't shared with the ${team.key} team, so the new ticket has no project.`);
    project = null;
  }
  const fromThread = request.threadTs && request.messageTs !== request.threadTs ? flow.thread.rootText : "";
  const title = titleFromText(fromThread || request.prompt) || "Request from Slack";
  const description = [
    `Requested by ${flow.requesterName} in Slack${flow.sourcePermalink ? `: ${flow.sourcePermalink}` : "."}`,
    "",
    `> ${request.prompt || "(no prompt)"}`,
    ...(flow.thread.transcript ? ["", "**Slack thread**", "", "```", flow.thread.transcript, "```"] : []),
  ].join("\n");
  return await createLinearIssue(createKey, {
    teamId: team.id,
    title,
    description,
    assigneeId: requester?.linearUserId ?? undefined,
    projectId: project?.id,
  });
}

/**
 * No GitHub issue in the thread of a GitHub team: the requester's Ghostex creates it (`slack.request` with `action: "createIssue"`, server/src/team_sync/slack_request.rs) in the repo the channel maps to, and the flow runs again with that issue once it reports back (slackFlowReport.ts).
 */
/**
 * The command that links the requester's Slack user to their Ghostex, with a one-time code only this
 * requester sees: it goes into their private reply, and Convex keeps only its SHA-256 for 30 minutes.
 *
 * CDXC:TeamSync 2026-10-25 DECISION:
 * User (security audit, Q5 "one-time code sent privately in Slack"): linking a Slack user needs proof that the member controls it. Before this any member could link any unlinked Slack user and receive that person's waiting requests (`teams:setIdentity`). The code reaches only the Slack user who wrote to Ghostex, and setIdentity links a Slack user only with an unused, unexpired code for that same user and team.
 * SEE-ALSO: teams.ts `setIdentity`, server/src/team_sync/operations.rs (`--slack-code`), apps/desktop/src/app/window/settings_modal/tabs/workspaces/team.rs (Settings).
 */
async function slackLinkCommand(ctx: ActionCtx, request: { teamId: FlowContext["loaded"]["request"]["teamId"]; slackUserId: string }): Promise<string> {
  const code = newSecret("gxl_", 16);
  await ctx.runMutation(internal.slackFlowState.recordSlackLinkCode, {
    teamId: request.teamId,
    slackUserId: request.slackUserId,
    codeHash: await sha256Hex(code),
  });
  return `ghostex team identity --slack-user ${request.slackUserId} --slack-code ${code}`;
}

async function queueGithubIssue(ctx: ActionCtx, flow: FlowContext): Promise<FlowOutcome> {
  const { request, settings } = flow.loaded;
  const mapping = settings.channelRepos.find((entry) => entry.channelId === request.channelId) ?? null;
  const fromThread = request.threadTs && request.messageTs !== request.threadTs ? flow.thread.rootText : "";
  const title = titleFromText(fromThread || request.prompt) || "Request from Slack";
  const body = [
    `Requested by ${flow.requesterName} in Slack${flow.sourcePermalink ? `: ${flow.sourcePermalink}` : "."}`,
    "",
    `> ${request.prompt || "(no prompt)"}`,
    ...(flow.thread.transcript ? ["", "**Slack thread**", "", "```", flow.thread.transcript, "```"] : []),
  ].join("\n");
  const queued = await ctx.runMutation(internal.slackGithubIssue.queueIssueCreation, {
    requestId: request._id,
    payload: {
      action: "createIssue",
      requestId: request._id,
      title,
      body,
      repo: mapping ? { repo: mapping.repo, project: mapping.project } : null,
      requester: { slackUserId: request.slackUserId, name: flow.requesterName },
      source: {
        kind: request.kind,
        channelId: request.channelId,
        threadTs: request.threadTs ?? null,
        messageTs: request.messageTs ?? null,
        permalink: flow.sourcePermalink,
      },
    },
  });
  if (!queued.assigned) {
    await flow.note(
      `There's no GitHub issue in this thread, and Ghostex doesn't know your Slack user yet, so no Ghostex can create one. In Ghostex, run \`${await slackLinkCommand(ctx, request)}\` (or paste your member ID and the code into Settings > Workspaces > Your Slack user) within 30 minutes, and it starts right away.`,
    );
  } else if (!queued.online) {
    await flow.note("There's no GitHub issue in this thread. Your Ghostex isn't running; it creates the issue and starts as soon as it's back.");
  }
  return { status: "done", tickets: [{ creatingGithubIssue: true, commandId: queued.commandId }] };
}

function quote(text: string, maxLines: number, maxChars: number): string {
  const clipped = text.length > maxChars ? `${text.slice(0, maxChars - 1)}…` : text;
  return clipped
    .split("\n")
    .slice(0, maxLines)
    .map((line) => `> ${line}`)
    .join("\n");
}

function sourceLine(flow: FlowContext): string {
  const { request } = flow.loaded;
  const from = flow.thread.rootAuthor && flow.thread.rootAuthor !== flow.requesterName ? ` from ${flow.thread.rootAuthor}` : "";
  if (!request.threadTs) return `Source: \`/ghostex\` in <#${request.channelId}> from ${flow.requesterName}`;
  return `Source: ${flow.threadPermalink ? `<${flow.threadPermalink}|thread>` : "thread"} in <#${request.channelId}>${from}`;
}

/**
 * The working thread's opening post: ID and title, the source link, the request and what the thread says, tagging only the requester and the dev/QC owner. `head` and `tags` travel with the start command, so the post can be rewritten with the thread's requirements (slackPost.ts `updateOpeningPost`).
 *
 * CDXC:TeamSync 2026-10-09 DECISION:
 * User: the working thread's top-level post tags only the requester and the dev/QC owner, never the people who reported the bug.
 */
function openingPost(flow: FlowContext, ticket: TicketRef, issue: LinearIssue | null) {
  const { request, settings } = flow.loaded;
  const tags = [request.slackUserId, settings.qcOwnerSlackUserId]
    .filter((id, index, all): id is string => Boolean(id) && all.indexOf(id) === index)
    .map((id) => `<@${id}>`)
    .join(" ");
  const head = [`*${ticketLabel(ticket, issue)}*`, sourceLine(flow)];
  if (request.prompt) head.push(`Request: ${escapeSlack(request.prompt)}`);
  const fromThread = request.threadTs && request.messageTs !== request.threadTs ? flow.thread.rootText : "";
  const body = fromThread ? ["From the thread:", quote(escapeSlack(fromThread), 8, 700)] : [];
  return { head, tags, text: [...head, ...body, tags].join("\n") };
}

function forwardPost(flow: FlowContext, watchOnly: boolean): string {
  const { request } = flow.loaded;
  const where = flow.sourcePermalink ? `<${flow.sourcePermalink}|this message>` : "a message";
  const words = request.prompt || request.text;
  return `<@${request.slackUserId}> in ${where} in <#${request.channelId}>${watchOnly ? " (watch-only)" : ""}:\n${quote(escapeSlack(words), 20, 3000)}`;
}

/** Copies the thread's images into this deployment's file storage, so the requester's Ghostex can fetch them without the bot token. */
async function storeImages(ctx: ActionCtx, images: ThreadContext["images"]) {
  const stored: { name: string; mimetype: string; url: string; messageTs: string }[] = [];
  for (const image of images.slice(-MAX_IMAGES)) {
    if (image.size !== null && image.size > MAX_IMAGE_BYTES) continue;
    try {
      const blob = await downloadFile(image.urlPrivate);
      if (blob.size > MAX_IMAGE_BYTES) continue;
      const storageId = await ctx.storage.store(blob);
      const url = await ctx.storage.getUrl(storageId);
      if (url) stored.push({ name: image.name, mimetype: image.mimetype, url, messageTs: image.messageTs });
    } catch {
      // A file the bot cannot read (no `files:read`, deleted) is left out; the transcript still names it.
    }
  }
  return stored;
}

async function workOnTicket(
  ctx: ActionCtx,
  flow: FlowContext,
  ticket: TicketRef,
  createdTicket: LinearIssue | null,
  watchOnly: boolean,
): Promise<Record<string, unknown>> {
  const { request, settings } = flow.loaded;
  const { readKey } = flow;
  const issue =
    createdTicket?.identifier === ticket.key ? createdTicket : ticket.kind === "linear" && readKey ? await linearIssue(readKey, ticket.key) : null;
  if (ticket.kind === "linear" && readKey && !issue) throw new Error(`Linear has no ticket ${ticket.key}.`);
  const inWorkingThread = flow.loaded.isWorkingThread && flow.loaded.threadTicket === ticket.key;

  let linkPostedAt: number | null = null;
  if (request.threadTs && !inWorkingThread) {
    ({ linkPostedAt } = await ctx.runMutation(internal.slackFlowState.linkThread, {
      teamId: request.teamId,
      channelId: request.channelId,
      threadTs: request.threadTs,
      permalink: flow.threadPermalink ?? undefined,
      ticket: ticket.key,
    }));
  }

  // Step 3: the ticket's working thread.
  let working = await ctx.runQuery(internal.slackFlowState.getWorkingThread, { teamId: request.teamId, ticket: ticket.key });
  let openedWorkingThread = false;
  let opening: ReturnType<typeof openingPost> | null = null;
  if (!working) {
    if (!settings.workingChannelId) {
      throw new Error("this team has no working channel yet. Run `ghostex team flow set --working-channel <channel ID>` in Ghostex.");
    }
    opening = openingPost(flow, ticket, issue);
    const posted = await postMessage({ channel: settings.workingChannelId, text: opening.text });
    const link = await permalink(posted.channel, posted.ts).catch(() => undefined);
    const recorded = await ctx.runMutation(internal.slackFlowState.recordWorkingThread, {
      teamId: request.teamId,
      ticket: ticket.key,
      channelId: posted.channel,
      threadTs: posted.ts,
      permalink: link,
    });
    working = recorded;
    openedWorkingThread = recorded.created;
  }
  const workingLink = working.permalink ? `<${working.permalink}|its working thread>` : "its working thread";

  if (watchOnly) {
    await postMessage({ channel: working.channelId, threadTs: working.threadTs, text: forwardPost(flow, true) });
    if (request.messageTs) await addReaction(request.channelId, request.messageTs, "eyes");
    return { ticket: ticket.key, forwarded: true, openedWorkingThread };
  }

  // The source thread gets the working thread's link once (runFlow posts one reply for every ticket of the request).
  if (request.threadTs && !inWorkingThread && linkPostedAt === null) {
    flow.sourceLinks.push({ label: ticketLabel(ticket, issue), permalink: working.permalink, channelId: working.channelId });
  }
  // A request typed outside the working thread shows up there too (a new working thread already quotes it).
  if (!inWorkingThread && !openedWorkingThread) {
    await postMessage({ channel: working.channelId, threadTs: working.threadTs, text: forwardPost(flow, false) });
  }

  // Step 4: start or reuse the ticket's session.
  const images = await storeImages(ctx, flow.thread.images);
  const runPlace = request.mode ?? settings.defaultRunPlace;
  const mapping = settings.channelRepos.find((entry) => entry.channelId === request.channelId) ?? null;
  const ticketPayload = {
    key: ticket.key,
    kind: ticket.kind,
    title: issue?.title ?? null,
    url: ticketUrl(ticket, issue),
    teamName: issue?.teamName ?? null,
    projectName: issue?.projectName ?? null,
    branchName: issue?.branchName ?? null,
    description: issue?.description ?? null,
    state: issue?.state ?? null,
    labels: issue?.labels ?? [],
    comments: issue?.comments ?? [],
    attachments: issue?.attachments ?? [],
    repo: ticket.kind === "github" ? ticket.repo : null,
    number: ticket.kind === "github" ? ticket.number : null,
    pullRequest: ticket.kind === "github" ? ticket.pullRequest : false,
  };
  const shared = {
    requestId: request._id,
    prompt: request.prompt,
    requester: { slackUserId: request.slackUserId, name: flow.requesterName },
    source: {
      kind: request.kind,
      channelId: request.channelId,
      threadTs: request.threadTs ?? null,
      messageTs: request.messageTs ?? null,
      permalink: flow.sourcePermalink,
    },
    workingThread: { channelId: working.channelId, threadTs: working.threadTs, permalink: working.permalink },
  };
  const queued: QueuedWork = await ctx.runMutation(internal.slackFlowState.queueTicketWork, {
    requestId: request._id,
    ticket: ticket.key,
    runPlace,
    startPayload: {
      action: "start",
      ...shared,
      ticket: ticketPayload,
      runPlace,
      transcript: flow.thread.transcript,
      repo: mapping ? { repo: mapping.repo, project: mapping.project } : null,
      instructions: settings.instructions,
      images,
      createdTicket: createdTicket?.identifier === ticket.key || flow.createdKey === ticket.key,
      openedWorkingThread,
      // What the requester's Ghostex needs to replace the post's quote with the thread's requirements.
      openingPost:
        openedWorkingThread && opening ? { channelId: working.channelId, ts: working.threadTs, head: opening.head, tags: opening.tags } : null,
    },
    messagePayload: {
      action: "message",
      ...shared,
      ticket: { key: ticket.key, kind: ticket.kind, title: issue?.title ?? null, url: ticketUrl(ticket, issue) },
      images: images.filter((image) => image.messageTs === request.messageTs),
    },
  });

  if (queued.action === "cloudStarting") {
    await flow.note(
      `${ticket.key}'s cloud session is still starting, so there's nothing to send your message to yet. Ask again in a minute.`,
    );
  } else if (!queued.assigned) {
    await flow.note(
      `Ghostex doesn't know your Slack user yet, so no Ghostex can run this. In Ghostex, run \`${await slackLinkCommand(ctx, request)}\` (or paste your member ID and the code into Settings > Workspaces > Your Slack user) within 30 minutes, and it starts right away.`,
    );
  } else if (queued.action === "message") {
    if (inWorkingThread) {
      if (request.messageTs) await addReaction(request.channelId, request.messageTs, "eyes");
    } else {
      await flow.note(
        `${ticket.key} already has a working thread, so your message goes to its session instead of starting a new one.${queued.online ? "" : ` ${queued.memberName ?? "Its owner"}'s Ghostex isn't running; it gets the message when it's back.`}`,
        [
          {
            type: "section",
            text: {
              type: "mrkdwn",
              text: `${ticket.key} already has ${workingLink}, so your message goes to its session instead of starting a new one.${queued.online ? "" : ` ${queued.memberName ?? "Its owner"}'s Ghostex isn't running; it gets the message when it's back.`}`,
            },
          },
        ],
      );
    }
  } else if (!queued.online) {
    const what = createdTicket?.identifier === ticket.key || flow.createdKey === ticket.key
      ? `I've created ${ticket.key}${openedWorkingThread ? " and its working thread" : ""}`
      : openedWorkingThread
        ? `I've opened ${ticket.key}'s working thread`
        : `${ticket.key} has its working thread`;
    const text = `Your Ghostex isn't running. ${what}, and I'll start Claude as soon as your Ghostex is back.`;
    await flow.note(text, [
      { type: "section", text: { type: "mrkdwn", text } },
      {
        type: "actions",
        elements: [
          { type: "button", action_id: "gx_offline_ok", text: { type: "plain_text", text: "OK" }, value: queued.commandId },
          { type: "button", action_id: "gx_offline_cancel", text: { type: "plain_text", text: "Cancel" }, value: queued.commandId, style: "danger" },
        ],
      },
    ]);
  }
  return {
    ticket: ticket.key,
    createdTicket: createdTicket?.identifier === ticket.key || flow.createdKey === ticket.key,
    openedWorkingThread,
    action: queued.action,
    commandId: "commandId" in queued ? queued.commandId : null,
  };
}
