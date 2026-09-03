import { IconAlertTriangle, IconCheck, IconCopy } from '@tabler/icons-react';
import { useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Card, CardContent } from '@/packages/components/ui/card';

function CopyBeadsFixPromptButton({ prompt }: { prompt: string }) {
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'error'>('idle');
  const label = copyState === 'copied' ? 'Prompt copied' : copyState === 'error' ? 'Copy failed' : 'Copy fix prompt';

  return (
    <Button
      aria-label='Copy a prompt for an agent to fix this Beads issue'
      className='project-board-notice-copy-prompt'
      onClick={() => {
        void navigator.clipboard.writeText(prompt).then(
          () => setCopyState('copied'),
          () => setCopyState('error')
        );
      }}
      size='sm'
      type='button'
      variant='outline'
    >
      {copyState === 'copied' ? <IconCheck aria-hidden='true' /> : <IconCopy aria-hidden='true' />}
      <span aria-live='polite'>{label}</span>
    </Button>
  );
}

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
    if (typeof payload !== 'object' || payload === null) {
      return undefined;
    }
    const gateValue = (payload as Record<string, unknown>).remote_migrate_gate;
    if (typeof gateValue !== 'object' || gateValue === null) {
      return undefined;
    }
    const gate = gateValue as Record<string, unknown>;
    const options = Array.isArray(gate.options)
      ? gate.options.flatMap((option): RemoteMigrateGateOption[] => {
          if (typeof option !== 'object' || option === null) {
            return [];
          }
          const value = option as Record<string, unknown>;
          const id = typeof value.id === 'string' ? value.id : '';
          const commands = Array.isArray(value.commands)
            ? value.commands.filter((command): command is string => typeof command === 'string')
            : [];
          if (!id || commands.length === 0) {
            return [];
          }
          return [
            {
              commands,
              id,
              risk: typeof value.risk === 'string' ? value.risk : '',
              when: typeof value.when === 'string' ? value.when : '',
            },
          ];
        })
      : [];
    if (options.length === 0) {
      return undefined;
    }
    return {
      currentVersion: typeof gate.current_version === 'number' ? gate.current_version : undefined,
      decision: typeof gate.decision === 'string' ? gate.decision : undefined,
      docs: typeof gate.docs === 'string' ? gate.docs : undefined,
      fallbackReason: typeof gate.fallback_reason === 'string' ? gate.fallback_reason : undefined,
      latestVersion: typeof gate.latest_version === 'number' ? gate.latest_version : undefined,
      options,
      pending: typeof gate.pending === 'number' ? gate.pending : undefined,
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
    return issueIds.join(' and ');
  }
  return `${issueIds.slice(0, -1).join(', ')}, and ${issueIds[issueIds.length - 1]}`;
}

export function currentBeadsMigrationDocsUrl(url: string | undefined): string | undefined {
  return url?.replace('/website/docs/getting-started/upgrading.md', '/docs/getting-started/upgrading.md');
}

export function beadsMigrationOptionLabel(id: string): string {
  switch (id) {
    case 'migrate':
      return 'Migrate this clone';
    case 'adopt':
      return 'Adopt the remote';
    case 'adopt-fast-forward':
      return 'Adopt the remote (lossless)';
    case 'reconcile-fork':
      return 'Back up and adopt the canonical remote';
    default:
      return id;
  }
}

function buildBeadsFixPrompt({ message, projectPath, title }: { message: string; projectPath: string; title: string }) {
  return `Fix this Beads Project Board issue.

Project path: ${projectPath}

Notice: ${title}

Reported issue:
${message}

Use the machine-installed bd CLI in the environment that runs this project. Diagnose and correct the underlying problem safely. Preserve all existing and unpushed Beads data. If this is a remote-backed database or migration, follow the coordinated migration gate: designate exactly one clone to migrate and publish, and have every other clone adopt that result. Do not bypass the remote migration gate or migrate multiple clones independently. Verify the fix by running a normal read such as bd status from the project path, then report what changed.`;
}

