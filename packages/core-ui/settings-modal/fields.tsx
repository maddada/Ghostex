import { DragDropProvider, type DragDropEventHandlers } from '@dnd-kit/react';
import { isSortableOperation, useSortable } from '@dnd-kit/react/sortable';
import { useEffect, useId, useRef, useState, type ComponentProps, type ReactNode, type RefObject } from 'react';
import { flushSync } from 'react-dom';
import ColorPicker from 'react-best-gradient-color-picker';
import { cn } from '@/packages/components/utils';
import { Button } from '@/packages/components/ui/button';
import { Card, CardContent, CardTitle } from '@/packages/components/ui/card';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/packages/components/ui/dialog';
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldTitle,
} from '@/packages/components/ui/field';
import { Input as BaseInput } from '@/packages/components/ui/input';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/packages/components/ui/select';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { Slider } from '@/packages/components/ui/slider';
import { Switch } from '@/packages/components/ui/switch';
import { Textarea as BaseTextarea } from '@/packages/components/ui/textarea';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/packages/components/ui/tooltip';
import { AppTooltip } from '../app-tooltip';
import { DisabledSettingControlTooltip } from '../disabled-setting-control-tooltip';
import {
  IconAsterisk,
  IconAlertTriangle,
  IconArrowBigUp,
  IconChevronRight,
  IconDownload,
  IconEye,
  IconEyeOff,
  IconFolderOpen,
  IconGripVertical,
  IconInfoCircle,
  IconMinus,
  IconPalette,
  IconPhoto,
  IconPlayerPlay,
  IconPlus,
  IconTrash,
  IconX,
} from '@tabler/icons-react';
import {
  COMPLETION_SOUND_OPTIONS,
  type CompletionSoundPreference,
  type CompletionSoundSetting,
} from '../../shared/completion-sound';
import { type SidebarAppIconInfo, type SidebarAppIconStateMessage } from '../../shared/session-grid-contract';
import {
  DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR,
  DEFAULT_ghostex_SETTINGS,
  DIAGNOSTIC_LOGGING_SCENARIOS,
  PREFERRED_AGENT_INTERFACE_OPTIONS,
  SESSION_CHAT_THEME_OPTIONS,
  SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS,
  SIDEBAR_SETTINGS_PRESETS,
  SIDEBAR_SPACES_ENABLED_OPTIONS,
  TERMINAL_VIEW_WIDTH_MODE_OPTIONS,
  normalizeTerminalDevServerIgnoredPortRuleInput,
  normalizeTerminalDevServerIgnoredPortRules,
  type DiagnosticLoggingScenarioId,
  type DiagnosticLoggingSettings,
  type PreferredAgentInterface,
  type SidebarSettingsPresetId,
  type SidebarProjectGroupStyle,
  type TerminalViewWidthMode,
} from '../../shared/ghostex-settings';
import { type SessionChatTheme } from '../../shared/session-chat';
import { PET_OPTIONS, type PetId } from '../../shared/pets';
import {
  getSidebarSessionTagListItemLabel,
  normalizeSidebarSessionTagListItems,
  type SidebarSessionTagListItem,
} from '../../shared/session-tags';
import { PetAvatar } from '../pet-avatar';
import { SessionTagIcon } from '../session-tag-ui';
import { createSettingsSidebarTagListItemDragData, getSettingsSidebarTagListItemDragData, moveId } from './drag-data';
import {
  DEFAULT_DIAGNOSTIC_LOGGING_ENABLE_DURATION,
  DIAGNOSTIC_LOGGING_DURATION_OPTIONS,
  DIAGNOSTIC_LOGGING_GROUPS,
  DiagnosticLoggingDurationValue,
  SettingModificationProps,
} from './types';

export const MODIFIED_SETTING_TOOLTIP = 'Modified Setting.\n \nClick to Reset to Default';

/*
 * CDXC:SettingsTextFields 2026-06-15-18:19:
 * Settings text fields hold explicit configuration values, including Remote SSH names, users, hosts, ports, identity files, commands, and prompts. Disable browser and macOS text assistance at the Settings modal field boundary so autocomplete, autocorrect, capitalization, and spellcheck cannot rewrite user-entered configuration.
 */
export function SettingsInput({
  autoCapitalize = 'none',
  autoComplete = 'off',
  autoCorrect = 'off',
  spellCheck = false,
  ...props
}: ComponentProps<'input'>) {
  return (
    <BaseInput
      autoCapitalize={autoCapitalize}
      autoComplete={autoComplete}
      autoCorrect={autoCorrect}
      spellCheck={spellCheck}
      {...props}
    />
  );
}

export function SettingsTextarea({
  autoCapitalize = 'none',
  autoComplete = 'off',
  autoCorrect = 'off',
  spellCheck = false,
  ...props
}: ComponentProps<'textarea'>) {
  return (
    <BaseTextarea
      autoCapitalize={autoCapitalize}
      autoComplete={autoComplete}
      autoCorrect={autoCorrect}
      spellCheck={spellCheck}
      {...props}
    />
  );
}

export function SettingsSelect({
  disabled,
  disabledReason,
  disabledTooltipClassName,
  onOpenChange,
  onValueChange,
  ...props
}: ComponentProps<typeof Select> & {
  disabledReason?: string;
  disabledTooltipClassName?: string;
}) {
  const [selectOpen, setSelectOpen] = useState(false);

  useEffect(() => {
    if (disabled && selectOpen) {
      setSelectOpen(false);
    }
  }, [disabled, selectOpen]);

  const closeSelect = () => {
    flushSync(() => {
      setSelectOpen(false);
    });
  };

  /*
   * CDXC:SettingsDropdowns 2026-06-19-19:22:
   * Settings select changes save immediately through the native modal host.
   * Close every Base UI popup before posting the setting update so portaled
   * dropdowns, including Default Prompt Agent and command editor selects,
   * cannot keep their modal focus trap alive while gxserver and native settings
   * hydration re-render the dialog.
   */
  const select = (
    <Select
      {...props}
      disabled={disabled}
      onOpenChange={(nextOpen, eventDetails) => {
        setSelectOpen(nextOpen);
        onOpenChange?.(nextOpen, eventDetails);
      }}
      onValueChange={(nextValue) => {
        closeSelect();
        onValueChange?.(nextValue);
      }}
      open={selectOpen}
    />
  );

  return (
    <DisabledSettingControlTooltip
      className={disabledTooltipClassName}
      disabled={disabled === true}
      reason={disabledReason}
    >
      {select}
    </DisabledSettingControlTooltip>
  );
}

export function SettingButton({
  disabledReason,
  disabledTooltipClassName,
  ...props
}: ComponentProps<typeof Button> & {
  disabledReason: string;
  disabledTooltipClassName?: string;
}) {
  const disabled = props.disabled === true;
  return (
    <DisabledSettingControlTooltip className={disabledTooltipClassName} disabled={disabled} reason={disabledReason}>
      <Button {...props} />
    </DisabledSettingControlTooltip>
  );
}

export function SettingSwitch({
  disabledReason,
  ...props
}: ComponentProps<typeof Switch> & {
  disabledReason: string;
}) {
  const disabled = props.disabled === true;
  return (
    <DisabledSettingControlTooltip disabled={disabled} reason={disabledReason}>
      <Switch {...props} />
    </DisabledSettingControlTooltip>
  );
}

export function SettingsSelectContent({ className, ...props }: ComponentProps<typeof SelectContent>) {
  /*
   * CDXC:SettingsDropdowns 2026-06-16-16:58:
   * Settings Select popups are portaled outside the Settings dialog subtree.
   * Carry a stable class on the popup so row hover, focus, and selected states
   * can stay neutral gray instead of inheriting saturated app accent styling.
   */
  return <SelectContent className={cn('settings-select-content', className)} {...props} />;
}

