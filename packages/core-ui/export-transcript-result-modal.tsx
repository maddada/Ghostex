import { IconCheck, IconCopy, IconFolderSearch, IconLoader2 } from '@tabler/icons-react';
import { useEffect, useId, useMemo, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/packages/components/ui/select';
import { Switch } from '@/packages/components/ui/switch';
import { type SidebarAgentButton } from '../shared/sidebar-agents';
import {
  APP_MODAL_SELECT_CONTENT_CLASS,
  AppModalButton,
  AppModalDescription,
  AppModalFooter,
  AppModalHeader,
  AppModalShell,
  AppModalTitle,
} from './app-modal-shell';
import { AppTooltip } from './app-tooltip';

/**
 * CDXC:ExportTranscriptOptions 2026-08-24:
 * The export dialog's include-toggles. User and agent messages are never
 * optional, so only the three optional record families appear here. The
 * defaults mirror the daemon's historical selection: commands and patches in,
 * reasoning out. The user's last combination is a per-client UI preference so
 * repeat exports reopen exactly as they left them without involving gxserver.
 */
export type ExportTranscriptIncludeOptions = {
  includeCommands: boolean;
  includePatches: boolean;
  includeReasoning: boolean;
};

export const DEFAULT_EXPORT_TRANSCRIPT_INCLUDE_OPTIONS: ExportTranscriptIncludeOptions = {
  includeCommands: true,
  includePatches: true,
  includeReasoning: false,
};

const EXPORT_TRANSCRIPT_INCLUDE_OPTIONS_STORAGE_KEY = 'ghostex.exportTranscript.includeOptions';

function readExportTranscriptIncludeOptions(): ExportTranscriptIncludeOptions {
  if (typeof window === 'undefined') {
    return DEFAULT_EXPORT_TRANSCRIPT_INCLUDE_OPTIONS;
  }
  try {
    const stored = JSON.parse(
      window.localStorage.getItem(EXPORT_TRANSCRIPT_INCLUDE_OPTIONS_STORAGE_KEY) ?? 'null'
    ) as Partial<ExportTranscriptIncludeOptions> | null;
    return {
      includeCommands:
        typeof stored?.includeCommands === 'boolean'
          ? stored.includeCommands
          : DEFAULT_EXPORT_TRANSCRIPT_INCLUDE_OPTIONS.includeCommands,
      includePatches:
        typeof stored?.includePatches === 'boolean'
          ? stored.includePatches
          : DEFAULT_EXPORT_TRANSCRIPT_INCLUDE_OPTIONS.includePatches,
      includeReasoning:
        typeof stored?.includeReasoning === 'boolean'
          ? stored.includeReasoning
          : DEFAULT_EXPORT_TRANSCRIPT_INCLUDE_OPTIONS.includeReasoning,
    };
  } catch {
    return DEFAULT_EXPORT_TRANSCRIPT_INCLUDE_OPTIONS;
  }
}

function writeExportTranscriptIncludeOptions(options: ExportTranscriptIncludeOptions): void {
  try {
    window.localStorage.setItem(EXPORT_TRANSCRIPT_INCLUDE_OPTIONS_STORAGE_KEY, JSON.stringify(options));
  } catch {
    // Storage can be unavailable in isolated web, test, and story contexts.
  }
}

/**
 * The dialog's lifecycle, owned by the host: choose what to include, watch the
 * daemon write the file, then follow up on the result. `failed` keeps the
 * dialog open with the daemon's structured message (unsupported agent, no
 * transcript yet, …) and a way back to the export.
 */
export type ExportTranscriptModalStage =
  | { stage: 'options' }
  | { stage: 'exporting' }
  | { agentId?: string; canReveal: boolean; path: string; stage: 'done' }
  | { message: string; stage: 'failed' };

export type ExportTranscriptModalProps = {
  /** A follow-up action's failure (copy, session create), shown without leaving the done stage. */
  actionErrorMessage?: string;
  /** Configured agents offered for the follow-up conversation. */
  agents?: SidebarAgentButton[];
  /**
   * The exported session's own agent, preselected so the obvious "continue with
   * the same agent" choice is one click away.
   */
  defaultAgentId?: string;
  isOpen: boolean;
  onClose: () => void;
  /** Runs the export with the chosen include-toggles. The host answers by moving `stage` forward. */
  onExport: (options: ExportTranscriptIncludeOptions) => void;
  onRevealInFinder?: () => void;
  onStartNewConversation: (agentId: string) => void;
  stage: ExportTranscriptModalStage;
  /** Disables Start New Conversation while the host is creating the session. */
  startBusy?: boolean;
};

const INCLUDE_TOGGLE_ROWS: Array<{
  description: string;
  key: keyof ExportTranscriptIncludeOptions;
  label: string;
}> = [
  {
    description: 'Terminal commands the agent ran, with the tail of their output.',
    key: 'includeCommands',
    label: 'Commands & output',
  },
  {
    description: 'The patches the agent applied to files.',
    key: 'includePatches',
    label: 'File changes',
  },
  {
    description: "The agent's own reasoning sections.",
    key: 'includeReasoning',
    label: 'Reasoning',
  },
];

/**
 * CDXC:ExportTranscript 2026-08-20 / CDXC:ExportTranscriptOptions 2026-08-24:
 * Export Transcript's one dialog: an options stage with include-toggles, the
 * in-flight stage, and the result stage. The export only runs once the user
 * confirms it here, so the toggles govern the file that is actually written.
 * On the result, two numbered choices separate copying the path from staging
 * the file as a mention in a fresh conversation's input. Reveal stays a small
 * icon action beside the path instead of competing with those choices.
 * Starting a conversation never sends a prompt for the user — the mention is
 * typed into the new agent's input and left unsubmitted. Reveal is omitted
 * entirely — not disabled — when the file lives on another machine, because
 * there is nothing on this host to reveal.
 */
export function ExportTranscriptModal({
  actionErrorMessage,
  agents = [],
  defaultAgentId,
  isOpen,
  onClose,
  onExport,
  onRevealInFinder,
  onStartNewConversation,
  stage,
  startBusy = false,
}: ExportTranscriptModalProps) {
  const agentSelectId = useId();
  const [selectedAgentId, setSelectedAgentId] = useState('');
  const [copied, setCopied] = useState(false);
  const [includeOptions, setIncludeOptions] = useState(readExportTranscriptIncludeOptions);
  const promptAgents = useMemo(() => agents.filter((agent) => agent.command?.trim()), [agents]);
  const doneAgentId = stage.stage === 'done' ? stage.agentId : undefined;
  const effectiveAgentId =
    promptAgents.find((agent) => agent.agentId === selectedAgentId)?.agentId ??
    promptAgents.find((agent) => agent.agentId === (doneAgentId ?? defaultAgentId))?.agentId ??
    promptAgents[0]?.agentId ??
    '';

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    setSelectedAgentId('');
    setCopied(false);
    setIncludeOptions(readExportTranscriptIncludeOptions());
  }, [isOpen]);

  useEffect(() => {
    if (!copied) {
      return;
    }
    const timeoutId = window.setTimeout(() => setCopied(false), 1_500);
    return () => window.clearTimeout(timeoutId);
  }, [copied]);

  const isExporting = stage.stage === 'exporting';
  const showOptions = stage.stage !== 'done';

  return (
    <AppModalShell className='export-transcript-modal-shadcn' isOpen={isOpen} onClose={onClose} width={540}>
      <AppModalHeader className='gap-1'>
        <AppModalTitle>{stage.stage === 'done' ? 'Transcript Exported' : 'Export Transcript'}</AppModalTitle>
        <AppModalDescription>
          {stage.stage === 'done'
            ? 'Copy the exported file path, or continue the conversation with another agent.'
            : 'The conversation is written as a markdown file. Choose what to include alongside the messages.'}
        </AppModalDescription>
      </AppModalHeader>
      {showOptions ? (
        <div className='export-transcript-modal-body'>
          <div className='export-transcript-section-title'>Include</div>
          <div className='export-transcript-toggle-list'>
            {INCLUDE_TOGGLE_ROWS.map((row) => (
              <label className='export-transcript-toggle-row' key={row.key}>
                <span className='export-transcript-toggle-copy'>
                  <span className='export-transcript-toggle-label'>{row.label}</span>
                  <span className='export-transcript-toggle-description'>{row.description}</span>
                </span>
                <Switch
                  checked={includeOptions[row.key]}
                  disabled={isExporting}
                  onCheckedChange={(checked) => {
                    const next = { ...includeOptions, [row.key]: checked === true };
                    setIncludeOptions(next);
                    writeExportTranscriptIncludeOptions(next);
                  }}
                />
              </label>
            ))}
          </div>
          {stage.stage === 'failed' ? (
            <p className='export-transcript-error' role='alert'>
              {stage.message}
            </p>
          ) : null}
        </div>
      ) : (
        <div className='export-transcript-modal-body export-transcript-result-options'>
          <section className='export-transcript-result-option'>
            <div className='export-transcript-result-option-heading'>
              <span aria-hidden='true' className='export-transcript-result-option-number'>
                1
              </span>
              <div className='export-transcript-result-option-title'>Copy the path</div>
            </div>
            <div className='export-transcript-path-row'>
              <code className='export-transcript-path'>{stage.stage === 'done' ? stage.path : ''}</code>
              {stage.stage === 'done' && stage.canReveal && onRevealInFinder ? (
                <AppTooltip content='Reveal in Finder'>
                  <Button
                    aria-label='Reveal in Finder'
                    className='export-transcript-reveal-button'
                    onClick={onRevealInFinder}
                    size='icon'
                    type='button'
                    variant='ghost'
                  >
                    <IconFolderSearch aria-hidden='true' size={15} stroke={1.9} />
                  </Button>
                </AppTooltip>
              ) : null}
            </div>
            <AppModalButton
              className='export-transcript-result-action'
              onClick={() => {
                if (stage.stage !== 'done') {
                  return;
                }
                void navigator.clipboard.writeText(stage.path).then(
                  () => setCopied(true),
                  () => setCopied(false)
                );
              }}
              type='button'
            >
              {copied ? (
                <IconCheck aria-hidden='true' size={15} stroke={1.9} />
              ) : (
                <IconCopy aria-hidden='true' size={15} stroke={1.9} />
              )}
              {copied ? 'Path Copied' : 'Copy Path'}
            </AppModalButton>
          </section>
          {promptAgents.length > 0 ? (
            <section className='export-transcript-result-option'>
              <div className='export-transcript-result-option-heading'>
                <span aria-hidden='true' className='export-transcript-result-option-number'>
                  2
                </span>
                <div className='export-transcript-result-option-title'>Continue with another agent</div>
              </div>
              <Select onValueChange={setSelectedAgentId} value={effectiveAgentId}>
                <SelectTrigger aria-label='New conversation agent' id={agentSelectId}>
                  <SelectValue placeholder='Select agent' />
                </SelectTrigger>
                <SelectContent alignItemWithTrigger={false} className={APP_MODAL_SELECT_CONTENT_CLASS}>
                  <SelectGroup>
                    {promptAgents.map((agent) => (
                      <SelectItem key={agent.agentId} value={agent.agentId}>
                        {agent.name}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
              <AppModalButton
                className='export-transcript-result-action'
                disabled={!effectiveAgentId || startBusy}
                onClick={() => onStartNewConversation(effectiveAgentId)}
                type='button'
              >
                {startBusy ? 'Starting…' : 'Start New Conversation'}
              </AppModalButton>
            </section>
          ) : null}
          {actionErrorMessage ? (
            <p className='export-transcript-error' role='alert'>
              {actionErrorMessage}
            </p>
          ) : null}
        </div>
      )}
      {showOptions ? (
        <AppModalFooter>
          <AppModalButton onClick={onClose} type='button'>
            Cancel
          </AppModalButton>
          <AppModalButton disabled={isExporting} onClick={() => onExport(includeOptions)} type='button'>
            {isExporting ? (
              <IconLoader2 aria-hidden='true' className='export-transcript-spinner' size={15} stroke={1.9} />
            ) : null}
            {isExporting ? 'Exporting…' : stage.stage === 'failed' ? 'Try Again' : 'Export'}
          </AppModalButton>
        </AppModalFooter>
      ) : null}
    </AppModalShell>
  );
}
