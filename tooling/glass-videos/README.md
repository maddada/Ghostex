# Glass video library

Ghostex can play a muted, looping, blurred video behind its window glass (Settings → Theme → Transparency →
More transparency options → Video). Besides macOS aerials and the user's own files, it offers the Ghostex video
library: short loops made for the glass, listed in a catalogue and downloaded only when the user presses Get.

- App side: `apps/desktop/src/app/helpers/glass_video_library.rs` (catalogue, downloads, `library:<id>`),
  `apps/desktop/src/app/glass_video_library_dispatch.rs` (settings bridge).
- Settings side: `packages/core-ui/settings-modal/glass-video-gallery.tsx`, `use-glass-video-library.ts`,
  `packages/shared/glass-video-library.ts`.
- The video shipped inside the app: `media/glass-videos/` (`bundled.json` + files), staged into
  `Contents/Resources/glass-videos` by `apps/desktop/scripts/build-macos-app.sh`.

## Video recipe

`publish.ts` turns any clip into a library video:

- 240p, 24 fps, H.264 around 200 kbps, no audio, `+faststart`.
- A seamless loop: the segment's last second fades into its first, then whole loops repeat up to the target length
  (5 minutes by default, so a 5-minute video is about 7 MB).
- A poster (`<id>.jpg`) and a 3 second animated preview (`<id>.webp`, 12 fps, 120 px tall).
- sha256 and size in the catalogue; the app refuses a download that does not match.

Needs `ffmpeg`, `ffprobe` and `img2webp` (`brew install ffmpeg webp`).

## Make a video

```bash
bun tooling/glass-videos/publish.ts path/to/clip.mp4 --id ink-bloom --name "Ink Bloom" \
  --tags calm,colourful --tone dark
```

Files and `catalogue.json` go to `build/glass-videos/` (git-ignored). `--tone` is only a hint shown on the card.
`--added YYYY-MM-DD` sets the date for the 7-day "New" badge (default today).

The bundled placeholder was made with the procedural source and a 60 second loop to keep the app small:

```bash
bun tooling/glass-videos/publish.ts calm-source tmp/glass-videos/calm-source.mp4 --seconds 61
bun tooling/glass-videos/publish.ts tmp/glass-videos/calm-source.mp4 --id calm-drift --name "Calm Drift" \
  --tags calm,soft --tone any --duration 60 --bundled
```

Replace it by running the second command with another source (keep it under ~3 MB).

## Hosting (not set up yet)

The app reads `https://github.com/maddada/ghostex-glass-videos/releases/download/library/catalogue.json`
(`GLASS_VIDEO_CATALOGUE_URL` in `glass_video_library.rs`). Until that release exists the library shows only the
bundled video, and a catalogue fetch failure falls back to the last saved copy.

When approved, create the repo and its rolling `library` release once:

```bash
gh repo create maddada/ghostex-glass-videos --public --description "Ghostex glass video library"
gh release create library --repo maddada/ghostex-glass-videos --title "Glass video library" \
  --notes "Videos for Ghostex's window glass. catalogue.json lists them."
```

Then publish each video with `--upload`, which also re-uploads the updated `catalogue.json`:

```bash
bun tooling/glass-videos/publish.ts path/to/clip.mp4 --id ink-bloom --name "Ink Bloom" --tags calm --upload
```

Always publish from the same `build/glass-videos/catalogue.json` (or download the current one from the release into
it first) so earlier entries are kept.
