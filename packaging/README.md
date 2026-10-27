# Packaging

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
iscc /DMiraVersion=0.5.0 packaging/windows/mira.iss
```

Output: `packaging/windows/Output/mira_0.5.0_windows_x64_setup.exe`.
The portable binary is `target/release/mira-win.exe`. The installer writes to
LocalAppData, offers optional startup at sign-in, and includes an uninstaller.
Personal settings and the SQLite database under `%APPDATA%\Mira` are preserved.
The release workflow builds and publishes the `.deb`, portable `.exe`, and
installer together; publication requires both platform builds to succeed.

Windows signing is prepared through SignPath Foundation; activation, prerequisites and independent verification are documented in [SIGNING.md](windows/SIGNING.md). Local builds are unsigned until approved signing is configured.
