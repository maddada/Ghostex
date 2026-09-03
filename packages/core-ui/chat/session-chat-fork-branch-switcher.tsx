/*
CDXC:SessionFork 2026-08-28:
The chat's branch switcher. A Codex fork keeps the earlier conversation on the
session it branched off, and Previous Sessions hides that ancestor once
something continues from it, so without this control a user who forked can no
longer reach the thread they forked away from. `/api/sessionForkBranches`
answers with the whole family, ancestors included, and this renders it as one
compact control that only exists when there is actually something to switch
between.

It asks ONCE per session, lazily, and caches the answer for the lifetime of the
page: the family only changes when a session is forked or retired, both of
which land the user on a different session key anyway. A daemon or host that
cannot answer leaves the control unrendered rather than showing an empty menu.

CDXC:SessionFork 2026-09-03:
Picking a STOPPED branch revives that same registry row (the hosts wake it
before focusing), so the row says so instead of silently doing nothing. A host
that cannot switch omits `onSelectBranch` and the rows stay disabled.
*/
import { IconGitBranch } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import type { GxserverSessionForkBranch } from '../../shared/gxserver-protocol';
import { cn } from '@/packages/components/utils';
import { Button } from '../../components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '../../components/ui/dropdown-menu';
import { formatRelativeTimeLabel } from '../relative-time';

export interface SessionChatForkBranchSwitcherProps {
  /** Stable identity of the conversation; a change re-asks for the family. */
  sessionKey: string;
  /** Transport-backed reader; omitted by hosts without a route to the endpoint. */
  loadBranches?: () => Promise<{ branches: readonly GxserverSessionForkBranch[] }>;
  /**
   * Navigates the host to another branch. Hosts that cannot switch sessions
   * from this surface omit it and the rows render as a read-only list.
   */
  onSelectBranch?: (branch: GxserverSessionForkBranch) => void;
}

/** Answers already fetched on this page, keyed by session. */
const branchCache = new Map<string, readonly GxserverSessionForkBranch[]>();

function branchLifecycleDotClassName(branch: GxserverSessionForkBranch): string {
  if (branch.lifecycleState === 'running') {
    return 'bg-emerald-500';
  }
  if (branch.lifecycleState === 'sleeping') {
    return 'bg-muted-foreground/60';
  }
  return 'bg-muted-foreground/35';
}

function branchLastActiveLabel(branch: GxserverSessionForkBranch): string {
  if (!Number.isFinite(branch.lastActiveMs) || branch.lastActiveMs <= 0) {
    return '';
  }
  return formatRelativeTimeLabel(new Date(branch.lastActiveMs).toISOString());
}

export function SessionChatForkBranchSwitcher({
  loadBranches,
  onSelectBranch,
  sessionKey,
}: SessionChatForkBranchSwitcherProps) {
  const [branches, setBranches] = useState<readonly GxserverSessionForkBranch[]>(
    () => branchCache.get(sessionKey) ?? []
  );

  useEffect(() => {
    const cached = branchCache.get(sessionKey);
    if (cached) {
      setBranches(cached);
      return;
    }
    setBranches([]);
    if (!loadBranches) {
      return;
    }
    let cancelled = false;
    void loadBranches()
      .then((result) => {
        const next = result.branches ?? [];
        branchCache.set(sessionKey, next);
        if (!cancelled) {
          setBranches(next);
        }
      })
      .catch(() => {
        /*
        A daemon that predates the endpoint, or a machine that dropped mid
        request. Nothing is cached, so the next mount of this session asks
        again; the control simply stays hidden until an answer arrives.
        */
      });
    return () => {
      cancelled = true;
    };
  }, [loadBranches, sessionKey]);

  // One branch is not a family: there is nothing to switch to.
  if (branches.length < 2) {
    return null;
  }

  const tooltip = `This conversation has ${branches.length} branches that share earlier history.`;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            aria-label={tooltip}
            className='h-6 gap-1 px-1.5 text-[11px] font-normal text-muted-foreground'
            size='sm'
            title={tooltip}
            variant='ghost'
          />
        }
      >
        <IconGitBranch aria-hidden='true' className='size-3.5' stroke={2} />
        {branches.length}
      </DropdownMenuTrigger>
      <DropdownMenuContent align='end' className='w-72 min-w-72'>
        {/*
        Base UI's GroupLabel needs a Group context and throws (error #31) without one.
        With no error boundary in the chat page that unmounted the whole transcript.
        */}
        <DropdownMenuGroup>
          <DropdownMenuLabel>Branches</DropdownMenuLabel>
          {branches.map((branch) => {
            const lastActive = branchLastActiveLabel(branch);
            return (
              <DropdownMenuItem
                disabled={branch.current || !onSelectBranch}
                key={`${branch.projectId}:${branch.sessionId}`}
                onClick={() => {
                  if (!branch.current) {
                    onSelectBranch?.(branch);
                  }
                }}
              >
                <span
                  aria-hidden='true'
                  className={cn('size-1.5 shrink-0 rounded-full', branchLifecycleDotClassName(branch))}
                />
                <span className='flex min-w-0 flex-1 flex-col gap-0.5'>
                  <span className='truncate'>{branch.title || 'Untitled session'}</span>
                  <span className='truncate text-[11px] text-muted-foreground'>
                    {[
                      branch.ancestor ? 'Earlier thread' : '',
                      branch.lifecycleState === 'stopped' && !branch.current ? 'Resumes when opened' : '',
                      lastActive,
                    ]
                      .filter(Boolean)
                      .join(' · ')}
                  </span>
                </span>
                {branch.current ? <span className='shrink-0 text-[11px] text-muted-foreground'>Current</span> : null}
              </DropdownMenuItem>
            );
          })}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
