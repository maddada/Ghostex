import { useCallback, useEffect, useRef, useState } from 'react';
import type { SessionChatSkill } from '@/packages/shared/session-chat';
import type { SessionChatTransport } from './session-chat-transport';

interface SkillLoadState {
  transport: SessionChatTransport;
  agentId: string | null;
  skills?: readonly SessionChatSkill[];
  loading: boolean;
  error?: string;
}

/**
 * CDXC:AgentSkills 2026-09-11 WHY:
 * A failed mount-time request used to become a permanently empty skills list in retained desktop chats.
 * Cache successful reads only; opening the picker again or pressing Retry can repeat a failed read, and changing agents invalidates the catalog.
 */
export function useSessionChatSkills(transport: SessionChatTransport, agentId: string | null) {
  const [state, setState] = useState<SkillLoadState>();
  const requestRef = useRef<(() => void) | null>(null);
  const requestSkills = useCallback(() => requestRef.current?.(), []);

  useEffect(() => {
    let active = true;
    let loading = false;
    let loaded = false;
    const readSkills = transport.readSkills?.bind(transport);
    const publish = (result: Pick<SkillLoadState, 'skills' | 'loading' | 'error'>): void => {
      if (active) setState({ transport, agentId, ...result });
    };
    const load = async (): Promise<void> => {
      if (!active || loading || loaded || !readSkills) return;
      loading = true;
      publish({ loading: true });
      try {
        const result = await readSkills();
        loaded = true;
        publish({ skills: result.skills, loading: false });
      } catch {
        publish({ loading: false, error: 'Could not load skills.' });
      } finally {
        loading = false;
      }
    };
    requestRef.current = () => void load();
    if (readSkills) void load();
    else publish({ skills: [], loading: false });
    return () => {
      active = false;
      requestRef.current = null;
    };
  }, [transport, agentId]);

  const current = state?.transport === transport && state.agentId === agentId ? state : undefined;
  return {
    skills: current?.skills,
    skillsLoading: current?.loading ?? Boolean(transport.readSkills),
    skillsError: current?.error,
    requestSkills,
  };
}
