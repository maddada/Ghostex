# Play Console checklist for Ghostex (`io.ghostex`)

Fill these in Play Console for the Android app. The listing text and graphics are the other files in this folder. This checklist is the rest: permissions, Data safety, content rating, and audience.

The binary is the Play App Bundle from `tooling/release-mobile/android.sh` with `GHOSTEX_ANDROID_DISTRIBUTION=play` (which sets `GHOSTEX_ANDROID_SELF_UPDATE=0`). For 11.0.0 that script's version code is **110000** (`MAJOR * 10000 + MINOR * 100 + PATCH`). Play updates of the installed app replace the binary. The Play build does not declare `REQUEST_INSTALL_PACKAGES` and has no in-app APK installer. Release builds still check `https://ota.ghostex.dev` for a JavaScript update of the same runtime version (`app.config.js`, `src/updates/overTheAir.ts` in the phone app).

Store listing to paste from this folder:

- App name: `Ghostex: AI Coding Agents`
- Short description: the `shortDescription` in `listing.json`
- Full description: `description.en-US.txt`
- Release notes: `release-notes.en-US.txt`
- Icon, feature graphic, and the seven phone screenshots in `graphics/`
- Privacy policy: `https://ghostex.dev/privacy-policy`
- Website: `https://ghostex.dev`
- Email and phone: the support contact already used for the App Store

App access: the app shows an empty setup screen until it is paired with a computer running Ghostex. Give the reviewer a computer that stays online, the same way the App Store review notes do (`tooling/release-mobile/app-store/README.md`), and tell them to use **Use Tailscale → Or type the details** with the SSH address, username, and password. Easy Connect codes expire.

Pricing: Free. Countries: the same set as the rest of Ghostex, including whatever you decided about France and encryption for the iOS app. The phone uses its own SSH and WireGuard, not only the platform's TLS.

Ads: No. In-app products: none. The app does not use the advertising ID.

Category: **Productivity**. Play has no Developer tools category. Productivity matches the App Store secondary category (`PRODUCTIVITY`); Tools is the other reasonable choice.

## Permissions

This list is **derived**, not read from a built Play AAB. This environment has no Android SDK, release keystore, or staged native libraries, so `expo prebuild` and `bundleRelease` were not run. The list is the union of:

- `modules/ghostex-native/android/src/main/AndroidManifest.xml` in the phone app (pinned submodule `apps/mobile/app`)
- the Expo config plugins in `app.json` / `app.config.js` with `GHOSTEX_ANDROID_SELF_UPDATE=0`
- the Android manifests of the Expo libraries the app depends on (camera, image picker, file system, notifications, updates, haptics, speech recognition) and of `firebase-messaging` 25.0.1, which `expo-notifications` links

After the first Play AAB exists, confirm with `aapt dump permissions` on the universal APK that `android.sh` already builds for its own checks, and correct this list if a transitive manifest adds a row. In particular, look for `com.google.android.gms.permission.AD_ID`. It was not in the manifests inspected. If a Play services library merges it, remove it (`blockedPermissions`) and answer Play's advertising-ID question **No**. The app does not read the advertising ID.

### Removed on the Play build

`app.config.js` writes these with `tools:node="remove"` so a library cannot merge them back in:

| Permission | Why it is absent |
| --- | --- |
| `android.permission.REQUEST_INSTALL_PACKAGES` | The GitHub APK's self-updater. Play rejects it. Off when `GHOSTEX_ANDROID_SELF_UPDATE=0`. |
| `android.permission.SYSTEM_ALERT_WINDOW` | Expo template leftover. `blockedPermissions` in `app.json`. The app does not draw over other apps. |

### Expected uses-permission rows

