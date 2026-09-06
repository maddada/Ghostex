import { useCallback, useEffect, useRef, useState } from 'react';
import type { AccountsTransport, AgentAccountsRequest, AgentAccountsState } from '@/packages/shared/agent-accounts';
export function useAccounts(transport: AccountsTransport | undefined, session = false, active = true) {
  const [data, setData] = useState<AgentAccountsState>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);
  const pending = useRef(false);
  const request = useCallback(
    async (params: AgentAccountsRequest) => {
      if (!transport) return false;
      const id = ++generation.current;
      pending.current = true;
      setBusy(true);
      setError('');
      try {
        const result = await transport(params);
        if (generation.current === id) setData(result);
        return true;
      } catch (e) {
        if (generation.current === id) setError(e instanceof Error ? e.message : 'Account request failed.');
        return false;
      } finally {
        if (generation.current === id) {
          setBusy(false);
          pending.current = false;
        }
      }
    },
    [transport]
  );
  useEffect(() => {
    setData(undefined);
    if (!active || !transport) return;
    void request({ operation: session ? 'session' : 'list' });
    const timer = setInterval(() => {
      if (!document.hidden && !pending.current) void request({ operation: session ? 'session' : 'list' });
    }, 30000);
    return () => {
      clearInterval(timer);
      generation.current++;
      pending.current = false;
    };
  }, [active, transport, session, request]);
  return { data, error, busy, request };
}
