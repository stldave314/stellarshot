# Application
stellarshot = Stellarshot

# Empty state
empty-title = Keep your files safe
empty-body = Back up your folders to another drive or folder. Backups are encrypted, and only changes are stored after the first one.
create-backup = Create a Backup…
open-existing = Open an existing backup

# Profile page
never-backed-up = Not backed up yet
backed-up-just-now = Last backup just now
backed-up-minutes-ago = Last backup { $count ->
    [one] a minute
   *[other] { $count } minutes
} ago
backed-up-hours-ago = Last backup { $count ->
    [one] an hour
   *[other] { $count } hours
} ago
backed-up-days-ago = Last backup { $count ->
    [one] a day
   *[other] { $count } days
} ago
status-detail = { $destination } · { $count ->
    [one] one snapshot
   *[other] { $count } snapshots
}
back-up-now = Back Up Now
choose-what-title = Choose what to back up
choose-what-body = This backup does not have any folders to back up yet.
choose-what-button = Choose Folders…
unlock-title = Enter the password for this backup
unlock = Unlock
remember-password = Remember password
remember-password-description = Stored in your keyring. Scheduled backups will need it.
recent-snapshots = Recent snapshots
no-snapshots-yet = No snapshots yet. Press Back Up Now to take the first one.
snapshot-row = { $id } · { $size } · { $added } new
show-all-snapshots = Show all { $count } snapshots
manage = Manage
edit-backup = What to back up
edit-backup-description = Change the included and excluded folders.
change-password-row = Password
change-password-row-description = Change the password this backup is unlocked with.
change-password-title = Change password
change-password-body = Adds a key for the new password, then removes the one you are unlocked with now. Anyone else's key, or a recovery sheet's, is left alone.
change-password-command-note = You will also need to update it wherever the current password is stored — this only changes the backup itself.
change-password-failed = The password could not be changed.
password-source-row = How the password is provided
password-source-keyring = Kept in the keyring
password-source-command = From a command
password-source-title = How the password is provided
password-source-body = A command that prints the password on its standard output, run fresh every time one is needed, instead of the keyring — for a password manager with a command-line client, such as the Bitwarden CLI. Leave empty to use the keyring instead.
password-source-placeholder = e.g. bw get password stellarshot-home
password-command-failed = The password command could not be run.
remove-backup = Remove from Stellarshot
remove-backup-description = Stellarshot forgets this backup. The backup itself stays where it is.
delete-backup = Delete backup and all data
delete-backup-description = Permanently deletes every snapshot in this backup.
progress-starting = Starting…
progress-preparing = Preparing…
progress-backing-up = Backing up…
progress-restoring = Restoring…
progress-checking = Checking…
progress-amount = { $done } of { $total }

