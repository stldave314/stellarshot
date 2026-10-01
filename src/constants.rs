// SPDX-License-Identifier: GPL-3.0-only

//! Implementation tuning values.
//!
//! These rarely change and are not user settings, so they live here as
//! compile-time constants rather than in a runtime config file. User-facing
//! settings belong in `cosmic-config` instead. Add a value here only when code
//! uses it.

/// The application ID: desktop entry, icon, settings and keyring items.
pub const APP_ID: &str = "io.github.stldave314.Stellarshot";

/// The upstream application ID, before this app's own settings existed
/// under its own.
pub const OLD_APP_ID: &str = "com.github.cosmic-utils.Stellarshot";

/// The settings' version: 2 replaced the `repositories` list with backup
/// profiles.
pub const CONFIG_VERSION: u64 = 2;

/// Initial window width, in logical pixels.
pub const WINDOW_WIDTH: f32 = 800.0;

/// Initial window height, in logical pixels.
pub const WINDOW_HEIGHT: f32 = 800.0;

/// Smallest width the window may be resized to.
pub const WINDOW_MIN_WIDTH: f32 = 400.0;

/// Smallest height the window may be resized to.
pub const WINDOW_MIN_HEIGHT: f32 = 180.0;

/// Widest a page's content grows (home, a backup's page, history), in
/// logical pixels: lines of text any longer are hard to read.
pub const PAGE_MAX_WIDTH: f32 = 900.0;

/// Widest the restore page grows: wider than other pages, for its file
/// lists' extra columns.
pub const RESTORE_MAX_WIDTH: f32 = 960.0;

/// Widest the new-backup wizard grows.
pub const WIZARD_MAX_WIDTH: f32 = 760.0;

/// Widest the first-run page's text grows.
pub const EMPTY_MAX_WIDTH: f32 = 460.0;

/// Size of the app icon on the first-run page.
pub const EMPTY_ICON_SIZE: u16 = 96;

/// Size of the status and file-type icons in lists.
pub const LIST_ICON_SIZE: u16 = 16;

/// Size of the warning icon beside a backup page's notices.
pub const NOTICE_ICON_SIZE: u16 = 20;

/// Height of the wizard's folder-size list.
pub const FOLDER_LIST_HEIGHT: f32 = 320.0;

/// Indent of a folder with nothing to expand in the folder-size list: the
/// width of the expand button the others have.
pub const FOLDER_LIST_INDENT: f32 = 24.0;

/// Width of the bandwidth limit field.
pub const BANDWIDTH_FIELD_WIDTH: f32 = 120.0;

/// Widest the "Browse…" hint's text grows before it wraps. Wide enough for
/// its longest translation to wrap onto a few lines rather than many.
pub const HINT_MAX_WIDTH: f32 = 300.0;

/// Shortest interval between two progress reports from one operation. rustic
/// reports per blob; anything faster than this only costs redraws.
pub const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// How long a child process's output is still read after it has exited.
/// Its last lines are already in the pipe by then; anything that keeps the
/// pipe open longer is a process it left behind.
pub const DRAIN_AFTER_EXIT: std::time::Duration = std::time::Duration::from_secs(2);

/// How much of a `--run` child's stderr is kept while it runs (its tracing
/// writes one line per unreadable file, per ownership failure on restore,
/// and per line of rclone's own stderr). Older bytes are discarded as new
/// ones arrive, rather than buffered without bound, and rather than left in
/// the pipe until the child exits: a pipe's buffer is much smaller than
/// this, and a child blocked writing to a full one would hold the
/// repository lock for as long as nothing drains it.
pub const CHILD_STDERR_TAIL: usize = 16 * 1024;

/// How much of that tail reaches an error message or the event log. Capped
/// well below `CHILD_STDERR_TAIL` so one very talkative failure cannot
/// flood either.
pub const CHILD_STDERR_DETAIL: usize = 4 * 1024;

