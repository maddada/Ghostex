import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { IconArrowsMaximize, IconCheck, IconCopy, IconFocusCentered, IconMinus, IconPlus } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { AppModalShell } from '../app-modal-shell';
import { DialogTitle } from '@/packages/components/ui/dialog';
import { openAppModal } from '../app-modal-host-bridge';
import { mermaidImageUrl, renderMermaid, type MermaidTheme } from './mermaid-runtime';
import './mermaid.css';

function useMermaidTheme(ref: RefObject<HTMLDivElement | null>) {
  const [theme, setTheme] = useState<MermaidTheme>();
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 1;
    const context = canvas.getContext('2d', { willReadFrequently: true });
    if (!context) return;
    const color = (css: string) => {
      context.clearRect(0, 0, 1, 1);
      context.fillStyle = css;
      context.fillRect(0, 0, 1, 1);
      return [...context.getImageData(0, 0, 1, 1).data].slice(0, 3);
    };
    const hex = (rgb: number[]) => `#${rgb.map((value) => value.toString(16).padStart(2, '0')).join('')}`;
    const update = () => {
      const styles = getComputedStyle(node);
      const background = color(styles.backgroundColor);
      const next = {
        background: hex(background),
        foreground: hex(color(styles.color)),
        dark: background[0] * 0.299 + background[1] * 0.587 + background[2] * 0.114 < 128,
      };
      setTheme((previous) => (JSON.stringify(previous) === JSON.stringify(next) ? previous : next));
    };
    update();
    const observer = new MutationObserver(update);
    for (let ancestor: HTMLElement | null = node.parentElement; ancestor; ancestor = ancestor.parentElement) {
      observer.observe(ancestor, {
        attributes: true,
        attributeFilter: ['class', 'style', 'data-theme', 'data-sidebar-theme'],
      });
    }
    return () => observer.disconnect();
  }, [ref]);
  return theme;
}

export interface MermaidDiagramProps {
  source: string;
  pending?: boolean;
  expanded?: boolean;
  onExpand?: (source: string) => void;
  onResize?: () => void;
  /** Optional owner-held preferences survive a virtualized transcript row unmount. */
  viewState?: { mode?: string; zoom?: number };
}

