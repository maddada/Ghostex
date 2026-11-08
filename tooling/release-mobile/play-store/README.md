# Google Play listing for the Android app

Store-listing material for `io.ghostex`, the Play build of the phone app (`GHOSTEX_ANDROID_DISTRIBUTION=play`, `GHOSTEX_ANDROID_SELF_UPDATE=0`). These files are what a person pastes into Play Console. Nothing here is read by `tooling/release-mobile/play-upload.mjs` or the release workflows.

The version in this folder is **11.0.0** (GitHub release v11.0.0).

| File | What it holds |
| --- | --- |
| `listing.json` | Name, short description, URLs, category, and where the other files live |
| `description.en-US.txt` | Full description, en-US |
| `release-notes.en-US.txt` | What's new for 11.0.0. Play allows 500 characters. Paste it, or pass it as `GHOSTEX_PLAY_RELEASE_NOTES` |
| `PLAY-CONSOLE.md` | Checklist for the rest of the store setup: permissions, Data safety, content rating, audience |
| `graphics/icon-512.png` | Hi-res icon, 512×512, 32-bit PNG |
| `graphics/feature-graphic.png` | Feature graphic, 1024×500, 24-bit PNG, no alpha |
| `graphics/phone/01`–`07` | Phone screenshots, 1080×1920, 24-bit PNG, no alpha, in file-name order |
| `graphics/compose.py` | Regenerates the graphics and checks the text lengths and image sizes |

`python3 tooling/release-mobile/play-store/graphics/compose.py` rewrites the graphics. `python3 tooling/release-mobile/play-store/graphics/compose.py --check` only checks them. It needs Pillow (`pip install pillow`).

## Screenshots

The phone screenshots are composites of the shared React Native app UI captured for the iOS listing (`tooling/release-mobile/app-store/screenshots/iphone-6.9/`) and real Android OS pixels. The composition crops the iOS status bar, Dynamic Island, and home-indicator area. Its status bar and gesture navigation strip come from a headless Pixel API 35 emulator at 1080×1920. The terminal and Find keyboard regions use dark Gboard captures from that emulator, with Enter and Search action keys respectively. Find and Docs use Android-style back arrows in place of the iOS back controls. The app content inside the frame remains the iOS-origin capture; these composites are not direct Android app screenshots.

The cropped system captures are in `graphics/source/pixel-*.png`. `compose.py` scales them to the phone frame without recreating OS glyphs. The keyboard assets retain their native key geometry: the join removes only an empty band between the keys and navigation controls.

Tablet screenshots are not included. The only tablet captures on hand are iPad screenshots, and those would show the wrong device.

## Text lengths

Play's limits, and the counts for the text in this folder (a trailing newline is not counted):

| Field | Characters | Limit |
| --- | ---: | ---: |
| App name | 25 | 30 |
| Short description | 64 | 80 |
| Full description | 2574 | 4000 |
| Release notes (11.0.0) | 385 | 500 |

The name matches the App Store name, `Ghostex: AI Coding Agents`.
