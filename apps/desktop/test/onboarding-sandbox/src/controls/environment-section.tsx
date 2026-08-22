/*
 * Environment editor: every SimEnvState field. patchEnv() is a shallow top-level
 * merge, so nested groups (ghostexCli, cuaDriver, gxserver, settings, timing,
 * bundledSkills) are always passed as a fully rebuilt object.
 */
import {
  BUNDLED_SKILL_IDS,
  PRIORITY_AGENT_IDS,
  SIM_AGENT_IDS,
  type BundledSkillId,
  type GxserverHealthScenario,
  type SimAgentId,
  type SimAgentState,
  type SimEnvState,
  type SimHookState,
} from "../state/types";
import { useSandboxStore } from "../state/store";
import { Btn, MsSlider, Row, Section, SelectField, Stepper, SubGroup, Toggle } from "./control-primitives";

const HOOK_STATE_OPTIONS: readonly { value: SimHookState; label: string }[] = [
  { value: "notInstalled", label: "not installed" },
  { value: "installed", label: "installed" },
  { value: "outdated", label: "outdated" },
];

const GXSERVER_SCENARIO_OPTIONS: readonly { value: GxserverHealthScenario; label: string }[] = [
  { value: "healthyToolsAvailable", label: "healthy, tools available" },
  { value: "healthyToolsUnavailable", label: "healthy, tools unavailable" },
  { value: "buildMismatch", label: "build mismatch" },
  { value: "protocolMismatch", label: "protocol mismatch" },
  { value: "spawnFailure", label: "spawn failure" },
];

const AGENT_LABELS: Partial<Record<SimAgentId, string>> = {
  codex: "Codex",
  claude: "Claude",
  opencode: "OpenCode",
  pi: "Pi",
  cursor: "Cursor",
  gemini: "Gemini",
  kiro: "Kiro",
  copilot: "Copilot",
  droid: "Droid",
  grok: "Grok",
  antigravity: "Antigravity",
  amp: "Amp",
  omp: "Omp",
  rovodev: "Rovo Dev",
  "hermes-agent": "Hermes",
  codebuddy: "CodeBuddy",
  qoder: "Qoder",
};

const SKILL_LABELS: Record<BundledSkillId, string> = {
  browser: "browser",
  embeddedBrowser: "embedded browser",
  computerUse: "computer use",
  agentOrchestration: "agent orchestration",
  fable56Orchestration: "fable 5.6 orchestration",
  findPrevSession: "find prev session",
  generateTitle: "generate title",
  moveCodexSession: "move codex session",
};

/** Mirrors server/src/agent_hooks/api.rs read_hook_status (display only). */
function deriveAgentStatus(state: SimAgentState): string {
  if (!state.cliInstalled) return "cliMissing";
  if (state.hookState === "installed") return "installed";
  if (state.hookState === "outdated") return "updateRequired";
  return "missing";
}

function AgentRow({ agentId }: { agentId: SimAgentId }) {
  const agent = useSandboxStore((s) => s.env.agents[agentId]);
  const setAgentState = useSandboxStore((s) => s.setAgentState);
  const status = deriveAgentStatus(agent);
  return (
    <div className="cp-agent-row">
      <span className="cp-agent-name">{AGENT_LABELS[agentId] ?? agentId}</span>
      <Toggle
        checked={agent.cliInstalled}
        label="CLI"
        onChange={(next) => setAgentState(agentId, { cliInstalled: next })}
        title={`${agentId} CLI installed on PATH`}
      />
      <SelectField
        onChange={(next) => setAgentState(agentId, { hookState: next })}
        options={HOOK_STATE_OPTIONS}
        title="On-disk hook state"
        value={agent.hookState}
      />
      <span className={`cp-status cp-status--${status}`}>{status}</span>
    </div>
  );
}

