# Mira

A lightweight, local-first **"dynamic island" notch** for the top of your
screen. Mira shows your music, synced lyrics, timers, system notifications and a
file shelf, then gets out of the way. It runs on **Windows 10/11** and
**Ubuntu / GNOME (Wayland)**, and looks identical on both.

> **Status: v0.2.0.** Linux has the daemon and GNOME extension. Windows has an
> initial native host with cursor-following eyes, a clock, media-key controls,
> Pomodoro and a persistent daily water counter. Windows feature parity is in progress.

## Principles

- **Light** — target ~0% idle CPU and under 40 MB RAM. No webview, no Electron.
- **Event-driven** — subscribe to OS events; never poll. Redraw only while an
  animation runs.
- **Local-first** — no accounts, no telemetry. The only network access is to
  [LRCLIB](https://lrclib.net) for lyrics.
- **Identical on both OSes** — one design-token file (`design/tokens.json`)
  drives sizes, colors, animations and pages for both UIs.
- **Bilingual** — Romanian and English from the start.

## Repository layout

```
mira-notch-assistant/
├─ design/tokens.json         # single source of truth for the notch's look
├─ crates/
│  ├─ mira-core/              # ✅ OS-agnostic logic (state, events, lyrics, storage)
│  ├─ mira-platform-linux/    # ✅ Linux data sources (MPRIS, UPower, … via D-Bus)
│  ├─ mira-win/               # ✅ Initial native Win32 UI
│  └─ mira-daemon/            # ✅ Linux daemon (D-Bus service ro.novabytecode.Mira)
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
| `lrclib` | LRCLIB client (the only networked code) |
| `store`  | SQLite (`rusqlite`, bundled) + `config.toml` settings |
| `source` | per-OS data-source traits (`MediaSource`, `PowerSource`, … + `MediaControl`) |
| `tokens` | typed view over `design/tokens.json` (embedded at build time) |

## Building & testing

Requires a stable Rust toolchain (1.80+) and a C compiler (for bundled SQLite
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
iscc /DMiraVersion=0.2.0 packaging/windows/mira.iss
```

Move the pointer anywhere on the desktop: the resting eyes follow it smoothly
and blink. Hover to expand; click the title to cycle Music, Pomodoro and Water.
Click the actions below each page to control playback, start/pause/reset focus,
or adjust water. Right-click for Exit. Water is stored in `%APPDATA%\Mira\mira.db`;
settings, including `language = "ro"` or `"en"`, are read from `config.toml` in
that folder. The installer preserves this personal data on uninstall.

This first Windows host uses system media keys. Track metadata, synced lyrics,
to-dos, file shelf, battery alerts, fullscreen hiding and tray integration are
still Linux-only or pending. Test mixed-DPI displays, a real media player,
water persistence across restart, hover open/close and installer/uninstaller
on a Windows machine before wider distribution. Linux cross-compilation checks
buildability but cannot verify Windows UI behavior.

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
