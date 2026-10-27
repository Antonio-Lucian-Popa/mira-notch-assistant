# Packaging

## Automated GitHub Releases

After an approved code change reaches protected `main`, the release workflow
chooses the next `vX.Y.Z` tag and builds the Ubuntu `.deb`, Android preview APK,
and Windows artifacts (when the Windows crate exists). Publication waits for all
required builds to succeed. Documentation/website-only pushes are excluded.

The Android build reuses the pull-request workflow. Its actual APK metadata is
checked against the release version before the asset is named
`mira_X.Y.Z_android_preview.apk`. Its Android version code is assigned from the
release workflow run number, above the historical preview code. Ubuntu packages
use the same version through `MIRA_VERSION`. `SHA256SUMS` covers the `.deb` and APK.

Only the publishing job has `contents: write`; PR build jobs remain read-only.
Manual release runs are limited to `main` or validated version tags, and version
tags must point to commits already contained in `main`. Builds need no personal
access token. The Android APK remains a development-key-signed preview, not a
production-signed Play Store build; this is also stated in the release notes.

## Linux (`.deb`) — ready

```sh
packaging/linux/build-deb.sh      # -> target/debian/mira_<ver>_amd64.deb
sudo apt install ./target/debian/mira_*_amd64.deb
```

The package installs:

| Path | What |
|------|------|
| `/usr/bin/mira-daemon` | the daemon binary (release, stripped) |
| `/usr/lib/systemd/user/mira-daemon.service` | systemd **user** unit (`Type=dbus`) |
| `/usr/share/dbus-1/services/ro.novabytecode.Mira.service` | D-Bus activation (`SystemdService=`) |
| `/usr/share/gnome-shell/extensions/mira@novabytecode.ro/` | the GNOME extension |

After install, per user:

```sh
systemctl --user enable --now mira-daemon.service
gnome-extensions enable mira@novabytecode.ro   # after logging out/in
```

extensions.gnome.org cannot host the daemon binary, so the `.deb` is the
distribution channel on Ubuntu.

## Windows (per-user installer)

On Windows with Rust and Inno Setup 6:

```powershell
cargo build --release --locked -p mira-win
iscc /DMiraVersion=0.7.0 packaging/windows/mira.iss
```

Output: `packaging/windows/Output/mira_0.7.0_windows_x64_setup.exe`.
The portable binary is `target/release/mira-win.exe`. The installer writes to
LocalAppData, offers optional startup at sign-in, and includes an uninstaller.
Personal settings and the SQLite database under `%APPDATA%\Mira` are preserved.
The release workflow publishes the `.deb`, Android preview APK, portable `.exe`,
and installer together; publication requires the configured platform builds to succeed.

Windows signing is prepared through SignPath Foundation; activation, prerequisites and independent verification are documented in [SIGNING.md](windows/SIGNING.md). Local builds are unsigned until approved signing is configured.
