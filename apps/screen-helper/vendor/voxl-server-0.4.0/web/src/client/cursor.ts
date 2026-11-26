// The remote cursor, drawn locally by the browser so pointer movement has no stream
// latency. Cursor messages: type byte, pad, width, height, hot x, hot y (u16 each),
// serial (u32), then RGBA.
import type { App } from './app';
import { el } from './app';

// On-screen cursor size bounds in CSS pixels, applied to the image's longer side.
// 24 is the standard pointer size (Xcursor's default), so a desktop scaled down
// into a small window (an embedded sidebar) keeps a normal-sized pointer.
const MIN_CURSOR_PX = 24;
const MAX_CURSOR_PX = 128;

export function createCursor(app: App) {
	let msg: ArrayBuffer | null = null;
	const scaled = document.createElement('canvas');
	/** Trackpad mode's cursor (pointer.ts), which shows the same image. */
	const pointer = el('pointer');

	function apply() {
		if (!msg) return;
		const v = new DataView(msg);
		const w = v.getUint16(2),
			h = v.getUint16(4),
			hx = v.getUint16(6),
			hy = v.getUint16(8);
		if (!w || !h) {
			app.surface.style.cursor = 'none';
			pointer.classList.add('none');
			return;
		}
		const src = document.createElement('canvas');
		src.width = w;
		src.height = h;
		src
			.getContext('2d')!
			.putImageData(
				new ImageData(new Uint8ClampedArray(msg, 14, w * h * 4), w, h),
				0,
				0
			);
		// Cursor images are in desktop pixels; show them at the on-screen scale, but
		// never smaller than a normal pointer.
		const dpr = window.devicePixelRatio || 1;
		const size = Math.max(w, h);
		const s = Math.min(
			Math.max(app.view.scale, MIN_CURSOR_PX / size),
			MAX_CURSOR_PX / size
		);
		const cw = Math.max(1, Math.round(w * s)),
			ch = Math.max(1, Math.round(h * s));
		const dpw = Math.max(1, Math.round(cw * dpr)),
			dph = Math.max(1, Math.round(ch * dpr));
		scaled.width = dpw;
		scaled.height = dph;
		const cx = scaled.getContext('2d')!;
		cx.imageSmoothingQuality = 'high';
		cx.clearRect(0, 0, dpw, dph);
		cx.drawImage(src, 0, 0, dpw, dph);
		const url = scaled.toDataURL('image/png');
		const hsx = Math.round(hx * s),
			hsy = Math.round(hy * s);
		app.surface.style.cursor =
			dpr > 1
				? `-webkit-image-set(url(${url}) ${dpr}x) ${hsx} ${hsy}, url(${url}) ${hsx} ${hsy}, default`
				: `url(${url}) ${hsx} ${hsy}, default`;
		if (dpr > 1 && !app.surface.style.cursor)
			app.surface.style.cursor = `url(${url}) ${hsx} ${hsy}, default`;
		Object.assign(pointer.style, {
			backgroundImage: `url(${url})`,
			width: `${cw}px`,
			height: `${ch}px`,
			margin: `${-hsy}px 0 0 ${-hsx}px`,
			transformOrigin: `${hsx}px ${hsy}px`,
		});
		pointer.classList.remove('none');
	}

	return {
		set(buf: ArrayBuffer) {
			msg = buf;
			apply();
		},
		apply,
	};
}