/*
 * CDXC:SettingsPerformance 2026-06-29-00:40:
 * Settings management rows still need dnd-kit to register one element as both sortable item and drag source, but the row components need React Compiler coverage.
 * Keep the callback-ref mutation behind this helper so render code does not directly invoke ref-named mutators.
 */
export function setSettingsSortableRowElement(
  sortableRefs: Pick<ReturnType<typeof useSortable>, 'ref' | 'sourceRef'>,
  element: HTMLDivElement | null
): void {
  sortableRefs.ref(element);
  sortableRefs.sourceRef(element);
}

export function SettingsNativeScrollArea({
  children,
  className,
  onScrollCapture,
  viewportClassName,
  ...props
}: ComponentProps<'div'> & {
  viewportClassName?: string;
}) {
  return (
    <div {...props} className={cn('relative', className)} data-slot='scroll-area'>
      {/*
       * CDXC:SettingsPerformance 2026-06-29-00:40:
       * Settings pages must scroll with native overflow instead of Base UI
       * ScrollArea because long pages do not need custom scrollbar metrics or
       * scroll-linked edge masks on every frame. Keep the viewport data-slot so
       * existing section tracking and padding CSS continue to target the
       * scrollable element.
       */}
      <div
        className={cn(
          'settings-native-scroll-viewport size-full overflow-x-hidden overflow-y-auto rounded-none outline-none focus-visible:ring-[3px] focus-visible:ring-ring/20 focus-visible:outline-1',
          viewportClassName
        )}
        data-slot='scroll-area-viewport'
        onScrollCapture={onScrollCapture}
      >
        {children}
      </div>
    </div>
  );
}

export function TerminalDevServerIgnoredPortsField({
  advanced,
  ignoredPortRules,
  isModified,
  onChange,
  onResetToDefault,
}: {
  advanced?: boolean;
  ignoredPortRules: readonly string[];
  onChange: (ignoredPortRules: readonly string[]) => void;
} & SettingModificationProps) {
  const id = useId();
  const [inputValue, setInputValue] = useState('');
  const [error, setError] = useState('');
  const addIgnoredPortRule = () => {
    const canonicalRule = normalizeTerminalDevServerIgnoredPortRuleInput(inputValue);
    if (!canonicalRule) {
      setError('Enter a port (e.g. 9229) or a range (e.g. 24678-24680).');
      return;
    }
    setError('');
    setInputValue('');
    onChange(normalizeTerminalDevServerIgnoredPortRules([...ignoredPortRules, canonicalRule]));
  };
  const removeIgnoredPortRule = (rule: string) => {
    onChange(
      normalizeTerminalDevServerIgnoredPortRules(ignoredPortRules.filter((ignoredPortRule) => ignoredPortRule !== rule))
    );
  };

  return (
    <SettingRow
      advanced={advanced}
      description='Servers on these ports are hidden from the server menu. Enter a port or an inclusive range.'
      htmlFor={id}
      isModified={isModified}
      label='Ignored ports'
      onResetToDefault={onResetToDefault}
    >
      <div className='grid gap-3' id={id}>
        <div className='grid gap-2'>
          {ignoredPortRules.length === 0 ? (
            <div className='text-sm text-muted-foreground'>No ignored ports.</div>
          ) : (
            ignoredPortRules.map((rule) => (
              <div
                className='flex min-h-9 items-center justify-between gap-3 rounded-none border border-border/70 bg-card/40 px-3 py-2'
                key={rule}
              >
                <span className='min-w-0 truncate font-mono text-sm'>{rule}</span>
                <Button
                  aria-label={`Remove ignored port ${rule}`}
                  onClick={() => removeIgnoredPortRule(rule)}
                  size='icon-xs'
                  type='button'
                  variant='ghost'
                >
                  <IconTrash aria-hidden='true' size={14} />
                </Button>
              </div>
            ))
          )}
        </div>
        <div className='flex items-center gap-2'>
          <SettingsInput
            aria-invalid={Boolean(error)}
            aria-label='Ignored port or range'
            className='h-8 min-w-0 flex-1 px-3 text-[13px]'
            onChange={(event) => {
              setInputValue(event.currentTarget.value);
              if (error) {
                setError('');
              }
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                addIgnoredPortRule();
              }
            }}
            placeholder='e.g. 9229 or 24678-24680'
            value={inputValue}
          />
          <SettingButton
            disabled={!inputValue.trim()}
            disabledReason='Enter a port or port range first.'
            onClick={addIgnoredPortRule}
            type='button'
            variant='outline'
          >
            <IconPlus aria-hidden='true' data-icon='inline-start' />
            Add
          </SettingButton>
        </div>
        {error ? (
          <div className='text-sm text-destructive' role='alert'>
            {error}
          </div>
        ) : null}
      </div>
    </SettingRow>
  );
}

export function SettingsSection({
  actions,
  children,
  description,
  descriptionClassName,
  sectionRef,
  title,
}: {
  actions?: ReactNode;
  children: ReactNode;
  description?: ReactNode;
  descriptionClassName?: string;
  sectionRef?: RefObject<HTMLDivElement | null>;
  title: string;
}) {
  return (
    <div className='settings-section-anchor' ref={sectionRef}>
      <Card
        className={cn(
          'settings-section-card relative mt-5 overflow-visible pb-[25px] pt-8',
          actions && 'settings-section-with-actions'
        )}
        size='sm'
      >
        {/* CDXC:Settings 2026-04-26-12:31: The target settings examples stack the
          text above controls. Keeping rows vertical avoids squeezing labels in
          the narrow ghostex sidebar modal. */}
        {/* CDXC:Settings 2026-04-26-21:00: Settings sections need extra space
          above each header, while adjacent settings should separate by rhythm
          instead of divider lines. */}
        {/* CDXC:Settings 2026-04-26-21:03: Each settings category is a distinct
          shadcn card. The heading is larger and sits over the top border so
          the card reads as a labeled group without reintroducing row dividers. */}
        {/* CDXC:Settings 2026-04-26-21:22: Section card labels must stay on one
          line and clear the card contents, including multi-word headings like
          Session Cards. */}
        {/* CDXC:Settings 2026-04-27-01:01: The title pill cannot use shadcn
          CardHeader because its container-query size containment makes
          max-content resolve to the padding width instead of the text width. */}
        {/* CDXC:Settings 2026-06-12-21:00: Settings section cards need exactly
          25px of total bottom space between their last row and the card border,
          matching the compact bordered card style used by Agent Hooks and
          adjacent grouped settings sections. */}
        <div className='settings-section-title-pill'>
          <CardTitle className='settings-section-title-pill-text'>{title}</CardTitle>
        </div>
        {/* CDXC:UnifiedSettings 2026-05-09-17:01: Agents and Actions management
          controls belong in the section header row. Action creation labels omit
          "Add", while the agent creation CTA keeps "Add Agent" per product
          requirements. */}
        {actions ? <div className='settings-section-header-actions'>{actions}</div> : null}
        <CardContent className='pt-2'>
          {description ? (
            <p className={cn('m-0 pb-5 text-sm leading-6 text-muted-foreground', descriptionClassName)}>
              {description}
            </p>
          ) : null}
          <FieldGroup className='gap-6'>{children}</FieldGroup>
        </CardContent>
      </Card>
    </div>
  );
}

