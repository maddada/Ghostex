/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Entry point kept at the original path so `main.tsx` (the only importer) does not
have to change. The 21,861-line implementation now lives in
`./gxserver-runtime/`; see `gxserver-runtime/core.ts` for the class shell and the
prototype composition that re-attaches the per-responsibility method modules.
*/
export { createGpuiSidebarRuntime, GpuiSidebarLocalMessageSource } from "./gxserver-runtime/core";
export type {
  GhostexGpuiSidebarBridge,
  GpuiCommandPaneSessionSummary,
  GpuiGxserverBootstrap,
  GpuiWorkspaceSessionDelayedSendSummary,
} from "./gxserver-runtime/types-and-protocol";
