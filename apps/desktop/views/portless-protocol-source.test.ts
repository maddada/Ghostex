import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const gxserverProtocolSource = readFileSync(
  new URL("../../../shared/gxserver-protocol.ts", import.meta.url),
  "utf8",
);
const nativeHostProtocolSource = readFileSync(
  new URL("../../../shared/native-ghostty-host-protocol.ts", import.meta.url),
  "utf8",
);
const sidebarContractSource = readFileSync(
  new URL("../../../shared/session-grid-contract-sidebar.ts", import.meta.url),
  "utf8",
);
const portlessRustSource = readFileSync(
  new URL("../../../gxserver-rs/src/portless.rs", import.meta.url),
  "utf8",
);
const presentationRustSource = readFileSync(
  new URL("../../../gxserver-rs/src/presentation.rs", import.meta.url),
  "utf8",
);
const protocolRustSource = readFileSync(
  new URL("../../../gxserver-rs/src/protocol.rs", import.meta.url),
  "utf8",
);
const serverRustSource = readFileSync(
  new URL("../../../gxserver-rs/src/server.rs", import.meta.url),
  "utf8",
);

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("Portless Phase 12 protocol plumbing source contract", () => {
  test("gxserver and shared contracts expose metadata-only status and route previews", () => {
    /*
    CDXC:PortlessProtocol 2026-06-23-00:25:
    Phase 12 protocol plumbing must let React render Portless state, desired route previews, and action metadata without reading Portless files or receiving paths, commands, env, process output, tokens, cookies, terminal text, full URLs, or query strings.
    */
    expect(protocolRustSource).toContain("pub portless: PortlessStatusPayload");
    expect(serverRustSource).toContain("portless: read_portless_status_payload_for_paths(&state.paths)");
    expect(presentationRustSource).toContain("insert_portless_presentation_payload(&mut snapshot, db)");
    expect(presentationRustSource).toContain('"portless".to_string()');

    const sharedStatusSource = sourceBetween(
      gxserverProtocolSource,
      "export type GxserverPortlessProtocol",
      "export interface GxserverRuntimeMetadata",
    );
    expect(sharedStatusSource).toContain('export type GxserverPortlessProtocol = "https" | "http"');
    expect(sharedStatusSource).toContain("export interface GxserverPortlessStatus");
    expect(sharedStatusSource).toContain("export interface GxserverPortlessPresentation");
    expect(sharedStatusSource).toContain("export interface GxserverPortlessAssignedDomain");
    expect(sharedStatusSource).toContain("export interface GxserverPortlessRoutePreview");
    expect(sharedStatusSource).toContain("hostname: string");
    expect(sharedStatusSource).toContain("port: number");
    expect(sharedStatusSource).toContain("projectId: GxserverProjectId");
    expect(sharedStatusSource).toContain("parentProjectId?: GxserverProjectId");
    expect(sharedStatusSource).toContain("sessionId: GxserverSessionId");
    expect(sharedStatusSource).toContain("assignedDomains: readonly GxserverPortlessAssignedDomain[]");
    expect(sharedStatusSource).toContain("routePreviews: readonly GxserverPortlessRoutePreview[]");
    expect(sharedStatusSource).not.toContain("url:");
    expect(sharedStatusSource).not.toContain("path:");
    expect(sharedStatusSource).not.toContain("command");
    expect(sharedStatusSource).not.toContain("stdout");
    expect(sharedStatusSource).not.toContain("stderr");
    expect(sharedStatusSource).not.toContain("env");
    expect(gxserverProtocolSource).toContain("portless?: GxserverPortlessStatus");
    expect(gxserverProtocolSource).toContain("portless?: GxserverPortlessPresentation");
  });

  test("Rust route preview payload joins desired routes without Portless file reads or private fields", () => {
    /*
    CDXC:PortlessProtocol 2026-06-23-00:25:
    Desired route previews should be derived from existing listener and route computation, not routes.json. The preview payload must omit pids because React only needs route target metadata and stable owner ids.
    */
    const presentationPayload = sourceBetween(
      portlessRustSource,
      "pub struct PortlessPresentationPayload",
      "pub fn read_portless_status_payload_for_paths",
    );
    const presentationReader = sourceBetween(
      portlessRustSource,
      "pub fn read_portless_presentation_payload",
      "fn read_portless_assigned_domains",
    );
    const assignedDomainReader = sourceBetween(
      portlessRustSource,
      "fn read_portless_assigned_domains",
      "fn portless_status_payload_from_record",
    );
    expect(presentationPayload).toContain("pub assigned_domains: Vec<PortlessAssignedDomain>");
    expect(presentationPayload).toContain("pub parent_project_id: Option<String>");
    expect(presentationPayload).toContain("pub route_previews: Vec<PortlessRoutePreview>");
    expect(presentationPayload).toContain("pub hostname: String");
    expect(presentationPayload).toContain("pub port: u16");
    expect(presentationPayload).toContain("pub project_id: String");
    expect(presentationPayload).toContain("pub session_id: String");
    expect(presentationReader).toContain("compute_live_portless_owned_listeners(db)");
    expect(presentationReader).toContain("compute_desired_portless_routes(db, &listeners)");
    expect(presentationReader).toContain("portless_route_previews_for_desired_routes");
    expect(assignedDomainReader).toContain("backfill_domain_identities()");
    expect(assignedDomainReader).toContain('format!("{}.localhost", project.slug)');
    expect(assignedDomainReader).toContain("worktree.parent_project_id");
    expect(presentationPayload).not.toContain("routes.json");
    expect(presentationPayload).not.toContain("PORTLESS_ROUTES_FILE");
    expect(presentationPayload).not.toContain("pub pid");
    expect(presentationPayload).not.toContain("stdout");
    expect(presentationPayload).not.toContain("stderr");
    expect(presentationPayload).not.toContain("env");
    expect(presentationPayload).not.toContain("url");
  });

  test("native sidebar exposes local-only action availability and sanitized admin results through HUD", () => {
    /*
    CDXC:PortlessProtocol 2026-06-23-00:25:
    Native-sidebar is the first local-Mac action boundary. Its HUD projection may make recommended setup actions available only when the native bridge exists, while remote/non-native gxserver metadata remains localMacOnly.
    */
    const sidebarPortlessStateSource = sourceBetween(
      sidebarContractSource,
      "export type SidebarPortlessState =",
      "export type SidebarHudState =",
    );
    expect(sidebarPortlessStateSource).toContain("health: GxserverPortlessStatus");
    expect(sidebarPortlessStateSource).toContain("presentation?: GxserverPortlessPresentation");
    expect(sidebarPortlessStateSource).toContain("lastResult?: NativePortlessAdminResult");
    expect(sidebarPortlessStateSource).not.toContain("stdout");
    expect(sidebarPortlessStateSource).not.toContain("stderr");

    expect(nativeHostProtocolSource).toContain("export type NativePortlessAdminResult = Extract<");
    const sharedNativeResult = sourceBetween(
      nativeHostProtocolSource,
      "Native Portless admin results are structured and sanitized.",
      "protocolVersion: typeof NATIVE_GHOSTTY_HOST_PROTOCOL_VERSION;",
    );
    expect(sharedNativeResult).toContain("requestId: string");
    expect(sharedNativeResult).toContain("status: string");
    expect(sharedNativeResult).not.toContain("stdout");
    expect(sharedNativeResult).not.toContain("stderr");
  });

  test("Phase 16 state update contract covers protocol change, failure, retry, disable, and remove", () => {
    /*
    CDXC:PortlessFailureUX 2026-06-23-04:28:
    Phase 16 recovery must flow through gxserver-owned state. Native-sidebar may
    launch privileged local actions, but setup failure, retry, Disable route
    clearing, protocol changes, and explicit service removal are reported as
    sanitized enum/boolean/protocol updates.
    */
    const sharedStatusSource = sourceBetween(
      gxserverProtocolSource,
      "export type GxserverPortlessProtocol",
      "export interface GxserverPortlessAdminActionAvailability",
    );
    expect(gxserverProtocolSource).toContain('| "/api/updatePortlessState"');
    expect(protocolRustSource).toContain('| "/api/updatePortlessState"');
    expect(serverRustSource).toContain('"/api/updatePortlessState"');
    expect(serverRustSource).toContain("handle_portless_state_http");
    expect(portlessRustSource).toContain("pub enum PortlessStateUpdate");
    expect(portlessRustSource).toContain("SetProtocol");
    expect(portlessRustSource).toContain("RecordAdminResult");
    expect(portlessRustSource).toContain("sync_portless_routes(paths, &[])");
    expect(sharedStatusSource).toContain("export type GxserverPortlessStateUpdateParams");
    expect(sharedStatusSource).toContain('kind: "setProtocol"');
    expect(sharedStatusSource).toContain('kind: "recordAdminResult"');
    expect(sharedStatusSource).toContain('kind: "setEnabled"');
    expect(sharedStatusSource).not.toContain("stdout");
    expect(sharedStatusSource).not.toContain("stderr");
    expect(sharedStatusSource).not.toContain("command");
    expect(sharedStatusSource).not.toContain("env");
  });
});
