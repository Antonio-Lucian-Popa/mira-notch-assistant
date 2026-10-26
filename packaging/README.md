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

## Windows (per-user installer) — pending

Will build a per-user installer (no admin rights) with a clean uninstall, via
Inno Setup, under `packaging/windows/`. Added with the Windows host.
