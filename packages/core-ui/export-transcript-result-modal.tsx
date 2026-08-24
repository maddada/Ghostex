import { IconCheck, IconCopy, IconFolderSearch, IconLoader2 } from '@tabler/icons-react';
import { useEffect, useId, useMemo, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
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

/**
 * CDXC:ExportTranscriptOptions 2026-08-24:
 * The export dialog's include-toggles. User and agent messages are never
 * optional, so only the three optional record families appear here. The
 * defaults mirror the daemon's historical selection: commands and patches in,
 * reasoning out.
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
 * On the result, every button is an optional follow-up: stage the file as a
 * mention in a fresh conversation's input, copy its path, or reveal it.
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
  const [includeOptions, setIncludeOptions] = useState(DEFAULT_EXPORT_TRANSCRIPT_INCLUDE_OPTIONS);
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
    setIncludeOptions(DEFAULT_EXPORT_TRANSCRIPT_INCLUDE_OPTIONS);
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
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onClose();
        }
      }}
      open={isOpen}
    >
      <DialogContent className='command-config-modal-shadcn export-transcript-modal-shadcn font-sans'>
        <DialogHeader className='gap-1'>
          <DialogTitle className='export-transcript-modal-title'>
            {stage.stage === 'done' ? 'Transcript Exported' : 'Export Transcript'}
          </DialogTitle>
          <DialogDescription className='export-transcript-modal-description'>
            {stage.stage === 'done'
              ? 'Start a new conversation with this file already mentioned in its input, or take the path somewhere else.'
              : 'The conversation is written as a markdown file. Choose what to include alongside the messages.'}
          </DialogDescription>
        </DialogHeader>
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
                    onCheckedChange={(checked) =>
                      setIncludeOptions((current) => ({ ...current, [row.key]: checked === true }))
                    }
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
          <div className='export-transcript-modal-body'>
            <div className='export-transcript-section-title'>Exported file</div>
            <div className='export-transcript-path-row'>
              <code className='export-transcript-path'>{stage.stage === 'done' ? stage.path : ''}</code>
              <Button
                aria-label={copied ? 'Path copied' : 'Copy path'}
                className='export-transcript-copy-button'
                onClick={() => {
                  if (stage.stage !== 'done') {
                    return;
                  }
                  void navigator.clipboard.writeText(stage.path).then(
                    () => setCopied(true),
                    () => setCopied(false)
                  );
                }}
                size='icon'
                type='button'
                variant='ghost'
              >
                {copied ? (
                  <IconCheck aria-hidden='true' size={15} stroke={1.9} />
                ) : (
                  <IconCopy aria-hidden='true' size={15} stroke={1.9} />
                )}
              </Button>
            </div>
            {promptAgents.length > 0 ? (
              <>
                <div className='export-transcript-section-title'>Continue with</div>
                <Select onValueChange={setSelectedAgentId} value={effectiveAgentId}>
                  <SelectTrigger aria-label='New conversation agent' id={agentSelectId}>
                    <SelectValue placeholder='Select agent' />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {promptAgents.map((agent) => (
                        <SelectItem key={agent.agentId} value={agent.agentId}>
                          {agent.name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
                <p className='export-transcript-hint'>
                  The new session starts in the same project with the transcript mentioned in its
                  input. Nothing is sent until you write your prompt and press Enter.
                </p>
              </>
            ) : null}
            {actionErrorMessage ? (
              <p className='export-transcript-error' role='alert'>
                {actionErrorMessage}
              </p>
            ) : null}
          </div>
        )}
        <DialogFooter>
          {showOptions ? (
            <>
              <Button onClick={onClose} type='button' variant='secondary'>
                Cancel
              </Button>
              <Button disabled={isExporting} onClick={() => onExport(includeOptions)} type='button'>
                {isExporting ? (
                  <IconLoader2 aria-hidden='true' className='export-transcript-spinner' size={15} stroke={1.9} />
                ) : null}
                {isExporting ? 'Exporting…' : stage.stage === 'failed' ? 'Try Again' : 'Export'}
              </Button>
            </>
          ) : (
            <>
              {stage.stage === 'done' && stage.canReveal && onRevealInFinder ? (
                <Button onClick={onRevealInFinder} type='button' variant='secondary'>
                  <IconFolderSearch aria-hidden='true' size={15} stroke={1.9} />
                  Reveal in Finder
                </Button>
              ) : null}
              <Button
                disabled={!effectiveAgentId || startBusy}
                onClick={() => onStartNewConversation(effectiveAgentId)}
                type='button'
              >
                {startBusy ? 'Starting…' : 'Start New Conversation'}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
