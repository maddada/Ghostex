import {
  IconChevronDown,
  IconChevronRight,
  IconCloud,
  IconExternalLink,
  IconFolder,
  IconLink,
  IconMessage,
  IconPlus,
} from "@tabler/icons-react";
import { useEffect, useState, type ReactNode } from "react";
import { Button, Dropdown, MenuItem, Spinner, cx } from "./components";
import type {
  CloudDraft,
  CloudProvider,
  CurrentSession,
  WorkAgent,
  WorkItem,
  WorkItemSession,
  WorkProject,
} from "./types";

export interface StartChatChoice {
  agentId?: string;
  projectId: string;
}

/** Whether the ▾ menu offers Link to current session, and why it cannot when it is greyed. */
export interface LinkOffer {
  session: CurrentSession | null;
  /** Shown greyed with this reason instead of acting. */
  disabledReason?: string;
}

/**
 * The menu row for the session the window has selected: hidden when that session already links
 * this ticket, greyed with the reason when it cannot take it (apps/desktop/src/app/work_view/
 * current_session.rs has the DECISION).
 */
export function linkOffer(
  item: WorkItem,
  current: CurrentSession | null,
): LinkOffer | null {
  if (!current)
    return {
      session: null,
      disabledReason: "Select a session in the sidebar to link it here.",
    };
  const links = current.links;
  const prNumber = item.pullRequest?.number;
  const linked =
    item.kind === "linearIssue"
      ? !!item.linearIssue &&
        links.linearIssues.some(
          (id) => id.toUpperCase() === item.linearIssue?.toUpperCase(),
        )
      : item.kind === "githubIssue"
        ? item.githubIssue != null &&
          links.githubIssues.includes(item.githubIssue)
        : prNumber != null && links.pullRequest?.number === prNumber;
  if (linked) return null;
  if (current.remote)
    return {
      session: current,
      disabledReason: `‘${current.title}’ is on ${current.machineName ?? "another computer"}; only this computer’s sessions can be linked here.`,
    };
  if (!current.workMode)
    return {
      session: current,
      disabledReason: `‘${current.title}’ is in ${current.projectName ?? "a project"}, which has Work mode off.`,
    };
  if (
    item.kind === "pullRequest" &&
    links.pullRequest &&
    links.pullRequest.number !== prNumber
  )
    return {
      session: current,
      disabledReason: `‘${current.title}’ is linked to #${links.pullRequest.number}.`,
    };
  return { session: current };
}

/** A menu row with a second, quieter line. */
function MenuRow({
  icon,
  label,
  sub,
  disabled,
  className,
  trailing,
  onSelect,
}: {
  icon: ReactNode;
  label: ReactNode;
  sub?: ReactNode;
  disabled?: boolean;
  className?: string;
  trailing?: ReactNode;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      className={cx("w-menu-item w-menu-row", className)}
      aria-disabled={disabled || undefined}
      disabled={disabled}
      onClick={onSelect}
    >
      <span className="w-menu-row-icon">{icon}</span>
      <span className="w-menu-row-text">
        <span className="w-menu-row-label">{label}</span>
        {sub ? <span className="w-menu-row-sub">{sub}</span> : null}
      </span>
      {trailing}
    </button>
  );
}

function ProviderIcon({
  provider,
  agents,
}: {
  provider: CloudProvider;
  agents: WorkAgent[];
}) {
  const icon = agents.find(
    (agent) => agent.id === provider.agentId,
  )?.iconDataUrl;
  return icon ? (
    <img className="w-agent-icon" src={icon} alt="" />
  ) : (
    <IconCloud size={14} />
  );
}

/**
 * The ticket's action row: the primary split button (Open chat, Start chat, or Open on GitHub for
 * a PR) with its ▾ menu, and Start in cloud ▾ beside it. Both reach the same clouds.
 *
 * CDXC:WorkMode 2026-10-09 DECISION:
 * User: the ticket's one button is "Open chat" when one of your sessions links it (it selects that
 * session in the sidebar and shows it in the chat column, nothing else moves), otherwise "Start
 * chat" with an arrow menu for the agent, which starts a session in a new worktree on the ticket's
 * branch, linked to it, and sends nothing. The repo comes from the ticket; Ghostex asks only when
 * it cannot tell.
 */
