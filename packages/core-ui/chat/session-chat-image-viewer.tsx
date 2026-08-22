// Session chat images. Pictures shared in the conversation render inline as
// thumbnails (SessionChatInlineImage) and click through to a centered overlay
// at full size (max 75% of the window height, original aspect ratio). Clicking
// the overlay picture steps it through three zoom levels and back to the
// fitted size, panning by scroll while zoomed; the zoom-in/zoom-out cursor is
// the affordance for it, and an image with no detail beyond its fitted size
// never offers the toggle. Right-clicking it offers Copy image (PNG, to the
// system clipboard) and Save image (the host's native save panel where there
// is one, a browser download otherwise).
// Machine paths load through the transport's readSessionChatImage RPC — the
// paths inside "[Image #N](path)" references live on the session's machine, so
// the page cannot open them directly. http(s)/data URLs render as-is.

import { IconLoader2, IconPhotoX, IconX } from "@tabler/icons-react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";
import { cn } from "@/packages/components/utils";

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

/**
 * Three steps between the fitted size and 1:1, spaced geometrically so the
 * first click is always a modest zoom no matter how much bigger the original
 * is: a 3x-larger original steps by ~1.44x, a 27x-larger one by 3x.
 */
const ZOOM_LEVEL_COUNT = 3;

/** Rendered widths for each zoom level, fitted width first, 1:1 last. */
function zoomWidthsForImage(fitWidth: number, naturalWidth: number): number[] {
  if (!(fitWidth > 0) || !(naturalWidth > fitWidth + 1)) {
    return [];
  }
  const step = Math.pow(naturalWidth / fitWidth, 1 / ZOOM_LEVEL_COUNT);
  const widths: number[] = [];
  for (let level = 1; level < ZOOM_LEVEL_COUNT; level += 1) {
    widths.push(fitWidth * Math.pow(step, level));
  }
  widths.push(naturalWidth);
  return widths;
}

/** File name to suggest when the picture is saved out of the overlay. */
export function sessionChatImageFileName(target: SessionChatImageTarget): string {
  const source = target.path ?? target.url ?? "";
  if (/^data:/i.test(source)) {
    const subtype = /^data:image\/([a-z0-9.+-]+)/i.exec(source)?.[1];
    return `image.${subtype === undefined || subtype === "jpeg" ? "png" : subtype}`;
  }
  const bare = source.split(/[?#]/, 1)[0] ?? source;
  let base = bare.split("/").pop() ?? "";
  try {
    base = decodeURIComponent(base);
  } catch {
    // Malformed escapes: keep the raw segment.
  }
  return base === "" ? "image.png" : base;
}

/** Re-encodes the decoded picture as PNG — the only format clipboards take. */
async function imageAsPngBlob(image: HTMLImageElement): Promise<Blob> {
  const canvas = document.createElement("canvas");
  canvas.width = image.naturalWidth;
  canvas.height = image.naturalHeight;
  const context = canvas.getContext("2d");
  if (context === null) {
    throw new Error("The image could not be rendered for copying.");
  }
  context.drawImage(image, 0, 0);
  return await new Promise<Blob>((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (blob === null) {
        reject(new Error("The image could not be encoded for copying."));
        return;
      }
      resolve(blob);
    }, "image/png");
  });
}

/** Original bytes behind a rendered source, base64, for handing to the host. */
async function base64FromSource(src: string): Promise<string> {
  if (src.startsWith("data:")) {
    const comma = src.indexOf(",");
    const payload = src.slice(comma + 1);
    if (/;base64/i.test(src.slice(0, comma))) {
      return payload;
    }
    return await base64FromBlob(new Blob([decodeURIComponent(payload)]));
  }
  return await base64FromBlob(await (await fetch(src)).blob());
}

function base64FromBlob(blob: Blob): Promise<string> {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("The image bytes could not be read."));
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      // data:<mime>;base64,<payload>
      resolve(result.slice(result.indexOf(",") + 1));
    };
    reader.readAsDataURL(blob);
  });
}

type ViewerState =
  | { status: "closed" }
  | { status: "loading"; alt?: string }
  | { status: "ready"; src: string; alt?: string; name: string }
  | { status: "error"; alt?: string };

