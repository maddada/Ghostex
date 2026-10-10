import {
  IconArrowLeft,
  IconBox,
  IconCircleCheck,
  IconCircleDashed,
  IconCircleDot,
  IconCircleX,
  IconDots,
  IconExternalLink,
  IconFolder,
  IconGitBranch,
  IconLoader2,
  IconMessage,
  IconMessages,
  IconPaperclip,
  IconRefresh,
  IconVideo,
} from "@tabler/icons-react";
import { useState } from "react";
import { CloudSessionRows } from "./cloud-sessions";
import {
  Avatar,
  Button,
  Card,
  Dropdown,
  LiveDot,
  MenuItem,
  PullRequestIcon,
  Spinner,
  StatusGlyph,
  cx,
} from "./components";
import { formatDuration, relativeTime } from "./format";
import { ClampedText, LinkifiedText, MediaPlayer } from "./rich-text";
import { SlackThreadCards } from "./slack-threads";
import { TeamFlowTracker } from "./team-flow";
import { TeamSessionRows } from "./team-sessions";
import {
  CloudTaskBox,
  StartActions,
  linkOffer,
  type StartChatChoice,
} from "./start-actions";
import type {
  CloudDraft,
  CloudProvider,
  CurrentSession,
  PullRequestDetails,
  WorkAgent,
  WorkComment,
  WorkItem,
  WorkItemDetails,
  WorkItemSession,
} from "./types";

export type { StartChatChoice } from "./start-actions";

/** The Start in cloud box's state, kept by the page (work.tsx). */
export interface CloudBoxState {
  provider: CloudProvider;
  draft: CloudDraft | null;
  loading: boolean;
  error: string | null;
  starting: boolean;
}

const KIND_LABEL: Record<WorkItem["kind"], string> = {
  linearIssue: "Linear issue",
  githubIssue: "GitHub issue",
  pullRequest: "Pull request",
};

