/*
 * CDXC:Extensions 2026-08-30:
 * The Markdown/HTML "open in Docs or Code" controls used to live on the
 * Customize page, which became the Extensions page. They are ordinary app
 * behaviour rather than an extension, so they moved to General → Tools → File
 * opening. The control itself moved verbatim so the keys, values, and segmented
 * control behaviour are unchanged.
 */
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { CHAT_FILE_OPEN_VIEW_OPTIONS, type ChatFileOpenView } from '../../shared/ghostex-settings';
import { SettingRow } from './fields';

export function ChatFileOpenViewSetting({
  id,
  label,
  onChange,
  subtitle,
  value,
}: {
  id: string;
  label: string;
  onChange: (value: ChatFileOpenView) => void;
  subtitle: string;
  value: ChatFileOpenView;
}) {
  return (
    <SettingRow htmlFor={id} label={label} subtitle={subtitle}>
      <SegmentedControl
        aria-label={`${label} open view`}
        id={id}
        onValueChange={(nextValue) => onChange(nextValue as ChatFileOpenView)}
        size='sm'
        value={value}
      >
        {CHAT_FILE_OPEN_VIEW_OPTIONS.map((option) => (
          <SegmentedControlItem key={option.value} value={option.value}>
            {option.label}
          </SegmentedControlItem>
        ))}
      </SegmentedControl>
    </SettingRow>
  );
}
