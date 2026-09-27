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
//! PAM), throttled per address after repeated failures. [`routes`] is the
//! REST API itself, reachable only once a request has passed all of that.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::Engine;
use sha2::Digest;
use subtle::ConstantTimeEq;

use crate::app::config::{NetworkScope, StellarshotConfig};
use crate::debug::WEB;
use crate::engine::Secret;
use crate::{debug_log, error_log};

mod routes;

/// This server's own origins: a request whose `Origin` matches none of
/// these, and which does not otherwise identify itself as same-origin via
/// `Sec-Fetch-Site`, is rejected by [`reject_cross_site`]. Built once at
/// startup from the network scope and port — `https://` plus `localhost`,
/// `127.0.0.1`, `[::1]`, and, in LAN scope, this machine's own hostname and
/// mDNS name (`<hostname>.local`, the address the LAN scope's own Settings
/// page shows; see `app::web_address`).
///
/// Does not enumerate this machine's actual LAN IP addresses. That is a
/// narrower, disclosed gap, not a security hole: a browser that opens the
/// documented mDNS address always sees its own page as same-origin
/// regardless of this set (`Sec-Fetch-Site` compares against the *page's*
/// origin, not this list); this list only matters as a fallback for a
/// browser old enough to send `Origin` but not `Sec-Fetch-Site`, and a miss
/// there fails closed (403), not open.
fn allowed_origins(scope: NetworkScope, port: u16) -> HashSet<url::Origin> {
    let mut origins = HashSet::new();
    let mut add = |host: &str| {
        if let Ok(url) = url::Url::parse(&format!("https://{host}:{port}")) {
            origins.insert(url.origin());
        }
    };
    match scope {
        NetworkScope::Off => {}
        NetworkScope::Localhost => {
            add("localhost");
            add("127.0.0.1");
            add("[::1]");
        }
        NetworkScope::Lan => {
            add("localhost");
            add("127.0.0.1");
            add("[::1]");
            let hostname = gethostname::gethostname().to_string_lossy().into_owned();
            add(&hostname);
            add(&format!("{hostname}.local"));
        }
    }
    origins
}

/// Cross-site requests never reach a route, regardless of credentials —
/// see [`allowed_origins`]'s own doc comment. This, together with three
/// properties enforced elsewhere and each covered by its own test (CORS
/// never enabled; a 401 challenges only `Bearer`, never `Basic`; no cookie
/// is ever set), is what keeps this cookie-free, CORS-free API safe from
/// CSRF. Anyone adding a cookie session here must add CSRF tokens first —
/// see the review's own WEB-12 for what an HTML interface would still need.
async fn reject_cross_site(
    State(origins): State<Arc<HashSet<url::Origin>>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    if let Some(reason) = cross_site_reason(&origins, request.headers()) {
        debug_log!(
            WEB,
            "rejected {addr} {} {}: {reason}",
            request.method(),
            request.uri().path()
        );
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

/// `Some(reason)` if `headers` describe a request that must be rejected as
/// cross-site. `Sec-Fetch-Site` is authoritative when present (every current
/// browser sends it); `Origin` is the fallback for one old enough not to.
/// Neither header present means a non-browser client (curl, a script, the
/// documented API examples), which is let through.
fn cross_site_reason(origins: &HashSet<url::Origin>, headers: &HeaderMap) -> Option<&'static str> {
    if let Some(site) = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
    {
        return match site {
            "same-origin" | "none" => None,
            _ => Some("Sec-Fetch-Site"),
        };
    }
    let origin = headers.get(header::ORIGIN)?.to_str().ok()?;
    if origin == "null" {
        return Some("Origin: null");
    }
    match url::Url::parse(origin) {
        Ok(url) if origins.contains(&url.origin()) => None,
        _ => Some("Origin"),
    }
}

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
    throttle: Throttle,
}

/// Entry point for the `stellarshot-web` binary.
pub fn main(_args: &[String]) -> ExitCode {
    crate::app::settings::set_logger_for_child();
    crate::core::localization::init();
    // Rustls otherwise has to choose between two crypto providers compiled
    // in (`aws-lc-rs`, which it and axum-server already use, and `ring`,
    // which `rcgen` alone would pull in without its own feature change
    // above) the first time it needs one — installed explicitly, once, so a
    // future dependency enabling `rustls/ring` cannot make that choice
    // ambiguous and panic instead. Only ever called once, so a `Result` it
    // is safe to ignore, not a genuine "already installed" error to report.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let config = StellarshotConfig::config();
    let Some(addr) = bind_address(config.web.scope, config.web.port) else {
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
        let tls = match crate::web_tls::config(config.web.custom_tls()).await {
            Ok(tls) => tls,
            Err(err) => {
                error_log!(WEB, "could not load a TLS certificate: {err}");
                return ExitCode::FAILURE;
            }
        };
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
            throttle: Throttle::new(),
        };
        let state = Arc::new(routes::AppState::new(
            config.profiles,
            config.global_exclude_patterns,
        ));
        serve(
            addr,
            tls,
            config.web.port,
            AppConfig {
                allowed_addresses: config.web.allowed_addresses,
                scope: config.web.scope,
                auth,
            },
            state,
        )
        .await
    })
}

/// Everything [`app`] needs to build the router, bundled so [`serve`] does
/// not need eight separate parameters for what is really one unit of
/// configuration.
struct AppConfig {
    allowed_addresses: Vec<String>,
    scope: NetworkScope,
    auth: AuthConfig,
}

/// Where to listen, or `None` if the network scope is off.
fn bind_address(scope: NetworkScope, port: u16) -> Option<SocketAddr> {
    match scope {
        NetworkScope::Off => None,
        NetworkScope::Localhost => Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)),
        NetworkScope::Lan => Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port)),
    }
}

