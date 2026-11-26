// Owns the WebSocket, decodes H.264 with WebCodecs and paints straight onto an
// OffscreenCanvas the moment each frame is decoded (no rAF queueing), or hands the
// frames to the page when it presents them through <video>.
import { Bin, cmd, parseVideoHeader, VIDEO_HEADER_BYTES } from './protocol';
import type {
	FromWorker,
	PcmChunk,
	ServerMessage,
	Stats,
	Stripe,
	StripeFrame,
	ToWorker,
} from './protocol';

declare const self: DedicatedWorkerGlobalScope;

const noDraw = /nodraw/.test(self.location.search);
let presentToMain = false;
let ws: WebSocket | null = null;
let url = '';
let canvas: OffscreenCanvas | null = null;
let ctx: OffscreenCanvasRenderingContext2D | null = null;
let audioDecoder: AudioDecoder | null = null;
let audioPort: MessagePort | null = null;
let audioTs = 0;
let wantVideo = true;
let wantAudio = false;

// The screen arrives as independent H.264 stripes, one decoder each. A frame is a
// set of stripe updates sharing an id; frames are presented whole and in order.
interface StripeState {
	dec: VideoDecoder | null;
	codec: string | null;
	waitingKey: boolean;
	ts: number;
	/** Decoder timestamp -> frame id. */
	ids: Map<number, number>;
}
interface PendingFrame {
	need: number;
	got: StripeFrame[];
	t0: number;
}
let layout: Stripe[] = [];
const stripes: StripeState[] = [];
const frames = new Map<number, PendingFrame>();
/** Frame ids awaiting presentation, oldest first. */
const order: number[] = [];
let kfRequested = 0;
const st = {
	frames: 0,
	bytes: 0,
	decodeMs: 0,
	pipeMs: 0,
	lastReport: performance.now(),
	dropped: 0,
};

// Firefox throws if getContext('2d') is called twice on one canvas; reuse it.
// Fall back to default attrs if the ones below are rejected.
function get2dContext(
	c: OffscreenCanvas
): OffscreenCanvasRenderingContext2D | null {
	if (ctx) return ctx;
	try {
		return c.getContext('2d', { alpha: false, desynchronized: true });
	} catch {
		try {
			return c.getContext('2d');
		} catch {
			return null;
		}
	}
}

function send(s: string) {
	if (ws && ws.readyState === WebSocket.OPEN) ws.send(s);
}
function post(m: FromWorker, transfer: Transferable[] = []) {
	self.postMessage(m, transfer);
}

function connect() {
	const sock = new WebSocket(url);
	ws = sock;
	sock.binaryType = 'arraybuffer';
	sock.addEventListener('open', () => {
		post({ t: 'status', s: 'open' });
		send(cmd.video(wantVideo));
		send(cmd.audio(wantAudio));
	});
	sock.addEventListener('close', () => {
		post({ t: 'status', s: 'closed' });
		resetDecoder();
		setTimeout(connect, 1000);
	});
	sock.addEventListener('message', (e: MessageEvent<string | ArrayBuffer>) => {
		if (typeof e.data === 'string') return onText(e.data);
		const b = new Uint8Array(e.data);
		switch (b[0]) {
			case Bin.Video:
				onVideo(b);
				break;
			case Bin.Audio:
				onAudio(b);
				break;
			case Bin.Cursor:
				post({ t: 'cursor', buf: e.data }, [e.data]);
				break;
			case Bin.ClipImage:
				post({ t: 'clipImage', buf: e.data }, [e.data]);
				break;
		}
	});
}

function onText(s: string) {
	let m: ServerMessage;
	try {
		m = JSON.parse(s);
	} catch {
		return;
	}
	if (m.t === 'ping') {
		send(cmd.pong(m.s));
		return;
	}
	if (m.t === 'close') {
		try {
			ws?.close();
		} catch {}
		return;
	}
	if (m.t === 'hello' && canvas) {
		if (canvas.width !== m.w || canvas.height !== m.h) {
			canvas.width = m.w;
			canvas.height = m.h;
		}
		ctx = get2dContext(canvas);
		layout = m.stripes && m.stripes.length ? m.stripes : [[0, m.h]];
		resetDecoder();
	}
	post(m);
}

function resetDecoder() {
	for (const sd of stripes)
		if (sd.dec && sd.dec.state !== 'closed') {
			try {
				sd.dec.close();
			} catch {}
		}
	stripes.length = 0;
	for (const f of frames.values()) for (const g of f.got) g.frame.close();
	frames.clear();
	order.length = 0;
}

function requestKeyframe() {
	const now = performance.now();
	if (now - kfRequested > 500) {
		kfRequested = now;
		send(cmd.keyframe);
	}
}

const hex2 = (x: number) => x.toString(16).padStart(2, '0');

/** avc1.PPCCLL from the first SPS in an Annex-B keyframe. */
function codecFromSps(d: Uint8Array): string | null {
	for (let i = 0; i + 4 < d.length; i++) {
		if (
			d[i] === 0 &&
			d[i + 1] === 0 &&
			d[i + 2] === 1 &&
			(d[i + 3] & 0x1f) === 7
		) {
			return 'avc1.' + hex2(d[i + 4]) + hex2(d[i + 5]) + hex2(d[i + 6]);
		}
	}
	return null;
}

function stripeState(i: number): StripeState {
	return (
		stripes[i] ||
		(stripes[i] = {
			dec: null,
			codec: null,
			waitingKey: true,
			ts: 0,
			ids: new Map(),
		})
	);
}