| Permission | Where it comes from | Why it is there |
| --- | --- | --- |
| `android.permission.INTERNET` | ghostex-native, expo-file-system, React Native | SSH to the paired computer, Easy Connect, the update check, and Send Bug / Feedback. |
| `android.permission.ACCESS_NETWORK_STATE` | ghostex-native, expo-updates, firebase-messaging | Know whether a connection is available. |
| `android.permission.ACCESS_WIFI_STATE` | ghostex-native | The notification's Keep-awake action holds a high-performance wifi lock next to a partial wake lock, so an open terminal stays connected. |
| `android.permission.WAKE_LOCK` | ghostex-native, firebase-messaging | Same Keep-awake action (`PARTIAL_WAKE_LOCK`). Firebase messaging declares it too. |
| `android.permission.FOREGROUND_SERVICE` | ghostex-native | Required to run `GhostexForegroundService`. |
| `android.permission.FOREGROUND_SERVICE_SPECIAL_USE` | ghostex-native | The session-list notification. Declaration text below. |
| `android.permission.FOREGROUND_SERVICE_MICROPHONE` | ghostex-native | Voice agent with the screen locked. Declaration text below. |
| `android.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK` | ghostex-native | Read aloud with the screen locked. Declaration text below. |
| `android.permission.RECORD_AUDIO` | ghostex-native, and the expo-speech-recognition plugin | Dictate and the voice agent. Runtime prompt when the user starts one. |
| `android.permission.MODIFY_AUDIO_SETTINGS` | ghostex-native | Route audio during a voice conversation. |
| `android.permission.CAMERA` | expo-camera plugin and library, expo-image-picker | Scan a pairing code, and take a photo to attach to a session. Runtime prompt. |
| `android.permission.POST_NOTIFICATIONS` | ghostex-native, expo-notifications, firebase-messaging | The ongoing session notification (Android 13+). The app asks for it when a computer is saved. |
| `android.permission.VIBRATE` | expo-haptics | Key and control haptics. |
| `android.permission.RECEIVE_BOOT_COMPLETED` | expo-notifications | Merged from that library. The Android session notification is started by the app while it runs, not by a boot receiver the app schedules. |
| `com.google.android.c2dm.permission.RECEIVE` | firebase-messaging | Merged because expo-notifications links Firebase Cloud Messaging. The phone's notification centre (`src/notifications/notificationCentre.ts`) is iOS-only; Android's ongoing notification is the foreground service, not FCM. |
| `android.permission.READ_EXTERNAL_STORAGE` (`maxSdkVersion` 32) | expo-file-system, expo-image-picker | Legacy storage for Android 12 and below. Not requested on Android 13+. |
| `android.permission.WRITE_EXTERNAL_STORAGE` (`maxSdkVersion` 32) | expo-file-system, expo-image-picker | Same. |

