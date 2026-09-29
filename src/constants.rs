// SPDX-License-Identifier: GPL-3.0-only

//! Implementation tuning values.
//!
//! These rarely change and are not user settings, so they live here as
//! compile-time constants rather than in a runtime config file. User-facing
//! settings belong in `cosmic-config` instead. Add a value here only when code
//! uses it.

/// Initial window width, in logical pixels.
pub const WINDOW_WIDTH: f32 = 800.0;

/// Initial window height, in logical pixels.
pub const WINDOW_HEIGHT: f32 = 800.0;

/// Smallest width the window may be resized to.
pub const WINDOW_MIN_WIDTH: f32 = 400.0;

/// Smallest height the window may be resized to.
pub const WINDOW_MIN_HEIGHT: f32 = 180.0;

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

/// How often a scheduled backup also checks the repository for damage. A
/// check reads every index and tree, which is slow on a large backup behind
/// a slow connection, so it runs after a backup at most this often.
pub const CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30 * 86_400);

/// How long a scheduled run waits for the user to click its failure
/// notification before exiting. Clicking opens the backup in Stellarshot.
pub const NOTIFICATION_WAIT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// How long the web interface's server waits for a client to finish sending
/// its request headers before giving up on the connection. Without an
/// explicit timer, `axum_server`'s TLS listener has none at all (only a
/// `warn!` that the default was dropped), so a client that opens a
/// connection and sends nothing ties up a file descriptor forever — enough
/// of those exhaust the service.
pub const WEB_HEADER_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub const WEB_HTTP2_KEEPALIVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
pub const WEB_HTTP2_KEEPALIVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub const WEB_HTTP2_MAX_CONCURRENT_STREAMS: u32 = 32;
/// Connections held open at once, across every client: past this, a new one
/// is refused rather than accepted and left to queue behind the rest.
pub const WEB_MAX_CONNECTIONS: usize = 64;
/// How long a request may take end to end before the server gives up on it.
pub const WEB_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// The API takes no request bodies today; large enough for one that
/// legitimately needs a small JSON object later, nowhere near enough to let
/// a client tie up memory with an oversized one.
pub const WEB_REQUEST_BODY_LIMIT: usize = 16 * 1024;
/// Requests that open a repository (snapshots, browsing) held at once, so a
/// slow remote cannot tie up an unbounded number of blocking threads or
/// multiply memory use under parallel load.
pub const WEB_REPOSITORY_REQUEST_PERMITS: usize = 2;
/// How long a SIGTERM (Stop, Restart) gives the web daemon to stop
/// accepting new connections and let in-flight HTTP requests and any
/// backup it started finish, before it gives up waiting: HTTP requests
/// answer almost at once regardless (`POST .../run` returns before the
/// backup itself is done), so this really only bounds how long a backup
/// already running gets before it is recorded as canceled and the process
/// exits anyway. Comfortably under the unit's own `TimeoutStopSec`, so
/// systemd never has to force it.
pub const WEB_GRACEFUL_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// How often the web daemon checks whether its own binary has been
/// replaced (a package upgrade), so it can exit and let `Restart=` start
/// the new one instead of quietly running old code until the next login.
pub const WEB_UPGRADE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// `stellarshot-web.service`'s own hardening, kept together so a value used
/// in the unit text and asserted on in `web_daemon.rs`'s own test cannot
/// drift apart. Restarting every 5 seconds forever (the previous
/// `RestartSec`, with no start limit at all) turns a daemon that cannot
/// even start — a bad TLS path, the port already in use, the binary
/// removed — into a tight, endless restart loop instead of settling into
/// `failed` the way a one-shot problem should.
pub const WEB_UNIT_START_LIMIT_INTERVAL_SECS: u32 = 300;
pub const WEB_UNIT_START_LIMIT_BURST: u32 = 5;
pub const WEB_UNIT_RESTART_SECS: u32 = 30;
pub const WEB_UNIT_LIMIT_NOFILE: u32 = 1024;
pub const WEB_UNIT_MEMORY_MAX: &str = "1G";

/// How long the web interface's self-signed certificate stays valid before
/// it needs regenerating. `rcgen`'s own default (1975 to 4096) trains users
/// to ignore an expiry date that will never actually arrive; about two
/// years is long enough not to nag, short enough that a certificate this
/// old actually says something.
pub const WEB_CERT_VALIDITY: std::time::Duration = std::time::Duration::from_secs(2 * 365 * 86_400);

/// Shortest password Settings accepts for the web interface's shared
/// password (OWASP ASVS 5.0 §6.2's minimum for a user-chosen password with
/// no other strength check). The only rule before this was "not empty".
pub const WEB_PASSWORD_MIN_LENGTH: usize = 12;

/// The private address ranges an empty allow-list falls back to in `Lan`
/// scope, and the ranges a cross-site `Origin` check also treats as
/// same-machine-or-LAN rather than the public internet. IPv4 private ranges
/// (RFC 1918), link-local (RFC 3927), unique local IPv6 (RFC 4193) and IPv6
/// link-local: never the whole internet, even though the daemon is bound to
/// every interface.
pub const WEB_PRIVATE_RANGES: &[&str] = &[
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "127.0.0.0/8",
    "fc00::/7",
    "fe80::/10",
    "::1/128",
];

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

/// How many failed web interface login attempts an address gets before it
/// is locked out, and for how long: 5, 15 and 60 minutes, then capped at
/// 24 hours, one step further each time the address returns and fails
/// again after its previous lockout (or accumulation window) has fully
/// passed. A first-time mistake is cheap; a repeat offender's guesses get
/// expensive fast.
pub const WEB_LOCKOUT_MAX_ATTEMPTS: u32 = 5;
pub const WEB_LOCKOUT_LEVEL_SECS: [i64; 4] = [5 * 60, 15 * 60, 60 * 60, 24 * 60 * 60];

/// A burst of failures across many addresses at once is a campaign, not
/// one address's problem: past this many failures from anyone, in this
/// window, password auth is paused for everyone (an API token, unaffected
/// by guessing a password, keeps working) until the window passes.
pub const WEB_GLOBAL_BUDGET_MAX: u32 = 50;
pub const WEB_GLOBAL_BUDGET_WINDOW_SECS: i64 = 10 * 60;

/// How long a web interface address's escalation level is remembered
/// after its most recent failure, even once its own lockout has long since
/// passed: long enough that returning the next day still escalates,
/// bounded so the map backing it does not grow forever.
pub const WEB_ATTEMPTS_MEMORY_SECS: i64 = 7 * 86_400;

/// The web interface's own throttle map is capped at this many addresses;
/// past it, the least-recently-active one is dropped to make room for a
/// new one, rather than growing without bound.
pub const WEB_MAX_TRACKED_ADDRESSES: usize = 10_000;

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
