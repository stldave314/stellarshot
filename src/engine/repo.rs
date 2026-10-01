// SPDX-License-Identifier: GPL-3.0-only

//! Opening and creating repositories.

use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustic_backend::BackendOptions;
use rustic_core::{
    ConfigOptions, Credentials, KeyOptions, OpenStatus, Repository, RepositoryBackends,
    RepositoryOptions,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::error::{EngineError, ErrorKind};
use super::location::{InitCheck, check_init_location, is_repository};
use super::progress::{ProgressSink, SinkBars, SlotGuard};
use super::serve::Serve;
use super::uploads::ParallelUploads;
use crate::constants::RCLONE_SERVE_FLAGS;
use crate::debug::ENGINE;
use crate::debug_log;

/// A repository password. Never printed, and wiped from memory on drop:
/// backed by `secrecy::SecretString` rather than a plain `String`, so
/// `drop`ping a `Job` or a cloned copy of one actually zeroizes the bytes
/// instead of just freeing them (a plain `String`'s allocator free leaves
/// the bytes as they were, recoverable from a core dump or a use-after-free
/// elsewhere in the same process).
#[derive(Clone)]
pub struct Secret(secrecy::SecretString);

impl Secret {
    pub fn new(password: impl Into<String>) -> Self {
        Self(secrecy::SecretString::from(password.into()))
    }

    pub fn expose(&self) -> &str {
        use secrecy::ExposeSecret;
        self.0.expose_secret()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// Deliberately exposes the secret: serializing a `Job` onto the `--run`
/// child's stdin (see `runner.rs`) is the one place a `Secret` leaves this
/// process's memory at all. `secrecy::SecretString` refuses a derived
/// `Serialize` specifically to prevent this happening *by accident*
/// (`SerializableSecret` is an opt-in marker trait `str` deliberately does
/// not implement); this hand-written impl exists because that boundary here
/// is deliberate and singular, not accidental — `grep -rn expose_secret src`
/// should only ever find this, `expose()` above, and nothing else.
impl Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.expose().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        secrecy::SecretString::deserialize(deserializer).map(Self)
    }
}

/// Where a repository lives.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Location {
    /// A folder on this computer (including a mounted drive).
    Local { path: PathBuf },
    /// A path on an rclone remote: SFTP, cloud storage, or rclone's own
    /// `:local:` backend in tests. `config` is the rclone configuration that
    /// defines the remote.
    Rclone {
        remote: String,
        path: String,
        config: PathBuf,
        /// rclone's `--bwlimit` syntax (`"1M"`, `"8M:2M"` for up:down, or
        /// empty for no limit).
        #[serde(default)]
        bandwidth_limit: String,
    },
    /// A repository on a rest-server or rustic-server, reached directly
    /// (not through rclone). `url` is the full server URL including the
    /// repository name and any HTTP basic auth (`http://user:pass@host:port/repo/`).
    Rest { url: String },
}

/// Hand-written so `Rest`'s `url` is redacted, the same reason `Secret`'s own
/// `Debug` above is hand-written: a `#[derive]` would print the URL's HTTP
/// basic auth credentials straight into a log line or `format!("{:?}", …)`.
impl fmt::Debug for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local { path } => f.debug_struct("Local").field("path", path).finish(),
            Self::Rclone {
                remote,
                path,
                config,
                bandwidth_limit,
            } => f
                .debug_struct("Rclone")
                .field("remote", remote)
                .field("path", path)
                .field("config", config)
                .field("bandwidth_limit", bandwidth_limit)
                .finish(),
            Self::Rest { url } => f
                .debug_struct("Rest")
                .field("url", &redact_url(url))
                .finish(),
        }
    }
}

