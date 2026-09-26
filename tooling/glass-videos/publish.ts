#!/usr/bin/env bun
/**
 * Prepares a video for the Ghostex glass video library: a 240p, 24 fps, silent H.264 loop that
 * starts where it ends, plus its poster and a 3 second animated preview, then records it in a
 * catalogue. See tooling/glass-videos/README.md.
 *
 * CDXC:Theming 2026-09-26 WHY:
 * The glass blurs everything by about 60px, so 240p and ~200 kbps cost nothing visible while keeping a 5 minute loop
 * near 7 MB. The loop is made seamless by fading its last second into its first, so the player's jump back to the
 * start cannot be seen. The desktop app reads the catalogue this writes (apps/desktop/src/app/helpers/glass_video_library.rs).
 */
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';

const DEFAULT_REPO = 'maddada/ghostex-glass-videos';
const DEFAULT_RELEASE_TAG = 'library';
const DEFAULT_BASE_URL = `https://github.com/${DEFAULT_REPO}/releases/download/${DEFAULT_RELEASE_TAG}`;
const BUNDLED_DIR = resolve(import.meta.dir, '../../media/glass-videos');

type CatalogueVideo = {
  id: string;
  name: string;
  tags: string[];
  tone: 'dark' | 'light' | 'any';
  added: string;
  durationSeconds: number;
  sizeBytes: number;
  sha256: string;
  video: string;
  poster: string;
  preview: string;
};

type Catalogue = { version: 1; videos: CatalogueVideo[] };

function usage(): never {
  console.log(`Usage:
  bun tooling/glass-videos/publish.ts <source-video> --id <slug> --name "<Name>" [options]
  bun tooling/glass-videos/publish.ts calm-source <out.mp4> [--seconds 61]

Options:
  --tags a,b,c          Short tags shown on the card (calm, colourful, ...)
  --tone dark|light|any Which appearance it suits; only a hint in Settings (default any)
  --added YYYY-MM-DD    Date for the 7-day "New" badge (default today)
  --duration <seconds>  Loop length to aim for (default 300; a shorter source repeats)
  --out <dir>           Where files and catalogue.json go (default build/glass-videos)
  --base-url <url>      Where the files will be hosted (default ${DEFAULT_BASE_URL})
  --bundled             Write into media/glass-videos (shipped inside the app) instead
  --upload              Upload the files and catalogue with \`gh release upload\` (the repo and its
                        "${DEFAULT_RELEASE_TAG}" release must already exist)
  --repo owner/name     Repository for --upload (default ${DEFAULT_REPO})

calm-source writes a procedural, slowly drifting colour field that repeats exactly every 60 s,
used as the placeholder video that ships with the app.`);
  process.exit(1);
}

function run(command: string, args: string[]): string {
  const result = Bun.spawnSync([command, ...args], { stderr: 'pipe', stdout: 'pipe' });
  if (result.exitCode !== 0) {
    throw new Error(`${command} ${args.join(' ')}\n${result.stderr.toString()}`);
  }
  return result.stdout.toString();
}

function option(args: string[], name: string): string | undefined {
  const index = args.indexOf(`--${name}`);
  return index >= 0 ? args[index + 1] : undefined;
}

function durationOf(path: string): number {
  const seconds = Number(
    run('ffprobe', ['-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', path]).trim()
  );
  if (!Number.isFinite(seconds) || seconds <= 0) {
    throw new Error(`Could not read the length of ${path}`);
  }
  return seconds;
}

/** A 60-second-periodic drifting colour field, blurred soft, so the placeholder loops exactly. */
function calmSource(out: string, seconds: number) {
  const wave = (expression: string) => `(0.5+0.5*sin(2*PI*(${expression})))`;
  const red = `40+60*${wave('X/W+T/60')}+40*${wave('Y/H*0.7-2*T/60')}`;
  const green = `30+45*${wave('X/W*0.6-Y/H*0.5+T/60')}`;
  const blue = `80+80*${wave('Y/H+X/W*0.3+T/60')}+30*${wave('X/W-T/30+0.25')}`;
  run('ffmpeg', [
    '-y',
    '-f',
    'lavfi',
    '-i',
    `nullsrc=s=160x90:r=24:d=${seconds}`,
    '-vf',
    `geq=r='${red}':g='${green}':b='${blue}',gblur=sigma=6,scale=426:240:flags=bicubic,gblur=sigma=4,format=yuv420p`,
    '-c:v',
    'libx264',
    '-crf',
    '12',
    '-preset',
    'veryfast',
    out,
  ]);
}

