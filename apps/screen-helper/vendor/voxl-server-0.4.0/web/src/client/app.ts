// What the page's modules share: the worker, the main elements and the desktop geometry.
import type { ToWorker } from '../protocol';

export interface App {
	worker: Worker;
	/** Send a text command (see `cmd` in protocol.ts). */
	send(command: string): void;
	/** Send a binary message: `type` byte, then `bytes`. */
	sendBin(type: number, bytes: Uint8Array): void;
	post(m: ToWorker, transfer?: Transferable[]): void;
	surface: HTMLElement;
	/** Hidden textarea that receives keyboard, IME and paste input. */
	kbd: HTMLTextAreaElement;
	tools: HTMLElement;
	isMac: boolean;
	/** The desktop is a Mac (from the server's hello). */
	remoteMac: boolean;
	/** The server only shows the desktop (--read-only), from the server's hello. */
	readOnly: boolean;
	inIframe: boolean;
	/** Desktop size in pixels; 0 until the server says hello. */
	desk: { w: number; h: number };
	/** On-screen CSS pixels per desktop pixel, before any pinch zoom (zoom.ts). */
	view: { scale: number };
}

export function el<T extends HTMLElement = HTMLElement>(id: string): T {
	const e = document.getElementById(id);
	if (!e) throw new Error(`voxl: #${id} is missing from index.html`);
	return e as T;
}