impl Location {
    /// A repository in a local folder.
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self::Local { path: path.into() }
    }

    /// A repository on an rclone remote defined in Stellarshot's own rclone
    /// configuration.
    pub fn rclone(remote: impl Into<String>, path: impl Into<String>) -> Self {
        Self::Rclone {
            remote: remote.into(),
            path: path.into(),
            config: super::rclone::config_path(),
            bandwidth_limit: String::new(),
        }
    }

    /// The same location, with a bandwidth limit applied if this is an
    /// rclone location; a no-op for a local one, which has no transfer to
    /// limit.
    #[must_use]
    pub fn with_bandwidth_limit(mut self, limit: &str) -> Self {
        if let Self::Rclone {
            bandwidth_limit, ..
        } = &mut self
        {
            *bandwidth_limit = limit.to_owned();
        }
        self
    }

    /// The folder, for a local repository.
    pub fn local_path(&self) -> Option<&Path> {
        match self {
            Self::Local { path } => Some(path),
            Self::Rclone { .. } | Self::Rest { .. } => None,
        }
    }

    /// How the location reads in messages. Never includes a REST URL's own
    /// embedded credentials, if it has any.
    pub fn describe(&self) -> String {
        match self {
            Self::Local { path } => path.display().to_string(),
            Self::Rclone { remote, path, .. } => super::rclone::target(remote, path),
            Self::Rest { url } => redact_url(url),
        }
    }

    /// A short stable identifier, used to name the lock and progress files
    /// every process writing to this repository shares.
    ///
    /// A local path is canonicalized first (falling back to the raw path if
    /// that fails, which it always will for a destination that does not
    /// exist yet), so `/backups/home`, `/backups/home/` and a symlinked
    /// alias for the same folder all share one lock — rustic takes no lock
    /// of its own, so two different keys naming what is really one
    /// repository would let a backup and a prune run against it at once. A
    /// REST URL's own credentials are dropped first too, so they never end
    /// up as part of a lock file's name.
    pub fn key(&self) -> String {
        Self::digest(&self.key_bytes())
    }

    fn key_bytes(&self) -> Vec<u8> {
        match self {
            Self::Local { path } => canonicalize_or_raw(path)
                .as_os_str()
                .as_encoded_bytes()
                .to_vec(),
            Self::Rclone { remote, path, .. } => super::rclone::target(remote, path).into_bytes(),
            Self::Rest { url } => redact_url(url).into_bytes(),
        }
    }

    /// The key an older Stellarshot (before canonicalization and REST
    /// redaction landed here) would have computed for this same location,
    /// kept for one release so its lock is also visible to one still
    /// running during a package upgrade. `None` once it is identical to
    /// [`Location::key`] — the common case, an `Rclone` location always,
    /// and any `Local` one whose path was already canonical — so a caller
    /// does not take a second lock file for nothing.
    pub(super) fn legacy_key(&self) -> Option<String> {
        let legacy_bytes = match self {
            Self::Local { path } => path.as_os_str().as_encoded_bytes().to_vec(),
            Self::Rclone { .. } => return None,
            Self::Rest { url } => url.as_bytes().to_vec(),
        };
        let legacy = Self::digest(&legacy_bytes);
        (legacy != self.key()).then_some(legacy)
    }

    fn digest(bytes: &[u8]) -> String {
        Sha256::digest(bytes)[..8]
            .iter()
            .fold(String::with_capacity(16), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    }

    /// What rustic needs to reach this location, and, for one reached over
    /// rclone, the `rclone serve restic` process it talks to, which must be
    /// kept alive (and dropped after) for as long as rustic uses it.
    fn backend_options(&self) -> Result<(BackendOptions, Option<Serve>), EngineError> {
        match self {
            Self::Local { path } => {
                let repository = path.to_str().map(str::to_owned).ok_or_else(|| {
                    EngineError::new(
                        ErrorKind::Io,
                        format!("{} is not valid UTF-8", path.display()),
                    )
                })?;
                Ok((BackendOptions::default().repository(repository), None))
            }
            Self::Rclone {
                remote,
                path,
                config,
                bandwidth_limit,
            } => {
                if !super::rclone::available() {
                    return Err(EngineError::new(ErrorKind::RcloneMissing, "rclone"));
                }
                if config.to_str().is_none() {
                    return Err(EngineError::new(
                        ErrorKind::Io,
                        format!("{} is not valid UTF-8", config.display()),
                    ));
                }
                let serve = Serve::start(
                    &rclone_command(config, bandwidth_limit),
                    &super::rclone::target(remote, path),
                )?;
                let options = BackendOptions::default().repository(format!("rest:{}", serve.url()));
                Ok((options, Some(serve)))
            }
            Self::Rest { url } => Ok((
                BackendOptions::default().repository(format!("rest:{url}")),
                None,
            )),
        }
    }

    /// Fail with `DestinationUnavailable` when a local location cannot be
    /// reached at all, as with an unplugged drive whose mount point has gone.
    /// Remote locations are checked by trying them.
    fn check_reachable(&self) -> Result<(), EngineError> {
        let Self::Local { path } = self else {
            return Ok(());
        };
        let reachable = path.exists() || path.parent().is_some_and(Path::exists);
        if reachable {
            Ok(())
        } else {
            Err(EngineError::new(
                ErrorKind::DestinationUnavailable,
                path.display().to_string(),
            ))
        }
    }
}

