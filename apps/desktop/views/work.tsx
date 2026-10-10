import { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  installFixtureAnswers,
  isFixtureMode,
  onCurrentSession,
  onWorkOpen,
  onWorkRefresh,
  workRequest,
} from "./work/bridge";
import { itemRef, refKey } from "./work/format";
import {
  readStoredGroupBy,
  storeGroupBy,
  type WorkGroupBy,
} from "./work/grouping";
import {
  TicketDetailsView,
  type CloudBoxState,
  type StartChatChoice,
} from "./work/ticket-details";
import { WorkToast, type WorkToastState } from "./work/toast";
import type {
  CloudDraft,
  CloudProvider,
  CloudStartResult,
  CurrentSession,
  LinkResult,
  StartWorkResult,
  WorkAgent,
  WorkItem,
  WorkItemDetails,
  WorkItemRef,
  WorkItemSession,
  WorkList,
  WorkReady,
} from "./work/types";
import {
  DEFAULT_WORK_FILTERS,
  WorkListView,
  type WorkFilters,
} from "./work/work-list";
import "./work/work.css";

/** The list re-reads itself this often while the page is on screen. */
const AUTO_REFRESH_MS = 90_000;
/** A cloud start takes 5 to 30 seconds; the app gives up after 200 (work_view/bridge.rs). */
const CLOUD_START_TIMEOUT_MS = 210_000;
/** While gxserver says it is still fetching, ask again after this long, a few times. */
const REFRESHING_RETRY_MS = 4_000;
const REFRESHING_RETRIES = 5;