export function TicketDetailsView({
  details,
  fallbackItem,
  loading,
  error,
  agents,
  starting,
  startError,
  now,
  currentSession,
  linking,
  cloudBox,
  onBack,
  onRefresh,
  onOpenChat,
  onStartChat,
  onOpenUrl,
  onLinkCurrentSession,
  onPickCloud,
  onStartCloud,
  onCancelCloud,
  onOpenCloudInTerminal,
}: {
  details: WorkItemDetails | null;
  /** The list's row, drawn while the details load. */
  fallbackItem: WorkItem | null;
  loading: boolean;
  error: string | null;
  agents: WorkAgent[];
  starting: boolean;
  startError: string | null;
  now: number;
  currentSession: CurrentSession | null;
  linking: boolean;
  cloudBox: CloudBoxState | null;
  onBack: () => void;
  onRefresh: () => void;
  onOpenChat: (session: WorkItemSession) => void;
  onStartChat: (choice: StartChatChoice) => void;
  onOpenUrl: (url: string) => void;
  onLinkCurrentSession: (session: CurrentSession, item: WorkItem) => void;
  onPickCloud: (provider: CloudProvider) => void;
  onStartCloud: (prompt: string) => void;
  onCancelCloud: () => void;
  onOpenCloudInTerminal: (sessionUrl: string, item: WorkItem) => void;
}) {
  const item = details?.item ?? fallbackItem;
  const linear = details?.linear ?? null;
  const githubIssue = details?.githubIssue ?? null;
  const pullRequest = details?.pullRequest ?? null;
  const sessions = item?.sessions ?? [];
  const teamSessions = details?.team?.sessions ?? [];
  const cloudSessions = details?.cloudSessions ?? [];
  const conversationCount =
    sessions.length + cloudSessions.length + teamSessions.length;
  const link = item ? linkOffer(item, currentSession) : null;
  const ticketUrl = linear?.url ?? githubIssue?.url ?? item?.url;
  const projects = details?.projects ?? [];
  const projectName =
    item?.projectName ??
    projects.find((project) => project.projectId === item?.projectId)?.name;

  return (
    <div className="w-page ticket-details">
      <div className="w-detail-top">
        <button type="button" className="w-back back-to-list" onClick={onBack}>
          <IconArrowLeft size={14} />
          All work
        </button>
        <span className="w-spacer" />
        <Button
          variant="ghost"
          size="icon"
          title="Refresh"
          onClick={onRefresh}
          disabled={loading}
        >
          {loading ? <Spinner /> : <IconRefresh size={15} />}
        </Button>
        <Dropdown
          align="end"
          trigger={(_open, toggle) => (
            <Button variant="ghost" size="icon" title="More" onClick={toggle}>
              <IconDots size={15} />
            </Button>
          )}
        >
          {(close) => (
            <>
              {ticketUrl ? (
                <MenuItem
                  onSelect={() => {
                    void navigator.clipboard
                      ?.writeText(ticketUrl)
                      .catch(() => undefined);
                    close();
                  }}
                >
                  Copy link
                </MenuItem>
              ) : null}
              {ticketUrl ? (
                <MenuItem
                  onSelect={() => {
                    onOpenUrl(ticketUrl);
                    close();
                  }}
                >
                  {item?.kind === "linearIssue"
                    ? "Open in Linear"
                    : "Open on GitHub"}
                </MenuItem>
              ) : null}
              {item?.branchName ? (
                <MenuItem
                  onSelect={() => {
                    void navigator.clipboard
                      ?.writeText(item.branchName ?? "")
                      .catch(() => undefined);
                    close();
                  }}
                >
                  Copy branch
                </MenuItem>
              ) : null}
            </>
          )}
        </Dropdown>
      </div>

      {item ? (
        <header className="w-detail-head">
          <div className="w-detail-kicker">
            <StatusGlyph item={item} />
            <span className="w-id is-strong">{item.id}</span>
            <span>
              · {linear?.stateName ?? item.status.name} ·{" "}
              {KIND_LABEL[item.kind]}
            </span>
          </div>
          <h1 className="w-detail-title">
            {linear?.title ?? githubIssue?.title ?? item.title}
          </h1>
          <div className="w-detail-meta">
            {item.assignee ? (
              <span className="w-meta-chip">
                <Avatar
                  name={item.assignee.name}
                  url={item.assignee.avatarUrl}
                  size={16}
                />
                {item.assignee.isMe ? "You" : item.assignee.name}
              </span>
            ) : null}
            {projectName ? (
              <span className="w-meta-chip">
                <IconFolder size={13} />
                {projectName}
              </span>
            ) : null}
            {item.linearProject ? (
              <span className="w-meta-chip">
                <IconBox size={13} className="c-linear" />
                {item.linearProject.name}
              </span>
            ) : null}
            {item.githubProject ? (
              <span className="w-meta-chip">
                <IconBox size={13} />
                {item.githubProject.name}
              </span>
            ) : null}
            {item.cycle ? (
              <span className="w-meta-chip">{item.cycle}</span>
            ) : null}
            {(linear?.labels ?? item.labels).map((label) => (
              <span key={label} className="w-meta-chip">
                {label}
              </span>
            ))}
          </div>
        </header>
      ) : null}

      {item ? (
        <div className="w-detail-actions">
          <StartActions
            item={item}
            projects={projects}
            agents={agents}
            providers={details?.cloudProviders ?? []}
            starting={starting}
            cloudStarting={cloudBox?.starting ?? false}
            link={link}
            linking={linking}
            onOpenChat={onOpenChat}
            onStartChat={onStartChat}
            onOpenUrl={onOpenUrl}
            onLink={() =>
              link?.session && !link.disabledReason
                ? onLinkCurrentSession(link.session, item)
                : undefined
            }
            onPickCloud={onPickCloud}
          />
          {ticketUrl && item.kind !== "pullRequest" ? (
            <Button onClick={() => onOpenUrl(ticketUrl)}>
              <IconExternalLink size={14} />
              {item.kind === "linearIssue" ? "Linear" : "GitHub"}
            </Button>
          ) : null}
          {item.pullRequest?.url && item.kind !== "pullRequest" ? (
            <Button onClick={() => onOpenUrl(item.pullRequest?.url ?? "")}>
              <PullRequestIcon state={item.pullRequest.state} size={14} />#
              {item.pullRequest.number}
            </Button>
          ) : null}
          <span className="w-spacer" />
          <span className="w-hint">
            {sessions.length > 0
              ? `In your sidebar${projectName ? ` · ${projectName}` : ""}`
              : "Not in your sidebar"}
          </span>
        </div>
      ) : null}

      {cloudBox ? (
        <CloudTaskBox
          provider={cloudBox.provider}
          draft={cloudBox.draft}
          loading={cloudBox.loading}
          error={cloudBox.error}
          starting={cloudBox.starting}
          onStart={onStartCloud}
          onCancel={onCancelCloud}
        />
      ) : null}

      {startError ? (
        <div className="w-notice is-error">{startError}</div>
      ) : null}
      {error ? <div className="w-notice is-error">{error}</div> : null}
      {details?.errors.map((message) => (
        <div key={message} className="w-notice">
          {message}
        </div>
      ))}

      {details ? (
        <TeamFlowTracker steps={details.teamFlow.steps} onOpenUrl={onOpenUrl} />
      ) : loading ? (
        <div className="w-skel w-skel--flow" />
      ) : null}

      {linear ? (
        <Card
          className="linear-issue-card"
          icon={<IconCircleDot size={14} className="c-linear" />}
          title="Linear issue"
          sub={`· ${linear.commentCount} comment${linear.commentCount === 1 ? "" : "s"}`}
          action={
            linear.url ? (
              <button
                type="button"
                className="w-ext"
                onClick={() => onOpenUrl(linear.url ?? "")}
              >
                <IconExternalLink size={12} />
                Open in Linear
              </button>
            ) : null
          }
        >
          <div className="w-card-body">
            <ClampedText
              text={linear.description ?? ""}
              onOpenUrl={onOpenUrl}
            />
            <Comments
              comments={linear.comments}
              now={now}
              onOpenUrl={onOpenUrl}
            />
          </div>
        </Card>
      ) : null}

      {githubIssue ? (
        <Card
          className="github-issue-card"
          icon={<IconCircleDot size={14} className="c-open" />}
          title={`Issue #${githubIssue.number}`}
          sub={`· ${githubIssue.commentCount} comment${githubIssue.commentCount === 1 ? "" : "s"}`}
          action={
            githubIssue.url ? (
              <button
                type="button"
                className="w-ext"
                onClick={() => onOpenUrl(githubIssue.url ?? "")}
              >
                <IconExternalLink size={12} />
                Open on GitHub
              </button>
            ) : null
          }
        >
          <div className="w-card-body">
            <ClampedText text={githubIssue.body ?? ""} onOpenUrl={onOpenUrl} />
            <Comments
              comments={githubIssue.comments}
              now={now}
              onOpenUrl={onOpenUrl}
            />
          </div>
        </Card>
      ) : null}

      {details?.team && details.team.threads.length > 0 ? (
        <SlackThreadCards
          threads={details.team.threads}
          now={now}
          onOpenUrl={onOpenUrl}
        />
      ) : null}

      {pullRequest ? (
        <PullRequestCard pullRequest={pullRequest} onOpenUrl={onOpenUrl} />
      ) : null}

      {details && details.media.length > 0 ? (
        <Card
          className="media-card"
          icon={<IconVideo size={14} />}
          title="Videos"
          sub={`· ${details.media.length}`}
        >
          <div className="w-card-body w-media-list">
            {details.media.map((media) => (
              <figure key={media.embedUrl} className="w-media">
                <MediaPlayer media={media} />
                <figcaption>
                  <button
                    type="button"
                    className="w-link-btn"
                    onClick={() => onOpenUrl(media.url)}
                  >
                    {media.url.replace(/^https?:\/\/(www\.)?/u, "")}
                  </button>
                </figcaption>
              </figure>
            ))}
          </div>
        </Card>
      ) : null}

      {item ? (
        <Card
          className="conversations-card"
          icon={<IconMessages size={14} className="c-claude" />}
          title="Conversations"
          sub={conversationCount > 0 ? `· ${conversationCount}` : "· none yet"}
        >
          {conversationCount > 0
            ? sessions.map((session) => (
                <div
                  key={`${session.projectId}:${session.sessionId}`}
                  className="w-convo"
                >
                  <span className="w-convo-icon">
                    <IconMessage size={13} />
                  </span>
                  <div className="w-convo-main">
                    <div className="w-convo-who">
                      {session.title}
                      <span className="w-faint">you · in your sidebar</span>
                    </div>
                    <div className="w-convo-sub">
                      {session.working
                        ? "Working right now"
                        : session.lifecycle === "sleeping"
                          ? "Sleeping"
                          : "Idle"}
                    </div>
                  </div>
                  <div className="w-convo-right">
                    {session.working ? <LiveDot /> : null}
                    <Button size="sm" onClick={() => onOpenChat(session)}>
                      Open
                    </Button>
                  </div>
                </div>
              ))
            : null}
          {cloudSessions.length > 0 ? (
            <CloudSessionRows
              sessions={cloudSessions}
              now={now}
              onOpenUrl={onOpenUrl}
              onOpenInTerminal={(url) => onOpenCloudInTerminal(url, item)}
            />
          ) : null}
          {teamSessions.length > 0 ? (
            <TeamSessionRows
              sessions={teamSessions}
              now={now}
              onOpenUrl={onOpenUrl}
              onOpenInTerminal={(url) => onOpenCloudInTerminal(url, item)}
            />
          ) : null}
          {conversationCount === 0 ? (
            <div className="w-card-body w-faint">
              Nobody has a session on this ticket yet.
            </div>
          ) : null}
        </Card>
      ) : null}

      {details && details.links.length > 0 ? (
        <Card
          className="files-card"
          icon={<IconPaperclip size={14} />}
          title="Files and links"
          sub={`· ${details.links.length}`}
        >
          <div className="w-card-body">
            {details.links.map((link) => (
              <button
                key={link.url}
                type="button"
                className="w-file-row"
                onClick={() => onOpenUrl(link.url)}
              >
                <IconPaperclip size={13} />
                <span className="w-file-name">{link.title}</span>
                {link.subtitle ? (
                  <span className="w-faint">{link.subtitle}</span>
                ) : null}
              </button>
            ))}
          </div>
        </Card>
      ) : null}

      {!item && loading ? (
        <div className="w-empty">Loading the ticket…</div>
      ) : null}
    </div>
  );
}