function buildRemoteMigrationFixPrompt({ gate, projectPath }: { gate: RemoteMigrateGate; projectPath: string }) {
  const schemaSummary =
    gate.currentVersion !== undefined && gate.latestVersion !== undefined
      ? `Current schema: v${gate.currentVersion}\nTarget schema: v${gate.latestVersion}${gate.pending ? `\nPending migrations: ${gate.pending}` : ''}`
      : 'Schema state: a migration is pending';
  const optionSummary = gate.options
    .map((option) => {
      const details = [
        `- ${beadsMigrationOptionLabel(option.id)}`,
        option.when ? `  Use only when: ${option.when}` : '',
        option.risk ? `  Risk: ${option.risk}` : '',
      ].filter(Boolean);
      return details.join('\n');
    })
    .join('\n');

  return `Fix this coordinated Beads Project Board migration issue.

Project path: ${projectPath}

${schemaSummary}${gate.decision ? `\nMigration decision reported by Beads: ${gate.decision}` : ''}${gate.fallbackReason ? `\nFallback reason: ${gate.fallbackReason}` : ''}

Migration choices reported by Beads:
${optionSummary}

Use the machine-installed bd CLI in the environment that runs this project. Inspect the current database and remote state before changing anything. Preserve all existing and unpushed Beads data. Follow the remote migration gate exactly: designate one canonical clone to migrate and publish, and have every other clone adopt that result. Never migrate multiple clones independently or bypass the gate. Verify the repaired board with a normal read such as bd status from the project path, then report what changed.`;
}

export function RemoteMigrateGateNotice({ gate, projectPath }: { gate: RemoteMigrateGate; projectPath: string }) {
  const docsUrl = currentBeadsMigrationDocsUrl(gate.docs);
  const versionSummary =
    gate.currentVersion !== undefined && gate.latestVersion !== undefined
      ? `Schema v${gate.currentVersion} → v${gate.latestVersion}${gate.pending ? ` (${gate.pending} pending)` : ''}`
      : 'A schema migration is pending.';
  const explanation =
    gate.decision === 'adopt' || gate.decision === 'adopt-ff'
      ? 'The remote is already migrated. This clone must adopt that result; migrating it independently would fork the board schema.'
      : gate.decision === 'fork-skew'
        ? 'This clone and its remote have already applied different migration content. Choose a canonical clone before replacing any local database.'
        : 'Ghostex could not prove which clone should migrate. Exactly one clone may migrate and publish; every other clone must adopt that result.';
  const fallback =
    gate.fallbackReason === 'unreadable-remote-state'
      ? 'The cached remote schema state could not be read, so Ghostex cannot safely choose for you.'
      : gate.fallbackReason === 'below-convergence-floor'
        ? "This database predates Beads' merge-safe migration floor, so unattended migration is unsafe."
        : '';
  const fixPrompt = buildRemoteMigrationFixPrompt({ gate, projectPath });
  return (
    <Card className='project-board-notice' data-kind='migration' role='alert' size='sm'>
      <CardContent>
        <div className='project-board-notice-icon' aria-hidden='true'>
          <IconAlertTriangle />
        </div>
        <div className='project-board-notice-body'>
          <strong>Project board migration needs coordination</strong>
          <p>{versionSummary}</p>
          <p>
            First install or update to the latest Beads release on every clone, then follow one coordinated migration
            path below.
          </p>
          <p>{explanation}</p>
          {fallback ? <p>{fallback}</p> : null}
          <div className='project-board-migration-options'>
            {gate.options.map((option) => (
              <div className='project-board-migration-option' key={option.id}>
                <strong>{beadsMigrationOptionLabel(option.id)}</strong>
                {option.when ? <p>Use only when {option.when}.</p> : null}
                {option.risk ? <p className='project-board-migration-risk'>Risk: {option.risk}.</p> : null}
              </div>
            ))}
          </div>
          <CopyBeadsFixPromptButton prompt={fixPrompt} />
          {docsUrl ? (
            <a href={docsUrl} rel='noreferrer' target='_blank'>
              Read the Beads multi-clone migration guide
            </a>
          ) : null}
        </div>
      </CardContent>
    </Card>
  );
}

