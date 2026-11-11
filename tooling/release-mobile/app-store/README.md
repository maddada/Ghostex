# App Store listing for the iOS app

Everything App Store Connect shows for `com.maddada.ghostex.ios`, kept here so a release is a workflow run instead of clicking through App Store Connect. `tooling/release-mobile/asc-app-store.mjs` reads these files; the workflow **Release mobile iOS to the App Store** (`.github/workflows/release-mobile-ios-app-store.yml`) runs it with the release API key.

| File                              | What it holds                                                                                                                                 |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `app-store.json`                  | Name, subtitle, URLs, promotional text, keywords, copyright, categories, content rights, release type, age rating answers, screenshot folders |
| `description.en-US.txt`           | The App Store description                                                                                                                     |
| `review-notes.txt`                | Notes for App Review. The in-app demo steps; no placeholders to fill                                         |
| `screenshots/iphone-6.9/NN-*.png` | iPhone screenshots, 1320 x 2868, uploaded in file-name order                                                                                  |
| `screenshots/ipad-13/NN-*.png`    | iPad screenshots, 2064 x 2752 (required while the app supports iPad)                                                                          |

`node tooling/release-mobile/asc-app-store.mjs validate` checks the lengths, the screenshot sizes and that no screenshot has an alpha channel, without the API key.

## Releasing a version

1. Upload the build: run **Release mobile iOS to TestFlight** with the version (for example `10.27.0`). Try it on TestFlight.
2. Run **Release mobile iOS to the App Store** with mode `check` to see what App Store Connect has and what is missing.
3. Run it with mode `prepare` and the same version (`whats_new` is required from the second App Store version on; Apple does not take it on the first). It creates the version, writes the listing and screenshots, attaches the build once Apple has processed it, and fills App Review Information. `dry_run` shows what it would change.
4. Look the version over in App Store Connect.
5. Run it with mode `submit`. Review usually takes one to two days. The release type in `app-store.json` is `AFTER_APPROVAL`, so the version goes to the App Store by itself once Apple approves it. For a version that was submitted as `MANUAL`, run mode `release`: it switches the version to release after approval while it is in review, and releases it if Apple already approved it.

## Secrets the workflow reads

Already set: `APPLE_ASC_ISSUER_ID`, `APPLE_ASC_KEY_ID`, `APPLE_ASC_KEY_BASE64`.

To add (`gh secret set <NAME> -R maddada/Ghostex`):

| Secret                                                              | Value                                                                                                      |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `APPLE_REVIEW_CONTACT_FIRST_NAME`, `APPLE_REVIEW_CONTACT_LAST_NAME` | Who App Review contacts                                                                                    |
| `APPLE_REVIEW_CONTACT_PHONE`                                        | Phone with country code, for example `+971 50 123 4567`                                                    |
| `APPLE_REVIEW_CONTACT_EMAIL`                                        | Email App Review writes to                                                                                 |
| `APPLE_REVIEW_DEMO_USERNAME`, `APPLE_REVIEW_DEMO_PASSWORD`          | Optional sign-in. Leave unset for the in-app demo                                                         |
| `APPLE_REVIEW_DEMO_ADDRESS`, `APPLE_REVIEW_DEMO_SSH_PORT`           | Optional. Passed through and ignored. The notes have no SSH placeholders                                  |

The logs of this public repository never print these values. `prepare` writes App Review Information only when all four contact values are set. Without them it leaves App Review Information as it is in App Store Connect, so it can also be typed there by hand. When the demo username or password is unset, that write sets `demoAccountRequired` to false and sends no username or password. A sign-in already stored in App Store Connect stays until you clear it there. Set both secrets only if App Review should show a sign-in. The in-app demo does not use one.

## The demo for App Review

Reviewers use the demo built into the app. Launch it and tap **Try a demo computer** (or **Connect your computer**, then the **Try a demo computer** card, then **Open demo**). The Sessions list for **Demo computer** opens. The steps are in `review-notes.txt`. The demo runs fully offline, keeps its data in memory, and needs no account, no SSH server, and no network. Remove it from Settings > Demo computer > Remove demo computer.

`demo-computer/` builds local Docker containers used to take the App Store screenshots on a simulator (`127.0.0.1:2222` and `:2223`). Its README has the steps. App Review uses the in-app demo. Keep the containers' SSH login on this computer: the local seeding step copies an agent login into those throwaway containers only, and that copy is removed when the seed finishes.

## One-time steps in App Store Connect (no API)

- **App Privacy** (App Store Connect → the app → App Privacy). Answer _Yes, we collect data_, then:
  - **Usage Data → Product Interaction** and **Diagnostics → Other Diagnostic Data**: linked to the user, not used for tracking, purpose _Analytics_. Ghostex on the paired computer reports that a phone connected, with the phone's OS release and app version, to PostHog under a one-way hash of the user's agent account id (`docs/important/ANALYTICS.md`, `src/analytics/clientHello.ts` in the phone app).
  - Nothing else. Send Bug / Feedback posts a public GitHub issue the user writes and reviews, with their GitHub username on the form, each time they choose to; that meets Apple's "optional to disclose" criteria for feedback forms. Conversations, code and files go only to the user's own computer.
  - Privacy policy URL: `https://ghostex.dev/privacy-policy` (set by `prepare` too).
  - The app's privacy manifest declares the same two data types (`ios.privacyManifests` in `apps/mobile/app/app.json`); keep the two in step.
- **Pricing and Availability**: price Free; choose the countries.
- **EU Digital Services Act trader status** (Business → the account's compliance section): declare whether you are a trader. A trader's address, phone and email are shown on the EU App Store. Without an answer the app is not offered in the EU.
- **Export compliance and France**: every build's Info.plist answers it (`ITSAppUsesNonExemptEncryption: false` in `apps/mobile/app/app.json`). The app ships its own industry-standard encryption (libssh2/OpenSSL for SSH, WireGuard in the Easy Connect bridge), not only Apple's. Apple's table ([Export compliance documentation for encryption](https://developer.apple.com/help/app-store-connect/reference/app-information/export-compliance-documentation-for-encryption)) asks no US documents for that, only a French encryption declaration when the app is offered on the App Store in France. So either leave France out of the countries, or file the declaration with ANSSI, upload it under App Encryption Documentation, set `ITSAppUsesNonExemptEncryption` to true and add the `ITSEncryptionExportComplianceCode` Apple returns (`asc-testflight-distribute.mjs` then needs to accept `usesNonExemptEncryption: true`; it stops on anything but false today).

## Age rating

`prepare` writes the answers in `app-store.json`. Everything is "none" except **Unrestricted Web Access**, because Web Preview accepts any web address; Apple rates that 16+.
