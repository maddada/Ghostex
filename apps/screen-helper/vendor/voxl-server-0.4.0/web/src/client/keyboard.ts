// Keyboard, IME and paste input. Keys are sent as X keysyms; the server maps them to
// keycodes, so the browser's layout decides what each key types.
import { Bin, cmd } from '../protocol';
import type { App } from './app';
import type { Clipboard } from './clipboard';

const NAMED: Record<string, number> = {
	Backspace: 0xff08,
	Tab: 0xff09,
	Enter: 0xff0d,
	Escape: 0xff1b,
	Delete: 0xffff,
	Home: 0xff50,
	ArrowLeft: 0xff51,
	ArrowUp: 0xff52,
	ArrowRight: 0xff53,
	ArrowDown: 0xff54,
	PageUp: 0xff55,
	PageDown: 0xff56,
	End: 0xff57,
	Insert: 0xff63,
	CapsLock: 0xffe5,
	NumLock: 0xff7f,
	ScrollLock: 0xff14,
	Pause: 0xff13,
	PrintScreen: 0xff61,
	ContextMenu: 0xff67,
	AltGraph: 0xfe03,
	' ': 0x20,
};
for (let i = 1; i <= 24; i++) NAMED['F' + i] = 0xffbd + i;
const XK_BACKSPACE = 0xff08,
	XK_RETURN = 0xff0d,
	XK_DELETE = 0xffff;

/** Keysym of a printable character (Latin-1 directly, Unicode via 0x01000000 + code point). */
const charKeysym = (cp: number) =>
	(cp >= 0x20 && cp <= 0x7e) || (cp >= 0xa0 && cp <= 0xff)
		? cp
		: 0x01000000 + cp;

async function toPng(file: File): Promise<ArrayBuffer> {
	if (file.type === 'image/png') return file.arrayBuffer();
	const bmp = await createImageBitmap(file);
	const c = new OffscreenCanvas(bmp.width, bmp.height);
	c.getContext('2d')!.drawImage(bmp, 0, 0);
	return (await c.convertToBlob({ type: 'image/png' })).arrayBuffer();
}

