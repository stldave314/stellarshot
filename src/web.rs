// SPDX-License-Identifier: GPL-3.0-only

//! `stellarshot-web`: the web interface's own server.
//!
//! A per-user daemon, separate from the desktop window, so remote access
//! keeps working whether or not the window is open. Reads and writes will
//! go through the same engine and runner the window itself uses, the way
//! [`crate::scheduled`] already does for a scheduled backup: no subprocess,
//! since a daemon is already its own process — the child-process isolation
//! in `crate::app::child` exists to protect the *window's* long-lived
//! process, not this one.
//!
//! Every request meets, in order: the network scope's own bind address (a
//! request from outside it never arrives at all), the IP allow-list, then
//! authentication (a shared password, an API token, or — not yet wired up —
//! PAM). [`routes`] is the REST API itself, reachable only once a request
//! has passed all three.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::Engine;
use subtle::ConstantTimeEq;

use crate::app::config::{NetworkScope, StellarshotConfig};
use crate::debug::WEB;
use crate::engine::Secret;
use crate::profile::Profile;
use crate::{debug_log, error_log};

mod routes;

/// Fixed for now: not yet exposed as a setting.
const PORT: u16 = 8737;

/// What a request needs to satisfy to be let through, resolved once at
/// startup: the shared password is read from the keyring here rather than
/// on every request, since a keyring round-trip is real per-request latency
/// and a place authentication could fail open if the keyring hiccups.
/// Changing an auth setting takes effect on the next restart, the same as a
/// network scope change does.
struct AuthConfig {
    password_enabled: bool,
    password: Option<Secret>,
    token_enabled: bool,
    token_hash: Option<String>,
}

/// Entry point for the `stellarshot-web` binary.
pub fn main(_args: &[String]) -> ExitCode {
    crate::app::settings::set_logger_for_child();
    crate::core::localization::init();
    let config = StellarshotConfig::config();
    let Some(addr) = bind_address(config.web.scope) else {
        debug_log!(WEB, "network scope is off; not starting");
        return ExitCode::SUCCESS;
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            error_log!(WEB, "could not start the async runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(async move {
        let password = if config.web.password_enabled {
            crate::keyring::load_web_password().await
        } else {
            None
        };
        let auth = AuthConfig {
            password_enabled: config.web.password_enabled,
            password,
            token_enabled: config.web.token_enabled,
            token_hash: config.web.token_hash,
        };
        serve(addr, config.web.allowed_addresses, auth, config.profiles).await
    })
}

/// Where to listen, or `None` if the network scope is off.
fn bind_address(scope: NetworkScope) -> Option<SocketAddr> {
    match scope {
        NetworkScope::Off => None,
        NetworkScope::Localhost => Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), PORT)),
        NetworkScope::Lan => Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), PORT)),
    }
}

/// The router: an allow-list gate, then authentication, in front of every
/// route. Separate from [`serve`] so a test can mount it on a listener of
/// its own, on an ephemeral port, without going through [`main`]'s real
/// config and fixed [`PORT`].
///
/// Layers added with [`Router::layer`] run outermost-first — see axum's own
/// "Ordering" documentation for [`middleware`] — so the allow-list, added
/// last, is what a request meets first, before authentication is even
/// considered.
fn app(allowed_addresses: Vec<String>, auth: AuthConfig, profiles: Vec<Profile>) -> Router {
    let allowed = Arc::new(allowed_addresses);
    let auth = Arc::new(auth);
    Router::new()
        .route("/api/v1/health", get(health))
        .merge(routes::router(Arc::new(profiles)))
        .layer(middleware::from_fn_with_state(auth, authenticate))
        .layer(middleware::from_fn_with_state(allowed, allow_list))
}