/// Pack files uploaded at once to storage reached through rclone (SFTP and
/// cloud storage). rustic uploads one at a time, which leaves a slow
/// connection idle between packs; restic's rclone backend uses 5. Each
/// upload holds one pack (32 MiB and up) in memory until it is stored.
pub const UPLOAD_CONNECTIONS: usize = 4;

/// Flags for the `rclone serve restic` that carries SFTP and cloud backups.
/// rclone ignores the flags of a storage type it is not using.
///
/// - Google Drive keeps deleted files in its trash, where they still count
///   against the quota for 30 days, so a clean-up would free nothing there.
///   Everything rustic deletes is data no snapshot uses; Déjà Dup deletes
///   it permanently too.
/// - Google Drive uploads in chunks of 8 MiB by default, a round trip each.
///   Packs are 32 MiB and up, so this sends most packs in one chunk. Each
///   upload in flight holds one chunk in memory.
pub const RCLONE_SERVE_FLAGS: &[&str] = &["--drive-use-trash=false", "--drive-chunk-size=32M"];

/// Longest wait for rclone to say what is at a location, before a new
/// backup is created there or an existing one opened. Cloud storage usually
/// answers in a few seconds; one that is slowing rclone down (Google Drive
/// limits how fast one app may make requests) is reported rather than
/// waited on without end.
pub const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Flags for rclone commands that only look at a location. rclone retries a
/// failing request 10 times by default, waiting longer each time, which
/// turns an error into minutes of silence; fewer retries report it sooner.
pub const RCLONE_LOOK_FLAGS: &[&str] = &["--low-level-retries=3", "--contimeout=20s"];

/// How long a running backup's figures may stand still before its card
/// says what may be holding it up. Reading moves them many times a second;
/// a pause this long is the destination, or rustic reading its index.
pub const STALL_NOTICE: std::time::Duration = std::time::Duration::from_secs(15);

/// How often the window redraws while something shows a running time: a
/// backup, a destination being checked, a repository being created.
pub const WAITING_TICK: std::time::Duration = std::time::Duration::from_secs(1);

/// Longest wait for the desktop keyring. Long enough to type a password into
/// an unlock prompt; short enough that a keyring that never answers (no
/// Secret Service, or one waiting on a prompt nobody can see) turns into
/// "not remembered" instead of a request that never ends.
pub const KEYRING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Longest wait for a `password_command`. Long enough for a password
/// manager CLI to unlock a vault interactively once; short enough that one
/// left stuck waiting on a prompt nobody can see (a GUI pinentry, a device
/// that never got plugged in) fails instead of hanging a `--scheduled` run
/// forever, silently skipping every timer fire after it.
pub const PASSWORD_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Longest wait for one hook (stopping or starting a service, say) before it
/// is killed and treated as a failure. A `Before` hook stuck this long would
/// otherwise hold the repository's write lock open and block the backup it
/// was meant to make safe forever.
pub const HOOK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// How long a run that has been told to stop (SIGTERM: Cancel in the
/// window, `systemctl --user stop`, logout, shutdown) gets to run its
/// `After` hooks and report itself canceled before it is killed outright:
/// one hook's own timeout, plus a margin for the rest. The window's
/// `ChildHandle::cancel` escalates to SIGKILL after this, and the scheduled
/// unit's `TimeoutStopSec` is this too, so both paths give the hooks the
/// same chance. See `proc_signal`.
pub const TERM_GRACE: std::time::Duration =
    std::time::Duration::from_secs(HOOK_TIMEOUT.as_secs() + 10);

/// Longest a scheduled run may take before systemd gives up on it. A
/// `Type=oneshot` unit has no start timeout at all by default, so a run
/// stuck on a hard NFS mount or a stalled `rclone serve` would otherwise
/// stay "activating" forever, and every later timer fire would be skipped
/// without a word. A day: a first backup of a large folder over a slow
/// connection can genuinely take most of one.
pub const SCHEDULED_UNIT_TIMEOUT_START: std::time::Duration =
    std::time::Duration::from_secs(24 * 3600);

