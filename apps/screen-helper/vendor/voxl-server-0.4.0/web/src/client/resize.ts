// Dynamic resolution: ask the server for a desktop that matches this window, so it
// is shown 1:1 instead of being scaled down. Sent on connect and, debounced, when
// the window changes size (sidebar drags, fullscreen, rotating a tablet).
import { cmd } from '../protocol';
import type { App } from './app';

const DEBOUNCE_MS = 250;

export function createResize(app: App) {
	let scale = 0;
	let last = '';
	let timer: ReturnType<typeof setTimeout> | undefined;
	// ?scale= overrides the server's desktop-pixels-per-CSS-pixel (e.g. 2 for sharper text).
	const override = Number(new URLSearchParams(location.search).get('scale'));

	function send() {
		if (!scale || !window.innerWidth || !window.innerHeight) return;
		const s = override > 0 ? override : scale;
		const w = Math.round(window.innerWidth * s);
		const h = Math.round(window.innerHeight * s);
		const key = `${w},${h}`;
		if (key === last) return;
		last = key;
		app.send(cmd.resize(w, h));
	}

	/** From each hello: whether the server resizes (and at what scale). */
	function setup(cfg: { scale: number } | undefined) {
		scale = cfg?.scale ?? 0;
		last = '';
		send();
	}

	window.addEventListener('resize', () => {
		clearTimeout(timer);
		timer = setTimeout(send, DEBOUNCE_MS);
	});
	return { setup };
}