async fn serve(
    addr: SocketAddr,
    allowed_addresses: Vec<String>,
    auth: AuthConfig,
    profiles: Vec<Profile>,
) -> ExitCode {
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(err) => {
            error_log!(WEB, "could not listen on {addr}: {err}");
            return ExitCode::FAILURE;
        }
    };
    debug_log!(WEB, "listening on {addr}");
    let result = axum::serve(
        listener,
        app(allowed_addresses, auth, profiles).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await;
    if let Err(err) = result {
        error_log!(WEB, "server stopped: {err}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
}

/// Reject anything not on the allow-list before it reaches any real route.
/// An empty list means every address the network scope itself already
/// allows, unrestricted.
async fn allow_list(
    State(allowed): State<Arc<Vec<String>>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    if is_allowed(&allowed, addr.ip()) {
        next.run(request).await
    } else {
        debug_log!(WEB, "rejected {addr}: not on the allow-list");
        StatusCode::FORBIDDEN.into_response()
    }
}

fn is_allowed(allowed: &[String], addr: IpAddr) -> bool {
    allowed.is_empty() || allowed.iter().any(|entry| matches(entry, addr))
}

/// `entry` is either a single address or a CIDR range; either matches `addr`.
fn matches(entry: &str, addr: IpAddr) -> bool {
    if let Ok(net) = entry.parse::<ipnet::IpNet>() {
        return net.contains(&addr);
    }
    entry.parse::<IpAddr>() == Ok(addr)
}

/// Require whichever of the enabled methods actually applies: a request is
/// let through if it satisfies *any* enabled method. If none is enabled,
/// every request is rejected — a daemon someone deliberately configured with
/// no way in should fail closed, not silently become an open one.
async fn authenticate(
    State(auth): State<Arc<AuthConfig>>,
    request: Request,
    next: Next,
) -> Response {
    if is_authenticated(&auth, request.headers()) {
        next.run(request).await
    } else {
        // Neither "no credentials" nor "wrong credentials" is distinguished
        // here, and no `WWW-Authenticate` header is sent: this is a REST
        // API, not a page a browser should pop its own login dialog for.
        StatusCode::UNAUTHORIZED.into_response()
    }
}

fn is_authenticated(auth: &AuthConfig, headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    if auth.password_enabled
        && let Some(password) = &auth.password
        && let Some(candidate) = basic_password(value)
        && constant_time_eq(candidate.as_bytes(), password.expose().as_bytes())
    {
        return true;
    }
    if auth.token_enabled
        && let Some(hash) = &auth.token_hash
        && let Some(token) = value.strip_prefix("Bearer ")
        && crate::web_token::verify(token, hash)
    {
        return true;
    }
    false
}

/// The password from an `Authorization: Basic <base64(username:password)>`
/// header. The username is not checked against anything: this is a single
/// shared password, not an account system.
fn basic_password(header_value: &str) -> Option<String> {
    let encoded = header_value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (_username, password) = text.split_once(':')?;
    Some(password.to_owned())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_off_scope_binds_nowhere() {
        assert_eq!(bind_address(NetworkScope::Off), None);
    }

    #[test]
    fn the_localhost_scope_binds_only_loopback() {
        let addr = bind_address(NetworkScope::Localhost).unwrap();
        assert!(addr.ip().is_loopback());
        assert_eq!(addr.port(), PORT);
    }

    #[test]
    fn the_lan_scope_binds_every_interface() {
        let addr = bind_address(NetworkScope::Lan).unwrap();
        assert_eq!(addr.ip(), IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    }

    #[test]
    fn an_empty_allow_list_allows_everything() {
        assert!(is_allowed(&[], "203.0.113.7".parse().unwrap()));
    }

    #[test]
    fn a_single_address_only_allows_itself() {
        let allowed = vec!["192.168.1.10".to_owned()];
        assert!(is_allowed(&allowed, "192.168.1.10".parse().unwrap()));
        assert!(!is_allowed(&allowed, "192.168.1.11".parse().unwrap()));
    }

    #[test]
    fn a_cidr_range_allows_every_address_inside_it() {
        let allowed = vec!["192.168.1.0/24".to_owned()];
        assert!(is_allowed(&allowed, "192.168.1.1".parse().unwrap()));
        assert!(is_allowed(&allowed, "192.168.1.254".parse().unwrap()));
        assert!(!is_allowed(&allowed, "192.168.2.1".parse().unwrap()));
    }

    #[test]
    fn an_unparseable_entry_matches_nothing_rather_than_panicking() {
        let allowed = vec!["not an address".to_owned()];
        assert!(!is_allowed(&allowed, "192.168.1.1".parse().unwrap()));
    }

    fn no_auth() -> AuthConfig {
        AuthConfig {
            password_enabled: false,
            password: None,
            token_enabled: false,
            token_hash: None,
        }
    }

    fn password_auth(password: &str) -> AuthConfig {
        AuthConfig {
            password_enabled: true,
            password: Some(Secret::new(password.to_owned())),
            ..no_auth()
        }
    }

    fn token_auth(hash: &str) -> AuthConfig {
        AuthConfig {
            token_enabled: true,
            token_hash: Some(hash.to_owned()),
            ..no_auth()
        }
    }

    #[test]
    fn no_credentials_at_all_are_never_authenticated() {
        assert!(!is_authenticated(
            &password_auth("secret"),
            &HeaderMap::new()
        ));
    }

    #[test]
    fn nothing_authenticates_when_no_method_is_enabled() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, basic_header("anything", "secret"));
        assert!(!is_authenticated(&no_auth(), &headers));
    }

    #[test]
    fn the_correct_shared_password_authenticates() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, basic_header("ignored", "secret"));
        assert!(is_authenticated(&password_auth("secret"), &headers));
    }

    #[test]
    fn the_wrong_shared_password_does_not_authenticate() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, basic_header("ignored", "wrong"));
        assert!(!is_authenticated(&password_auth("secret"), &headers));
    }

    #[test]
    fn a_valid_bearer_token_authenticates() {
        let token = crate::web_token::generate();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", token.raw).parse().unwrap(),
        );
        assert!(is_authenticated(&token_auth(&token.hash), &headers));
    }

    #[test]
    fn an_invalid_bearer_token_does_not_authenticate() {
        let token = crate::web_token::generate();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            "Bearer not-the-token".parse().unwrap(),
        );
        assert!(!is_authenticated(&token_auth(&token.hash), &headers));
    }

    #[test]
    fn basic_password_ignores_the_username() {
        let header = basic_header("alice", "secret");
        assert_eq!(
            basic_password(header.to_str().unwrap()),
            Some("secret".to_owned())
        );
    }

    #[test]
    fn constant_time_eq_still_compares_correctly() {
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"different-length"));
        assert!(!constant_time_eq(b"same", b"diff"));
    }

    fn basic_header(username: &str, password: &str) -> axum::http::HeaderValue {
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        format!("Basic {encoded}").parse().unwrap()
    }

    /// Serve `allowed_addresses`/`auth` (with no backups configured) on a
    /// real, ephemeral loopback port and return its address: an actual
    /// `TcpListener` and `axum::serve`, not a mocked request, so a wiring
    /// mistake between the middleware layers and `ConnectInfo` extraction
    /// (which only they working together can reveal) would show up here.
    async fn spawn(allowed_addresses: Vec<String>, auth: AuthConfig) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                app(allowed_addresses, auth, Vec::new())
                    .into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
        });
        addr
    }

    /// A bare-bones HTTP/1.1 client good enough to read one status line:
    /// real bytes over a real socket, without pulling in an HTTP client
    /// crate for a single request.
    async fn get(addr: SocketAddr, path: &str, authorization: Option<&str>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let auth_header = authorization
            .map(|value| format!("Authorization: {value}\r\n"))
            .unwrap_or_default();
        stream
            .write_all(
                format!(
                    "GET {path} HTTP/1.1\r\nHost: localhost\r\n{auth_header}Connection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    #[tokio::test]
    async fn a_real_request_from_an_address_not_on_the_allow_list_is_forbidden() {
        // Valid credentials, so a rejection here can only be the allow-list:
        // isolating the layer under test from the authentication layer.
        let addr = spawn(vec!["203.0.113.1".to_owned()], password_auth("secret")).await;
        let response = get(addr, "/api/v1/health", Some("Basic aWdub3JlZDpzZWNyZXQ=")).await;
        assert!(
            response.starts_with("HTTP/1.1 403"),
            "expected 403, got: {response}"
        );
    }

    #[tokio::test]
    async fn a_real_request_is_rejected_when_no_auth_method_is_enabled() {
        // No allow-list restriction either, so a rejection here can only be
        // authentication's own fail-closed default.
        let addr = spawn(Vec::new(), no_auth()).await;
        let response = get(addr, "/api/v1/health", None).await;
        assert!(
            response.starts_with("HTTP/1.1 401"),
            "expected 401, got: {response}"
        );
    }

    #[tokio::test]
    async fn a_real_request_with_the_correct_shared_password_reaches_the_health_route() {
        let addr = spawn(Vec::new(), password_auth("secret")).await;
        let response = get(addr, "/api/v1/health", Some("Basic aWdub3JlZDpzZWNyZXQ=")).await;
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "expected 200, got: {response}"
        );
        assert!(response.contains(r#"{"ok":true}"#), "body: {response}");
    }

    #[tokio::test]
    async fn a_real_request_with_the_wrong_shared_password_is_unauthorized() {
        let addr = spawn(Vec::new(), password_auth("secret")).await;
        let response = get(addr, "/api/v1/health", Some("Basic aWdub3JlZDp3cm9uZw==")).await;
        assert!(
            response.starts_with("HTTP/1.1 401"),
            "expected 401, got: {response}"
        );
    }

    #[tokio::test]
    async fn a_real_request_with_a_valid_api_token_reaches_the_health_route() {
        let token = crate::web_token::generate();
        let addr = spawn(Vec::new(), token_auth(&token.hash)).await;
        let response = get(
            addr,
            "/api/v1/health",
            Some(&format!("Bearer {}", token.raw)),
        )
        .await;
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "expected 200, got: {response}"
        );
    }
}