export function StartActions({
  item,
  projects,
  agents,
  providers,
  starting,
  cloudStarting,
  link,
  linking,
  onOpenChat,
  onStartChat,
  onOpenUrl,
  onLink,
  onPickCloud,
}: {
  item: WorkItem;
  projects: WorkProject[];
  agents: WorkAgent[];
  providers: CloudProvider[];
  starting: boolean;
  cloudStarting: boolean;
  link: LinkOffer | null;
  linking: boolean;
  onOpenChat: (session: WorkItemSession) => void;
  onStartChat: (choice: StartChatChoice) => void;
  onOpenUrl: (url: string) => void;
  onLink: () => void;
  onPickCloud: (provider: CloudProvider) => void;
}) {
  const defaultAgent =
    agents.find((agent) => agent.primary)?.id ?? agents[0]?.id;
  const [agentId, setAgentId] = useState<string | undefined>(defaultAgent);
  const [projectId, setProjectId] = useState<string | undefined>(
    item.projectId,
  );
  const [cloudOpen, setCloudOpen] = useState(false);
  useEffect(
    () => setAgentId((current) => current ?? defaultAgent),
    [defaultAgent],
  );
  useEffect(() => setProjectId(item.projectId), [item.projectId]);

  const session =
    item.sessions.find((candidate) => candidate.working) ?? item.sessions[0];
  const canStartChat = !session && item.kind !== "pullRequest";
  const project = projects.find(
    (candidate) => candidate.projectId === projectId,
  );
  const agent = agents.find((candidate) => candidate.id === agentId);
  const startChat = () => projectId && onStartChat({ agentId, projectId });

  const primary = session ? (
    <Button
      variant="primary"
      className="open-chat-btn"
      onClick={() => onOpenChat(session)}
    >
      <IconMessage size={14} />
      Open chat
    </Button>
  ) : canStartChat ? (
    <Button
      variant="primary"
      className="start-chat-btn"
      disabled={starting}
      onClick={startChat}
      title={projectId ? undefined : "Pick the project to work in"}
    >
      {starting ? <Spinner /> : <IconPlus size={14} />}
      {starting ? "Starting…" : "Start chat"}
    </Button>
  ) : (
    <Button
      variant="primary"
      className="open-on-github-btn"
      disabled={!item.url}
      onClick={() => onOpenUrl(item.url ?? "")}
    >
      <IconExternalLink size={14} />
      Open on GitHub
    </Button>
  );

  const linkSub = link?.session
    ? `‘${link.session.title}’ · ${[link.session.projectName, link.session.agentName].filter(Boolean).join(" · ")}`
    : undefined;

  return (
    <>
      <Dropdown
        className="start-chat-split"
        trigger={(open, toggle) => (
          <span className="w-split">
            {primary}
            <Button
              variant="primary"
              className={cx("start-chat-options", open && "is-open")}
              disabled={starting}
              onClick={toggle}
              title="More ways to work on this ticket"
            >
              <IconChevronDown size={14} />
            </Button>
          </span>
        )}
      >
        {(close) => (
          <div className="w-start-menu start-chat-menu">
            {canStartChat ? (
              <MenuRow
                className="menu-start-chat"
                icon={<IconPlus size={14} />}
                label="Start chat"
                sub={
                  item.branchName
                    ? `New worktree on ${item.branchName}`
                    : "New worktree on the ticket’s branch"
                }
                disabled={!projectId || starting}
                onSelect={() => {
                  close();
                  startChat();
                }}
              />
            ) : null}
            {link ? (
              <MenuRow
                className="menu-link-current"
                icon={linking ? <Spinner /> : <IconLink size={14} />}
                label="Link to current session"
                sub={link.disabledReason ?? linkSub}
                disabled={!!link.disabledReason || linking}
                onSelect={() => {
                  close();
                  onLink();
                }}
              />
            ) : null}
            {providers.length > 0 ? (
              <>
                <div className="w-menu-sep" />
                <MenuRow
                  className="menu-start-in-cloud"
                  icon={<IconCloud size={14} />}
                  label="Start in cloud"
                  sub={providers.map((provider) => provider.name).join(", ")}
                  disabled={cloudStarting}
                  trailing={
                    <IconChevronRight
                      size={14}
                      className={cx(
                        "w-menu-row-chevron",
                        cloudOpen && "is-open",
                      )}
                    />
                  }
                  onSelect={() => setCloudOpen((open) => !open)}
                />
                {cloudOpen
                  ? providers.map((provider) => (
                      <MenuRow
                        key={provider.id}
                        className="w-menu-row--nested menu-cloud-provider"
                        icon={
                          <ProviderIcon provider={provider} agents={agents} />
                        }
                        label={provider.name}
                        onSelect={() => {
                          close();
                          onPickCloud(provider);
                        }}
                      />
                    ))
                  : null}
              </>
            ) : null}
            {canStartChat ? (
              <>
                <div className="w-menu-sep" />
                {agents.length > 0 ? (
                  <div className="w-menu-title">Start chat with</div>
                ) : null}
                {agents.map((candidate) => (
                  <MenuItem
                    key={candidate.id}
                    checked={candidate.id === agentId}
                    onSelect={() => setAgentId(candidate.id)}
                  >
                    {candidate.iconDataUrl ? (
                      <img
                        className="w-agent-icon"
                        src={candidate.iconDataUrl}
                        alt=""
                      />
                    ) : null}
                    {candidate.name ?? candidate.id}
                    {candidate.primary ? (
                      <span className="w-menu-hint">default</span>
                    ) : null}
                  </MenuItem>
                ))}
                <div className="w-menu-title">In</div>
                {projects.map((candidate) => (
                  <MenuItem
                    key={candidate.projectId}
                    checked={candidate.projectId === projectId}
                    onSelect={() => setProjectId(candidate.projectId)}
                  >
                    <IconFolder size={13} />
                    {candidate.name}
                    {candidate.projectId === item.projectId ? (
                      <span className="w-menu-hint">from the ticket</span>
                    ) : null}
                  </MenuItem>
                ))}
                <div className="w-menu-note">
                  Starts {agent?.name ?? "the agent"}
                  {project ? ` in ${project.name}` : ""} and links the session
                  to {item.id}. Nothing is sent.
                </div>
              </>
            ) : null}
          </div>
        )}
      </Dropdown>
      {providers.length > 0 ? (
        <Dropdown
          className="start-in-cloud"
          trigger={(open, toggle) => (
            <Button
              className={cx("start-in-cloud-btn", open && "is-open")}
              disabled={cloudStarting}
              onClick={toggle}
            >
              {cloudStarting ? <Spinner /> : <IconCloud size={14} />}
              {cloudStarting ? "Starting…" : "Start in cloud"}
              <IconChevronDown size={13} className="w-faint" />
            </Button>
          )}
        >
          {(close) => (
            <div className="w-start-menu start-in-cloud-menu">
              <div className="w-menu-title">Start in cloud with</div>
              {providers.map((provider) => (
                <MenuRow
                  key={provider.id}
                  className="menu-cloud-provider"
                  icon={<ProviderIcon provider={provider} agents={agents} />}
                  label={provider.name}
                  sub={
                    item.branchName
                      ? `On ${item.branchName}, with a task you can edit`
                      : "With a task you can edit"
                  }
                  onSelect={() => {
                    close();
                    onPickCloud(provider);
                  }}
                />
              ))}
            </div>
          )}
        </Dropdown>
      ) : null}
    </>
  );
}