/// How often a scheduled backup also checks the repository for damage. A
/// check reads every index and tree, which is slow on a large backup behind
/// a slow connection, so it runs after a backup at most this often.
pub const CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30 * 86_400);

/// How long a scheduled run waits for the user to click its failure
/// notification before exiting. Clicking opens the backup in Stellarshot.
pub const NOTIFICATION_WAIT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Entries kept in a backup's own event log. The oldest are dropped as new
/// ones arrive, so a backup that has run for years does not grow its log
/// without bound.
pub const EVENT_LOG_CAPACITY: usize = 200;

/// How much later than its own schedule a backup may run before it is shown
/// as overdue: enough slack for the timer's own jitter, not so much that a
/// truly stuck schedule (an unplugged drive, a laptop closed for days) goes
/// unnoticed.
pub const OVERDUE_FACTOR: i64 = 2;

/// A mounted snapshot never changes once taken, so there is nothing a
/// short TTL would ever need to catch — a stat or a directory listing is
/// good until the filesystem is unmounted.
pub const MOUNT_ATTR_TTL: std::time::Duration = std::time::Duration::from_secs(365 * 24 * 3600);

/// How often a blocking wait loop (a hook's or rclone's own child process
/// exiting) re-checks rather than blocking on it directly: frequent enough
/// that a short-lived command is not kept waiting noticeably, cheap enough
/// not to matter for a long-running one.
pub const PROCESS_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

/// A scheduled backup's own systemd timer runs `Nice`d this far below
/// normal priority: background work should not compete with whatever the
/// person at the keyboard is doing.
pub const SCHEDULED_UNIT_NICE: i32 = 10;

/// How far a scheduled backup's own start can be randomly delayed, so
/// several backups due at the same wall-clock moment (every timer set to
/// "daily", say) do not all start at once.
pub const SCHEDULED_UNIT_RANDOMIZED_DELAY: std::time::Duration =
    std::time::Duration::from_secs(10 * 60);

/// How often relative times ("2 hours ago") are refreshed.
pub const WINDOW_CLOCK_TICK: std::time::Duration = std::time::Duration::from_secs(30);

/// How often the panel applet re-reads every backup's status while its
/// popup is actually open and someone might be looking at it.
pub const APPLET_REFRESH: std::time::Duration = std::time::Duration::from_secs(3);

/// How often the applet does the same while its popup is closed: a lock
/// probe and a config read per backup are each cheap on their own, but
/// there is no reason to spend them at all when nothing could be showing
/// the result.
pub const APPLET_IDLE_REFRESH: std::time::Duration = std::time::Duration::from_secs(60);

/// How many rows a list on the restore page shows before capping with
/// "Show more": search results, a folder's entries, a diff's groups, a
/// missing-files list. A folder or diff with far more than this costs this
/// many widgets per frame, not the folder's or diff's own real size.
pub const RESTORE_RESULT_LIMIT: usize = 500;

/// How many rows a backup's own page shows at once before "Show all": its
/// most recent snapshots, and separately, its most recent history entries.
pub const PROFILE_RECENT_ROWS: usize = 5;

/// Most History page entries shown at once, newest first: a machine that
/// has backed up for years across several destinations could otherwise
/// mean rendering thousands of rows for one screen.
pub const HISTORY_LIMIT: usize = 500;

/// The default port a bare SSH destination (no `:port` given) is assumed
/// to listen on.
pub const SSH_DEFAULT_PORT: u16 = 22;

/// The most days a retention rule's "keep everything from the last `n`
/// days" may ask for. The wizard only ever offers 90, 182 or 365; this
/// exists so a hand-edited config or an imported settings file cannot pass
/// a value so large that building the `jiff::Span` for it panics (jiff's
/// own range tops out at roughly 7.3 million days). 100 years is already
/// far past anything a real retention policy needs.
pub const RETENTION_MAX_DAYS: u32 = 36_500;