/// The router: an allow-list gate, then authentication, in front of every
/// route. Separate from [`serve`] so a test can mount it on a listener of
/// its own, on an ephemeral port, without going through [`main`]'s real
/// config and chosen port.
///
/// Layers added with [`Router::layer`] run outermost-first — see axum's own
/// "Ordering" documentation for [`middleware`] — so the allow-list, added
/// last, is what a request meets first, before authentication is even
/// considered.
fn app(config: AppConfig, origins: HashSet<url::Origin>, state: Arc<routes::AppState>) -> Router {
    let AppConfig {
        allowed_addresses,
        scope,
        auth,
    } = config;
    let allowed = Arc::new(allowed_addresses);
    let scope = Arc::new(scope);
    let auth = Arc::new(auth);
    let origins = Arc::new(origins);
    Router::new()
        .route("/api/v1/health", get(health))
        .merge(routes::router(state))
        .layer(middleware::from_fn_with_state(auth, authenticate))
        .layer(middleware::from_fn_with_state(origins, reject_cross_site))
        .layer(middleware::from_fn_with_state((allowed, scope), allow_list))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            crate::constants::WEB_REQUEST_TIMEOUT,
        ))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            crate::constants::WEB_REQUEST_BODY_LIMIT,
        ))
        // Fixed headers every response carries, regardless of route or
        // outcome — none of them depend on the request, so these cover any
        // route WEB-12 adds later too, without it having to remember them.
        // HSTS is deliberately not sent: with the self-signed certificate
        // this ships by default, it would teach browsers to demand HTTPS
        // for this host even after the daemon is turned back off, which is
        // worse than the warning it replaces.
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        ))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("cross-origin-resource-policy"),
            HeaderValue::from_static("same-origin"),
        ))
}

async fn serve(
    addr: SocketAddr,
    tls: axum_server::tls_rustls::RustlsConfig,
    port: u16,
    config: AppConfig,
    state: Arc<routes::AppState>,
) -> ExitCode {
    // Bound explicitly (rather than letting `axum_server` bind lazily on
    // first poll) so a port already in use or otherwise unavailable is
    // reported here, with the address that failed, instead of surfacing from
    // wherever the server future first happens to be polled.
    let listener = match std::net::TcpListener::bind(addr) {
        Ok(listener) => listener,
        Err(err) => {
            error_log!(WEB, "could not listen on {addr}: {err}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = listener.set_nonblocking(true) {
        error_log!(WEB, "could not configure {addr}: {err}");
        return ExitCode::FAILURE;
    }
    debug_log!(WEB, "listening on {addr}");
    let allow_at_accept = AllowListAcceptor {
        allowed: Arc::new(config.allowed_addresses.clone()),
        scope: config.scope,
        connections: Arc::new(tokio::sync::Semaphore::new(
            crate::constants::WEB_MAX_CONNECTIONS,
        )),
    };
    let rustls_acceptor =
        axum_server::tls_rustls::RustlsAcceptor::new(tls).acceptor(allow_at_accept);
    let mut server = match axum_server::from_tcp(listener) {
        Ok(server) => server.acceptor(rustls_acceptor),
        Err(err) => {
            error_log!(WEB, "could not start TLS on {addr}: {err}");
            return ExitCode::FAILURE;
        }
    };
    // `axum_server` builds hyper's own connection handling with no timer at
    // all, so hyper's usual default header-read timeout is silently
    // dropped rather than applied: a client that opens a connection and
    // sends nothing would otherwise tie up a file descriptor forever, and
    // enough of those exhaust the service (see WEB-2 in the review plan).
    server
        .http_builder()
        .http1()
        .timer(hyper_util::rt::TokioTimer::new())
        .header_read_timeout(crate::constants::WEB_HEADER_READ_TIMEOUT);
    server
        .http_builder()
        .http2()
        .timer(hyper_util::rt::TokioTimer::new())
        .keep_alive_interval(Some(crate::constants::WEB_HTTP2_KEEPALIVE_INTERVAL))
        .keep_alive_timeout(crate::constants::WEB_HTTP2_KEEPALIVE_TIMEOUT)
        .max_concurrent_streams(crate::constants::WEB_HTTP2_MAX_CONCURRENT_STREAMS);
    let origins = allowed_origins(config.scope, port);
    let handle = axum_server::Handle::new();
    let router = app(config, origins, Arc::clone(&state))
        .into_make_service_with_connect_info::<SocketAddr>();
    let serving = server.handle(handle.clone()).serve(router);
    let shutdown = wait_then_drain(handle, state);
    let (result, reason) = tokio::join!(serving, shutdown);
    if let Err(err) = result {
        error_log!(WEB, "server stopped: {err}");
        return ExitCode::FAILURE;
    }
    if reason == ShutdownReason::Upgraded {
        // Not a failure — `Restart=on-failure` just needs a non-zero exit
        // to actually restart into the binary that replaced this one.
        return ExitCode::from(75);
    }
    ExitCode::SUCCESS
}

/// Waits for SIGTERM (sent by `systemctl stop`/`restart`, including the
/// WEB-1 restart-on-settings-change), then stops accepting new connections
/// and gives already-running work [`crate::constants::WEB_GRACEFUL_SHUTDOWN_TIMEOUT`]
/// to finish, rather than the previous behavior — nothing caught the
/// signal at all, so the default action killed the process (and whatever
/// backup it had started) with no chance to record anything.
/// Why [`wait_then_drain`] returned: whether the process should exit as if
/// nothing went wrong (a normal stop) or with a distinct, non-zero code so
/// `Restart=on-failure` actually restarts it into the binary that replaced
/// this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShutdownReason {
    Signal,
    Upgraded,
}

/// Waits for either trigger, then drains in-flight work the same way for
/// both: SIGTERM (sent by `systemctl stop`/`restart`, including the WEB-1
/// restart-on-settings-change) had no handler at all before this — nothing
/// caught it, so the default action killed the process, and whatever
/// backup it had started, with no chance to record anything. A package
/// upgrade unlinking this process's own binary is the same shape of
/// problem with no signal to catch at all, so it is polled for instead
/// ([`crate::exe::was_replaced`]).
async fn wait_then_drain(
    handle: axum_server::Handle<SocketAddr>,
    state: Arc<routes::AppState>,
) -> ShutdownReason {
    let reason = wait_for_shutdown_trigger().await;
    debug_log!(WEB, "shutting down ({reason:?}); draining in-flight work");
    handle.graceful_shutdown(Some(crate::constants::WEB_GRACEFUL_SHUTDOWN_TIMEOUT));
    routes::drain_running_jobs(&state, crate::constants::WEB_GRACEFUL_SHUTDOWN_TIMEOUT).await;
    reason
}

async fn wait_for_shutdown_trigger() -> ShutdownReason {
    let sigterm = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                error_log!(WEB, "could not install a SIGTERM handler: {err}");
                // No way to be told to stop by signal now; still worth
                // running so an upgrade is still noticed, but this arm
                // must never win a `select!` against one that can.
                std::future::pending::<()>().await;
            }
        }
    };
    let upgraded = async {
        let mut interval = tokio::time::interval(crate::constants::WEB_UPGRADE_POLL_INTERVAL);
        loop {
            interval.tick().await;
            if crate::exe::was_replaced() {
                return;
            }
        }
    };
    tokio::select! {
        () = sigterm => ShutdownReason::Signal,
        () = upgraded => ShutdownReason::Upgraded,
    }
}