export function setupKeyboard(
	app: App,
	clipboard: Clipboard,
	onInteract: () => void
) {
	const { kbd, surface, send, isMac } = app;
	const XK_CONTROL_L = 0xffe3,
		XK_CONTROL_R = 0xffe4,
		XK_SUPER_L = 0xffeb,
		XK_SUPER_R = 0xffec;
	const BY_CODE: Record<string, number> = {
		ShiftLeft: 0xffe1,
		ShiftRight: 0xffe2,
		AltLeft: 0xffe9,
		AltRight: 0xffea,
		NumpadEnter: 0xff0d,
	};
	// The shortcut key is Command on a Mac and Control elsewhere. Linux desktops get
	// Control for a Mac viewer's Command; a Mac desktop gets Command for another
	// viewer's Control (and Control for its Windows/Super key).
	function modifierKeysym(code: string): number | undefined {
		const isCtrl = code.startsWith('Control');
		const isMeta = code.startsWith('Meta') || code.startsWith('OS');
		if (!isCtrl && !isMeta) return undefined;
		const right = code.endsWith('Right');
		const ctrl = right ? XK_CONTROL_R : XK_CONTROL_L;
		const sup = right ? XK_SUPER_R : XK_SUPER_L;
		if (app.remoteMac) return isMac === isCtrl ? ctrl : sup;
		if (isMac && code.startsWith('Meta')) return ctrl;
		return isCtrl ? ctrl : sup;
	}

	function keysymOf(e: KeyboardEvent): number {
		const mod = modifierKeysym(e.code);
		if (mod !== undefined) return mod;
		if (BY_CODE[e.code] !== undefined) {
			if (e.code === 'AltRight' && e.key === 'AltGraph') return 0xfe03;
			return BY_CODE[e.code];
		}
		const k = e.key;
		if (NAMED[k] !== undefined) return NAMED[k];
		if (!k || k === 'Dead' || k === 'Unidentified' || k === 'Process') return 0;
		if ([...k].length !== 1) return 0;
		return charKeysym(k.codePointAt(0)!);
	}

	const down = new Map<string, number>(); // code -> keysym
	let pendingPaste: {
		code: string;
		ks: number;
		timer: ReturnType<typeof setTimeout>;
	} | null = null;
	const press = (code: string, ks: number) => {
		down.set(code, ks);
		send(cmd.key(ks, true));
	};
	const tap = (ks: number) => {
		send(cmd.key(ks, true));
		send(cmd.key(ks, false));
	};
	function release(code: string) {
		const ks = down.get(code);
		if (ks === undefined) return;
		down.delete(code);
		send(cmd.key(ks, false));
	}
	function releaseAll() {
		down.clear();
		send(cmd.releaseAll);
	}
	function flushPaste() {
		if (!pendingPaste) return;
		const p = pendingPaste;
		pendingPaste = null;
		clearTimeout(p.timer);
		press(p.code, p.ks);
	}

	function onKeyDown(e: KeyboardEvent) {
		if (e.isComposing) return;
		onInteract();
		const ks = keysymOf(e);
		if (!ks) return;
		const accel = isMac ? e.metaKey : e.ctrlKey;
		if (accel && !e.altKey && (e.key === 'v' || e.key === 'V') && !e.repeat) {
			// Let the browser deliver a paste event to the hidden textarea first, so the
			// remote clipboard holds the local contents before the shortcut lands.
			pendingPaste = { code: e.code, ks, timer: setTimeout(flushPaste, 150) };
			return;
		}
		e.preventDefault();
		const held = down.get(e.code);
		if (e.repeat && held !== undefined) {
			send(cmd.key(held, true));
			return;
		}
		press(e.code, ks);
	}
	function onKeyUp(e: KeyboardEvent) {
		e.preventDefault();
		if (pendingPaste && pendingPaste.code === e.code) flushPaste();
		release(e.code);
		// macOS swallows keyups of keys released while Cmd is held.
		if (isMac && (e.code === 'MetaLeft' || e.code === 'MetaRight')) {
			for (const code of down.keys())
				if (!/^(Shift|Control|Alt|Meta)/.test(code)) release(code);
		}
	}
	for (const target of [kbd, surface]) {
		target.addEventListener('keydown', onKeyDown);
		target.addEventListener('keyup', onKeyUp);
	}

	kbd.addEventListener('paste', (e) => {
		e.preventDefault();
		const dt = e.clipboardData;
		const text = dt?.getData('text/plain');
		if (text) {
			clipboard.noteLocal(text);
			send(cmd.clipboard(text));
			flushPaste();
			return;
		}
		const file = dt
			? [...dt.items]
					.find((i) => i.kind === 'file' && i.type.startsWith('image/'))
					?.getAsFile()
			: null;
		if (!file) {
			flushPaste();
			return;
		}
		// Hold the paste shortcut until the image is on the remote clipboard.
		if (pendingPaste) {
			clearTimeout(pendingPaste.timer);
			pendingPaste.timer = setTimeout(flushPaste, 3000);
		}
		toPng(file)
			.then((png) => app.sendBin(Bin.ClipImageUp, new Uint8Array(png)))
			.catch((err) => console.warn('[voxl] image paste failed', err))
			.finally(flushPaste);
	});

	// Text that arrives without usable keydowns (IME, dead-key composition, mobile
	// keyboards, insertText) is typed as individual keysyms.
	function typeText(text: string) {
		for (const ch of text)
			tap(ch === '\n' ? XK_RETURN : charKeysym(ch.codePointAt(0)!));
	}
	kbd.addEventListener('compositionend', (e) => {
		if (e.data) typeText(e.data);
		kbd.value = '';
	});
	kbd.addEventListener('input', (e) => {
		const ie = e as InputEvent;
		if (ie.isComposing) return;
		// On-screen keyboards often deliver text and edits only as input events.
		if (ie.inputType === 'insertText' && ie.data) typeText(ie.data);
		else if (ie.inputType === 'deleteContentBackward') tap(XK_BACKSPACE);
		else if (ie.inputType === 'deleteContentForward') tap(XK_DELETE);
		else if (
			ie.inputType === 'insertLineBreak' ||
			ie.inputType === 'insertParagraph'
		)
			tap(XK_RETURN);
		kbd.value = '';
	});
	window.addEventListener('blur', releaseAll);
	surface.addEventListener('focus', () => kbd.focus({ preventScroll: true }));
	if (!app.inIframe) kbd.focus({ preventScroll: true });

	return { releaseAll };
}
