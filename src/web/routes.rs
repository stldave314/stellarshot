// SPDX-License-Identifier: GPL-3.0-only

//! The REST API's own routes: a backup's status, its snapshots, the files
//! inside one, and starting an existing backup. Nothing here creates a new
//! backup or restores one yet.
//!
//! The list of backups is read once, at startup (see [`super::main`]),
//! rather than from disk on every request: the same choice already made for
//! the daemon's own authentication settings, and it is what makes these
//! routes testable against a router built directly from a chosen list of
//! profiles, with no dependency on this machine's real settings.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::app::tasks;
use crate::debug::WEB;
use crate::engine::{self, EngineError, ErrorKind, SnapshotSummary, TreeEntry};
use crate::event_log;
use crate::profile::Profile;
use crate::runner::{self, Job, Operation, Output};
use crate::status::{self, Status};
use crate::error_log;

/// What every route needs: the backups to act on, and the exclusion patterns
/// that apply to all of them, the same as [`crate::scheduled`] reads once at
/// its own start rather than per run.
pub struct AppState {
    pub profiles: Vec<Profile>,
    pub global_exclude_patterns: Vec<String>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/backups", get(list_backups))
        .route("/api/v1/backups/{id}/snapshots", get(list_snapshots))
        .route(
            "/api/v1/backups/{id}/snapshots/{snapshot}/browse",
            get(browse),
        )
        .route("/api/v1/backups/{id}/run", post(run_backup))
        .with_state(state)
}

/// Wraps [`EngineError`] so a handler can return it directly with `?`; maps
/// each [`ErrorKind`] to the HTTP status a REST client should actually act
/// on, rather than one status for everything.
#[derive(Debug)]
struct ApiError(EngineError);

