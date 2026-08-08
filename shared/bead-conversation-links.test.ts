import { describe, expect, test } from "vitest";
import { createGxserverPresentationProjectSessionId } from "./gxserver-presentation-sidebar-projection";
import {
  beadConversationLinkMatchKey,
  beadConversationLinkStoreKey,
  canonicalizeBeadConversationLinksForBoard,
  createBeadConversationLinkId,
  indexBeadConversationLinksByBead,
  normalizeBeadConversationLinks,
  parseBeadConversationLinkSessionPersistenceName,
  resolveBeadConversationLinkBoardSessionId,
  resolveBeadConversationLinkSessionReference,
  selectBeadConversationLinks,
  selectBeadConversationLinkStoreProjects,
} from "./bead-conversation-links";

describe("bead conversation links", () => {
  test("should keep multiple beads linked to the same Ghostex conversation", () => {
    /*
     * CDXC:ProjectBoard 2026-05-26-10:20:
     * The link model must allow several Beads issues to route to one agent conversation.
     * Normalize link records independently by bead id instead of deduplicating by Ghostex session id.
     */
    const links = normalizeBeadConversationLinks(
      [
        {
          beadId: "zmux-123",
          ghostexSessionId: "session-a",
          id: "link-1",
          projectId: "project-a",
          status: "active",
        },
        {
          beadId: "zmux-456",
          ghostexSessionId: "session-a",
          id: "link-2",
          projectId: "project-a",
          status: "active",
        },
      ],
      "project-a",
    );

    expect(links).toHaveLength(2);
    expect(links.map((link) => link.beadId)).toEqual(["zmux-123", "zmux-456"]);
    expect(new Set(links.map((link) => link.ghostexSessionId))).toEqual(new Set(["session-a"]));
  });

  test("should derive stable ids from project, bead, and Ghostex session", () => {
    expect(createBeadConversationLinkId("My Project", "ZMUX 123", "session/a")).toBe(
      "My-Project:ZMUX-123:session-a",
    );
  });

  test("should discard unusable records and normalize optional metadata", () => {
    const links = normalizeBeadConversationLinks(
      [
        { beadId: "zmux-1" },
        {
          agentName: " codex ",
          agentSessionId: " 019-session ",
          beadDisplayId: " ZMX-1 ",
          beadId: " zmux-1 ",
          ghostexSessionId: " session-1 ",
          sessionPersistenceProvider: "zmx",
          status: "archived",
        },
      ],
      "project-a",
    );

    expect(links).toMatchObject([
      {
        agentName: "codex",
        agentSessionId: "019-session",
        beadDisplayId: "ZMX-1",
        beadId: "zmux-1",
        ghostexSessionId: "session-1",
        projectId: "project-a",
        sessionPersistenceProvider: "zmx",
        status: "archived",
      },
    ]);
  });
});

describe("bead conversation link matching", () => {
  const link = (beadId: string, ghostexSessionId: string) => ({
    beadId,
    ghostexSessionId,
    id: `${beadId}:${ghostexSessionId}`,
  });

  test("should keep a link matched to its bead after a prefix rename", () => {
    /*
     * A board load can rename every issue prefix (zmux-95421485 becomes
     * ghostex-95421485). Links persist the id they were written with, so the
     * lookup must survive the rename instead of orphaning the conversation.
     */
    const index = indexBeadConversationLinksByBead([link("zmux-95421485", "session-a")]);

    expect(selectBeadConversationLinks(index, "ghostex-95421485").map((entry) => entry.id)).toEqual([
      "zmux-95421485:session-a",
    ]);
  });

  test("should group pre-rename and post-rename links under one bead", () => {
    const index = indexBeadConversationLinksByBead([
      link("ghostex-95421485", "session-b"),
      link("zmux-95421485", "session-a"),
    ]);

    expect(selectBeadConversationLinks(index, "ghostex-95421485").map((entry) => entry.id)).toEqual([
      "ghostex-95421485:session-b",
      "zmux-95421485:session-a",
    ]);
  });

  test("should not match links belonging to a different bead", () => {
    const index = indexBeadConversationLinksByBead([link("zmux-95421485", "session-a")]);

    expect(selectBeadConversationLinks(index, "ghostex-11112222")).toEqual([]);
  });

  test("should match hyphenated prefixes on the trailing issue id only", () => {
    expect(beadConversationLinkMatchKey("agent-bo-95421485")).toBe("95421485");
    expect(beadConversationLinkMatchKey(" ZMUX-95421485 ")).toBe("95421485");
    expect(beadConversationLinkMatchKey("95421485")).toBe("95421485");
    expect(beadConversationLinkMatchKey("zmux-")).toBe("zmux-");
  });

  test("should return no links for a bead nothing was ever linked to", () => {
    expect(selectBeadConversationLinks(indexBeadConversationLinksByBead([]), "zmux-1")).toEqual([]);
  });
});

