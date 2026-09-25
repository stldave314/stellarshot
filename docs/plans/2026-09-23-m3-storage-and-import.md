# M3: Storage locations and Déjà Dup import, implementation plan

**Goal:** Back up to more than a local folder:
- **USB drives**, recognized by their filesystem UUID wherever they're mounted.
- **SFTP servers.**
- **Google Drive**, with the sign-in done inside Stellarshot.
- **Any rclone remote** the user already has.

It also covers importing a Déjà Dup backup (restic format only; passwords are never imported).

**Architecture:**
- **Two location kinds.** `engine::Location` becomes an enum: `Local { path }` and `Rclone { remote, path }`.
  - rclone locations go through `rustic_backend`'s `rclone` feature. That starts `rclone serve restic`, and we pass it `--config <Stellarshot's own rclone.conf>`, so the user's `~/.config/rclone/rclone.conf` is never read or written.
  - `src/engine/rclone.rs` runs the few rclone commands the app needs: version, listing a location (probe), deleting a repository's entries, signing in, listing and copying the user's remotes.
- **Drives.** `src/drives.rs` finds mounted removable drives and resolves a UUID to a mount point from `/dev/disk/by-uuid` and `/proc/self/mountinfo`.
- **Déjà Dup.** `src/dejadup.rs` reads Déjà Dup's settings (the Flatpak keyfile, or `dconf dump` for a native install) and maps them to a profile.

**Spec:** §4 (Destination), §7 (Déjà Dup import)

## Global constraints

- **Stellarshot's rclone config** is `$XDG_CONFIG_HOME/stellarshot/rclone.conf`, created with mode 0600. It's always passed explicitly with `--config`, never through the environment.
- **SFTP verifies host keys.** It uses `known_hosts_file=~/.ssh/known_hosts`, so an unknown or changed host key fails rather than being trusted silently. Authentication is the SSH agent or the user's keys. Stellarshot never stores an SSH password.
- **Google sign-in** runs `rclone config create <name> drive scope=drive`. rclone opens the browser and receives the token on localhost. The token is stored only in Stellarshot's own rclone config.
- **Déjà Dup:**
  - Its settings and keyring entries are never modified.
  - Its password is never read.
  - Only a destination that proves to be a restic repository is imported.

## Deviations from the spec, decided here

- **OneDrive has no dedicated sign-in button in M3.** rclone's OneDrive setup asks which drive to use, interactively. It's reachable through "Use one of your rclone remotes", and a dedicated flow is on the roadmap.
- **SFTP runs through rclone's `sftp` backend** (an on-the-fly remote), not a separate SFTP implementation. One transport means one set of tests.

## Review focus

1. **A drive plugged in at a different mount point.** The profile must still find it. Test: `uuid_resolves_to_its_current_mount_point`.
2. **A drive that isn't plugged in.** The error must be `DestinationUnavailable` naming the drive, never "not a repository". Test: `missing_drive_is_unavailable`.
3. **A remote path with an existing repository, or with other files.** The probe must tell them apart, just as it does for a folder. Test: `rclone_probe_classifies_like_a_folder` (rclone's `:local:` backend).
4. **Deleting a repository on a remote.** Only its own entries go. Test: `rclone_delete_leaves_foreign_files`.
5. **Déjà Dup settings with missing keys.** The schema defaults apply: `$HOME` included, Trash and Downloads excluded. Test: `missing_keys_take_the_schema_defaults`.

---

### Task 1: Location enum, rclone backend, rclone commands
- Enable the `rustic_backend` feature `rclone`.
- `Location::Rclone { remote, path }`. `backend_string()` gives `rclone:<remote>:<path>`, and the options carry `rclone-command = "rclone serve restic --addr localhost:0 --config '<conf>'"`.
- `engine::rclone::{available, probe, delete_repository, config_path}`.
- **Tests** (rclone's `:local:` backend): `backup_through_rclone_round_trips`, `rclone_probe_classifies_like_a_folder`, `rclone_delete_leaves_foreign_files`. They fail, rather than skip, when rclone is missing. CI installs rclone.

### Task 2: Drives (`src/drives.rs`)
- `mounted_drives() -> Vec<Drive { uuid, label, mount_point }>`: filesystems mounted under `/media` or `/run/media` that have a UUID.
- `mount_point(uuid) -> Option<PathBuf>`.
- `drive_for(path) -> Option<(Drive, relative)>`.
- Pure parsing is tested with mountinfo and by-uuid fixtures: `uuid_resolves_to_its_current_mount_point`, `missing_drive_is_unavailable`, `drive_for_finds_the_containing_mount`.

### Task 3: Destinations and profiles
- `Destination::{Local, Removable { uuid, relative_path, label }, Sftp { host, user, port, path }, Rclone { remote, path, provider }}`.
- `Profile::location() -> Result<Location, EngineError>`. A removable destination resolves at run time.
- Choosing a folder on a removable drive stores it as `Removable` automatically.

### Task 4: Wizard "Where" step
- **Kinds:** Folder, Removable drive (detected drives listed; one click uses `<drive>/Stellarshot/<hostname>`), Network server (SFTP form), Google Drive (Sign in, then a folder name), and one of your rclone remotes (copied into Stellarshot's config).
- **Checks:** `probe` runs for every kind. A missing rclone blocks the cloud kinds, with an install hint.

### Task 5: Déjà Dup import (`src/dejadup.rs`)
- Settings are read from the Flatpak keyfile, then from `dconf dump /org/gnome/deja-dup/`.
- They map to a profile: sources, excludes, destination (`local`, `drive`, `remote` sftp, `google`, `rclone`), with `$TOKENS` resolved through the XDG user directories.
- **Where it appears:** "Import from Déjà Dup" on the first screen when settings are found. It opens the wizard in Open mode, pre-filled, and Google needs a sign-in first.
- **Tests:** a fixture keyfile modelled on a real Déjà Dup 50 install, a `dconf dump` fixture, schema defaults, token resolution, and unsupported backends refused.

### Task 6: Verify, document, commit, push
