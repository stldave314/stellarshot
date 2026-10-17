# Flatpak packaging: what the manifest needs

**Date:** 2026-10-16 · For whoever writes the Flatpak manifest by hand (see
ST-1 in `docs/plans/2026-10-16-cosmic-store-audit.md`). This says what the
application already does inside a sandbox, and what the manifest must give it.
It is not a manifest.

## What the application does inside a sandbox

Everything below hangs off one check, `sandbox::sandboxed()` (the presence of
`/.flatpak-info`). Native packages are unaffected.

| Area | Native | In a Flatpak |
|---|---|---|
| Scheduled backups | systemd user timers and path units (`src/timers.rs`) | `stellarshot --background` (`src/background.rs`): one long-running process, a check every minute, each due backup run through the same `--scheduled` path |
| Starting at login | the timers themselves | the Background portal; where there is none (COSMIC, as of xdg-desktop-portal-cosmic 1.10.0), an autostart entry written to `~/.config/autostart/io.github.stldave314.Stellarshot.desktop` |
| "Run now" | `systemctl --user start` | the `--scheduled` child started directly |
| Next run shown on a backup's page | the timer's next elapse, over D-Bus | the last run plus the schedule's period |
| Removable drives | `/proc/self/mountinfo` and `/dev/disk/by-*` | UDisks2 over the system bus (`drives::udisks_drives`) |
| Backup when its drive connects | a systemd path unit | the background process notices the drive appear |
| A failure notification's click | `systemd-run --user` | the program started again directly |
| Mount as Folder, hooks, password command | available | shown as unavailable, with the reason (`sandbox-unavailable`) |
| Remembered passwords | the Secret Service | the same through `oo7`, which uses the Secret portal by itself when sandboxed |

## Permissions

Each needs its reason in the submission. These are the ones the code above
relies on, and no more:

| Permission | Why |
|---|---|
| `--share=network` | SFTP, REST and cloud destinations |
| `--share=ipc`, `--socket=wayland`, `--socket=fallback-x11`, `--device=dri` | The window |
| `--filesystem=host` | Back up and restore whatever folders the user chooses (Déjà Dup's precedent on Flathub) |
| `--filesystem=xdg-config/cosmic`, `--talk-name=com.system76.CosmicSettingsDaemon` | Follow the COSMIC theme; what every COSMIC Flatpak asks for |
| `--filesystem=xdg-config/autostart:create` | Start scheduled backups at login where the desktop has no Background portal |
| `--socket=ssh-auth` | SFTP through the user's SSH agent |
| `--talk-name=org.freedesktop.Notifications` | Failure and overdue notifications |
| `--system-talk-name=org.freedesktop.UDisks2` | Find removable drives |
| `--system-talk-name=org.freedesktop.UPower`, `--system-talk-name=org.freedesktop.NetworkManager` | A schedule's conditions (mains power, battery, metered, trusted networks) |

Not `--talk-name=org.freedesktop.systemd1`, `--filesystem=xdg-config/systemd`
or `--talk-name=org.freedesktop.Flatpak`: each lets a sandboxed app run
anything outside it, and nothing above needs them.

## Build

- Runtime `org.freedesktop.Platform`, SDK `org.freedesktop.Sdk` with the
  `rust-stable` extension, the version current at submission.
- Cargo dependencies as offline sources generated from `Cargo.lock` with
  `flatpak-cargo-generator.py` (`flatpak-builder-tools`); it handles the git
  dependencies. The build has no network.
- Build with `--features release-build`, as every other package does.
- rclone built from source as its own module (Go, vendored modules); Déjà
  Dup's Flathub manifest builds it the same way.
- Install both binaries, both desktop entries, both icons, both metainfo
  files and the license (to `share/licenses/$FLATPAK_ID`).
- Lint with `flatpak run --command=flatpak-builder-lint org.flatpak.Builder
  manifest …` and `… repo …`; Flathub treats warnings as failures.

## Checks before submitting

Run in the built Flatpak, on COSMIC:

1. Back up to a folder, a USB drive, an SFTP server and Google Drive; restore
   from each.
2. Remember a password, restart, and find it remembered.
3. Set a daily schedule: the autostart entry appears, the background process
   is running, the backup runs; log out and in, and it runs again when due.
4. Set "when the drive connects": plug the drive in, one backup runs.
5. Remove the schedule from every backup: the autostart entry goes at the
   next start of the window.
6. Mount as Folder, Hooks and the password source show as unavailable.
7. Change the theme; the window follows.

Then the same on GNOME, where the Background portal exists, to see the portal
path instead of the autostart file.

## The applet

Flathub refuses applets. The applet goes to `pop-os/cosmic-flatpak` once
`pop-os/cosmic-panel#694` (Flatpak 1.18.4 strips `X-CosmicApplet` from
exported desktop entries) is fixed. It reads the app's run state, so shipping
it as a separate Flatpak needs read access to the app's state directory, or
both binaries in one Flatpak; decide which before writing that manifest. Its
metainfo is `res/io.github.stldave314.Stellarshot.Applet.metainfo.xml`.

Settings do not carry over from a native install (a Flatpak keeps its own
under `~/.var/app`): Export Settings, then Import Settings in the Flatpak.