/// The `rclone serve restic` command line rustic runs for an `Rclone`
/// location, built as its own pure function so it can be tested without
/// rclone actually being installed — `backend_options`'s own `available()`
/// check, which does need it, guards the only call site.
///
/// rustic re-splits this whole string with `shell_words`, not a real shell,
/// but that still means a value containing a quote can end its own argument
/// and start a new one — quoted with `shell_words::quote` rather than a
/// hand-written `'...'`, so whatever `bandwidth_limit` contains can never
/// do that.
fn rclone_command(config: &Path, bandwidth_limit: &str) -> String {
    let mut command = format!(
        "rclone serve restic --addr localhost:0 --config {} {}",
        shell_words::quote(&config.display().to_string()),
        RCLONE_SERVE_FLAGS.join(" ")
    );
    if !bandwidth_limit.is_empty() {
        let _ = write!(
            command,
            " --bwlimit {}",
            shell_words::quote(bandwidth_limit)
        );
    }
    // Defense in depth against a remote name outside the shape
    // `Destination::location` already enforces (see SEC-1): a trailing `--`
    // means anything rustic_backend or rustic_core appends after this
    // string, however it got here, can never be parsed as an rclone flag.
    command.push_str(" --");
    command
}

/// `path`, canonicalized, or `path` itself if that fails — a destination
/// that does not exist yet always does, and this is only ever used to name
/// a lock file, not to open anything, so falling back rather than erroring
/// out is the right default.
pub(super) fn canonicalize_or_raw(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `url` with any embedded HTTP basic auth (`user:pass@`) removed, for
/// showing in messages, logs, and a settings export.
/// Removes any `userinfo@` (`user:pass@`, or just `user@`) immediately
/// after a `scheme://` found anywhere inside `text`, for a message that is
/// not itself a bare URL (use [`redact_url`] for that) but may have one
/// embedded in it — an error from `rustic_backend` or the `reqwest` it uses
/// for a REST location, which can include the URL it was trying to reach,
/// credentials and all, in its own `Display` text.
///
/// Conservative on purpose: only scans within one "word" (no whitespace)
/// after `://`, and only treats an `@` in it as userinfo's end when nothing
/// before it looks like a path separator — otherwise the word is left
/// untouched, rather than risk mangling unrelated text that merely contains
/// `://`.
pub fn scrub_url_credentials(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(scheme_at) = rest.find("://") {
        let (before, after) = rest.split_at(scheme_at + 3);
        result.push_str(before);
        let word_end = after.find(char::is_whitespace).unwrap_or(after.len());
        let (word, tail) = after.split_at(word_end);
        match word.find('@') {
            Some(at) if !word[..at].contains('/') => result.push_str(&word[at + 1..]),
            _ => result.push_str(word),
        }
        rest = tail;
    }
    result.push_str(rest);
    result
}

pub fn redact_url(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        // A URL that fails to parse at all is typically one whose password
        // contains a character (`/`, `?`, `#`) the URL syntax does not
        // allow unescaped there — which also means there is no reliable way
        // to tell by hand where credentials would end. Showing the raw text
        // risks showing them, so this fails closed instead of open.
        return "(a URL that could not be parsed, redacted)".to_owned();
    };
    if parsed.username().is_empty() && parsed.password().is_none() {
        return url.to_owned();
    }
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.to_string()
}

