# Over-the-air updates for the phone app

The phone app (`apps/mobile/app`, Expo) downloads new JavaScript from `https://ota.ghostex.dev` instead of waiting for a new APK, TestFlight build or store release. This folder is that server: a Cloudflare Worker (`src/worker.js`) with one R2 bucket, speaking the [Expo Updates protocol v1](https://docs.expo.dev/technical-specs/expo-updates-1/) that the app's `expo-updates` client uses.

## How a release reaches phones

1. The Android release job (`.github/workflows/release-gpui-android.yml`) builds the APK and then runs `tooling/release-mobile/ota-export.sh`, which exports the same release's JavaScript for Android and iOS together with its runtime version.
2. After the Android stage of the GitHub release is published, `release-gpui.yml`'s `publish_ota` job runs `tooling/release-mobile/ota-publish.mjs publish` (through `.github/workflows/release-mobile-ota-publish.yml`). It signs each manifest with the private key and uploads the files and the signed manifest here. Prereleases publish nothing.
3. A phone checks at every launch and when the app comes back to the foreground (at most every 30 minutes). It downloads the update in the background and runs it at the next launch, or right away when the user taps **Restart to update** in Settings › Updates.

An update only reaches a phone whose app has the **same runtime version**: a hash of everything that ends up as native code (the app's Kotlin and Swift, config plugins, native dependencies, the Rust chat core and the Go tailcat bridge; `apps/mobile/app/scripts/ota-runtime-version.js` has the exact list). A release that changes any of those needs a new binary, and phones on the old binary keep their last update until they install it. On the GitHub APK, the in-app updater offers the new APK only in that case. It asks `/v1/status` whether the newest release already reached this binary over the air.

The Google Play bundle has its own runtime version (it lacks the APK installer permission) and no update is published for it yet.

## Security

- CI signs every manifest and directive with an RSA key. The app carries only the certificate (`apps/mobile/app/certs/certificate.pem`, valid until 2056) and refuses anything unsigned or signed by another key, before it downloads a single file.
- The Worker never sees the private key. The upload token lets CI write to the bucket, but a stolen token cannot make phones run anything without the key.
- `ota-publish.mjs` checks every signature against the app's certificate before it uploads, so a wrong key in the secrets fails the job instead of reaching phones.

Keep the private key in two places only: the `GHOSTEX_OTA_SIGNING_KEY` GitHub secret and your password manager. If it is lost, updates stop until every phone installs a binary with a new certificate. If it leaks together with the upload token, someone could ship JavaScript to every phone, so rotate it then (new key and certificate with `npx expo-updates codesigning:generate`, then a new binary).

## One-time setup

From this folder, with a Cloudflare account that owns the `ghostex.dev` zone:

```sh
bunx wrangler login
bunx wrangler r2 bucket create ghostex-ota
bunx wrangler deploy                      # creates the ota.ghostex.dev custom domain too
TOKEN="$(openssl rand -hex 32)"
printf '%s' "$TOKEN" | bunx wrangler secret put UPLOAD_TOKEN
printf '%s' "$TOKEN" | gh secret set GHOSTEX_OTA_UPLOAD_TOKEN -R maddada/Ghostex
gh secret set GHOSTEX_OTA_SIGNING_KEY -R maddada/Ghostex < ~/.config/ghostex-release/ota-signing/private-key.pem
```

Until both GitHub secrets exist, `publish_ota` prints a notice and phones get releases only as new binaries.

## Day to day

Nothing: every non-prerelease Ghostex release that rebuilt the Android app publishes its update. To see what a runtime version is served:

```sh
curl https://ota.ghostex.dev/v1/status/production/android/<runtime-version>
node apps/mobile/app/scripts/ota-runtime-version.js android   # this checkout's runtime version
```

Settings › Updates on the phone shows the running version and the app build it runs on.

## Phone-only fixes without a release

`bun run release:ota` (add `-- --dry-run` to preview) publishes main's phone JavaScript over the air with no Ghostex release: `.github/workflows/release-mobile-ota.yml` exports it with `ota-export.sh` and publishes through `release-mobile-ota-publish.yml`, the same workflow the release's `publish_ota` job calls. It carries the newest release's version and refuses when a platform's runtime version has no update on this server yet, because then the native side changed and only a new app build reaches phones. Details: `tooling/release-mobile/ota-release-lib.mjs` and the ghostex-release skill.

## When an update is bad

Run these from the repo root with `GHOSTEX_OTA_SIGNING_KEY_FILE` (or `GHOSTEX_OTA_SIGNING_KEY`) and `GHOSTEX_OTA_UPLOAD_TOKEN` set:

```sh
# Phones on that runtime drop their downloaded update and run the code inside the app.
node tooling/release-mobile/ota-publish.mjs rollback-to-embedded --platform android --runtime <rv>

# Or serve an earlier update again (it gets a new id and date, so phones take it).
node tooling/release-mobile/ota-publish.mjs history --platform android --runtime <rv>
node tooling/release-mobile/ota-publish.mjs republish --platform android --runtime <rv> --from <history-name>
```

expo-updates also protects phones on its own: an update that crashes on launch is rolled back to the previous one.

## Testing locally

```sh
cd tooling/release-mobile/ota-server
bunx wrangler dev --local --port 8787 --var UPLOAD_TOKEN:<any 32+ characters>
```

Build the app with `GHOSTEX_RELEASE_VERSION=<version> GHOSTEX_OTA_SERVER=http://127.0.0.1:8787` (a release build; the Android emulator reaches it after `adb reverse tcp:8787 tcp:8787`). Export with the same `GHOSTEX_OTA_SERVER` and a higher version through `ota-export.sh`, then publish with `GHOSTEX_OTA_SERVER=http://127.0.0.1:8787 node tooling/release-mobile/ota-publish.mjs publish <export-dir>`.