export function SliderNumberField({
  advanced,
  description,
  isModified,
  label,
  max,
  min,
  onChange,
  onCommit,
  onResetToDefault,
  step,
  value,
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  max: number;
  min: number;
  onChange: (value: number) => void;
  onCommit: (value: number) => void;
  step: number;
  value: number;
} & SettingModificationProps) {
  const id = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [inputText, setInputText] = useState(() => formatSliderNumber(value, step));
  const valueText = formatSliderNumber(value, step);

  useEffect(() => {
    if (document.activeElement !== inputRef.current) {
      setInputText(valueText);
    }
  }, [valueText]);

  const updateValue = (nextValue: number) => {
    if (!Number.isFinite(nextValue)) {
      return value;
    }
    const clampedValue = clampNumber(snapNumberToStep(nextValue, min, step), min, max);
    onChange(clampedValue);
    return clampedValue;
  };

  const commitValue = (nextValue: number) => {
    const clampedValue = Number.isFinite(nextValue)
      ? clampNumber(snapNumberToStep(nextValue, min, step), min, max)
      : value;
    setInputText(formatSliderNumber(clampedValue, step));
    onCommit(clampedValue);
  };

  const updateInputText = (nextText: string) => {
    setInputText(nextText);
    const nextValue = Number(nextText);
    if (nextText.trim() === '' || !Number.isFinite(nextValue) || nextValue < min || nextValue > max) {
      return;
    }
    onChange(clampNumber(snapNumberToStep(nextValue, min, step), min, max));
  };

  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <div className='grid grid-cols-[minmax(0,1fr)_4.75rem] items-center gap-3'>
        <Slider
          aria-label={label}
          max={max}
          min={min}
          onValueCommit={([nextValue]) => commitValue(nextValue ?? value)}
          onValueChange={([nextValue]) => updateValue(nextValue ?? value)}
          step={step}
          value={[value]}
        />
        <SettingsInput
          id={id}
          className='h-8 px-3 text-[13px] tabular-nums'
          onBlur={(event) => commitValue(Number(event.currentTarget.value))}
          onChange={(event) => updateInputText(event.currentTarget.value)}
          onFocus={(event) => event.currentTarget.select()}
          max={max}
          min={min}
          ref={inputRef}
          step={step}
          type='number'
          value={inputText}
        />
      </div>
    </SettingRow>
  );
}

