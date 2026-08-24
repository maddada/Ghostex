/**
 * CDXC:ProjectBoardDialogRedesign 2026-08-24:
 * The automation create/edit dialog moved out of project-board-app.tsx so the
 * Codex-style redesign can be rendered from Storybook with mock props. It is
 * pure presentation over the automation draft state and its save callback.
 */
import type { ComponentProps, ReactNode } from "react";
import { Button } from "@/packages/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/packages/components/ui/dialog";
import { Input } from "@/packages/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/packages/components/ui/select";
import { Switch } from "@/packages/components/ui/switch";
import { Textarea } from "@/packages/components/ui/textarea";
import type {
  AutomationExecutionMode,
  ProjectAutomationsBridgeState,
} from "@/packages/shared/automations";
import type { ProjectBoardConversationState } from "@/packages/shared/bead-conversation-links";
import { AutomationAgentOptionLabel } from "./agent-labels";
import {
  AUTOMATION_SCHEDULE_PRESETS,
  AUTOMATION_TIMER_UNIT_OPTIONS,
  AUTOMATION_WEEKDAY_OPTIONS,
  type AutomationDraft,
  type AutomationScheduleMode,
  type AutomationTimerUnit,
} from "./automations-drafts";

type SelectItems = ComponentProps<typeof Select>["items"];

function AutomationSection({
  children,
  title,
}: {
  children: ReactNode;
  title: string;
}) {
  return (
    <div className="grid gap-2.5">
      <div className="project-automation-form-section-title">{title}</div>
      {children}
    </div>
  );
}

