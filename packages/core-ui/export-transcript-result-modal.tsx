import { IconCheck, IconCopy, IconFolderSearch } from '@tabler/icons-react';
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
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/packages/components/ui/field';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/packages/components/ui/select';
import { type SidebarAgentButton } from '../shared/sidebar-agents';

export type ExportTranscriptResultModalProps = {
  /** Configured agents offered for the follow-up conversation. */
  agents?: SidebarAgentButton[];
  /**
   * The exported session's own agent, preselected so the obvious "continue with
   * the same agent" choice is one click away.
   */
  defaultAgentId?: string;
  isOpen: boolean;
  onClose: () => void;
  onRevealInFinder?: () => void;
  onStartNewConversation: (agentId: string) => void;
  /** Absolute path of the written markdown file, on the exporting machine. */
  path: string;
};

/**
 * CDXC:ExportTranscript 2026-08-20:
 * Shown once Export Transcript has written the markdown file. The export itself
 * is already done by the time this opens, so every button here is an optional
 * follow-up: stage the file as a mention in a fresh conversation's input, copy
 * its path, or reveal it. Starting a conversation never sends a prompt for the
 * user — the mention is typed into the new agent's input and left unsubmitted.
 * Reveal is omitted entirely — not disabled — when the file lives on another
 * machine, because there is nothing on this host to reveal.
 */
export function ExportTranscriptResultModal({
  agents = [],
  defaultAgentId,
  isOpen,
  onClose,
  onRevealInFinder,
  onStartNewConversation,
  path,
}: ExportTranscriptResultModalProps) {
  const agentSelectId = useId();
  const [selectedAgentId, setSelectedAgentId] = useState('');
  const [copied, setCopied] = useState(false);
  const promptAgents = useMemo(() => agents.filter((agent) => agent.command?.trim()), [agents]);
  const effectiveAgentId =
    promptAgents.find((agent) => agent.agentId === selectedAgentId)?.agentId ??
    promptAgents.find((agent) => agent.agentId === defaultAgentId)?.agentId ??
    promptAgents[0]?.agentId ??
    '';

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    setSelectedAgentId('');
    setCopied(false);
  }, [isOpen, path]);

  useEffect(() => {
    if (!copied) {
      return;
    }
    const timeoutId = window.setTimeout(() => setCopied(false), 1_500);
    return () => window.clearTimeout(timeoutId);
  }, [copied]);

  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onClose();
        }
      }}
      open={isOpen}
    >
      <DialogContent className='command-config-modal-shadcn export-transcript-result-modal-shadcn font-sans'>
        <DialogHeader>
          <DialogTitle className='text-xl'>Transcript Exported</DialogTitle>
          <DialogDescription>
            Start a new conversation with this file already mentioned in its input to carry the context over, or take
            the path somewhere else.
          </DialogDescription>
        </DialogHeader>
        <FieldGroup className='export-transcript-result-field-group'>
          <Field>
            <FieldLabel htmlFor={`${agentSelectId}-path`}>Exported file</FieldLabel>
            <code className='export-transcript-result-path' id={`${agentSelectId}-path`}>
              {path}
            </code>
          </Field>
          {promptAgents.length > 0 ? (
            <Field>
              <FieldLabel htmlFor={agentSelectId}>Continue with</FieldLabel>
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
              <FieldDescription>
                The new session starts in the same project with the transcript mentioned in its input. Nothing is sent
                until you write your prompt and press Enter.
              </FieldDescription>
            </Field>
          ) : null}
        </FieldGroup>
        <DialogFooter>
          <Button
            onClick={() => {
              void navigator.clipboard.writeText(path).then(
                () => setCopied(true),
                () => setCopied(false)
              );
            }}
            type='button'
            variant='secondary'
          >
            {copied ? (
              <IconCheck aria-hidden='true' size={15} stroke={1.9} />
            ) : (
              <IconCopy aria-hidden='true' size={15} stroke={1.9} />
            )}
            {copied ? 'Copied' : 'Copy Path'}
          </Button>
          {onRevealInFinder ? (
            <Button onClick={onRevealInFinder} type='button' variant='secondary'>
              <IconFolderSearch aria-hidden='true' size={15} stroke={1.9} />
              Reveal in Finder
            </Button>
          ) : null}
          <Button disabled={!effectiveAgentId} onClick={() => onStartNewConversation(effectiveAgentId)} type='button'>
            Start New Conversation
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
