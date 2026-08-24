import {
  IconAlertTriangle,
  IconCopy,
  IconLoader2,
  IconPlayerPlay,
} from "@tabler/icons-react";
import { useState } from "react";
import { Button } from "@/packages/components/ui/button";
import {
  Card,
  CardContent,
} from "@/packages/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/packages/components/ui/dialog";
import {
  type RunnableBeadsMigrationOption,
  projectBoardCommandRunKey,
} from "./types";

export type RemoteMigrateGateOption = {
  commands: string[];
  id: string;
  risk: string;
  when: string;
};

export type RemoteMigrateGate = {
  currentVersion?: number;
  decision?: string;
  docs?: string;
  fallbackReason?: string;
  latestVersion?: number;
  options: RemoteMigrateGateOption[];
  pending?: number;
};

export function parseRemoteMigrateGate(message: string): RemoteMigrateGate | undefined {
  try {
    const payload = JSON.parse(message) as unknown;
    if (typeof payload !== "object" || payload === null) {
      return undefined;
    }
    const gateValue = (payload as Record<string, unknown>).remote_migrate_gate;
    if (typeof gateValue !== "object" || gateValue === null) {
      return undefined;
    }
    const gate = gateValue as Record<string, unknown>;
    const options = Array.isArray(gate.options)
      ? gate.options.flatMap((option): RemoteMigrateGateOption[] => {
          if (typeof option !== "object" || option === null) {
            return [];
          }
          const value = option as Record<string, unknown>;
          const id = typeof value.id === "string" ? value.id : "";
          const commands = Array.isArray(value.commands)
            ? value.commands.filter((command): command is string => typeof command === "string")
            : [];
          if (!id || commands.length === 0) {
            return [];
          }
          return [{
            commands,
            id,
            risk: typeof value.risk === "string" ? value.risk : "",
            when: typeof value.when === "string" ? value.when : "",
          }];
        })
      : [];
    if (options.length === 0) {
      return undefined;
    }
    return {
      currentVersion: typeof gate.current_version === "number" ? gate.current_version : undefined,
      decision: typeof gate.decision === "string" ? gate.decision : undefined,
      docs: typeof gate.docs === "string" ? gate.docs : undefined,
      fallbackReason: typeof gate.fallback_reason === "string" ? gate.fallback_reason : undefined,
      latestVersion: typeof gate.latest_version === "number" ? gate.latest_version : undefined,
      options,
      pending: typeof gate.pending === "number" ? gate.pending : undefined,
    };
  } catch {
    return undefined;
  }
}

export function beadsRejectionToastId(ticketId: string): string {
  return `project-board-beads-rejection:${ticketId}`;
}

export function formatIssueIdList(issueIds: string[]): string {
  if (issueIds.length <= 2) {
    return issueIds.join(" and ");
  }
  return `${issueIds.slice(0, -1).join(", ")}, and ${issueIds[issueIds.length - 1]}`;
}

export function currentBeadsMigrationDocsUrl(url: string | undefined): string | undefined {
  return url?.replace("/website/docs/getting-started/upgrading.md", "/docs/getting-started/upgrading.md");
}

export function runnableBeadsMigrationOption(id: string): RunnableBeadsMigrationOption | undefined {
  return id === "migrate" ||
    id === "adopt" ||
    id === "adopt-fast-forward" ||
    id === "reconcile-fork"
    ? id
    : undefined;
}

export function beadsMigrationOptionLabel(id: string): string {
  switch (id) {
    case "migrate":
      return "Migrate this clone";
    case "adopt":
      return "Adopt the remote";
    case "adopt-fast-forward":
      return "Adopt the remote (lossless)";
    case "reconcile-fork":
      return "Back up and adopt the canonical remote";
    default:
      return id;
  }
}

