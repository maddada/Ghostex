// Touch: tap = click, drag = left-drag, long press = right click, pinch = zoom the
// view in or out, two fingers = scroll. The content follows the fingers: two fingers
// move a zoomed-in view, and scroll the desktop once the view reaches its edge.
//
// Trackpad mode (the toolbar's trackpad button) makes the screen work like a laptop's
// trackpad instead: one finger moves the cursor from where it is, farther the faster
// it moves; tap = click and two-finger tap = right click where the cursor is; press
// and hold, then move = drag. Two fingers and pinch work as above.
import { cmd } from '../protocol';
import type { App } from './app';
import type { Zoom } from './zoom';

const SLOP = 10;
const LONG_PRESS = 550;
/** A tap this soon (GTK's and Qt's double-click time) and this close (CSS px) after
 * the last one clicks the same spot, so double-taps double-click even though the
 * finger lands a little off. */
const MULTI_TAP_MS = 400;
const MULTI_TAP_PX = 24;
/** Trackpad mode: two fingers lifted this soon after landing, without moving, right click. */
const TWO_TAP_MS = 300;
/** Trackpad mode's pointer acceleration: the cursor moves (1 + ACCEL * speed) times as
 * far across the desktop on screen as the finger, speed in CSS px per ms, at most
 * MAX_GAIN times: slow moves are precise, fast ones cross the screen. */
const ACCEL = 0.8;
const MAX_GAIN = 3;

interface Hooks {
	/** Move the remote pointer to a client (CSS pixel) position. */
	moveTo(x: number, y: number): void;
	/** Trackpad mode: move the remote pointer by (dx, dy) desktop pixels. */
	moveBy(dx: number, dy: number): void;
	/** Trackpad mode: send the cursor's position again, so a click lands on it. */
	point(): void;
	queueScroll(dx: number, dy: number): void;
	deskPerPx(): number;
	zoom: Zoom;
}