/// What a location holds, before anything is created or opened there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Probe {
    /// Missing or empty: a repository can be created here.
    Empty,
    /// Already a repository: open it.
    Repository,
    /// Holds other files: neither create nor open.
    NotEmpty,
}

/// Classify a location without touching it.
pub fn probe(location: &Location) -> Result<Probe, EngineError> {
    location.check_reachable()?;
    match location {
        Location::Local { path } => Ok(match check_init_location(path)? {
            InitCheck::Empty => Probe::Empty,
            InitCheck::ExistingRepository => Probe::Repository,
            InitCheck::NotEmpty => Probe::NotEmpty,
        }),
        Location::Rclone {
            remote,
            path,
            config,
            ..
        } => super::rclone::probe(config, remote, path),
        // No generic way to list a REST server path's contents the way a
        // filesystem walk or `rclone lsf` does, so "holds other files, not
        // a repository" is not distinguished here: only whether a `config`
        // file already exists.
        Location::Rest { .. } => {
            let bars = SinkBars::default();
            match unopened(location, &bars)?.0.config_id()? {
                Some(_) => Ok(Probe::Repository),
                None => Ok(Probe::Empty),
            }
        }
    }
}

/// Delete the repository at `location`: only the entries the repository
/// format creates, never anything else there.
pub fn delete_repository(location: &Location) -> Result<(), EngineError> {
    match location {
        Location::Local { path } => {
            super::location::delete_repository(path)?;
            Ok(())
        }
        Location::Rclone {
            remote,
            path,
            config,
            ..
        } => super::rclone::delete_repository(config, remote, path),
        // Deleting only the repository's own entries, never anything else
        // at that path, needs a generic directory listing; rustic_core
        // exposes no public API for that against a REST server. "Remove
        // from Stellarshot" (forgetting it here, leaving the data) still
        // works; only wiping it from here does not.
        // The user-facing explanation lives with the other localized error
        // text (`app::errors::explain`), not here: this is a written
        // sentence for a person to read, not rustic's own technical detail,
        // so it goes through `fl!()` rather than being hardcoded in English.
        Location::Rest { .. } => Err(EngineError::new(ErrorKind::DeleteUnsupported, "")),
    }
}

/// An open repository.
pub struct Repo {
    pub(crate) location: Location,
    pub(crate) bars: SinkBars,
    pub(crate) inner: Repository<OpenStatus>,
    /// The rclone process `inner` talks to. After it, so it is dropped after
    /// it.
    pub(crate) serve: Option<Serve>,
}

impl fmt::Debug for Repo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Repo")
            .field("location", &self.location)
            .finish()
    }
}

impl Repo {
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Route rustic's progress to `sink` until the guard is dropped.
    pub(crate) fn report_to(&self, sink: Arc<dyn ProgressSink>) -> SlotGuard {
        self.bars.slot.attach(sink)
    }

    /// Whether this repository is append-only: `forget` and `prune` refuse
    /// to run against it, the same as rustic's own tools, rather than
    /// failing against the server on every attempt.
    pub fn is_append_only(&self) -> bool {
        self.inner.config().append_only == Some(true)
    }

    /// The compression level actually stored in the repository's own
    /// config, or `None` if it was created without overriding rustic's
    /// default.
    pub fn compression_level(&self) -> Option<i32> {
        self.inner.config().compression
    }
}

