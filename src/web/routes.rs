// SPDX-License-Identifier: GPL-3.0-only

//! The REST API's own routes: a backup's status, its snapshots, and the
//! files inside one. Every route reads only — nothing here writes to a
//! repository yet.
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
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::app::tasks;
use crate::engine::{self, EngineError, ErrorKind, SnapshotSummary, TreeEntry};
use crate::profile::Profile;
use crate::status::{self, Status};

pub fn router(profiles: Arc<Vec<Profile>>) -> Router {
    Router::new()
        .route("/api/v1/backups", get(list_backups))
        .route("/api/v1/backups/{id}/snapshots", get(list_snapshots))
        .route(
            "/api/v1/backups/{id}/snapshots/{snapshot}/browse",
            get(browse),
        )
        .with_state(profiles)
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
            ErrorKind::WrongPassword | ErrorKind::PasswordNotRemembered | ErrorKind::AuthFailed => {
                StatusCode::UNAUTHORIZED
            }
            ErrorKind::NotARepository => StatusCode::NOT_FOUND,
            ErrorKind::AlreadyExists | ErrorKind::Locked | ErrorKind::Canceled => {
                StatusCode::CONFLICT
            }
            ErrorKind::DestinationUnavailable | ErrorKind::TimedOut | ErrorKind::RcloneMissing => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            ErrorKind::LocationNotEmpty
            | ErrorKind::RepositoryDamaged
            | ErrorKind::Io
            | ErrorKind::KeyringUnavailable
            | ErrorKind::DeleteUnsupported
            | ErrorKind::ConditionsNotMet
            | ErrorKind::HookFailed
            | ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(self.0)).into_response()
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

async fn list_backups(State(profiles): State<Arc<Vec<Profile>>>) -> Json<Vec<Status>> {
    let now = jiff::Timestamp::now().as_second();
    Json(status::all(&profiles, now))
}

async fn list_snapshots(
    State(profiles): State<Arc<Vec<Profile>>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<SnapshotSummary>>, ApiError> {
    let profile = backup_by_id(&profiles, &id)?;
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
    State(profiles): State<Arc<Vec<Profile>>>,
    Path((id, snapshot)): Path<(String, String)>,
    Query(query): Query<BrowseQuery>,
) -> Result<Json<Vec<TreeEntry>>, ApiError> {
    let profile = backup_by_id(&profiles, &id)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_engine_error_kind_maps_to_a_sensible_status() {
        let cases = [
            (ErrorKind::WrongPassword, StatusCode::UNAUTHORIZED),
            (ErrorKind::NotARepository, StatusCode::NOT_FOUND),
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

        let mut profile = test_profile("real");
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
        tokio::spawn(async move { axum::serve(listener, router(Arc::new(profiles))).await });
        addr
    }

    /// A bare-bones HTTP/1.1 client good enough to read a status code and a
    /// JSON body: real bytes over a real socket, matching the same approach
    /// already proven against this router's own allow-list and
    /// authentication in `super::super::tests`, without a second HTTP
    /// client dependency for one request shape.
    async fn get_json(addr: std::net::SocketAddr, path: &str) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
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
}
