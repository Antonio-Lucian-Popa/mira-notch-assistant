# Mira

A lightweight, local-first **"dynamic island" notch** for the top of your
screen. Mira shows your music, synced lyrics, timers, system notifications and a
file shelf, then gets out of the way. It runs on **Windows 10/11** and
**Ubuntu / GNOME (Wayland)**, and looks identical on both.

> **Status: Phase 1, in progress.** This repository is being built in small,
> always-compiling steps. The shared core is done; platform hosts follow.

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
│  ├─ mira-platform-win/      # ⏳ Windows data sources (GSMTC, WMI, …)
│  ├─ mira-platform-linux/    # ⏳ Linux data sources (MPRIS, UPower, … via D-Bus)
│  ├─ mira-win/               # ⏳ Windows executable (native UI + tray)
│  └─ mira-daemon/            # ⏳ Linux daemon (D-Bus service ro.novabytecode.Mira)
├─ gnome-extension/           # ⏳ GNOME Shell UI (GJS)
└─ packaging/                 # ⏳ Windows installer, .deb, systemd unit
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

Platform-specific "how to test manually" sections (Windows notch, GNOME
extension) are added as those hosts land.

## Roadmap

**Phase 1 (current):** core → Windows host (notch window, GSMTC media, LRCLIB
lyrics, battery/charger alerts, fullscreen hide, tray + autostart) → Linux daemon
(MPRIS + UPower over D-Bus) → GNOME extension (integrated & floating modes) →
minimal settings → packaging (per-user Windows installer, `.deb`).

**Phase 2 (after sign-off):** Pomodoro, water, todos + reminders, shelf, Wi-Fi,
headphones, volume/brightness/Caps-Lock/keyboard-language overlays, the mascot.

## License

MIT.
