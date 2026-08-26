import { cn } from '@/packages/components/utils';

const MAX_PRESERVED_END_LENGTH = 24;

interface MiddleEllipsisTextProps {
  readonly className?: string;
  readonly value: string;
}

export function MiddleEllipsisText({ className, value }: MiddleEllipsisTextProps) {
  const lastSeparatorIndex = Math.max(value.lastIndexOf('/'), value.lastIndexOf('\\'));
  const separatorSplitIndex = lastSeparatorIndex + 1;
  const splitIndex =
    separatorSplitIndex > 0 && value.length - separatorSplitIndex <= MAX_PRESERVED_END_LENGTH
      ? separatorSplitIndex
      : Math.max(1, value.length - MAX_PRESERVED_END_LENGTH);

  return (
    <span className={cn('flex min-w-0 whitespace-nowrap', className)} title={value}>
      <span className='min-w-0 truncate'>{value.slice(0, splitIndex)}</span>
      <span className='shrink-0'>{value.slice(splitIndex)}</span>
    </span>
  );
}