export function EnvironmentSection() {
  const env = useSandboxStore((s) => s.env);
  const patchEnv = useSandboxStore((s) => s.patchEnv);
  const setAgentState = useSandboxStore((s) => s.setAgentState);

  const otherAgentIds = SIM_AGENT_IDS.filter((id) => !PRIORITY_AGENT_IDS.includes(id));
  const setEveryAgent = (patch: Partial<SimAgentState>) => {
    for (const agentId of SIM_AGENT_IDS) setAgentState(agentId, patch);
  };
  const setEverySkill = (installed: boolean) => {
    const bundledSkills: SimEnvState["bundledSkills"] = { ...env.bundledSkills };
    for (const skillId of BUNDLED_SKILL_IDS) bundledSkills[skillId] = installed;
    patchEnv({ bundledSkills });
  };
  const setSkill = (skillId: BundledSkillId, installed: boolean) => {
    const bundledSkills: SimEnvState["bundledSkills"] = { ...env.bundledSkills };
    bundledSkills[skillId] = installed;
    patchEnv({ bundledSkills });
  };
  const installedSkillCount = BUNDLED_SKILL_IDS.filter((id) => env.bundledSkills[id]).length;

  return (
    <Section defaultOpen id="environment" title="Environment">
      <Row hint="drives the Windows-only firstLaunchSetup followup chain" label="Platform">
        <SelectField
          onChange={(next) => patchEnv({ platform: next })}
          options={[
            { value: "macos", label: "macOS" },
            { value: "windows", label: "Windows" },
          ]}
          value={env.platform}
        />
      </Row>

      <div className="cp-group-label">
        Priority agents
        <span className="cp-dim"> — any installed satisfies the first-launch hooks gate</span>
      </div>
      <div className="cp-agent-list">
        {PRIORITY_AGENT_IDS.map((agentId) => (
          <AgentRow agentId={agentId} key={agentId} />
        ))}
      </div>

      <SubGroup id="more-agents" title={`More agents (${otherAgentIds.length})`}>
        <div className="cp-btn-row">
          <Btn onClick={() => setEveryAgent({ cliInstalled: true, hookState: "installed" })} tone="ghost">
            All agents installed
          </Btn>
          <Btn
            onClick={() => setEveryAgent({ cliInstalled: false, hookState: "notInstalled" })}
            tone="ghost"
          >
            All agents missing
          </Btn>
        </div>
        <div className="cp-agent-list">
          {otherAgentIds.map((agentId) => (
            <AgentRow agentId={agentId} key={agentId} />
          ))}
        </div>
      </SubGroup>

      <div className="cp-group-label">Ghostex CLI</div>
      <Row label="installed">
        <Toggle
          checked={env.ghostexCli.installed}
          onChange={(next) => patchEnv({ ghostexCli: { ...env.ghostexCli, installed: next } })}
        />
      </Row>
      <Row hint="`gx` resolves to Ghostex" label="gx usable">
        <Toggle
          checked={env.ghostexCli.gxUsable}
          onChange={(next) => patchEnv({ ghostexCli: { ...env.ghostexCli, gxUsable: next } })}
        />
      </Row>
      <Row hint="another `gx` shadows ours" label="gx blocked">
        <Toggle
          checked={env.ghostexCli.gxBlockedByExistingCommand}
          onChange={(next) =>
            patchEnv({ ghostexCli: { ...env.ghostexCli, gxBlockedByExistingCommand: next } })
          }
        />
      </Row>

      <div className="cp-group-label">
        Bundled skills
        <span className="cp-dim"> — {installedSkillCount}/{BUNDLED_SKILL_IDS.length} installed</span>
      </div>
      <div className="cp-btn-row">
        <Btn onClick={() => setEverySkill(true)} tone="ghost">
          All on
        </Btn>
        <Btn onClick={() => setEverySkill(false)} tone="ghost">
          All off
        </Btn>
      </div>
      <div className="cp-checkbox-grid">
        {BUNDLED_SKILL_IDS.map((skillId) => (
          <Toggle
            checked={env.bundledSkills[skillId]}
            key={skillId}
            label={SKILL_LABELS[skillId]}
            onChange={(next) => setSkill(skillId, next)}
          />
        ))}
      </div>

      <div className="cp-group-label">cua-driver</div>
      <div className="cp-checkbox-grid">
        <Toggle
          checked={env.cuaDriver.appInstalled}
          label="app installed"
          onChange={(next) => patchEnv({ cuaDriver: { ...env.cuaDriver, appInstalled: next } })}
        />
        <Toggle
          checked={env.cuaDriver.cliInstalled}
          label="cli installed"
          onChange={(next) => patchEnv({ cuaDriver: { ...env.cuaDriver, cliInstalled: next } })}
        />
        <Toggle
          checked={env.cuaDriver.accessibilityPermission}
          label="accessibility"
          onChange={(next) =>
            patchEnv({ cuaDriver: { ...env.cuaDriver, accessibilityPermission: next } })
          }
        />
        <Toggle
          checked={env.cuaDriver.screenRecordingPermission}
          label="screen recording"
          onChange={(next) =>
            patchEnv({ cuaDriver: { ...env.cuaDriver, screenRecordingPermission: next } })
          }
        />
      </div>

      <div className="cp-group-label">gxserver</div>
      <Row hint="decides Track A's startup branch" label="Health scenario">
        <SelectField
          onChange={(next) => patchEnv({ gxserver: { ...env.gxserver, scenario: next } })}
          options={GXSERVER_SCENARIO_OPTIONS}
          value={env.gxserver.scenario}
        />
      </Row>
      <Row hint="respawn heals the scenario for later launches" label="Respawn heals">
        <Toggle
          checked={env.gxserver.respawnFixesHealth}
          onChange={(next) => patchEnv({ gxserver: { ...env.gxserver, respawnFixesHealth: next } })}
        />
      </Row>

      <div className="cp-group-label">Workspace</div>
      <Row hint="0 shows the sidebar empty state" label="Projects">
        <Stepper max={99} onChange={(next) => patchEnv({ projectCount: next })} value={env.projectCount} />
      </Row>
      <Row label="Update available">
        <Toggle checked={env.updateAvailable} onChange={(next) => patchEnv({ updateAvailable: next })} />
      </Row>

      <div className="cp-group-label">
        Settings
        <span className="cp-dim"> — the only tips notices computed at startup</span>
      </div>
      <Row label="Debugging mode">
        <Toggle
          checked={env.settings.debuggingMode}
          onChange={(next) => patchEnv({ settings: { ...env.settings, debuggingMode: next } })}
        />
      </Row>
      <Row label="Session persistence off">
        <Toggle
          checked={env.settings.sessionPersistenceOff}
          onChange={(next) =>
            patchEnv({ settings: { ...env.settings, sessionPersistenceOff: next } })
          }
        />
      </Row>

      <div className="cp-group-label">Timing</div>
      <Row hint="Track A" label="gxserver probe">
        <MsSlider
          onChange={(next) => patchEnv({ timing: { ...env.timing, gxserverProbeMs: next } })}
          value={env.timing.gxserverProbeMs}
        />
      </Row>
      <Row hint="Track B" label="CEF init">
        <MsSlider
          onChange={(next) => patchEnv({ timing: { ...env.timing, cefInitMs: next } })}
          value={env.timing.cefInitMs}
        />
      </Row>
      <Row label="Install action">
        <MsSlider
          onChange={(next) => patchEnv({ timing: { ...env.timing, installActionMs: next } })}
          value={env.timing.installActionMs}
        />
      </Row>
      <Row label="Hook status / agent">
        <MsSlider
          onChange={(next) => patchEnv({ timing: { ...env.timing, hookStatusPerAgentMs: next } })}
          value={env.timing.hookStatusPerAgentMs}
        />
      </Row>
    </Section>
  );
}
