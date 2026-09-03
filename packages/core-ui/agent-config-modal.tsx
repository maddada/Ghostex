import { useEffect, useId, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { cn } from '@/packages/components/utils';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldTitle,
} from '@/packages/components/ui/field';
import { Input } from '@/packages/components/ui/input';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/packages/components/ui/select';
import {
  AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS,
  type AgentAcceptAllMode,
  supportsAgentAcceptAll,
} from '../shared/sidebar-agent-accept-all';
import { DEFAULT_SIDEBAR_AGENTS, getDefaultSidebarAgentByIcon, type SidebarAgentIcon } from '../shared/sidebar-agents';
import type { SidebarTheme } from '../shared/session-grid-contract';
import { AgentTypeSelectOption } from './agent-type-select-option';

export type AgentConfigDraft = {
  acceptAllMode?: AgentAcceptAllMode;
  agentId?: string;
  command: string;
  icon?: SidebarAgentIcon;
  name: string;
};

export type AgentConfigModalProps = {
  draft: AgentConfigDraft;
  isOpen: boolean;
  onCancel: () => void;
  onSave: (draft: AgentConfigDraft) => void;
  theme?: SidebarTheme;
};

const AGENT_TYPE_SELECT_ITEMS = [
  { label: 'Custom', value: 'custom' },
  ...DEFAULT_SIDEBAR_AGENTS.map((agent) => ({
    label: agent.name,
    value: agent.icon,
  })),
];

/**
 * CDXC:AppModal 2026-05-08-09:00
 * Reference-mode agent configuration uses the shared shadcn dialog stack so
 * native and web modal hosts render consistent focus management, sizing, and
 * close behavior.
 */