# Wizard
wizard-create-title = New backup
wizard-open-title = Open an existing backup
wizard-edit-title = What “{ $name }” backs up
wizard-step = Step { $current } of { $total }
wizard-what-intro = Choose the folders to back up, and anything inside them to leave out.
wizard-include = Include
wizard-exclude = Exclude
wizard-exclude-outside = Not inside an included folder, so it changes nothing
wizard-add-folders = Add Folders…
browse-open = Browse…
browse-close = Close
browse-scanning = Scanning… { $count } found so far
browse-mark-included = Included
browse-mark-partial = Partly included
browse-mark-excluded = Excluded
browse-include = Include this folder
browse-exclude = Leave this folder out
wizard-pick-sources = Choose folders to back up
wizard-pick-excludes = Choose folders to leave out
wizard-advanced = Advanced
wizard-pattern-placeholder = Leave out names matching, e.g. *.tmp or node_modules
wizard-one-file-system = Stay on the same drive
wizard-one-file-system-description = Do not follow into other drives or network shares mounted inside these folders.
wizard-exclude-caches = Leave out cache folders
wizard-exclude-caches-description = Skip any folder that marks itself as disposable cache data with a CACHEDIR.TAG file.
wizard-git-ignore = Honor .gitignore
wizard-git-ignore-description = Leave out whatever each project's own .gitignore already excludes.
wizard-skip-if-unchanged = Skip empty backups
wizard-skip-if-unchanged-description = Do not record a new snapshot when nothing has changed since the last one.
wizard-estimate-label = Estimated backup size
wizard-estimate = { $size } · { $files } files
wizard-estimate-counting = Counting…
wizard-estimate-note = The first backup is usually smaller after compression and deduplication. Later backups only store what changed.
wizard-where-intro = Choose where the backup is kept: an empty folder, ideally on another drive.
wizard-where-title = Location
wizard-no-folder = No folder chosen
wizard-choose-folder = Choose Folder…
select-repo-folder = Choose a folder for the backup
wizard-where-new = A new backup will be created here.
wizard-where-existing = This folder already holds a backup. Use “Open an existing backup” to add it instead.
wizard-where-found = A backup was found here.
wizard-where-no-repository = There is no backup in this folder.
wizard-where-not-empty = This folder already contains other files. Choose an empty folder.
wizard-name = Name
wizard-name-placeholder = For example, Home to USB drive
wizard-secure-intro = Choose a password. Your backup is encrypted with it.
wizard-open-intro = Enter the password this backup was created with.
wizard-confirm = Confirm password
wizard-mismatch = The passwords do not match.
wizard-password-warning = If you lose this password, your backups cannot be restored. Nobody can recover it for you.
wizard-finish-create = Create and Back Up Now
wizard-finish-open = Open

# Dialogs and buttons
ok = Ok
save = Save
add = Add
back = Back
next = Next
edit = Edit
remove = Remove
delete = Delete
cancel = Cancel
password = Password
remove-title = Remove “{ $name }”?
remove-body = Stellarshot will forget this backup and its saved password. The backup and its snapshots are not deleted, and can be opened again later.
delete-title = Delete “{ $name }” and all its data?
delete-body = This permanently deletes the backup and every snapshot in it. Other files in the same folder are not touched. Type { $name } to confirm.

# Errors
error-title = Something went wrong
error-details = Details: { $details }
location-not-empty = { $path } already contains other files. Choose an empty folder, or a folder that already holds a repository.
create-repo-failed = The repository could not be created.
delete-repo-failed = The repository could not be deleted.
delete-snapshot-failed = The snapshot could not be deleted.
pin-snapshot-failed = The snapshot's pin could not be changed.
pin-snapshot = Pin, so cleaning up never removes this snapshot
unpin-snapshot = Unpin, so cleaning up may remove this snapshot again
delete-snapshot-row = Delete this snapshot
snapshot-failed = The snapshot could not be created.
open-repo-failed = The repository could not be opened.
error-wrong-password = The password is incorrect.
error-not-a-repository = There is no repository at { $path }.
error-already-exists = { $path } already holds a repository.
error-destination-unavailable = { $path } cannot be reached. If it is on a removable drive or a network share, check that it is connected.
error-locked = Another backup is already using this repository. Try again when it has finished.
error-canceled = The operation was canceled. Nothing was changed.
error-repository-damaged = The repository check found problems. Do not delete other copies of your data until this is resolved.

# About
about = About
about-author = Stellarshot contributors
about-credits = Based on Stellarshot by the cosmic-utils project.
repository = Repository
support = Support

# Settings
settings = Settings
appearance = Appearance
theme = Theme
match-desktop = Match desktop
dark = Dark
light = Light

# Menu
file = File
menu-new-backup = New Backup…
new-backup = New backup
new-window = New window
quit = Quit
view = View
menu-settings = Settings...
menu-about = About Stellarshot...