/// Enforces the allow-list, and a cap on how many connections may be open
/// at once, before the TLS handshake even starts. `allow_list` (the
/// middleware layer, kept as a second check) cannot run until after a full
/// request has already been read on an established connection — reachable
/// on the network at all is enough to open one and hold it open otherwise,
/// which is what let about a thousand such connections exhaust the
/// service's file descriptors (see WEB-2 in the review plan).
#[derive(Clone)]
struct AllowListAcceptor {
    allowed: Arc<Vec<String>>,
    scope: NetworkScope,
    connections: Arc<tokio::sync::Semaphore>,
}

impl AllowListAcceptor {
    /// The actual decision, separated from [`Accept::accept`] itself so it
    /// can be tested directly against a plain [`IpAddr`], without a real
    /// socket: `Err` if `addr` is not on the allow-list, or every
    /// connection permit is already taken. The permit `Ok` carries is
    /// reserved as part of making the decision, under the same lock the
    /// semaphore itself already serializes on — not a separate
    /// check-then-reserve that concurrent accepts could both pass.
    fn decide(&self, addr: IpAddr) -> std::io::Result<tokio::sync::OwnedSemaphorePermit> {
        if !is_allowed(&self.allowed, self.scope, addr) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "not on the allow-list",
            ));
        }
        Arc::clone(&self.connections)
            .try_acquire_owned()
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::WouldBlock, "too many open connections")
            })
    }
}

impl<S: Send + 'static> axum_server::accept::Accept<tokio::net::TcpStream, S>
    for AllowListAcceptor
{
    type Stream = LimitedStream<tokio::net::TcpStream>;
    type Service = S;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = std::io::Result<(Self::Stream, S)>> + Send>,
    >;

    fn accept(&self, stream: tokio::net::TcpStream, service: S) -> Self::Future {
        let acceptor = self.clone();
        Box::pin(async move {
            let addr = stream.peer_addr()?;
            let permit = acceptor.decide(addr.ip()).inspect_err(|err| {
                debug_log!(WEB, "rejected {addr} at accept: {err}");
            })?;
            Ok((
                LimitedStream {
                    inner: stream,
                    _permit: permit,
                },
                service,
            ))
        })
    }
}

pin_project_lite::pin_project! {
    /// `T`, holding `_permit` for as long as the connection itself stays
    /// open: released back to [`AllowListAcceptor`]'s cap when this drops,
    /// not when it was merely accepted, so a slow or abandoned connection
    /// still counts against the cap for as long as it is actually open.
    struct LimitedStream<T> {
        #[pin]
        inner: T,
        _permit: tokio::sync::OwnedSemaphorePermit,
    }
}

impl<T: tokio::io::AsyncRead> tokio::io::AsyncRead for LimitedStream<T> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.project().inner.poll_read(cx, buf)
    }
}

impl<T: tokio::io::AsyncWrite> tokio::io::AsyncWrite for LimitedStream<T> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.project().inner.poll_write(cx, buf)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.project().inner.poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.project().inner.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
}

/// Reject anything not on the allow-list before it reaches any real route.
/// An empty list means every address the network scope's own private
/// ranges allow (see [`is_allowed`]) — never literally everyone, even
/// though `Lan` scope binds every interface (a VPN, a Docker bridge, a
/// public Wi-Fi network, a VPS's own public address).
async fn allow_list(
    State((allowed, scope)): State<(Arc<Vec<String>>, Arc<NetworkScope>)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    if is_allowed(&allowed, *scope, addr.ip()) {
        next.run(request).await
    } else {
        debug_log!(WEB, "rejected {addr}: not on the allow-list");
        StatusCode::FORBIDDEN.into_response()
    }
}

/// Whether `addr` may reach the daemon at all, before authentication is
/// even considered. An explicit entry always wins. An **empty** list falls
/// back to `scope`'s own private ranges (`WEB_PRIVATE_RANGES`) in `Lan`
/// scope — not everyone, which "reachable on the network" could otherwise
/// be read to mean — and to loopback only in `Localhost` scope, matching
/// what that scope already binds to.
fn is_allowed(allowed: &[String], scope: NetworkScope, addr: IpAddr) -> bool {
    if !allowed.is_empty() {
        return allowed.iter().any(|entry| matches(entry, addr));
    }
    match scope {
        NetworkScope::Off => false,
        NetworkScope::Localhost => addr.to_canonical().is_loopback(),
        NetworkScope::Lan => crate::constants::WEB_PRIVATE_RANGES
            .iter()
            .any(|range| matches(range, addr)),
    }
}

/// `entry` is either a single address or a CIDR range; either matches `addr`.
/// `addr`, canonicalized first (an IPv4-mapped IPv6 address, `::ffff:a.b.c.d`,
/// becomes plain `a.b.c.d`) so it can never slip past an IPv4-only `entry`
/// on a future dual-stack bind, matches `entry` — a single address or a
/// CIDR range.
fn matches(entry: &str, addr: IpAddr) -> bool {
    let addr = addr.to_canonical();
    if let Ok(net) = entry.parse::<ipnet::IpNet>() {
        return net.contains(&addr);
    }
    entry.parse::<IpAddr>() == Ok(addr)
}

/// Require whichever of the enabled methods actually applies: a request is
/// let through if it satisfies *any* enabled method. If none is enabled,
/// every request is rejected — a daemon someone deliberately configured with
/// no way in should fail closed, not silently become an open one.
///
/// A failed attempt only counts against the throttle when the request
/// actually presented `Basic` or `Bearer` credentials ([`presented_credentials`]):
/// a page a browser visits can fire credential-less requests at this daemon
/// (most are already refused cross-site by [`reject_cross_site`], but a
/// same-origin one, or one arriving through a proxy that strips `Origin`,
/// is not), and those must never be able to lock the owner out. The attempt
/// is reserved under [`Throttle::try_begin`] *before* verification runs, so
/// concurrent requests cannot all slip in between a check and its own
/// increment.
async fn authenticate(
    State(auth): State<Arc<AuthConfig>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let ip = addr.ip();
    let reservation = if presented_credentials(request.headers()) {
        match auth.throttle.try_begin(ip) {
            Ok(reservation) => Some(reservation),
            Err(retry_after) => {
                debug_log!(WEB, "rejected {addr}: locked out for {retry_after}s");
                let mut response = StatusCode::TOO_MANY_REQUESTS.into_response();
                if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
                    response.headers_mut().insert(header::RETRY_AFTER, value);
                }
                return response;
            }
        }
    } else {
        None
    };
    if is_authenticated(&auth, request.headers()) {
        if let Some(reservation) = reservation {
            reservation.succeeded();
        }
        return next.run(request).await;
    }
    if reservation.is_some() {
        auth.throttle.note_global_failure();
    }
    let mut response = StatusCode::UNAUTHORIZED.into_response();
    // Bearer, not Basic: this is a REST API, and Basic's challenge is what
    // makes a browser pop its own login prompt for it.
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        HeaderValue::from_static(r#"Bearer realm="stellarshot""#),
    );
    response
}

