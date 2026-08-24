import { AppTooltip } from '@/packages/core-ui/app-tooltip';
import { ManageTooltipButtonProps } from './types';

export function ManageTooltipButton({ tooltip, ...buttonProps }: ManageTooltipButtonProps) {
  return (
    <AppTooltip content={tooltip}>
      <button {...buttonProps} />
    </AppTooltip>
  );
}