describe("bead conversation link stores", () => {
  const project = (projectId: string, path: string, beadsDirectory?: string) => ({
    path,
    projectBoardConfig: beadsDirectory ? { beadsDirectory } : {},
    projectId,
  });

  test("should treat project rows pointed at one beads directory as a single link store", () => {
    /*
     * The same board can be mounted by two project rows (a checkout and its
     * worktree, or the same folder registered twice). Each row keeps its own
     * beadConversationLinks, so a card must read every row that shares the
     * board or half its history is invisible.
     */
    const board = project("P1", "/code/app");
    const mirror = project("P2", "/code/app-worktree", "/code/app");
    const unrelated = project("P3", "/code/other");

    expect(
      selectBeadConversationLinkStoreProjects(board, [board, mirror, unrelated]).map(
        (entry) => entry.projectId,
      ),
    ).toEqual(["P1", "P2"]);
  });

  test("should fall back to the project path when no beads directory is configured", () => {
    const board = project("P1", "/code/app/");
    const duplicate = project("P2", "/code/app");

    expect(beadConversationLinkStoreKey(board)).toBe("/code/app");
    expect(
      selectBeadConversationLinkStoreProjects(board, [duplicate]).map((entry) => entry.projectId),
    ).toEqual(["P1", "P2"]);
  });

  test("should keep a pathless project row to itself", () => {
    const board = { projectBoardConfig: {}, projectId: "P1" };
    const other = { projectBoardConfig: {}, projectId: "P2" };

    expect(selectBeadConversationLinkStoreProjects(board, [other])).toEqual([board]);
  });
});

describe("board-relative bead conversation links", () => {
  const link = (projectId: string, ghostexSessionId: string, updatedAt: string) => ({
    beadId: "ghostex-95421485",
    ghostexSessionId,
    id: `${projectId}:ghostex-95421485:${ghostexSessionId}`,
    projectId,
    updatedAt,
  });

  test("should leave links owned by the board project untouched", () => {
    const links = [link("P1", "G1", "2026-08-06T10:00:00.000Z")];

    expect(canonicalizeBeadConversationLinksForBoard(links, "P1")).toEqual([
      { ...links[0], sessionProjectId: "P1" },
    ]);
  });

  test("should re-express a sibling row's link against the board project", () => {
    const links = [link("P2", "G1", "2026-08-06T10:00:00.000Z")];

    expect(canonicalizeBeadConversationLinksForBoard(links, "P1")[0]?.ghostexSessionId).toBe(
      createGxserverPresentationProjectSessionId("P2", "G1"),
    );
  });

  test("should collapse one conversation stored by two rows, keeping the newest", () => {
    const scoped = createGxserverPresentationProjectSessionId("P1", "G1");
    const links = [
      link("P2", scoped, "2026-08-05T10:00:00.000Z"),
      link("P1", "G1", "2026-08-06T10:00:00.000Z"),
    ];

    const canonical = canonicalizeBeadConversationLinksForBoard(links, "P1");

    expect(canonical).toHaveLength(1);
    expect(canonical[0]?.projectId).toBe("P1");
    expect(canonical[0]?.updatedAt).toBe("2026-08-06T10:00:00.000Z");
  });

  test("should resolve the board-relative session id a link points at", () => {
    expect(resolveBeadConversationLinkBoardSessionId(link("P2", "G1", ""), "P1")).toBe(
      createGxserverPresentationProjectSessionId("P2", "G1"),
    );
    expect(
      resolveBeadConversationLinkBoardSessionId(
        link("P2", createGxserverPresentationProjectSessionId("P1", "G1"), ""),
        "P1",
      ),
    ).toBe("G1");
  });
});