/// How many failed attempts an address gets before it is locked out, and for
/// how long: 5, 15 and 60 minutes, then capped at 24 hours, one step further
/// each time the address returns and fails again after its previous lockout
/// (or accumulation window) has fully passed. A first-time mistake is cheap;
/// a repeat offender's guesses get expensive fast.
const MAX_ATTEMPTS: u32 = 5;
const LOCKOUT_LEVEL_SECS: [i64; 4] = [5 * 60, 15 * 60, 60 * 60, 24 * 60 * 60];

fn lockout_duration(level: u32) -> i64 {
    LOCKOUT_LEVEL_SECS[(level as usize).min(LOCKOUT_LEVEL_SECS.len() - 1)]
}

/// A burst of failures across many addresses at once is a campaign, not one
/// address's problem: past this many failures from anyone, in this window,
/// password auth is paused for everyone (an API token, unaffected by guessing
/// a password, keeps working) until the window passes.
const GLOBAL_BUDGET_MAX: u32 = 50;
const GLOBAL_BUDGET_WINDOW_SECS: i64 = 10 * 60;

/// How long an address's escalation level is remembered after its most
/// recent failure, even once its own lockout has long since passed: long
/// enough that returning the next day still escalates, bounded so the map
/// backing it does not grow forever.
const ATTEMPTS_MEMORY_SECS: i64 = 7 * 86_400;

/// [`Throttle`]'s own map is capped at this many addresses; past it, the
/// least-recently-active one is dropped to make room for a new one, rather
/// than growing without bound.
const MAX_TRACKED_ADDRESSES: usize = 10_000;

fn now_secs() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// One address's recent failed attempts, and how many times it has already
/// been locked out.
#[derive(Debug, Clone, Copy)]
struct Attempts {
    count: u32,
    first_failure: i64,
    level: u32,
}

/// Whether `attempts` currently locks its address out, and if so, the whole
/// number of seconds left before it does not — never `0`, so a client is
/// never told to retry immediately and get the exact same answer again.
fn lockout_remaining(attempts: Attempts, now: i64) -> Option<i64> {
    if attempts.count < MAX_ATTEMPTS {
        return None;
    }
    let remaining = lockout_duration(attempts.level) - (now - attempts.first_failure);
    (remaining > 0).then_some(remaining.max(1))
}

/// `existing`, with one more failure counted in: within the same window
/// (accumulating toward a lockout, or already serving one), one higher; a
/// fresh window — no entry yet, or the previous one fully passed — starts
/// over at one, escalated one level further than last time (capped) if this
/// address has been here before.
fn next_attempts(existing: Option<Attempts>, now: i64) -> Attempts {
    match existing {
        Some(attempts) if now - attempts.first_failure < lockout_duration(attempts.level) => {
            Attempts {
                count: attempts.count + 1,
                ..attempts
            }
        }
        Some(attempts) => Attempts {
            count: 1,
            first_failure: now,
            level: attempts.level + 1,
        },
        None => Attempts {
            count: 1,
            first_failure: now,
            level: 0,
        },
    }
}

fn prune_expired(attempts: &mut HashMap<IpAddr, Attempts>, now: i64) {
    attempts.retain(|_, a| now - a.first_failure < ATTEMPTS_MEMORY_SECS);
}

/// One request's reserved attempt, from [`Throttle::try_begin`]: the failure
/// is already counted, so only a success needs to undo it.
struct Reservation<'a> {
    throttle: &'a Throttle,
    addr: IpAddr,
}

impl Reservation<'_> {
    fn succeeded(self) {
        self.throttle.record_success(self.addr);
    }
}

/// How many failures, across every address, in [`GLOBAL_BUDGET_WINDOW_SECS`].
#[derive(Debug, Clone, Copy)]
struct GlobalBudget {
    count: u32,
    window_start: i64,
}

/// Failed attempts against one daemon, by address, plus the budget shared
/// across all of them. A wrong guess is expensive only in how many of them
/// an address gets, not in making each one slower: after [`MAX_ATTEMPTS`]
/// within its current window, every further request from that address is
/// rejected — including one with the *correct* credentials — until the
/// window passes, rather than only the wrong ones. Only a request that
/// actually presented credentials reaches any of this; see [`authenticate`].
struct Throttle {
    attempts: Mutex<HashMap<IpAddr, Attempts>>,
    global: Mutex<GlobalBudget>,
}