export function clampNumber(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function snapNumberToStep(value: number, min: number, step: number): number {
  /**
   * CDXC:Settings 2026-04-29-08:56
   * Slider-backed numeric settings must persist the same step increments the
   * UI presents. This keeps Ghostty scroll multipliers on 0.25 increments even
   * when users type values into the adjacent number field.
   */
  const decimals = Math.max(0, step.toString().split('.')[1]?.length ?? 0);
  const scaledValue = Math.round((value - min) / step) * step + min;
  return Number(scaledValue.toFixed(decimals));
}

export function formatSliderNumber(value: number, step: number): string {
  if (Number.isInteger(step)) {
    return String(Math.round(value));
  }
  const decimals = Math.max(0, step.toString().split('.')[1]?.length ?? 0);
  return value.toFixed(decimals);
}

export function ActionButtonField({
  advanced,
  children,
  description,
  label,
  onClick,
}: {
  advanced?: boolean;
  children: ReactNode;
  description?: string;
  label: string;
  onClick: () => void;
}) {
  const id = useId();
  return (
    <SettingRow advanced={advanced} description={description} htmlFor={id} label={label}>
      <Button className='h-8 w-full justify-start px-3 text-[13px]' id={id} onClick={onClick} type='button'>
        {children}
      </Button>
    </SettingRow>
  );
}

export function ActionButtonPairField({
  advanced,
  actions,
  description,
  label,
}: {
  advanced?: boolean;
  actions: ReadonlyArray<{ label: string; onClick: () => void }>;
  description?: string;
  label: string;
}) {
  const id = useId();
  return (
    <SettingRow advanced={advanced} description={description} htmlFor={id} label={label}>
      <div className='grid w-full grid-cols-1 gap-2 sm:grid-cols-2'>
        {actions.map((action, index) => (
          <Button
            className='h-8 w-full justify-center px-3 text-center text-[13px]'
            id={index === 0 ? id : undefined}
            key={action.label}
            onClick={action.onClick}
            type='button'
            variant='outline'
          >
            {action.label}
          </Button>
        ))}
      </div>
    </SettingRow>
  );
}

export function SelectField({
  advanced,
  contentClassName,
  description,
  disabled,
  disabledReason,
  isModified,
  label,
  onChange,
  onResetToDefault,
  options,
  showScrollButtons,
  supportingContent,
  value,
}: {
  advanced?: boolean;
  contentClassName?: string;
  description?: string;
  disabled?: boolean;
  disabledReason?: string;
  label: string;
  onChange: (value: string) => void;
  options: ReadonlyArray<{ label: string; value: string }>;
  showScrollButtons?: boolean;
  supportingContent?: ReactNode;
  value: string;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <SettingsSelect
        disabled={disabled}
        disabledReason={disabledReason}
        disabledTooltipClassName='w-full'
        items={options}
        onValueChange={onChange}
        value={value}
      >
        <SelectTrigger className='h-8 w-full px-3 text-[13px]' disabled={disabled} id={id}>
          <SelectValue />
        </SelectTrigger>
        <SettingsSelectContent className={contentClassName} showScrollButtons={showScrollButtons}>
          <SelectGroup>
            {options.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SettingsSelectContent>
      </SettingsSelect>
      {supportingContent}
    </SettingRow>
  );
}

export function StaticNoteField({
  advanced,
  description,
  label,
  surface = 'boxed',
  value = 'Not available',
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  surface?: 'boxed' | 'plain';
  value?: string;
}) {
  const id = useId();
  return (
    <SettingRow advanced={advanced} description={description} htmlFor={id} label={label}>
      <div
        className={
          surface === 'plain'
            ? 'text-sm text-muted-foreground'
            : 'rounded-none border border-border bg-muted/30 px-3 py-2 text-sm text-muted-foreground'
        }
        id={id}
      >
        {value}
      </div>
    </SettingRow>
  );
}

export function PetPickerField({
  advanced,
  isModified,
  onChange,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  onChange: (value: PetId) => void;
  value: PetId;
} & SettingModificationProps) {
  const id = useId();
  const selectedPet = PET_OPTIONS.find((option) => option.id === value) ?? PET_OPTIONS[0]!;
  return (
    <SettingRow
      advanced={advanced}
      description='Choose the pet sprite.'
      htmlFor={id}
      isModified={isModified}
      label='Pet'
      onResetToDefault={onResetToDefault}
    >
      <div className='flex min-w-0 items-center gap-3'>
        <div className='flex size-16 shrink-0 items-center justify-center overflow-hidden rounded-none border border-border bg-muted/30'>
          <PetAvatar className='scale-[0.42]' petId={selectedPet.id} />
        </div>
        <div className='flex min-w-0 flex-1 flex-col gap-2'>
          <SettingsSelect onValueChange={(nextValue) => onChange(nextValue as PetId)} value={value}>
            <SelectTrigger className='h-8 w-full px-3 text-[13px]' id={id}>
              <SelectValue />
            </SelectTrigger>
            <SettingsSelectContent>
              <SelectGroup>
                {PET_OPTIONS.map((option) => (
                  <SelectItem key={option.id} value={option.id}>
                    {option.displayName}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SettingsSelectContent>
          </SettingsSelect>
          <div className='truncate text-xs text-muted-foreground'>{selectedPet.description}</div>
        </div>
      </div>
    </SettingRow>
  );
}

/**
 * CDXC:AppIconPicker 2026-06-28-06:05:
 * App Icon is an advanced custom-image flow, not a preset gallery. Render one
 * selected-icon preview, one Select Image button, and an X on the custom preview
 * to restore the empty/default source id. Selection still posts to native first;
 * persistence happens upstream only after native confirms with appIconState.
 */
export function AppIconPickerField({
  advanced,
  error,
  onChooseFile,
  onSelect,
  state,
}: {
  advanced?: boolean;
  error?: string;
  onChooseFile: () => void;
  onSelect: (sourceId: string) => void;
  state: SidebarAppIconStateMessage | undefined;
}) {
  const id = useId();
  const allIcons: SidebarAppIconInfo[] = state?.icons ?? [];
  const defaultIcon = allIcons.find((icon) => icon.id === '');
  const icons = allIcons.filter((icon) => icon.id !== '');
  const selectedId = state?.selectedId ?? '';
  const isDefaultSelected = selectedId === '';
  const selectedIcon = isDefaultSelected ? defaultIcon : icons.find((icon) => icon.id === selectedId);

  const previewIcon = selectedIcon ?? defaultIcon;

  return (
    <SettingRow
      advanced={advanced}
      description='Choose a PNG for the macOS Dock and app-switcher icon.'
      htmlFor={id}
      label='Custom app icon'
    >
      <div className='flex min-w-0 flex-col gap-3'>
        <div className='flex min-w-0 items-center gap-3'>
          <div className='relative flex size-16 shrink-0 items-center justify-center overflow-visible'>
            <div className='flex size-16 items-center justify-center overflow-hidden rounded-none border border-border bg-muted/30'>
              {previewIcon ? (
                <img alt={previewIcon.name} className='size-full object-contain' src={previewIcon.thumbnailDataUrl} />
              ) : (
                <IconPhoto aria-hidden='true' className='size-7 text-muted-foreground' />
              )}
            </div>
            {!isDefaultSelected ? (
              <Tooltip>
                <TooltipTrigger
                  render={
                    <button
                      aria-label='Use default app icon'
                      className='absolute -right-2 -top-2 flex size-6 items-center justify-center rounded-none border border-border bg-background text-muted-foreground shadow-sm hover:text-foreground'
                      onClick={() => onSelect('')}
                      type='button'
                    >
                      <IconX aria-hidden='true' className='size-3.5' />
                    </button>
                  }
                />
                <TooltipContent sideOffset={6}>Use default icon</TooltipContent>
              </Tooltip>
            ) : null}
          </div>
          <div className='flex min-w-0 flex-1 flex-col gap-2'>
            <Button
              className='h-9 w-fit rounded-none px-3 text-sm'
              id={id}
              onClick={onChooseFile}
              type='button'
              variant='outline'
            >
              <IconDownload aria-hidden='true' data-icon='inline-start' />
              Select Image
            </Button>
            <div className='truncate text-xs text-muted-foreground'>
              {isDefaultSelected ? 'Using the bundled Ghostex icon.' : (selectedIcon?.name ?? selectedId)}
            </div>
          </div>
        </div>

        {error ? (
          <div className='flex items-start gap-2 rounded-none border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive'>
            <IconAlertTriangle aria-hidden='true' className='mt-0.5 size-4 shrink-0' />
            <span className='min-w-0'>{error}</span>
          </div>
        ) : null}
      </div>
    </SettingRow>
  );
}

export function SoundField({
  advanced,
  allowOff = false,
  description,
  isModified,
  label,
  onChange,
  onPlay,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  allowOff?: boolean;
  description?: string;
  label: string;
  onChange: (value: CompletionSoundPreference) => void;
  onPlay?: (value: CompletionSoundSetting) => void;
  value: CompletionSoundPreference;
} & SettingModificationProps) {
  /**
   * CDXC:Settings 2026-04-29-17:01
   * Sound pickers have enough options that Radix hover-scroll buttons can
   * fight wheel scrolling inside the modal. Disable those auto-scroll zones so
   * mouse and trackpad wheel direction remains stable.
   *
   * CDXC:Settings 2026-05-11-02:06
   * Every sound picker needs an adjacent icon-only preview button so users can
   * audition the selected sound without changing settings or triggering the
   * broader agent-completion notification test flow.
   */
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <div className='grid grid-cols-[minmax(0,1fr)_2.5rem] items-center gap-2'>
        <SettingsSelect onValueChange={(nextValue) => onChange(nextValue as CompletionSoundPreference)} value={value}>
          <SelectTrigger className='h-8 w-full px-3 text-[13px]' id={id}>
            <SelectValue />
          </SelectTrigger>
          <SettingsSelectContent className='max-h-72' showScrollButtons={false}>
            <SelectGroup>
              {allowOff ? <SelectItem value='off'>Off</SelectItem> : null}
              {COMPLETION_SOUND_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SettingsSelectContent>
        </SettingsSelect>
        <DisabledSettingControlTooltip
          disabled={!onPlay || value === 'off'}
          reason={value === 'off' ? 'Choose a sound to preview it.' : 'Sound preview isn’t available here.'}
        >
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  aria-label={`Play ${label}`}
                  className='size-8 rounded-none'
                  disabled={!onPlay || value === 'off'}
                  onClick={() => value !== 'off' && onPlay?.(value)}
                  size='icon'
                  type='button'
                  variant='outline'
                >
                  <IconPlayerPlay aria-hidden='true' className='size-4' />
                </Button>
              }
            />
            <TooltipContent sideOffset={6}>Play selected sound</TooltipContent>
          </Tooltip>
        </DisabledSettingControlTooltip>
      </div>
    </SettingRow>
  );
}

export function TextField({
  advanced,
  browseLabel,
  description,
  isModified,
  label,
  onBrowse,
  onChange,
  onResetToDefault,
  placeholder,
  value,
}: {
  advanced?: boolean;
  browseLabel?: string;
  description?: string;
  label: string;
  onBrowse?: () => void;
  onChange: (value: string) => void;
  placeholder?: string;
  value: string;
} & SettingModificationProps) {
  const id = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [inputValue, setInputValue] = useState(value);

  useEffect(() => {
    /*
     * CDXC:SettingsTextFields 2026-06-19-16:53:
     * Immediate-save Settings text fields must keep the user's focused edit
     * buffer while native settings hydration echoes persisted values back into
     * the modal host. Sync external values only when the field is not actively
     * editing so Font Family and command fields do not repaint focus back to
     * Settings search after the first typed character.
     */
    if (inputRef.current?.ownerDocument.activeElement === inputRef.current) {
      return;
    }
    setInputValue(value);
  }, [value]);

  const updateInputValue = (nextValue: string) => {
    setInputValue(nextValue);
    onChange(nextValue);
  };

  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      {onBrowse ? (
        <div className='grid grid-cols-[minmax(0,1fr)_2.5rem] items-center gap-2'>
          <SettingsInput
            id={id}
            className='h-8 px-3 text-[13px]'
            onBlur={(event) => updateInputValue(event.currentTarget.value)}
            onChange={(event) => updateInputValue(event.currentTarget.value)}
            placeholder={placeholder}
            ref={inputRef}
            value={inputValue}
          />
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  aria-label={browseLabel ?? `Browse for ${label}`}
                  className='size-8 rounded-none'
                  onClick={onBrowse}
                  size='icon'
                  type='button'
                  variant='outline'
                >
                  <IconFolderOpen aria-hidden='true' className='size-4' />
                </Button>
              }
            />
            <TooltipContent sideOffset={6}>{browseLabel ?? 'Browse…'}</TooltipContent>
          </Tooltip>
        </div>
      ) : (
        <SettingsInput
          id={id}
          className='h-8 px-3 text-[13px]'
          onBlur={(event) => updateInputValue(event.currentTarget.value)}
          onChange={(event) => updateInputValue(event.currentTarget.value)}
          placeholder={placeholder}
          ref={inputRef}
          value={inputValue}
        />
      )}
    </SettingRow>
  );
}

export function DisabledCommandPreviewField({
  advanced,
  description,
  label,
  value,
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  value: string;
}) {
  const id = useId();
  return (
    <SettingRow advanced={advanced} description={description} htmlFor={id} label={label}>
      <SettingsTextarea
        className='min-h-24 resize-none px-3 py-2 font-mono text-xs leading-5'
        disabled
        id={id}
        readOnly
        value={value}
      />
    </SettingRow>
  );
}

export function ColorField({
  advanced,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  onChange: (value: string) => void;
  value: string;
} & SettingModificationProps) {
  const id = useId();
  const colorValue = normalizeColorInputValue(value);
  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <div className='grid grid-cols-[2.75rem_minmax(0,1fr)] items-center gap-3'>
        <SettingsInput
          aria-label={`${label} picker`}
          className='h-8 cursor-pointer rounded-none p-1'
          onChange={(event) => onChange(event.currentTarget.value)}
          type='color'
          value={colorValue}
        />
        <SettingsInput
          id={id}
          className='h-8 px-3 text-[13px]'
          onChange={(event) => onChange(event.currentTarget.value)}
          value={value}
        />
      </div>
    </SettingRow>
  );
}

export const SIDEBAR_TITLEBAR_TINT_SWATCHES: ReadonlyArray<{ label: string; value: string }> = [
  { label: 'White', value: '#ffffff' },
  { label: 'Neutral Gray', value: DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR },
  { label: 'Black', value: '#000000' },
  { label: 'Steel', value: '#4f6672' },
  { label: 'Red', value: '#884444' },
  { label: 'Orange', value: '#8a5330' },
  { label: 'Amber', value: '#8a6a2f' },
  { label: 'Olive', value: '#657a3f' },
  { label: 'Green', value: '#3f7a5f' },
  { label: 'Teal', value: '#2f7d66' },
  { label: 'Cyan', value: '#287c7f' },
  { label: 'Blue', value: '#336699' },
  { label: 'Indigo', value: '#4f5f96' },
  { label: 'Violet', value: '#6c4f8f' },
  { label: 'Pink', value: '#854f7a' },
  { label: 'Rose', value: '#8a4f5f' },
];

export function WebColorPickerField({
  advanced,
  description,
  isModified,
  label,
  onChange,
  onCommit,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  onChange: (value: string) => void;
  onCommit?: (value: string) => void;
  value: string;
} & SettingModificationProps) {
  const id = useId();
  const savedColorValue = normalizeColorInputValue(value, DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR);
  const [colorText, setColorText] = useState(savedColorValue);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerValue, setPickerValue] = useState(savedColorValue);
  const colorValue = normalizePickerColorValue(colorText, savedColorValue);

  useEffect(() => {
    setColorText(savedColorValue);
    setPickerValue(savedColorValue);
  }, [savedColorValue]);

  const previewColor = (nextColor: string) => {
    const normalizedColor = normalizePickerColorValue(nextColor, DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR);
    setColorText(normalizedColor);
    setPickerValue(nextColor);
    onChange(normalizedColor);
    return normalizedColor;
  };

  const commitColor = (nextColor: string) => {
    const normalizedColor = previewColor(nextColor);
    onCommit?.(normalizedColor);
  };
  const commitColorAfterClosingPicker = (nextColor: string) => {
    /*
     * CDXC:SidebarTitlebarColors 2026-06-19-19:51:
     * The custom tint picker is a nested Base UI dialog inside Settings.
     * Close the dialog before the final setting commit so native settings
     * hydration cannot re-render while the picker still owns modal focus.
     */
    flushSync(() => {
      setPickerOpen(false);
    });
    commitColor(nextColor);
  };

  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      {/*
        CDXC:SidebarTitlebarColors 2026-06-15-15:28:
        Background Tint must be a web picker, not input[type=color], so the
        macOS color panel never opens. Use swatches plus a hex field and let
        shared settings normalize the saved color.

        CDXC:SidebarTitlebarColors 2026-06-15-16:04:
        The first tint picker rendered as a full-width framed popover trigger,
        which made the Settings section look like an empty bordered slab. Keep
        the control inline and compact: swatches first, hex value second, no
        extra container chrome.

        CDXC:SidebarTitlebarColors 2026-06-15-16:13:
        Users need both more tint presets and a way to pick any tint color.
        Keep presets inline, and put the full web picker behind a compact
        swatch trigger so the Settings row does not regain the oversized
        framed surface that was removed.

        CDXC:SidebarTitlebarColors 2026-06-15-16:13:
        Picker dragging should preview immediately from local color state while
        the saved tint setting still uses the existing debounced Settings write
        path before native sidebar/titlebar chrome is updated.

        CDXC:SidebarTitlebarColors 2026-06-15-17:34:
        Replace the hand-built hue picker with the same
        react-best-gradient-color-picker control used in Sharptabs. Keep this
        setting solid-color only, and expose it as a simple Pick Color dialog
        rather than showing technical hue/saturation labels in the Settings row.

        CDXC:SidebarTitlebarColors 2026-06-19-13:44:
        Background Tint presets should scan as neutrals first and then a hue-wheel sequence. Keep only fifteen presets by removing near-duplicate Sky and Purple stops, and use compact row spacing so the custom picker and hex field remain on the same row.

        CDXC:SidebarTitlebarColors 2026-06-19-14:20:
        Add Black to the neutral preset group because white, gray, and black are all valid untinted chrome choices. Keep the input two character cells narrower than the first compact layout so the added swatch does not force the hex field onto a second row.

        CDXC:SidebarTitlebarColors 2026-06-19-14:36:
        The hex field should use exactly the remaining first-row width after the swatches and custom picker. Use a zero-basis flexible input instead of a fixed character width so it fills the right-side remainder without wrapping to a second line.
      */}
      <div className='flex flex-wrap items-center gap-1.5'>
        {SIDEBAR_TITLEBAR_TINT_SWATCHES.map((swatch) => {
          const isSelected = colorValue === swatch.value;
          return (
            <AppTooltip content={swatch.label} key={swatch.value}>
              <Button
                aria-label={`Use ${swatch.label} tint`}
                aria-pressed={isSelected}
                className={cn(
                  'size-7 min-w-0 shrink-0 border p-0',
                  isSelected ? 'border-ring ring-2 ring-ring/45' : 'border-border/80'
                )}
                onClick={() => commitColor(swatch.value)}
                style={{ backgroundColor: swatch.value }}
                type='button'
                variant='ghost'
              />
            </AppTooltip>
          );
        })}
        <AppTooltip content='Pick custom tint color'>
          <Button
            aria-label={`${label} custom color picker`}
            className='h-8 min-w-0 gap-2 px-2 text-xs'
            onClick={() => {
              setPickerValue(colorValue);
              setPickerOpen(true);
            }}
            type='button'
            variant='outline'
          >
            <span
              aria-hidden='true'
              className='size-4 shrink-0 border border-border'
              style={{ backgroundColor: colorValue }}
            />
            <IconPalette aria-hidden='true' data-icon='inline-end' />
          </Button>
        </AppTooltip>
        <Dialog
          open={pickerOpen}
          onOpenChange={(open) => {
            if (!open) {
              commitColorAfterClosingPicker(colorValue);
              return;
            }
            setPickerOpen(open);
          }}
        >
          <DialogContent className='w-[22rem] gap-4 p-4' showCloseButton={false}>
            <DialogHeader>
              <DialogTitle>Pick Color</DialogTitle>
            </DialogHeader>
            <div className='mx-auto'>
              <ColorPicker
                hideAdvancedSliders
                hideColorGuide
                hideColorTypeBtns
                hideEyeDrop
                hideGradientAngle
                hideGradientControls
                hideGradientStop
                hideGradientType
                hideInputType
                hideOpacity
                hidePresets
                idSuffix='sidebar-titlebar-tint'
                onChange={previewColor}
                value={pickerValue}
                width={294}
              />
            </div>
            <DialogFooter>
              <Button
                onClick={() => {
                  commitColorAfterClosingPicker(colorValue);
                }}
                type='button'
              >
                Done
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
        <SettingsInput
          aria-label={`${label} hex color`}
          className='h-8 min-w-0 flex-1 px-2 font-mono text-xs uppercase'
          id={id}
          inputMode='text'
          onBlur={() => commitColor(colorText)}
          onChange={(event) => {
            const nextValue = event.currentTarget.value;
            setColorText(nextValue);
            if (/^#[0-9a-f]{6}$/iu.test(nextValue.trim())) {
              onChange(nextValue.trim().toLowerCase());
            }
          }}
          placeholder={DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR}
          spellCheck={false}
          value={colorText}
        />
      </div>
    </SettingRow>
  );
}

