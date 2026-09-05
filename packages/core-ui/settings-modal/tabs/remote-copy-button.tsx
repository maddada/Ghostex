import { useState, type ReactNode } from 'react';
import { Button } from '@/packages/components/ui/button';
import { IconCircleCheckFilled, IconCopy } from '@tabler/icons-react';
import { AppTooltip } from '../../app-tooltip';

/** Copies `value` to the clipboard and flashes a check for a moment. */
export function RemoteCopyButton({
  children,
  className,
  copyLabel,
  size = 'icon-xs',
  value,
  variant = 'ghost',
}: {
  children?: ReactNode;
  className?: string;
  copyLabel: string;
  size?: 'icon-xs' | 'icon' | 'xs' | 'sm';
  value: string;
  variant?: 'ghost' | 'outline' | 'secondary' | 'default';
}) {
  const [copied, setCopied] = useState(false);
  return (
    <AppTooltip content={copied ? 'Copied' : copyLabel}>
      <Button
        aria-label={copyLabel}
        className={className}
        onClick={() => {
          void navigator.clipboard.writeText(value).then(
            () => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1200);
            },
            () => undefined
          );
        }}
        size={size}
        type='button'
        variant={variant}
      >
        {copied ? (
          <IconCircleCheckFilled aria-hidden='true' data-icon={children ? 'inline-start' : undefined} />
        ) : (
          <IconCopy aria-hidden='true' data-icon={children ? 'inline-start' : undefined} />
        )}
        {children}
      </Button>
    </AppTooltip>
  );
}
