// Result dialog for the Export Transcript agent action. The action itself
// lives in session-chat-host.tsx (it runs from both the chat surface and the
// terminal overlay); it reports "exporting" / "exported" / "failed" on one
// window event and this host — mounted once in the app shell next to the other
// web modal hosts — renders the outcome.
//
// The exported markdown sits on the DAEMON's filesystem, so the browser can
// only offer the path itself (copy) or hand it to a new agent session on that
// same machine; there is no Reveal in Finder on web.

import { useCallback, useEffect, useState, type MouseEvent } from 'react';
import { rpcForMachine } from '../connections/connection-registry';
import type { GhostexWebFocusSessionDetail } from '../sidebar-runtime/sidebar-runtime';
import type { ExportTranscriptStatusDetail } from './action-events';

export function publishExportTranscriptStatus(detail: ExportTranscriptStatusDetail): void {
  window.dispatchEvent(new CustomEvent('ghostex-web:exportTranscriptStatus', { detail }));
}

/*
CDXC:GxserverFirstUserInputDraft 2026-08-20:
Plan 015 §7: the follow-up conversation is never given a prompt on the user's
behalf. gxserver types this draft into the new session's composer once and never
submits it, so all we stage is a mention of the exported markdown. The trailing
space is load-bearing — it separates the mention from the prompt the user types
after it — so the value must reach gxserver verbatim, untrimmed.
*/
function transcriptMentionDraft(path: string): string {
  return `@${path} `;
}

export function ExportTranscriptModalHost() {
  const [detail, setDetail] = useState<ExportTranscriptStatusDetail>();
  const [copied, setCopied] = useState(false);
  const [starting, setStarting] = useState(false);
  const [actionError, setActionError] = useState<string>();

  const close = useCallback(() => setDetail(undefined), []);

  useEffect(() => {
    const onStatus = (event: WindowEventMap['ghostex-web:exportTranscriptStatus']) => {
      setCopied(false);
      setStarting(false);
      setActionError(undefined);
      setDetail(event.detail);
    };
    window.addEventListener('ghostex-web:exportTranscriptStatus', onStatus);
    window.addEventListener('ghostex-web:closeAppModal', close);
    return () => {
      window.removeEventListener('ghostex-web:exportTranscriptStatus', onStatus);
      window.removeEventListener('ghostex-web:closeAppModal', close);
    };
  }, [close]);

  useEffect(() => {
    if (!detail) {
      return;
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        close();
      }
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [close, detail]);

  if (!detail) {
    return null;
  }

  const copyPath = (path: string) => {
    void navigator.clipboard.writeText(path).then(
      () => {
        setActionError(undefined);
        setCopied(true);
      },
      (error: unknown) => {
        setCopied(false);
        setActionError(error instanceof Error ? error.message : String(error));
      }
    );
  };

  const startNewConversation = async (path: string, agentId: string) => {
    setActionError(undefined);
    setStarting(true);
    try {
      const { session } = await rpcForMachine<{
        session?: { projectId?: string; sessionId?: string };
      }>(detail.machineId, '/api/createAgentSession', {
        agentId,
        projectId: detail.projectId,
        requireLaunchCommand: true,
        runtimeSettings: { firstUserInputDraft: transcriptMentionDraft(path) },
        surface: 'workspace',
        title: `${agentId} Session`,
      });
      const sessionId = session?.sessionId;
      if (!sessionId) {
        throw new Error('gxserver created the session without reporting its id.');
      }
      const focusDetail: GhostexWebFocusSessionDetail = {
        machineId: detail.machineId,
        placement: 'focusedPane',
        placementTargetSessionId: detail.sessionId,
        projectId: session?.projectId ?? detail.projectId,
        sessionId,
        source: 'sidebar',
      };
      window.dispatchEvent(new CustomEvent('ghostex-web:focusSession', { detail: focusDetail }));
      close();
    } catch (error: unknown) {
      setStarting(false);
      setActionError(error instanceof Error ? error.message : String(error));
    }
  };

  const stopPropagation = (event: MouseEvent) => event.stopPropagation();

  return (
    <div className='export-transcript-backdrop' onMouseDown={close} role='presentation'>
      <section
        aria-labelledby='export-transcript-title'
        aria-modal='true'
        className='export-transcript-modal'
        onMouseDown={stopPropagation}
        role='dialog'
      >
        <header className='export-transcript-modal__header'>
          <div>
            <h2 id='export-transcript-title'>Export Transcript</h2>
            <p>{detail.sessionTitle}</p>
          </div>
          <button
            aria-label='Close export transcript'
            className='export-transcript-modal__close'
            onClick={close}
            type='button'
          >
            ×
          </button>
        </header>

        <div className='export-transcript-modal__body'>
          {detail.status === 'exporting' && (
            <p className='export-transcript-modal__status' role='status'>
              Exporting the conversation to markdown…
            </p>
          )}

          {detail.status === 'failed' && (
            <p className='export-transcript-modal__error' role='alert'>
              {detail.message}
            </p>
          )}

          {detail.status === 'exported' && (
            <>
              <p className='export-transcript-modal__status'>
                Saved on the machine running this session, not in this browser.
              </p>
              <code className='export-transcript-modal__path'>{detail.result.path}</code>
              <p className='export-transcript-modal__meta'>
                {[
                  ...(detail.result.agent ? [detail.result.agent] : []),
                  `${detail.result.renderedEntries} of ${detail.result.parsedEntries} entries`,
                  `${detail.result.bytes.toLocaleString()} bytes`,
                ].join(' · ')}
              </p>
              {actionError && (
                <p className='export-transcript-modal__error' role='alert'>
                  {actionError}
                </p>
              )}
              {detail.agentId ? (
                <p className='export-transcript-modal__meta'>
                  Starting a new conversation waits with a mention of this file in its input. Nothing is sent until you
                  write your prompt and send it.
                </p>
              ) : (
                <p className='export-transcript-modal__meta'>
                  This session reports no agent, so a follow-up conversation cannot be started.
                </p>
              )}
              <div className='export-transcript-modal__actions'>
                <button
                  className='export-transcript-modal__button'
                  onClick={() => copyPath(detail.result.path)}
                  type='button'
                >
                  {copied ? 'Copied' : 'Copy path'}
                </button>
                <button
                  className='export-transcript-modal__button export-transcript-modal__button--primary'
                  disabled={starting || !detail.agentId}
                  onClick={() => {
                    if (detail.agentId) {
                      void startNewConversation(detail.result.path, detail.agentId);
                    }
                  }}
                  type='button'
                >
                  {starting ? 'Starting…' : 'Start new conversation'}
                </button>
              </div>
            </>
          )}
        </div>
      </section>
    </div>
  );
}
