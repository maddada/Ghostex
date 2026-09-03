import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent as ReactKeyboardEvent,
} from 'react';
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/packages/components/ui/field';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/packages/components/ui/select';
import { Textarea } from '@/packages/components/ui/textarea';
import {
  APP_MODAL_SELECT_CONTENT_CLASS,
  AppModalButton,
  AppModalDescription,
  AppModalFooter,
  AppModalForm,
  AppModalHeader,
  AppModalShell,
  AppModalTitle,
} from './app-modal-shell';
import { createSidebarAgentSelectItems, type SidebarAgentButton } from '../shared/sidebar-agents';
import { normalizeSessionRenameTitle } from '../shared/session-grid-contract';

const SESSION_RENAME_GENERATE_NAME_THRESHOLD = 70;

export type SessionRenameModalProps = {
  agents?: SidebarAgentButton[];
  /**
   * CDXC:SessionTitles 2026-07-29:
   * When the host supports it (gpui + claude/codex/cursor sessions), Generate
   * Name works without typed input: an empty or unchanged name submits an
   * empty title with shouldGenerateTitle so the controller summarizes the
   * session's recent user messages instead of pasted text.
   */
  canGenerateNameFromSessionHistory?: boolean;
  initialTitle: string;
  isOpen: boolean;
  onCancel: () => void;
  onConfirm: (title: string, options?: { agentId?: string; shouldGenerateTitle?: boolean }) => void;
  onPromptAgentIdChange?: (agentId: string) => void;
  promptAgentId?: string;
};

/**
 * CDXC:Sidebar 2026-04-26-18:19
 * Session-card renaming must happen inside the sidebar instead of delegating to
 * VS Code's input box, while still submitting through the existing
 * renameSession message so controller-side title generation and Agent CLI sync
 * behavior remain exactly the same.
 */
