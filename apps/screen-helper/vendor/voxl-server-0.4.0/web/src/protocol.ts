// Wire protocol between the browser and the server (src/server.rs), and the messages
// the page and its worker exchange. One WebSocket carries everything: binary messages
// start with a type byte, text messages are JSON from the server and short
// comma-separated commands from the client.

/** Type byte of binary messages. */
export const Bin = {
	/** Server: H.264 stripe update (16-byte header, then Annex-B). */
	Video: 0x01,
	/** Server: Opus desktop audio. */
	Audio: 0x02,
	/** Server: cursor image (RGBA). */
	Cursor: 0x03,
	/** Server: clipboard image (PNG). */
	ClipImage: 0x04,
	/** Client: clipboard image (PNG). */
	ClipImageUp: 0x11,
	/** Client: microphone Opus packet. */
	MicOpus: 0x12,
	/** Client: microphone PCM (s16le mono 48 kHz). */
	MicPcm: 0x13,
} as const;

/** A horizontal band of the screen encoded as its own H.264 stream: [y, height]. */
export type Stripe = [y: number, h: number];

export interface FilesConfig {
	upload: boolean;
	download: boolean;
	/** Folder (relative to the shared root) the panel opens and uploads into. */
	dir: string;
}

export interface Hello {
	t: 'hello';
	w: number;
	h: number;
	/** The server's platform: 'linux' (X11) or 'macos'. */
	os?: string;
	audio: boolean;
	mic?: boolean;
	stripes?: Stripe[];
	files?: FilesConfig;
	/** The server only shows the desktop: the viewer cannot send input. */
	readOnly?: boolean;
	/** Dynamic resolution: desktop pixels per CSS pixel of the viewer's window. */
	resize?: { scale: number };
	/** macOS: one shared display, selected by its CoreGraphics ID. */
	displays?: { selected: number; available: { id: number; name: string }[] };
}

/** JSON text messages from the server. */
export type ServerMessage =
	| Hello
	| { t: 'ping'; s: number }
	| { t: 'clip'; d: string }
	| { t: 'mic'; on: boolean }
	| { t: 'close' };

/** Text commands to the server. */
export const cmd = {
	ack: (frame: number) => `a,${frame}`,
	move: (x: number, y: number) => `m,${x},${y}`,
	button: (button: number, down: boolean) => `b,${button},${down ? 1 : 0}`,
	/** Scroll in desktop pixels; the server turns it into smooth scrolling or wheel clicks. */
	scroll: (dx: number, dy: number) => `sp,${dx.toFixed(1)},${dy.toFixed(1)}`,
	key: (keysym: number, down: boolean) => `k,${keysym},${down ? 1 : 0}`,
	releaseAll: 'r',
	clipboard: (text: string) => `c,${text}`,
	video: (on: boolean) => `v,${on ? 1 : 0}`,
	audio: (on: boolean) => `au,${on ? 1 : 0}`,
	mic: (on: boolean) => `mi,${on ? 1 : 0}`,
	/** Ask for a desktop of this size (dynamic resolution). */
	resize: (w: number, h: number) => `sz,${w},${h}`,
	display: (id: number) => `ds,${id}`,
	keyframe: 'kf',
	pong: (sent: number) => `po,${sent}`,
} as const;

/** Header of a video message (after the type byte). */
export interface VideoHeader {
	key: boolean;
	frame: number;
	/** Server-side pipeline time (capture to send), in 0.1 ms. */
	pipe: number;
	stripe: number;
	/** Stripe updates that make up this frame. */
	parts: number;
}

export function parseVideoHeader(b: Uint8Array): VideoHeader {
	const u32 = (i: number) =>
		((b[i] << 24) | (b[i + 1] << 16) | (b[i + 2] << 8) | b[i + 3]) >>> 0;
	return {
		key: (b[1] & 1) === 1,
		frame: u32(2),
		pipe: u32(6),
		stripe: b[10],
		parts: b[12],
	};
}
export const VIDEO_HEADER_BYTES = 16;

// ---- page <-> worker ----

export interface StripeFrame {
	i: number;
	y: number;
	frame: VideoFrame;
}

export interface Stats {
	t: 'stats';
	fps: number;
	kbps: number;
	decodeMs: number;
	pipeMs: number;
	codec: string | null;
	stripes: number;
	dropped: number;
	queue: number;
}

/** Worker -> page. Server messages other than ping and close are forwarded as is. */
export type FromWorker =
	| { t: 'frames'; parts: StripeFrame[] }
	| { t: 'status'; s: 'open' | 'closed' }
	| { t: 'cursor'; buf: ArrayBuffer }
	| { t: 'clipImage'; buf: ArrayBuffer }
	| { t: 'log'; s: string }
	| Stats
	| Exclude<ServerMessage, { t: 'ping' } | { t: 'close' }>;

/** Page -> worker. */
export type ToWorker =
	| { t: 'init'; canvas: OffscreenCanvas; url: string; presentToMain: boolean }
	| { t: 'send'; d: string }
	| { t: 'sendBin'; d: ArrayBuffer }
	| { t: 'video'; on: boolean }
	| { t: 'audioPort'; port: MessagePort };

/** Worker -> desktop audio worklet: one decoded chunk, planar. */
export interface PcmChunk {
	l: Float32Array;
	r: Float32Array;
}