export function SessionChatImageViewerProvider({
  children,
  loadImage,
  saveImageAs,
}: {
  children: ReactNode;
  /** Resolves a machine path to a data URL; omit when the host cannot. */
  loadImage?: (path: string) => Promise<string>;
  /**
   * Writes the picture wherever the user chooses, through the host's own save
   * panel (gpui). Hosts without one omit it and the overlay saves with a
   * browser download instead.
   */
  saveImageAs?: (params: { base64Data: string; suggestedName: string }) => Promise<void>;
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
  const saveImageAsRef = useRef(saveImageAs);
  saveImageAsRef.current = saveImageAs;
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const imageRef = useRef<HTMLImageElement | null>(null);
  // Where in the picture the zoom click landed, as 0..1 fractions, so the
  // detail that was clicked ends up under the pointer instead of the overlay
  // jumping to the top-left corner.
  const zoomFocusRef = useRef<{ x: number; y: number } | null>(null);
  // 0 is the fitted size; 1..zoomWidths.length are the zoom steps.
  const [zoomLevel, setZoomLevel] = useState(0);
  const [fitWidth, setFitWidth] = useState(0);
  const [naturalWidth, setNaturalWidth] = useState(0);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  const [menuError, setMenuError] = useState<string | null>(null);

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
        const name = sessionChatImageFileName(target);
        openSequenceRef.current += 1;
        const sequence = openSequenceRef.current;
        setState({ status: "loading", ...(alt !== undefined ? { alt } : {}) });
        source
          .then((src) => {
            if (openSequenceRef.current === sequence) {
              setState({ name, src, status: "ready", ...(alt !== undefined ? { alt } : {}) });
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

  const source = state.status === "ready" ? state.src : null;
  const zoomWidths = useMemo(
    () => zoomWidthsForImage(fitWidth, naturalWidth),
    [fitWidth, naturalWidth],
  );
  const zoomWidth = zoomLevel > 0 ? zoomWidths[zoomLevel - 1] : undefined;

  // Every open (and every close) starts fitted, unzoomed and without a menu.
  useEffect(() => {
    zoomFocusRef.current = null;
    setZoomLevel(0);
    setFitWidth(0);
    setNaturalWidth(0);
    setMenuAt(null);
    setMenuError(null);
  }, [source]);

  /*
  Zoom is only worth offering when 1:1 would actually show more than the fitted
  box already does — a small picture is already at full size, so it keeps the
  plain cursor and ignores clicks rather than pretending to zoom. Measured from
  the fitted render, so it is re-read whenever the window resizes.
  */
  const measureFit = useCallback((): void => {
    const image = imageRef.current;
    if (image === null) {
      return;
    }
    setFitWidth(image.clientWidth);
    setNaturalWidth(image.naturalWidth);
  }, []);

  useEffect(() => {
    if (source === null || zoomLevel > 0) {
      return;
    }
    window.addEventListener("resize", measureFit);
    return () => {
      window.removeEventListener("resize", measureFit);
    };
  }, [measureFit, source, zoomLevel]);

  // A picture already decoded when the overlay mounts (the thumbnail read it
  // first, and both share one source) never fires `load`, so measure it here.
  useLayoutEffect(() => {
    const image = imageRef.current;
    if (image !== null && image.complete && image.naturalWidth > 0) {
      measureFit();
    }
  }, [measureFit, source]);

  const stepZoom = (event: ReactMouseEvent<HTMLImageElement>): void => {
    // Clicking the picture itself zooms it; only the surround dismisses.
    event.stopPropagation();
    if (menuAt !== null) {
      setMenuAt(null);
      return;
    }
    if (zoomWidths.length === 0) {
      return;
    }
    // Past the last step the next click returns to the fitted size.
    const next = zoomLevel >= zoomWidths.length ? 0 : zoomLevel + 1;
    if (next === 0) {
      zoomFocusRef.current = null;
      setZoomLevel(0);
      return;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    zoomFocusRef.current = {
      x: (event.clientX - rect.left) / rect.width,
      y: (event.clientY - rect.top) / rect.height,
    };
    setZoomLevel(next);
  };

  // Scroll the freshly enlarged picture to the point that was clicked, before
  // the browser paints the new size.
  useLayoutEffect(() => {
    const focus = zoomFocusRef.current;
    zoomFocusRef.current = null;
    const scroll = scrollRef.current;
    const image = imageRef.current;
    if (zoomLevel === 0 || focus === null || scroll === null || image === null) {
      return;
    }
    const imageRect = image.getBoundingClientRect();
    const scrollRect = scroll.getBoundingClientRect();
    scroll.scrollLeft +=
      imageRect.left - scrollRect.left + focus.x * imageRect.width - scroll.clientWidth / 2;
    scroll.scrollTop +=
      imageRect.top - scrollRect.top + focus.y * imageRect.height - scroll.clientHeight / 2;
  }, [zoomLevel]);

  // Nudge a menu opened near the right or bottom edge back inside the window.
  // One correcting pass: after the shift there is no overflow left to react to.
  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (menuAt === null || menu === null) {
      return;
    }
    const rect = menu.getBoundingClientRect();
    const overflowX = Math.max(0, rect.right - window.innerWidth + 8);
    const overflowY = Math.max(0, rect.bottom - window.innerHeight + 8);
    if (overflowX === 0 && overflowY === 0) {
      return;
    }
    setMenuAt({ x: menuAt.x - overflowX, y: menuAt.y - overflowY });
  }, [menuAt]);

  useEffect(() => {
    if (menuError === null) {
      return;
    }
    const timer = window.setTimeout(() => setMenuError(null), 4000);
    return () => {
      window.clearTimeout(timer);
    };
  }, [menuError]);

  const copyImage = (): void => {
    const image = imageRef.current;
    if (image === null) {
      return;
    }
    setMenuAt(null);
    // The blob is handed over as a promise so the write stays inside the click
    // gesture that opened the menu; re-encoding first would lose it.
    void navigator.clipboard
      .write([new ClipboardItem({ "image/png": imageAsPngBlob(image) })])
      .catch((error: unknown) => {
        console.error("[session-chat] Copying the image failed.", error);
        setMenuError("The image could not be copied.");
      });
  };

  const saveImage = (): void => {
    if (state.status !== "ready") {
      return;
    }
    const { name, src } = state;
    setMenuAt(null);
    const hostSave = saveImageAsRef.current;
    if (hostSave === undefined) {
      // Browser hosts write the original bytes straight to the download folder.
      const anchor = document.createElement("a");
      anchor.download = name;
      anchor.href = src;
      anchor.rel = "noopener";
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      return;
    }
    // The original bytes, not a re-encode: the suggested name carries the
    // original extension, and a saved copy should match the file it came from.
    void base64FromSource(src)
      .then((base64Data) => hostSave({ base64Data, suggestedName: name }))
      .catch((error: unknown) => {
        console.error("[session-chat] Saving the image failed.", error);
        setMenuError("The image could not be saved.");
      });
  };

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
        if (menuAt !== null) {
          setMenuAt(null);
          return;
        }
        close();
      }
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [close, menuAt, open]);

  return (
    <SessionChatImageViewerContext.Provider value={api}>
      {children}
      {open ? (
        <div
          aria-label={state.alt ?? "Image preview"}
          aria-modal="true"
          className="fixed inset-0 z-50 bg-black/70 backdrop-blur-[2px]"
          onClick={() => {
            // An open menu is what a stray click is aiming to dismiss; only a
            // click with no menu up means "close the picture".
            if (menuAt !== null) {
              setMenuAt(null);
              return;
            }
            close();
          }}
          role="dialog"
        >
          {/* Outside the scrolling layer so it stays put while a zoomed
              picture is panned around under it. */}
          <button
            aria-label="Close image preview"
            className="absolute right-3 top-3 z-10 flex size-8 items-center justify-center rounded-full bg-black/50 text-white/80 transition-colors hover:text-white"
            onClick={close}
            type="button"
          >
            <IconX aria-hidden="true" size={18} stroke={2} />
          </button>
          <div
            className="absolute inset-0 overflow-auto"
            onScroll={() => {
              // A menu anchored to page coordinates would drift away from the
              // pixel it was opened on once the picture is panned.
              if (menuAt !== null) {
                setMenuAt(null);
              }
            }}
            ref={scrollRef}
          >
            <div className="ghostex-chat-image-preview-stage">
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
                  className="ghostex-chat-image-preview rounded-lg shadow-2xl"
                  data-zoom={
                    zoomWidths.length === 0
                      ? "none"
                      : zoomLevel >= zoomWidths.length
                        ? "out"
                        : "in"
                  }
                  data-zoomed={zoomLevel > 0 ? "true" : undefined}
                  // Native image dragging would fight scroll-to-pan.
                  draggable={false}
                  onClick={stepZoom}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    setMenuError(null);
                    setMenuAt({ x: event.clientX, y: event.clientY });
                  }}
                  onLoad={measureFit}
                  ref={imageRef}
                  src={state.src}
                  {...(zoomWidth === undefined ? {} : { style: { width: zoomWidth } })}
                />
              ) : null}
            </div>
          </div>
          {menuAt !== null ? (
            <div
              className="ghostex-chat-image-menu"
              onClick={(event) => {
                event.stopPropagation();
              }}
              onContextMenu={(event) => {
                event.preventDefault();
              }}
              ref={menuRef}
              role="menu"
              style={{ left: menuAt.x, top: menuAt.y }}
            >
              <button
                className="ghostex-chat-image-menu-item"
                onClick={copyImage}
                role="menuitem"
                type="button"
              >
                Copy image
              </button>
              <button
                className="ghostex-chat-image-menu-item"
                onClick={saveImage}
                role="menuitem"
                type="button"
              >
                Save image
              </button>
            </div>
          ) : null}
          {menuError !== null ? (
            <div className="ghostex-chat-image-menu-error" role="status">
              {menuError}
            </div>
          ) : null}
        </div>
      ) : null}
    </SessionChatImageViewerContext.Provider>
  );
}