export function SessionRenameModal({
  agents = [],
  canGenerateNameFromSessionHistory = false,
  initialTitle,
  isOpen,
  onCancel,
  onConfirm,
  onPromptAgentIdChange,
  promptAgentId,
}: SessionRenameModalProps) {
  const [title, setTitle] = useState(initialTitle);
  const [localPromptAgentId, setLocalPromptAgentId] = useState('');
  const agentSelectId = useId();
  const inputId = useId();
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const userInteractedAfterOpenRef = useRef(false);
  const promptAgents = useMemo(() => agents.filter((agent) => agent.command?.trim()), [agents]);
  const promptAgentSelectItems = useMemo(() => createSidebarAgentSelectItems(promptAgents), [promptAgents]);
  const effectivePromptAgentId = promptAgentId ?? localPromptAgentId;
  const selectedPromptAgentId =
    promptAgents.find((agent) => agent.agentId === effectivePromptAgentId)?.agentId ?? promptAgents[0]?.agentId ?? '';

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    userInteractedAfterOpenRef.current = false;
    setTitle(initialTitle);
  }, [initialTitle, isOpen]);

  /**
   * CDXC:Sessions 2026-06-13-12:00
   * Opening Rename Session should drop the caret into the name field with the
   * existing title fully selected so the user can immediately type a
   * replacement. Own the initial focus through the dialog's initialFocus hook
   * (returning false so base-ui does not re-focus and collapse the selection
   * afterward), and keep a requestAnimationFrame pass as a fallback for hosts
   * where initialFocus runs before the textarea has its value.
   *
   * CDXC:Sessions 2026-06-15-01:27:
   * Rename Session now opens in a hidden native child-window host that becomes
   * key after React has already rendered and reported `presented`. Re-run the
   * same focus/select request across the native window-focus boundary, but stop
   * as soon as the user clicks or types in the modal so delayed host focus never
   * steals an intentional caret or button/select interaction.
   */
  const focusAndSelectInput = useCallback(() => {
    const input = inputRef.current;
    if (input) {
      input.focus({ preventScroll: true });
      input.setSelectionRange(0, input.value.length);
    }
    return false as const;
  }, []);

  const markUserInteractedAfterOpen = useCallback(() => {
    userInteractedAfterOpenRef.current = true;
  }, []);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const focusUnlessUserInteracted = () => {
      if (userInteractedAfterOpenRef.current) {
        return;
      }
      focusAndSelectInput();
    };
    const retryDelaysMs = [0, 16, 50, 100, 250, 500, 1000, 1600, 2400];
    const timeoutIds = retryDelaysMs.map((delayMs) => window.setTimeout(focusUnlessUserInteracted, delayMs));
    const animationFrame = window.requestAnimationFrame(focusUnlessUserInteracted);
    const windowFocusTimeoutIds: number[] = [];
    const windowFocusAnimationFrames: number[] = [];
    const handleWindowFocus = () => {
      windowFocusTimeoutIds.push(window.setTimeout(focusUnlessUserInteracted, 0));
      windowFocusAnimationFrames.push(window.requestAnimationFrame(focusUnlessUserInteracted));
    };

    window.addEventListener('focus', handleWindowFocus);
    return () => {
      window.cancelAnimationFrame(animationFrame);
      timeoutIds.forEach((timeoutId) => window.clearTimeout(timeoutId));
      windowFocusTimeoutIds.forEach((timeoutId) => window.clearTimeout(timeoutId));
      windowFocusAnimationFrames.forEach((frameId) => window.cancelAnimationFrame(frameId));
      window.removeEventListener('focus', handleWindowFocus);
    };
  }, [focusAndSelectInput, initialTitle, isOpen]);

  if (!isOpen) {
    return null;
  }

  const trimmedTitle = title.trim();
  const directRenameTitle = normalizeSessionRenameTitle(title);
  /**
   * CDXC:Sessions 2026-05-09-17:25
   * Long pasted rename text must stay in the input so the user can edit or
   * cancel it. Once the entered text is longer than 70 characters, Enter
   * becomes a Generate Name submit and explicitly asks the controller to
   * summarize from that text instead of applying it verbatim.
   */
  const canGenerateTitle = trimmedTitle.length > SESSION_RENAME_GENERATE_NAME_THRESHOLD;
  /**
   * CDXC:SessionTitles 2026-07-29:
   * The name field opens prefilled with the current title, so "the user wrote
   * nothing" means the field is empty or still exactly the initial title. In
   * that state Generate Name submits an empty title and the controller names
   * the session from its recent user-sent messages instead of the field text.
   */
  const canGenerateTitleFromSessionHistory =
    canGenerateNameFromSessionHistory && (trimmedTitle.length === 0 || trimmedTitle === initialTitle.trim());
  const confirmGenerateFromSessionHistory = () => {
    onConfirm('', { agentId: selectedPromptAgentId || undefined, shouldGenerateTitle: true });
  };
  const confirmTitle = (nextTitle: string, shouldGenerateTitle: boolean) => {
    const normalizedTitle = shouldGenerateTitle ? nextTitle.trim() : normalizeSessionRenameTitle(nextTitle);
    if (!normalizedTitle) {
      return;
    }

    onConfirm(
      normalizedTitle,
      shouldGenerateTitle ? { agentId: selectedPromptAgentId || undefined, shouldGenerateTitle: true } : undefined
    );
  };

  const submitRename = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    confirmTitle(title, canGenerateTitle);
  };

  const handleInputKeyDown = (event: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== 'Enter' || event.nativeEvent.isComposing) {
      return;
    }
    if (event.shiftKey) {
      return;
    }

    /**
     * CDXC:Sessions 2026-05-09-17:25
     * The native full-window modal host must preserve the reference rename
     * behavior where pressing Enter immediately submits the existing
     * renameSession command path. Bind Enter at the input so WKWebView
     * form-submission differences cannot leave the modal inert, but route
     * entered text longer than 70 characters to Generate Name.
     *
     * CDXC:AgentLauncher 2026-05-29-10:53:
     * Generate Name exposes a plain agent dropdown so users can choose the
     * prompt agent per modal without changing the global default. The host
     * remembers that modal choice and resets it when Settings default changes.
     */
    event.preventDefault();
    event.stopPropagation();
    if (canGenerateTitleFromSessionHistory && event.currentTarget.value.trim().length === 0) {
      confirmGenerateFromSessionHistory();
      return;
    }
    confirmTitle(event.currentTarget.value, canGenerateTitle);
  };

  const handlePromptAgentChange = (agentId: string) => {
    if (onPromptAgentIdChange) {
      onPromptAgentIdChange(agentId);
      return;
    }
    setLocalPromptAgentId(agentId);
  };

  return (
    <AppModalShell
      className='session-rename-modal-shadcn rename-session-modal-shadcn'
      initialFocus={focusAndSelectInput}
      isOpen={isOpen}
      onClose={onCancel}
    >
      <AppModalForm
        className='session-rename-form rename-session-form'
        onKeyDownCapture={markUserInteractedAfterOpen}
        onPointerDownCapture={markUserInteractedAfterOpen}
        onSubmit={submitRename}
      >
        <AppModalHeader>
          <AppModalTitle>Rename Session</AppModalTitle>
          <AppModalDescription>
            {canGenerateNameFromSessionHistory
              ? "Rename directly, or Generate Name from longer text; leave the name unchanged to generate from the session's recent messages."
              : 'Rename directly or generate a name from longer text.'}
          </AppModalDescription>
        </AppModalHeader>
        <FieldGroup className='session-rename-field-group'>
          <Field>
            <FieldLabel htmlFor={inputId}>Session name</FieldLabel>
            <Textarea
              aria-label='Session Name'
              className='session-rename-textarea'
              id={inputId}
              onChange={(event) => setTitle(event.currentTarget.value)}
              onKeyDown={handleInputKeyDown}
              ref={inputRef}
              value={title}
            />
            <FieldDescription>
              {trimmedTitle.length} / {SESSION_RENAME_GENERATE_NAME_THRESHOLD} characters
            </FieldDescription>
          </Field>
          {promptAgents.length > 0 ? (
            <Field>
              <FieldLabel htmlFor={agentSelectId}>Generate with</FieldLabel>
              <Select
                items={promptAgentSelectItems}
                onValueChange={handlePromptAgentChange}
                value={selectedPromptAgentId}
              >
                <SelectTrigger aria-label='Generate name agent' id={agentSelectId}>
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
            </Field>
          ) : null}
        </FieldGroup>
        <AppModalFooter>
          <AppModalButton disabled={!directRenameTitle} onClick={() => confirmTitle(title, false)} type='button'>
            Rename
          </AppModalButton>
          <AppModalButton
            disabled={!canGenerateTitle && !canGenerateTitleFromSessionHistory}
            onClick={() => {
              if (canGenerateTitleFromSessionHistory) {
                confirmGenerateFromSessionHistory();
                return;
              }
              confirmTitle(title, true);
            }}
            type='button'
          >
            Generate Name
          </AppModalButton>
        </AppModalFooter>
      </AppModalForm>
    </AppModalShell>
  );
}
