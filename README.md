# Stellarshot

A backup application for the [COSMIC](https://system76.com/cosmic) desktop,
built on [rustic](https://rustic.cli.rs/).

Most people know they should back up and don't, because the tools ask too much.
The command-line tools are excellent and expect you to remember flags, write
cron jobs and read restore output. The friendly desktop tools let you keep
exactly one backup and make getting a single file back harder than it should be.

Stellarshot is heading towards a COSMIC-native backup app you set up once and
trust: several independent backups (your home folder to a USB drive, your
photos to Google Drive), each on its own schedule, and a restore process that
lets you browse any snapshot, see every version of a file, find what you
deleted, and preview a restore before anything is written.

Every backup is a standard **restic repository**. If this app disappeared
tomorrow, the `restic` and `rustic` command-line tools could still read every
snapshot you ever made.

![A backup and its snapshots, with Back Up Now](docs/screenshots/profile.png)

> **Status: early, and not yet something to trust with your only copy.** You
> can set up backups of folders to a USB drive, another folder, an SSH server
> or Google Drive, run them in the background, and manage their snapshots.
> Restore and schedules are still to come. Keep another backup, and follow the
> [3-2-1 rule](https://www.backblaze.com/blog/the-3-2-1-backup-strategy/).

**[Roadmap](ROADMAP.md)** · **[Changelog](CHANGELOG.md)** ·
**[Security](SECURITY.md)** · **[Validation](VALIDATION.md)** ·
**[Contributing](CONTRIBUTING.md)**

---

## What it does today

- **Several backups, each with its own settings.** "Home to the USB drive" and
  "Projects to the NAS folder" live side by side in the sidebar, each with its
  own folders, exclusions, destination and password.
- **A setup wizard.** Three steps: what to back up, where to keep it, and the
  password. It starts from your home folder with the usual clutter
  (`~/.cache`, the Trash, `~/Downloads`) already left out.
- **A size estimate you can trust.** The wizard shows how much will be backed up
  while you choose, with excluded folders **subtracted** from the folders they
  sit in. It is worked out from the very same file list the backup reads, so
  it matches what the backup processes, to the byte. Each included folder shows
  its own size, and each exclusion shows how much it takes out.
- **"Back Up Now" on the main screen**, with a progress card and **Cancel**.
  Backups run in their own process: the window never freezes, a cancelled or
  interrupted backup never leaves a half-written snapshot, and closing the
  window lets a running backup finish.
- **Passwords remembered in your keyring**, if you want (it is on by default).
  They go to the desktop's Secret Service (GNOME Keyring, KWallet) and nowhere
  else. Without it, the page asks once per session.
- **Status at a glance.** "Last backup 2 hours ago", where the backup is, how
  many snapshots it holds, and the most recent snapshots with their size and
  how much new data each added.
- **Two different ways to let go of a backup.** *Remove from Stellarshot*
  forgets it and leaves the data alone. *Delete backup and all data* deletes
  it, and only after you type the backup's name.
- **Deleting never touches anything else.** Only the entries the repository
  format creates are removed; other files in the same folder stay, symlinks are
  not followed, and nothing is deleted while another backup is writing.
- **Back up wherever suits you.** A folder, a **USB drive** (recognised by
  its ID, so it still works when it is mounted somewhere new), an **SSH
  server**, **Google Drive** (you sign in from Stellarshot), or any of **your
  own rclone remotes**: OneDrive, Dropbox, S3 and everything else rclone
  supports.
- **Imports Déjà Dup backups.** If you have used Déjà Dup, Stellarshot offers to
  bring its backup across, folders, exclusions and full history included, as
  long as it is in the restic format Déjà Dup uses for new backups. You type
  the password; Stellarshot never reads Déjà Dup's.
- **Opens existing backups.** Point it at a repository made by an earlier
  Stellarshot, by `restic` or by `rustic`, give the password, and it carries on
  with the same folders the last snapshot covered.
- **Incremental and deduplicated.** Unchanged files are not stored again, and
  identical data is stored once however many files contain it.
- **Your language.** English, Bulgarian, German, Swedish and Swiss German,
  following the desktop's language.

## What is coming

In the order it will land (the detail is in [ROADMAP.md](ROADMAP.md)):

1. **A complete restore**: browse snapshots, restore single files or folders,
   see every version of a file, find deleted files, compare two snapshots, and
   preview exactly what a restore will do.
2. **Automation**: scheduled backups, retention policies, notifications and
   periodic integrity checks.

---

## Requirements

- COSMIC desktop (Pop!\_OS 24.04 or newer, or any COSMIC session)
- The XDG desktop portal, which provides the folder chooser (installed with
  COSMIC)
- A Secret Service keyring (GNOME Keyring or KWallet) to remember passwords;
  optional
- A Rust toolchain (1.93 or newer) if building from source

[rclone](https://rclone.org/) is needed for SSH servers, Google Drive and
other cloud storage, and is recommended by the packages. Folders and USB drives
work without it.

## Installing

### Packages

`.deb`, `.rpm` and a portable tarball are attached to each
[release](../../releases).

```sh
sudo apt install ./stellarshot_*.deb      # Debian, Ubuntu, Pop!_OS
sudo dnf install ./stellarshot-*.rpm      # Fedora
```

### From source

Clone this repository, then:

```sh
./install.sh
```

`install.sh` builds the release binary and installs it with the desktop entry,
icons and AppStream metadata under `/usr`. It is the single build path for local
installs, packages and CI:

| Command | What it does |
| --- | --- |
| `./install.sh` | Build and install system-wide |
| `./install.sh build` | Build only |
| `./install.sh uninstall` | Remove an installed copy (settings and backups are kept) |
| `./install.sh deb` / `rpm` / `tarball` | Build one package into `dist/` |
| `./install.sh package` | Build all three |

Every build it makes passes `--features release-build`, so an installed or
packaged binary can never carry developer debug logging. Set `CARGO_JOBS=4` to
limit parallel compile jobs on a small machine.

---

## Using it

### Setting up a backup

On first launch, press **Create a Backup…** (or **File → New Backup…**,
<kbd>Ctrl</kbd>+<kbd>N</kbd>, or **New Backup** from the launcher's
right-click menu).

![Setting up a backup, with a live size estimate](docs/screenshots/wizard.png)

1. **What.** Your home folder is included, with `~/.cache`, the Trash and
   `~/Downloads` left out. **Add Folders…** under *Include* or *Exclude* adds
   more (you can pick several at once). The estimate at the top updates as you
   go. An exclusion that is not inside any included folder is marked as
   changing nothing. Under **Advanced** you can leave out names that match a
   pattern anywhere, such as `*.tmp` or `node_modules`, and choose whether to
   stay on the same drive (on by default, so a network share mounted inside
   your home folder is not swept up).
2. **Where.** Choose where the backup is kept, and give it a name:

   | Choice | What you provide |
   | --- | --- |
   | **Folder** | An empty folder. A folder on a USB drive is recognised as one automatically |
   | **Removable drive** | One of the drives plugged in now, and a folder on it (`Stellarshot/<computer name>` by default) |
   | **Network server (SFTP)** | Server, user name, port and folder. Uses your SSH agent or keys; the server must already be in `~/.ssh/known_hosts` |
   | **Google Drive** | **Sign In with Google…** opens your browser; then a folder in your Drive |
   | **One of your rclone remotes** | Pick a remote you set up with `rclone config` (OneDrive, Dropbox, S3, …) and a folder on it |

   Press **Check** (folders and drives are checked straight away). A location
   that already holds other files is refused; one that already holds a backup
   is pointed out, so you can open it instead.
3. **Password.** Choose one and confirm it. **Remember password** keeps it in
   your keyring.

**Create and Back Up Now** creates the backup and takes the first snapshot
straight away.

> **Remember the password.** The backup is encrypted with it. If it is lost,
> nobody can recover the backups, including you.

If the backup folder is inside one of the folders you back up — `~` backed up
to `~/Backups/home` — Stellarshot leaves the backup folder out automatically,
so a backup never copies itself.

### Opening an existing backup

**Open an existing backup** on the first screen asks where it is (any of the
places above) and its password. The folders to back up are taken from the most
recent snapshot, so the backup carries on as it was.

### Importing from Déjà Dup

When Déjà Dup's settings are found (the Flatpak or the native install), the
first screen shows **Import from Déjà Dup**; it is also in the **File** menu.
It opens the same steps with Déjà Dup's destination, folders and exclusions
filled in:

- **Google Drive:** sign in with the same Google account Déjà Dup used. The
  folder name is already filled in.
- **A USB drive:** the drive is picked out by its ID when it is plugged in.
- **A folder or an SSH server:** checked straight away or with **Check**.

Then enter the backup's password. Déjà Dup has always asked you to keep it
safe; Stellarshot does not read it from Déjà Dup or your keyring. Every
snapshot Déjà Dup made is there afterwards.

Only backups in the **restic** format can be imported, which is what current
Déjà Dup versions make. A backup in Déjà Dup's older duplicity format is
recognised and refused, and stays readable in Déjà Dup. Déjà Dup's own
settings are never changed; if its automatic backups are on, turn them off in
Déjà Dup so the two apps do not both back up the same folders.

### Backing up

Select the backup in the sidebar and press **Back Up Now**
(<kbd>Ctrl</kbd>+<kbd>B</kbd>). A progress card shows the phase and how much
has been stored; **Cancel** stops it, keeping nothing half-finished.

If the password is not remembered, the page asks for it first. Enter it once
and it is kept for the rest of the session (and in the keyring if you leave
**Remember password** on).

### Changing a backup

The **Manage** section at the bottom of each backup's page:

| Action | What happens |
| --- | --- |
| **What to back up → Edit** | Opens the first wizard step to change the included and excluded folders |
| **Remove from Stellarshot** | Forgets the backup and its remembered password. The data stays where it is and can be opened again later |
| **Delete backup and all data** | Permanently deletes every snapshot. You type the backup's name to confirm. Only the repository's own files are removed |

Individual snapshots are deleted with the bin icon on their row.

### Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| <kbd>Ctrl</kbd>+<kbd>N</kbd> | New backup |
| <kbd>Ctrl</kbd>+<kbd>B</kbd> | Back up the selected backup now |
| <kbd>Ctrl</kbd>+<kbd>,</kbd> | Settings |
| <kbd>Ctrl</kbd>+<kbd>I</kbd> | About |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd> | New window |
| <kbd>Ctrl</kbd>+<kbd>W</kbd> | Close the window |

### Command line

| Command | Effect |
| --- | --- |
| `stellarshot` | Open the window |
| `stellarshot --new-backup` | Open the window straight into the setup wizard (the launcher's **New Backup** action) |
| `stellarshot --run <operation>` | Internal: runs one backup, restore, check or snapshot deletion for the window, reading its job from stdin. Not meant to be run by hand |

### Reading your backups without Stellarshot

Every backup is a standard restic repository:

```sh
rustic -r /path/to/backup snapshots
restic -r /path/to/backup snapshots
restic -r /path/to/backup restore latest --target ~/restored
```

---

## Settings

| Setting | Default | What it does |
| --- | --- | --- |
| Theme | Match desktop | Follow the desktop's light or dark mode, or force one |

Each backup's own settings (folders, exclusions, destination) are edited on its
page. Everything is stored through `cosmic-config` in
`~/.config/cosmic/io.github.stldave314.Stellarshot/`. Passwords are never
stored there, only in the keyring when you ask.

---

## Troubleshooting

**"… already contains other files" when choosing where to keep a backup.**
Working as intended. Choose an empty folder, create a new one in the folder
chooser, or use **Open an existing backup** for a folder that already holds
one.

**My backups from an earlier Stellarshot show "Choose what to back up".**
Earlier versions kept a list of repositories with no idea what was in them.
Each one is now a backup with no folders chosen yet. Press **Choose
Folders…** to pick them; the repository and its snapshots are unchanged.

**An earlier Stellarshot created a backup in my home folder.**
Earlier versions accepted any folder. If you picked your home folder, it
contains `config`, `keys`, `data`, `index` and `snapshots` entries that belong
to the repository. It shows up in the sidebar; select it and use **Delete
backup and all data**, which removes only those entries and nothing else in
your home folder.

**My backups disappeared after updating.**
Settings are copied once from the upstream application ID
(`com.github.cosmic-utils.Stellarshot`) on first launch, and never over newer
settings. If you had already started this version before the copy could run,
copy them by hand and restart Stellarshot:

```sh
cp -r ~/.config/cosmic/com.github.cosmic-utils.Stellarshot/v1 \
      ~/.config/cosmic/io.github.stldave314.Stellarshot/
```

**The password is asked for every time.**
**Remember password** was off, or the keyring could not be reached. Check that
a Secret Service is running (`busctl --user list | grep org.freedesktop.secrets`)
and that your login keyring is unlocked. Stellarshot waits up to a minute for
the keyring, for example while you answer an unlock prompt, and then carries on
without it.

**"Backing up here needs rclone."**
SSH servers, Google Drive and other cloud storage go through rclone. Install
it (`sudo apt install rclone` on Debian, Ubuntu and Pop!\_OS) and go back to
the step.

**An SSH server is refused with a host key error.**
Stellarshot checks the server's key against `~/.ssh/known_hosts` and refuses
servers it does not know, so a changed or spoofed key is never trusted
silently. Connect once with `ssh user@server` and accept the key, then press
**Check** again.

**Google sign-in does not finish.**
Sign-in happens in your browser and hands the result back to Stellarshot on
this computer. If the browser shows an error or you close it, the step says so
and you can try again. The sign-in is kept only in
`~/.config/stellarshot/rclone.conf`, which only you can read.

**"… cannot be reached" for a USB drive.**
The drive is not plugged in (or not mounted). Plug it in; Stellarshot finds it
wherever it is mounted.

**"Another backup is already using this repository."**
Only one process may write to a backup at a time. Wait for the other one to
finish. If none is running, nothing is holding the lock either: it is released
automatically when a process ends, even when it crashes.

**"The password is incorrect."**
The password is the one set when the backup was created. There is no way to
recover or reset it.

**Something else is wrong.**
Turn on developer logging: set `DEVELOPER_LOGGING` to `true` in
`src/debug.rs`, rebuild *without* `--features release-build`, reproduce the
problem, and read `/tmp/stellarshot-debug.log`. Lines are tagged by category
(`ENGINE`, `UI`, `CONFIG`) so you can `grep` a run. Genuine errors are always
written to stderr too.

---

## Known limitations

- **OneDrive has no sign-in button of its own yet.** Set it up once with
  `rclone config` and choose it under **One of your rclone remotes**.
- **Google sign-in uses rclone's shared Google client**, which Google limits in
  how fast it may be used; very large first backups can be slower than with
  your own client ID.
- **No schedule yet.** Backups run when you press **Back Up Now**.
- **No restore in the app yet.** Use `restic restore` or `rustic restore` (see
  [Reading your backups](#reading-your-backups-without-stellarshot)).
- **Old snapshots are kept until you delete them.** Retention policies come
  with scheduling. Deleting a snapshot does not free its space until the
  repository is pruned, which also comes with scheduling.
- **Some translations are machine-assisted.** Strings added recently were
  translated without review by native speakers. `tests/i18n.rs` proves every
  locale has the same keys and placeholders as English, but not that the
  wording is right. Corrections are welcome.

---

## How it works

```
  ┌────────────────────────────────────┐        job on stdin (JSON,
  │  window  (libcosmic)               │        including the password)
  │  sidebar of backups, profile page, │ ───────────────────────────┐
  │  setup wizard, dialogs             │                            │
  └──────┬─────────────────────────────┘                            ▼
         │ reads on blocking threads          ┌──────────────────────────────┐
         │ (open, list, estimate, probe)      │  stellarshot --run backup    │
         │                                    │  one write, in its own       │
         │ ◀──── progress and outcome ─────── │  process; holds the lock     │
         │       as JSON lines on stdout      └──────────────┬───────────────┘
         ▼                                                   ▼
  ┌──────────────────────────────────────────────────────────────────────────┐
  │  engine: the only code that talks to rustic                              │
  │  open · init · probe · estimate · snapshots · backup · restore · check   │
  └──────────────────────────────────┬───────────────────────────────────────┘
                                     ▼
                    rustic_core → restic-format repository

  keyring ── Secret Service (GNOME Keyring, KWallet), one item per backup
  rclone  ── SSH servers and cloud storage, with Stellarshot's own rclone.conf
```

Writes (backup, restore, check, deleting snapshots) run in a child process,
`stellarshot --run <operation>`, because rustic cannot be interrupted once an
operation starts and a process can. The password reaches the child on stdin,
never in its command line or environment, both of which other programs
running as you can read.

| Module | Responsibility |
| --- | --- |
| `profile` | A backup profile: folders, exclusions, destination; turning it into what the engine needs |
| `engine` | Everything that touches rustic: repositories, backup, restore, checks, snapshots, the size estimate. Synchronous, plain types, typed errors |
| `engine::location` | Whether a folder may hold a repository; deleting only a repository's own entries |
| `engine::lock` | One writer per repository, shared by every process; released by the kernel if the holder dies |
| `engine::rclone` | The rclone commands besides the backup itself: probing a remote folder, deleting a repository's entries, signing in, copying one of your remotes |
| `engine::estimate` | The size of a backup before it runs, from the same file list the backup reads |
| `drives` | Mounted removable drives, and where a drive with a given ID is mounted now |
| `dejadup` | Reading Déjà Dup's settings (never its password) and turning them into a backup |
| `runner` | `stellarshot --run`: reads a job from stdin, runs it under the lock, reports JSON lines |
| `keyring` | Remembered passwords in the Secret Service, each request bounded by a timeout |
| `app` | The window: sidebar, menus, dialogs, settings |
| `app::pages` | The first-launch screen and each backup's page |
| `app::wizard` | The setup wizard's steps and validation; `place` is the "where" step |
| `app::tasks` | Engine calls off the UI thread, the folder chooser, the estimate as a stream |
| `app::child` | Spawns `--run`, streams its events into the UI, cancels it |
| `app::errors` | A localized explanation for every kind of engine error |
| `app::migrate` | One-time moves of settings from older versions |

---

## Building

```sh
cargo build --release --features release-build
cargo test
cargo clippy --all-targets --all-features
```

Both `cargo check` and `cargo clippy` are warning-free, and CI enforces it with
`RUSTFLAGS=-D warnings`. `tests/keyring.rs` needs a running Secret Service and
fails without one; CI provides it with an unlocked gnome-keyring. What each
check proves, and how each one is kept from passing when it has nothing to
check, is in [VALIDATION.md](VALIDATION.md).

The screenshots are made from the real application by
`scripts/screenshots.sh`, with a demo home folder and demo backup so no
personal data appears in them.

## Translations

English, Bulgarian, German, Swedish and Swiss German. Each locale lives in
`i18n/<locale>/stellarshot.ftl`, and `tests/i18n.rs` fails the build if any
locale has a missing, extra or duplicated key, or a changed `{ $placeholder }`.
See [CONTRIBUTING.md](CONTRIBUTING.md#translations).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md). Security problems go through
[SECURITY.md](SECURITY.md), not the public issue tracker.

## Credits

Stellarshot was created by **Aaron Honeycutt** and **Eduardo Flores** in the
[cosmic-utils](https://github.com/cosmic-utils) organisation, with translations
from its contributors. This project continues from their work.

Backups are made by [rustic](https://rustic.cli.rs/), a Rust implementation of
the [restic](https://restic.net/) repository format.

## Licence

GPL-3.0-only. See [LICENSE](LICENSE).
