# Mira for Android — preview

An independent, local-first Android companion built with **React Native 0.85 / TypeScript** and a **native Kotlin floating capsule**. Android 8.0+ (API 26); this preview APK includes ARM64 phones and x86-64 emulators. iOS is not implemented.

## What works

- Five screens, including Voice & PC: Home, Reminders, Studio, Settings and Voice & PC. Romanian interface.
- Native floating capsule above ordinary apps, enabled explicitly by the user. Tap to expand, tap the expanded body to open Mira; tap × to collapse. Long press opens Mira. Eyes follow a finger moved inside the capsule.
- Four themes: Mira, Orbit, Nori, Pixel. Preview expressions; capsule scale, vertical spacing and reduced motion controls.
- Persistent local reminders, relative minutes or the next local clock time, optional daily recurrence. Countdown and agitated eyes during the last minute. Complete and snooze for five minutes in the app or notification. Deleting cancels the series; snoozing preserves the original daily clock time. Repeated completion of the same occurrence cannot skip another day.
- A persistent 25-minute focus timer with pause, resume, stop and completion notification. Water counter and real daily totals; stopped sessions do not count as completed sessions.
- Alarm rescheduling after reboot, timezone/time changes and app startup. The capsule does **not** automatically restart after boot or force-stop.

One native `SharedPreferences` store is authoritative for the React Native UI, service and receivers. The floating view draws on Android Canvas, stops frame callbacks when the screen turns off, and draws animations at 30 fps only while moving. The React Native screen refreshes once a second only while foregrounded. No login, analytics, ads, cloud service, notification-reading permission or Accessibility service.

## Android behavior

The capsule requires Android's **Display over other apps** permission. Grant it from Settings inside Mira, return to the app, then select **Start capsule**. It stays below protected system surfaces and does not replace the physical camera cutout or status bar. Placement varies by device; adjust its spacing and scale. A foreground service notification provides a stop control. Android 14+ `specialUse` explains the user-enabled overlay use case in the manifest; Play review/approval has not been obtained.

Allow notifications separately for reminder and focus alerts. Alarms deliberately use `setAndAllowWhileIdle`, without exact-alarm permission: **Android may delay them in Doze or under manufacturer battery restrictions**. These are personal reminders, not a precise alarm clock. Force-stopping the app prevents deliveries until it is opened again. Denying notification access leaves reminders visible in the app/capsule but prevents system alerts. Device testing beyond the API 35 emulator is still needed.

The preview uses the standard public Android **debug signing key**, bundles JavaScript and works without Metro. It is a test APK, not a production Play release. Production distribution requires a private signing/upload key kept outside Git; Windows SignPath configuration does not sign Android APKs.

GitHub Releases include the preview APK alongside the Ubuntu package, named
`mira_X.Y.Z_android_preview.apk`. Its Android version name matches the release tag
plus `-preview`, and the app displays that native version in Settings. Preview
builds retain their development signing key; production signing remains separate.

## Build

Prerequisites: Node 22.13+, npm, JDK 17, Android SDK platform/build-tools 36, NDK 27.1.12297006. Set `ANDROID_HOME` or an ignored `android/local.properties` with `sdk.dir=/path/to/Android/Sdk`.

```sh
cd mobile
npm ci
npm run typecheck
npm run lint
npm test -- --runInBand
npm run apk
# android/app/build/outputs/apk/preview/release/app-preview-release.apk
```

For development, start Metro with `npm start`, then `npm run android` with a connected phone/emulator. The `previewDebug` variant uses Metro; `previewRelease` is standalone.

Native verification:

```sh
cd mobile/android
./gradlew testPreviewDebugUnitTest
# An emulator/device must be connected for the following:
./gradlew connectedPreviewDebugAndroidTest
```

Unit tests cover local scheduling, input validation, urgency, countdowns and DST recurrence. The React Native integration test submits the reminder form. Instrumented store tests cover persistence, recurring completion idempotence, snooze, cancellation, water and focus pause/resume, using separate disposable test preferences.

## Voice & PC — new in 0.2

The Voice & PC screen uses **local Whisper**, independent of Google/Samsung
speech providers. Download the free multilingual base model once (about 148 MB),
then enable **Activare vocală Mira** with microphone permission. Say “Mira”,
followed by a whitelisted command, together or within 15 seconds. “Mira gata”
saves dictation and returns to listening; a switch and notification Stop disable
the microphone. Manual transcription is local too. Dictation is limited to
30 minutes. No paid API, account or key is involved.