/**
 * The task a cloud session starts with, drafted by gxserver from the ticket and editable before
 * Start (server/src/work_mode/cloud_work.rs).
 */
export function CloudTaskBox({
  provider,
  draft,
  loading,
  error,
  starting,
  onStart,
  onCancel,
}: {
  provider: CloudProvider;
  draft: CloudDraft | null;
  loading: boolean;
  error: string | null;
  starting: boolean;
  onStart: (prompt: string) => void;
  onCancel: () => void;
}) {
  const [prompt, setPrompt] = useState(draft?.prompt ?? "");
  useEffect(() => setPrompt(draft?.prompt ?? ""), [draft]);
  const tooLong = draft ? prompt.length > draft.maxChars : false;
  return (
    <section className="w-card cloud-task-box">
      <header className="w-card-head">
        <IconCloud size={14} />
        <span className="w-card-title">Start in cloud · {provider.name}</span>
        {draft ? (
          <span className="w-card-sub cloud-task-branch">
            {draft.branchOnRemote
              ? `· works on ${draft.branch}`
              : `· creates ${draft.branch}`}
          </span>
        ) : null}
      </header>
      <div className="w-card-body">
        {loading && !draft ? (
          <div className="w-faint cloud-task-loading">
            <Spinner /> Writing the task from the ticket…
          </div>
        ) : null}
        {draft ? (
          <textarea
            className="w-textarea cloud-task-text"
            value={prompt}
            disabled={starting}
            rows={10}
            spellCheck={false}
            onChange={(event) => setPrompt(event.target.value)}
          />
        ) : null}
        {draft?.warnings.map((warning) => (
          <div key={warning} className="w-notice">
            {warning}
          </div>
        ))}
        {error ? (
          <div className="w-notice is-error cloud-task-error">{error}</div>
        ) : null}
        <div className="cloud-task-actions">
          {draft ? (
            <span className={cx("w-faint", tooLong && "c-closed")}>
              {prompt.length.toLocaleString()} /{" "}
              {draft.maxChars.toLocaleString()}
            </span>
          ) : null}
          <span className="w-spacer" />
          {starting ? (
            <span className="w-faint">This takes up to half a minute.</span>
          ) : null}
          <Button size="sm" disabled={starting} onClick={onCancel}>
            Cancel
          </Button>
          <Button
            size="sm"
            variant="primary"
            className="cloud-task-start"
            disabled={!draft || starting || tooLong || !prompt.trim()}
            onClick={() => onStart(prompt)}
          >
            {starting ? <Spinner size={13} /> : null}
            {starting ? "Starting…" : "Start"}
          </Button>
        </div>
      </div>
    </section>
  );
}
