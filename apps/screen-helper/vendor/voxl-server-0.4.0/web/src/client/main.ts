// Main thread: layout, input capture, cursor, clipboard, files, microphone and audio.
// Video decode/paint and the socket live in the worker (worker.ts).
import { cmd } from '../protocol';
import type { FromWorker, ToWorker } from '../protocol';
import type { App } from './app';
import { el } from './app';
import { createAudio } from './audio';
import { createClipboard } from './clipboard';
import { createCursor } from './cursor';
import { createFiles } from './files';
import { setupKeyboard } from './keyboard';
import { createMic } from './mic';
import { setupPointer } from './pointer';
import { createResize } from './resize';
import { setupToolbar } from './toolbar';
import { createVideo } from './video';
import { createZoom } from './zoom';

/**
 * Commands a read-only viewer sends: acks, video and audio on/off, keyframe requests
 * and ping replies. Anything else would act on the desktop (input, clipboard,
 * microphone, resizing). The server accepts only these from read-only viewers too
 * (READ_ONLY_COMMANDS in server.rs), so a page that ignores the hello flag cannot
 * drive the desktop either; this keeps it from sending what would be dropped.
 */
const READ_ONLY_COMMANDS = new Set(['a', 'v', 'au', 'kf', 'po']);

function blocksInput(app: App, command: string): boolean {
	return (
		(app.readOnly || !app.desk.w) &&
		!READ_ONLY_COMMANDS.has(command.split(',')[0])
	);
}

function main() {
	const canvas = el<HTMLCanvasElement>('screen');
	const statusEl = el('status');
	const statsEl = el('stats');
	const params = new URLSearchParams(location.search);
	window.fps = 0;
	window.voxlStats = {};

	// Browsers only offer WebCodecs on HTTPS pages and localhost, so a page opened over
	// plain HTTP from another machine can't decode the stream whatever the browser.
	if (!window.isSecureContext) {
		statusEl.textContent =
			'voxl needs HTTPS or localhost: browsers only decode video on secure pages. Serve it over HTTPS, or forward the port and open localhost.';
		return;
	}
	if (!('VideoDecoder' in window) || !canvas.transferControlToOffscreen) {
		statusEl.textContent =
			'This browser lacks WebCodecs/OffscreenCanvas support.';
		return;
	}

	const wsUrl = new URL('ws', location.href);
	wsUrl.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
	wsUrl.search = location.search;
	const worker = new Worker('worker.js' + location.search);
	const post = (m: ToWorker, transfer: Transferable[] = []) =>
		worker.postMessage(m, transfer);
	const app: App = {
		worker,
		post,
		send: (d) => {
			if (!blocksInput(app, d)) post({ t: 'send', d });
		},
		sendBin: (type, bytes) => {
			// The server drops every binary message from read-only viewers.
			if (app.readOnly || !app.desk.w) return;
			const msg = new Uint8Array(bytes.byteLength + 1);
			msg[0] = type;
			msg.set(bytes, 1);
			post({ t: 'sendBin', d: msg.buffer }, [msg.buffer]);
		},
		surface: el('surface'),
		kbd: el<HTMLTextAreaElement>('kbd'),
		tools: el('tools'),
		isMac: /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent),
		remoteMac: false,
		readOnly: false,
		inIframe: window !== window.top,
		desk: { w: 0, h: 0 },
		view: { scale: 1 },
	};

	const cursor = createCursor(app);
	const zoom = createZoom(app);
	const useVideo =
		params.get('present') !== 'canvas' &&
		typeof MediaStreamTrackGenerator !== 'undefined';
	const video = createVideo(app, canvas, useVideo, () => {
		cursor.apply();
		zoom.fit();
	});
	const clipboard = createClipboard(app);
	const audio = createAudio(app);
	// Browsers only allow audio and clipboard writes after a user gesture.
	const onInteract = () => {
		audio.start();
		clipboard.flush();
	};
	const pointer = setupPointer(app, zoom, onInteract);
	setupKeyboard(app, clipboard, onInteract);
	const updateDisplays = setupToolbar(app, video.layout, pointer.setTrackpad);
	const files = createFiles(app);
	const mic = createMic(app);
	const resize = createResize(app);

	const offscreen = canvas.transferControlToOffscreen();
	post(
		{ t: 'init', canvas: offscreen, url: wsUrl.href, presentToMain: useVideo },
		[offscreen]
	);

	// Stop video when nobody can see it.
	let videoOn: boolean | null = null;
	function updateVisibility(force = false) {
		const on =
			!document.hidden && window.innerWidth > 0 && window.innerHeight > 0;
		if (on !== videoOn || force) {
			videoOn = on;
			post({ t: 'video', on });
		}
	}
	document.addEventListener('visibilitychange', () => {
		updateVisibility();
		if (document.hidden) app.send(cmd.releaseAll);
	});
	window.addEventListener('resize', () => {
		video.layout();
		updateVisibility();
	});
	updateVisibility();

	worker.addEventListener('message', (e: MessageEvent<FromWorker>) => {
		const m = e.data;
		switch (m.t) {
			case 'frames':
				video.present(m.parts);
				break;
			case 'hello':
				app.desk.w = m.w;
				app.desk.h = m.h;
				app.remoteMac = m.os === 'macos';
				app.readOnly = !!m.readOnly;
				app.tools.classList.toggle('read-only', app.readOnly);
				video.build(m.stripes && m.stripes.length ? m.stripes : [[0, m.h]]);
				video.layout();
				files.setup(m.files);
				mic.setup(!!m.mic && !app.readOnly);
				updateDisplays(m.displays);
				resize.setup(m.resize);
				statusEl.style.display = 'none';
				break;
			case 'status':
				if (m.s === 'closed') {
					app.desk.w = 0;
					mic.stop();
					statusEl.style.display = '';
					statusEl.textContent = 'Reconnecting…';
				}
				if (m.s === 'open') updateVisibility(true);
				break;
			case 'cursor':
				cursor.set(m.buf);
				break;
			case 'clip':
				clipboard.onRemoteText(m.d);
				break;
			case 'clipImage':
				clipboard.onRemoteImage(m.buf);
				break;
			case 'mic':
				if (!m.on) mic.stop();
				break;
			case 'stats':
				window.fps = Math.round(m.fps);
				window.voxlStats = m;
				if (params.has('stats')) {
					statsEl.style.display = 'block';
					statsEl.textContent =
						`${m.fps.toFixed(1)} fps  ${(m.kbps / 1000).toFixed(2)} Mbps\n` +
						`decode ${m.decodeMs.toFixed(1)} ms  server ${m.pipeMs.toFixed(1)} ms\n${m.codec || ''} q=${m.queue}`;
				}
				break;
			case 'log':
				console.warn('[voxl]', m.s);
				break;
		}
	});

	// Autoplay may be allowed (iframe allow="autoplay"); try right away.
	audio.start();
}

main();
