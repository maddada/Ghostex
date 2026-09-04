import { useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';

export function AgentApprovalPolicyControl({
  className,
  disabled = false,
  enabled,
  id,
  onChange,
  size = 'default',
}: {
  className?: string;
  disabled?: boolean;
  enabled: boolean;
  id?: string;
  onChange: (enabled: boolean) => void;
  size?: 'default' | 'sm';
}) {
  const [confirmingBypass, setConfirmingBypass] = useState(false);

  return (
    <>
      <SegmentedControl
        aria-label='Agent approvals'
        className={className}
        disabled={disabled}
        id={id}
        onValueChange={(value) => {
          if (value === 'bypass') {
            setConfirmingBypass(true);
            return;
          }
          onChange(false);
        }}
        size={size}
        value={enabled ? 'bypass' : 'ask'}
      >
        <SegmentedControlItem value='ask'>Keep default</SegmentedControlItem>
        <SegmentedControlItem value='bypass'>Skip permissions</SegmentedControlItem>
      </SegmentedControl>

      <Dialog onOpenChange={setConfirmingBypass} open={confirmingBypass}>
        <DialogContent className='ghostex-settings-shadcn w-[25rem] gap-4 p-5' nested showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>Skip permissions?</DialogTitle>
            <DialogDescription>
              Supported agents may edit files and run commands without asking you first. Only enable this for agents and
              projects you trust.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button onClick={() => setConfirmingBypass(false)} type='button' variant='outline'>
              Cancel
            </Button>
            <Button
              onClick={() => {
                onChange(true);
                setConfirmingBypass(false);
              }}
              type='button'
              variant='destructive'
            >
              Skip permissions
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