export function ProjectBoardNotice({ message, projectPath }: { message: string; projectPath: string }) {
  const remoteMigrateGate = parseRemoteMigrateGate(message);
  if (remoteMigrateGate) {
    return <RemoteMigrateGateNotice gate={remoteMigrateGate} projectPath={projectPath} />;
  }
  /*
   * CDXC:ProjectBoard 2026-08-08:
   * A database/schema failure proves that Beads found a workspace, so generic
   * words such as "database" or ".beads" must never turn it into a `bd init`
   * instruction. Only Beads' explicit missing-workspace messages belong to the
   * initialization notice.
   */
  const isMissingProject =
    /not initialized|no storage|not a beads(?: workspace| project| repository)|no beads database found|run ['"]?bd init['"]?/i.test(
      message
    );
  const isMissingBeads =
    !isMissingProject &&
    /bd was not found|beads cli|executable|command not found|not found: bd|bd: not found|env: bd: no such file|cannot find/i.test(
      message
    );
  const title = isMissingBeads
    ? 'Beads CLI unavailable'
    : isMissingProject
      ? 'Initialize Beads for this project'
      : 'Project board unavailable';
  /*
  CDXC:Copy 2026-09-03:
  User decision: Ghostex-owned user-facing copy in the desktop, web, and mobile apps uses no em dashes; use punctuation that preserves the sentence's natural reading instead.
  */
  const bodyLines = isMissingBeads
    ? [
        'Ghostex uses the Beads CLI installed in the environment running this project: macOS, Linux, or the selected WSL distribution.',
        'Install the latest Beads release in that environment and ensure bd is available on its PATH, then refresh the board.',
      ]
    : isMissingProject
      ? [
          'This project does not have a Beads workspace yet. Copy a fix prompt for an agent to inspect and initialize it safely.',
        ]
      : [
          message,
          'Update Beads to the latest release, then retry. If this is a remote-backed migration, follow the coordinated migration instructions instead of migrating multiple clones independently.',
        ];
  const fixPrompt = buildBeadsFixPrompt({ message, projectPath, title });
  return (
    <Card
      className='project-board-notice'
      data-kind={isMissingBeads ? 'install' : isMissingProject ? 'init' : 'error'}
      role='status'
      size='sm'
    >
      <CardContent>
        {/*
          CDXC:ProjectBoard 2026-05-28-15:27:
          Initialization is a normal first-run state for Beads-backed projects, not an app failure.
          Present bd init as an explanatory setup callout with a direct Run action so users can initialize the project in a visible command pane before the board reloads.

          CDXC:ProjectBoard 2026-05-29-15:49:
          Missing-Beads setup should use the same polished notice shell but stay intentionally terse: one header and two lines below.
          Explain why Beads is required and keep copy/run controls in the single command row.

          CDXC:ProjectBoard 2026-08-12:
          Project/Kanban and shell agents intentionally use the same machine-installed `bd`. Missing and command-failure notices therefore direct the operator to install or update Beads instead of repairing Ghostex app resources.

          CDXC:ProjectBoard 2026-08-14:
          Local setup and update commands run visibly in the active project's command pane. The renderer sends only a fixed action selector; Rust owns the command literal and completion refresh.
        */}
        <div className='project-board-notice-icon' aria-hidden='true'>
          <IconAlertTriangle />
        </div>
        <div className='project-board-notice-body'>
          <strong>{title}</strong>
          {bodyLines.map((line) => (
            <p key={line}>{line}</p>
          ))}
          <CopyBeadsFixPromptButton prompt={fixPrompt} />
          {isMissingBeads || !isMissingProject ? (
            <a
              href='https://github.com/gastownhall/beads/blob/main/docs/INSTALLING.md'
              rel='noreferrer'
              target='_blank'
            >
              Beads install and update guide
            </a>
          ) : null}
        </div>
      </CardContent>
    </Card>
  );
}