# Storage locations
google-drive = Google Drive
place-folder = Folder
place-folder-description = A folder on this computer or a mounted drive
place-drive = Removable drive
place-drive-description = A USB drive, found wherever it is mounted
place-server = Network server (SFTP)
place-server-description = A folder on a computer you can reach with SSH
place-google-description = Your Google account, signed in from Stellarshot
place-remote = One of your rclone remotes
place-remote-description = OneDrive, Dropbox, S3 and anything else rclone can reach
place-rest = REST server
place-rest-description = A rest-server or rustic-server you run yourself
place-rest-url = Server URL
place-rest-url-placeholder = http://user:pass@host:8000/repo/
place-rclone-missing = Backing up here needs rclone. Install it (for example, sudo apt install rclone) and try again.
place-check-failed = Could not check this location.
place-check = Check
place-no-drives = No removable drives are plugged in. Plug one in, then go back and forward to look again.
place-folder-on-drive = Folder on the drive
place-host = Server
place-user = User name
place-user-placeholder = Your user name on this computer
place-port = Port
place-server-path = Folder on the server
place-server-note = Authentication uses your SSH agent or keys, and the server must already be in ~/.ssh/known_hosts: connect to it once with ssh first.
place-signing-in = Finish signing in in your browser. Stellarshot is waiting…
place-google-intro = Stellarshot will open your browser so you can sign in to Google. Only Stellarshot's own settings will hold the sign-in.
place-sign-in = Sign In with Google…
place-signed-in = Signed in.
place-cloud-folder = Folder
place-no-remotes = You have no rclone remotes. Set one up with rclone config, then come back.
error-rclone-missing = rclone is not installed. Install it (for example, sudo apt install rclone) to use this location.
error-auth-failed = Signing in did not complete: { $details }

# Déjà Dup import
dejadup-import = Import from Déjà Dup
dejadup-title = Import a Déjà Dup backup
dejadup-name = Déjà Dup backup
dejadup-none = No Déjà Dup backup settings were found.
dejadup-other-format = This Déjà Dup backup uses the older duplicity format, which Stellarshot cannot read. Create a new backup instead; the old one stays readable in Déjà Dup.
dejadup-unsupported = Déjà Dup keeps this backup in a place Stellarshot cannot use ({ $backend }). Create a new backup instead.
menu-import-dejadup = Import from Déjà Dup…

# Restore
restore-open = Restore…
restore-title = Restore from { $name }
restore-loading = Opening the backup…
folder-up = Up one folder
tab-browse = Browse
tab-deleted = Deleted files
tab-compare = Compare
selected-count = { $count ->
    [one] 1 item selected
   *[other] { $count } items selected
}
restore-button = Restore…
search-placeholder = Search this snapshot
search-results = { $count ->
    [one] 1 match
   *[other] { $count } matches
}
folder-empty = This folder is empty.
versions-title = Versions
versions-same = { $count ->
    [one] ↳ the same in 1 older snapshot
   *[other] ↳ the same in { $count } older snapshots
}
open-copy = Open Copy
restore-this-version = Restore This Version…
deleted-scope = Files in { $folder } that are in backups from the last { $days } days but no longer on disk.
deleted-change-folder = Change Folder…
deleted-find = Find Deleted Files
restore-searching = Looking…
deleted-intro = Look for files you deleted that a backup still has.
deleted-none = Nothing is missing: every file in these backups is still on disk.
deleted-last-seen = last backed up { $when }
compare-button = Compare
compare-intro = Choose two snapshots to see what changed between them.
compare-none = Nothing changed between these snapshots.
compare-summary = { $added } added · { $removed } removed · { $changed } changed
restore-sheet-title = { $count ->
    [one] Restore 1 item
   *[other] Restore { $count } items
}
restore-to = Restore to
restore-to-original = Where they were
restore-to-folder = Another folder…
restore-to-folder-chosen = Into { $folder }
restore-existing = If a file already exists
policy-keep-both = Keep both
policy-keep-both-description = The restored copy gets a new name; your file is not touched.
policy-overwrite = Overwrite
policy-overwrite-description = Replace it with the backed-up copy.
policy-skip = Skip
policy-skip-description = Leave it, and do not restore that file.
restore-advanced = Advanced
restore-verify-existing = Verify existing files
restore-verify-existing-description = Read and check a file that already looks unchanged, instead of trusting its size and modification time.
restore-ownership-preserve = Restore the original owner
restore-ownership-numeric = Restore numeric user and group IDs
restore-ownership-none = Do not restore ownership
restore-previewing = Working out what will happen…
restore-choose-folder = Choose the folder to restore into.
restore-preview-failed = Could not work out what the restore would do.
preview-restore = { $count ->
    [one] 1 file will be restored ({ $size })
   *[other] { $count } files will be restored ({ $size })
}
preview-kept = { $count ->
    [one] 1 existing file differs and will be kept alongside
   *[other] { $count } existing files differ and will be kept alongside
}
preview-replaced = { $count ->
    [one] 1 existing file differs and will be replaced
   *[other] { $count } existing files differ and will be replaced
}
preview-skipped = { $count ->
    [one] 1 existing file differs and will be skipped
   *[other] { $count } existing files differ and will be skipped
}
preview-unchanged = { $count ->
    [one] 1 file is already identical and will not be touched
   *[other] { $count } files are already identical and will not be touched
}
restore-done-title = Restore finished
restore-done-body = { $count ->
    [one] Restored 1 file ({ $size }).
   *[other] Restored { $count } files ({ $size }).
} { $conflicts ->
    [0] {""}
   *[other] Files that already existed were handled as you chose.
}
restore-failed = The restore did not finish.
browse-failed = Could not read this backup.
open-copy-failed = Could not open a copy of this file.
select-scope-folder = Choose a folder to look in
select-restore-folder = Choose where to restore to

