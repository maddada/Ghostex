/**
 * Sample answers for the page's dev mode (`work.html?fixtures=1`), shaped exactly like gxserver's
 * and the desktop bridge's, so the page can be drawn and screenshotted without the app. Loaded
 * only in that mode.
 */
import type {
  WorkItem,
  WorkItemDetails,
  WorkList,
  WorkReady,
  WorkTeamDetails,
} from "./types";

const minutesAgo = (minutes: number) =>
  new Date(Date.now() - minutes * 60_000).toISOString();

/** Yesterday at 15:00 local time, so the Updated grouping always has a Yesterday group. */
const yesterdayAfternoon = () => {
  const date = new Date();
  date.setDate(date.getDate() - 1);
  date.setHours(15, 0, 0, 0);
  return date.toISOString();
};

const linearUrl = (identifier: string) =>
  `https://linear.app/shortpoint/issue/${identifier}`;
const pullUrl = (repo: string, number: number) =>
  `https://github.com/${repo}/pull/${number}`;
const linearProject = (name: string, slugId: string) => ({
  name,
  url: `https://linear.app/shortpoint/project/${name.toLowerCase().replace(/\s+/gu, "-")}-${slugId}`,
});
const githubProject = (name: string, number: number) => ({
  name,
  url: `https://github.com/orgs/acme/projects/${number}`,
});

const session = (title: string, working = false) => ({
  projectId: "p-shortpoint",
  sessionId: `s-${title}`,
  title,
  working,
  lifecycle: "running",
  agentId: "claude",
});

const base: Pick<
  WorkItem,
  "labels" | "noTicket" | "sessions" | "assignedToMe"
> = {
  labels: [],
  noTicket: false,
  sessions: [],
  assignedToMe: false,
};