export function MermaidDiagram({
  source,
  pending = false,
  expanded = false,
  onExpand,
  onResize,
  viewState,
}: MermaidDiagramProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const theme = useMermaidTheme(rootRef);
  const [nearViewport, setNearViewport] = useState(false);
  const [mode, setMode] = useState(viewState?.mode === 'source' ? 'source' : 'diagram');
  const [zoom, setZoom] = useState(
    typeof viewState?.zoom === 'number' ? Math.max(0.5, Math.min(4, viewState.zoom)) : 1
  );
  const previousSource = useRef(source);
  useLayoutEffect(() => {
    if (viewState) {
      viewState.mode = mode;
      viewState.zoom = zoom;
    }
  }, [mode, viewState, zoom]);
  const [imageRatio, setImageRatio] = useState(1);
  const [frame, setFrame] = useState({ width: 0, height: 416 });
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState('');
  const [localExpanded, setLocalExpanded] = useState(false);
  const [result, setResult] = useState<{ key: string; url?: string; error?: string }>();
  const renderKey = JSON.stringify([source, theme]);
  const current = result?.key === renderKey ? result : undefined;
  const drag = useRef<{ id: number; x: number; y: number; left: number; top: number } | null>(null);

  useEffect(() => {
    const node = viewportRef.current;
    if (!node) return;
    const update = () => setFrame({ width: node.clientWidth - 24, height: expanded ? node.clientHeight - 24 : 416 });
    const observer = new ResizeObserver(update);
    observer.observe(node);
    update();
    return () => observer.disconnect();
  }, [expanded, mode]);

  useEffect(() => {
    const node = rootRef.current;
    if (!node || nearViewport) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) setNearViewport(true);
      },
      { rootMargin: '400px' }
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [nearViewport]);

  useEffect(() => {
    const node = rootRef.current;
    if (!node || !onResize) return;
    const observer = new ResizeObserver(onResize);
    observer.observe(node);
    return () => observer.disconnect();
  }, [onResize]);

  useEffect(() => {
    if (!nearViewport || !theme || pending) return;
    let cancelled = false;
    let url: string | undefined;
    void renderMermaid(source, theme)
      .then((svg) => {
        if (cancelled) return;
        url = mermaidImageUrl(svg);
        setResult({ key: renderKey, url });
      })
      .catch((error: unknown) => {
        if (!cancelled) setResult({ key: renderKey, error: error instanceof Error ? error.message : String(error) });
      });
    return () => {
      cancelled = true;
      if (url) URL.revokeObjectURL(url);
    };
  }, [nearViewport, pending, renderKey, source, theme]);

  useEffect(() => {
    if (previousSource.current === source) return;
    previousSource.current = source;
    setZoom(1);
    viewportRef.current?.scrollTo(0, 0);
  }, [source]);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1200);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const fit = () => {
    setZoom(1);
    viewportRef.current?.scrollTo(0, 0);
  };
  const expand = () => {
    if (onExpand) onExpand(source);
    else if ('ghostexGpui' in window) {
      openAppModal({ modal: 'mermaidDiagram', source, type: 'open' });
    } else setLocalExpanded(true);
  };

  return (
    <div
      className='ghostex-mermaid'
      data-expanded={expanded}
      data-session-chat-retain={localExpanded || undefined}
      ref={rootRef}
    >
      <div className='ghostex-mermaid-toolbar' aria-label='Diagram controls' role='toolbar'>
        <SegmentedControl aria-label='Diagram display' size='sm' value={mode} onValueChange={setMode}>
          <SegmentedControlItem value='diagram'>Diagram</SegmentedControlItem>
          <SegmentedControlItem value='source'>Source</SegmentedControlItem>
        </SegmentedControl>
        <div className='ghostex-mermaid-actions'>
          {mode === 'diagram' && (
            <>
              <Button
                size='icon-xs'
                variant='ghost'
                title='Zoom out'
                aria-label='Zoom out'
                disabled={!current?.url || zoom <= 0.5}
                onClick={() => setZoom((value) => Math.max(0.5, value / 1.25))}
              >
                <IconMinus />
              </Button>
              <Button
                size='icon-xs'
                variant='ghost'
                title='Fit diagram'
                aria-label='Fit diagram'
                disabled={!current?.url}
                onClick={fit}
              >
                <IconFocusCentered />
              </Button>
              <Button
                size='icon-xs'
                variant='ghost'
                title='Zoom in'
                aria-label='Zoom in'
                disabled={!current?.url || zoom >= 4}
                onClick={() => setZoom((value) => Math.min(4, value * 1.25))}
              >
                <IconPlus />
              </Button>
            </>
          )}
          <Button
            size='icon-xs'
            variant='ghost'
            title={copied ? 'Copied' : 'Copy source'}
            aria-label={copied ? 'Copied' : 'Copy source'}
            onClick={() => {
              setCopyError('');
              void navigator.clipboard.writeText(source).then(
                () => setCopied(true),
                () => setCopyError('Could not copy the diagram source.')
              );
            }}
          >
            {copied ? <IconCheck /> : <IconCopy />}
          </Button>
          {!expanded && (
            <Button
              size='icon-xs'
              variant='ghost'
              title='Expand diagram'
              aria-label='Expand diagram'
              disabled={pending}
              onClick={expand}
            >
              <IconArrowsMaximize />
            </Button>
          )}
        </div>
      </div>
      {copyError && (
        <p className='ghostex-mermaid-status' role='alert'>
          {copyError}
        </p>
      )}
      {mode === 'source' ? (
        <pre className='ghostex-mermaid-source'>
          <code>{source}</code>
        </pre>
      ) : (
        <div
          className='ghostex-mermaid-viewport'
          ref={viewportRef}
          tabIndex={0}
          aria-label='Mermaid diagram. Use zoom controls or scroll to explore.'
          onPointerDown={(event) => {
            if (event.button !== 0 || event.pointerType === 'touch' || !current?.url) return;
            const node = event.currentTarget;
            drag.current = {
              id: event.pointerId,
              x: event.clientX,
              y: event.clientY,
              left: node.scrollLeft,
              top: node.scrollTop,
            };
            node.setPointerCapture(event.pointerId);
          }}
          onPointerMove={(event) => {
            const start = drag.current;
            if (!start || start.id !== event.pointerId) return;
            event.currentTarget.scrollLeft = start.left + start.x - event.clientX;
            event.currentTarget.scrollTop = start.top + start.y - event.clientY;
          }}
          onPointerUp={() => {
            drag.current = null;
          }}
          onLostPointerCapture={() => {
            drag.current = null;
          }}
        >
          {pending ? (
            <p className='ghostex-mermaid-status' role='status'>
              Composing diagram…
            </p>
          ) : current?.error ? (
            <div className='ghostex-mermaid-status' role='alert'>
              <strong>Could not render diagram</strong>
              <pre>{current.error}</pre>
              <p>Open Source to inspect the Mermaid text.</p>
            </div>
          ) : current?.url ? (
            <img
              className='ghostex-mermaid-image'
              src={current.url}
              alt='Mermaid diagram'
              draggable={false}
              style={{
                width: frame.width > 0 ? `${zoom * Math.min(frame.width, frame.height * imageRatio)}px` : '100%',
              }}
              onLoad={(event) => {
                setImageRatio(event.currentTarget.naturalWidth / event.currentTarget.naturalHeight);
                onResize?.();
              }}
              onError={() => setResult({ key: renderKey, error: 'The diagram image could not be displayed.' })}
            />
          ) : (
            <p className='ghostex-mermaid-status' role='status'>
              Rendering diagram…
            </p>
          )}
        </div>
      )}
      {localExpanded && <MermaidDiagramModal source={source} onClose={() => setLocalExpanded(false)} />}
    </div>
  );
}

export function MermaidDiagramModal({ source, onClose }: { source: string; onClose: () => void }) {
  return (
    <AppModalShell isOpen onClose={onClose} width={1248} showCloseButton className='ghostex-mermaid-modal'>
      <DialogTitle className='sr-only'>Mermaid diagram</DialogTitle>
      <MermaidDiagram source={source} expanded />
    </AppModalShell>
  );
}
