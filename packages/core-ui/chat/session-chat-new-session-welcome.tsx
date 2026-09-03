/*
The new-session welcome: the brand logo and headline a session shows while it
has no transcript yet. Moved out of session-chat-view.tsx verbatim (together
with `displayAgentName`, which the view still imports from here) so the agent
crossfade below has a home of its own.

The welcome fills the transcript region and nothing else. It used to be an
`absolute inset-0` overlay spanning the whole chat column, which painted its
centered logo and title straight through the terminal-notice / interactive
cards stacked above the composer. Living in flow means the cards take their
height first and the welcome centers in whatever is left; `showTitle` drops the
headline once a card is up, so the remaining space belongs to the logo alone.
*/

import { IconRobot } from '@tabler/icons-react';
import { useEffect, useRef, useState } from 'react';
import { cn } from '@/packages/components/utils';
import { getDefaultSidebarAgentById, isSidebarAgentIcon, type SidebarAgentIcon } from '../../shared/sidebar-agents';
import { sessionChatAgentIconId } from '../../shared/session-chat';
import { getBrandAgentLogoStyle } from '../agent-logos';

export function displayAgentName(agentLabel?: string | null): string | null {
  const normalized = agentLabel?.trim();
  if (!normalized) {
    return null;
  }
  return (
    getDefaultSidebarAgentById(normalized)?.name ??
    normalized.replace(/[-_]+/g, ' ').replace(/\b\p{L}/gu, (letter) => letter.toLocaleUpperCase())
  );
}

/*
CDXC:Drafts 2026-08-28:
One rendered agent identity. Switching a draft's agent CLI replaces the logo
and the headline under the user, so the outgoing identity stays mounted for one
fade while the incoming one fades in. At most two layers ever exist: the
incoming one and the single layer it is replacing.
*/
interface WelcomeAgentLayer {
  agentName: string | null;
  icon: SidebarAgentIcon | undefined;
  /** React key and identity for the animation-end drop. `0` is the mount layer. */
  id: number;
  /** What "the same agent" means for the crossfade: artwork plus name. */
  identity: string;
}

export function NewSessionWelcome({
  agentIcon,
  agentLabel,
  agentName: agentNameOverride,
  showTitle = true,
}: {
  /*
  CDXC:Drafts 2026-08-28:
  A draft's own agent row, when the read state has one. A project custom agent
  has no entry in the default agent table, so its name and brand artwork can
  only come from the daemon's list — without them a custom agent's draft would
  greet the user as its base family, or as no agent at all.
  */
  agentIcon?: string;
  agentLabel?: string | null;
  agentName?: string;
  showTitle?: boolean;
}) {
  const defaultAgent = agentLabel ? getDefaultSidebarAgentById(agentLabel) : undefined;
  // A read-state label is the transcript family id, which is not always the
  // sidebar agent id the artwork is registered under.
  const familyIconId = sessionChatAgentIconId(agentLabel) ?? undefined;
  const familyIcon = isSidebarAgentIcon(familyIconId) ? familyIconId : undefined;
  const icon = (isSidebarAgentIcon(agentIcon) ? agentIcon : undefined) ?? defaultAgent?.icon ?? familyIcon;
  const agentName = agentNameOverride ?? displayAgentName(agentLabel);
  const identity = `${icon ?? ''}|${agentName ?? ''}`;
  const [layers, setLayers] = useState<WelcomeAgentLayer[]>(() => [{ agentName, icon, id: 0, identity }]);
  /*
  The mount layer is id 0 and deliberately does NOT animate: opening a session
  is not an agent change, and a fade there would be a second entrance behind
  the pane's own. Every later layer gets a fresh id, which is also what makes
  React remount it and restart the CSS animation.
  */
  const nextLayerIdRef = useRef(1);
  const topIdentityRef = useRef(identity);
  useEffect(() => {
    if (topIdentityRef.current === identity) {
      return;
    }
    topIdentityRef.current = identity;
    const id = nextLayerIdRef.current;
    nextLayerIdRef.current = id + 1;
    // slice(-1): a switch that lands mid-fade replaces the outgoing layer
    // rather than stacking a third one.
    setLayers((current) => [...current.slice(-1), { agentName, icon, id, identity }]);
  }, [agentName, icon, identity]);

  const dropOutgoingLayer = (id: number): void => {
    setLayers((current) => (current.length > 1 && current[0]?.id === id ? current.slice(1) : current));
  };

  const layerClassName = (layer: WelcomeAgentLayer, leaving: boolean): string | undefined => {
    if (leaving) {
      // Every layer fades OUT, including the mount one — its animationend is
      // also what removes it.
      return 'ghostex-chat-new-session-leaving';
    }
    return layer.id === 0 ? undefined : 'ghostex-chat-new-session-entering';
  };

  return (
    <div className='ghostex-chat-new-session pointer-events-none min-h-0 flex-1 overflow-hidden'>
      <div aria-label={agentName ?? 'Agent'} className='ghostex-chat-new-session-agent' role='img'>
        {layers.map((layer, index) => {
          const leaving = index < layers.length - 1;
          return (
            <span
              aria-hidden='true'
              className={cn('ghostex-chat-new-session-agent-slot', layerClassName(layer, leaving))}
              key={layer.id}
              // The logo layer owns the cleanup for the whole crossfade: the
              // headline can be hidden (showTitle) while this one never is.
              onAnimationEnd={leaving ? () => dropOutgoingLayer(layer.id) : undefined}
            >
              {layer.icon ? (
                <span className='ghostex-chat-new-session-agent-logo' style={getBrandAgentLogoStyle(layer.icon)} />
              ) : (
                <IconRobot size={28} stroke={1.7} />
              )}
            </span>
          );
        })}
      </div>
      {showTitle ? (
        <div className='ghostex-chat-new-session-title'>
          {layers.map((layer, index) => (
            <span className={layerClassName(layer, index < layers.length - 1)} key={layer.id}>
              {layer.agentName ? <>What should we build with {layer.agentName}?</> : 'What should we work on?'}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  );
}