This uses speech-gated CPU inference, not a hardware hotword engine. It has
recognition latency and consumes battery/memory while enabled. Android can stop
background capture. Raw audio is bounded in memory and is never archived or
uploaded. The model download is SHA-256 verified. See [LOCAL_VOICE.md](LOCAL_VOICE.md)
for model integrity, licensing, build requirements and testing limitations.

Records live in private app files (up to 100); export creates a cached, immutable
PDF with Unicode text, A4 pagination and a repeating header/footer. Share uses a
scoped FileProvider, without broad storage permission. Deleting a phone record
cannot remove a copy already received by the PC. An interrupted draft remains
readable; completed sessions are eligible for upload.

On desktop 0.7, open **Telefon**, start receiving and scan the QR live (or paste
its code) in **Voice & PC**. The in-app camera preview automatically decodes QR frames locally; no photo
is taken or saved and frames are not sent to a model. Camera access is requested
when scanning starts; Back closes the scanner. Pairing verifies the pinned TLS certificate before
saving the bearer token. It grants PDF uploads and the PC's registered shortcuts.
Only the same reachable IPv4 LAN is supported; no Bluetooth proximity detector or
cloud relay. Re-pair after a PC IP change. Unmetered-network WorkManager jobs retry
transcript delivery and run periodically (minimum 15 minutes, potentially delayed
by Android). Manual sync also exists. A 200 receipt is required before marking a
record delivered; retrying does not create duplicate PDFs. Disconnect cancels jobs.

Load remote buttons from the paired PC. Define custom combinations **on the PC**,
e.g. CTRL+ALT+2, or use the OS lock profile. Requests carry an action ID, never shell
commands or arbitrary key sequences. Offline commands fail instead of being queued
for a surprise execution later. “Accepted” is queue acknowledgement; the target OS
may reserve or ignore a combination. Revoking on the desktop invalidates pairing.

The camera notch uses reported display-cutout geometry: a black tab attached to
the top edge, curved shoulders, two blinking white eyes below the lens, and a
wide cyan glow along its rounded bottom. X/Y calibration and scale remain
available; upward calibration keeps the eyes below the camera. The ordinary
capsule remains the fallback for landscape, off-center/wide cutouts or missing
cutout data, keeping status controls reachable.
The physical camera lens is never painted over. Status/system surfaces can cover app overlays. Verified on
an emulated punch-hole, not an actual Samsung S26. The native Whisper backend was checked on a connected Samsung phone with
offline synthesized Romanian command fixtures and microphone startup; actual
speaker accuracy and live camera QR reading still need device-specific testing.

See the root README for transport, privacy and test details. Instrumented tests
should use a disposable emulator: they install the test variant and manage pairing
state. Pure rules cover wake boundaries and stop-phrase removal; an instrumented
round trip rejects a wrong certificate and transfers a real multipage PDF.

### Shake: leave / return

In Voice & PC, enable “Merg la baie” after pairing and starting the Mira
overlay. On the PC's Phone page, choose “Plecare” and “Revenire” from the saved
shortcut list (for example CTRL+ALT+2 and CTRL+ALT+1). Three distinct strong
shakes within 1.2 seconds request the next configured action over pinned TLS.
The PC persists the mapping and next step, including across receiver restarts.
Repeated requests with the same ID do not advance the sequence twice. The step
advances when the PC accepts a request, which does not prove OS execution or
actual lock state. Use “Resetează la plecare” after manual state changes.
Revoking/re-pairing rotates credentials while retaining profiles and resetting
the step to departure. Detection runs with the
overlay while the phone screen is on, including over other apps. It pauses
when the screen is off and stops with the overlay. There is no cooldown between complete three-shake sequences. A 120 ms
per-peak debounce and low/high hysteresis reject repeated accelerometer events
from the same movement; the counter resets immediately on the third shake. Commands are not queued for an offline PC. Disabling the gesture or
unpairing stops detection. Without a return mapping the departure action is
repeated. Return sends the configured shortcut; it never bypasses system
password/biometric authentication. Reserved shortcuts may be rejected by the OS.