export function RemoteMigrateGateNotice({
  canRunBeadsCommands,
  gate,
  onRunBeadsMigration,
  runningCommand,
}: {
  canRunBeadsCommands: boolean;
  gate: RemoteMigrateGate;
  onRunBeadsMigration: (option: RunnableBeadsMigrationOption) => void;
  runningCommand: string;
}) {
  const [confirmingOption, setConfirmingOption] = useState<RemoteMigrateGateOption>();
  const docsUrl = currentBeadsMigrationDocsUrl(gate.docs);
  const versionSummary =
    gate.currentVersion !== undefined && gate.latestVersion !== undefined
      ? `Schema v${gate.currentVersion} → v${gate.latestVersion}${gate.pending ? ` (${gate.pending} pending)` : ""}`
      : "A schema migration is pending.";
  const explanation = gate.decision === "adopt" || gate.decision === "adopt-ff"
    ? "The remote is already migrated. This clone must adopt that result; migrating it independently would fork the board schema."
    : gate.decision === "fork-skew"
      ? "This clone and its remote have already applied different migration content. Choose a canonical clone before replacing any local database."
      : "Ghostex could not prove which clone should migrate. Exactly one clone may migrate and publish; every other clone must adopt that result.";
  const fallback = gate.fallbackReason === "unreadable-remote-state"
    ? "The cached remote schema state could not be read, so Ghostex cannot safely choose for you."
    : gate.fallbackReason === "below-convergence-floor"
      ? "This database predates Beads' merge-safe migration floor, so unattended migration is unsafe."
      : "";
  const confirmingOptionId = confirmingOption
    ? runnableBeadsMigrationOption(confirmingOption.id)
    : undefined;
  const confirmationText = confirmingOptionId === "migrate"
    ? "Confirm that this is the one designated clone allowed to migrate and publish. Running this on another clone can fork the shared board schema unrecoverably."
    : confirmingOptionId === "adopt-fast-forward"
      ? "Beads reports that this clone can adopt the migrated remote without losing local work. Confirm before replacing the local database."
      : confirmingOptionId === "reconcile-fork"
        ? "This first exports a backup, then replaces this clone's database from the canonical remote. Confirm that the canonical clone has already been chosen."
        : "Adopting re-clones and replaces this local database. Confirm that needed local work has been pushed or exported first.";
  return (
    <>
      <Card className="project-board-notice" data-kind="migration" role="alert" size="sm">
        <CardContent>
          <div className="project-board-notice-icon" aria-hidden="true">
            <IconAlertTriangle />
          </div>
          <div className="project-board-notice-body">
          <strong>Project board migration needs coordination</strong>
          <p>{versionSummary}</p>
          <p>First install or update to the latest Beads release on every clone, then follow one coordinated migration path below.</p>
          <p>{explanation}</p>
          {fallback ? <p>{fallback}</p> : null}
          <div className="project-board-migration-options">
            {gate.options.map((option) => {
              const commands = option.commands;
              const label = beadsMigrationOptionLabel(option.id);
              const runnableOption = runnableBeadsMigrationOption(option.id);
              const runKey = runnableOption
                ? projectBoardCommandRunKey("runBeadsMigration", runnableOption)
                : "";
              const isRunning = runKey === runningCommand;
              return (
                <div className="project-board-migration-option" key={option.id}>
                  <strong>{label}</strong>
                  {option.when ? <p>Use only when {option.when}.</p> : null}
                  {option.risk ? <p className="project-board-migration-risk">Risk: {option.risk}.</p> : null}
                  <div className="project-board-notice-command">
                    <code>{commands.join(" && ")}</code>
                    <Button
                      aria-label={`Copy ${label.toLowerCase()} commands`}
                      onClick={() => void navigator.clipboard.writeText(commands.join("\n"))}
                      size="icon-sm"
                      type="button"
                      variant="ghost"
                    >
                      <IconCopy />
                    </Button>
                    {canRunBeadsCommands && runnableOption ? (
                      <Button
                        className="project-board-notice-run-button"
                        disabled={Boolean(runningCommand)}
                        onClick={() => setConfirmingOption(option)}
                        size="sm"
                        type="button"
                        variant="ghost"
                      >
                        {isRunning ? (
                          <IconLoader2 className="animate-spin" />
                        ) : (
                          <IconPlayerPlay />
                        )}
                        Run in Terminal
                      </Button>
                    ) : null}
                  </div>
                </div>
              );
            })}
          </div>
          {docsUrl ? (
            <a href={docsUrl} rel="noreferrer" target="_blank">Read the Beads multi-clone migration guide</a>
          ) : null}
          </div>
        </CardContent>
      </Card>
      <Dialog
        open={Boolean(confirmingOptionId)}
        onOpenChange={(open) => {
          if (!open) {
            setConfirmingOption(undefined);
          }
        }}
      >
        <DialogContent className="project-ticket-dialog gap-4 p-5">
          <DialogHeader className="gap-1">
            <DialogTitle className="text-[15px] font-normal">
              Confirm {confirmingOption ? beadsMigrationOptionLabel(confirmingOption.id) : "Beads command"}
            </DialogTitle>
            <DialogDescription className="text-xs font-normal text-muted-foreground">
              {confirmationText}
            </DialogDescription>
          </DialogHeader>
          {confirmingOption ? (
            <div className="project-board-notice-command project-board-confirm-command">
              <code>{confirmingOption.commands.join(" && ")}</code>
            </div>
          ) : null}
          <DialogFooter>
            <Button onClick={() => setConfirmingOption(undefined)} type="button" variant="outline">
              Cancel
            </Button>
            <Button
              disabled={!confirmingOptionId || Boolean(runningCommand)}
              onClick={() => {
                if (!confirmingOptionId) {
                  return;
                }
                onRunBeadsMigration(confirmingOptionId);
                setConfirmingOption(undefined);
              }}
              type="button"
            >
              <IconPlayerPlay />
              Confirm and Run in Terminal
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

export function ProjectBoardNotice({
  canRunBeadsCommands,
  message,
  onInstallOrUpdateBeads,
  onInitializeBeads,
  onRunBeadsMigration,
  runningCommand,
}: {
  canRunBeadsCommands: boolean;
  message: string;
  onInstallOrUpdateBeads: () => void;
  onInitializeBeads: () => void;
  onRunBeadsMigration: (option: RunnableBeadsMigrationOption) => void;
  runningCommand: string;
}) {
  const remoteMigrateGate = parseRemoteMigrateGate(message);
  if (remoteMigrateGate) {
    return (
      <RemoteMigrateGateNotice
        canRunBeadsCommands={canRunBeadsCommands}
        gate={remoteMigrateGate}
        onRunBeadsMigration={onRunBeadsMigration}
        runningCommand={runningCommand}
      />
    );
  }
  /*
   * CDXC:ProjectBoardBeadsSchema 2026-08-08:
   * A database/schema failure proves that Beads found a workspace, so generic
   * words such as "database" or ".beads" must never turn it into a `bd init`
   * instruction. Only Beads' explicit missing-workspace messages belong to the
   * initialization notice.
   */
  const isMissingProject =
    /not initialized|no storage|not a beads(?: workspace| project| repository)|no beads database found|run ['"]?bd init['"]?/i.test(
      message,
    );
  const isMissingBeads =
    !isMissingProject &&
    /bd was not found|beads cli|executable|command not found|not found: bd|bd: not found|env: bd: no such file|cannot find/i.test(message);
  const latestBeadsInstallCommand = "curl -fsSL https://raw.githubusercontent.com/gastownhall/beads/main/scripts/install.sh | bash";
  const command = isMissingProject ? "bd init" : latestBeadsInstallCommand;
  const title = isMissingBeads
    ? "Beads CLI unavailable"
    : isMissingProject
      ? "Initialize Beads for this project"
      : "Project board unavailable";
  const bodyLines = isMissingBeads
    ? [
        "Ghostex uses the Beads CLI installed in the environment running this project—macOS, Linux, or the selected WSL distribution.",
        "Install the latest Beads release in that environment and ensure bd is available on its PATH, then refresh the board.",
      ]
    : isMissingProject
      ? [
          "This project does not have a Beads workspace yet. Run this once from the project root; Ghostex will refresh the board when it finishes.",
        ]
      : [message, "Update Beads to the latest release, then retry. If this is a remote-backed migration, follow the coordinated migration instructions instead of migrating multiple clones independently."];
  return (
    <Card
      className="project-board-notice"
      data-kind={isMissingBeads ? "install" : isMissingProject ? "init" : "error"}
      role="status"
      size="sm"
    >
      <CardContent>
        {/*
          CDXC:ProjectBoard 2026-05-28-15:27:
          Initialization is a normal first-run state for Beads-backed projects, not an app failure.
          Present bd init as an explanatory setup callout with a direct Run action so users can initialize the project in a visible command pane before the board reloads.

          CDXC:ProjectBoard 2026-05-29-15:49:
          Missing-Beads setup should use the same polished notice shell but stay intentionally terse: one header and two lines below.
          Explain why Beads is required and keep copy/run controls in the single command row.

          CDXC:ProjectBoardSystemBeads 2026-08-12:
          Project/Kanban and shell agents intentionally use the same machine-installed `bd`. Missing and command-failure notices therefore direct the operator to install or update Beads instead of repairing Ghostex app resources.

          CDXC:ProjectBoardBeadsCommands 2026-08-14:
          Local setup and update commands run visibly in the active project's command pane. The renderer sends only a fixed action selector; Rust owns the command literal and completion refresh.
        */}
        <div className="project-board-notice-icon" aria-hidden="true">
          <IconAlertTriangle />
        </div>
        <div className="project-board-notice-body">
          <strong>{title}</strong>
          {bodyLines.map((line) => (
            <p key={line}>{line}</p>
          ))}
          {command ? (
            <div className="project-board-notice-command">
              <code>{command}</code>
              {isMissingProject && canRunBeadsCommands ? (
                <Button
                  className="project-board-notice-run-button"
                  disabled={Boolean(runningCommand)}
                  onClick={onInitializeBeads}
                  size="sm"
                  type="button"
                  variant="ghost"
                >
                  {runningCommand === "initializeBeads" ? (
                    <IconLoader2 className="animate-spin" />
                  ) : (
                    <IconPlayerPlay />
                  )}
                  Run in Terminal
                </Button>
              ) : (
                <>
                  <Button
                    aria-label={`Copy ${command}`}
                    onClick={() => void navigator.clipboard.writeText(command)}
                    size="icon-sm"
                    type="button"
                    variant="ghost"
                  >
                    <IconCopy />
                  </Button>
                  {!isMissingProject && canRunBeadsCommands ? (
                    <Button
                      className="project-board-notice-run-button"
                      disabled={Boolean(runningCommand)}
                      onClick={onInstallOrUpdateBeads}
                      size="sm"
                      type="button"
                      variant="ghost"
                    >
                      {runningCommand === "installOrUpdateBeads" ? (
                        <IconLoader2 className="animate-spin" />
                      ) : (
                        <IconPlayerPlay />
                      )}
                      Run in Terminal
                    </Button>
                  ) : null}
                </>
              )}
            </div>
          ) : null}
          {isMissingBeads || !isMissingProject ? (
            <a href="https://github.com/gastownhall/beads/blob/main/docs/INSTALLING.md" rel="noreferrer" target="_blank">
              Beads install and update guide
            </a>
          ) : null}
        </div>
      </CardContent>
    </Card>
  );
}