// Session chat images. Pictures shared in the conversation render inline as
// thumbnails (SessionChatInlineImage) and click through to a centered overlay
// at full size (max 75% of the window height, original aspect ratio). Machine
// paths load through the transport's readSessionChatImage RPC — the paths
// inside "[Image #N](path)" references live on the session's machine, so the
// page cannot open them directly. http(s)/data URLs render as-is.

import { IconLoader2, IconPhotoX, IconX } from "@tabler/icons-react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { cn } from "../../lib/utils";

export interface SessionChatImageTarget {
  /** Absolute path on the session's machine (loaded over the transport). */
  path?: string;
  /** Directly renderable URL (http(s)/data). */
  url?: string;
  alt?: string;
}

export interface SessionChatImageViewerApi {
  /** True when the viewer can display this target at all. */
  canOpen: (target: SessionChatImageTarget) => boolean;
  open: (target: SessionChatImageTarget) => void;
  /**
   * Renderable source for a target, deduplicated per path/url so the inline
   * thumbnail and the overlay of the same image share one read. Undefined
   * when the target cannot be shown at all.
   */
  resolve: (target: SessionChatImageTarget) => Promise<string> | undefined;
}

const SessionChatImageViewerContext = createContext<SessionChatImageViewerApi | null>(
  null,
);

export function useSessionChatImageViewer(): SessionChatImageViewerApi | null {
  return useContext(SessionChatImageViewerContext);
}

const IMAGE_HREF_PATTERN = /\.(avif|bmp|gif|heic|heif|ico|jpe?g|png|svg|tiff?|webp)$/i;