export function AutomationDialog({
  automationActionId,
  automationAgentSelectItems,
  automationConversationState,
  automationDraft,
  automationDraftCanUseWorktrees,
  automationDraftWorktreeUnavailableReason,
  automationProjectSelectItems,
  automationScheduleSelectItems,
  automationSessionSelectItems,
  automationState,
  automationTimerUnitSelectItems,
  automationWeekdaySelectItems,
  isAutomationGlobalScope,
  onOpenChange,
  onProjectChange,
  onSave,
  open,
  projectName,
  setAutomationDraft,
}: {
  automationActionId: string;
  automationAgentSelectItems: SelectItems;
  automationConversationState: ProjectBoardConversationState;
  automationDraft: AutomationDraft;
  automationDraftCanUseWorktrees: boolean;
  automationDraftWorktreeUnavailableReason?: string;
  automationProjectSelectItems: SelectItems;
  automationScheduleSelectItems: SelectItems;
  automationSessionSelectItems: SelectItems;
  automationState: ProjectAutomationsBridgeState;
  automationTimerUnitSelectItems: SelectItems;
  automationWeekdaySelectItems: SelectItems;
  isAutomationGlobalScope: boolean;
  onOpenChange: (open: boolean) => void;
  onProjectChange: (projectId: string) => void;
  onSave: () => void;
  open: boolean;
  projectName: string;
  setAutomationDraft: (update: (current: AutomationDraft) => AutomationDraft) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="project-ticket-dialog project-automation-dialog gap-4 p-5">
        <DialogHeader className="gap-1">
          <DialogTitle className="text-[15px] font-normal">
            {automationDraft.id ? "Edit automation" : "Create automation"}
          </DialogTitle>
          <DialogDescription className="text-xs font-normal text-muted-foreground">
            {isAutomationGlobalScope
              ? "Schedule agent work once or repeatedly for a selected project."
              : `Schedule agent work once or repeatedly for ${projectName}.`}
          </DialogDescription>
        </DialogHeader>
        <div className="project-ticket-dialog-body project-automation-form vertical-scroll-fade-mask">
          {/*
           * CDXC:ProjectAutomations 2026-06-09-10:30:
           * Automation setup is scoped to the Project board's current project, so the create/edit dialog drops project switching and keeps dropdown widths aligned at 250px for agent, schedule, weekday, and thread-session fields.
           *
           * CDXC:Automations 2026-06-30-11:05:
           * The Quick-level global Automations page shows all projects, so its create/edit dialog restores a Project selector. Project-scoped Automate pages keep the original no-project-switch form.
           */}
          <label className="project-automation-field-full">
            <span>Name</span>
            <Input
              onChange={(event) => {
                const name = event.currentTarget.value;
                setAutomationDraft((current) => ({ ...current, name }));
              }}
              value={automationDraft.name}
            />
          </label>
          <div className="project-automation-form-grid">
            {isAutomationGlobalScope ? (
              <label>
                <span>Project</span>
                <Select
                  items={automationProjectSelectItems}
                  onValueChange={onProjectChange}
                  value={automationDraft.projectId}
                >
                  <SelectTrigger className="project-automation-select">
                    <SelectValue placeholder="Choose project" />
                  </SelectTrigger>
                  <SelectContent>
                    {automationState.projects.map((project) => (
                      <SelectItem key={project.projectId} value={project.projectId}>
                        {project.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
            ) : null}
            <label>
              <span>Agent</span>
              <Select
                disabled={automationState.agents.length === 0}
                items={automationAgentSelectItems}
                onValueChange={(value) =>
                  setAutomationDraft((current) => ({ ...current, agentId: value }))
                }
                value={automationDraft.agentId}
              >
                <SelectTrigger className="project-automation-select">
                  <SelectValue
                    placeholder={
                      automationState.agents.length === 0 ? "No agents configured" : "Choose agent"
                    }
                  />
                </SelectTrigger>
                <SelectContent>
                  {automationState.agents.map((agent) => (
                    <SelectItem key={agent.agentId} value={agent.agentId}>
                      <AutomationAgentOptionLabel agent={agent} />
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </label>
          </div>
          <AutomationSection title="Timing">
            <div className="project-automation-segmented" role="group" aria-label="Schedule type">
              {[
                ["repeat", "Repeat"],
                ["timer", "Timer"],
                ["date", "Date"],
              ].map(([value, label]) => (
                <button
                  data-active={automationDraft.scheduleMode === value}
                  key={value}
                  onClick={() =>
                    setAutomationDraft((current) => ({
                      ...current,
                      scheduleMode: value as AutomationScheduleMode,
                    }))
                  }
                  type="button"
                >
                  {label}
                </button>
              ))}
            </div>
            {automationDraft.scheduleMode === "repeat" ? (
              <div className="project-automation-form-grid">
                <label>
                  <span>Repeat</span>
                  <Select
                    items={automationScheduleSelectItems}
                    onValueChange={(value) =>
                      setAutomationDraft((current) => ({
                        ...current,
                        schedulePreset: value as AutomationDraft["schedulePreset"],
                      }))
                    }
                    value={automationDraft.schedulePreset}
                  >
                    <SelectTrigger className="project-automation-select">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {AUTOMATION_SCHEDULE_PRESETS.map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                          {option.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </label>
                {automationDraft.schedulePreset === "weekly" ? (
                  <label>
                    <span>Day</span>
                    <Select
                      items={automationWeekdaySelectItems}
                      onValueChange={(value) =>
                        setAutomationDraft((current) => ({ ...current, weeklyDay: value }))
                      }
                      value={automationDraft.weeklyDay}
                    >
                      <SelectTrigger className="project-automation-select">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {AUTOMATION_WEEKDAY_OPTIONS.map((day, index) => (
                          <SelectItem key={day} value={String(index)}>
                            {day}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </label>
                ) : null}
                {automationDraft.schedulePreset === "daily" ||
                automationDraft.schedulePreset === "weekly" ||
                automationDraft.schedulePreset === "weekdays" ? (
                  <label>
                    <span>Time</span>
                    <Input
                      className="project-automation-select"
                      onChange={(event) => {
                        const scheduleTime = event.currentTarget.value;
                        setAutomationDraft((current) => ({
                          ...current,
                          scheduleTime,
                        }));
                      }}
                      type="time"
                      value={automationDraft.scheduleTime}
                    />
                  </label>
                ) : null}
              </div>
            ) : automationDraft.scheduleMode === "timer" ? (
              <div className="project-automation-form-grid">
                <label>
                  <span>Run in</span>
                  <Input
                    className="project-automation-select"
                    min="1"
                    onChange={(event) => {
                      const timerAmount = event.currentTarget.value;
                      setAutomationDraft((current) => ({
                        ...current,
                        timerAmount,
                      }));
                    }}
                    step="1"
                    type="number"
                    value={automationDraft.timerAmount}
                  />
                </label>
                <label>
                  <span>Unit</span>
                  <Select
                    items={automationTimerUnitSelectItems}
                    onValueChange={(value) =>
                      setAutomationDraft((current) => ({
                        ...current,
                        timerUnit: value as AutomationTimerUnit,
                      }))
                    }
                    value={automationDraft.timerUnit}
                  >
                    <SelectTrigger className="project-automation-select">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {AUTOMATION_TIMER_UNIT_OPTIONS.map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                          {option.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </label>
              </div>
            ) : (
              <label className="project-automation-field-full">
                <span>Run on</span>
                <Input
                  onChange={(event) => {
                    const runAt = event.currentTarget.value;
                    setAutomationDraft((current) => ({
                      ...current,
                      runAt,
                    }));
                  }}
                  type="datetime-local"
                  value={automationDraft.runAt}
                />
              </label>
            )}
            {automationDraft.scheduleMode === "repeat" &&
            automationDraft.schedulePreset === "cron" ? (
              <label className="project-automation-field-full">
                <span>Cron</span>
                <Input
                  onChange={(event) => {
                    const cronExpression = event.currentTarget.value;
                    setAutomationDraft((current) => ({
                      ...current,
                      cronExpression,
                    }));
                  }}
                  placeholder="*/15 * * * *"
                  value={automationDraft.cronExpression}
                />
              </label>
            ) : null}
          </AutomationSection>
          <AutomationSection title="Execution">
            <div className="project-automation-segmented" role="group" aria-label="Execution mode">
              {[
                ["worktree", "Worktree"],
                ["local", "Local"],
                ["thread", "Thread"],
              ].map(([value, label]) => {
                const disabled = value === "worktree" && !automationDraftCanUseWorktrees;
                return (
                  <button
                    data-active={automationDraft.executionKind === value}
                    disabled={disabled}
                    key={value}
                    onClick={() =>
                      setAutomationDraft((current) => ({
                        ...current,
                        executionKind: value as AutomationExecutionMode["kind"],
                      }))
                    }
                    type="button"
                  >
                    {label}
                  </button>
                );
              })}
            </div>
            {!automationDraftCanUseWorktrees && automationDraftWorktreeUnavailableReason ? (
              <p className="m-0 -mt-1 text-xs font-normal leading-relaxed text-muted-foreground">
                {automationDraftWorktreeUnavailableReason}
              </p>
            ) : null}
            {automationDraft.executionKind === "worktree" ? (
              <label>
                <span>Setup command</span>
                <Input
                  onChange={(event) => {
                    const setupCommand = event.currentTarget.value;
                    setAutomationDraft((current) => ({
                      ...current,
                      setupCommand,
                    }));
                  }}
                  placeholder="Use project worktree command"
                  value={automationDraft.setupCommand}
                />
              </label>
            ) : null}
            {automationDraft.executionKind === "thread" ? (
              <div className="project-automation-form-grid">
                <label>
                  <span>Session</span>
                  <Select
                    items={automationSessionSelectItems}
                    onValueChange={(value) =>
                      setAutomationDraft((current) => ({ ...current, threadSessionId: value }))
                    }
                    value={automationDraft.threadSessionId}
                  >
                    <SelectTrigger className="project-automation-select">
                      <SelectValue placeholder="Choose session" />
                    </SelectTrigger>
                    <SelectContent>
                      {automationConversationState.sessions.map((session) => (
                        <SelectItem key={session.sessionId} value={session.sessionId}>
                          {session.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </label>
                <label>
                  <span>Expires</span>
                  <Input
                    className="project-automation-select"
                    onChange={(event) => {
                      const expiresAt = event.currentTarget.value;
                      setAutomationDraft((current) => ({
                        ...current,
                        expiresAt,
                      }));
                    }}
                    type="datetime-local"
                    value={automationDraft.expiresAt}
                  />
                </label>
              </div>
            ) : null}
          </AutomationSection>
          <label className="project-automation-prompt-field">
            <span>Prompt</span>
            <Textarea
              onChange={(event) => {
                const prompt = event.currentTarget.value;
                setAutomationDraft((current) => ({ ...current, prompt }));
              }}
              value={automationDraft.prompt}
            />
          </label>
          <div className="flex flex-row items-center gap-2 text-[13px] font-normal text-foreground/85">
            <Switch
              checked={automationDraft.enabled}
              onCheckedChange={(enabled: boolean) => {
                setAutomationDraft((current) => ({ ...current, enabled }));
              }}
              size="sm"
            />
            <span>Enabled</span>
          </div>
        </div>
        <DialogFooter className="project-ticket-dialog-footer">
          <Button
            className="ml-auto"
            onClick={() => onOpenChange(false)}
            type="button"
            variant="ghost"
          >
            Cancel
          </Button>
          <Button disabled={Boolean(automationActionId)} onClick={onSave} type="button">
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