export function createTouch(app: App, hooks: Hooks) {
	const { send } = app;
	const pts = new Map<number, { x: number; y: number }>();
	let trackpad = false;
	/** 'two': two fingers down, not yet moved enough to tell a scroll from a pinch.
	 * 'move': trackpad mode, one finger moving the cursor. */
	let mode:
		| 'pending'
		| 'drag'
		| 'move'
		| 'two'
		| 'scroll'
		| 'pinch'
		| 'done'
		| null = null;
	let start = { x: 0, y: 0 };
	let timer: ReturnType<typeof setTimeout> | undefined;
	let lastMid: [number, number] = [0, 0];
	let startMid: [number, number] = [0, 0];
	let startSpread = 0;
	let lastTap = { x: 0, y: 0, t: -Infinity };
	/** Trackpad mode: where the finger was, and when, at the last cursor move. */
	let last = { x: 0, y: 0, t: 0 };
	/** When the second finger landed. */
	let twoAt = 0;
	const mid = (): [number, number] => {
		let x = 0,
			y = 0;
		for (const p of pts.values()) {
			x += p.x;
			y += p.y;
		}
		return [x / pts.size, y / pts.size];
	};
	/** Mean distance of the fingers from their centroid (half the gap for two). */
	const spread = () => {
		const [mx, my] = mid();
		let d = 0;
		for (const p of pts.values()) d += Math.hypot(p.x - mx, p.y - my);
		return d / pts.size;
	};
	/** Measure the gesture from where the fingers are now (they were added or lifted). */
	const rebase = () => {
		startMid = lastMid = mid();
		startSpread = spread();
		if (mode === 'pinch') hooks.zoom.begin(...startMid, startSpread);
	};
	const click = (button: number) => {
		send(cmd.button(button, true));
		send(cmd.button(button, false));
	};
	/** Trackpad mode: move the cursor as far as the finger moved, more when it's fast. */
	function glide(p: { x: number; y: number }, t: number) {
		const dx = p.x - last.x,
			dy = p.y - last.y;
		const speed = Math.hypot(dx, dy) / Math.max(1, t - last.t);
		const k = hooks.deskPerPx() * Math.min(MAX_GAIN, 1 + ACCEL * speed);
		last = { x: p.x, y: p.y, t };
		hooks.moveBy(dx * k, dy * k);
	}

	function down(e: PointerEvent) {
		try {
			app.surface.setPointerCapture(e.pointerId);
		} catch {}
		pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
		if (pts.size === 1) {
			mode = 'pending';
			start = { x: e.clientX, y: e.clientY };
			last = { ...start, t: e.timeStamp };
			timer = setTimeout(() => {
				if (mode !== 'pending') return;
				if (trackpad) {
					// Held still: press the button where the cursor is and drag from there.
					hooks.point();
					send(cmd.button(1, true));
					mode = 'drag';
				} else {
					hooks.moveTo(start.x, start.y);
					click(3);
					mode = 'done';
				}
				navigator.vibrate?.(10);
			}, LONG_PRESS);
		} else {
			if (pts.size === 2) {
				clearTimeout(timer);
				if (mode === 'drag') send(cmd.button(1, false));
				mode = 'two';
				twoAt = e.timeStamp;
			}
			rebase();
		}
	}
	function move(e: PointerEvent) {
		const p = pts.get(e.pointerId);
		if (!p) return;
		p.x = e.clientX;
		p.y = e.clientY;
		if (mode === 'pending' && Math.hypot(p.x - start.x, p.y - start.y) > SLOP) {
			clearTimeout(timer);
			if (trackpad) mode = 'move';
			else {
				mode = 'drag';
				hooks.moveTo(start.x, start.y);
				send(cmd.button(1, true));
			}
		}
		if (mode === 'move' || (mode === 'drag' && trackpad)) glide(p, e.timeStamp);
		else if (mode === 'drag') hooks.moveTo(p.x, p.y);
		if (pts.size < 2) return;
		if (mode === 'two') {
			// A pinch changes the fingers' gap faster than it moves their centroid
			// (twice as fast with one finger still); a scroll moves them together.
			const [mx, my] = mid();
			const moved = Math.hypot(mx - startMid[0], my - startMid[1]);
			const gap = 2 * Math.abs(spread() - startSpread);
			if (gap > SLOP && gap > moved) {
				mode = 'pinch';
				rebase();
			} else if (moved > SLOP) mode = 'scroll';
		}
		if (mode === 'pinch') hooks.zoom.pinch(...mid(), spread());
		else if (mode === 'scroll') {
			const m = mid(),
				k = hooks.deskPerPx();
			const [rx, ry] = hooks.zoom.pan(m[0] - lastMid[0], m[1] - lastMid[1]);
			hooks.queueScroll(-rx * k, -ry * k);
			lastMid = m;
		}
	}
	function up(e: PointerEvent) {
		if (!pts.delete(e.pointerId)) return;
		if (mode === 'pending') {
			clearTimeout(timer);
			if (trackpad) {
				// The cursor stays put between taps, so double-taps double-click.
				hooks.point();
				click(1);
			} else {
				const now = performance.now();
				const at =
					now - lastTap.t < MULTI_TAP_MS &&
					Math.hypot(start.x - lastTap.x, start.y - lastTap.y) < MULTI_TAP_PX
						? lastTap
						: start;
				lastTap = { x: at.x, y: at.y, t: now };
				hooks.moveTo(at.x, at.y);
				click(1);
			}
			mode = 'done';
		} else if (mode === 'drag') {
			send(cmd.button(1, false));
			mode = 'done';
		} else if (mode === 'move') mode = 'done';
		else if (
			mode === 'two' &&
			trackpad &&
			pts.size === 1 &&
			e.timeStamp - twoAt < TWO_TAP_MS
		) {
			hooks.point();
			click(3);
			mode = 'done';
		} else if (mode === 'two' || mode === 'scroll' || mode === 'pinch') {
			if (pts.size < 2) mode = 'done';
			else rebase();
		}
		if (pts.size === 0) mode = null;
	}
	function cancel(e: PointerEvent) {
		clearTimeout(timer);
		if (mode === 'drag') send(cmd.button(1, false));
		pts.delete(e.pointerId);
		mode = pts.size ? 'done' : null;
	}
	/** Switch trackpad mode on or off, ending a gesture in progress. */
	function setTrackpad(on: boolean) {
		trackpad = on;
		clearTimeout(timer);
		if (mode === 'drag') send(cmd.button(1, false));
		mode = pts.size ? 'done' : null;
	}
	return { down, move, up, cancel, setTrackpad };
}