impl Throttle {
    fn new() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            global: Mutex::new(GlobalBudget {
                count: 0,
                window_start: now_secs(),
            }),
        }
    }

    /// Reserves one attempt for `addr` before it is verified, under the same
    /// lock that checks whether it is already locked out — so concurrent
    /// requests cannot all read "not locked out yet" before any of them
    /// increments the count. `Err(seconds)` if `addr` was already locked out;
    /// that attempt is refused without being counted again. On success, call
    /// [`Reservation::succeeded`] to clear the address's history.
    fn try_begin(&self, addr: IpAddr) -> Result<Reservation<'_>, i64> {
        let mut attempts = self.attempts.lock().unwrap_or_else(PoisonError::into_inner);
        let now = now_secs();
        if let Some(remaining) = attempts
            .get(&addr)
            .copied()
            .and_then(|existing| lockout_remaining(existing, now))
        {
            return Err(remaining);
        }
        prune_expired(&mut attempts, now);
        if attempts.len() >= MAX_TRACKED_ADDRESSES
            && !attempts.contains_key(&addr)
            && let Some(oldest) = attempts
                .iter()
                .min_by_key(|(_, a)| a.first_failure)
                .map(|(addr, _)| *addr)
        {
            attempts.remove(&oldest);
        }
        let updated = next_attempts(attempts.get(&addr).copied(), now);
        if updated.count == MAX_ATTEMPTS {
            error_log!(
                WEB,
                "{addr} locked out for {}s after {MAX_ATTEMPTS} failed attempts (level {})",
                lockout_duration(updated.level),
                updated.level
            );
        }
        attempts.insert(addr, updated);
        Ok(Reservation {
            throttle: self,
            addr,
        })
    }

    /// A correct credential clears the address's history: once the right
    /// owner is back, they should not still be limited by earlier mistakes.
    fn record_success(&self, addr: IpAddr) {
        self.attempts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&addr);
    }

    /// One more failure, from any address, against the shared budget. Logs
    /// once, the moment the budget is exceeded — not on every request after.
    fn note_global_failure(&self) {
        let mut budget = self.global.lock().unwrap_or_else(PoisonError::into_inner);
        let now = now_secs();
        if now - budget.window_start >= GLOBAL_BUDGET_WINDOW_SECS {
            *budget = GlobalBudget {
                count: 0,
                window_start: now,
            };
        }
        budget.count += 1;
        if budget.count == GLOBAL_BUDGET_MAX + 1 {
            error_log!(
                WEB,
                "password authentication paused for {}m: more than {GLOBAL_BUDGET_MAX} failures across all addresses",
                GLOBAL_BUDGET_WINDOW_SECS / 60
            );
        }
    }

    /// Whether the global budget is currently exceeded, so password auth
    /// (not token auth) should be refused regardless of the password itself.
    fn password_paused(&self) -> bool {
        let budget = self.global.lock().unwrap_or_else(PoisonError::into_inner);
        now_secs() - budget.window_start < GLOBAL_BUDGET_WINDOW_SECS
            && budget.count > GLOBAL_BUDGET_MAX
    }
}

/// The `Authorization` header's scheme and the rest of its value, split
/// apart. RFC 9110 §11.1 requires the scheme to be matched case-insensitively
/// (every client actually seen so far already sends `Basic`/`Bearer` in their
/// conventional case, but nothing requires that).
fn split_scheme(header_value: &str) -> Option<(&str, &str)> {
    header_value.split_once(' ')
}

/// Whether `headers` carry an `Authorization` header this daemon recognizes
/// the scheme of, regardless of whether the credentials turn out correct.
/// [`authenticate`] only counts a failure against the throttle when this is
/// true, so a credential-less request can never lock anyone out.
fn presented_credentials(headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some((scheme, _)) = split_scheme(value) else {
        return false;
    };
    scheme.eq_ignore_ascii_case("basic") || scheme.eq_ignore_ascii_case("bearer")
}

fn is_authenticated(auth: &AuthConfig, headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    if auth.password_enabled
        && !auth.throttle.password_paused()
        && let Some(password) = &auth.password
        && let Some(candidate) = basic_password(value)
        && constant_time_eq(candidate.as_bytes(), password.expose().as_bytes())
    {
        return true;
    }
    if auth.token_enabled
        && let Some(hash) = &auth.token_hash
        && let Some((scheme, token)) = split_scheme(value)
        && scheme.eq_ignore_ascii_case("bearer")
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
    let (scheme, encoded) = split_scheme(header_value)?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (_username, password) = text.split_once(':')?;
    Some(password.to_owned())
}

