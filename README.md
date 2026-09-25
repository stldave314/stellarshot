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
> or Google Drive, run them on a schedule, manage their snapshots, and get
> files back. Keep another backup, and follow the
> [3-2-1 rule](https://www.backblaze.com/blog/the-3-2-1-backup-strategy/).

**[Roadmap](ROADMAP.md)** · **[Changelog](CHANGELOG.md)** ·
**[Security](SECURITY.md)** · **[Validation](VALIDATION.md)** ·
**[Contributing](CONTRIBUTING.md)**

---

## What it does today

- **Several backups, each with its own settings.** "Home to the USB drive" and
  "Projects to the NAS folder" live side by side in the sidebar, each with its
  own folders, exclusions, destination and password.
- **A setup wizard.** Four steps: what to back up, where to keep it, when it
  runs, and the password. It starts from your home folder with the usual clutter
  (`~/.cache`, the Trash, `~/Downloads`) already left out.
- **A size estimate you can trust.** The wizard shows how much will be backed up
  while you choose, with excluded folders **subtracted** from the folders they
  sit in, and the sum written out: *45 GB included − 12 GB excluded = 33 GB*.
  It is worked out from the very same file list the backup reads, so it
  matches what the backup processes, to the byte. Each included folder shows
  its own size, and each exclusion shows how much it takes out.
- **"Back Up Now" on the main screen**, with a progress card and **Cancel**.
  The card counts its running time, shows how much has reached cloud storage,
  and says what it is waiting for when the figures stand still. Backups run
  in their own process: the window never freezes, **Cancel** stops the backup
  and the rclone connection under it at once, a canceled or interrupted
  backup never leaves a half-written snapshot, and closing the window lets a
  running backup finish.
- **Passwords remembered in your keyring**, if you want (it is on by default).
  They go to the desktop's Secret Service (GNOME Keyring, KWallet) and nowhere
  else. Without it, the page asks once per session.
- **Status at a glance.** "Last backup 2 hours ago", where the backup is, how
  many snapshots it holds, and the most recent snapshots with their size and
  how much new data each added. **Pin** a snapshot ("before the upgrade") so
  cleaning up never removes it, however old it gets.
- **Two different ways to let go of a backup.** *Remove from Stellarshot*
  forgets it and leaves the data alone. *Delete backup and all data* deletes
  it, and only after you type the backup's name.
- **Deleting never touches anything else.** Only the entries the repository
  format creates are removed; other files in the same folder stay, symlinks are
  not followed, and nothing is deleted while another backup is writing.
- **Back up wherever suits you.** A folder, a **USB drive** (recognized by
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
- **Getting files back without the command line.** Browse any snapshot as a
  folder tree, search it, and see every version of a file, with identical
  versions marked. **Deleted files** lists what your backups still have that is
  no longer on disk, and **Compare** shows what changed between two snapshots.
  **Download…** saves a file, an older version of one, or a whole folder as a
  `.tar.gz`, straight from the snapshot, without restoring anything.
- **A restore that shows you what it will do first.** Put files back where
  they were or in another folder. When a file is already there, choose **Keep
  both** (the default: your file is not touched and the restored copy gets a
  dated name), **Overwrite** or **Skip**. A dry run counts what will be
  restored, replaced, kept alongside or left alone before anything is written.
- **Backups that run on their own.** Hourly, daily or weekly, through a
  systemd timer, whether or not Stellarshot is open. A backup missed while the
  computer was off runs as soon as you are back. If one fails, a notification
  says so and a click opens the backup; an unplugged drive is simply tried
  again at the next slot.
- **Old snapshots cleaned up for you.** Keep a smart history (the newest
  snapshot of each of the last 7 days, 4 weeks and 12 months that have one;
  [exactly how](#how-smart-decides-what-to-keep)) or everything from the last
  3 months, 6 months or year. Space no snapshot needs is freed
  automatically where that is safe, and the repository is checked for damage
  every 30 days.
- **Incremental and deduplicated.** Unchanged files are not stored again, and
  identical data is stored once however many files contain it.
- **Your language.** English, Bulgarian, German, Swedish and Swiss German,
  following the desktop's language.

## What is coming

A clearer picture of every backup, finer control over what is backed up,
more storage options and alerts beyond the desktop, on the way to 1.0. The
detail is in [ROADMAP.md](ROADMAP.md).

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
   more (you can pick several at once). **Browse…** on an included folder
   opens a disk-usage tree rooted there instead: every row sized as it is
   expanded, marked **Included**, **Excluded**, or **Partly included**, with
   a button to flip it — an easier way to find what is actually taking up
   space than guessing folder names ahead of time. The estimate at the top
   updates as you go, and once it has counted, it writes out the sum: what the
   included folders hold, minus what the exclusions take out, equals the
   backup. An exclusion that is not inside any included folder is marked as
   changing nothing. Under **Advanced** you can leave out names that match a
   pattern anywhere, such as `*.tmp` or `node_modules`; choose whether to
   stay on the same drive (on by default, so a network share mounted inside
   your home folder is not swept up); leave out any folder tagged as
   disposable cache data; honor each project's own `.gitignore`; and skip
   recording a snapshot at all when nothing has changed since the last one.
   Patterns left out of every backup at once, rather than just this one, are
   set under [Settings](#settings).
2. **Where.** Choose where the backup is kept, and give it a name:

   | Choice | What you provide |
   | --- | --- |
   | **Folder** | An empty folder. A folder on a USB drive is recognized as one automatically |
   | **Removable drive** | One of the drives plugged in now, and a folder on it (`Stellarshot/<computer name>` by default) |
   | **Network server (SFTP)** | Server, user name, port and folder. Uses your SSH agent or keys; the server must already be in `~/.ssh/known_hosts` |
   | **Google Drive** | **Sign In with Google…** opens your browser; then a folder in your Drive. **Use my own Google API credentials…** lets you sign in with a Google Cloud client of your own instead of the one rclone shares with everyone who has not set one up |
   | **One of your rclone remotes** | Pick a remote you set up with `rclone config` (OneDrive, Dropbox, S3, …) and a folder on it |
   | **REST server** | A [rest-server](https://github.com/restic/rest-server) or [rustic-server](https://github.com/rustic-rs/rustic_server) you run yourself, as a URL including the repository name and any credentials (`http://user:pass@host:8000/repo/`) |

   A server, Google Drive or rclone remote also gets an **Advanced** section
   with a bandwidth limit, in rclone's own syntax (`1M`, or `8M:2M` for
   upload:download), left empty for no limit.

   **Next** checks the location and moves on as soon as the check passes; the
   button counts the seconds while it waits (folders and drives are checked
   straight away, and **Check** checks without moving on). A check that gets
   no answer within a minute stops and says so, and **Next** tries again. A
   location that already holds other files is refused; one that already
   holds a backup is pointed out, so you can open it instead.
3. **When.** **Back up automatically** is on, daily, by default; choose
   hourly or weekly, or turn it off. Backing up to a removable drive offers a
   4th choice instead: **When its drive is connected**, which runs as soon as
   the drive is plugged in rather than on a fixed schedule. **Keep** decides
   which old snapshots are forgotten:

   | Keep | What stays |
   | --- | --- |
   | **Smart** (default) | The newest snapshot of each of the last 7 days, 4 weeks and 12 months that have one ([exactly how](#how-smart-decides-what-to-keep)) |
   | **At least 3 months / 6 months / a year** | Every snapshot from that long before the newest one |
   | **Forever** | Everything; the backup only grows |

   **Free up space automatically** deletes data no remaining snapshot needs.
   It is on for folders and drives on this computer and off for servers and
   cloud storage, which another computer may be backing up to at the same
   time.

   While automatic backups are on, a **Conditions** section lets a laptop
   skip a slot rather than run in a state you would not want it to: only on
   mains power, only above a battery level, not on a connection marked
   metered, or only on a trusted Wi-Fi network or a VPN (Tailscale,
   WireGuard, or another). A slot skipped this way is quiet, the same as a
   destination that is not reachable; the next one tries again.
4. **Password.** Choose one and confirm it. **Remember password** keeps it in
   your keyring. Automatic backups need it remembered: they run when nobody
   is there to type it. While the backup is being created the button counts
   the seconds; on cloud storage this can take a minute.

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
recognized and refused, and stays readable in Déjà Dup. Déjà Dup's own
settings are never changed; if its automatic backups are on, turn them off in
Déjà Dup so the two apps do not both back up the same folders.

### Backing up

Select the backup in the sidebar and press **Back Up Now**
(<kbd>Ctrl</kbd>+<kbd>B</kbd>). A progress card shows the phase, how much has
been read, and how long it has been running; **Cancel** stops it, keeping
nothing half-finished.

For SSH servers and cloud storage the card also shows how much has been
stored there. That figure is smaller than what has been read, because data is
compressed and anything already in the backup is not sent again. Four pieces
of the backup are uploaded at once, so a slow connection is kept busy.

If the figures stand still for 15 seconds, the card says so and why: at the
start, the backup is reading what the repository already holds, which takes a
while on cloud storage; later, the destination is slow to accept data or is
limiting requests. The backup carries on by itself either way.

If the password is not remembered, the page asks for it first. Enter it once
and it is kept for the rest of the session (and in the keyring if you leave
**Remember password** on).

### Getting files back

On a backup's page, press **Restore…** (or choose **Restore Files** from the
launcher's right-click menu, which opens the selected backup's restore page as
soon as it is unlocked).

![Browsing a snapshot to restore files](docs/screenshots/restore.png)

| Tab | What it is for |
| --- | --- |
| **Browse** | Pick a snapshot from the list (newest first) and move through its folders. **Search this snapshot** finds names anywhere in it. Every row has a **Download…** button, saving that file or folder as it is in the chosen snapshot without restoring it. Click a file to see every snapshot that has it; versions identical to the one above them are marked, so you can see when it actually changed. **Open Copy** opens one version read-only without restoring it; **Download…** saves that one version; **Restore This Version…** restores just that one |
| **Deleted files** | Files under a folder (your first backed-up folder unless you choose another) that are in a backup from the last 30 days but no longer on disk. Each comes back from the newest snapshot that still has it |
| **Compare** | Choose two snapshots to list what was added, removed and changed between them. Ticking a changed or removed item restores it as it was in the snapshot on the left |

Tick the files and folders you want and press **Restore…**. Then choose:

- **Restore to:** *Where they were*, or *Another folder…* (each item lands
  in it under its own name).
- **If a file already exists:**

  | Choice | What happens to your file | What happens to the backed-up copy |
  | --- | --- | --- |
  | **Keep both** (default) | Not touched | Restored next to it as `name (restored 2026-09-23).ext` |
  | **Overwrite** | Replaced | Restored in its place |
  | **Skip** | Not touched | Not restored |

- **Advanced:** verify existing files by content instead of trusting their
  size and date (slower, but catches a file that changed without its date
  moving); and whether to restore ownership as it was backed up, as numeric
  IDs (for restoring onto another machine or user where the names would not
  mean the same accounts), or not at all.

Before anything is written, a dry run shows how many files will be restored
and how many existing ones will be kept alongside, replaced or skipped.
Files that are already identical are never rewritten. **Restore** stays
unavailable until the dry run for your current choices has finished. The
restore then runs in the background with progress and **Cancel**.

Restoring never deletes anything: files that are on disk but not in the
snapshot are left alone.

**Open Copy** restores the one file into a private folder in your session's
runtime directory (`$XDG_RUNTIME_DIR`, which is cleared when you log out),
makes it read-only, and opens it with its usual application.

### Automatic backups

A scheduled backup runs `stellarshot --scheduled <id>` from a systemd user
timer (or, for **When its drive is connected**, a systemd path unit watching
for the drive instead), in the background and at low priority, whether or not
the window is open. It backs up, forgets old snapshots under the **Keep**
setting, checks the repository if the last check was 30 days ago or more, and
then frees space if that is turned on.

- **Missed runs catch up.** If the computer was off or asleep, an hourly,
  daily or weekly backup runs shortly after you log in. A drive-connected
  backup has nothing to catch up on; it simply runs the next time the drive
  is plugged in.
- **An unplugged drive or no network** is not an error: the run is skipped and
  the next slot tries again. The page shows how long ago the last backup was.
- **A laptop condition that is not met** (on battery, a metered connection, an
  untrusted network) is skipped the same quiet way.
- **A real failure** (a password that is no longer remembered, a full disk)
  raises a notification. Clicking it opens the backup, which shows what went
  wrong and a button to try again. The sidebar marks the backup with a
  warning until it succeeds.
- **Only this computer's snapshots are ever forgotten.** If another computer
  backs up to the same place, its snapshots are left to its own settings.
- **Damage found by a check** pauses freeing space until a check passes, and
  is shown on the page with **Check Again**.

Only one thing writes to a backup at a time: if you press **Back Up Now**
while a scheduled backup is running, you are told it is busy.

### The panel applet

Add **Stellarshot** from COSMIC Settings' panel applet list for a status icon:
plain when everything is fine, a sync icon while a backup (scheduled or
started from the window) is running, and a warning icon if one has failed or
fallen overdue. Its popup lists every backup with when each last succeeded,
and an **Open Stellarshot** button.

Closing the window minimizes it to the panel rather than quitting — the
process, and any backup in progress, keeps running. Opening Stellarshot again,
from the applet or the launcher, brings the same window back rather than
starting a second one.

### Hooks

A backup can run a command or program at four points:

| Timing | When |
| --- | --- |
| **Before the backup** | Before anything is read. A failure here stops the backup from running at all |
| **After a successful backup** | Once the backup has finished cleanly |
| **After a failed backup** | Once the backup has failed |
| **After the backup, either way** | Always, once the backup has finished |

Stop a database before it runs and start it again after, or unmount a network
share once a backup to it is done, for example. A hook's command line is split
the same way the password command under **How the password is provided** is:
without invoking a real shell, so it is never subject to shell injection — a
small wrapper script covers a pipe or another shell operator if one is
needed. A hook that runs longer than two minutes is killed and treated as a
failure, so a stuck one cannot hang a backup forever.

An **After** hook's own failure is recorded in the log but does not undo an
already-finished backup or turn its success into a reported failure.

### How "Smart" decides what to keep

A backup is only as trustworthy as the rules that delete from it, so here
they are in full. After each automatic backup, and on **Clean Up Now**,
Smart keeps:

1. **Days.** The newest snapshot of each of the last **7 days that have one**.
   A day with no backup is passed over, not counted: after a week away, the 7
   days kept are the 7 most recent days you did back up.
2. **Weeks.** The newest snapshot of each of the last **4 weeks that have
   one**. Weeks run Monday to Sunday.
3. **Months.** The newest snapshot of each of the last **12 calendar months
   that have one**.
4. **The very first snapshot**, for as long as the backups span fewer than 12
   calendar months.

The newest snapshot of a day is the one taken latest that day; earlier ones
the same day are not kept by these rules. The rules overlap: today's newest
snapshot is also this week's and this month's, so it counts for all three,
and Smart keeps at most 23 snapshots, usually fewer. Every snapshot none of
the rules keeps is **forgotten**: removed from the list, and its data freed
by the next clean-up if **Free up space automatically** is on. Days, weeks and
months are those of the time each snapshot was taken, in the time zone it was
taken in. A week that runs across New Year is counted as two, one in each
year, as rustic counts it.

Two limits keep the rules from reaching further than they should:

- **Only this computer's snapshots.** Another computer backing up to the
  same place is never touched.
- **Each set of folders on its own.** Snapshots are grouped by the folders
  they back up. Changing the folders starts a new group with its own 7 days,
  4 weeks and 12 months, and the old group keeps its snapshots under the same
  rules.

The rules are rustic's own `keep-daily 7`, `keep-weekly 4` and
`keep-monthly 12`, so `rustic forget --keep-daily 7 --keep-weekly 4
--keep-monthly 12 --dry-run` shows exactly what the next clean-up would
forget. `src/engine/maintenance.rs` has tests for each rule above.

### Changing a backup

The **Manage** section at the bottom of each backup's page:

| Action | What happens |
| --- | --- |
| **When it runs → Change…** | Automatic backups on or off, how often, what to keep, and whether to free space |
| **Hooks → Change…** | Commands or programs run before and after a backup — see [Hooks](#hooks) below |
| **Check for damage → Check Now** | Verifies every snapshot, folder and index entry. It shows when it last ran |
| **Free up space → Clean Up Now** | Forgets snapshots **Keep** no longer needs and deletes data nothing uses. It cannot be stopped once started |
| **What to back up → Edit** | Opens the first wizard step to change the included and excluded folders |
| **How the password is provided → Change…** | The keyring (default), or a command that prints the password on its standard output, run fresh every time one is needed — for a password manager with a command-line client, such as the Bitwarden CLI |
| **Password → Change…** | Needs the backup unlocked already. Adds a key for the new password, then removes the one you unlocked it with; updates the keyring entry too, if it was remembered |
| **Remove from Stellarshot** | Forgets the backup and its remembered password. The data stays where it is and can be opened again later |
| **Delete backup and all data** | Permanently deletes every snapshot. You type the backup's name to confirm. Only the repository's own files are removed |

Individual snapshots are deleted with the bin icon on their row; the pin icon
next to it keeps one however old it gets, until unpinned.

### Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| <kbd>Ctrl</kbd>+<kbd>N</kbd> | New backup |
| <kbd>Ctrl</kbd>+<kbd>B</kbd> | Back up the selected backup now |
| <kbd>Ctrl</kbd>+<kbd>,</kbd> | Settings |
| <kbd>Ctrl</kbd>+<kbd>I</kbd> | About |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd> | New window — a genuinely separate one, not just refocusing this one |
| <kbd>Ctrl</kbd>+<kbd>W</kbd> | Close the window (minimizes to the panel; see [The panel applet](#the-panel-applet)) |

### Command line

| Command | Effect |
| --- | --- |
| `stellarshot` | Open the window, or bring an already-running one to the front |
| `stellarshot --new-backup` | Open the window straight into the setup wizard (the launcher's **New Backup** action) |
| `stellarshot --restore` | Open the selected backup's restore page as soon as it is unlocked (the launcher's **Restore Files** action) |
| `stellarshot --profile <id>` | Open the window on one backup (what clicking a failure notification does) |
| `stellarshot --scheduled <id>` | Run one backup as its timer does: back up, forget, check if due, free space. Exits 0 when skipped because the destination is unreachable or a laptop condition is not met |
| `stellarshot-applet` | The panel applet; run by the panel itself, not normally launched directly |
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
| Left out of every backup | None | Glob patterns, such as `node_modules` or `target`, left out of every backup without adding them to each one |
| Cache location | rustic's own default (`~/.cache/rustic`) | Another folder, or no local cache at all, for every repository this computer opens |

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

**Automatic backups do not run.**
Check the timer: `systemctl --user list-timers 'stellarshot-*'` should list one
per scheduled backup, with the next run. If it is missing, open Stellarshot:
it sets up the timers for every scheduled backup each time it starts. The
last run's output is in `journalctl --user -u 'stellarshot-backup-*'`. Timers
run only while you are logged in; a slot missed while logged out runs at the
next login.

**"Scheduled backups need the password remembered."**
Open the backup, and enter its password with **Remember password** on. The
next automatic backup will run.

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

**Checking a cloud location says there was no answer.**
The check waits up to a minute. Google Drive in particular slows down apps
that make many requests, and rclone then waits and retries. Wait a moment and
press **Next** again. If it keeps happening, check that `rclone lsf
<remote>:` works in a terminal with Stellarshot's configuration
(`--config ~/.config/stellarshot/rclone.conf`).

**A cloud backup is slower than I expected.**
The first backup uploads everything, and cloud storage limits how fast one
app may send requests (see *Known limitations*). Stellarshot uploads four
pieces at once and sends Google Drive each piece in one go. Later backups
only send what changed.

**Clean Up Now freed no space on Google Drive.**
Data Stellarshot deletes from Google Drive is deleted permanently, not moved
to Drive's trash, so the space is freed straight away. Two exceptions: data
no snapshot needs is only deleted by a clean-up a day or more after it was
first found unused (see *Known limitations*), and clean-ups by earlier
versions moved data to the trash, which you can empty in Google Drive.

**An SSH server is refused with a host key error.**
Stellarshot checks the server's key against `~/.ssh/known_hosts` and refuses
servers it does not know, so a changed or spoofed key is never trusted
silently. Connect once with `ssh user@server` and accept the key, then press
**Next** again.

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

**Restore… is grayed out.**
The backup needs to be unlocked (the password entered or remembered), hold at
least one snapshot, and not be in the middle of a backup.

**"Nothing is missing" in Deleted files, but I deleted something.**
It only looks under the folder shown at the top of the tab, and only in
backups from the last 30 days. Press **Change Folder…** to look somewhere else,
or find the file under **Browse** in an older snapshot.

**"The password is incorrect."**
The password is the one set when the backup was created. There is no way to
recover or reset it.

**Cancel does not seem to stop a backup.**
Cancel stops the backup's process and everything it started, including the
`rclone` that carries SSH and cloud backups. If a backup keeps going after
**Cancel**, it was probably started by the timer rather than by the window:
the window can only stop what it started. Stop a timer's run with
`systemctl --user stop 'stellarshot-backup-*'`.

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
- **Google sign-in uses rclone's shared Google client by default**, which
  Google limits in how fast it may be used; very large first backups can be
  slower than with your own client ID. Déjà Dup has a Google client of its
  own, which is one reason its Google Drive backups can be faster. **Use my
  own Google API credentials…** under Google Drive in the wizard lets you
  give Stellarshot a client ID and secret from your own Google Cloud project
  (console.cloud.google.com, with the Drive API enabled), so this backup's
  traffic no longer shares that quota with the rest of rclone's users. rclone
  itself is retiring its shared client during 2026, so this will move from
  optional to necessary; see [ROADMAP.md](ROADMAP.md) for a client bundled
  with Stellarshot itself, so most people never have to do this by hand.
- **Timers need systemd.** On a system without a systemd user session,
  automatic backups cannot be set up; **Back Up Now** still works.
- **Search, Deleted files and Compare show at most 500 entries.** Narrow the
  search, choose a smaller folder, or compare snapshots closer together.
- **Opening the restore page reads the whole index first,** which can take a
  while for a large backup on a slow connection.
- **Freed space can take a day to appear.** Data no snapshot needs is first
  marked, and deleted by a later clean-up at least 23 hours on, so a backup
  running elsewhere at the same moment cannot lose data it still refers to.
- **Some translations are unreviewed.** Strings added recently have not yet
  been reviewed by native speakers. `tests/i18n.rs` proves every
  locale has the same keys and placeholders as English, but not that the
  wording is right. Corrections are welcome.

---

## How it works

```
  ┌────────────────────────────────────┐        job on stdin (JSON,
  │  window  (libcosmic)               │        including the password)
  │  sidebar of backups, profile page, │ ───────────────────────────┐
  │  restore page, setup wizard        │                            │
  └──────┬─────────────────────────────┘                            ▼
         │ reads on blocking threads          ┌──────────────────────────────┐
         │ (open, browse, estimate, probe,    │  stellarshot --run backup    │
         │  dry-run restore)                  │  one write, in its own       │
         │ ◀──── progress and outcome ─────── │  process; holds the lock     │
         │       as JSON lines on stdout      └──────────────┬───────────────┘
         ▼                                                   ▼
  ┌──────────────────────────────────────────────────────────────────────────┐
  │  engine: the only code that talks to rustic                              │
  │  open · init · probe · estimate · snapshots · browse · backup · restore  │
  └──────────────────────────────────┬───────────────────────────────────────┘
                                     ▼
                    rustic_core → restic-format repository

  keyring ── Secret Service (GNOME Keyring, KWallet), one item per backup
  rclone  ── SSH servers and cloud storage, with Stellarshot's own rclone.conf
  timers  ── systemd user timers run `stellarshot --scheduled <id>`, which
             backs up through the same runner as the window's child process
  applet  ── stellarshot-applet, a separate process the panel runs. Reads
             each backup's status straight off disk (run history, and
             whether something holds its repository's lock), the same way
             the window itself does; no D-Bus link to the window at all
```

Writes (backup, restore, check, clean-up, deleting snapshots) run in a child
process, `stellarshot --run <operation>`, because rustic cannot be interrupted
once an operation starts and a process can. Canceling kills the child's whole
process group, so the `rclone` that rustic starts for SSH and cloud storage
stops with it. The password reaches the child on stdin,
never in its command line or environment, both of which other programs
running as you can read.

| Module | Responsibility |
| --- | --- |
| `profile` | A backup profile: folders, exclusions, destination; turning it into what the engine needs |
| `engine` | Everything that touches rustic: repositories, backup, restore, checks, snapshots, the size estimate. Synchronous, plain types, typed errors |
| `engine::location` | Whether a folder may hold a repository; deleting only a repository's own entries |
| `engine::lock` | One writer per repository, shared by every process; released by the kernel if the holder dies |
| `engine::rclone` | The rclone commands besides the backup itself: probing a remote folder (with a time limit), deleting a repository's entries, signing in, copying one of your remotes |
| `engine::uploads` | Several pack uploads at once for SSH and cloud storage, without letting an index or snapshot be written before every pack it names has arrived |
| `engine::browse` | Looking inside snapshots with the index loaded once: folders, search, versions of a file, comparing two snapshots, deleted files |
| `engine::restore` | Selected files to their original place or a folder, with the Keep both, Overwrite or Skip decision made before rustic sees the file list, and a dry run that counts the same way |
| `engine::estimate` | The size of a backup before it runs, from the same file list the backup reads |
| `drives` | Mounted removable drives, and where a drive with a given ID is mounted now |
| `dejadup` | Reading Déjà Dup's settings (never its password) and turning them into a backup |
| `runner` | `stellarshot --run`: reads a job from stdin, runs it under the lock, reports JSON lines |
| `hooks` | Runs a backup's hooks: a `Before` failure stops the backup, an `After` failure is only logged |
| `engine::maintenance` | Checks, forgetting by retention rules (this computer's snapshots only) and pruning |
| `schedule` | Writing, enabling and removing each scheduled backup's systemd timer or path unit, and keeping them in line with the settings |
| `scheduled` | `stellarshot --scheduled`: a timer's run, from backup to check and clean-up, and what is worth a notification |
| `conditions` | Whether a laptop's power, battery and network state satisfy a scheduled backup's conditions; reading the real state (UPower, NetworkManager) and deciding are kept apart |
| `run_state` | What happened when each backup last ran on its own, in cosmic-config's state store |
| `status` | Each backup's status from what any process can see on disk: run history, and whether its repository lock is currently held. Shared by the window (a run it did not itself start) and the applet |
| `notify` | Desktop notifications, and opening the backup when one is clicked |
| `keyring` | Remembered passwords in the Secret Service, each request bounded by a timeout |
| `app` | The window: sidebar, menus, dialogs, settings |
| `app::applet` | The panel applet: `stellarshot-applet`'s own window, sharing the library but nothing else with the main window |
| `app::pages` | The first-launch screen, each backup's page, and the restore page |
| `app::wizard` | The setup wizard's steps and validation; `place` is the "where" step |
| `app::tasks` | Engine calls off the UI thread, the folder chooser, the estimate as a stream |
| `app::child` | Spawns `--run` in its own process group, streams its events into the UI, cancels it and everything it started |
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
[cosmic-utils](https://github.com/cosmic-utils) organization, with translations
from its contributors. This project continues from their work.

Backups are made by [rustic](https://rustic.cli.rs/), a Rust implementation of
the [restic](https://restic.net/) repository format.

## License

GPL-3.0-only. See [LICENSE](LICENSE).