export function normalizeColorInputValue(value: string, fallback = '#121212'): string {
  const normalized = value.trim().toLowerCase();
  return /^#[0-9a-f]{6}$/u.test(normalized) ? normalized : fallback;
}

export function normalizePickerColorValue(value: string, fallback = '#121212'): string {
  const normalized = value.trim().toLowerCase();
  if (/^#[0-9a-f]{6}$/u.test(normalized)) {
    return normalized;
  }
  const rgbMatch = /^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})(?:\s*,\s*(?:0|1|0?\.\d+))?\s*\)$/u.exec(
    normalized
  );
  if (!rgbMatch) {
    return fallback;
  }
  return rgbToHexColor({
    blue: Number(rgbMatch[3] ?? 0),
    green: Number(rgbMatch[2] ?? 0),
    red: Number(rgbMatch[1] ?? 0),
  });
}

export function rgbToHexColor(color: { blue: number; green: number; red: number }): string {
  const toHexComponent = (component: number) => clampNumber(component, 0, 255).toString(16).padStart(2, '0');
  return `#${toHexComponent(color.red)}${toHexComponent(color.green)}${toHexComponent(color.blue)}`;
}

export function SidebarPresetField({
  activePresetId,
  advanced,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
}: {
  activePresetId?: SidebarSettingsPresetId;
  advanced?: boolean;
  description?: string;
  label: string;
  onChange: (presetId: SidebarSettingsPresetId) => void;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <div className='flex flex-col gap-2'>
        <SegmentedControl
          aria-label={label}
          onValueChange={(nextPresetId) => {
            onChange(nextPresetId as SidebarSettingsPresetId);
          }}
          stretch
          value={activePresetId ?? ''}
        >
          {SIDEBAR_SETTINGS_PRESETS.map((preset, index) => (
            <SegmentedControlItem
              aria-label={preset.label}
              id={index === 0 ? id : undefined}
              key={preset.id}
              value={preset.id}
            >
              {preset.label}
            </SegmentedControlItem>
          ))}
        </SegmentedControl>
        {activePresetId ? null : <span className='text-sm text-muted-foreground'>Custom</span>}
      </div>
    </SettingRow>
  );
}

