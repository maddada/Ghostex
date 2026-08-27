import { AgentHookBenefits } from './agent-hook-benefits';
import {
  AppModalButton,
  AppModalColumn,
  AppModalDescription,
  AppModalFooter,
  AppModalHeader,
  AppModalShell,
  AppModalTitle,
} from './app-modal-shell';

export type AgentHooksRequiredModalProps = {
  agentName: string;
  isOpen: boolean;
  onClose: () => void;
  onInstall: () => void;
  onSkip: () => void;
};

export function AgentHooksRequiredModal({
  agentName,
  isOpen,
  onClose,
  onInstall,
  onSkip,
}: AgentHooksRequiredModalProps) {
  return (
    <AppModalShell className='agent-hooks-required-modal' isOpen={isOpen} onClose={onClose}>
      <AppModalColumn>
        <AppModalHeader>
          <AppModalTitle>Install required hooks for {agentName}</AppModalTitle>
          <AppModalDescription>
            Ghostex needs {agentName} hooks to enable these features. Install and approve this small local helper so
            Ghostex can identify the session, follow its lifecycle, and reconnect to the same conversation after sleep,
            reload, or app restart.
          </AppModalDescription>
        </AppModalHeader>
        <AgentHookBenefits />
        <p className='agent-hooks-required-note'>
          If you skip, these features will not work correctly for {agentName} until its hooks are installed and
          approved.
        </p>
        <AppModalFooter>
          <AppModalButton onClick={onSkip} type='button'>
            Skip for now
          </AppModalButton>
          <AppModalButton onClick={onInstall} tone='primary' type='button'>
            Install hooks
          </AppModalButton>
        </AppModalFooter>
      </AppModalColumn>
    </AppModalShell>
  );
}