/** True when a markdown href points at an image (query/hash tolerated). */
export function isSessionChatImageHref(href: string): boolean {
  const bare = href.split(/[?#]/, 1)[0] ?? href;
  return IMAGE_HREF_PATTERN.test(bare);
}

/** Classifies a markdown image href into a viewer target. */
export function sessionChatImageTargetForHref(href: string): SessionChatImageTarget {
  if (/^(https?:|data:)/i.test(href)) {
    return { url: href };
  }
  // Markdown link destinations arrive percent-encoded; machine paths need
  // the literal characters back.
  let path = href;
  try {
    path = decodeURI(href);
  } catch {
    // Malformed escapes: use the raw href.
  }
  return { path };
}

/**
 * Inline thumbnail for a picture shared in the conversation. Reading the bytes
 * is deferred until the row is near the viewport — on the phone every machine
 * path is a base64 round trip over SSH, so a long transcript must not fetch
 * every image it holds — and a target that cannot be read renders `fallback`
 * (the attachment chip or link the image would otherwise have been) instead of
 * a broken image.
 */
export function SessionChatInlineImage({
  className,
  fallback,
  target,
}: {
  className?: string;
  fallback?: ReactNode;
  target: SessionChatImageTarget;
}) {
  const viewer = useSessionChatImageViewer();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [nearViewport, setNearViewport] = useState(false);
  const [source, setSource] = useState<
    { status: "loading" } | { status: "ready"; src: string } | { status: "error" }
  >({ status: "loading" });
  const targetKey = target.url ?? target.path ?? "";

  useEffect(() => {
    const node = containerRef.current;
    if (node === null || nearViewport) {
      return;
    }
    if (typeof IntersectionObserver === "undefined") {
      setNearViewport(true);
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setNearViewport(true);
        }
      },
      // A screen of lead time, so scrolling meets loaded pictures.
      { rootMargin: "600px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [nearViewport]);

  useEffect(() => {
    if (!nearViewport || !viewer) {
      return;
    }
    const pending = viewer.resolve(target);
    if (pending === undefined) {
      setSource({ status: "error" });
      return;
    }
    let cancelled = false;
    setSource({ status: "loading" });
    pending
      .then((src) => {
        if (!cancelled) {
          setSource({ src, status: "ready" });
        }
      })
      .catch(() => {
        if (!cancelled) {
          setSource({ status: "error" });
        }
      });
    return () => {
      cancelled = true;
    };
    // The target object is rebuilt per render; its path/url identifies it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nearViewport, targetKey, viewer]);

  if (source.status === "error" || viewer === null) {
    return <>{fallback ?? null}</>;
  }
  return (
    <div className={cn("ghostex-chat-inline-image-frame", className)} ref={containerRef}>
      {source.status === "ready" ? (
        <button
          aria-label={target.alt ? `View ${target.alt}` : "View image"}
          className="ghostex-chat-inline-image-button"
          onClick={() => viewer.open(target)}
          type="button"
        >
          <img alt={target.alt ?? ""} className="ghostex-chat-inline-image" src={source.src} />
        </button>
      ) : (
        <span aria-label="Loading image" className="ghostex-chat-inline-image-pending" role="img">
          <IconLoader2 aria-hidden="true" className="size-4 animate-spin" stroke={2} />
        </span>
      )}
    </div>
  );
}

type ViewerState =
  | { status: "closed" }
  | { status: "loading"; alt?: string }
  | { status: "ready"; src: string; alt?: string }
  | { status: "error"; alt?: string };

export function SessionChatImageViewerProvider({
  children,
  loadImage,
}: {
  children: ReactNode;
  /** Resolves a machine path to a data URL; omit when the host cannot. */
  loadImage?: (path: string) => Promise<string>;
}) {
  const [state, setState] = useState<ViewerState>({ status: "closed" });
  // Distinguishes stale loads from the current one after rapid re-opens.
  const openSequenceRef = useRef(0);
  const loadImageRef = useRef(loadImage);
  loadImageRef.current = loadImage;
  /*
  One read per machine path for the whole conversation: the same image can be
  an inline thumbnail, a re-render of it, and the overlay, and on the phone
  every read is a base64 round trip over SSH. Failed reads are evicted so a
  connection hiccup does not make an image permanently unviewable.
  */
  const sourcesRef = useRef(new Map<string, Promise<string>>());

  const close = useCallback((): void => {
    openSequenceRef.current += 1;
    setState({ status: "closed" });
  }, []);

  const api = useMemo<SessionChatImageViewerApi>(() => {
    const resolve = (target: SessionChatImageTarget): Promise<string> | undefined => {
      if (target.url !== undefined) {
        return Promise.resolve(target.url);
      }
      const path = target.path;
      const load = loadImageRef.current;
      if (path === undefined || load === undefined) {
        return undefined;
      }
      const cached = sourcesRef.current.get(path);
      if (cached) {
        return cached;
      }
      const pending = load(path);
      sourcesRef.current.set(path, pending);
      void pending.catch(() => {
        if (sourcesRef.current.get(path) === pending) {
          sourcesRef.current.delete(path);
        }
      });
      return pending;
    };
    return {
      canOpen: (target) =>
        target.url !== undefined ||
        (target.path !== undefined && loadImageRef.current !== undefined),
      open: (target) => {
        const alt = target.alt;
        const source = resolve(target);
        if (source === undefined) {
          return;
        }
        openSequenceRef.current += 1;
        const sequence = openSequenceRef.current;
        setState({ status: "loading", ...(alt !== undefined ? { alt } : {}) });
        source
          .then((src) => {
            if (openSequenceRef.current === sequence) {
              setState({ src, status: "ready", ...(alt !== undefined ? { alt } : {}) });
            }
          })
          .catch(() => {
            if (openSequenceRef.current === sequence) {
              setState({ status: "error", ...(alt !== undefined ? { alt } : {}) });
            }
          });
      },
      resolve,
    };
  }, []);

  // Escape closes the overlay before the composer's interrupt shortcut can
  // see the key (window capture, only while open).
  const open = state.status !== "closed";
  useEffect(() => {
    if (!open) {
      return;
    }
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        close();
      }
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [close, open]);

  return (
    <SessionChatImageViewerContext.Provider value={api}>
      {children}
      {open ? (
        <div
          aria-label={state.alt ?? "Image preview"}
          aria-modal="true"
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-[2px]"
          onClick={close}
          role="dialog"
        >
          <button
            aria-label="Close image preview"
            className="absolute right-3 top-3 flex size-8 items-center justify-center rounded-full bg-black/50 text-white/80 transition-colors hover:text-white"
            onClick={close}
            type="button"
          >
            <IconX aria-hidden="true" size={18} stroke={2} />
          </button>
          {state.status === "loading" ? (
            <IconLoader2
              aria-label="Loading image"
              className="size-7 animate-spin text-white/80"
              stroke={2}
            />
          ) : null}
          {state.status === "error" ? (
            <div className="flex flex-col items-center gap-2 text-white/80">
              <IconPhotoX aria-hidden="true" className="size-7" stroke={1.8} />
              <span className="text-sm">The image could not be loaded.</span>
            </div>
          ) : null}
          {state.status === "ready" ? (
            <img
              alt={state.alt ?? "Image preview"}
              className="max-h-[75vh] max-w-[90vw] rounded-lg object-contain shadow-2xl"
              onClick={(event) => {
                // Clicking the picture itself should not dismiss it.
                event.stopPropagation();
              }}
              src={state.src}
            />
          ) : null}
        </div>
      ) : null}
    </SessionChatImageViewerContext.Provider>
  );
}