impl From<EngineError> for ApiError {
    fn from(err: EngineError) -> Self {
        Self(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0.kind {
            // Not this API's own credentials (see WWW-Authenticate on the
            // 401 `authenticate` itself sends): the repository's password
            // is a precondition of the request succeeding, not of the
            // caller's identity, so a valid token still sees a real status
            // rather than "your credentials are wrong."
            ErrorKind::WrongPassword | ErrorKind::PasswordNotRemembered => StatusCode::CONFLICT,
            ErrorKind::NotARepository | ErrorKind::NotFound => StatusCode::NOT_FOUND,
            // The client's own request named the problem, not the server:
            // `..` or an absolute path in a snapshot entry.
            ErrorKind::UnsafePath => StatusCode::BAD_REQUEST,
            ErrorKind::AlreadyExists | ErrorKind::Locked | ErrorKind::Canceled => {
                StatusCode::CONFLICT
            }
            ErrorKind::DestinationUnavailable
            | ErrorKind::TimedOut
            | ErrorKind::RcloneMissing
            | ErrorKind::AuthFailed => StatusCode::SERVICE_UNAVAILABLE,
            ErrorKind::LocationNotEmpty
            | ErrorKind::RepositoryDamaged
            | ErrorKind::Io
            | ErrorKind::KeyringUnavailable
            | ErrorKind::DeleteUnsupported
            | ErrorKind::ConditionsNotMet
            | ErrorKind::HookFailed
            | ErrorKind::AppUpdated
            | ErrorKind::InvalidRemote
            | ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        // `EngineError.detail` can hold the password command's stderr,
        // rclone's own stderr, or a local path — never handed to whoever
        // asked, even on a 5xx. It still goes to the log, tied to the same
        // ID the response carries, so it is not lost, only kept server-side.
        let request_id = uuid::Uuid::new_v4().to_string();
        error_log!(
            WEB,
            "request {request_id} failed ({status}): {}",
            self.0.detail
        );
        let body = ApiErrorBody {
            kind: self.0.kind,
            message: safe_message(self.0.kind),
            request_id,
        };
        (status, Json(body)).into_response()
    }
}

/// The response body for a failed request: never [`EngineError::detail`],
/// which is not safe to hand to whoever asked (see [`ApiError`]'s own
/// `IntoResponse`). `request_id` is what to mention when asking for help;
/// the matching detail is in this daemon's own log.
#[derive(Serialize)]
struct ApiErrorBody {
    kind: ErrorKind,
    message: &'static str,
    request_id: String,
}

/// A description of `kind` safe to hand to any caller: stable across
/// releases (unlike the localized, UI-facing text in `app::errors`, which
/// also is not built into this binary), and never anything technical that
/// `EngineError::detail` might have carried instead.
fn safe_message(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::WrongPassword => "the repository password is wrong",
        ErrorKind::PasswordNotRemembered => "no password is remembered for this backup",
        ErrorKind::NotARepository | ErrorKind::NotFound => {
            "no backup, snapshot or path matches this request"
        }
        ErrorKind::UnsafePath => "the requested path is not valid",
        ErrorKind::AlreadyExists => "a repository already exists at that location",
        ErrorKind::Locked => "the backup is already running",
        ErrorKind::Canceled => "the operation was canceled",
        ErrorKind::DestinationUnavailable => "the backup's storage could not be reached",
        ErrorKind::TimedOut => "the storage did not answer in time",
        ErrorKind::RcloneMissing => "rclone is required for this backup but not installed",
        ErrorKind::AuthFailed => "signing in to the cloud account did not complete",
        ErrorKind::LocationNotEmpty
        | ErrorKind::RepositoryDamaged
        | ErrorKind::Io
        | ErrorKind::KeyringUnavailable
        | ErrorKind::DeleteUnsupported
        | ErrorKind::ConditionsNotMet
        | ErrorKind::HookFailed
        | ErrorKind::AppUpdated
        | ErrorKind::InvalidRemote
        | ErrorKind::Internal => "the request could not be completed",
    }
}

fn not_found(what: &str) -> ApiError {
    ApiError(EngineError::new(ErrorKind::NotARepository, what.to_owned()))
}

fn no_password() -> ApiError {
    ApiError(EngineError::new(
        ErrorKind::PasswordNotRemembered,
        "no remembered password for this backup",
    ))
}

fn backup_by_id(profiles: &[Profile], id: &str) -> Result<Profile, ApiError> {
    profiles
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .ok_or_else(|| not_found("no backup with that ID"))
}

async fn list_backups(State(state): State<Arc<AppState>>) -> Json<Vec<Status>> {
    let now = jiff::Timestamp::now().as_second();
    Json(status::all(&state.profiles, now))
}

async fn list_snapshots(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<SnapshotSummary>>, ApiError> {
    let profile = backup_by_id(&state.profiles, &id)?;
    let location = profile.location()?;
    let secret = profile.password().await?.ok_or_else(no_password)?;
    let snapshots =
        tasks::blocking(move || Ok(engine::open(&location, &secret)?.browse()?.snapshots()))
            .await?;
    Ok(Json(snapshots))
}

#[derive(Deserialize)]
struct BrowseQuery {
    #[serde(default)]
    path: Option<PathBuf>,
}

async fn browse(
    State(state): State<Arc<AppState>>,
    Path((id, snapshot)): Path<(String, String)>,
    Query(query): Query<BrowseQuery>,
) -> Result<Json<Vec<TreeEntry>>, ApiError> {
    let profile = backup_by_id(&state.profiles, &id)?;
    let location = profile.location()?;
    let secret = profile.password().await?.ok_or_else(no_password)?;
    let dir = query.path.unwrap_or_else(|| PathBuf::from("/"));
    let entries = tasks::blocking(move || {
        engine::open(&location, &secret)?
            .browse()?
            .list(&snapshot, &dir)
    })
    .await?;
    Ok(Json(entries))
}

#[derive(Serialize)]
struct RunStarted {
    started: bool,
}

/// Starts an existing backup and returns immediately: a backup can run for a
/// long time, and the caller already has `GET /api/v1/backups` to poll for
/// when it finishes. Only a backup, not a restore or a new one — the same
/// restriction the desktop window's own "Back Up Now" applies, just reached
/// over the API instead of a button.
///
/// Runs in-process, through the same [`runner::run`] the window's own
/// `--run` child calls: this daemon is already its own process, so the
/// child-process isolation `crate::app::child` exists for (protecting the
/// *window's* long-lived process from a crash mid-backup) does not apply
/// here.
async fn run_backup(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<RunStarted>), ApiError> {
    let profile = backup_by_id(&state.profiles, &id)?;
    let location = profile.location()?;
    let secret = profile.password().await?.ok_or_else(no_password)?;
    let request = profile.backup_request(&state.global_exclude_patterns);
    let job = Job {
        request: Some(request),
        hooks: profile.hooks.clone(),
        ..Job::new(location.clone(), secret)
    };
    tokio::spawn(record_backup(profile.id, location, job));
    Ok((StatusCode::ACCEPTED, Json(RunStarted { started: true })))
}

/// Runs `job` and records what happened under [`event_log::Source::Web`],
/// the same way [`crate::scheduled`] records a timer's own run — except a
/// success or failure triggered here is never "quiet": nothing is polling a
/// timer's retry schedule, so the one place this result is visible at all is
/// the History page and the next `GET /api/v1/backups`.
async fn record_backup(profile_id: String, location: crate::engine::Location, job: Job) {
    let result = tasks::blocking(move || {
        runner::run(
            Operation::Backup,
            job,
            Arc::new(Output::progress_file_only(&location)),
        )
    })
    .await;
    let now = jiff::Timestamp::now().as_second();
    match result {
        Ok(_) => event_log::record(
            &profile_id,
            now,
            event_log::EventKind::BackedUp,
            event_log::Source::Web,
        ),
        Err(err) => event_log::record(
            &profile_id,
            now,
            event_log::EventKind::Failed {
                stage: crate::run_state::Stage::Backup,
                kind: err.kind,
                detail: err.detail,
            },
            event_log::Source::Web,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_engine_error_kind_maps_to_a_sensible_status() {
        let cases = [
            (ErrorKind::WrongPassword, StatusCode::CONFLICT),
            (ErrorKind::PasswordNotRemembered, StatusCode::CONFLICT),
            (ErrorKind::AuthFailed, StatusCode::SERVICE_UNAVAILABLE),
            (ErrorKind::NotARepository, StatusCode::NOT_FOUND),
            (ErrorKind::NotFound, StatusCode::NOT_FOUND),
            (ErrorKind::UnsafePath, StatusCode::BAD_REQUEST),
            (ErrorKind::Locked, StatusCode::CONFLICT),
            (
                ErrorKind::DestinationUnavailable,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (ErrorKind::Internal, StatusCode::INTERNAL_SERVER_ERROR),
        ];
        for (kind, expected) in cases {
            let error = ApiError(EngineError::new(kind, "test"));
            assert_eq!(error.into_response().status(), expected, "{kind:?}");
        }
    }

    /// `EngineError.detail` can hold a password command's stderr, rclone's
    /// own stderr, or a local path — none of it belongs in a response to
    /// whoever asked, only in this daemon's own log.
    #[tokio::test]
    async fn the_response_body_never_carries_the_engine_errors_own_detail() {
        let error = ApiError(EngineError::new(ErrorKind::Internal, "LEAKME: secret detail"));
        let body = error.into_response().into_body();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(!text.contains("LEAKME"), "leaked the detail: {text}");
        assert!(text.contains("\"request_id\""), "missing a request ID: {text}");
        assert!(text.contains("\"kind\":\"internal\""), "missing the kind: {text}");
    }

    #[test]
    fn backup_by_id_finds_the_matching_profile_only() {
        let profiles = vec![test_profile("a"), test_profile("b")];
        assert_eq!(backup_by_id(&profiles, "b").unwrap().id, "b");
        assert!(backup_by_id(&profiles, "missing").is_err());
    }

    fn test_profile(id: &str) -> Profile {
        let mut profile = Profile::new(
            id.to_owned(),
            crate::profile::Destination::Local {
                path: PathBuf::from("/tmp"),
            },
            Vec::new(),
        );
        profile.id = id.to_owned();
        profile
    }

    /// A real backup, its own temporary repository, and a `Profile` whose
    /// password comes from a command (`echo`, run for real, not the
    /// keyring) rather than anything this machine's own settings hold.
    struct RealBackup {
        _dir: tempfile::TempDir,
        profile: Profile,
    }

    fn real_backup() -> RealBackup {
        real_backup_with_id("real")
    }

    /// [`real_backup`], with a chosen profile ID: the event log a test
    /// triggering a real run lands in is this machine's own real state
    /// store, keyed by profile ID with no test isolation of its own — a
    /// fresh random ID (see `starting_a_backup_records_it_in_the_history_under_the_web_source`)
    /// keeps it from ever colliding with a real backup's history.
    fn real_backup_with_id(id: &str) -> RealBackup {
        let dir = tempfile::TempDir::new().unwrap();
        let repo_path = dir.path().join("repo");
        let source = dir.path().join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("hello.txt"), b"hello from a real backup").unwrap();

        let secret = engine::Secret::new("correct horse battery staple");
        let location = engine::Location::local(&repo_path);
        engine::init(&location, &secret).unwrap();
        engine::open(&location, &secret)
            .unwrap()
            .backup(
                &engine::BackupRequest {
                    sources: vec![source.clone()],
                    ..engine::BackupRequest::default()
                },
                Arc::new(engine::NoProgress),
            )
            .unwrap();

        let mut profile = test_profile(id);
        profile.destination = crate::profile::Destination::Local { path: repo_path };
        profile.password_command = "echo correct horse battery staple".to_owned();
        profile.sources = vec![source];
        RealBackup { _dir: dir, profile }
    }

    /// Serve `profiles` on a real, ephemeral loopback port: these routes
    /// have no allow-list or authentication of their own (that is
    /// `super::super`'s job, tested there), so a request here needs neither.
    async fn spawn(profiles: Vec<Profile>) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(AppState {
            profiles,
            global_exclude_patterns: Vec::new(),
        });
        tokio::spawn(async move { axum::serve(listener, router(state)).await });
        addr
    }

    /// A bare-bones HTTP/1.1 client good enough to read a status code and a
    /// JSON body: real bytes over a real socket, matching the same approach
    /// already proven against this router's own allow-list and
    /// authentication in `super::super::tests`, without a second HTTP
    /// client dependency for one request shape.
    async fn get_json(addr: std::net::SocketAddr, path: &str) -> (u16, serde_json::Value) {
        request(addr, "GET", path).await
    }

    async fn post_json(addr: std::net::SocketAddr, path: &str) -> (u16, serde_json::Value) {
        request(addr, "POST", path).await
    }

    async fn request(
        addr: std::net::SocketAddr,
        method: &str,
        path: &str,
    ) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                format!(
                    "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        let status = response
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .expect("a status line");
        let body = response
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .unwrap_or_default();
        let json = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn a_real_request_lists_the_configured_backup() {
        let backup = real_backup();
        let id = backup.profile.id.clone();
        let addr = spawn(vec![backup.profile]).await;

        let (status, body) = get_json(addr, "/api/v1/backups").await;

        assert_eq!(status, 200);
        assert_eq!(body[0]["profile_id"], id);
    }

    #[tokio::test]
    async fn a_real_request_lists_the_one_real_snapshot() {
        let backup = real_backup();
        let id = backup.profile.id.clone();
        let addr = spawn(vec![backup.profile]).await;

        let (status, body) = get_json(addr, &format!("/api/v1/backups/{id}/snapshots")).await;

        assert_eq!(status, 200);
        assert_eq!(body.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_real_request_browses_the_backed_up_file() {
        let backup = real_backup();
        let id = backup.profile.id.clone();
        let source = backup.profile.sources.first().cloned().unwrap_or_default();
        let addr = spawn(vec![backup.profile]).await;

        let path = format!(
            "/api/v1/backups/{id}/snapshots/latest/browse?path={}",
            source.display()
        );
        let (status, body) = get_json(addr, &path).await;

        assert_eq!(status, 200);
        assert_eq!(body[0]["name"], "hello.txt");
    }

    #[tokio::test]
    async fn a_real_request_for_an_unknown_backup_is_not_found() {
        let addr = spawn(Vec::new()).await;

        let (status, _) = get_json(addr, "/api/v1/backups/nope/snapshots").await;

        assert_eq!(status, 404);
    }

    /// Waits for a second snapshot to appear, or panics: proves the run
    /// actually finished rather than merely that the request returned, since
    /// [`run_backup`] answers before the backup itself is done.
    async fn wait_for_a_second_snapshot(addr: std::net::SocketAddr, id: &str) {
        for _ in 0..100 {
            let (_, body) = get_json(addr, &format!("/api/v1/backups/{id}/snapshots")).await;
            if body
                .as_array()
                .is_some_and(|snapshots| snapshots.len() == 2)
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("a second snapshot never appeared");
    }

    #[tokio::test]
    async fn a_real_post_starts_an_existing_backup_and_a_new_snapshot_appears() {
        let backup = real_backup();
        let id = backup.profile.id.clone();
        let addr = spawn(vec![backup.profile]).await;

        let (status, body) = post_json(addr, &format!("/api/v1/backups/{id}/run")).await;

        assert_eq!(status, 202);
        assert_eq!(body["started"], true);
        wait_for_a_second_snapshot(addr, &id).await;
    }

    #[tokio::test]
    async fn a_real_post_for_an_unknown_backup_is_not_found() {
        let addr = spawn(Vec::new()).await;

        let (status, _) = post_json(addr, "/api/v1/backups/nope/run").await;

        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn starting_a_backup_records_it_in_the_history_under_the_web_source() {
        // A fresh random ID: `event_log` has no notion of a test-only
        // namespace, so this is what keeps a real run made by this test from
        // ever landing in — or colliding with — a real backup's own history.
        let backup = real_backup_with_id(&uuid::Uuid::new_v4().to_string());
        let id = backup.profile.id.clone();
        let addr = spawn(vec![backup.profile]).await;

        post_json(addr, &format!("/api/v1/backups/{id}/run")).await;
        wait_for_a_second_snapshot(addr, &id).await;

        let events = event_log::load(&id);
        let last = events.last().expect("the run left a history entry");
        assert_eq!(last.source, event_log::Source::Web);
        assert!(matches!(last.kind, event_log::EventKind::BackedUp));
    }
}