type Route =
  | { view: "list" }
  | { view: "details"; ref: WorkItemRef; item: WorkItem | null };

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function WorkApp() {
  const [agents, setAgents] = useState<WorkAgent[]>([]);
  const [list, setList] = useState<WorkList | null>(null);
  const [listLoading, setListLoading] = useState(true);
  const [listError, setListError] = useState<string | null>(null);
  // Kept in memory: the page lives as long as its view tab, so filters survive switching views.
  const [filters, setFilters] = useState<WorkFilters>(DEFAULT_WORK_FILTERS);
  const [groupBy, setGroupBy] = useState<WorkGroupBy>(readStoredGroupBy);
  const [collapsedGroups, setCollapsedGroups] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [route, setRoute] = useState<Route>({ view: "list" });
  const [details, setDetails] = useState<WorkItemDetails | null>(null);
  const [detailsLoading, setDetailsLoading] = useState(false);
  const [detailsError, setDetailsError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);
  const [newTicketError, setNewTicketError] = useState<string | null>(null);
  const [currentSession, setCurrentSession] = useState<CurrentSession | null>(
    null,
  );
  const [linking, setLinking] = useState(false);
  const [cloudBox, setCloudBox] = useState<CloudBoxState | null>(null);
  const [toast, setToast] = useState<WorkToastState | null>(null);
  const closeToast = useCallback(() => setToast(null), []);
  const [now, setNow] = useState(() => Date.now());
  const listRequest = useRef(0);
  const detailsRequest = useRef(0);
  const routeRef = useRef(route);
  routeRef.current = route;

  const loadList = useCallback(
    (force: boolean, retries = REFRESHING_RETRIES) => {
      const request = ++listRequest.current;
      setListLoading(true);
      workRequest<WorkList>("work.list", { force })
        .then((data) => {
          if (request !== listRequest.current) return;
          setList(data);
          setListError(null);
          setNow(Date.now());
          if (data.refreshing && retries > 0) {
            window.setTimeout(() => {
              if (request === listRequest.current) loadList(false, retries - 1);
            }, REFRESHING_RETRY_MS);
          }
        })
        .catch((error: unknown) => {
          if (request === listRequest.current) setListError(errorText(error));
        })
        .finally(() => {
          if (request === listRequest.current) setListLoading(false);
        });
    },
    [],
  );

  const loadDetails = useCallback((ref: WorkItemRef, force: boolean) => {
    const request = ++detailsRequest.current;
    setDetailsLoading(true);
    setDetailsError(null);
    workRequest<WorkItemDetails>("work.read", { ...ref, force })
      .then((data) => {
        if (request === detailsRequest.current) setDetails(data);
      })
      .catch((error: unknown) => {
        if (request === detailsRequest.current)
          setDetailsError(errorText(error));
      })
      .finally(() => {
        if (request === detailsRequest.current) setDetailsLoading(false);
      });
  }, []);

  const openRef = useCallback(
    (ref: WorkItemRef, item: WorkItem | null) => {
      setDetails(null);
      setStartError(null);
      setCloudBox(null);
      setRoute({ view: "details", ref, item });
      loadDetails(ref, false);
      document.querySelector(".w-scroll")?.scrollTo({ top: 0 });
    },
    [loadDetails],
  );

  // A chip on a session card: the row it names, when the list already has it.
  const openFromChip = useCallback(
    (ref: WorkItemRef) => {
      void workRequest("work.ackOpen").catch(() => undefined);
      const key = refKey(ref);
      const row =
        list?.items.find(
          (item) =>
            refKey(itemRef(item)) === key ||
            (!!ref.pullRequest && item.pullRequest?.url === ref.pullRequest),
        ) ?? null;
      openRef(
        row
          ? { ...itemRef(row), projectId: row.projectId ?? ref.projectId }
          : ref,
        row,
      );
    },
    [list, openRef],
  );

  useEffect(() => {
    let cancelled = false;
    workRequest<WorkReady>("work.ready")
      .then((ready) => {
        if (cancelled) return;
        setAgents(ready.agents ?? []);
        setCurrentSession(ready.currentSession ?? null);
        if (ready.pendingOpen) openFromChip(ready.pendingOpen);
      })
      .catch(() => undefined);
    loadList(false);
    return () => {
      cancelled = true;
    };
    // Once, on mount; later opens arrive through onWorkOpen.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => onWorkOpen(openFromChip), [openFromChip]);

  useEffect(() => onCurrentSession(setCurrentSession), []);

  // A ticket made in the app's dialog (New ticket below, or a project's "…" menu) is new to Linear,
  // so the list skips gxserver's cache.
  useEffect(() => onWorkRefresh(() => loadList(true)), [loadList]);

  useEffect(() => {
    const tick = window.setInterval(() => setNow(Date.now()), 30_000);
    const refresh = window.setInterval(() => {
      if (
        document.visibilityState === "visible" &&
        routeRef.current.view === "list"
      )
        loadList(false);
    }, AUTO_REFRESH_MS);
    return () => {
      window.clearInterval(tick);
      window.clearInterval(refresh);
    };
  }, [loadList]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        routeRef.current.view === "details" &&
        !document.querySelector(".w-menu")
      ) {
        setRoute({ view: "list" });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // The app opens its own Create Linear Ticket dialog: for the repo the list is filtered to, the
  // only project, or with the dialog's Project picker.
  const newTicket = () => {
    const projects = list?.projects ?? [];
    const projectId =
      filters.projectId ||
      (projects.length === 1 ? projects[0]?.projectId : undefined);
    setNewTicketError(null);
    void workRequest("work.createTicket", { projectId }).catch(
      (error: unknown) => setNewTicketError(errorText(error)),
    );
  };

  // Hidden at once; gxserver remembers it for every window.
  const dismissNotice = (notice: string) => {
    setList((current) =>
      current?.githubProjects && notice === "githubProjectsScope"
        ? {
            ...current,
            githubProjects: { ...current.githubProjects, noticeDismissed: true },
          }
        : current,
    );
    void workRequest("work.dismissNotice", { notice }).catch(() => undefined);
  };

  const openUrl = (url: string) => {
    void workRequest("work.openUrl", { url }).catch(() => undefined);
  };

  const openChat = (session: WorkItemSession) => {
    void workRequest("work.openChat", {
      projectId: session.projectId,
      sessionId: session.sessionId,
    }).catch((error: unknown) => setStartError(errorText(error)));
  };

  const startChat = (choice: StartChatChoice) => {
    if (route.view !== "details") return;
    const ref = route.ref;
    setStarting(true);
    setStartError(null);
    workRequest<StartWorkResult>("work.startChat", {
      linearIssue: ref.linearIssue,
      githubIssue: ref.githubIssue,
      projectId: choice.projectId,
      agentId: choice.agentId,
    })
      .then(() => {
        loadDetails({ ...ref, projectId: choice.projectId }, true);
        loadList(false);
      })
      .catch((error: unknown) => setStartError(errorText(error)))
      .finally(() => setStarting(false));
  };

  const refreshDetails = () => {
    if (route.view !== "details") return;
    loadDetails(route.ref, true);
    loadList(false);
  };

  // Link to current session: the ticket joins the session's links (gxserver keeps the others),
  // and Undo sends back exactly what the link answered with (server/src/work_mode/link_add.rs).
  const linkCurrentSession = (session: CurrentSession, item: WorkItem) => {
    const target = {
      projectId: session.projectId,
      sessionId: session.sessionId,
    };
    setLinking(true);
    workRequest<LinkResult>("work.linkCurrentSession", {
      ...target,
      linearIssue: item.kind === "linearIssue" ? item.linearIssue : undefined,
      githubIssue: item.kind === "githubIssue" ? item.githubIssue : undefined,
      pullRequest:
        item.kind === "pullRequest"
          ? (item.pullRequest?.url ?? item.pullRequest?.number)
          : undefined,
    })
      .then((result) => {
        refreshDetails();
        const undo = result.undo;
        setToast({
          id: Date.now(),
          text: `Linked ${item.id} to ‘${session.title}’`,
          action:
            undo && Object.keys(undo).length > 0
              ? {
                  label: "Undo",
                  run: () => {
                    setToast(null);
                    workRequest("work.undoLink", { ...target, undo })
                      .then(refreshDetails)
                      .catch((error: unknown) =>
                        setToast({
                          id: Date.now(),
                          text: errorText(error),
                          tone: "error",
                        }),
                      );
                  },
                }
              : undefined,
        });
      })
      .catch((error: unknown) =>
        setToast({ id: Date.now(), text: errorText(error), tone: "error" }),
      )
      .finally(() => setLinking(false));
  };

  // Start in cloud: gxserver drafts the task from the ticket, the box shows it for editing, and
  // Start runs the cloud start (server/src/work_mode/cloud_work.rs).
  const ticketParams = () =>
    route.view === "details"
      ? {
          projectId: details?.item?.projectId ?? route.ref.projectId,
          linearIssue: route.ref.linearIssue,
          githubIssue: route.ref.githubIssue,
          pullRequest: route.ref.pullRequest,
        }
      : null;

  const pickCloud = (provider: CloudProvider) => {
    const params = ticketParams();
    if (!params) return;
    setCloudBox({
      provider,
      draft: null,
      loading: true,
      error: null,
      starting: false,
    });
    workRequest<CloudDraft>("work.draftCloud", params)
      .then((draft) =>
        setCloudBox((box) =>
          box?.provider.id === provider.id
            ? { ...box, draft, loading: false }
            : box,
        ),
      )
      .catch((error: unknown) =>
        setCloudBox((box) =>
          box?.provider.id === provider.id
            ? { ...box, loading: false, error: errorText(error) }
            : box,
        ),
      );
  };

  const startCloud = (prompt: string) => {
    const params = ticketParams();
    if (!params || !cloudBox) return;
    const provider = cloudBox.provider;
    setCloudBox({ ...cloudBox, starting: true, error: null });
    workRequest<CloudStartResult>(
      "work.startCloud",
      { ...params, provider: provider.id, prompt },
      CLOUD_START_TIMEOUT_MS,
    )
      .then((result) => {
        setCloudBox(null);
        openUrl(result.sessionUrl);
        refreshDetails();
        setToast({
          id: Date.now(),
          text: [
            `${provider.name} started in the cloud on ${result.branch}`,
            ...result.warnings,
          ].join(". "),
          tone: result.warnings.length > 0 ? "warning" : undefined,
        });
      })
      .catch((error: unknown) =>
        setCloudBox((box) =>
          box ? { ...box, starting: false, error: errorText(error) } : box,
        ),
      );
  };

  const openCloudInTerminal = (sessionUrl: string, item: WorkItem) => {
    const projectId = item.projectId ?? details?.projects[0]?.projectId;
    void workRequest("work.openCloudInTerminal", {
      projectId,
      sessionUrl,
      title: `${item.id} · ${item.title}`.slice(0, 60),
    }).catch((error: unknown) =>
      setToast({ id: Date.now(), text: errorText(error), tone: "error" }),
    );
  };

  return (
    <div className="w-scroll">
      {route.view === "list" ? (
        <WorkListView
          data={list}
          loading={listLoading}
          error={listError}
          filters={filters}
          onFiltersChange={setFilters}
          onRefresh={() => loadList(true)}
          onOpen={(item) => openRef(itemRef(item), item)}
          onNewTicket={newTicket}
          newTicketError={newTicketError}
          onDismissNotice={dismissNotice}
          groupBy={groupBy}
          onGroupByChange={(next) => {
            setGroupBy(next);
            storeGroupBy(next);
          }}
          collapsedGroups={collapsedGroups}
          onToggleGroup={(groupId) =>
            setCollapsedGroups((current) => {
              const next = new Set(current);
              if (!next.delete(groupId)) next.add(groupId);
              return next;
            })
          }
          onOpenUrl={openUrl}
          now={now}
        />
      ) : (
        <TicketDetailsView
          details={details}
          fallbackItem={route.item}
          loading={detailsLoading}
          error={detailsError}
          agents={agents}
          starting={starting}
          startError={startError}
          now={now}
          currentSession={currentSession}
          linking={linking}
          cloudBox={cloudBox}
          onBack={() => {
            setRoute({ view: "list" });
            loadList(false);
          }}
          onRefresh={() => loadDetails(route.ref, true)}
          onOpenChat={openChat}
          onStartChat={startChat}
          onOpenUrl={openUrl}
          onLinkCurrentSession={linkCurrentSession}
          onPickCloud={pickCloud}
          onStartCloud={startCloud}
          onCancelCloud={() => setCloudBox(null)}
          onOpenCloudInTerminal={openCloudInTerminal}
        />
      )}
      <WorkToast toast={toast} onClose={closeToast} />
    </div>
  );
}

async function mount(): Promise<void> {
  if (isFixtureMode()) {
    const { answerFromFixtures } = await import("./work/fixtures");
    installFixtureAnswers(answerFromFixtures);
  }
  const root = document.getElementById("root");
  if (root) createRoot(root).render(<WorkApp />);
}

void mount();
