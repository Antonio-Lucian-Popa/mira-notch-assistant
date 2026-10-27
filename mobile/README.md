# Mira for Android — 0.2.0 preview

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

The preview uses the standard public Android **debug signing key**, bundles JavaScript and works without Metro. It is a test APK, not a production Play release. Keep a private production signing/upload key outside Git before public distribution; Windows SignPath configuration does not sign Android APKs.

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

The native microphone service uses the configured Android speech provider, with
explicit RECORD_AUDIO permission, a visible foreground notification and Stop.
“Listen for Mira” is a **60-second command session**, not permanent hotword
recognition. Say “Mira / Hey Mira” then “pornește transcrierea”; the direct button
starts dictation immediately. “Mira gata” is stripped even across ASR result chunks.
Transcription stops after 30 minutes or on unrecoverable provider errors. Confirmed
text is saved; there is no boot-time / invisible microphone activation, local LLM,
paid model API, or raw audio archive. Romanian availability and accuracy depend on
the device/provider; audio may be processed online by that provider. Wake + focus,
pause/resume/stop and water commands are a finite whitelist.

Records live in private app files (up to 100); export creates a cached, immutable
PDF with Unicode text, A4 pagination and a repeating header/footer. Share uses a
scoped FileProvider, without broad storage permission. Deleting a phone record
cannot remove a copy already received by the PC. An interrupted draft remains
readable; completed sessions are eligible for upload.

On desktop 0.7, open **Telefon**, start receiving and scan/photograph the QR (or paste
its code) in **Voice & PC**. The camera photo is decoded locally as QR and deleted;
it is not sent to a model. Pairing verifies the pinned TLS certificate before
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

The single-eye camera style uses reported display-cutout geometry with X/Y
calibration; ordinary capsule remains the fallback. It draws around a lens, never
on the physical hole. Status/system surfaces can cover app overlays. Verified on
an emulated punch-hole, not an actual Samsung S26. Speech service start/stop was
verified on the emulator, while recognition accuracy and photo-based QR reading
still need real microphone/camera testing on a phone.

See the root README for transport, privacy and test details. Instrumented tests
should use a disposable emulator: they install the test variant and manage pairing
state. Pure rules cover wake boundaries and stop-phrase removal; an instrumented
round trip rejects a wrong certificate and transfers a real multipage PDF.