describe("bead conversation link session owners", () => {
  /*
   * Live repro (2026-08-07): one demo bead rendered two identical rows because
   * each board mount stored its own link for the same worked session, and the
   * bare session id was read as belonging to whichever row stored it. The
   * session actually lived in a third project, which is also why the resume
   * plan lookup missed.
   */
  const storedLink = (projectId: string, updatedAt: string) => ({
    beadId: "agent-bo-95421491",
    createdAt: updatedAt,
    ghostexSessionId: "G45qx",
    id: `${projectId}:agent-bo-95421491:G45qx`,
    projectId,
    sessionPersistenceName: "S60-P9gzj-G45qx",
    updatedAt,
  });

  test("should collapse one conversation stored by two board mounts", () => {
    const canonical = canonicalizeBeadConversationLinksForBoard(
      [
        storedLink("P3mjq", "2026-08-07T05:00:04.000Z"),
        storedLink("P85fx", "2026-08-07T05:35:00.000Z"),
      ],
      "P85fx",
    );

    expect(canonical).toHaveLength(1);
    expect(canonical[0]?.projectId).toBe("P85fx");
    expect(canonical[0]?.ghostexSessionId).toBe(
      createGxserverPresentationProjectSessionId("P9gzj", "G45qx"),
    );
  });

  test("should collapse the pair when only one row records the session owner", () => {
    /*
     * Only the row that wrote the link while the session was live is
     * guaranteed to carry the provider name, so one side of a duplicate pair
     * can be left guessing its own project as the owner.
     */
    const stored = [
      { ...storedLink("P3mjq", "2026-08-07T05:00:04.000Z"), sessionPersistenceName: undefined },
      storedLink("P85fx", "2026-08-07T05:35:00.000Z"),
    ];
    const canonical = canonicalizeBeadConversationLinksForBoard(stored, "P85fx");

    expect(canonical).toHaveLength(1);
    expect(canonical[0]?.ghostexSessionId).toBe(
      createGxserverPresentationProjectSessionId("P9gzj", "G45qx"),
    );
    expect(canonical[0]?.sessionProjectId).toBe("P9gzj");
    for (const link of stored) {
      expect(resolveBeadConversationLinkBoardSessionId(link, "P85fx", stored)).toBe(
        canonical[0]?.ghostexSessionId,
      );
    }
  });

  test("should keep two conversations that name different owning projects", () => {
    const canonical = canonicalizeBeadConversationLinksForBoard(
      [
        storedLink("P3mjq", "2026-08-07T05:00:04.000Z"),
        {
          ...storedLink("P85fx", "2026-08-07T05:35:00.000Z"),
          sessionPersistenceName: "S60-P2def-G45qx",
        },
      ],
      "P85fx",
    );

    expect(canonical).toHaveLength(2);
  });

  test("should read the owning project out of the zmx session name", () => {
    expect(parseBeadConversationLinkSessionPersistenceName("S60-P9gzj-G45qx")).toEqual({
      projectId: "P9gzj",
      sessionId: "G45qx",
    });
    expect(parseBeadConversationLinkSessionPersistenceName("ghostex-work")).toBeUndefined();
    expect(parseBeadConversationLinkSessionPersistenceName("S60-P9gzj")).toBeUndefined();
    expect(parseBeadConversationLinkSessionPersistenceName("S60-X9gzj-G45qx")).toBeUndefined();
    expect(parseBeadConversationLinkSessionPersistenceName(undefined)).toBeUndefined();
  });

  test("should prefer explicit session owners over the zmx session name", () => {
    expect(
      resolveBeadConversationLinkSessionReference({
        ...storedLink("P3mjq", "2026-08-07T05:00:04.000Z"),
        sessionProjectId: "P1abc",
      }),
    ).toEqual({ projectId: "P1abc", sessionId: "G45qx" });

    expect(
      resolveBeadConversationLinkSessionReference({
        ...storedLink("P3mjq", "2026-08-07T05:00:04.000Z"),
        ghostexSessionId: createGxserverPresentationProjectSessionId("P2def", "G45qx"),
        sessionProjectId: "P1abc",
      }),
    ).toEqual({ projectId: "P2def", sessionId: "G45qx" });
  });

  test("should fall back to the storing project when nothing names the session owner", () => {
    expect(
      resolveBeadConversationLinkSessionReference({
        ...storedLink("P3mjq", "2026-08-07T05:00:04.000Z"),
        sessionPersistenceName: undefined,
      }),
    ).toEqual({ projectId: "P3mjq", sessionId: "G45qx" });

    expect(
      resolveBeadConversationLinkSessionReference({
        ...storedLink("P3mjq", "2026-08-07T05:00:04.000Z"),
        sessionPersistenceName: "S60-P9gzj-G99zz",
      }),
    ).toEqual({ projectId: "P3mjq", sessionId: "G45qx" });
  });
});
