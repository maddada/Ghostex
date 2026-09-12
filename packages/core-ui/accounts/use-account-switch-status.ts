import { useEffect, useRef, useState } from 'react';
import type { AccountSwitchProgress } from '@/packages/shared/agent-accounts';

/** Only the brief success acknowledgement uses a timer; every in-flight step comes from gxserver. */
export function useAccountSwitchStatus(progress: AccountSwitchProgress | null, sessionKey: string | undefined) {
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now);
  const observed = useRef<string | null>(null);
  useEffect(() => {
    setDismissed(null);
    observed.current = null;
  }, [sessionKey]);
  useEffect(() => {
    if (!progress) return;
    if (progress.phase !== 'success') {
      observed.current = progress.id;
      return;
    }
    // An old completed switch must not reappear when reopening a conversation.
    const recent = Date.now() - Date.parse(progress.updatedAt) < 5000;
    if (observed.current !== progress.id && !recent) {
      setDismissed(progress.id);
      return;
    }
    const timer = window.setTimeout(() => setDismissed(progress.id), 1800);
    return () => window.clearTimeout(timer);
  }, [progress?.id, progress?.phase, progress?.updatedAt]);
  const visible =
    progress &&
    progress.phase !== 'cancelled' &&
    dismissed !== progress.id &&
    (progress.phase !== 'success' ||
      observed.current === progress.id ||
      Date.now() - Date.parse(progress.updatedAt) < 5000)
      ? progress
      : null;
  useEffect(() => {
    if (!visible) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 30000);
    return () => window.clearInterval(timer);
  }, [visible?.id]);
  return { visible, now, busy: !!progress && ['switching', 'resuming', 'continuing'].includes(progress.phase) };
}
