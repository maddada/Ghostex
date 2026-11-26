// Files panel: browse the shared folder and download (folders as zips), upload files
// and folders with the buttons or by dropping them anywhere on the page. Uses the
// server's /files HTTP API.
import type { FilesConfig } from '../protocol';
import type { App } from './app';
import { el } from './app';

interface Entry {
	name: string;
	dir: boolean;
	size: number;
	/** Modification time, ms since the epoch. */
	mtime: number;
}
interface Listing {
	path: string;
	entries: Entry[];
}
/** A file to upload, with its folder inside the upload ('' for a plain file). */
interface Picked {
	f: File;
	sub: string;
}
/** A dropped or picked folder: one progress row for all its files. */
interface Group {
	name: string;
	row: HTMLElement;
	files: number;
	bytes: number;
	done: number;
	sent: number;
	failed: number;
}
interface Upload {
	f: File;
	sub: string;
	dir: string;
	row: HTMLElement;
	group?: Group;
}

// Icons from the Tembo UI package's icon set (16px with 2px padding).
const svg = (d: string) =>
	`<svg class="i" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${d}</svg>`;
const ICON_DIR = svg(
	'<path d="M3 6V17C3 18.1046 3.89543 19 5 19H19C20.1046 19 21 18.1046 21 17V9C21 7.89543 20.1046 7 19 7H12.5352C12.2008 7 11.8886 6.8329 11.7031 6.5547L10.5937 4.8906C10.2228 4.3342 9.59834 4 8.92963 4H5C3.89543 4 3 4.89543 3 6Z"/>'
);
const ICON_FILE = svg(
	'<path d="M12.1716 3H7C5.89543 3 5 3.89543 5 5V19C5 20.1046 5.89543 21 7 21H17C18.1046 21 19 20.1046 19 19V9.82843C19 9.29799 18.7893 8.78929 18.4142 8.41421L13.5858 3.58579C13.2107 3.21071 12.702 3 12.1716 3Z"/><path d="M13 3.5V7C13 8.10457 13.8954 9 15 9H18.5"/>'
);
const ICON_DOWNLOAD = svg(
	'<path d="M12 4V15"/><path d="M7 10.5L12 15.5L17 10.5"/><path d="M5 20H19"/>'
);
const ICON_CHEVRON = svg(
	'<path d="M10 16L13.2929 12.7071C13.6834 12.3166 13.6834 11.6834 13.2929 11.2929L10 8"/>'
);

// The password from a sign-in link (/?token=...), passed on as the socket gets it.
const token = new URLSearchParams(location.search).get('token');
const api = (op: string, q: Record<string, string>) =>
	`files/${op}?${new URLSearchParams(token ? { ...q, token } : q)}`;
const label = (p: string) => (p ? p.split('/').pop()! : 'Home');
const join = (a: string, b: string) => (a ? a + '/' : '') + b;
const size = (n: number) =>
	n < 1024
		? `${n} B`
		: n < 1 << 20
			? `${(n / 1024).toFixed(1)} KB`
			: n < 1 << 30
				? `${(n / 1048576).toFixed(1)} MB`
				: `${(n / 1073741824).toFixed(2)} GB`;
function when(t: number) {
	const d = new Date(t),
		now = new Date();
	if (d.toDateString() === now.toDateString())
		return d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
	return d.toLocaleDateString([], {
		month: 'short',
		day: 'numeric',
		year: d.getFullYear() === now.getFullYear() ? undefined : 'numeric',
	});
}

function status(row: HTMLElement, text: string, cls?: 'done' | 'err') {
	row.querySelector('.f-meta')!.textContent = text;
	if (cls) row.classList.add(cls);
}
const hasFiles = (e: DragEvent) =>
	!!e.dataTransfer && [...e.dataTransfer.types].includes('Files');

/** Every entry of a dropped folder: readEntries returns at most ~100 per call. */
async function readAll(
	reader: FileSystemDirectoryReader
): Promise<FileSystemEntry[]> {
	const batch = await new Promise<FileSystemEntry[]>((res, rej) =>
		reader.readEntries(res, rej)
	).catch(() => [] as FileSystemEntry[]);
	return batch.length ? [...batch, ...(await readAll(reader))] : [];
}