export function SidebarProjectGroupStyleField({
  advanced,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  onChange: (value: SidebarProjectGroupStyle) => void;
  value: SidebarProjectGroupStyle;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <SegmentedControl
        aria-label={label}
        onValueChange={(nextValue) => {
          onChange(nextValue as SidebarProjectGroupStyle);
        }}
        stretch
        value={value}
      >
        {SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS.map((option, index) => (
          <SegmentedControlItem
            aria-label={option.label}
            id={index === 0 ? id : undefined}
            key={option.value}
            value={option.value}
          >
            {option.label}
          </SegmentedControlItem>
        ))}
      </SegmentedControl>
    </SettingRow>
  );
}

export function TerminalViewWidthModeField({
  advanced,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  onChange: (value: TerminalViewWidthMode) => void;
  value: TerminalViewWidthMode;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <SegmentedControl
        aria-label={label}
        onValueChange={(nextValue) => onChange(nextValue as TerminalViewWidthMode)}
        stretch
        value={value}
      >
        {TERMINAL_VIEW_WIDTH_MODE_OPTIONS.map((option, index) => (
          <SegmentedControlItem
            aria-label={option.label}
            id={index === 0 ? id : undefined}
            key={option.value}
            value={option.value}
          >
            {option.label}
          </SegmentedControlItem>
        ))}
      </SegmentedControl>
    </SettingRow>
  );
}

/*
 * CDXC:SidebarSpaces 2026-08-28:
 * Spaces is a feature switch rather than a density tweak, so it reads as the
 * same combined button the Project group style row above it uses instead of the
 * small toggle the per-row visibility settings use.
 */
export function SidebarSpacesField({
  advanced,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  onChange: (value: boolean) => void;
  value: boolean;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <SegmentedControl
        aria-label={label}
        onValueChange={(nextValue) => {
          onChange(nextValue === 'on');
        }}
        stretch
        value={value ? 'on' : 'off'}
      >
        {SIDEBAR_SPACES_ENABLED_OPTIONS.map((option, index) => (
          <SegmentedControlItem
            aria-label={option.label}
            id={index === 0 ? id : undefined}
            key={option.value}
            value={option.value}
          >
            {option.label}
          </SegmentedControlItem>
        ))}
      </SegmentedControl>
    </SettingRow>
  );
}

export function PreferredAgentInterfaceField({
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  description?: string;
  label: string;
  onChange: (value: PreferredAgentInterface) => void;
  value: PreferredAgentInterface;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <SegmentedControl
        aria-label={label}
        onValueChange={(nextInterface) => {
          onChange(nextInterface as PreferredAgentInterface);
        }}
        stretch
        value={value}
      >
        {PREFERRED_AGENT_INTERFACE_OPTIONS.map((option, index) => (
          <SegmentedControlItem
            aria-label={option.label}
            id={index === 0 ? id : undefined}
            key={option.value}
            value={option.value}
          >
            {option.label}
          </SegmentedControlItem>
        ))}
      </SegmentedControl>
    </SettingRow>
  );
}

export function SessionChatThemeField({
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  description?: string;
  label: string;
  onChange: (value: SessionChatTheme) => void;
  value: SessionChatTheme;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <SegmentedControl
        aria-label={label}
        onValueChange={(nextValue) => {
          onChange(nextValue as SessionChatTheme);
        }}
        stretch
        value={value}
      >
        {SESSION_CHAT_THEME_OPTIONS.map((option, index) => (
          <SegmentedControlItem
            aria-label={option.label}
            id={index === 0 ? id : undefined}
            key={option.value}
            value={option.value}
          >
            {option.label}
          </SegmentedControlItem>
        ))}
      </SegmentedControl>
    </SettingRow>
  );
}

export function ToggleField({
  advanced,
  checked,
  description,
  disabled,
  disabledReason,
  isModified,
  label,
  onChange,
  onResetToDefault,
  subtitle,
}: {
  advanced?: boolean;
  checked: boolean;
  description?: string;
  disabled?: boolean;
  disabledReason?: string;
  label: string;
  onChange: (checked: boolean) => void;
  subtitle?: string;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
      subtitle={subtitle}
    >
      {disabled && disabledReason ? (
        <SettingSwitch checked={checked} disabled disabledReason={disabledReason} id={id} onCheckedChange={onChange} />
      ) : (
        <Switch checked={checked} disabled={disabled} id={id} onCheckedChange={onChange} />
      )}
    </SettingRow>
  );
}

