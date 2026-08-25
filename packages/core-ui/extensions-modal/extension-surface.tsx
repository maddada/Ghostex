import type { ReactNode } from 'react';
import type { Icon as TablerIcon } from '@tabler/icons-react';
import { cn } from '@/packages/components/utils';

export function ExtensionEmptyState({
  action,
  description,
  icon: Icon,
  title,
}: {
  action?: ReactNode;
  description: string;
  icon: TablerIcon;
  title: string;
}) {
  return (
    <section className='flex min-h-0 flex-1 flex-col items-center justify-center gap-2.5 p-8 text-center'>
      <div className='mb-1 flex size-12 items-center justify-center rounded-xl border border-border/80 bg-white/[0.04] text-muted-foreground'>
        <Icon aria-hidden='true' className='size-6' />
      </div>
      <span className='text-sm font-normal text-foreground'>{title}</span>
      <p className='max-w-xs text-[13px] font-normal leading-relaxed text-muted-foreground'>{description}</p>
      {action ? <div className='mt-2'>{action}</div> : null}
    </section>
  );
}

export function ExtensionSectionLabel({ children, id }: { children: ReactNode; id?: string }) {
  return (
    <h3 className='text-[13px] font-normal text-muted-foreground' id={id}>
      {children}
    </h3>
  );
}

export function ExtensionGroup({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={cn(
        'divide-y divide-border/60 overflow-hidden rounded-xl border border-border/80 bg-white/[0.03]',
        className
      )}
    >
      {children}
    </div>
  );
}