# Automation
error-password-not-remembered = Scheduled backups need the password remembered in your keyring. Open the backup, enter its password with “Remember password” on, and the next scheduled backup will run.
error-keyring-unavailable = The password was changed, but it could not be saved to your keyring: { $details }. Enter it there yourself, or a scheduled backup using it will fail.
error-delete-unsupported = Stellarshot cannot delete this destination's own data by itself. Remove it there yourself, or use Remove to forget it here without deleting anything.
change = Change…
schedule-row = When it runs
check-row = Check for damage
check-row-last = Last checked { $when }
check-row-never = Never checked
check-now = Check Now
check-again = Check Again
check-failed = The check did not finish.
check-passed-title = No damage found
check-passed-body = Every snapshot, folder and index entry in this backup is present and consistent.
clean-up-row = Free up space
clean-up-row-description = Forgets snapshots the “Keep” setting no longer needs, and deletes data no snapshot uses.
clean-up-now = Clean Up Now
clean-up-failed = Cleaning up did not finish.
clean-up-cannot-stop = Freeing space cannot be stopped once it starts.
clean-up-done-title = Clean-up finished
clean-up-done-body = { $count ->
    [one] Forgot 1 snapshot.
   *[other] Forgot { $count } snapshots.
} { $size } no longer needed.
progress-cleaning-up = Freeing up space…
damaged-title = A check found damage in this backup
damaged-body = Automatic freeing of space is paused until a check passes. Snapshots may still be restorable; to be safe, start a new backup somewhere else.
failed-just-now = just now
failed-minutes-ago = { $count ->
    [one] a minute ago
   *[other] { $count } minutes ago
}
failed-hours-ago = { $count ->
    [one] an hour ago
   *[other] { $count } hours ago
}
failed-days-ago = { $count ->
    [one] yesterday
   *[other] { $count } days ago
}
scheduled-backup-failed = The automatic backup failed ({ $when })
scheduled-cleanup-failed = Cleaning up after the automatic backup failed ({ $when })
scheduled-check-failed = The automatic check failed ({ $when })
schedule-failed = The schedule could not be set up.
schedule-manual = Backs up only when you press Back Up Now
schedule-hourly = Backs up automatically every hour
schedule-daily = Backs up automatically every day
schedule-weekly = Backs up automatically every week
frequency-hourly = Every hour
frequency-daily = Every day
frequency-weekly = Every week
keep-smart = Smart (recommended)
keep-3-months = At least 3 months
keep-6-months = At least 6 months
keep-1-year = At least a year
keep-days = At least { $days } days
keep-forever = Forever
keep-smart-description = Keeps the newest snapshot of each of the last 7 days that have one, of each of the last 4 weeks (Monday to Sunday) that have one, and of each of the last 12 calendar months that have one. Days, weeks and months without a backup are passed over, not counted, and one snapshot can be the one kept for its day, its week and its month at once. While the backups span fewer than 12 months, the very first snapshot is kept as well. Every other snapshot is forgotten. Only this computer’s snapshots are affected, and each set of folders backed up is counted on its own.
keep-forever-description = Every snapshot is kept. The backup only grows.
keep-for-description = Every snapshot from the { $days } days before the newest one, and older ones as they fall due.
wizard-schedule-title = When “{ $name }” runs
wizard-when-intro = Backups can run on their own. If the computer is off or asleep at the time, the backup runs as soon as you are back.
wizard-automatic = Back up automatically
wizard-automatic-description = Runs in the background, even when Stellarshot is closed.
wizard-frequency = How often
wizard-keep = Old snapshots
wizard-keep-label = Keep
wizard-prune = Free up space automatically
wizard-prune-description = Delete data no snapshot needs any more. Leave this off if another computer backs up to the same place.
wizard-append-only = Append-only
wizard-append-only-description = A guard against mistakes, not attacks: rustic itself refuses to delete a snapshot from an append-only repository, but a tool that does not have to obey that could still remove one directly. Stellarshot offers no way to turn this off again once it is on: Free up space, and the Keep setting above, stop working from that point on. Choose carefully; there is no way back from this screen.
wizard-remember-for-schedule = Scheduled backups can only run with the password remembered.
notify-backup-failed = Backup “{ $name }” failed
notify-cleanup-failed = Cleaning up “{ $name }” failed
notify-check-failed = Checking “{ $name }” failed
notify-open = Open
error-timed-out = There was no answer within { $seconds } seconds. The connection may be slow, or the storage service may be limiting requests. Check your connection and try again.
place-checking-for = Checking… { $time }
wizard-creating = Creating… { $time }
wizard-opening = Opening… { $time }
wizard-saving = Saving…
wizard-creating-note = Setting up the repository. On cloud storage this can take a minute.
wizard-opening-note = Reading the backup’s snapshots. On cloud storage this can take a minute.
progress-uploaded = { $amount } · { $uploaded } stored
progress-elapsed = Running for { $time }
progress-waiting-preparing = Reading what the backup already holds ({ $time } so far). On cloud storage this can take several minutes.
progress-waiting = Nothing has moved for { $time }. This happens while cloud storage is slow to accept data or is limiting requests; the backup carries on by itself.
wizard-estimate-arithmetic = { $included } included − { $excluded } excluded = { $total }
wizard-estimate-nothing-excluded = Nothing is excluded from the folders above.
wizard-estimate-adding-up = Counting… adding up what the exclusions leave out.
wizard-patterns-remove = These patterns leave out { $size } that the excluded folders do not already.
status-up-to-date = Up to date
status-running = Running
status-overdue = Overdue
status-failed = Failed
status-damaged = Damaged
nav-running = { $name } — running…
nav-running-percent = { $name } — { $percent }%
menu-help = Help
help = Help
help-icons-title = What the icons mean
help-terms-title = Terms
term-repository = Repository
term-repository-description = Where a backup's encrypted, deduplicated data lives: a folder, a drive, a server or cloud storage. Each backup profile has its own.
term-snapshot = Snapshot
term-snapshot-description = One backup run's picture of your files, taken at a point in time. A repository holds many, and restoring reads from one of them.
term-rclone = rclone
term-rclone-description = The separate program Stellarshot uses to reach SSH servers, Google Drive and other cloud storage. It is not part of Stellarshot and keeps its own configuration.
term-prune = Prune
term-prune-description = Delete the data that no remaining snapshot needs any more, after old snapshots have been forgotten. "Free up space automatically" does this for you.
term-keep = Keep
term-keep-description = Which old snapshots survive when space is freed. "Smart" keeps a shrinking history; "Forever" keeps everything, and the backup only grows.
next-run = Next backup: { $time }
summary-title = Folders
summary-none = None
summary-included = Included
summary-excluded = Excluded
summary-freed = Freed by clean-ups
statistics-title = Repository statistics
statistics-description = The real size in storage, the compression ratio, and how much could still be reclaimed. Reads every index file and lists the destination.
statistics-calculate = Calculate
statistics-calculating = Calculating…
statistics-stored = Stored at the destination
statistics-ratio = Compression ratio
statistics-no-ratio = Not yet known
statistics-reclaimable = Could be reclaimed by a clean-up
history-title = History
event-backed-up = Backed up
event-stage-backup = Backup
event-stage-check = Check
event-stage-cleanup = Clean-up
event-failed = { $stage } failed: { $reason }
event-skipped = Skipped: { $reason }
event-checked-sound = Check passed
event-checked-damaged = Check found damage
event-cleaned-up = Forgot { $count } snapshots, freed { $size }
notify-overdue = "{ $name }" has not backed up in a while
notify-overdue-body = Its destination has not been reachable at its scheduled times. { $schedule } Check that it is connected, then open Stellarshot to back up now.
settings-backup-title = Backup and restore Stellarshot's own settings
settings-export = Export settings
settings-export-description = Every backup's folders, destination and schedule, and its history. Never a password.
settings-export-button = Export…
settings-export-title = Save Stellarshot's settings
settings-export-done-title = Settings exported
settings-export-done-body = Every backup's settings and history were saved to the file you chose.
settings-export-failed = The settings could not be saved.
settings-import = Import settings
settings-import-description = Add backups from a settings export. A backup already here is left exactly as it is; only its history is added to.
settings-import-button = Import…
settings-import-title = Choose a settings export to import
settings-import-done-title = Settings imported
settings-import-done-body = { $added ->
    [0] No new backups were added.
    [1] One backup was added.
   *[other] { $added } backups were added.
} { $skipped ->
    [0] {""}
    [1] One was already here and was left as it is.
   *[other] { $skipped } were already here and were left as they are.
}
settings-import-failed = The settings could not be imported.
settings-cache-title = Local cache
settings-cache-dir = Cache location
settings-cache-dir-default = Default (~/.cache/rustic)
settings-cache-dir-choose = Choose…
settings-cache-dir-reset = Use the default
settings-cache-dir-title = Choose a cache folder
settings-no-cache = Do not cache at all
settings-no-cache-description = Slower, but nothing worth keeping on a machine low on disk space.
settings-global-excludes-title = Left out of every backup
settings-global-excludes-description = Glob patterns such as node_modules or target, applied to every backup without adding them to each one.
home = Overview
home-backups-title = Backups
home-backup-detail = { $status } · { $last }
home-view = View
home-folders-title = Folders backed up on this computer
home-locations-title = Storage locations
home-location-detail = { $kind } · { $backups }
wizard-resume = Resume setup
wizard-cancel-title = Stop setting up this backup?
wizard-cancel-body = You can come back to it later from the sidebar, or discard everything typed so far.
wizard-finish-later = Finish Later
wizard-discard = Discard
place-google-advanced = Use my own Google API credentials…
place-google-advanced-description = Sign in with a Google Cloud client of your own instead of the shared one rclone provides, so this backup's Google Drive traffic does not compete with everyone else who has never set up their own. Needs both a client ID and a client secret from your own Google Cloud project; leave both blank to use the shared default.
place-google-client-id = Client ID
place-google-client-secret = Client secret
place-advanced = Advanced
place-bandwidth-limit = Bandwidth limit
place-bandwidth-limit-description = Limit how fast this backup uploads and downloads, in rclone's own syntax (1M, or 8M:2M for upload:download). Empty for no limit.
place-bandwidth-limit-placeholder = e.g. 1M