const ITEMS: WorkItem[] = [
  {
    ...base,
    key: "linear:SPX-1250",
    kind: "linearIssue",
    id: "SPX-1250",
    url: linearUrl("SPX-1250"),
    title: "EasyPass share dialog ignores dark theme",
    updatedAt: minutesAgo(0),
    status: { group: "progress", name: "In Progress" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    linearProject: linearProject("EasyPass", "8f2c1a9e04b7"),
    assignee: { name: "Sami", isMe: false },
    pullRequest: {
      number: 6555,
      state: "draft",
      checks: "pending",
      url: pullUrl("shortpoint/shortpoint", 6555),
    },
    linearIssue: "SPX-1250",
    slackThreadCount: 1,
  },
  {
    ...base,
    key: "linear:SPX-1241",
    kind: "linearIssue",
    id: "SPX-1241",
    url: linearUrl("SPX-1241"),
    title: "Table element loses column widths after paste",
    updatedAt: minutesAgo(1),
    status: { group: "progress", name: "In Progress" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    linearProject: linearProject("Table element", "3d91e6b2c5a0"),
    assignee: { name: "Yahia", isMe: true },
    assignedToMe: true,
    pullRequest: {
      number: 6551,
      state: "draft",
      checks: "pending",
      url: pullUrl("shortpoint/shortpoint", 6551),
    },
    sessions: [session("table-paste-widths", true)],
    linearIssue: "SPX-1241",
    slackThreadCount: 3,
  },
  {
    ...base,
    key: "linear:SPX-1234",
    kind: "linearIssue",
    id: "SPX-1234",
    url: linearUrl("SPX-1234"),
    title: "EasyPass Live mode disappears when a table is on the page",
    updatedAt: minutesAgo(12),
    status: { group: "review", name: "In Review" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    linearProject: linearProject("EasyPass", "8f2c1a9e04b7"),
    cycle: "Sprint 20",
    labels: ["Bug"],
    assignee: { name: "Yahia", isMe: true },
    assignedToMe: true,
    pullRequest: {
      number: 6538,
      state: "open",
      checks: "passing",
      url: "https://github.com/shortpoint/shortpoint/pull/6538",
    },
    sessions: [session("live-mode-table")],
    branchName: "yahia/spx-1234-live-mode-table",
    linearIssue: "SPX-1234",
    slackThreadCount: 2,
  },
  {
    ...base,
    key: "linear:SPX-1238",
    kind: "linearIssue",
    id: "SPX-1238",
    url: linearUrl("SPX-1238"),
    title: "Sign-up form accepts emails without a domain ending",
    updatedAt: minutesAgo(25),
    status: { group: "review", name: "In Review" },
    projectId: "p-website",
    projectName: "shortpoint-website",
    linearProject: linearProject("Website sign-up", "b07e4f1d9a23"),
    assignee: { name: "Yahia", isMe: true },
    assignedToMe: true,
    pullRequest: {
      number: 212,
      state: "open",
      checks: "failing",
      url: pullUrl("shortpoint/shortpoint-website", 212),
    },
    sessions: [{ ...session("signup-email-domain"), projectId: "p-website" }],
    linearIssue: "SPX-1238",
    slackThreadCount: 1,
  },
  {
    ...base,
    key: "pr:shortpoint/shortpoint#6552",
    kind: "pullRequest",
    id: "#6552",
    url: pullUrl("shortpoint/shortpoint", 6552),
    title: "Speed up table render tests",
    updatedAt: minutesAgo(120),
    status: { group: "open", name: "Open" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    assignee: { name: "yahia", isMe: true },
    assignedToMe: true,
    pullRequest: {
      number: 6552,
      state: "open",
      checks: "passing",
      url: pullUrl("shortpoint/shortpoint", 6552),
    },
    noTicket: true,
    pullRequestRef: "https://github.com/shortpoint/shortpoint/pull/6552",
  },
  {
    ...base,
    key: "linear:SPX-1239",
    kind: "linearIssue",
    id: "SPX-1239",
    url: linearUrl("SPX-1239"),
    title: "EasyPass token refresh fails after 24 hours",
    updatedAt: minutesAgo(180),
    status: { group: "review", name: "QA" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    linearProject: linearProject("EasyPass", "8f2c1a9e04b7"),
    assignee: { name: "Lina", isMe: false },
    pullRequest: {
      number: 6544,
      state: "open",
      checks: "passing",
      url: pullUrl("shortpoint/shortpoint", 6544),
    },
    linearIssue: "SPX-1239",
    slackThreadCount: 2,
  },
  {
    ...base,
    key: "issue:shortpoint/shortpoint-website#218",
    kind: "githubIssue",
    id: "#218",
    url: "https://github.com/shortpoint/shortpoint-website/issues/218",
    title: "Sign-up page: Arabic text overflows the plan cards",
    updatedAt: minutesAgo(240),
    status: { group: "todo", name: "Open" },
    projectId: "p-website",
    projectName: "shortpoint-website",
    assignee: { name: "yahia", isMe: true },
    assignedToMe: true,
    githubIssue: 218,
  },
  {
    ...base,
    key: "linear:SPX-1245",
    kind: "linearIssue",
    id: "SPX-1245",
    url: linearUrl("SPX-1245"),
    title: "Add “Copy link” to the EasyPass share menu",
    updatedAt: yesterdayAfternoon(),
    status: { group: "todo", name: "Todo" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    linearProject: linearProject("EasyPass", "8f2c1a9e04b7"),
    cycle: "Sprint 20",
    labels: ["Feature"],
    assignee: { name: "Yahia", isMe: true },
    assignedToMe: true,
    branchName: "yahia/spx-1245-copy-link",
    linearIssue: "SPX-1245",
    slackThreadCount: 1,
  },
  {
    ...base,
    key: "linear:SPX-1236",
    kind: "linearIssue",
    id: "SPX-1236",
    url: linearUrl("SPX-1236"),
    title: "Image alt text is lost when an element is duplicated",
    updatedAt: minutesAgo(60 * 24 * 3),
    status: { group: "review", name: "QA" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    linearProject: linearProject("Table element", "3d91e6b2c5a0"),
    assignee: { name: "Yahia", isMe: true },
    assignedToMe: true,
    pullRequest: {
      number: 6530,
      state: "merged",
      checks: "passing",
      url: pullUrl("shortpoint/shortpoint", 6530),
    },
    linearIssue: "SPX-1236",
  },
  {
    ...base,
    key: "linear:SPX-1247",
    kind: "linearIssue",
    id: "SPX-1247",
    url: linearUrl("SPX-1247"),
    title: "Pricing page FAQ skips the last answer with the keyboard",
    updatedAt: minutesAgo(60 * 24 * 10),
    status: { group: "progress", name: "In Progress" },
    projectId: "p-website",
    projectName: "shortpoint-website",
    linearProject: linearProject("Website sign-up", "b07e4f1d9a23"),
    assignee: { name: "Yahia", isMe: true },
    assignedToMe: true,
    pullRequest: {
      number: 219,
      state: "open",
      checks: "pending",
      url: pullUrl("shortpoint/shortpoint-website", 219),
    },
    linearIssue: "SPX-1247",
  },
  {
    ...base,
    key: "linear:SPX-1252",
    kind: "linearIssue",
    id: "SPX-1252",
    url: linearUrl("SPX-1252"),
    title: "Triage: EasyPass emails land in spam for Outlook users",
    updatedAt: minutesAgo(60 * 30),
    status: { group: "backlog", name: "Triage" },
    linearProject: linearProject("EasyPass", "8f2c1a9e04b7"),
    linearIssue: "SPX-1252",
  },
];

const LIST: WorkList = {
  items: ITEMS,
  projects: [
    {
      projectId: "p-shortpoint",
      name: "shortpoint",
      repo: "shortpoint/shortpoint",
    },
    {
      projectId: "p-website",
      name: "shortpoint-website",
      repo: "shortpoint/shortpoint-website",
    },
  ],
  viewer: { githubLogin: "yahia" },
  linearConfigured: true,
  tracker: "linear",
  ghAvailable: true,
  errors: [],
  generatedAt: new Date().toISOString(),
  refreshing: false,
};

/** A GitHub workspace (`?fixtures=1&tracker=github`): GitHub issues with their GitHub Project. */
const GITHUB_ITEMS: WorkItem[] = [
  {
    ...base,
    key: "issue:acme/web#218",
    kind: "githubIssue",
    id: "#218",
    url: "https://github.com/acme/web/issues/218",
    title: "Arabic plan cards overflow on narrow screens",
    updatedAt: minutesAgo(2),
    status: { group: "progress", name: "In progress" },
    projectId: "p-web",
    projectName: "web",
    githubProject: githubProject("Q4 Launch", 3),
    assignee: { name: "yahia", isMe: true },
    assignedToMe: true,
    pullRequest: {
      number: 231,
      state: "open",
      checks: "passing",
      url: pullUrl("acme/web", 231),
    },
    githubIssue: 218,
    sessions: [session("arabic-plan-cards", true)],
  },
  {
    ...base,
    key: "issue:acme/web#224",
    kind: "githubIssue",
    id: "#224",
    url: "https://github.com/acme/web/issues/224",
    title: "Copy link in the share menu",
    updatedAt: minutesAgo(40),
    status: { group: "todo", name: "Todo" },
    projectId: "p-web",
    projectName: "web",
    githubProject: githubProject("Q4 Launch", 3),
    assignee: { name: "yahia", isMe: true },
    assignedToMe: true,
    githubIssue: 224,
  },
  {
    ...base,
    key: "issue:acme/api#88",
    kind: "githubIssue",
    id: "#88",
    url: "https://github.com/acme/api/issues/88",
    title: "Rate-limit the export endpoint",
    updatedAt: minutesAgo(180),
    status: { group: "review", name: "In review" },
    projectId: "p-api",
    projectName: "api",
    githubProject: githubProject("Platform", 5),
    assignee: { name: "yahia", isMe: true },
    assignedToMe: true,
    githubIssue: 88,
  },
  {
    ...base,
    key: "pr:acme/api#91",
    kind: "pullRequest",
    id: "#91",
    url: pullUrl("acme/api", 91),
    title: "Bump the SDK",
    updatedAt: minutesAgo(300),
    status: { group: "open", name: "Open" },
    projectId: "p-api",
    projectName: "api",
    assignee: { name: "yahia", isMe: true },
    assignedToMe: true,
    pullRequest: {
      number: 91,
      state: "open",
      checks: "pending",
      url: pullUrl("acme/api", 91),
    },
    noTicket: true,
    pullRequestRef: "91",
  },
];

const GITHUB_LIST: WorkList = {
  items: GITHUB_ITEMS,
  projects: [
    { projectId: "p-web", name: "web", repo: "acme/web" },
    { projectId: "p-api", name: "api", repo: "acme/api" },
  ],
  viewer: { githubLogin: "yahia" },
  linearConfigured: true,
  tracker: "github",
  githubProjects: {
    access: "missingScope",
    command: "gh auth refresh -s read:project",
    noticeDismissed: false,
  },
  ghAvailable: true,
  errors: [],
  generatedAt: new Date().toISOString(),
  refreshing: false,
};

const fixtureTracker = () =>
  new URLSearchParams(location.search).get("tracker") === "github"
    ? "github"
    : "linear";

const READY: WorkReady = {
  projectIds: ["p-shortpoint", "p-website"],
  agents: [
    { id: "claude", name: "Claude", primary: true },
    { id: "codex", name: "Codex", primary: false },
  ],
  pendingOpen: null,
};

function details(item: WorkItem): WorkItemDetails {
  const isLinear = item.kind === "linearIssue";
  return {
    item,
    linear: isLinear
      ? {
          identifier: item.id,
          title: item.title,
          url: `https://linear.app/shortpoint/issue/${item.id.toLowerCase()}`,
          description:
            item.id === "SPX-1234"
              ? "Live mode disappears as soon as a Table element is on the page. Happens in the sandbox and on 9.199.\n\nSteps: add a Table, switch to Live mode, refresh.\n\nRepro: https://www.loom.com/share/5bbdeb480ba84e65b1b3de8c190e2003"
              : "People want to copy a share link without opening the share dialog. Add “Copy link” to the share menu, and show a short “Link copied” toast.",
          stateName: item.status.name,
          stateType: item.status.group === "todo" ? "unstarted" : "started",
          cycle: item.cycle,
          labels: item.labels,
          comments: [
            {
              author: "Rana",
              body: "Confirmed on 9.199.0.402, in Edge and Chrome.",
              createdAt: minutesAgo(300),
            },
            {
              author: "Omar",
              body: "It comes back if you switch tabs.",
              createdAt: minutesAgo(200),
            },
          ],
          commentCount: 2,
          attachments: [],
        }
      : null,
    githubIssue:
      item.kind === "githubIssue"
        ? {
            number: item.githubIssue ?? 0,
            title: item.title,
            url: "https://github.com/shortpoint/shortpoint-website/issues/218",
            state: "open",
            body: "On the Arabic sign-up page the plan names overflow their cards at 1280px.",
            comments: [],
            commentCount: 0,
          }
        : null,
    pullRequest: item.pullRequest
      ? {
          number: item.pullRequest.number,
          title:
            item.id === "SPX-1234"
              ? "Fix Live mode unmounting with a Table element"
              : (item.pullRequest.title ?? item.title),
          url:
            item.pullRequest.url ??
            `https://github.com/shortpoint/shortpoint/pull/${item.pullRequest.number}`,
          state: item.pullRequest.state,
          headBranch: item.branchName ?? null,
          reviewDecision:
            item.pullRequest.checks === "passing" ? "APPROVED" : null,
          reviews: {
            approved: item.pullRequest.checks === "passing" ? 1 : 0,
            changesRequested: 0,
            commented: 1,
          },
          unresolvedReviewThreads:
            item.pullRequest.checks === "failing" ? 2 : 0,
          labels: [],
          checks: [
            {
              name: "build-spfx",
              workflow: "CI",
              status: "passed",
              durationSeconds: 372,
            },
            {
              name: "unit-tests",
              workflow: "CI",
              status:
                item.pullRequest.checks === "failing" ? "failed" : "passed",
              durationSeconds: 220,
            },
            {
              name: "e2e-live-mode",
              workflow: "E2E",
              status:
                item.pullRequest.checks === "pending" ? "pending" : "passed",
              durationSeconds: 483,
            },
            {
              name: "Greptile review",
              status: "passed",
              durationSeconds: null,
            },
          ],
          checksSummary:
            item.pullRequest.checks === "failing"
              ? { total: 4, passed: 3, failed: 1, pending: 0, skipped: 0 }
              : item.pullRequest.checks === "pending"
                ? { total: 4, passed: 3, failed: 0, pending: 1, skipped: 0 }
                : { total: 4, passed: 4, failed: 0, pending: 0, skipped: 0 },
        }
      : null,
    media:
      item.id === "SPX-1234"
        ? [
            {
              kind: "loom",
              url: "https://www.loom.com/share/5bbdeb480ba84e65b1b3de8c190e2003",
              embedUrl:
                "https://www.loom.com/embed/5bbdeb480ba84e65b1b3de8c190e2003",
            },
          ]
        : [],
    links:
      item.id === "SPX-1234"
        ? [
            {
              title: "root-cause.html",
              subtitle: "Uploaded by Yahia",
              url: "https://example.com/root-cause.html",
            },
          ]
        : [],
    teamFlow: {
      source: "builtIn",
      steps:
        item.id === "SPX-1234"
          ? [
              {
                id: "ticket",
                label: "Ticket",
                status: "done",
                detail: "SPX-1234",
              },
              {
                id: "working-thread",
                label: "Working thread",
                status: "done",
                detail: "#kevin-the-bot",
                url: "https://shortpoint.slack.com/archives/C0KEVIN/p1791500100000100",
              },
              {
                id: "session",
                label: "Session",
                status: "done",
                detail: "3 sessions",
              },
              { id: "pr", label: "PR", status: "done", detail: "#6538" },
              {
                id: "review-comments",
                label: "Review comments",
                status: "done",
                detail: "0 open",
              },
              { id: "ci", label: "CI", status: "done", detail: "4 / 4" },
              {
                id: "video",
                label: "Video",
                status: "unknown",
                detail: "not recorded",
              },
              {
                id: "qc-package",
                label: "QC package",
                status: "current",
                detail: "no READY-FOR-QC",
              },
              {
                id: "validation",
                label: "Validation",
                status: "pending",
                detail: "not posted",
              },
            ]
          : [
              {
                id: "ticket",
                label: "Ticket",
                status: "done",
                detail: item.id,
              },
              {
                id: "working-thread",
                label: "Working thread",
                status: "unknown",
                detail: "not connected",
              },
              {
                id: "session",
                label: "Session",
                status: item.sessions.length ? "done" : "current",
                detail: item.sessions.length ? "1 session" : "none yet",
              },
              {
                id: "pr",
                label: "PR",
                status: "pending",
                detail: "not opened",
              },
              {
                id: "review-comments",
                label: "Review comments",
                status: "pending",
                detail: "after PR",
              },
              { id: "ci", label: "CI", status: "pending", detail: "after PR" },
              {
                id: "video",
                label: "Video",
                status: "unknown",
                detail: "not recorded",
              },
              {
                id: "qc-package",
                label: "QC package",
                status: "pending",
                detail: "after PR",
              },
              {
                id: "validation",
                label: "Validation",
                status: "unknown",
                detail: "not connected",
              },
            ],
    },
    team: item.id === "SPX-1234" ? TEAM_SPX_1234 : null,
    projects: LIST.projects,
    errors: [],
  };
}

const msAgo = (minutes: number) => Date.now() - minutes * 60_000;

const TEAM_SPX_1234: WorkTeamDetails = {
  connected: true,
  teamName: "ShortPoint",
  ticket: "SPX-1234",
  threads: [
    {
      id: "t-working",
      channelId: "C0KEVIN",
      channelName: "kevin-the-bot",
      threadTs: "1791500100.000100",
      permalink:
        "https://shortpoint.slack.com/archives/C0KEVIN/p1791500100000100",
      role: "working",
      isWorkingThread: true,
      replyCount: 9,
      hiddenReplyCount: 6,
      lastMessageAt: msAgo(12),
      messages: [
        {
          ts: "1",
          botId: "B01",
          authorName: "Ghostex",
          isApp: true,
          text: "*SPX-1234* · EasyPass Live mode disappears when a table is on the page\nRequested by @Yahia in #bugs. QC: @Rana",
          files: [],
          postedAt: msAgo(300),
          media: [],
        },
        {
          ts: "2",
          botId: "B01",
          authorName: "Claude · SPX-1234",
          isApp: true,
          text: "*2a* Root cause: the Table element remounts the page canvas on first layout, which drops Live mode. Fix in <https://github.com/shortpoint/shortpoint/pull/6538|#6538>.",
          files: [],
          postedAt: msAgo(60),
          media: [],
        },
        {
          ts: "3",
          botId: "B01",
          authorName: "Claude · SPX-1234",
          isApp: true,
          text: "*3a* Before/after video below. OK to build READY-FOR-QC and post the validation request?",
          files: [
            {
              id: "F1",
              name: "before-after.mp4",
              mimetype: "video/mp4",
              permalink: "https://shortpoint.slack.com/files/F1",
            },
          ],
          postedAt: msAgo(20),
          media: [],
        },
        {
          ts: "4",
          userId: "U0YAHIA",
          authorName: "Yahia",
          isApp: false,
          text: "approved, go ahead",
          files: [],
          postedAt: msAgo(12),
          media: [],
        },
      ],
    },
    {
      id: "t-source",
      channelId: "C0BUGS",
      channelName: "bugs",
      threadTs: "1791500000.000200",
      permalink:
        "https://shortpoint.slack.com/archives/C0BUGS/p1791500000000200",
      role: "source",
      isWorkingThread: false,
      replyCount: 3,
      hiddenReplyCount: 0,
      lastMessageAt: msAgo(290),
      messages: [
        {
          ts: "10",
          userId: "U0RANA",
          authorName: "Rana",
          isApp: false,
          text: "EasyPass Live mode disappears as soon as there's a Table element on the page. Happens in the sandbox and on `9.199`. Repro: <https://www.loom.com/share/5bbdeb480ba84e65b1b3de8c190e2003|loom.com/share/live-mode-table>",
          files: [],
          postedAt: msAgo(320),
          media: [
            {
              kind: "loom",
              url: "https://www.loom.com/share/5bbdeb480ba84e65b1b3de8c190e2003",
              embedUrl:
                "https://www.loom.com/embed/5bbdeb480ba84e65b1b3de8c190e2003",
            },
          ],
        },
        {
          ts: "11",
          userId: "U0OMAR",
          authorName: "Omar",
          isApp: false,
          text: "Confirmed on 9.199.0.402, in Edge and Chrome. It comes back if you switch tabs.",
          files: [
            {
              id: "F2",
              name: "live-mode-gone.png",
              mimetype: "image/png",
              permalink: "https://shortpoint.slack.com/files/F2",
            },
          ],
          postedAt: msAgo(313),
          media: [],
        },
        {
          ts: "12",
          userId: "U0YAHIA",
          authorName: "Yahia",
          isApp: false,
          text: "@Ghostex local fix this. We need a before/after video for QC.",
          files: [],
          postedAt: msAgo(305),
          media: [],
        },
        {
          ts: "13",
          botId: "B01",
          authorName: "Ghostex",
          isApp: true,
          text: "Working on this in <https://shortpoint.slack.com/archives/C0KEVIN/p1791500100000100|the working thread> in #kevin-the-bot. The result will be posted here.",
          files: [],
          postedAt: msAgo(304),
          media: [],
        },
      ],
    },
  ],
  sessions: [
    {
      id: "ts-omar",
      memberName: "Omar",
      isMe: false,
      runPlace: "cloud",
      status: "running",
      sessionUrl: "https://claude.ai/code/session_01ABCDEF",
      branch: "yahia/spx-1234-live-mode-table",
      createdAt: msAgo(180),
      updatedAt: msAgo(25),
    },
    {
      id: "ts-yara",
      memberName: "Yara",
      isMe: false,
      runPlace: "local",
      status: "running",
      branch: "yahia/spx-1234-live-mode-table",
      createdAt: msAgo(240),
      updatedAt: msAgo(90),
    },
  ],
};

export async function answerFromFixtures(
  action: string,
  params: Record<string, unknown>,
): Promise<unknown> {
  await new Promise((resolve) => setTimeout(resolve, 60));
  switch (action) {
    case "work.ready":
      return READY;
    case "work.list":
      return {
        ...(fixtureTracker() === "github" ? GITHUB_LIST : LIST),
        generatedAt: new Date().toISOString(),
      };
    case "work.read": {
      const item =
        [...ITEMS, ...GITHUB_ITEMS].find(
          (candidate) =>
            (params.linearIssue &&
              candidate.linearIssue === params.linearIssue) ||
            (params.githubIssue &&
              candidate.githubIssue === params.githubIssue) ||
            (params.pullRequest &&
              candidate.pullRequestRef === params.pullRequest),
        ) ?? ITEMS[0];
      return details(item as WorkItem);
    }
    case "work.createTicket":
      return { opened: true };
    case "work.openUrl": {
      // Kept on the window so a headless check can see which links a click opened.
      const opened = ((
        window as { workFixtureOpenedUrls?: unknown[] }
      ).workFixtureOpenedUrls ??= []);
      opened.push(params.url);
      return { opened: true };
    }
    case "work.startChat":
      return {
        projectId: params.projectId,
        sessionId: "s-new",
        branch: "yahia/spx-1245-copy-link",
      };
    default:
      return { ok: true };
  }
}