fn unopened(
    location: &Location,
    bars: &SinkBars,
) -> Result<(Repository<()>, Option<Serve>), EngineError> {
    let (options, serve) = location.backend_options()?;
    let mut backends = options.to_backends()?;
    // Storage behind a network connection gets several uploads at once.
    if let Location::Rclone { .. } = location {
        let uploads = ParallelUploads::new(backends.repository(), bars.slot.clone());
        backends = RepositoryBackends::new(Arc::new(uploads), backends.repo_hot());
    }
    let mut options = RepositoryOptions::default();
    super::cache_settings::apply(&mut options);
    // A test build that has not chosen a cache location of its own gets a
    // private one, with the cache *on*: see `cache_settings::test_cache_dir`
    // for why that, and not `no_cache`, is what keeps tests out of
    // `~/.cache/rustic`.
    #[cfg(feature = "test-support")]
    if options.cache_dir.is_none() && !options.no_cache {
        options.cache_dir = Some(super::cache_settings::test_cache_dir());
    }
    Ok((
        Repository::new_with_progress(&options, &backends, bars.clone())?,
        serve,
    ))
}

/// `err` as [`ErrorKind::DestinationUnavailable`] when it says a remote
/// location could not be reached at all (see
/// [`super::error::looks_unreachable`]): no network or a server that is down
/// is "try again at the next slot", not a failure worth a notification.
fn unreachable(location: &Location, err: EngineError) -> EngineError {
    let remote = !matches!(location, Location::Local { .. });
    if remote && err.kind == ErrorKind::Internal && super::error::looks_unreachable(&err.detail) {
        EngineError::new(ErrorKind::DestinationUnavailable, err.detail)
    } else {
        err
    }
}

/// Create a repository. Refuses a location that already holds a repository or
/// anything else.
pub fn init(location: &Location, secret: &Secret) -> Result<Repo, EngineError> {
    init_with(location, secret, false, None)
}

/// Create a repository, optionally in append-only mode and at a chosen
/// compression level from the start.
///
/// Append-only can only be chosen here, at creation, through Stellarshot's
/// own tools: rustic's `config` command, the only way to change it, refuses
/// every change to an append-only repository except turning append-only
/// back off, which Stellarshot exposes no UI for. (Once off, `config` works
/// normally again, including turning it back on — so this is not a
/// guarantee against someone with the repository's own password, only
/// against Stellarshot's own tools never doing it by themselves.) The
/// compression level is not one-way the same way: `rustic`'s own `config`
/// command could still change it later, Stellarshot simply has no UI for
/// that yet either.
pub fn init_with(
    location: &Location,
    secret: &Secret,
    append_only: bool,
    compression: Option<i32>,
) -> Result<Repo, EngineError> {
    match probe(location)? {
        Probe::NotEmpty => {
            return Err(EngineError::new(
                ErrorKind::LocationNotEmpty,
                location.describe(),
            ));
        }
        Probe::Repository => {
            return Err(EngineError::new(
                ErrorKind::AlreadyExists,
                location.describe(),
            ));
        }
        Probe::Empty => {}
    }
    debug_log!(
        ENGINE,
        "init {} (append-only: {append_only}, compression: {compression:?})",
        location.describe()
    );
    let bars = SinkBars::default();
    let mut config = ConfigOptions::default();
    if append_only {
        config = config.set_append_only(true);
    }
    if let Some(level) = compression {
        config = config.set_compression(level);
    }
    let (unopened, serve) = unopened(location, &bars)?;
    let inner = unopened.init(
        &Credentials::password(secret.expose()),
        &KeyOptions::default(),
        &config,
    )?;
    Ok(Repo {
        location: location.clone(),
        bars,
        inner,
        serve,
    })
}