export function DiagnosticLoggingSettingsField({
  isModified,
  onChange,
  onResetToDefault,
  value,
}: {
  isModified?: boolean;
  onChange: (scenarioId: DiagnosticLoggingScenarioId, duration: DiagnosticLoggingDurationValue) => void;
  onResetToDefault?: () => void;
  value: DiagnosticLoggingSettings;
}) {
  const idBase = useId();
  return (
    <SettingRow
      description='Routine logs are off by default and write only when Show debug UI controls and their scenario are enabled. Enable only the repro area you need; important warnings, errors, and crashes remain captured.'
      htmlFor={`${idBase}-native-terminal-focus`}
      isModified={isModified}
      label='Diagnostic disk logging scenarios'
      onResetToDefault={onResetToDefault}
    >
      <div className='grid gap-4'>
        {DIAGNOSTIC_LOGGING_GROUPS.map((group) => {
          const scenarios = DIAGNOSTIC_LOGGING_SCENARIOS.filter((scenario) => scenario.group === group);
          return (
            <div className='grid gap-2' key={group}>
              <div className='text-xs font-medium uppercase tracking-normal text-muted-foreground'>{group}</div>
              <div className='grid gap-2'>
                {scenarios.map((scenario) => {
                  const scenarioId = scenario.id as DiagnosticLoggingScenarioId;
                  const duration = getDiagnosticLoggingScenarioDuration(value, scenarioId);
                  const checked = duration !== 'off';
                  const switchId = `${idBase}-${scenario.id.replaceAll('.', '-')}`;
                  return (
                    <div
                      className='grid gap-2 border-t border-border/70 pt-2 first:border-t-0 first:pt-0'
                      key={scenario.id}
                    >
                      <div className='flex min-w-0 items-start justify-between gap-3'>
                        <div className='min-w-0'>
                          <FieldLabel className='text-sm' htmlFor={switchId}>
                            {scenario.label}
                          </FieldLabel>
                          <div className='mt-0.5 break-words text-xs text-muted-foreground'>
                            {scenario.logFiles.join(', ')}
                          </div>
                        </div>
                        <Switch
                          checked={checked}
                          id={switchId}
                          onCheckedChange={(nextChecked) =>
                            onChange(scenarioId, nextChecked ? DEFAULT_DIAGNOSTIC_LOGGING_ENABLE_DURATION : 'off')
                          }
                        />
                      </div>
                      {checked ? (
                        <SettingsSelect
                          onValueChange={(nextValue) =>
                            onChange(scenarioId, nextValue as DiagnosticLoggingDurationValue)
                          }
                          value={duration}
                        >
                          <SelectTrigger className='h-8 w-full sm:w-36'>
                            <SelectValue />
                          </SelectTrigger>
                          <SettingsSelectContent>
                            {DIAGNOSTIC_LOGGING_DURATION_OPTIONS.filter((option) => option.value !== 'off').map(
                              (option) => (
                                <SelectItem key={option.value} value={option.value}>
                                  {option.label}
                                </SelectItem>
                              )
                            )}
                          </SettingsSelectContent>
                        </SettingsSelect>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}
      </div>
    </SettingRow>
  );
}

export function getDiagnosticLoggingScenarioDuration(
  value: DiagnosticLoggingSettings,
  scenarioId: DiagnosticLoggingScenarioId,
  now: Date = new Date()
): DiagnosticLoggingDurationValue {
  const scenario = value.scenarios[scenarioId];
  if (!scenario?.enabled) {
    return 'off';
  }
  if (!scenario.expiresAt) {
    return 'always';
  }
  const expiresAtMs = Date.parse(scenario.expiresAt);
  if (!Number.isFinite(expiresAtMs) || expiresAtMs <= now.getTime()) {
    return 'off';
  }
  const remainingMs = expiresAtMs - now.getTime();
  return remainingMs <= 30 * 60 * 1000 ? '15m' : '1h';
}

export function getDiagnosticLoggingScenarioStateForDuration(
  duration: DiagnosticLoggingDurationValue,
  now: Date = new Date()
) {
  /*
   * CDXC:ChromeResponsivenessDiagnostics 2026-06-30-23:52:
   * Some lag/crash diagnostic scenarios are enabled by default so repro logs
   * exist immediately after update. Persist Off as an explicit disabled state
   * instead of deleting the scenario so Settings can override those defaults.
   */
  switch (duration) {
    case '15m':
      return {
        enabled: true,
        expiresAt: new Date(now.getTime() + 15 * 60 * 1000).toISOString(),
      };
    case '1h':
      return {
        enabled: true,
        expiresAt: new Date(now.getTime() + 60 * 60 * 1000).toISOString(),
      };
    case 'always':
      return { enabled: true };
    case 'off':
      return { enabled: false };
  }
}

export function SidebarTagListSettingsField({
  isModified,
  items,
  onChange,
  onResetToDefault,
}: {
  isModified: boolean;
  items: readonly SidebarSessionTagListItem[];
  onChange: (items: readonly SidebarSessionTagListItem[]) => void;
  onResetToDefault: () => void;
}) {
  const normalizedItems = normalizeSidebarSessionTagListItems(items);
  const handleDragEnd = ((event) => {
    if (event.canceled || !isSortableOperation(event.operation)) {
      return;
    }

    const { source, target } = event.operation;
    const sourceData = source ? getSettingsSidebarTagListItemDragData(source) : undefined;
    if (!source || !sourceData) {
      return;
    }

    const targetIndex = 'index' in source && typeof source.index === 'number' ? source.index : target?.index;
    if (targetIndex == null || source.initialIndex === targetIndex) {
      return;
    }

    const itemsById = new Map<string, SidebarSessionTagListItem>(normalizedItems.map((item) => [item.id, item]));
    onChange(
      moveId(
        normalizedItems.map((item) => item.id),
        source.initialIndex,
        targetIndex
      ).flatMap((itemId) => itemsById.get(itemId) ?? [])
    );
  }) satisfies DragDropEventHandlers['onDragEnd'];

  const updateItem = (itemId: string, patch: Partial<Pick<SidebarSessionTagListItem, 'enabled' | 'visible'>>) => {
    onChange(
      normalizedItems.map((item) =>
        item.id === itemId
          ? ({
              ...item,
              ...patch,
            } as SidebarSessionTagListItem)
          : item
      )
    );
  };
  const updateItemEnabled = (itemId: string, enabled: boolean) => {
    /*
     * CDXC:SessionTagFilters 2026-06-15-22:10:
     * The Settings switch is the primary on/off control for tag filters.
     * Switching a row off should also hide it from the sidebar filter menu,
     * while switching it back on restores visibility so the eye icon and switch
     * cannot drift into a half-on reset state.
     */
    updateItem(itemId, { enabled, visible: enabled });
  };

  const updateItemVisible = (itemId: string, visible: boolean) => {
    /*
     * CDXC:SessionTagFilters 2026-06-15-22:10:
     * The eye button mirrors the same on/off model as the switch for tag rows.
     * Showing a hidden row should re-enable it; hiding a row should disable it
     * so Settings does not present hidden filters as enabled.
     */
    updateItem(itemId, { enabled: visible, visible });
  };

  return (
    <details className='group w-full'>
      {/*
       * CDXC:SessionTagFilters 2026-06-13-17:50:
       * The bottom main Settings area starts collapsed and mirrors the
       * configurable-list chrome used by tab context menu item settings:
       * full-width rows, drag handles, enabled switches, and visibility icons.
       * Separators are real rows so users can move or hide them with tags.
       *
       * CDXC:SessionTagFilters 2026-06-15-14:02:
       * The expanded Sidebar Tags list should attach directly to the disclosure
       * header; no vertical gutter belongs between the header and its rows.
       */}
      <summary className='settings-management-row flex cursor-pointer list-none items-center justify-between gap-3 border border-border bg-muted/20 px-3 py-3 marker:hidden [&::-webkit-details-marker]:hidden'>
        <div className='flex min-w-0 flex-1 items-center gap-2.5'>
          <IconChevronRight
            aria-hidden='true'
            className='size-4 shrink-0 text-muted-foreground transition-transform duration-150 group-open:rotate-90'
          />
          <FieldContent className='min-w-0 gap-1'>
            <FieldLabel className='text-sm'>Tag filter list</FieldLabel>
            <FieldDescription className='text-xs text-muted-foreground'>
              Reorder, hide, or disable sidebar tag filters and separators.
            </FieldDescription>
          </FieldContent>
        </div>
        <SettingButton
          disabled={!isModified}
          disabledReason='These tag settings already match the defaults.'
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onResetToDefault();
          }}
          type='button'
          variant='outline'
        >
          Reset to Default
        </SettingButton>
      </summary>
      <div className='border border-border/80 bg-muted/10 p-3'>
        <DragDropProvider onDragEnd={handleDragEnd}>
          <div className='flex w-full flex-col gap-2'>
            {normalizedItems.map((item, index) => (
              <SidebarTagListSettingsRow
                index={index}
                item={item}
                key={item.id}
                onEnabledChange={(enabled) => updateItemEnabled(item.id, enabled)}
                onVisibleChange={(visible) => updateItemVisible(item.id, visible)}
              />
            ))}
          </div>
        </DragDropProvider>
      </div>
    </details>
  );
}

export function SidebarTagListSettingsRow({
  index,
  item,
  onEnabledChange,
  onVisibleChange,
}: {
  index: number;
  item: SidebarSessionTagListItem;
  onEnabledChange: (enabled: boolean) => void;
  onVisibleChange: (visible: boolean) => void;
}) {
  const sortable = useSortable({
    accept: 'settings-sidebar-tag-list-item',
    data: createSettingsSidebarTagListItemDragData(item.id),
    group: 'settings-sidebar-tag-list-items',
    id: item.id,
    index,
    type: 'settings-sidebar-tag-list-item',
  });
  const { handleRef, isDragging } = sortable;
  const isDimmed = !item.enabled || !item.visible;
  const label = getSidebarSessionTagListItemLabel(item);

  const setRowRef = (element: HTMLDivElement | null) => {
    setSettingsSortableRowElement(sortable, element);
  };

  return (
    <div
      className={cn(
        'settings-management-row flex w-full items-center gap-2 border border-border bg-muted/20 p-2',
        isDimmed && 'text-muted-foreground'
      )}
      data-dragging={String(Boolean(isDragging))}
      data-enabled={String(item.enabled)}
      data-visible={String(item.visible)}
      ref={setRowRef}
    >
      <Button aria-label={`Reorder ${label}`} ref={handleRef} size='icon' type='button' variant='ghost'>
        <IconGripVertical aria-hidden='true' />
      </Button>
      <div className='flex min-w-0 flex-1 items-center gap-3 px-2 py-2'>
        <span
          aria-hidden='true'
          className='settings-management-icon flex size-8 shrink-0 items-center justify-center bg-muted'
        >
          {item.type === 'separator' ? (
            <IconMinus className='text-muted-foreground' size={16} stroke={2} />
          ) : (
            <SessionTagIcon
              className='session-tag-colored-icon'
              fillFavorite
              size={15}
              stroke={1.8}
              tag={item.type === 'untagged' ? 'untagged' : item.tag}
            />
          )}
        </span>
        <span className='min-w-0 flex-1'>
          <span
            className={cn(
              'block truncate text-sm font-medium',
              item.type === 'separator' && 'italic text-muted-foreground'
            )}
          >
            {label}
          </span>
        </span>
      </div>
      <Switch
        aria-label={`${item.enabled ? 'Disable' : 'Enable'} ${label}`}
        checked={item.enabled}
        onCheckedChange={onEnabledChange}
      />
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              aria-label={`${item.visible ? 'Hide' : 'Show'} ${label}`}
              className='shrink-0'
              onClick={() => onVisibleChange(!item.visible)}
              size='icon'
              type='button'
              variant='ghost'
            >
              {item.visible ? (
                <IconEye aria-hidden='true' size={16} stroke={1.9} />
              ) : (
                <IconEyeOff aria-hidden='true' size={16} stroke={1.9} />
              )}
            </Button>
          }
        />
        <TooltipContent sideOffset={6}>{item.visible ? 'Hide' : 'Show'}</TooltipContent>
      </Tooltip>
    </div>
  );
}

/**
 * CDXC:Settings 2026-05-06-12:57
 * CDXC:SettingsModifiedState 2026-05-07-18:03
 * Every changed settings control needs a small, low-emphasis asterisk to the
 * left of its label. Position it absolutely so modified-state indication does
 * not reflow setting titles, while the tooltip action still resets only that
 * setting to DEFAULT_ghostex_SETTINGS.
 *
 * CDXC:SettingsDensity 2026-06-15-20:53:
 * Main Settings rows should not show explanatory subtitles inline because the
 * modal needs to stay dense and scannable. Reveal a compact info trigger only
 * while the row is hovered or focused, then show the description in a
 * right-side tooltip capped at 350px.
 *
 * CDXC:SettingsAdvanced 2026-06-16-10:40:
 * Advanced rows should no longer use a text badge. Mark them with a light blue
 * up-arrow affordance beside the label actions, immediately before the info
 * button when one is present, so the label stays compact while hover explains
 * the row as an Advanced Setting.
 *
 * CDXC:SettingsAdvanced 2026-06-16-18:22:
 * The advanced up-arrow is a persistent scan marker, not hover-only chrome, and
 * needs a small gap from the label so advanced rows are visible at rest.
 */
export function SettingRow({
  advanced,
  badge,
  children,
  description,
  htmlFor,
  isModified,
  label,
  onResetToDefault,
  subtitle,
}: {
  advanced?: boolean;
  /** Rows for newly shipped settings may carry a short label badge. */
  badge?: string;
  children: ReactNode;
  description?: string;
  htmlFor: string;
  isModified?: boolean;
  label: string;
  onResetToDefault?: () => void;
  subtitle?: string;
}) {
  return (
    <Field className='settings-row gap-2.5' orientation='vertical'>
      <FieldContent>
        <FieldTitle className='settings-row-title text-sm'>
          <span className='settings-row-label-line'>
            {isModified && onResetToDefault ? (
              <ModifiedSettingResetButton label={label} onResetToDefault={onResetToDefault} />
            ) : null}
            <FieldLabel className='text-sm' htmlFor={htmlFor}>
              {label}
            </FieldLabel>
            {badge ? (
              /* The badge uses the modal theme tokens and a quiet raised chip. */
              <span className='settings-row-badge inline-flex px-1.5 py-0.5 text-[11px] font-normal'>{badge}</span>
            ) : null}
            {advanced ? <AdvancedSettingTooltip label={label} /> : null}
            {description ? <SettingDescriptionTooltip description={description} label={label} /> : null}
          </span>
        </FieldTitle>
        {subtitle ? <FieldDescription className='settings-row-subtitle'>{subtitle}</FieldDescription> : null}
        {description ? <FieldDescription className='sr-only'>{description}</FieldDescription> : null}
      </FieldContent>
      <div className='min-w-0'>{children}</div>
    </Field>
  );
}

export function AdvancedSettingTooltip({ label }: { label: string }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button aria-label={`${label} is an advanced setting`} className='settings-row-advanced-button' type='button'>
            <IconArrowBigUp aria-hidden='true' />
          </button>
        }
      />
      <TooltipContent sideOffset={6}>Advanced Setting</TooltipContent>
    </Tooltip>
  );
}

export function SettingDescriptionTooltip({ description, label }: { description: string; label: string }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button aria-label={`${label} setting details`} className='settings-row-info-button' type='button'>
            <IconInfoCircle aria-hidden='true' />
          </button>
        }
      />
      <TooltipContent
        className='settings-row-info-tooltip'
        side='right'
        sideOffset={8}
        style={{ maxWidth: 'min(350px, calc(100vw - 32px))' }}
      >
        {description}
      </TooltipContent>
    </Tooltip>
  );
}

export function ModifiedSettingResetButton({
  label,
  onResetToDefault,
}: {
  label: string;
  onResetToDefault: () => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            aria-label={`Reset ${label} to default`}
            className='settings-modified-reset-button'
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onResetToDefault();
            }}
            size='icon-xs'
            type='button'
            variant='ghost'
          >
            <IconAsterisk aria-hidden='true' />
          </Button>
        }
      />
      <TooltipContent className='whitespace-pre-line text-center' sideOffset={6}>
        {MODIFIED_SETTING_TOOLTIP}
      </TooltipContent>
    </Tooltip>
  );
}
