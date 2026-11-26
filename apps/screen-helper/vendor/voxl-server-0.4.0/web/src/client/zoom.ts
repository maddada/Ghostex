// Pinch to zoom: magnifies the desktop on this screen (the remote desktop is
// unchanged). The view is a CSS transform on the surface, so pointer input, which
// maps client positions through the surface's on-screen box, follows it.
import type { App } from './app';

/** Zoom in until one desktop pixel covers this many CSS pixels. */
const MAX_CSS_PX_PER_DESK_PX = 4;

export type Zoom = ReturnType<typeof createZoom>;

/** An axis' offset (from `at`, the untransformed position) that keeps the zoomed
 * desktop (`size` long) covering the window, or centered in it while it is smaller. */
function place(t: number, at: number, size: number, view: number) {
	if (size <= view) return (view - size) / 2 - at;
	return Math.min(-at, Math.max(view - size - at, t));
}

export function createZoom(app: App) {
	const { surface } = app;
	let z = 1,
		tx = 0,
		ty = 0;
	/** The surface's untransformed box, in client pixels. */
	let box = { x: 0, y: 0, w: 0, h: 0 };
	/** The view and the fingers' centroid and spread when the pinch began. */
	let from = { z: 1, tx: 0, ty: 0, x: 0, y: 0, spread: 1 };

	function measure() {
		const r = surface.getBoundingClientRect();
		box = { x: r.left - tx, y: r.top - ty, w: r.width / z, h: r.height / z };
	}

	const limit = (nz: number) =>
		Math.min(
			Math.max(1, MAX_CSS_PX_PER_DESK_PX / app.view.scale),
			Math.max(1, nz)
		);

	function set(nz: number, ntx: number, nty: number) {
		z = limit(nz);
		// Trackpad mode's cursor scales back by this to keep its size (index.html).
		surface.style.setProperty('--zoom', String(z));
		if (z === 1) {
			tx = ty = 0;
			surface.style.transform = '';
			return;
		}
		tx = place(ntx, box.x, box.w * z, window.innerWidth);
		ty = place(nty, box.y, box.h * z, window.innerHeight);
		surface.style.transform = `translate(${tx}px, ${ty}px) scale(${z})`;
	}

	return {
		/** Start a pinch with the fingers' centroid at (x, y), `spread` apart. */
		begin(x: number, y: number, spread: number) {
			measure();
			from = { z, tx, ty, x, y, spread: Math.max(1, spread) };
		},
		/** Zoom by how much the fingers spread, keeping the desktop under them. */
		pinch(x: number, y: number, spread: number) {
			const nz = limit((from.z * Math.max(1, spread)) / from.spread);
			// The desktop point that was under the centroid stays under it.
			const ux = (from.x - box.x - from.tx) / from.z,
				uy = (from.y - box.y - from.ty) / from.z;
			set(nz, x - box.x - nz * ux, y - box.y - nz * uy);
		},
		/** Move a zoomed-in view by (dx, dy); returns what it couldn't move (at an edge). */
		pan(dx: number, dy: number): [number, number] {
			if (z === 1) return [dx, dy];
			const ox = tx,
				oy = ty;
			set(z, tx + dx, ty + dy);
			return [dx - (tx - ox), dy - (ty - oy)];
		},
		/** Keep the view inside the window after the desktop is laid out again. */
		fit() {
			measure();
			set(z, tx, ty);
		},
	};
}