function makeDecoder(i: number, codec: string): VideoDecoder {
	const sd = stripeState(i);
	const dec = new VideoDecoder({
		output: (frame) => {
			const id = sd.ids.get(frame.timestamp);
			sd.ids.delete(frame.timestamp);
			const f = id === undefined ? undefined : frames.get(id);
			if (!f) {
				frame.close();
				return;
			}
			f.got.push({ i, y: layout[i] ? layout[i][0] : 0, frame });
			flush();
		},
		error: (e) => {
			post({ t: 'log', s: `stripe ${i} decoder error: ${e.message}` });
			resetDecoder();
			requestKeyframe();
		},
	});
	dec.configure({
		codec,
		optimizeForLatency: true,
		hardwareAcceleration: 'no-preference',
	});
	return dec;
}

/** Present every complete frame at the head of the queue. */
function flush() {
	while (order.length) {
		const id = order[0];
		const f = frames.get(id);
		if (f && f.got.length < f.need) return;
		order.shift();
		frames.delete(id);
		if (!f) continue;
		if (f.got.length) {
			if (presentToMain)
				post(
					{ t: 'frames', parts: f.got },
					f.got.map((g) => g.frame)
				);
			else
				for (const g of f.got) {
					if (ctx && !noDraw) ctx.drawImage(g.frame, 0, g.y);
					g.frame.close();
				}
		}
		send(cmd.ack(id));
		st.frames++;
		st.decodeMs += performance.now() - f.t0;
	}
}

function onVideo(b: Uint8Array) {
	const h = parseVideoHeader(b);
	const data = b.subarray(VIDEO_HEADER_BYTES);
	st.bytes += b.length;
	let f = frames.get(h.frame);
	if (!f) {
		f = { need: h.parts, got: [], t0: performance.now() };
		frames.set(h.frame, f);
		order.push(h.frame);
		st.pipeMs += h.pipe / 10;
	}
	const sd = stripeState(h.stripe);
	if (sd.waitingKey && !h.key) {
		f.need--;
		st.dropped++;
		requestKeyframe();
		flush();
		return;
	}
	if (h.key) {
		const codec = codecFromSps(data) || 'avc1.640034';
		if (!sd.dec || sd.dec.state === 'closed' || codec !== sd.codec) {
			if (sd.dec && sd.dec.state !== 'closed') {
				try {
					sd.dec.close();
				} catch {}
			}
			sd.codec = codec;
			sd.ids.clear();
			try {
				sd.dec = makeDecoder(h.stripe, codec);
			} catch (e) {
				post({ t: 'log', s: 'configure failed ' + e });
				f.need--;
				flush();
				return;
			}
		}
		sd.waitingKey = false;
	}
	const ts = ++sd.ts;
	sd.ids.set(ts, h.frame);
	try {
		sd.dec!.decode(
			new EncodedVideoChunk({
				type: h.key ? 'key' : 'delta',
				timestamp: ts,
				data,
			})
		);
	} catch (e) {
		post({ t: 'log', s: 'decode threw ' + e });
		resetDecoder();
		requestKeyframe();
	}
}

function onAudio(b: Uint8Array) {
	const port = audioPort;
	if (!port || typeof AudioDecoder === 'undefined') return;
	if (!audioDecoder) {
		audioDecoder = new AudioDecoder({
			output: (ad) => {
				const n = ad.numberOfFrames;
				const chunk: PcmChunk = {
					l: new Float32Array(n),
					r: new Float32Array(n),
				};
				ad.copyTo(chunk.l, { planeIndex: 0, format: 'f32-planar' });
				if (ad.numberOfChannels > 1)
					ad.copyTo(chunk.r, { planeIndex: 1, format: 'f32-planar' });
				else chunk.r.set(chunk.l);
				ad.close();
				port.postMessage(chunk, [chunk.l.buffer, chunk.r.buffer]);
			},
			error: () => {
				audioDecoder = null;
			},
		});
		audioDecoder.configure({
			codec: 'opus',
			sampleRate: 48000,
			numberOfChannels: 2,
		});
	}
	audioTs += 10000;
	try {
		audioDecoder.decode(
			new EncodedAudioChunk({
				type: 'key',
				timestamp: audioTs,
				data: b.subarray(1),
			})
		);
	} catch {}
}

setInterval(() => {
	const now = performance.now();
	const dt = (now - st.lastReport) / 1000;
	const n = st.frames;
	const stats: Stats = {
		t: 'stats',
		fps: n / dt,
		kbps: (st.bytes * 8) / 1000 / dt,
		decodeMs: n ? st.decodeMs / n : 0,
		pipeMs: n ? st.pipeMs / n : 0,
		codec: stripes[0]?.codec ?? null,
		stripes: layout.length,
		dropped: st.dropped,
		queue: order.length,
	};
	post(stats);
	st.frames = 0;
	st.bytes = 0;
	st.decodeMs = 0;
	st.pipeMs = 0;
	st.lastReport = now;
}, 1000);

self.addEventListener('message', (e: MessageEvent<ToWorker>) => {
	const m = e.data;
	switch (m.t) {
		case 'init':
			canvas = m.canvas;
			url = m.url;
			presentToMain = m.presentToMain;
			ctx = get2dContext(canvas);
			connect();
			break;
		case 'send':
			send(m.d);
			break;
		case 'sendBin':
			if (ws && ws.readyState === WebSocket.OPEN) ws.send(m.d);
			break;
		case 'video':
			if (wantVideo !== m.on) {
				wantVideo = m.on;
				if (m.on) resetDecoder();
				send(cmd.video(m.on));
			}
			break;
		case 'audioPort':
			audioPort = m.port;
			wantAudio = true;
			send(cmd.audio(true));
			break;
	}
});