export function AgentConfigModal({ draft, isOpen, onCancel, onSave, theme = 'dark-1' }: AgentConfigModalProps) {
  const [acceptAllMode, setAcceptAllMode] = useState<AgentAcceptAllMode>(draft.acceptAllMode ?? 'inherit');
  const [command, setCommand] = useState(draft.command);
  const [icon, setIcon] = useState<SidebarAgentIcon | 'custom'>(draft.icon ?? 'custom');
  const [name, setName] = useState(draft.name);
  const acceptAllModeId = useId();
  const agentTypeId = useId();
  const commandId = useId();
  const nameId = useId();
  const isDarkTheme = getSidebarThemeVariant(theme) === 'dark';

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    setAcceptAllMode(draft.acceptAllMode ?? 'inherit');
    setCommand(draft.command);
    setIcon(draft.icon ?? 'custom');
    setName(draft.name);
  }, [draft, isOpen]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onCancel();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [isOpen, onCancel]);

  const isSaveDisabled = name.trim().length === 0 || command.trim().length === 0;
  const resolvedAgentId =
    draft.agentId ?? getDefaultSidebarAgentByIcon(icon === 'custom' ? undefined : icon)?.agentId ?? '';
  const acceptAllSupported = supportsAgentAcceptAll(resolvedAgentId, icon === 'custom' ? undefined : icon);
  const selectedDefaultAgent = getDefaultSidebarAgentByIcon(icon === 'custom' ? undefined : icon);

  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onCancel();
        }
      }}
      open={isOpen}
    >
      <DialogContent
        className={cn(
          'ghostex-settings-shadcn command-config-modal-shadcn agent-config-modal-shadcn',
          isDarkTheme && 'dark'
        )}
        data-sidebar-theme={theme}
      >
        <DialogHeader>
          <DialogTitle className='text-xl'>Configure agent</DialogTitle>
          <DialogDescription className='text-sm'>
            Launches a new Ghostex session and runs this agent command in it.
          </DialogDescription>
        </DialogHeader>
        <FieldGroup className='gap-6'>
          <Field className='gap-2.5'>
            <FieldContent>
              <FieldTitle>
                <FieldLabel className='text-sm' htmlFor={agentTypeId}>
                  Agent type
                </FieldLabel>
              </FieldTitle>
            </FieldContent>
            <Select
              items={AGENT_TYPE_SELECT_ITEMS}
              onValueChange={(value) => {
                const nextType = value as SidebarAgentIcon | 'custom';
                const previousDefaultAgent = getDefaultSidebarAgentByIcon(icon === 'custom' ? undefined : icon);
                const nextDefaultAgent = getDefaultSidebarAgentByIcon(nextType === 'custom' ? undefined : nextType);

                setIcon(nextType);
                if (!nextDefaultAgent) {
                  return;
                }

                setName((previousName) => {
                  if (previousName.trim().length === 0 || previousName === previousDefaultAgent?.name) {
                    return nextDefaultAgent.name;
                  }

                  return previousName;
                });
                setCommand((previousCommand) => {
                  if (previousCommand.trim().length === 0 || previousCommand === previousDefaultAgent?.command) {
                    return nextDefaultAgent.command;
                  }

                  return previousCommand;
                });
              }}
              value={icon}
            >
              <SelectTrigger className='h-10 w-full px-3 text-sm' id={agentTypeId}>
                <SelectValue>
                  <AgentTypeSelectOption icon={icon} name={selectedDefaultAgent?.name ?? 'Custom'} />
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value='custom'>
                    <AgentTypeSelectOption icon='custom' name='Custom' />
                  </SelectItem>
                  {DEFAULT_SIDEBAR_AGENTS.map((agent) => (
                    <SelectItem key={agent.agentId} value={agent.icon}>
                      <AgentTypeSelectOption icon={agent.icon} name={agent.name} />
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
          <Field className='gap-2.5'>
            <FieldContent>
              <FieldTitle>
                <FieldLabel className='text-sm' htmlFor={nameId}>
                  Name
                </FieldLabel>
              </FieldTitle>
            </FieldContent>
            <Input
              autoFocus
              className='h-10 px-3 text-sm md:text-sm'
              id={nameId}
              onChange={(event) => setName(event.currentTarget.value)}
              placeholder='Codex'
              value={name}
            />
          </Field>
          <Field className='gap-2.5'>
            <FieldContent>
              <FieldTitle>
                <FieldLabel className='text-sm' htmlFor={commandId}>
                  Command
                </FieldLabel>
              </FieldTitle>
            </FieldContent>
            <textarea
              className='command-config-textarea-shadcn'
              id={commandId}
              onChange={(event) => setCommand(event.currentTarget.value)}
              placeholder='codex'
              rows={3}
              value={command}
            />
          </Field>
          <Field className='gap-2.5'>
            <FieldContent>
              <FieldTitle>
                <FieldLabel className='text-sm' htmlFor={acceptAllModeId}>
                  Accept All
                </FieldLabel>
              </FieldTitle>
              <FieldDescription className='text-xs text-muted-foreground'>
                {acceptAllSupported
                  ? "Inherit uses the global Agents setting. Accept All applies this agent's permission-bypass mode at launch without changing the stored command."
                  : 'This agent does not expose a supported Accept All mode in Ghostex.'}
              </FieldDescription>
            </FieldContent>
            <Select
              disabled={!acceptAllSupported}
              items={AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS}
              onValueChange={(value) => setAcceptAllMode(value as AgentAcceptAllMode)}
              value={acceptAllMode}
            >
              <SelectTrigger className='h-10 w-full px-3 text-sm' id={acceptAllModeId}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS.map((item) => (
                    <SelectItem key={item.value} value={item.value}>
                      {item.label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
        </FieldGroup>
        <DialogFooter>
          <Button onClick={onCancel} type='button' variant='outline'>
            Cancel
          </Button>
          <Button
            disabled={isSaveDisabled}
            onClick={() =>
              onSave({
                acceptAllMode,
                agentId: draft.agentId,
                command: command.trim(),
                icon: icon === 'custom' ? undefined : icon,
                name: name.trim(),
              })
            }
            type='button'
          >
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function getSidebarThemeVariant(theme: SidebarTheme): 'dark' | 'light' {
  /**
   * CDXC:Theming 2026-06-15-01:43:
   * Agent editor modals share the app modal theme contract so Dark 1/Dark 2
   * keep dark shadcn mode and Light uses shadcn's light component tokens.
   */
  return theme.startsWith('light-') || theme === 'plain-light' ? 'light' : 'dark';
}