/// Whether `a` and `b` are equal, without letting comparison time depend on
/// even their *lengths* matching: both are hashed first (fixed-length
/// output), the same way `web_token::verify` already avoids leaking a
/// token's length.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    fn hash(bytes: &[u8]) -> [u8; 32] {
        sha2::Sha256::digest(bytes).into()
    }
    bool::from(hash(a).ct_eq(&hash(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_off_scope_binds_nowhere() {
        assert_eq!(bind_address(NetworkScope::Off, 8737), None);
    }

    /// `AllowListAcceptor::decide` against a plain [`IpAddr`], not a real
    /// socket: this sandbox's loopback networking hangs when a test both
    /// listens and connects to itself at once (reproduced even with a
    /// bare-minimum server, nothing specific to this code — see WEB-3's
    /// status note in the review plan), so `decide` is deliberately
    /// separated from [`Accept::accept`] itself to stay testable without
    /// one. `Accept::accept`'s own extra step — `stream.peer_addr()`, then
    /// wrapping the result in `LimitedStream` — is a thin, untested
    /// couple of lines as a result; [`a_limited_streams_permit_is_released_exactly_once_dropped`]
    /// covers `LimitedStream` itself directly instead, the same way.
    #[test]
    fn a_disallowed_peer_is_rejected() {
        let acceptor = AllowListAcceptor {
            allowed: Arc::new(vec!["203.0.113.1".to_owned()]),
            scope: NetworkScope::Lan,
            connections: Arc::new(tokio::sync::Semaphore::new(64)),
        };
        assert!(
            acceptor.decide("127.0.0.1".parse().unwrap()).is_err(),
            "127.0.0.1 is not in the allow-list"
        );
    }

    #[test]
    fn an_allowed_peer_past_the_connection_cap_is_still_rejected() {
        let connections = Arc::new(tokio::sync::Semaphore::new(1));
        // The only permit is already held, as if one connection were
        // already open, before this one is even accepted.
        let held = Arc::clone(&connections).try_acquire_owned().unwrap();
        let acceptor = AllowListAcceptor {
            allowed: Arc::new(Vec::new()),
            scope: NetworkScope::Lan,
            connections,
        };

        assert!(
            acceptor.decide("127.0.0.1".parse().unwrap()).is_err(),
            "the one permit was already taken"
        );
        drop(held);
    }

    #[test]
    fn an_allowed_peer_with_a_free_permit_reserves_it() {
        let connections = Arc::new(tokio::sync::Semaphore::new(1));
        let acceptor = AllowListAcceptor {
            allowed: Arc::new(Vec::new()),
            scope: NetworkScope::Lan,
            connections: Arc::clone(&connections),
        };

        let permit = acceptor.decide("127.0.0.1".parse().unwrap()).unwrap();

        assert_eq!(connections.available_permits(), 0);
        drop(permit);
    }

    #[tokio::test]
    async fn a_limited_streams_permit_is_released_exactly_once_dropped() {
        let connections = Arc::new(tokio::sync::Semaphore::new(1));
        let permit = Arc::clone(&connections).try_acquire_owned().unwrap();
        // An in-memory duplex pair stands in for a real connection: all
        // `LimitedStream` needs from it is that it is some `AsyncRead` +
        // `AsyncWrite`, which this is, without a real socket.
        let (inner, _other_end) = tokio::io::duplex(64);
        let limited = LimitedStream {
            inner,
            _permit: permit,
        };
        assert_eq!(
            connections.available_permits(),
            0,
            "the permit should be held while the connection is open"
        );

        drop(limited);

        assert_eq!(
            connections.available_permits(),
            1,
            "and released once the connection itself closes"
        );
    }

    #[test]
    fn the_localhost_scope_binds_only_loopback() {
        let addr = bind_address(NetworkScope::Localhost, 8737).unwrap();
        assert!(addr.ip().is_loopback());
        assert_eq!(addr.port(), 8737);
    }

    #[test]
    fn the_lan_scope_binds_every_interface() {
        let addr = bind_address(NetworkScope::Lan, 8737).unwrap();
        assert_eq!(addr.ip(), IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    }

    #[test]
    fn a_chosen_port_is_used_for_either_scope() {
        assert_eq!(
            bind_address(NetworkScope::Localhost, 9000).unwrap().port(),
            9000
        );
        assert_eq!(bind_address(NetworkScope::Lan, 9000).unwrap().port(), 9000);
    }

    #[test]
    fn an_empty_allow_list_falls_back_to_the_scopes_own_private_ranges() {
        let public = "203.0.113.7".parse().unwrap();
        let private = "192.168.1.5".parse().unwrap();
        let loopback = "127.0.0.1".parse().unwrap();
        assert!(!is_allowed(&[], NetworkScope::Off, public));
        assert!(!is_allowed(&[], NetworkScope::Off, loopback));
        assert!(is_allowed(&[], NetworkScope::Localhost, loopback));
        assert!(!is_allowed(&[], NetworkScope::Localhost, private));
        assert!(is_allowed(&[], NetworkScope::Lan, private));
        assert!(is_allowed(&[], NetworkScope::Lan, loopback));
        assert!(!is_allowed(&[], NetworkScope::Lan, public));
    }

    #[test]
    fn a_single_address_only_allows_itself_in_every_scope() {
        let allowed = vec!["192.168.1.10".to_owned()];
        for scope in [
            NetworkScope::Off,
            NetworkScope::Localhost,
            NetworkScope::Lan,
        ] {
            assert!(is_allowed(&allowed, scope, "192.168.1.10".parse().unwrap()));
            assert!(!is_allowed(
                &allowed,
                scope,
                "192.168.1.11".parse().unwrap()
            ));
        }
    }

    #[test]
    fn a_cidr_range_allows_every_address_inside_it() {
        let allowed = vec!["192.168.1.0/24".to_owned()];
        assert!(is_allowed(
            &allowed,
            NetworkScope::Lan,
            "192.168.1.1".parse().unwrap()
        ));
        assert!(is_allowed(
            &allowed,
            NetworkScope::Lan,
            "192.168.1.254".parse().unwrap()
        ));
        assert!(!is_allowed(
            &allowed,
            NetworkScope::Lan,
            "192.168.2.1".parse().unwrap()
        ));
    }

    #[test]
    fn an_unparseable_entry_matches_nothing_rather_than_panicking() {
        let allowed = vec!["not an address".to_owned()];
        assert!(!is_allowed(
            &allowed,
            NetworkScope::Lan,
            "192.168.1.1".parse().unwrap()
        ));
    }

    fn no_auth() -> AuthConfig {
        AuthConfig {
            password_enabled: false,
            password: None,
            token_enabled: false,
            token_hash: None,
            throttle: Throttle::new(),
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

    #[test]
    fn fewer_than_the_maximum_failures_never_locks_out() {
        let attempts = Attempts {
            count: MAX_ATTEMPTS - 1,
            first_failure: 1000,
            level: 0,
        };
        assert_eq!(lockout_remaining(attempts, 1000), None);
    }

    #[test]
    fn the_maximum_failures_locks_out_until_the_window_passes() {
        let attempts = Attempts {
            count: MAX_ATTEMPTS,
            first_failure: 1000,
            level: 0,
        };
        let lockout = LOCKOUT_LEVEL_SECS[0];
        assert_eq!(lockout_remaining(attempts, 1000), Some(lockout));
        assert_eq!(
            lockout_remaining(attempts, 1000 + lockout - 1),
            Some(1),
            "one second left is still locked out"
        );
        assert_eq!(
            lockout_remaining(attempts, 1000 + lockout),
            None,
            "the window has fully passed"
        );
        assert_eq!(lockout_remaining(attempts, 1000 + lockout + 100), None);
    }

    #[test]
    fn failures_accumulate_within_a_window_and_escalate_once_it_passes() {
        let first = next_attempts(None, 1000);
        assert_eq!(first.count, 1);
        assert_eq!(first.level, 0);

        let second = next_attempts(Some(first), 1001);
        assert_eq!(second.count, 2, "still within the same window");
        assert_eq!(
            second.first_failure, first.first_failure,
            "the window's start does not move"
        );
        assert_eq!(second.level, 0, "the level does not move either");

        let after_window = next_attempts(Some(second), 1000 + LOCKOUT_LEVEL_SECS[0]);
        assert_eq!(after_window.count, 1, "a fresh window starts over");
        assert_eq!(after_window.first_failure, 1000 + LOCKOUT_LEVEL_SECS[0]);
        assert_eq!(
            after_window.level, 1,
            "returning after the window passed escalates the level"
        );
    }

    #[test]
    fn the_escalation_level_is_capped_at_the_longest_lockout() {
        let mut attempts = next_attempts(None, 0);
        let mut now = 0;
        for _ in 0..LOCKOUT_LEVEL_SECS.len() + 5 {
            now += lockout_duration(attempts.level);
            attempts = next_attempts(Some(attempts), now);
        }
        assert_eq!(
            lockout_duration(attempts.level),
            *LOCKOUT_LEVEL_SECS.last().unwrap()
        );
    }

    #[test]
    fn a_same_origin_or_absent_sec_fetch_site_is_never_cross_site() {
        let origins = HashSet::new();
        for site in ["same-origin", "none"] {
            let mut headers = HeaderMap::new();
            headers.insert("sec-fetch-site", site.parse().unwrap());
            assert_eq!(cross_site_reason(&origins, &headers), None);
        }
    }

    #[test]
    fn a_cross_site_sec_fetch_site_is_rejected_even_with_no_origin_header() {
        let origins = HashSet::new();
        let mut headers = HeaderMap::new();
        headers.insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert!(cross_site_reason(&origins, &headers).is_some());
    }

    #[test]
    fn no_sec_fetch_site_or_origin_header_at_all_is_let_through() {
        assert_eq!(cross_site_reason(&HashSet::new(), &HeaderMap::new()), None);
    }

    #[test]
    fn an_origin_of_null_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, "null".parse().unwrap());
        assert!(cross_site_reason(&HashSet::new(), &headers).is_some());
    }

    #[test]
    fn an_origin_matching_the_allowed_set_is_let_through() {
        let origin: url::Origin = url::Url::parse("https://127.0.0.1:8737").unwrap().origin();
        let mut origins = HashSet::new();
        origins.insert(origin);
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, "https://127.0.0.1:8737".parse().unwrap());
        assert_eq!(cross_site_reason(&origins, &headers), None);
    }

    #[test]
    fn an_origin_not_in_the_allowed_set_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, "https://evil.example".parse().unwrap());
        assert!(cross_site_reason(&HashSet::new(), &headers).is_some());
    }

    #[test]
    fn a_request_with_no_authorization_header_presents_no_credentials() {
        assert!(!presented_credentials(&HeaderMap::new()));
    }

    #[test]
    fn a_basic_or_bearer_scheme_presents_credentials_regardless_of_case() {
        for scheme in ["Basic", "basic", "BASIC", "Bearer", "bearer", "BEARER"] {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::AUTHORIZATION,
                format!("{scheme} whatever").parse().unwrap(),
            );
            assert!(presented_credentials(&headers), "scheme {scheme}");
        }
    }

    #[test]
    fn an_unrecognized_scheme_presents_no_credentials() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Digest whatever".parse().unwrap());
        assert!(!presented_credentials(&headers));
    }

    #[tokio::test]
    async fn credential_less_requests_are_never_throttled_no_matter_how_many() {
        let addr = spawn(Vec::new(), password_auth("secret")).await;
        for attempt in 1..=10 {
            let response = get(addr, "/api/v1/health", None).await;
            assert!(
                response.starts_with("HTTP/1.1 401"),
                "attempt {attempt}: expected 401, got: {response}"
            );
        }
        // Ten credential-less requests did not count against the throttle,
        // so the correct password still works right after them.
        let response = get(addr, "/api/v1/health", Some("Basic aWdub3JlZDpzZWNyZXQ=")).await;
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "expected 200, got: {response}"
        );
    }

    #[tokio::test]
    async fn a_cross_site_request_is_forbidden_and_does_not_count_against_the_throttle() {
        let addr = spawn(Vec::new(), password_auth("secret")).await;
        let wrong = Some("Basic aWdub3JlZDp3cm9uZw==");
        for attempt in 1..=10 {
            let response = get_with_headers(
                addr,
                "/api/v1/health",
                wrong,
                "Sec-Fetch-Site: cross-site\r\n",
            )
            .await;
            assert!(
                response.starts_with("HTTP/1.1 403"),
                "attempt {attempt}: expected 403, got: {response}"
            );
        }
        let response = get(addr, "/api/v1/health", Some("Basic aWdub3JlZDpzZWNyZXQ=")).await;
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "cross-site rejections must not have been counted: {response}"
        );
    }

    /// The race [`Throttle::try_begin`] closes: 64 real OS threads (not
    /// tokio tasks — genuine parallelism, not cooperative interleaving on
    /// however many the runtime happens to schedule) hammering one address
    /// at once. A check-then-increment done under separate locks could let
    /// every one of them read "not locked out yet" before any commits; the
    /// single locked reserve-and-check here must not.
    #[test]
    fn concurrent_attempts_give_at_most_max_attempts_worth_of_reservations() {
        let throttle = Throttle::new();
        let addr: IpAddr = "203.0.113.9".parse().unwrap();
        let (reserved, refused) = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..64)
                .map(|_| scope.spawn(|| throttle.try_begin(addr).is_ok()))
                .collect();
            let results: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            let reserved = results.iter().filter(|ok| **ok).count();
            (reserved, results.len() - reserved)
        });
        assert!(
            reserved <= MAX_ATTEMPTS as usize,
            "at most {MAX_ATTEMPTS} concurrent attempts should be reserved, got {reserved}"
        );
        assert_eq!(reserved + refused, 64);
    }

    #[tokio::test]
    async fn repeated_wrong_passwords_from_one_address_eventually_lock_it_out() {
        let addr = spawn(Vec::new(), password_auth("secret")).await;
        let wrong = Some("Basic aWdub3JlZDp3cm9uZw==");

        for attempt in 1..=MAX_ATTEMPTS {
            let response = get(addr, "/api/v1/health", wrong).await;
            assert!(
                response.starts_with("HTTP/1.1 401"),
                "attempt {attempt}: expected 401, got: {response}"
            );
        }

        // The address is now locked out: even the *correct* password no
        // longer works, proving this blocks the address, not only wrong
        // guesses — otherwise an attacker's next guess would simply be let
        // through the moment it happened to be right.
        let response = get(addr, "/api/v1/health", Some("Basic aWdub3JlZDpzZWNyZXQ=")).await;
        assert!(
            response.starts_with("HTTP/1.1 429"),
            "expected 429 once locked out, got: {response}"
        );
        assert!(
            response.to_lowercase().contains("retry-after"),
            "a locked-out response should say how long to wait: {response}"
        );
    }

    #[tokio::test]
    async fn an_address_that_never_fails_is_never_throttled() {
        let addr = spawn(Vec::new(), password_auth("secret")).await;
        for _ in 0..MAX_ATTEMPTS + 5 {
            let response = get(addr, "/api/v1/health", Some("Basic aWdub3JlZDpzZWNyZXQ=")).await;
            assert!(
                response.starts_with("HTTP/1.1 200"),
                "a correct password should never be throttled: {response}"
            );
        }
    }

    #[tokio::test]
    async fn a_correct_password_after_a_few_wrong_ones_clears_the_count() {
        let addr = spawn(Vec::new(), password_auth("secret")).await;
        let wrong = Some("Basic aWdub3JlZDp3cm9uZw==");
        let right = Some("Basic aWdub3JlZDpzZWNyZXQ=");

        // Fewer than the threshold, then a real success: this must not
        // leave a partial count around to add to next time.
        for _ in 0..MAX_ATTEMPTS - 1 {
            get(addr, "/api/v1/health", wrong).await;
        }
        let response = get(addr, "/api/v1/health", right).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");

        for attempt in 1..=MAX_ATTEMPTS - 1 {
            let response = get(addr, "/api/v1/health", wrong).await;
            assert!(
                response.starts_with("HTTP/1.1 401"),
                "attempt {attempt} after the reset: expected 401 (not yet locked out), got: {response}"
            );
        }
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
                app(
                    AppConfig {
                        allowed_addresses,
                        scope: NetworkScope::Lan,
                        auth,
                    },
                    HashSet::new(),
                    Arc::new(routes::AppState::new(Vec::new(), Vec::new())),
                )
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
        request(addr, "GET", path, authorization, "").await
    }

    async fn post(addr: SocketAddr, path: &str, authorization: Option<&str>) -> String {
        request(addr, "POST", path, authorization, "").await
    }

    /// Like [`get`], with extra raw header lines (each already ending in
    /// `\r\n`) — for headers no other helper here sends, like `Origin` or
    /// `Sec-Fetch-Site`.
    async fn get_with_headers(
        addr: SocketAddr,
        path: &str,
        authorization: Option<&str>,
        extra_headers: &str,
    ) -> String {
        request(addr, "GET", path, authorization, extra_headers).await
    }

    async fn request(
        addr: SocketAddr,
        method: &str,
        path: &str,
        authorization: Option<&str>,
        extra_headers: &str,
    ) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let auth_header = authorization
            .map(|value| format!("Authorization: {value}\r\n"))
            .unwrap_or_default();
        stream
            .write_all(
                format!(
                    "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n{auth_header}{extra_headers}Connection: close\r\n\r\n"
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

    /// The routes in `routes::router` (everything but `/api/v1/health`) have
    /// no allow-list or authentication of their own — that is this outer
    /// `app`'s job. This is what actually proves the merge in [`app`] puts
    /// *every* route, not only the ones tested directly above, behind both
    /// layers: a route this test does not know the shape of tomorrow is
    /// still covered, since it is the composition being tested, not one
    /// route's own wiring.
    #[tokio::test]
    async fn an_unauthenticated_write_route_is_rejected_before_it_is_even_looked_up() {
        let addr = spawn(Vec::new(), no_auth()).await;

        // No profile with this ID exists either (`Vec::new()`), so a `401`
        // here (rather than a `404`) is proof the auth layer runs first, in
        // front of the route handler, not that the route happens to work.
        let response = post(addr, "/api/v1/backups/anything/run", None).await;

        assert!(
            response.starts_with("HTTP/1.1 401"),
            "expected 401, got: {response}"
        );
    }

    #[tokio::test]
    async fn a_write_route_from_an_address_not_on_the_allow_list_is_forbidden() {
        let addr = spawn(vec!["203.0.113.1".to_owned()], password_auth("secret")).await;

        let response = post(
            addr,
            "/api/v1/backups/anything/run",
            Some("Basic aWdub3JlZDpzZWNyZXQ="),
        )
        .await;

        assert!(
            response.starts_with("HTTP/1.1 403"),
            "expected 403, got: {response}"
        );
    }

    /// The fixed security headers are added by the *outermost* layer in
    /// [`app`], so every response carries them regardless of which inner
    /// layer actually decided the status — proven here across a `200`, a
    /// `401` and a `403`, three different layers' decisions, rather than
    /// trusting that "outermost" placement once and never checking it.
    #[tokio::test]
    async fn every_response_carries_the_fixed_security_headers_regardless_of_status() {
        let addr = spawn(vec!["203.0.113.1".to_owned()], password_auth("secret")).await;
        let ok = spawn(Vec::new(), password_auth("secret")).await;

        let cases = [
            get(ok, "/api/v1/health", Some("Basic aWdub3JlZDpzZWNyZXQ=")).await,
            get(ok, "/api/v1/health", None).await,
            get(addr, "/api/v1/health", Some("Basic aWdub3JlZDpzZWNyZXQ=")).await,
        ];
        for response in cases {
            let lower = response.to_lowercase();
            for expected in [
                "x-content-type-options: nosniff",
                "cache-control: no-store",
                "content-security-policy:",
                "referrer-policy: no-referrer",
                "cross-origin-resource-policy: same-origin",
            ] {
                assert!(
                    lower.contains(expected),
                    "missing {expected:?} in: {response}"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_401_carries_a_bearer_www_authenticate_challenge() {
        let addr = spawn(Vec::new(), password_auth("secret")).await;
        let response = get(addr, "/api/v1/health", None).await;
        assert!(
            response.starts_with("HTTP/1.1 401"),
            "expected 401, got: {response}"
        );
        assert!(
            response
                .to_lowercase()
                .contains(r#"www-authenticate: bearer realm="stellarshot""#),
            "expected a Bearer challenge, got: {response}"
        );
    }

    /// Proves TLS is really terminated by [`serve`]'s own server, not merely
    /// buildable in isolation (already proven in `web_tls`'s own tests): a
    /// real `curl` handshake, over a real socket, through the exact function
    /// [`main`] calls. A clean `401` (rather than `curl` failing the
    /// handshake, or a garbled response as if talking plain HTTP to a TLS
    /// port) is only possible if the certificate really was presented and
    /// accepted.
    #[tokio::test]
    async fn a_real_curl_request_over_tls_reaches_the_health_route() {
        let dir = tempfile::TempDir::new().unwrap();
        let (cert, key) = crate::web_tls::self_signed_paths(dir.path()).unwrap();
        let tls = crate::web_tls::config(Some((&cert, &key))).await.unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = axum_server::tls_rustls::from_tcp_rustls(listener, tls).unwrap();
        tokio::spawn(
            server.serve(
                app(
                    AppConfig {
                        allowed_addresses: Vec::new(),
                        scope: NetworkScope::Localhost,
                        auth: no_auth(),
                    },
                    HashSet::new(),
                    Arc::new(routes::AppState::new(Vec::new(), Vec::new())),
                )
                .into_make_service_with_connect_info::<SocketAddr>(),
            ),
        );

        let output = tokio::process::Command::new("curl")
            .args([
                "--silent",
                "--insecure",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
            ])
            .arg(format!("https://{addr}/api/v1/health"))
            .output()
            .await
            .expect("curl must be installed");
        let code = String::from_utf8_lossy(&output.stdout).into_owned();
        assert_eq!(
            code, "401",
            "no auth method is enabled, so a real handshake must still end in a clean 401, \
             not curl failing the handshake or a garbled response"
        );
    }
}
