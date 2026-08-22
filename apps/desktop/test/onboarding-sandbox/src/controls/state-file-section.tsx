/*
 * Live editor for the fake ~/.local/state/ghostex/gpui-first-run-onboarding-state.json.
 * Fields the engine burned during the CURRENT launch are highlighted, which is the
 * whole point of the exercise: the flags are persisted BEFORE the modals open.
 */
import { useLayoutEffect, useRef, useState } from "react";
import { useSandboxStore } from "../state/store";
import {
  FIRST_LAUNCH_SETUP_SEEN_REVISION,
  HIGHLIGHTED_FEATURES_SEEN_REVISION,
  type FirstRunOnboardingStateFile,
} from "../state/types";
import { Btn, Row, Section, SubGroup, Toggle } from "./control-primitives";

type BooleanStateFileKey =
  | "tipsAndTricksSeen"
  | "osIntegrationOnboardingSeen"
  | "firstLaunchSetupComplete"
  | "windowsTerminalSetupComplete";
type RevisionStateFileKey = "highlightedFeaturesSeenRevision" | "firstLaunchSetupSeenRevision";

/*
 * Patches are built by keyed assignment rather than a computed-key object
 * literal: TypeScript widens `{ [unionKey]: value }` to a string index
 * signature, which does not satisfy Partial<FirstRunOnboardingStateFile>.
 */
function booleanPatch(key: BooleanStateFileKey, value: boolean): Partial<FirstRunOnboardingStateFile> {
  const patch: Partial<FirstRunOnboardingStateFile> = {};
  patch[key] = value;
  return patch;
}

function revisionPatch(
  key: RevisionStateFileKey,
  value: string | null,
): Partial<FirstRunOnboardingStateFile> {
  const patch: Partial<FirstRunOnboardingStateFile> = {};
  patch[key] = value;
  return patch;
}

const BOOLEAN_FIELDS: readonly {
  key: BooleanStateFileKey;
  label: string;
  hint?: string;
}[] = [
  { key: "tipsAndTricksSeen", label: "tipsAndTricksSeen", hint: "burned silently on first run" },
  {
    key: "osIntegrationOnboardingSeen",
    label: "osIntegrationOnboardingSeen",
    hint: "false ⇒ OS-integration toast",
  },
  { key: "firstLaunchSetupComplete", label: "firstLaunchSetupComplete" },
  { key: "windowsTerminalSetupComplete", label: "windowsTerminalSetupComplete" },
];

const REVISION_FIELDS: readonly {
  key: RevisionStateFileKey;
  label: string;
  current: string;
  hint: string;
}[] = [
  {
    key: "highlightedFeaturesSeenRevision",
    label: "highlightedFeaturesSeenRevision",
    current: HIGHLIGHTED_FEATURES_SEEN_REVISION,
    hint: "burned silently — the Discover tour is never auto-shown",
  },
  {
    key: "firstLaunchSetupSeenRevision",
    label: "firstLaunchSetupSeenRevision",
    current: FIRST_LAUNCH_SETUP_SEEN_REVISION,
    hint: "mismatch ⇒ the tutorial video opens",
  },
];

function changedKeys(
  baseline: FirstRunOnboardingStateFile,
  current: FirstRunOnboardingStateFile,
): Set<string> {
  const changed = new Set<string>();
  for (const key of Object.keys(current) as (keyof FirstRunOnboardingStateFile)[]) {
    if (baseline[key] !== current[key]) changed.add(key);
  }
  return changed;
}

export function StateFileSection() {
  const stateFile = useSandboxStore((s) => s.stateFile);
  const launchCount = useSandboxStore((s) => s.launchCount);
  const patchStateFile = useSandboxStore((s) => s.patchStateFile);
  const wipeStateFile = useSandboxStore((s) => s.wipeStateFile);

  /*
   * Snapshot the file as it looked when the current launch started so the
   * flags the startup pass burns light up. useLayoutEffect (not useEffect) so
   * the snapshot is taken before any engine timer of the new launch fires.
   */
  const [baseline, setBaseline] = useState<FirstRunOnboardingStateFile>(stateFile);
  const lastLaunchRef = useRef(launchCount);
  const latestStateFileRef = useRef(stateFile);
  latestStateFileRef.current = stateFile;
  useLayoutEffect(() => {
    if (lastLaunchRef.current === launchCount) return;
    lastLaunchRef.current = launchCount;
    setBaseline(latestStateFileRef.current);
  }, [launchCount]);

  const changed = changedKeys(baseline, stateFile);

  return (
    <Section
      badge={changed.size > 0 ? `${changed.size} changed` : undefined}
      defaultOpen
      id="state-file"
      title="Persisted state file"
    >
      <p className="cp-note cp-note--info">
        <code>gpui-first-run-onboarding-state.json</code> — survives quit/relaunch. Highlighted rows
        changed during launch #{launchCount}.
      </p>
      {BOOLEAN_FIELDS.map((field) => (
        <Row changed={changed.has(field.key)} hint={field.hint} key={field.key} label={field.label}>
          <Toggle
            checked={stateFile[field.key]}
            onChange={(next) => patchStateFile(booleanPatch(field.key, next))}
          />
        </Row>
      ))}
      {REVISION_FIELDS.map((field) => {
        const value = stateFile[field.key];
        return (
          <div
            className={changed.has(field.key) ? "cp-revision is-changed" : "cp-revision"}
            key={field.key}
          >
            <div className="cp-row-label">
              <span>{field.label}</span>
              <span className="cp-row-hint">{field.hint}</span>
            </div>
            <code className={value === null ? "cp-revision-value is-null" : "cp-revision-value"}>
              {value === null ? "null" : value}
            </code>
            <div className="cp-btn-row">
              <Btn
                disabled={value === field.current}
                onClick={() => patchStateFile(revisionPatch(field.key, field.current))}
                title={field.current}
                tone="ghost"
              >
                Set current
              </Btn>
              <Btn
                disabled={value === null}
                onClick={() => patchStateFile(revisionPatch(field.key, null))}
                tone="ghost"
              >
                Clear
              </Btn>
              <Btn
                disabled={value === "stale-revision"}
                onClick={() => patchStateFile(revisionPatch(field.key, "stale-revision"))}
                title="Simulate an upgrading user who saw an older revision"
                tone="ghost"
              >
                Stale
              </Btn>
            </div>
          </div>
        );
      })}
      <SubGroup id="state-file-json" title="Raw JSON">
        <pre className="cp-json">{JSON.stringify(stateFile, null, 2)}</pre>
      </SubGroup>
      <div className="cp-btn-row">
        <Btn
          onClick={wipeStateFile}
          title="Delete the file: the next launch is a brand-new user"
          tone="danger"
          wide
        >
          Wipe (fresh user)
        </Btn>
      </div>
    </Section>
  );
}