/** All files under a dropped entry, with their folder paths relative to its parent. */
async function walk(entry: FileSystemEntry): Promise<Picked[]> {
	if (entry.isFile) {
		const f = await new Promise<File>((res, rej) =>
			(entry as FileSystemFileEntry).file(res, rej)
		).catch(() => null);
		const sub = entry.fullPath
			.split('/')
			.filter(Boolean)
			.slice(0, -1)
			.join('/');
		return f ? [{ f, sub }] : [];
	}
	if (!entry.isDirectory) return [];
	const children = await readAll(
		(entry as FileSystemDirectoryEntry).createReader()
	);
	return (await Promise.all(children.map(walk))).flat();
}

export function createFiles(app: App) {
	const panel = el('files');
	const list = el('files-list');
	const crumbs = el('files-crumbs');
	const uploads = el('files-uploads');
	const input = el<HTMLInputElement>('files-input');
	const dirInput = el<HTMLInputElement>('files-input-dir');
	const drop = el('drop');
	const dropDir = el('drop-dir');
	const uploadBtn = el('files-upload');
	const uploadDirBtn = el('files-upload-dir');
	const footer = panel.querySelector('footer')!;

	let cfg: FilesConfig | null = null;
	let cwd = '';
	let seq = 0;
	let shown = '';
	let poll: ReturnType<typeof setInterval> | undefined;

	function empty(title: string, text: string) {
		list.innerHTML = `<div class="f-empty">${ICON_DIR}<b></b><span></span></div>`;
		list.querySelector('b')!.textContent = title;
		list.querySelector('.f-empty span')!.textContent = text;
	}

	function setup(f: FilesConfig | undefined) {
		cfg = f && (f.upload || f.download) ? f : null;
		app.tools.classList.toggle('files', !!cfg);
		if (!cfg) {
			close();
			return;
		}
		if (!cwd) cwd = cfg.dir || '';
		dropDir.textContent = label(cwd);
		uploadBtn.style.display = cfg.upload ? '' : 'none';
		uploadDirBtn.style.display = cfg.upload ? '' : 'none';
		footer.textContent = [
			cfg.upload &&
				'Drop files or folders anywhere on the screen to upload them here.',
			cfg.download && 'Click a file to download it; folders download as zips.',
		]
			.filter(Boolean)
			.join(' ');
	}
	function open() {
		if (!cfg) return;
		panel.classList.add('open');
		load(cwd);
		clearInterval(poll);
		// Pick up files that appear in the VM (e.g. browser downloads) while open.
		poll = setInterval(() => {
			if (!document.hidden) load(cwd, true);
		}, 3000);
	}
	function close() {
		panel.classList.remove('open');
		clearInterval(poll);
		poll = undefined;
	}

	async function load(path: string, quiet = false) {
		const n = ++seq;
		let r: Response | null = null;
		let d: (Listing & { error?: string }) | null = null;
		try {
			r = await fetch(api('list', { path }), { cache: 'no-store' });
			d = await r.json();
		} catch {}
		if (n !== seq) return;
		if (!r || !r.ok || !d) {
			if (quiet) return;
			shown = '';
			empty(
				'Can’t open this folder',
				d?.error || 'The computer did not respond. Try again in a moment.'
			);
			return;
		}
		cwd = d.path;
		dropDir.textContent = label(cwd);
		const key = cwd + '\n' + JSON.stringify(d.entries);
		if (key !== shown) {
			shown = key;
			render(d.entries);
		}
	}

	function render(entries: Entry[]) {
		crumbs.innerHTML = '';
		const parts = cwd ? cwd.split('/') : [];
		const crumb = (text: string, path: string) => {
			const a = document.createElement('a');
			a.textContent = text;
			a.addEventListener('click', () => load(path));
			crumbs.append(a);
		};
		crumb('Home', '');
		parts.forEach((p, i) => {
			crumbs.insertAdjacentHTML('beforeend', ICON_CHEVRON);
			crumb(p, parts.slice(0, i + 1).join('/'));
		});
		const top = list.scrollTop;
		list.innerHTML = '';
		// Folders by name, then files newest first (downloads land at the top).
		entries.sort(
			(a, b) =>
				Number(b.dir) - Number(a.dir) ||
				(a.dir
					? a.name.localeCompare(b.name)
					: b.mtime - a.mtime || a.name.localeCompare(b.name))
		);
		if (!entries.length)
			empty(
				'No files yet',
				cfg?.upload
					? 'Drop files anywhere on the screen, or use Upload, to add them here.'
					: 'Files in this folder will show up here.'
			);
		for (const en of entries) {
			const row = document.createElement('div');
			row.className = 'f-row' + (en.dir ? ' dir' : '');
			row.innerHTML = `<span class="f-ico">${en.dir ? ICON_DIR : ICON_FILE}</span><span class="f-name"></span><span class="f-meta"></span>`;
			if (en.dir && cfg?.download) {
				const b = document.createElement('button');
				b.className = 'ib sm f-dl';
				b.type = 'button';
				b.title = `Download ${en.name} as a zip`;
				b.setAttribute('aria-label', b.title);
				b.innerHTML = ICON_DOWNLOAD;
				b.addEventListener('click', (e) => {
					e.stopPropagation();
					download(join(cwd, en.name));
				});
				row.querySelector('.f-meta')!.before(b);
			}
			row.querySelector('.f-name')!.textContent = en.name;
			row.querySelector('.f-meta')!.textContent = en.dir
				? when(en.mtime)
				: `${size(en.size)} · ${when(en.mtime)}`;
			row.title = en.dir
				? `Open ${en.name}`
				: cfg?.download
					? `Download ${en.name}`
					: en.name;
			const path = join(cwd, en.name);
			if (en.dir) row.addEventListener('click', () => load(path));
			else if (cfg?.download)
				row.addEventListener('click', () => download(path));
			else row.style.cursor = 'default';
			list.append(row);
		}
		list.scrollTop = top;
	}

	function download(path: string) {
		const a = document.createElement('a');
		a.href = api('download', { path });
		a.download = '';
		document.body.append(a);
		a.click();
		a.remove();
	}

	// Uploads run one at a time, each to the folder that was open when it was added.
	// A folder gets one row for all of its files.
	const queue: Upload[] = [];
	let busy = false;
	function upload(picked: Picked[]) {
		if (!cfg?.upload) return;
		open();
		const groups = new Map<string, Group>();
		for (const { f, sub } of picked) {
			const top = sub.split('/')[0];
			let group = top ? groups.get(top) : undefined;
			if (top && !group) {
				group = {
					name: top,
					row: newRow(top),
					files: 0,
					bytes: 0,
					done: 0,
					sent: 0,
					failed: 0,
				};
				groups.set(top, group);
			}
			if (group) {
				group.files++;
				group.bytes += f.size;
			}
			queue.push({
				f,
				sub,
				dir: cwd,
				row: group?.row ?? newRow(f.name),
				group,
			});
		}
		for (const g of groups.values()) groupStatus(g);
		pump();
	}
	function newRow(name: string) {
		const row = document.createElement('div');
		row.className = 'f-up';
		row.innerHTML =
			'<div class="f-up-top"><span class="f-name"></span><span class="f-meta">Waiting…</span></div><div class="f-bar"><i></i></div>';
		row.querySelector('.f-name')!.textContent = name;
		uploads.prepend(row);
		return row;
	}
	function groupStatus(g: Group, current = 0) {
		const pct = g.bytes ? ((g.sent + current) / g.bytes) * 100 : 100;
		g.row.querySelector<HTMLElement>('.f-bar i')!.style.width = `${pct}%`;
		const n = `${g.done} of ${g.files} file${g.files === 1 ? '' : 's'}`;
		status(g.row, `${n} · ${Math.floor(pct)}% of ${size(g.bytes)}`);
	}
	function groupFinished(g: Group, dir: string) {
		if (g.done + g.failed < g.files) return;
		if (g.failed)
			status(g.row, `${g.failed} of ${g.files} files failed`, 'err');
		else {
			status(g.row, `Uploaded to ${label(dir)}`, 'done');
			setTimeout(() => g.row.remove(), 6000);
		}
	}
	function pump() {
		const u = busy ? undefined : queue.shift();
		if (!u) return;
		busy = true;
		const bar = u.row.querySelector<HTMLElement>('.f-bar i')!;
		const xhr = new XMLHttpRequest();
		const finish = () => {
			busy = false;
			pump();
		};
		const q: Record<string, string> = { path: u.dir, name: u.f.name };
		if (u.sub) q.sub = u.sub;
		xhr.open('PUT', api('upload', q));
		// Required by the server: browsers only send custom headers cross-origin after a
		// CORS preflight, which it never answers, so other sites can't upload.
		xhr.setRequestHeader('X-Voxl-Upload', '1');
		const g = u.group;
		xhr.upload.addEventListener('progress', (e) => {
			if (!e.lengthComputable) return;
			if (g) return groupStatus(g, e.loaded);
			bar.style.width = `${(e.loaded / e.total) * 100}%`;
			status(
				u.row,
				`${Math.floor((e.loaded / e.total) * 100)}% of ${size(e.total)}`
			);
		});
		xhr.addEventListener('load', () => {
			let d: { path?: string; error?: string } = {};
			try {
				d = JSON.parse(xhr.responseText);
			} catch {}
			if (g) {
				if (xhr.status === 200) {
					g.done++;
					g.sent += u.f.size;
				} else g.failed++;
				groupStatus(g);
				groupFinished(g, u.dir);
				if (xhr.status === 200 && u.dir === cwd) load(cwd, true);
			} else if (xhr.status === 200) {
				const saved = (d.path || '').split('/').pop();
				if (saved && saved !== u.f.name)
					u.row.querySelector('.f-name')!.textContent = saved;
				status(u.row, `Uploaded to ${label(u.dir)}`, 'done');
				if (u.dir === cwd) load(cwd, true);
				setTimeout(() => u.row.remove(), 6000);
			} else status(u.row, d.error || `Failed (${xhr.status})`, 'err');
			finish();
		});
		xhr.addEventListener('error', () => {
			if (g) {
				g.failed++;
				groupFinished(g, u.dir);
			} else status(u.row, 'Connection lost', 'err');
			finish();
		});
		xhr.send(u.f);
	}

	el('files-btn').addEventListener('click', () =>
		panel.classList.contains('open') ? close() : open()
	);
	el('files-close').addEventListener('click', close);
	el('files-refresh').addEventListener('click', () => load(cwd));
	uploadBtn.addEventListener('click', () => input.click());
	uploadDirBtn.addEventListener('click', () => dirInput.click());
	input.addEventListener('change', () => {
		upload([...(input.files ?? [])].map((f) => ({ f, sub: '' })));
		input.value = '';
	});
	dirInput.addEventListener('change', () => {
		// webkitRelativePath is "Folder/sub/file.txt".
		upload(
			[...(dirInput.files ?? [])].map((f) => ({
				f,
				sub: f.webkitRelativePath.split('/').slice(0, -1).join('/'),
			}))
		);
		dirInput.value = '';
	});
	// Esc closes the panel unless the remote screen has focus (it goes to the desktop then).
	document.addEventListener('keydown', (e) => {
		if (e.key === 'Escape' && e.target !== app.kbd && e.target !== app.surface)
			close();
	});

	// Always swallow dropped files: otherwise the browser navigates away to open them.
	let depth = 0;
	window.addEventListener('dragenter', (e) => {
		if (!hasFiles(e)) return;
		e.preventDefault();
		if (cfg?.upload && ++depth === 1) drop.classList.add('show');
	});
	window.addEventListener('dragover', (e) => {
		if (!hasFiles(e)) return;
		e.preventDefault();
		e.dataTransfer!.dropEffect = cfg?.upload ? 'copy' : 'none';
	});
	window.addEventListener('dragleave', (e) => {
		if (hasFiles(e) && --depth <= 0) {
			depth = 0;
			drop.classList.remove('show');
		}
	});
	window.addEventListener('drop', (e) => {
		if (!hasFiles(e)) return;
		e.preventDefault();
		depth = 0;
		drop.classList.remove('show');
		if (!cfg?.upload) return;
		// Entries must be taken during the event; the walk can finish afterwards.
		const entries: FileSystemEntry[] = [];
		const plain: Picked[] = [];
		for (const it of e.dataTransfer!.items) {
			if (it.kind !== 'file') continue;
			const entry = it.webkitGetAsEntry?.();
			if (entry?.isDirectory) entries.push(entry);
			else {
				const f = it.getAsFile();
				if (f) plain.push({ f, sub: '' });
			}
		}
		if (!entries.length) return upload(plain);
		void Promise.all(entries.map(walk)).then((found) =>
			upload([...plain, ...found.flat()])
		);
	});

	return { setup };
}
