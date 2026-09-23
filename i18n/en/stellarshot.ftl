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
wizard-pick-sources = Choose folders to back up
wizard-pick-excludes = Choose folders to leave out
wizard-advanced = Advanced
wizard-pattern-placeholder = Leave out names matching, e.g. *.tmp or node_modules
wizard-one-file-system = Stay on the same drive
wizard-one-file-system-description = Do not follow into other drives or network shares mounted inside these folders.
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
snapshot-failed = The snapshot could not be created.
open-repo-failed = The repository could not be opened.
error-wrong-password = The password is incorrect.
error-not-a-repository = There is no repository at { $path }.
error-already-exists = { $path } already holds a repository.
error-destination-unavailable = { $path } cannot be reached. If it is on a removable drive or a network share, check that it is connected.
error-locked = Another backup is already using this repository. Try again when it has finished.
error-cancelled = The operation was cancelled. Nothing was changed.
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
