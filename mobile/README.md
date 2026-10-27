# Mira for Android — 0.1.0 preview

An independent, local-first Android companion built with **React Native 0.85 / TypeScript** and a **native Kotlin floating capsule**. Android 8.0+ (API 26); this preview APK includes ARM64 phones and x86-64 emulators. iOS is not implemented.

## What works

- Four screens: Home, Reminders, Studio, Settings. Romanian interface.
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

## Desktop connection: prepared, not active

There is **no pairing, synchronization or network listener** in this preview. `src/sync-contract.ts` defines a proposed versioned event envelope for a later companion protocol; it is not imported by the running app. Desktop changes are still required.

The next stage should implement QR pairing with one-time, expiring credentials, mutual device authentication and encrypted transport. Persist an outbox, deduplicate by event ID, scope entity IDs by origin, keep deletion tombstones and use expected occurrence timestamps for recurring completions. Define conflict resolution and notification ownership before enabling two-device reminders. Begin on the same Wi-Fi network; Internet relay/cloud support is a separate feature. A device address alone must never establish trust.
