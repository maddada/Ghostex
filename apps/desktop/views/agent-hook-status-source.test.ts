import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const modalHostSource = readFileSync(new URL("./modal-host.tsx", import.meta.url), "utf8");
const contractSource = readFileSync(
  new URL("../../../shared/session-grid-contract-sidebar.ts", import.meta.url),
  "utf8",
);
const gxserverProtocolSource = readFileSync(
  new URL("../../../shared/gxserver-protocol.ts", import.meta.url),
  "utf8",
);

describe("agent hook status source", () => {
  test("checks requested hook providers one at a time and prioritizes Codex, Claude, OpenCode, and Pi", () => {
    /*
     * CDXC:AgentHooks 2026-06-18-02:38:
     * First-launch can request the full supported hook set while native checks
     * providers one at a time, prioritizing Codex, Claude, OpenCode, and Pi
     * before the secondary agents and posting each partial result as soon as it
     * arrives.
     *
     * CDXC:AgentHooks 2026-06-19-08:42:
     * OpenCode participates in first-launch hook-warning suppression, so native
     * must request its status in the priority group instead of waiting for lower
     * priority provider probes.
     */
    expect(contractSource).toContain("agentIds?: readonly string[];");
    expect(modalHostSource).toContain("vscode.postMessage({ agentIds, type: \"requestAgentHookStatus\" });");
    expect(modalHostSource).toContain("vscode.postMessage({ agentIds, type: \"installAgentHooks\" });");
  });

  test("wires advanced Settings uninstall actions for hooks and bundled skills", () => {
    /*
     * CDXC:AgentHooks 2026-06-18-02:54:
     * Advanced Settings should expose explicit uninstall actions for Ghostex
     * hooks and bundled Ghostex skills, with hook cleanup routed through
     * gxserver and skill cleanup handled by the native bundled-skill catalog.
     *
     * CDXC:AgentHookSettings 2026-08-19-11:20:
     * Hook removal is per-agent from each hook row and all-at-once from the
     * Agent Hooks section, so the host must forward the selected agentIds
     * instead of always uninstalling every provider.
     */
    expect(contractSource).toContain('"requestAgentHookStatus"');
    expect(contractSource).toContain('"installAgentHooks"');
    expect(contractSource).toContain('"uninstallAgentHooks"');
    expect(contractSource).toContain('"uninstallBundledAgentSkills"');
    expect(modalHostSource).toContain(
      'vscode.postMessage({ agentIds, type: "uninstallAgentHooks" });',
    );
    expect(modalHostSource).toContain('vscode.postMessage({ type: "uninstallBundledAgentSkills" });');
    expect(gxserverProtocolSource).toContain('"/api/uninstallAgentHooks"');
  });

  test("does not keep a titlebar direct-install hook command", () => {
    /*
     * CDXC:AgentHooks 2026-06-23-05:09:
     * The titlebar Tips hook warning deep-links to Settings instead of invoking
     * installation directly, so the shared native command contract should keep
     * hook writes behind Settings and first-launch setup only.
     */
    expect(contractSource).not.toContain('"installAgentHooksFromTitlebarNotice"');
  });
});
