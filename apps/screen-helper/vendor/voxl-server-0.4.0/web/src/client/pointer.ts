// Mouse, wheel/trackpad and touch input on the desktop surface.
import { cmd } from '../protocol';
import type { App } from './app';
import { el } from './app';
import { createTouch } from './touch';
import type { Zoom } from './zoom';

/** DOM button -> X button (left, middle, right, back, forward). */
const BUTTONS = [1, 2, 3, 8, 9];
/** Trackpad mode keeps the cursor this far (CSS px) inside the window in a zoomed-in view. */
const EDGE = 32;

/** How far (dx) to move the view so that `at` is EDGE inside [0, size]. */
const inside = (at: number, size: number) =>
	at < EDGE ? EDGE - at : at > size - EDGE ? size - EDGE - at : 0;

/** A desktop coordinate as a CSS percentage of the surface; the middle if unknown. */
const percent = (v: number | undefined, size: number) =>
	v !== undefined && size ? `${(v * 100) / size}%` : '50%';

export function setupPointer(app: App, zoom: Zoom, onInteract: () => void) {
	const { surface, send } = app;
	/** Trackpad mode's cursor: touch screens show no pointer, so the page draws one. */
	const pointer = el('pointer');
	/** Behind the desktop and as big as the window: trackpad mode's touches around it. */
	const pad = el('pad');
	/** The pointer's desktop position (fractional in trackpad mode); null until it moves. */
	let pos: { x: number; y: number } | null = null;
	let trackpad = false;

	function toDesk(p: { clientX: number; clientY: number }): [number, number] {
		const r = surface.getBoundingClientRect();
		const x = Math.round(((p.clientX - r.left) * app.desk.w) / r.width);
		const y = Math.round(((p.clientY - r.top) * app.desk.h) / r.height);
		return [
			Math.max(0, Math.min(app.desk.w - 1, x)),
			Math.max(0, Math.min(app.desk.h - 1, y)),
		];
	}
	let lastMove = '';
	/** Move the remote pointer to a desktop position; returns it within the desktop. */
	function moveDesk(x: number, y: number, force = false) {
		const at = (pos = {
			x: Math.max(0, Math.min(app.desk.w - 1, x)),
			y: Math.max(0, Math.min(app.desk.h - 1, y)),
		});
		place();
		const s = cmd.move(Math.round(at.x), Math.round(at.y));
		if (s !== lastMove || force) {
			lastMove = s;
			send(s);
		}
		return at;
	}
	function move(p: { clientX: number; clientY: number }, force = false) {
		if (!app.desk.w) return;
		moveDesk(...toDesk(p), force);
	}
	/** Draw trackpad mode's cursor where the pointer is (the middle until it moves). */
	function place() {
		if (!trackpad) return;
		pointer.style.left = percent(pos?.x, app.desk.w);
		pointer.style.top = percent(pos?.y, app.desk.h);
	}
	/** Trackpad mode: move the pointer by (dx, dy) desktop pixels, and a zoomed-in view
	 * along with it when the cursor nears the window's edge. */
	function moveBy(dx: number, dy: number, force = false) {
		if (!app.desk.w) return;
		const from = pos ?? { x: app.desk.w / 2, y: app.desk.h / 2 };
		const at = moveDesk(from.x + dx, from.y + dy, force);
		const r = surface.getBoundingClientRect();
		const cx = r.left + (at.x * r.width) / app.desk.w,
			cy = r.top + (at.y * r.height) / app.desk.h;
		const px = inside(cx, window.innerWidth),
			py = inside(cy, window.innerHeight);
		if (px || py) zoom.pan(px, py);
	}

	// Scroll is sent in desktop pixels, batched per frame; the server turns it into
	// smooth (pixel-precise) scrolling, or wheel clicks on X servers that can't.
	let sx = 0,
		sy = 0,
		queued = false;
	function queueScroll(dx: number, dy: number) {
		sx += dx;
		sy += dy;
		if (queued) return;
		queued = true;
		requestAnimationFrame(() => {
			queued = false;
			if (Math.abs(sx) < 0.5 && Math.abs(sy) < 0.5) return;
			send(cmd.scroll(sx, sy));
			sx = 0;
			sy = 0;
		});
	}
	/** Desktop pixels per on-screen pixel. */
	const deskPerPx = () => app.desk.w / surface.getBoundingClientRect().width;

	const touch = createTouch(app, {
		moveTo: (x, y) => move({ clientX: x, clientY: y }, true),
		moveBy,
		point: () => moveBy(0, 0, true),
		queueScroll,
		deskPerPx,
		zoom,
	});

	surface.addEventListener('pointermove', (e) => {
		if (e.pointerType === 'touch') touch.move(e);
		else move(e);
	});
	surface.addEventListener('pointerdown', (e) => {
		e.preventDefault();
		onInteract();
		if (e.pointerType === 'touch') {
			touch.down(e);
			return;
		}
		app.kbd.focus({ preventScroll: true });
		move(e);
		try {
			surface.setPointerCapture(e.pointerId);
		} catch {}
		send(cmd.button(BUTTONS[e.button] || 1, true));
	});
	surface.addEventListener('pointerup', (e) => {
		e.preventDefault();
		if (e.pointerType === 'touch') {
			touch.up(e);
			return;
		}
		move(e);
		send(cmd.button(BUTTONS[e.button] || 1, false));
	});
	surface.addEventListener('pointercancel', (e) => {
		if (e.pointerType === 'touch') touch.cancel(e);
		else send(cmd.releaseAll);
	});
	// The touch captures the pointer to the surface, so its other events come there.
	pad.addEventListener('pointerdown', (e) => {
		if (e.pointerType !== 'touch') return;
		e.preventDefault();
		onInteract();
		touch.down(e);
	});
	pad.addEventListener('contextmenu', (e) => e.preventDefault());
	surface.addEventListener('contextmenu', (e) => e.preventDefault());
	surface.addEventListener(
		'wheel',
		(e) => {
			e.preventDefault();
			move(e);
			const k =
				(e.deltaMode === 1 ? 40 : e.deltaMode === 2 ? 800 : 1) * deskPerPx();
			queueScroll(e.deltaX * k, e.deltaY * k);
		},
		{ passive: false }
	);

	return {
		/** Touch moves a cursor like a laptop's trackpad (touch.ts) instead of pointing. */
		setTrackpad(on: boolean) {
			trackpad = on;
			surface.classList.toggle('trackpad', on);
			pad.hidden = !on;
			touch.setTrackpad(on);
			place();
		},
	};
}