Not declared, on purpose: `READ_MEDIA_IMAGES`, `READ_MEDIA_VIDEO`, `READ_MEDIA_AUDIO`, `ACCESS_FINE_LOCATION`, `ACCESS_COARSE_LOCATION`, `READ_CONTACTS`, and any billing permission. Photos are taken with the camera or picked with the system picker (`expo-image-picker`'s library launcher, `expo-document-picker` for files). Do not add a broad photo-library permission to satisfy a form.

`expo-camera` also links ML Kit barcode scanning (`com.google.mlkit:barcode-scanning`) and the Play services code scanner, and by that library's default also the ML Kit document-scanner Play services artifact. Those manifests add activities, not permissions. The app uses the barcode scanner on the Scan code screen. It does not open the document scanner.

Package visibility (`<queries>`, not permissions): the Tailscale app, a text-to-speech engine, an http viewer for "Open in browser", and a speech-recognition service. The speech plugin names `com.google.android.googlequicksearchbox` so Android 11+ can see a recognition service. Dictate does not use that recognizer; see Data safety.

### Foreground service declaration

One service, `expo.modules.ghostexnative.GhostexForegroundService`, `android:foregroundServiceType="specialUse|microphone|mediaPlayback"`. Play's special-use subtype must be the manifest property verbatim:

> Shows a live list of the user's coding agent sessions on their own computers, with each session's status, and keeps the SSH connections that feed it open while the app is in the background. Tapping a session opens it; Exit in the notification stops the service.

Paste that as the special-use description. It is `android.app.PROPERTY_SPECIAL_USE_FGS_SUBTYPE` in the native manifest. A video of the notification (session rows, tap opens the session, Exit stops it) is what Play's special-use review usually asks for.

Why each type:

- **specialUse.** While at least one computer is saved, the service posts a silent ongoing notification listing that computer's agent sessions and their status, and keeps the SSH connections (and warm terminals) that feed the list open in the background. The user turns it off with Exit. This is not a short data sync and not location. `src/app/persistentNotification.ts` starts it only on Android.
- **microphone.** A voice-agent conversation adds the microphone type to the same service, only after the user starts it from the foreground and only if `RECORD_AUDIO` is granted. The notification becomes "Voice mode on" with Hang up and Mute, so the conversation continues with the screen locked. It is not a second service.
- **mediaPlayback.** Read aloud (auto-read) adds the media-playback type while the app is in front. The notification reads "Reading <session> aloud" with Pause, Resume, and Stop, and speech continues with the screen locked. Stop ends playback and turns auto-read off. A call and read-aloud share the one notification.

### Microphone and camera

**RECORD_AUDIO.** The user starts Dictate in the chat or a voice-agent conversation. The phone records the microphone to a file, then sends that file to Ghostex on the user's own computer. The computer transcribes it, or runs the voice agent, with the speech service configured there (xAI, OpenAI, or local Hex). The recording is not sent to Ghostex's servers. The runtime prompt is the one in `app.json` (`expo-speech-recognition`'s `microphonePermission`). That sentence also says the microphone is used to record a video attachment. The composer picker that ships asks for images, not a video recording (`src/chat/native/composer/attachments.ts`). Declare the microphone for Dictate and the voice agent, which are the paths the code runs.

**CAMERA.** Two uses, both from the user: scan the pairing code on the computer (Scan code, ML Kit barcode on the camera preview), and take a photo to attach to a session. The prompt text is `expo-camera`'s `cameraPermission` in `app.json`.

## Data safety

Proposed answers from the phone app's code and from [the privacy policy](https://ghostex.dev/privacy-policy) (section 9, updated 19 October 2026). Confirm them against the built app before submitting. "Collected" here means the Play question: data leaves the phone.

**Does the app collect or share any of the required user data types?** Yes.

**Is all of the user data collected by the app encrypted in transit?** Yes. The phone talks to the computer over SSH, and over WireGuard when Easy Connect cannot go direct (the relay cannot read the tunnel). Update checks and feedback use HTTPS.

**Do you provide a way for users to request that their data is deleted?** Yes. A feedback issue is public on GitHub; the privacy policy says to ask Ghostex support to have one removed. Usage analytics are sent by Ghostex on the computer, under that computer's usage-analytics setting, not by a phone account (there is none). The update check's installation identifier is not stored (privacy policy).

**Independent security review?** No, unless you have commissioned one.

**Account creation?** No. The app has no Ghostex account.

The phone does not sell data. It does not collect name, email, address, precise location, contacts, SMS, financial info, or health info. Code, prompts, and conversations stay on the user's computer. The privacy policy says they are never sent to Ghostex.

| Data type | Collected? | Shared? | Purpose | Linked / tracking | Notes |
| --- | --- | --- | --- | --- | --- |
| App interactions | Yes | Yes, by the computer, to PostHog, if that computer's usage analytics is on | Analytics | Linked to a one-way hash of the agent account id on the computer. Not used for tracking. Ephemeral: no | The phone never calls PostHog. After a successful inventory fetch it runs `ghostex client-hello` on the computer (OS family, OS version, app version) and can run `ghostex client-view` for the screen. `src/analytics/clientHello.ts`. PostHog keeps the country it derives from the connection. |
| Device or other IDs | Yes | The update identifier goes to `ota.ghostex.dev` and is not stored. ML Kit's installation identifier goes to Google | App functionality (updates). Diagnostics (ML Kit) | Not used for tracking | Update check sends platform, runtime version, and Expo update ids (`expo-current-update-id`, `expo-embedded-update-id`). Privacy policy: a random installation identifier, not stored. |
| Diagnostics | Yes | ML Kit diagnostics are shared with Google. A crash or bug report is shared only when the user sends it | Analytics (ML Kit). App functionality (the report the user submits) | Not used for tracking | ML Kit barcode scanning sends Google device model, OS, app version, performance data, and a per-installation identifier (privacy policy). The QR image itself is decoded on the phone. |
| Audio | Yes, when the user dictates or starts the voice agent | Not with Ghostex. The file goes to the user's computer, which may send it to the speech service that user configured (xAI, OpenAI, or local Hex) | App functionality | Not used for tracking. Optional, user-initiated | Recorded on the phone first (`src/voice/dictation.ts`). The platform speech recognizer is unused. Read aloud uses the phone's own text-to-speech and does not upload speech. |
| Photos | Yes, when the user takes or picks one and attaches it | With the user's computer only | App functionality | Not used for tracking. Optional | Camera and the system photo picker. |
| Files and docs | Yes, when the user attaches a file | With the user's computer only | App functionality | Not used for tracking. Optional | Document picker. Project files are read from the computer over SSH; they are not uploaded to Ghostex. |
| App info and performance, crash logs | Only if the user sends a crash report | Public GitHub issue, after the user reviews it | App functionality | Not used for tracking. Optional | `src/feedback/`. The report is scrubbed of tokens, keys, passwords, cookies, and email addresses before send. The user can attach screenshots. The form can include the GitHub username they type. Posted to `https://feedback.ghostex.dev`, then to a public Ghostex GitHub issue. |

Feedback is optional and user-reviewed. The App Store privacy notes treat that form as optional to disclose. Play's form is stricter about anything that leaves the device, so the row above includes it. If you omit it, the public GitHub issue is still described in the privacy policy.

**Data the form should not claim:** location, web browsing history (Web Preview loads pages from the user's computer or from a site the user typed; Ghostex does not receive that history), installed apps, contacts, calendar, or advertising ID.

## Content rating (IARC)

Recommended answers. The app's own UI has none of the restricted categories. Two answers are not "none", and they are why the target age below is 18+.

| Question | Answer |
| --- | --- |
| Violence (cartoon, fantasy, realistic, prolonged) | No |
| Sexual content or nudity | No |
| Language, profanity, crude humor | No in the app's own UI. Agent output and pages opened in Web Preview are the user's own content |
| Alcohol, tobacco, drugs | No |
| Gambling, real or simulated | No |
| Horror or fear | No |
| Contests, loot boxes, paid random items | No |
| Users can purchase digital goods | No |
| Shares the user's physical location with other users | No |
| Unrestricted internet | **Yes.** Web Preview opens an address the user types, including ordinary websites, and dev servers forwarded from their computer |
| Users can interact or exchange content with other users | **Yes, in a limited way.** Ghostex Collab lets the owner share one chat through a link and a six-digit code. A guest follows that chat in a browser. The owner chooses whether a guest's message is held, sent, or not accepted. There is no public directory, no follower graph, and no account |
| Users can share content publicly inside the app | No, apart from Send Bug / Feedback, which posts the report the user reviewed to a public GitHub issue |
| Made for children, or appealing primarily to children | No |

The computed IARC rating may land on Teen or 16+ because of unrestricted web and the share link. That is fine. Still set the target audience as below so the app is not treated as a children's app.

## Target audience

**18 and older only.** Do not select any under-18 band. The app is not designed for children and should not be in the Families program. Web Preview can open any site, and the terminal and chat show whatever the user's agents print.

## Category

**Productivity.**

Play's category list has Productivity and Tools. It does not have Developer tools. The App Store listing uses Developer tools as the primary category and Productivity as the secondary (`tooling/release-mobile/app-store/app-store.json`). Use Productivity here. Tools is the alternative if you would rather sit beside utilities.

Tags, if Play asks for them: developer tools, terminal, remote access. Do not add a game tag.
