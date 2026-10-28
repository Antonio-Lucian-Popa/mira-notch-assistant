# Mira

A lightweight, local-first **"dynamic island" notch** for the top of your
screen. Mira shows your music, synced lyrics, timers, system notifications and a
file shelf, then gets out of the way. It runs on **Windows 10/11** and
**Ubuntu / GNOME (Wayland)**. Windows is currently a native preview with a smaller feature set.

> **Status: v0.7.0 desktop + Android 0.2.0 voice preview.** Linux has the daemon and GNOME extension. Windows has an
> initial native host with cursor-following eyes, a clock, media-key controls,
> Pomodoro, a persistent daily water counter and standalone reminders. Windows feature parity is in progress. An independent **Android preview** built with React Native and a native Kotlin overlay lives in [`mobile/`](mobile/README.md).

## Principles

- **Light desktop hosts** — target ~0% idle CPU and under 40 MB RAM. No webview, no Electron.
- **Event-driven** — subscribe to OS media events; update countdowns while needed. Avoid continuous
  redraws when the UI is at rest.
- **Local-first** — no accounts, no telemetry. Lyrics use [LRCLIB](https://lrclib.net). The optional paired-phone bridge stays on the LAN; Android speech uses the system recognition provider. No Mira cloud or telemetry.
- **Shared visual language** — one design-token file (`design/tokens.json`)
  drives sizes, colors, animations and pages for both desktop UIs.
- **Bilingual desktop** — Romanian and English. The Android preview starts in Romanian.

## Repository layout

```
mira-notch-assistant/
├─ design/tokens.json         # single source of truth for the notch's look
├─ crates/
│  ├─ mira-core/              # ✅ OS-agnostic logic (state, events, lyrics, storage)
│  ├─ mira-platform-linux/    # ✅ Linux data sources (MPRIS, UPower, … via D-Bus)
│  ├─ mira-win/               # ✅ Initial native Win32 UI
│  └─ mira-daemon/            # ✅ Linux daemon (D-Bus service ro.novabytecode.Mira)
├─ mobile/                    # Android preview: React Native + Kotlin overlay
├─ gnome-extension/           # ✅ GNOME Shell UI (GJS)
└─ packaging/                 # ✅ Windows installer, .deb, systemd unit
```

### `mira-core`

The heart of Mira, with no OS dependencies:

| module | responsibility |
|--------|----------------|
| `model`  | state types: `NowPlaying`, `Lyrics`, `Power`, `SystemEvent`, and phase-2 types |
| `event`  | internal event bus (`tokio::broadcast`) + media-command channel |
| `app`    | `AppState` aggregator; owns the one shared rule — which lyric line is active |
| `lyrics` | LRC parser + line⇄time synchronisation |
| `lrclib` | LRCLIB client (the only networked code), ranked search fallback and timeouts |
| `track` | Browser title normalization, private metadata detection and track identity |
| `store`  | SQLite (`rusqlite`, bundled) + `config.toml` settings |
| `source` | per-OS data-source traits (`MediaSource`, `PowerSource`, … + `MediaControl`) |
| `tokens` | typed view over `design/tokens.json` (embedded at build time) |

## Building & testing

Requires a stable Rust toolchain (1.88+) and a C compiler (for bundled SQLite
and rustls). No system SQLite or OpenSSL is needed.

```sh
# Build and test the core.
cargo test -p mira-core

# Lint & format.
cargo clippy --all-targets
cargo fmt --check
```

### How to test manually (core)

The core has no UI yet. Its behaviour is covered by unit tests:

```sh
cargo test -p mira-core -- --nocapture
```

- **LRC parsing & sync** — `lyrics::tests` cover timed/plain lyrics, multiple
  timestamps per line, metadata-tag handling, fraction precision and active-line
  selection at boundaries.
- **LRCLIB mapping** — `lrclib::tests` map API JSON to lyrics (synced preferred,
  plain fallback, instrumental handling) without hitting the network.
- **Persistence** — `store::tests` round-trip `config.toml` and the SQLite shelf.

### How to test manually (Linux: daemon + GNOME extension)

Build and smoke-test the daemon on a private session bus (no GNOME needed):

```sh
cargo build -p mira-daemon
dbus-run-session -- bash -c '
  ./target/debug/mira-daemon &
  sleep 2
  busctl --user introspect ro.novabytecode.Mira /ro/novabytecode/Mira
  busctl --user call ro.novabytecode.Mira /ro/novabytecode/Mira \
      ro.novabytecode.Mira.Service Ping
'
```

On a real Ubuntu/GNOME (Wayland) session:

```sh
# 1. Run the daemon (MPRIS needs the session bus, UPower the system bus).
cargo run -p mira-daemon

# 2. Build & install the extension, then log out/in and enable it.
./gnome-extension/build.sh install
gnome-extensions enable mira@novabytecode.ro
```

Then: play/pause music in Spotify or a browser and watch the notch update;
hover to open; switch to the Lyrics page; plug/unplug the charger to see the
alert. Check for errors with `journalctl -f /usr/bin/gnome-shell`. If the daemon
is not running, the notch still shows the clock and the preferences window says
so.

### Windows 10/11 (x64)

Download `mira-win.exe` or the per-user setup installer from Releases. The
installer offers optional startup at sign-in and needs no administrator rights.
To build on Windows:

```powershell
cargo build --release --locked -p mira-win
.\target\release\mira-win.exe
# Optional installer (Inno Setup 6 installed):
iscc /DMiraVersion=0.6.0 packaging/windows/mira.iss
```

Move the pointer anywhere on the desktop: the resting eyes follow it smoothly
and blink. Hover to expand; click the title to cycle Music, Pomodoro, Water, Reminders and Your day.
Click the actions below each page to control playback, start/pause/reset focus,
or adjust water. In Reminders, use + New to enter a title and a delay in minutes; browse reminders with the arrows, snooze with +5 min or acknowledge with Done. Right-click for Exit. Water is stored in `%APPDATA%\Mira\mira.db`;
settings, including `language = "ro"` or `"en"`, are read from `config.toml` in
that folder. The installer preserves this personal data on uninstall.

This first Windows host uses system media keys. Track metadata, synced lyrics,
to-dos, file shelf, battery alerts, fullscreen hiding and tray integration are
still Linux-only or pending. Test mixed-DPI displays, a real media player,
water persistence across restart, hover open/close and installer/uninstaller
on a Windows machine before wider distribution. Linux cross-compilation checks
buildability but cannot verify Windows UI behavior.

## Standalone reminders

Reminders have their own page and database table, separate from tasks and timers.
On GNOME, enter a title and `15m`, `2h`, `30s`, or a local time such as `14:30`
(a past clock time means tomorrow). Quick presets offer 5, 15, 30 and 60 minutes.
The next reminder's countdown stays beside the mascot; approaching reminders
open directly on hover. Eyes turn amber in the last five minutes, coral and more
restless in the final minute. At expiry the notification appears and the reminder
stays visible until acknowledged or snoozed for five minutes. Windows provides
the same urgency colors and mascot movement, with a native creation dialog for
the title and delay in minutes. Both hosts preserve pending reminders on restart.

To verify the scheduler on an isolated session bus and temporary database:

```sh
cargo build -p mira-daemon
node gnome-extension/tests/reminders.test.mjs
dbus-run-session -- python3 crates/mira-daemon/tests/reminders_dbus.py
```

## Browser music and lyrics (GNOME)

Mira watches events from **all MPRIS players**, including already-open Brave,
Chrome/Chromium and Firefox instances. Starting playback in another browser,
changing a track, seeking, or closing a player re-evaluates the active session.
Playback commands target the selected player. A player that disappears during
selection is skipped instead of stopping the source. The initial snapshot is
subscribed before discovery so an already-playing browser appears at startup.

Browser/site support still matters: only metadata exposed through MPRIS can be
read. Chromium-based private sessions can deliberately replace title/artist with
**“A site is playing media”**. This is a browser privacy feature, not a song name;
Mira labels it as hidden metadata instead of searching LRCLIB for that phrase.
See the [Chromium implementation](https://github.com/chromium/chromium/blob/main/chrome/browser/media/chrome_media_session_client.cc).

For a private session or incomplete metadata, open **Lyrics**, enter the song
**title** and optional **artist**, and press Search/Enter. This explicitly sends
those fields to LRCLIB without changing browser privacy settings. Manual results
are labeled; look up the next private song again if the browser still exposes the
same placeholder. Synced results follow the playback position supplied by the
browser; plain lyrics are scrollable. Some songs have no LRCLIB entry, and a
browser that withholds position cannot provide reliable synchronized highlighting.

Normal titles such as `Artist – Song (Official Video)` are normalized for lookup.
A new player, title, artist, album or duration invalidates the previous query;
late results from an older query are discarded. No browser history or page
contents are inspected. Windows preview currently has media keys only.

If no song is detected, start playback and check whether the browser exposes an
`org.mpris.MediaPlayer2.*` service (`busctl --user list`). If the browser or its
sandbox does not expose MPRIS, Mira cannot obtain metadata from it. Restart Mira
after upgrading the daemon; browser metadata integration must be enabled in the
browser where supported.

## Lunch and end-of-work greetings (GNOME)

Open Mira preferences → **Prânz și sfârșitul programului**:

- Enable lunch and choose its local time, for example `13:00`.
- Enable the end-of-work greeting and choose the finish time, for example `18:00`.
- Set an advance heads-up from 0 to 60 minutes (default: 5).
- Choose weekdays only or every day; use the three Preview buttons to see the animations.

Lunch uses happy eyes and hearts; the heads-up is gentle; the final goodbye
adds a small confetti celebration. The optional greeting name personalizes the
messages. Settings validate `HH:MM`, including `00:00`. Each event also produces
a desktop notification. Daily delivery markers survive extension restarts to
avoid duplicates; a 15-minute catch-up window handles short suspends without
replaying old lunch greetings late in the day. Calendar times follow the local
timezone and daylight-saving changes. These routines require an active GNOME
session with the extension enabled; pending banners wait until the notch is free.
This settings UI is not yet part of the Windows preview.

## Voice, PDFs and phone control (desktop 0.7 / Android 0.2 preview)

Desktop navigation now uses an icon toolbar at the top, with an active-tab highlight
and a compact date/time stamp. Side navigation arrows and bottom page dots are gone.
The toolbar includes **Telefon** for pairing, receiving transcripts and registering shortcuts.

1. On the PC, open **Telefon** and **Pornește primirea**. The receiver is off by
   default and stops when the GNOME extension / Windows application closes.
2. On Android, open **Voce & PC**, scan the desktop QR live or paste its pairing
   code, then select **Conectează**. Pairing authorizes PDF uploads **and** the
   locally registered keyboard actions. Keep the pairing code private; **Revocă
   asocierea** on the PC rotates its identity and invalidates old pairings.
3. Download the free local Whisper model once in **Voce & PC** (about 148 MB),
   then enable **Activare vocală Mira** and allow microphone access. Say `Mira`,
   then `pornește transcrierea`. Recognition and manual dictation run locally,
   independently of Google/Samsung speech engines, with no paid API or account.
   `Mira gata` saves the transcript and returns to wake listening. Disable from
   the switch or ongoing notification. Dictation is limited to 30 minutes.
4. Completed text stays on the phone, with a paginated A4 PDF export. The unchanged
   PDF is generated on the **phone**, then delivered to `Documents/Mira/Transcrieri`
   on the PC alongside UTF-8 text. This is typography, not an AI summary. Android
   schedules automatic uploads on unmetered networks; manual sync is available.
   The PC must be on, receiving, and reachable at the paired LAN address. Physical
   proximity alone is not detected; background work may be delayed. Re-pair if the
   PC's IP address changes. There is no Internet relay or shared cloud account.
5. Define keys **on the PC**, e.g. `CTRL+ALT+2` named “Plec la baie”; load the remote
   buttons on the phone. The shortcut must already have a meaning in your OS/app.
   The built-in **Blochează PC-ul** uses the OS lock action without requiring a
   shortcut. Requests name a registered profile; the phone cannot supply arbitrary
   commands, edit profiles remotely, or execute shell scripts. Offline key actions
   are not retried later; expired jobs are discarded. Linux ignores custom key
   injection while locked. OS-reserved combinations and secure Windows desktops
   can reject synthetic input.

The Android overlay can merge a reported camera cutout into a black notch
attached to the top edge, with curved shoulders, two blinking eyes below the
lens and a cyan bottom glow. Horizontal/vertical calibration and scale are
available, with the normal capsule as fallback. The
physical lens has no drawable pixels, and protected system surfaces may hide the
app's overlay. This was tested with an emulated punch-hole, **not a Galaxy S26**.

**Voice limits:** local Whisper processes speech segments after a short pause;
this is not a low-power hardware hotword detector. Accuracy, latency, CPU/memory
and battery use depend on the device and environment. Android can interrupt the
foreground service; locked-screen/24-hour operation is not guaranteed. Raw audio
stays in bounded memory buffers and is never uploaded or archived. Opted-in
listening resumes on opening the app, never at boot. Commands use a fixed
whitelist. See [mobile/LOCAL_VOICE.md](mobile/LOCAL_VOICE.md) for MIT attribution,
model integrity, privacy, build and test details.



Transport uses a pinned self-signed TLS identity and a high-entropy pairing token,
not a shared six-digit password or a trust-all HTTPS connection. Certificates and
credentials stay in private application storage. The receiver is opt-in, IPv4 LAN
only (port 45271); it does not change firewall rules or enable itself at startup.
PDF receipt IDs are immutable/idempotent, names cannot traverse directories, and
existing differing files are rejected. Keyboard profiles, queues and request sizes
are bounded. “Accepted” on the phone means queued, not proof that an OS shortcut ran.

Verification includes native Android microphone start/stop, camera-cutout placement,
PDF pagination/diacritics, Android-to-Rust TLS upload, wrong-certificate rejection,
receipt replay and profile-only key requests. The key-queue test does not lock or
send keys to the developer's personal desktop. Windows cross-compiles; actual
Windows key injection / installer / signature still require Windows validation.

AI enhancement is optional future work. Official [Sign in with ChatGPT](https://developers.openai.com/siwc/quickstart)
supports eligible plan usage in participating/open-source integrations under plan
limits; this is distinct from ordinary paid API keys. It is **not implemented** in
Mira and is unnecessary for these fixed commands/PDFs. [Claude's documentation](https://support.claude.com/en/articles/9876003-i-have-a-paid-claude-subscription-pro-max-team-or-enterprise-plans-why-is-claude-api-usage-billed-separately-from-my-paid-claude-plan)
describes separate API billing and plan-specific credits; a subscription is not a
blanket promise of unlimited third-party API usage.

## Desktop workspace (0.6.0)

The desktop companion now opens **on click**. Double-click its frame/eyes to return
home. Drag the eyes/header downward to detach a rounded workspace; drag its lower
right corner to resize, or throw it upward to dock it. The eyes remain visible in
the header and follow the cursor with 500 ms easing.

Right-click exposes **Follow (500 ms trail)**, **Pin**, **Free roam**, **Home**,
workspace pages, clipboard pause/clear and size controls. Follow pauses while the
pointer is on the robot, while dragging files or while the menu/workspace is open.
Free roam uses occasional moves and expressions; thought captions show actual CPU
and RAM measurements, never randomly generated percentages.

- **File shelf:** drop files/folders, or choose a folder from the menu. Entries are
  shortcuts; opening launches the original and clearing never deletes it. Folder
  context includes immediate children, up to 256 links and eight search roots.
  GNOME uses a GTK4 drop surface at the robot's position for real Wayland drops;
  it returns to the shelf after receiving files. Windows accepts Explorer drops.
  GNOME cards spring upward on hover; Windows cards gently float (motion can be disabled).
- **Folder under the robot:** on release, Windows attempts a one-shot UI Automation
  lookup for an Explorer/desktop folder item. GNOME supports desktop icon delegates
  exposing a file URI. Nautilus/DING and other process surfaces may not expose that
  information; dropping/selecting the folder works independently of this lookup.
  Windows lookup is compiled but still needs verification on an actual Windows desktop.
- **Sticky notepad:** multiline temporary text, including Ctrl+V. **Clipboard:** the
  last 32 unique copied texts (up to 16,384 characters each); click to copy one again.
  Windows also records copied file paths. Images/rich clipboard formats are not captured.
  Capture can be paused; GNOME avoids capturing on the lock screen. Clipboard/sticky
  contents live only in process memory, never SQLite/logs/cloud. Clear removes them;
  restarting the daemon/app ends the session. GNOME disable also clears the workspace.
- **Folder search:** literal, case-insensitive grep implemented with the Rust regex
  engine. Only folders explicitly added to this workspace are searched. Runs on a
  worker, supports cancellation, skips symlinks, binary/hidden files and common build
  directories. Bounds: 3 seconds, 4,000 files, 2 MiB per file, 200 hits. Partial results
  are labeled. Results include file, line and excerpt; opening uses your default app
  (editor-specific jump-to-line integration is not implemented).

GNOME preview gestures and UI were exercised in an isolated Wayland compositor;
Windows cross-compiles, but native gestures/Explorer integration need Windows testing.
The Android 0.1 preview is unchanged by this desktop release.

Integration verification (isolated session):

```sh
cargo build -p mira-daemon
dbus-run-session -- python3 crates/mira-daemon/tests/workspace_dbus.py
node --test gnome-extension/tests/*.test.mjs
```

## Companion features (0.5.0)

- **Contextual personality:** music brings subtle rhythmic movement, focus a
  concentrated expression, evening a sleepy one. Urgent reminders take priority.
  The music movement is decorative; Mira does not analyze microphone/audio data.
- **Four native themes:** Classic, Robot, Cat (with slit pupils) and Pixel.
  GNOME preferences → **Studio mascotă** selects a theme, contextual behavior,
  motion intensity (0–1), any of 16 expressions and four context previews.
  Studio previews wait while a reminder is urgent. Zero intensity disables
  expressive movement and particles while leaving text/countdowns readable.
- **Natural reminders, locally parsed:** `Sună-l pe Alex peste 20 de minute`,
  `Call in one hour`, `Raport mâine la 10`, `Ședință în fiecare luni la 9`,
  `Pauză every day at 13:00`. A preview shows the parsed title/time before saving.
  This is a documented deterministic grammar, not arbitrary-language AI.
  Invalid/past times produce feedback instead of silently choosing a default.
- **Recurring reminders:** daily and weekly rules survive restarts. ✓ completes
  the displayed occurrence and advances to the next future local-calendar slot;
  × deletes the series. Snooze keeps the recurrence. Duplicate confirmations
  cannot skip the next occurrence. DST gaps skip nonexistent recurring slots;
  one-off nonexistent local times are rejected. Ambiguous times use the earlier
  occurrence. Missed occurrences are not replayed as a notification storm.
- **Your day:** a separate page shows recorded focus minutes/sessions, completed
  tasks, confirmed reminders and water. The automatic summary has its own local
  time (default 21:00), also enriches the work-end goodbye, and uses persisted
  real activity. It does not invent history from before this upgrade. Water keys
  from the older Linux UTC-day format migrate to local calendar dates.

Windows preview includes the shared natural parser, recurrence and activity
storage, a Your day page and automatic summary. Its right-click menu offers the
four themes, contextual behavior, three motion levels and a native expression
studio. WinRT media events drive music context without polling. The summary time
is read from `daily_summary_time = "21:00"` in `%APPDATA%\Mira\config.toml`;
theme/intensity changes made in the menu save there too. Windows task counts stay
zero until a Windows task UI is implemented. Browser metadata/lyrics parity is
still pending; the media event watcher currently supplies playback state only.
The Windows build is cross-compiled here and requires real Windows UI testing.

## Windows signing

The release pipeline is prepared for **SignPath Foundation**, including the
application, Inno uninstaller and final setup signature, publisher/timestamp
checks, and an explicit `windows-signing-status.json` report. See the complete
[owner activation guide](packaging/windows/SIGNING.md). **No SignPath account has
been configured or approval obtained by this change; current local executables
are unsigned.** Once signing is enabled, failures block publishing instead of
falling back to unsigned output. A valid signature does not guarantee immediate
SmartScreen reputation for a new app. No security setting needs to be disabled.

## Roadmap

**Phase 1 (current):** core → Windows host (notch window, GSMTC media, LRCLIB
lyrics, battery/charger alerts, fullscreen hide, tray + autostart) → Linux daemon
(MPRIS + UPower over D-Bus) → GNOME extension (integrated & floating modes) →
minimal settings → packaging (per-user Windows installer, `.deb`).

**Phase 2 (Linux done):** Focus/Pomodoro timer, water counter, to-dos with
reminders, and a file shelf — all wired through the daemon (D-Bus interfaces
`Pomodoro`/`Water`/`Todo`/`Shelf`, SQLite persistence) and the GNOME extension,
plus the always-on blinking mascot. Add files to the shelf with `mira-add
<file>` (Wayland can't drop files onto shell chrome). Still to come: Wi-Fi,
headphones, and the volume/brightness/Caps-Lock/keyboard-language overlays, and
the Windows host for all of it.

## License

MIT.

### Regression checks for browsers and routines

```sh
cargo test --workspace --locked
cargo test -p mira-core --test lrclib_http -- --ignored
MIRA_TEST_PRIVATE_BUS=1 dbus-run-session -- cargo test -p mira-platform-linux --test mpris_sessions
node gnome-extension/tests/routines.test.mjs
node gnome-extension/tests/mascot.test.mjs
node gnome-extension/tests/reminders.test.mjs
node gnome-extension/tests/gaze.test.mjs
glib-compile-schemas --strict gnome-extension/schemas
```

The browser regression test uses two simulated MPRIS services on a private bus;
it covers switching already-open browsers, private metadata, command routing,
player exit and returning to an empty session. It is opt-in to keep ordinary
`cargo test` from touching a live user bus. Real browser behavior depends on the
installed browser, site, sandbox and privacy configuration.

On Ubuntu/GNOME, Mira starts in the top panel at the clock position. Hover to
open the workspace with its top icon tabs; leaving closes it after a short delay.
Existing installs return to this panel layout once on upgrade. Pin, follow and
free roam remain available explicitly from the companion context menu; choose
“Acasă · înapoi sus” to return to the panel and restore hover opening.

The compact GNOME layout reserves space for the clock/date even at 440 px.
Reminder titles use the full panel width, with time presets on a separate row.
Android 0.2.1 adds an opt-in “Merg la baie” shake gesture under Voice & PC: with
the overlay active and phone screen on, three firm shakes request the paired
PC's registered lock action. It requires an online PC with reception enabled,
does not retry offline commands, and allows consecutive three-shake sequences
without a command cooldown (only individual movement peaks are debounced).

The GNOME workspace uses a 520×340 default, with a fixed top toolbar and a
vertically scrollable content area. Larger forms and all registered phone
actions remain reachable without enlarging the whole panel. Android 0.2.2
reads pairing QR codes automatically from its live in-app camera preview,
without taking a photograph. Manual pairing codes remain available.

Android 0.2.3 requests the PC's saved shake sequence. On Ubuntu, add shortcuts
in Telefon, then mark one “Plecare” and another “Revenire”. Each accepted shake
request alternates the selected profiles; “Resetează la plecare” realigns the
sequence after manual changes. Mappings survive restarts and re-pairing. This
is request alternation, not automatic detection of the actual lock state.
The GNOME receiver can remain active while locked, with Mira's UI hidden,
but return shortcuts do not bypass the system's unlock authentication.

Any phone shortcut, including the built-in lock action or a selected shake
action, can be deleted. Deletion clears only its shake assignments and pending
queue entries. It never selects another action automatically. With one selected
action, shakes repeat that action; with neither selected, shaking does nothing.
These choices, including an empty list, remain saved after receiver restarts.

The GNOME workspace opens on a live Home overview with three gradient cards:
music, focus and upcoming reminders. Each card opens its full page and has a
working playback/focus/add control. Navigation is pinned below the content;
All sections are directly available as compact bottom icons; only the selected
section shows its label. Cards lift on hover when motion is
enabled, respect the motion-intensity setting, and fit the 440 px minimum width.