function Comments({
  comments,
  now,
  onOpenUrl,
}: {
  comments: WorkComment[];
  now: number;
  onOpenUrl: (url: string) => void;
}) {
  const [all, setAll] = useState(false);
  if (comments.length === 0) return null;
  const shown = all ? comments : comments.slice(-3);
  return (
    <div className="w-comments">
      {comments.length > shown.length ? (
        <button
          type="button"
          className="w-link-btn"
          onClick={() => setAll(true)}
        >
          Show {comments.length - shown.length} earlier comment
          {comments.length - shown.length === 1 ? "" : "s"}
        </button>
      ) : null}
      {shown.map((comment, index) => (
        <div key={`${comment.createdAt ?? ""}-${index}`} className="w-comment">
          <Avatar name={comment.author} url={comment.avatarUrl} size={20} />
          <div className="w-comment-main">
            <div className="w-comment-who">
              {comment.author ?? "Someone"}
              <span className="w-faint">
                {relativeTime(comment.createdAt, now)}
              </span>
            </div>
            <div className="w-comment-text">
              <LinkifiedText text={comment.body} onOpenUrl={onOpenUrl} />
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

function PullRequestCard({
  pullRequest,
  onOpenUrl,
}: {
  pullRequest: PullRequestDetails;
  onOpenUrl: (url: string) => void;
}) {
  const summary = pullRequest.checksSummary;
  const pill =
    pullRequest.state === "merged"
      ? { cls: "is-accent", text: "Merged" }
      : pullRequest.state === "closed"
        ? { cls: "is-bad", text: "Closed" }
        : pullRequest.state === "draft"
          ? { cls: "is-muted", text: "Draft" }
          : { cls: "is-ok", text: "Open" };
  const reviews = pullRequest.reviews;
  const reviewParts = [
    reviews.approved
      ? `${reviews.approved} approval${reviews.approved === 1 ? "" : "s"}`
      : null,
    reviews.changesRequested
      ? `${reviews.changesRequested} asking for changes`
      : null,
    pullRequest.unresolvedReviewThreads != null
      ? `${pullRequest.unresolvedReviewThreads} open comment${pullRequest.unresolvedReviewThreads === 1 ? "" : "s"}`
      : null,
  ].filter(Boolean);
  return (
    <Card
      className="pr-card"
      icon={<PullRequestIcon state={pullRequest.state} size={14} />}
      title={`#${pullRequest.number} ${pullRequest.title ?? ""}`}
      action={
        <button
          type="button"
          className="w-ext"
          onClick={() => onOpenUrl(pullRequest.url)}
        >
          <IconExternalLink size={12} />
          GitHub
        </button>
      }
    >
      <div className="w-card-body">
        <div className="w-kv">
          <span className="w-k">Status</span>
          <span className="w-v">
            <span className={cx("w-pill", pill.cls)}>{pill.text}</span>
            {pullRequest.reviewDecision === "APPROVED"
              ? "approved"
              : pullRequest.reviewDecision === "CHANGES_REQUESTED"
                ? "changes requested"
                : pullRequest.state === "open"
                  ? "ready for review"
                  : ""}
          </span>
          <span className="w-k">Checks</span>
          <span className="w-v">
            {summary.total === 0 ? (
              "No checks"
            ) : summary.failed > 0 ? (
              <>
                <IconCircleX size={13} className="c-closed" />
                {summary.failed} of {summary.total} failing
              </>
            ) : summary.pending > 0 ? (
              <>
                <IconLoader2 size={13} className="c-pending" />
                {summary.passed} of {summary.total} passed, {summary.pending}{" "}
                running
              </>
            ) : (
              <>
                <IconCircleCheck size={13} className="c-open" />
                {summary.passed} of {summary.total} passed
              </>
            )}
          </span>
          <span className="w-k">Reviews</span>
          <span className="w-v">
            {reviewParts.length > 0
              ? reviewParts.join(" · ")
              : "No reviews yet"}
          </span>
          {pullRequest.headBranch ? (
            <>
              <span className="w-k">Branch</span>
              <span className="w-v w-mono">
                <IconGitBranch size={13} />
                {pullRequest.headBranch}
              </span>
            </>
          ) : null}
        </div>
        {pullRequest.checks.length > 0 ? (
          <div className="w-checks pr-checks">
            {pullRequest.checks.map((check, index) => (
              <button
                key={`${check.name}-${index}`}
                type="button"
                className="w-check-row"
                onClick={() => (check.url ? onOpenUrl(check.url) : undefined)}
                disabled={!check.url}
              >
                {check.status === "passed" ? (
                  <IconCircleCheck size={13} className="c-open" />
                ) : check.status === "failed" ? (
                  <IconCircleX size={13} className="c-closed" />
                ) : check.status === "skipped" ? (
                  <IconCircleDashed size={13} className="w-faint" />
                ) : (
                  <IconLoader2 size={13} className="c-pending" />
                )}
                <span className="w-check-name">{check.name}</span>
                {check.workflow ? (
                  <span className="w-faint">{check.workflow}</span>
                ) : null}
                <span className="w-check-time">
                  {formatDuration(check.durationSeconds)}
                </span>
              </button>
            ))}
          </div>
        ) : null}
      </div>
    </Card>
  );
}