/// The largest settings export file an import will read. A real export is
/// a few kilobytes per backup plus its history (capped at
/// [`EVENT_LOG_CAPACITY`] entries each); this is far above any of that,
/// and exists only so a file picked by mistake — a multi-gigabyte
/// something-else with the wrong extension — is refused up front rather
/// than read whole into the window's memory.
pub const MAX_EXPORT_BYTES: u64 = 8 * 1024 * 1024;

/// The longest backup name an imported settings file may give a profile.
/// The wizard's own field is a single line; a file is not, and a name is
/// rendered into the sidebar, the applet, notifications and unit
/// descriptions, none of which should be handed a megabyte.
pub const IMPORT_NAME_MAX_CHARS: usize = 256;

/// The most a Stellarshot-run rclone command that changes something (deleting
/// a repository, forgetting a remote) may take before it is stopped. Long,
/// since a big repository on slow storage has many files to delete, and only
/// there to turn a connection that has stalled for good into an error.
pub const RCLONE_CHANGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3600);

/// The most a password command may print before it is refused: a password
/// is a few dozen bytes, so anything near this is not one, and a command
/// that prints without end must not fill memory.
pub const PASSWORD_COMMAND_MAX_OUTPUT: usize = 64 * 1024;

/// How much of an `rclone lsf` listing is read when looking at what is at a
/// destination: enough for any repository folder, which has a handful of
/// entries, and a cap on what a folder of millions of files can cost.
pub const RCLONE_LISTING_LIMIT: usize = 1024 * 1024;

/// How long a repository's lock name is remembered for the status poll: see
/// `engine::lock::is_running`. Short enough that a destination that changes
/// (a drive mounted somewhere else) is picked up within the minute.
pub const STATUS_KEY_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(60);

/// How long to wait for `rclone serve restic` to start listening. For SFTP and
/// most cloud storage it connects to the remote first (and exits if it
/// cannot), so this covers a slow connection too; one that takes longer is
/// treated as not reachable now.
pub const RCLONE_SERVE_START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// How long a scheduled run waits for UPower and NetworkManager to answer
/// before treating what they would have said as unknown. zbus sets no
/// timeout of its own, and a hung service would otherwise leave the
/// `--scheduled` unit "activating" for good, every later timer fire skipped
/// behind it.
pub const CONDITIONS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// How far back "Deleted files" looks by default, in days.
pub const DELETED_WINDOW_DAYS: i64 = 30;

/// Hard cap on rows the wizard's folder browser renders at once: expanding
/// enough folders to need more than this is rare, and re-walking tens of
/// thousands of them into fresh widgets on every keystroke or tick (`view()`
/// runs on both) is not something a tree this deep should ever cost.
pub const BROWSE_ROW_LIMIT: usize = 500;

/// The largest file "Open a copy" will restore: the copy goes in the user's
/// runtime folder, which is backed by memory, so a big one would push
/// everything else out. Anything larger is offered a restore to a folder.
pub const OPEN_COPY_MAX_BYTES: u64 = 512 * 1024 * 1024;

/// How long an "Open a copy" folder stays before it is removed. Long enough
/// that one still open in a viewer is not pulled away within a working day.
pub const OPEN_COPY_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// How long a cloud sign-in may take in the browser before rclone is stopped
/// and the sign-in reported as timed out.
pub const SIGN_IN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// How long a scheduled run waits before trying again a check that failed
/// with an error (not one that found damage): long enough that a check that
/// cannot run does not repeat at every slot.
pub const CHECK_RETRY: std::time::Duration = std::time::Duration::from_secs(86_400);

/// The most a `--run` job read from stdin may be: far more than any real job
/// (folders, exclusions and a password), and a cap on what a wrong caller can
/// make the child buffer.
pub const JOB_MAX_BYTES: usize = 16 * 1024 * 1024;

/// How deeply folders in a snapshot may be nested before comparing two
/// snapshots gives up: far beyond any real file system's paths, and well
/// short of what the recursion's stack can hold.
pub const TREE_MAX_DEPTH: usize = 1024;
