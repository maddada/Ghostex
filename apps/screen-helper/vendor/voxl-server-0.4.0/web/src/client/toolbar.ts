// Toolbar (top right on hover; bottom right and always visible on touch devices):
// display picker, on-screen keyboard, trackpad mode, files, microphone and fullscreen
// buttons.
import { cmd } from '../protocol';
import type { Hello } from '../protocol';
import type { App } from './app';
import { el } from './app';

/** localStorage key that remembers trackpad mode on this device. */
const TRACKPAD_KEY = 'voxl-trackpad';

export function setupToolbar(
	app: App,
	onFullscreenChange: () => void,
	onTrackpad: (on: boolean) => void
) {
	const { tools, kbd } = app;
	const kbBtn = el<HTMLButtonElement>('kb-btn');
	const tpBtn = el<HTMLButtonElement>('tp-btn');
	const fsBtn = el<HTMLButtonElement>('fs-btn');
	const picker = el('display-picker');
	const displayBtn = el<HTMLButtonElement>('display-btn');
	const displayLabel = el('display-label');
	const menu = el('display-menu');
	let selected = 0;
	const options = () => [...menu.children] as HTMLElement[];

	function openMenu() {
		menu.hidden = false;
		displayBtn.setAttribute('aria-expanded', 'true');
		tools.classList.add('menu-open');
		menu
			.querySelector<HTMLElement>('[aria-selected="true"]')
			?.focus({ preventScroll: true });
	}
	function closeMenu(toDesktop: boolean) {
		if (menu.hidden) return;
		menu.hidden = true;
		displayBtn.setAttribute('aria-expanded', 'false');
		tools.classList.remove('menu-open');
		// Keys go to the desktop again; touch devices skip this to keep the soft keyboard closed.
		if (toDesktop && !tools.classList.contains('touch'))
			kbd.focus({ preventScroll: true });
	}
	function pick(option: HTMLElement) {
		const id = Number(option.dataset.id);
		if (id !== selected) {
			app.send(cmd.releaseAll);
			app.send(cmd.display(id));
			displayBtn.disabled = true;
		}
		closeMenu(true);
	}

	displayBtn.addEventListener('click', () =>
		menu.hidden ? openMenu() : closeMenu(true)
	);
	displayBtn.addEventListener('keydown', (e) => {
		if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
		e.preventDefault();
		openMenu();
	});
	menu.addEventListener('keydown', (e) => {
		const all = options();
		const i = all.indexOf(document.activeElement as HTMLElement);
		const n = all.length;
		const move = (to: number) => all[to]?.focus({ preventScroll: true });
		switch (e.key) {
			case 'ArrowDown':
				move((i + 1) % n);
				break;
			case 'ArrowUp':
				move(i < 0 ? n - 1 : (i - 1 + n) % n);
				break;
			case 'Home':
				move(0);
				break;
			case 'End':
				move(n - 1);
				break;
			case 'Enter':
			case ' ':
				if (all[i]) pick(all[i]);
				break;
			case 'Escape':
			case 'Tab':
				closeMenu(true);
				break;
			default:
				return;
		}
		// Esc would otherwise also close the files panel.
		e.preventDefault();
		e.stopPropagation();
	});
	// Like the app's menus, the option under the pointer is the highlighted one.
	menu.addEventListener('pointermove', (e) => {
		const option = (e.target as Element).closest<HTMLElement>('.option');
		if (option && document.activeElement !== option)
			option.focus({ preventScroll: true });
	});
	menu.addEventListener('pointerleave', () =>
		menu.focus({ preventScroll: true })
	);
	menu.addEventListener('click', (e) => {
		const option = (e.target as Element).closest<HTMLElement>('.option');
		if (option) pick(option);
	});
	document.addEventListener(
		'pointerdown',
		(e) => {
			if (!picker.contains(e.target as Node)) closeMenu(false);
		},
		true
	);

	let hideTimer: ReturnType<typeof setTimeout> | undefined;
	function show(ms: number) {
		tools.classList.add('show');
		clearTimeout(hideTimer);
		if (ms) hideTimer = setTimeout(() => tools.classList.remove('show'), ms);
	}
	// Touch devices keep the toolbar visible; mice reveal it near the top edge.
	const touch = matchMedia('(pointer: coarse)').matches;
	if (touch) {
		tools.classList.add('touch');
		show(0);
	} else
		document.addEventListener('pointermove', (e) => {
			if (
				e.pointerType === 'mouse' &&
				e.clientY < 48 &&
				e.clientX > window.innerWidth - 160
			)
				show(2500);
		});
	// Clicks on the toolbar must not reach the desktop surface underneath.
	for (const b of tools.querySelectorAll('button, [role="listbox"]'))
		b.addEventListener('pointerdown', (e) => e.stopPropagation());

	kbBtn.addEventListener('click', () => {
		if (document.activeElement === kbd && kbd.dataset.soft) {
			kbd.blur();
			delete kbd.dataset.soft;
		} else {
			kbd.dataset.soft = '1';
			kbd.focus({ preventScroll: true });
		}
	});

	// Trackpad mode: only touch devices show its button, and read-only viewers send
	// no input to move a cursor with.
	let trackpad = false;
	try {
		trackpad = localStorage.getItem(TRACKPAD_KEY) === '1';
	} catch {}
	function applyTrackpad() {
		const on = trackpad && touch && !app.readOnly;
		tpBtn.classList.toggle('on', on);
		tpBtn.setAttribute('aria-pressed', String(on));
		onTrackpad(on);
	}
	tpBtn.addEventListener('click', () => {
		trackpad = !trackpad;
		try {
			if (trackpad) localStorage.setItem(TRACKPAD_KEY, '1');
			else localStorage.removeItem(TRACKPAD_KEY);
		} catch {}
		applyTrackpad();
	});
	applyTrackpad();

	if (!document.fullscreenEnabled) fsBtn.style.display = 'none';
	fsBtn.addEventListener('click', () => {
		if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
		else
			document.documentElement
				.requestFullscreen({ navigationUI: 'hide' })
				.catch(() => {});
	});
	// In fullscreen, capture browser shortcuts (Cmd/Ctrl+W, T, L, ...) for the desktop.
	// Keyboard Lock is Chromium-only and only allowed for the top-level page (an
	// embedding page has to lock on our behalf); hold Esc to leave fullscreen.
	document.addEventListener('fullscreenchange', () => {
		const kl = navigator.keyboard;
		if (document.fullscreenElement) kl?.lock().catch(() => {});
		else kl?.unlock();
		fsBtn.classList.toggle('on', !!document.fullscreenElement);
		onFullscreenChange();
	});
	// Called with each hello.
	return (info: Hello['displays']) => {
		applyTrackpad();
		closeMenu(false);
		picker.hidden = !info || info.available.length < 2 || app.readOnly;
		displayBtn.disabled = false;
		selected = info?.selected ?? 0;
		const available = info?.available ?? [];
		displayLabel.textContent =
			available.find((d) => d.id === selected)?.name ?? '';
		menu.replaceChildren(
			...available.map((d) => {
				const option = document.createElement('div');
				option.className = 'option';
				option.setAttribute('role', 'option');
				option.setAttribute('aria-selected', String(d.id === selected));
				option.tabIndex = -1;
				option.dataset.id = String(d.id);
				option.textContent = d.name;
				return option;
			})
		);
	};
}
