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
//! This first version only binds according to the network scope setting and
//! enforces the IP allow-list; it has no authentication and exactly one
//! route, proving the plumbing before anything real is reachable through it.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};

use crate::app::config::{NetworkScope, StellarshotConfig, WebConfig};
use crate::debug::WEB;
use crate::{debug_log, error_log};

/// Fixed for now: not yet exposed as a setting.
const PORT: u16 = 8737;

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
    runtime.block_on(serve(addr, config.web))
}

/// Where to listen, or `None` if the network scope is off.
fn bind_address(scope: NetworkScope) -> Option<SocketAddr> {
    match scope {
        NetworkScope::Off => None,
        NetworkScope::Localhost => Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), PORT)),
        NetworkScope::Lan => Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), PORT)),
    }
}

/// The router: an allow-list gate in front of every route. Separate from
/// [`serve`] so a test can mount it on a listener of its own, on an
/// ephemeral port, without going through [`main`]'s real config and fixed
/// [`PORT`].
fn app(allowed_addresses: Vec<String>) -> Router {
    let allowed = Arc::new(allowed_addresses);
    Router::new()
        .route("/api/v1/health", get(health))
        .layer(middleware::from_fn_with_state(allowed, allow_list))
}

async fn serve(addr: SocketAddr, web: WebConfig) -> ExitCode {
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
        app(web.allowed_addresses).into_make_service_with_connect_info::<SocketAddr>(),
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

    /// Serve `allowed_addresses` on a real, ephemeral loopback port and
    /// return its address: an actual `TcpListener` and `axum::serve`, not a
    /// mocked request, so a wiring mistake between the allow-list layer and
    /// `ConnectInfo` extraction (which only the two working together can
    /// reveal) would show up here.
    async fn spawn(allowed_addresses: Vec<String>) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                app(allowed_addresses).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
        });
        addr
    }

    /// A bare-bones HTTP/1.1 client good enough to read one status line:
    /// real bytes over a real socket, without pulling in an HTTP client
    /// crate for a single request.
    async fn get(addr: SocketAddr, path: &str) -> String {
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
        response
    }

    #[tokio::test]
    async fn a_real_request_from_an_address_not_on_the_allow_list_is_forbidden() {
        // The client always connects from 127.0.0.1 in this test, so an
        // allow-list of some other address rejects it for real.
        let addr = spawn(vec!["203.0.113.1".to_owned()]).await;
        let response = get(addr, "/api/v1/health").await;
        assert!(
            response.starts_with("HTTP/1.1 403"),
            "expected 403, got: {response}"
        );
    }

    #[tokio::test]
    async fn a_real_request_with_no_allow_list_reaches_the_health_route() {
        let addr = spawn(Vec::new()).await;
        let response = get(addr, "/api/v1/health").await;
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "expected 200, got: {response}"
        );
        assert!(response.contains(r#"{"ok":true}"#), "body: {response}");
    }
}