function sha256(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function publish(args: string[]) {
  const source = args[0];
  const id = option(args, 'id');
  const name = option(args, 'name');
  if (!source || !id || !name || !existsSync(source)) {
    usage();
  }
  if (!/^[a-z0-9-]{1,64}$/.test(id)) {
    throw new Error('--id must be a lowercase slug (a-z, 0-9, -)');
  }
  const bundled = args.includes('--bundled');
  const outDir = resolve(bundled ? BUNDLED_DIR : (option(args, 'out') ?? 'build/glass-videos'));
  const baseUrl = option(args, 'base-url') ?? DEFAULT_BASE_URL;
  const target = Number(option(args, 'duration') ?? '300');
  const tone = (option(args, 'tone') ?? 'any') as CatalogueVideo['tone'];
  const tags = (option(args, 'tags') ?? '')
    .split(',')
    .map((tag) => tag.trim())
    .filter(Boolean);
  const added = option(args, 'added') ?? new Date().toISOString().slice(0, 10);
  mkdirSync(outDir, { recursive: true });
  const work = join(outDir, `.work-${id}`);
  rmSync(work, { force: true, recursive: true });
  mkdirSync(work, { recursive: true });

  // 1. One seamless loop clip: the segment's last second fades into its first.
  const sourceSeconds = durationOf(source);
  const segment = Math.min(sourceSeconds, target + 1);
  const clip = segment - 1;
  if (clip < 2) {
    throw new Error('The source must be at least 3 seconds long.');
  }
  const loopClip = join(work, 'loop.mp4');
  run('ffmpeg', [
    '-y',
    '-i',
    source,
    '-filter_complex',
    [
      `[0:v]scale=-2:240:flags=lanczos,fps=24,trim=0:${segment},setpts=PTS-STARTPTS,split[a][b]`,
      `[a]trim=1:${segment},setpts=PTS-STARTPTS[main]`,
      `[b]trim=0:1,setpts=PTS-STARTPTS[head]`,
      `[main][head]xfade=transition=fade:duration=1:offset=${clip - 1},format=yuv420p[out]`,
    ].join(';'),
    '-map',
    '[out]',
    '-an',
    '-c:v',
    'libx264',
    '-crf',
    '10',
    '-preset',
    'veryfast',
    loopClip,
  ]);

  // 2. Repeat whole loops up to the target length, then encode small.
  const repeats = Math.max(1, Math.floor(target / clip));
  const total = repeats * clip;
  const video = join(outDir, `${id}.mp4`);
  run('ffmpeg', [
    '-y',
    '-stream_loop',
    String(repeats - 1),
    '-i',
    loopClip,
    '-t',
    total.toFixed(3),
    '-an',
    '-c:v',
    'libx264',
    '-b:v',
    '200k',
    '-maxrate',
    '260k',
    '-bufsize',
    '400k',
    '-preset',
    'slow',
    '-pix_fmt',
    'yuv420p',
    '-g',
    '48',
    '-movflags',
    '+faststart',
    video,
  ]);

  // 3. Poster and a 3 second animated preview.
  const poster = join(outDir, `${id}.jpg`);
  run('ffmpeg', ['-y', '-ss', '1', '-i', video, '-frames:v', '1', '-q:v', '4', poster]);
  const frames = join(work, 'frames');
  mkdirSync(frames, { recursive: true });
  run('ffmpeg', ['-y', '-t', '3', '-i', video, '-vf', 'fps=12,scale=-2:120', join(frames, 'f%03d.png')]);
  const preview = join(outDir, `${id}.webp`);
  const frameFiles = readdirSync(frames)
    .filter((file) => file.endsWith('.png'))
    .sort()
    .map((file) => join(frames, file));
  run('img2webp', ['-loop', '0', '-lossy', '-q', '60', '-d', '83', ...frameFiles, '-o', preview]);
  rmSync(work, { force: true, recursive: true });

  // 4. Record it.
  const entry: CatalogueVideo = {
    id,
    name,
    tags,
    tone,
    added,
    durationSeconds: Math.round(total),
    sizeBytes: statSync(video).size,
    sha256: sha256(video),
    video: bundled ? basename(video) : `${baseUrl}/${basename(video)}`,
    poster: bundled ? basename(poster) : `${baseUrl}/${basename(poster)}`,
    preview: bundled ? basename(preview) : `${baseUrl}/${basename(preview)}`,
  };
  const cataloguePath = join(outDir, bundled ? 'bundled.json' : 'catalogue.json');
  const catalogue: Catalogue = existsSync(cataloguePath)
    ? (JSON.parse(readFileSync(cataloguePath, 'utf8')) as Catalogue)
    : { version: 1, videos: [] };
  catalogue.videos = [...catalogue.videos.filter((video) => video.id !== id), entry];
  writeFileSync(cataloguePath, `${JSON.stringify(catalogue, null, 2)}\n`);
  console.log(
    `${id}: ${entry.durationSeconds}s, ${(entry.sizeBytes / 1024 / 1024).toFixed(1)} MB, sha256 ${entry.sha256}\n` +
      `wrote ${video}, ${poster}, ${preview} and ${cataloguePath}`
  );

  if (args.includes('--upload')) {
    if (bundled) {
      throw new Error('--upload does not apply to --bundled videos.');
    }
    const repo = option(args, 'repo') ?? DEFAULT_REPO;
    run('gh', [
      'release',
      'upload',
      DEFAULT_RELEASE_TAG,
      video,
      poster,
      preview,
      cataloguePath,
      '--clobber',
      '--repo',
      repo,
    ]);
    console.log(`uploaded to ${repo} release "${DEFAULT_RELEASE_TAG}"`);
  }
}

const args = process.argv.slice(2);
if (args[0] === 'calm-source') {
  const out = args[1];
  if (!out) {
    usage();
  }
  calmSource(out, Number(option(args, 'seconds') ?? '61'));
  console.log(`wrote ${out}`);
} else if (args.length === 0 || args[0] === '--help') {
  usage();
} else {
  publish(args);
}