/// Open an existing repository.
pub fn open(location: &Location, secret: &Secret) -> Result<Repo, EngineError> {
    location.check_reachable()?;
    if let Location::Local { path } = location
        && !is_repository(path)
    {
        // Nothing there at all, or an empty folder: most often a network
        // share or a drive whose mount point stays behind when it is not
        // mounted. That is "not reachable now" (a scheduled run skips it
        // quietly), not a folder that holds something else.
        let empty_or_missing =
            std::fs::read_dir(path).map_or(true, |mut entries| entries.next().is_none());
        if empty_or_missing {
            return Err(EngineError::new(
                ErrorKind::DestinationUnavailable,
                path.display().to_string(),
            ));
        }
        return Err(EngineError::not_a_repository(path));
    }
    debug_log!(ENGINE, "open {}", location.describe());
    let bars = SinkBars::default();
    let (unopened, serve) = unopened(location, &bars).map_err(|err| unreachable(location, err))?;
    let inner = unopened
        .open(&Credentials::password(secret.expose()))
        .map_err(|err| unreachable(location, err.into()))?;
    Ok(Repo {
        location: location.clone(),
        bars,
        inner,
        serve,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secrets_debug_output_is_redacted() {
        let secret = Secret::new("hunter2");
        assert_eq!(format!("{secret:?}"), "Secret(***)");
    }

    #[test]
    fn a_secret_round_trips_through_json_as_a_bare_string() {
        // `#[serde(transparent)]`'s old wire format, kept even though the
        // derive is gone: a `Job` already saved or logged as JSON, and every
        // existing `--run` child expecting this shape on its own stdin,
        // must keep working.
        let secret = Secret::new("hunter2");
        let json = serde_json::to_string(&secret).unwrap();
        assert_eq!(json, "\"hunter2\"");
        let restored: Secret = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.expose(), "hunter2");
    }

    /// The value of `--bwlimit` in the built command, split the same way
    /// rustic itself re-splits the whole string (`shell_words`, not a real
    /// shell) rather than a substring check, so a test cannot pass just
    /// because the raw text happens to look right.
    fn bwlimit_argument(command: &str) -> Option<String> {
        let args = shell_words::split(command).unwrap();
        args.iter()
            .position(|arg| arg == "--bwlimit")
            .and_then(|i| args.get(i + 1).cloned())
    }

    #[test]
    fn a_bandwidth_limit_is_passed_to_rclone() {
        let command = rclone_command(Path::new("/tmp/rclone.conf"), "1M");
        assert_eq!(bwlimit_argument(&command).as_deref(), Some("1M"));
    }

    #[test]
    fn no_bandwidth_limit_adds_no_flag() {
        let command = rclone_command(Path::new("/tmp/rclone.conf"), "");
        assert!(
            !command.contains("--bwlimit"),
            "an empty limit must not add the flag: {command}"
        );
    }

    #[test]
    fn a_bandwidth_limit_cannot_inject_a_second_rclone_argument() {
        let hostile = "1M' --password-command 'evil";
        let command = rclone_command(Path::new("/tmp/rclone.conf"), hostile);
        assert_eq!(
            bwlimit_argument(&command).as_deref(),
            Some(hostile),
            "the whole hostile value must survive as one argument, not split into several"
        );
        assert!(
            !shell_words::split(&command)
                .unwrap()
                .contains(&"--password-command".to_owned()),
            "quoting must stop it from ever becoming its own argument: {command}"
        );
    }

    #[test]
    fn the_built_command_ends_with_a_bare_double_dash() {
        for bandwidth_limit in ["", "1M"] {
            let command = rclone_command(Path::new("/tmp/rclone.conf"), bandwidth_limit);
            let args = shell_words::split(&command).unwrap();
            assert_eq!(
                args.last().map(String::as_str),
                Some("--"),
                "nothing appended after this string may be parsed as a flag: {command}"
            );
        }
    }

    #[test]
    fn a_local_location_ignores_a_bandwidth_limit() {
        let location = Location::local("/tmp/somewhere").with_bandwidth_limit("1M");
        assert_eq!(location, Location::local("/tmp/somewhere"));
    }

    #[test]
    fn redact_url_removes_a_parseable_urls_credentials() {
        let redacted = redact_url("http://alex:s3cret@nas:8000/repo/");
        assert!(!redacted.contains("s3cret"));
        assert!(!redacted.contains("alex"));
        assert!(redacted.contains("nas:8000/repo/"));
    }

    #[test]
    fn redact_url_leaves_a_credential_free_url_alone() {
        assert_eq!(redact_url("http://nas:8000/repo/"), "http://nas:8000/repo/");
    }

    #[test]
    fn redact_url_shows_nothing_of_a_url_it_cannot_parse() {
        // A password containing an unescaped `/` makes this fail to parse
        // at all (there is no reliable way to tell where credentials end
        // without a working parse) — it must still not show the password.
        let redacted = redact_url("http://alex:p/ss@nas:8000/repo/");
        assert!(
            !redacted.contains("s@nas"),
            "must not fail open: {redacted}"
        );
        assert!(!redacted.contains("p/ss"), "must not fail open: {redacted}");
    }

    #[test]
    fn scrub_url_credentials_removes_userinfo_from_an_embedded_url() {
        let scrubbed = scrub_url_credentials(
            "error sending request for url (http://alex:s3cret@nas:8000/repo/config)",
        );
        assert!(!scrubbed.contains("s3cret"), "{scrubbed}");
        assert!(!scrubbed.contains("alex"), "{scrubbed}");
        assert!(scrubbed.contains("nas:8000/repo/config"), "{scrubbed}");
    }

    #[test]
    fn scrub_url_credentials_leaves_a_credential_free_message_alone() {
        let text = "error sending request for url (http://nas:8000/repo/config)";
        assert_eq!(scrub_url_credentials(text), text);
    }

    #[test]
    fn scrub_url_credentials_leaves_text_with_no_url_alone() {
        let text = "connection timed out after 60 seconds";
        assert_eq!(scrub_url_credentials(text), text);
    }

    #[test]
    fn scrub_url_credentials_does_not_mistake_a_path_separator_for_userinfos_end() {
        // `@` appearing after a `/` in the same word is part of a path or
        // query, not userinfo (there is no way to have both in one HTTP
        // URL), so this must be left alone rather than eating part of it.
        let text = "http://nas:8000/repo/user@example.com/config";
        assert_eq!(scrub_url_credentials(text), text);
    }

    #[test]
    fn a_trailing_slash_and_a_symlink_share_one_lock_key() {
        let dir = tempfile::TempDir::new().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let linked = dir.path().join("linked");
        std::os::unix::fs::symlink(&real, &linked).unwrap();

        let plain = Location::local(&real);
        let trailing_slash = Location::local(PathBuf::from(format!("{}/", real.display())));
        let via_symlink = Location::local(&linked);

        assert_eq!(plain.key(), trailing_slash.key());
        assert_eq!(plain.key(), via_symlink.key());
    }

    #[test]
    fn a_nonexistent_path_falls_back_to_its_raw_form_and_has_no_legacy_key() {
        // Nothing to canonicalize yet (the destination has not been
        // created), so the new key is exactly the old one: no second lock
        // file is needed for this location.
        let location = Location::local("/does/not/exist/repo");
        assert!(location.legacy_key().is_none());
    }

    #[test]
    fn a_rest_locations_legacy_key_drops_credentials_too() {
        let with_credentials = Location::Rest {
            url: "http://alex:s3cret@nas:8000/repo/".into(),
        };
        // The legacy key hashed the raw URL, credentials and all; the new
        // one never does. They differ, so an old binary's lock is still
        // reachable for one release.
        assert_ne!(
            with_credentials.key(),
            with_credentials.legacy_key().unwrap()
        );
    }

    #[test]
    fn an_rclone_location_never_has_a_legacy_key() {
        let location = Location::rclone("stellarshot-deadbeef", "backups");
        assert!(location.legacy_key().is_none());
    }
}
