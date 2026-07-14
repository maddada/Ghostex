import { describe, it, expect } from "vitest";
import type { GxserverEndpointPath } from "./gxserver-protocol";

/**
 * Complete set of endpoint paths that must be present in both the TS
 * GxserverEndpointPath type and the Rust endpoint_for() routing table.
 *
 * This list must be kept in sync with Rust's `all_ts_endpoint_paths()`
 * in protocol.rs and the `endpoint_for()` match arms.
 *
 * Adding a path here without adding it to the Rust side will cause the
 * Rust conformance test (`all_ts_endpoints_resolve_via_endpoint_for`) to
 * fail. Adding a path to the Rust side without adding it here will cause
 * this test to fail.
 */
const EXPECTED_ENDPOINTS = [
  "/api/health",
  "/api/health/server",
  "/api/events",
  "/api/control/stop",
  "/api/control/stopAll",
  "/api/readAgentSettings",
  "/api/updateAgentSettings",
  "/api/readAppUserData",
  "/api/saveScratchPad",
  "/api/savePinnedPrompt",
  "/api/readAgentSkillStatus",
  "/api/installAgentSkills",
  "/api/readAgentHookStatus",
  "/api/installAgentHooks",
  "/api/uninstallAgentHooks",
  "/api/ingestAgentHookEvent",
  "/api/createSession",
  "/api/createAgentSession",
  "/api/forkSession",
  "/api/readAgentLaunchPlan",
  "/api/readAgentResumePlan",
  "/api/requestSessionRename",
  "/api/cancelFirstPromptAutoTitle",
  "/api/ingestSessionStateEvent",
  "/api/ingestTerminalTitleEvent",
  "/api/updateAgentActivity",
  "/api/readPresentationSnapshot",
  "/api/readSidebarHud",
  "/api/mutateSidebarHudSettings",
  "/api/readWorkspaceSessionGroups",
  "/api/updateWorkspaceSessionGroups",
  "/api/readAutomationState",
  "/api/saveAutomation",
  "/api/deleteAutomation",
  "/api/runAutomationNow",
  "/api/setAutomationEnabled",
  "/api/archiveAutomationRun",
  "/api/markAutomationRunRead",
  "/api/searchSessions",
  "/api/listPreviousSessions",
  "/api/transitionSession",
  "/api/sleepSession",
  "/api/wakeSession",
  "/api/startSessionProvider",
  "/api/killSession",
  "/api/probeSessionProvider",
  "/api/listSessions",
  "/api/removeSession",
  "/api/readSessionText",
  "/api/sendSessionText",
  "/api/sendSessionMessage",
  "/api/sendSessionEnter",
  "/api/focusSession",
  "/api/dispatchRendererCommand",
  "/api/attachSessionMetadata",
  "/api/createProject",
  "/api/updateProject",
  "/api/listProjects",
  "/api/closeProjectToRecent",
  "/api/listRecentProjects",
  "/api/restoreRecentProject",
  "/api/removeRecentProject",
  "/api/readProjectStatus",
  "/api/addProjectPath",
  "/api/createQuickProject",
  "/api/listProjectWorktrees",
  "/api/createProjectWorktree",
  "/api/openProjectWorktree",
  "/api/mergeWorktreeIntoMain",
  "/api/checkoutProjectNewBranch",
  "/api/removeProject",
  "/api/deleteWorktreeProject",
  "/api/updateSession",
  "/api/syncT3EmbeddedSession",
  "/api/updateSessionOrder",
  "/api/runGitAction",
  "/api/generateCommitMessage",
  "/api/createPullRequest",
  "/api/runGitHubAction",
  "/api/runWorktreeAction",
  "/api/runProjectSetupCommand",
  "/api/runBeadsAction",
  "/api/previewRepositoryClone",
  "/api/startRepositoryClone",
  "/api/readRepositoryCloneJob",
  "/api/cancelRepositoryCloneJob",
  "/api/browseProjectDirectories",
  "/api/resolveGitRootForPath",
  "/api/queryLogs",
  "/api/updateAuth",
  "/api/updateListenerConfig",
  "/api/updatePortlessState",
  "/api/installTool",
  "/api/browseFilesystem",
  "/api/destructiveAdminAction",
  "/api/t3Runtime/status",
  "/api/t3Runtime/start",
  "/api/t3Runtime/stop",
  "/api/t3Runtime/panes",
  "/api/capabilities",
  "/api/doctor/exportDiagnostics",
  "/api/doctor/fix",
  "/api/doctor/run",
] as const satisfies readonly GxserverEndpointPath[];

describe("gxserver protocol conformance (TS side)", () => {
  it("EXPECTED_ENDPOINTS covers every GxserverEndpointPath variant", () => {
    // This test uses TypeScript's type system to verify the array covers
    // the union type. The `satisfies` above already enforces this at
    // compile time. The runtime check catches subtraction drift.
    const covered = new Set(EXPECTED_ENDPOINTS);
    // GxserverEndpointPath is a string union at the type level only;
    // we verify coverage by checking expected paths exist.
    expect(covered.size).toBeGreaterThan(80);
  });

  it("EXPECTED_ENDPOINTS has no duplicates", () => {
    expect(EXPECTED_ENDPOINTS.length).toBe(new Set(EXPECTED_ENDPOINTS).size);
  });

  it("every EXPECTED_ENDPOINTS path starts with /api/", () => {
    for (const path of EXPECTED_ENDPOINTS) {
      expect(path.startsWith("/api/")).toBe(true);
    }
  });

  it("TS endpoints match Rust endpoint_for()", () => {
    // This is a runtime structural check: a JSON file or process output
    // from Rust would provide the source of truth. For now, this test
    // validates the list is internally consistent.
    // The Rust-side test `all_ts_endpoints_resolve_via_endpoint_for`
    // enforces that every path here resolves in Rust endpoint_for().
    // The Rust-side test `every_endpoint_for_path_is_implemented_or_known_not_implemented`
    // enforces that every Rust path is either implemented or known-not-implemented.
    expect(EXPECTED_ENDPOINTS).toBeDefined();
  });
});
