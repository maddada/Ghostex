/*
 * Zustand store wiring. The state shape and action signatures live in types.ts;
 * the behavior lives in ../engine/sim-controller.ts (engine agent's domain).
 * Do NOT add behavior here — extend the engine instead. See SPEC.md.
 */
import { create } from "zustand";
import { createEngineActions, createInitialSandboxState } from "../engine/sim-controller";
import type { SandboxStore } from "./types";

export const useSandboxStore = create<SandboxStore>()((set, get, api) => ({
  ...createInitialSandboxState(),
  ...createEngineActions(set, get, api),
}));

export type { SandboxStore } from "./types";

/*
 * Dev aid: the sandbox is a dev-server-only tool, so expose the store on the
 * window for console/CDP driving ("apply this preset, launch, read the events").
 */
declare global {
  interface Window {
    __onboardingSandboxStore?: typeof useSandboxStore;
  }
}
window.__onboardingSandboxStore = useSandboxStore;
