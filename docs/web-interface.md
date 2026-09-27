# The web interface

Stellarshot can run a small daemon, `stellarshot-web`, that exposes a REST
API for looking at your backups and starting one, without sitting at the
machine. It is off by default.

## Turning it on

In Stellarshot's own Settings, under **Web interface**:

1. Choose a network scope:
   - **Off** — the daemon does not run at all. The default.
   - **This computer only** — reachable at `127.0.0.1`, for example through
     your own SSH tunnel. Nothing on the network can reach it directly.
   - **Reachable on the network** — reachable from any other device on the
     same network.
2. Choosing anything other than Off starts the daemon immediately; choosing
   Off stops and disables it immediately. Settings shows the exact address
   it will listen at once a scope is chosen.
3. Turn on at least one authentication method (below). With none enabled,
   every request is rejected — the daemon fails closed rather than becoming
   an open API by omission.

Changing the **port**, or the TLS certificate, needs an explicit **Restart**
(see [The daemon](#the-daemon) below) to take effect; changing the network
scope itself does not, since turning it on or off already starts or stops
the daemon.

## TLS

The web interface is always reached over `https://`, never plain HTTP.

By default it generates its own self-signed certificate the first time it
needs one, and keeps reusing it — it is not regenerated on every restart,
so a browser's one-time trust exception for it keeps working. It is stored
under `~/.local/share/stellarshot/web/` (`cert.pem`, `key.pem`; the key is
readable only by you).

If you have your own certificate — from a certificate authority your
devices already trust, or one you manage yourself — set both **Certificate**
and **Private key** in Settings to its files instead. Both need to be set
together: setting only one falls back to the self-signed certificate.

A self-signed certificate means your browser or `curl` will need to accept
it explicitly the first time:

```
curl -k https://127.0.0.1:8737/api/v1/health
```

(`-k`/`--insecure` skips certificate verification — reasonable for a
certificate you generated yourself and can verify by other means, such as
checking it was created on this machine; less reasonable for a certificate
you did not generate.)

## Authentication

Any request needs to satisfy at least one enabled method:

- **Shared password** — HTTP Basic authentication, at least 12 characters
  (Settings refuses a shorter one). The username is ignored; only the
  password matters, compared by hashing both sides first, so neither a wrong
  guess's timing nor its length gives anything away. Set it once in
  Settings; it is kept in your keyring, never in the config file itself.
- **API token** — a generated token sent as `Authorization: Bearer <token>`.
  Shown once, when generated; only its hash is kept afterward, so if you
  lose it, generate a new one rather than trying to recover the old one.
- **This computer's own sign-in** (PAM) — not implemented yet. The setting
  exists and is saved, but the daemon does not check it.

A request with no `Authorization` header, or one this daemon does not
recognize, gets `401 Unauthorized` (with a `WWW-Authenticate: Bearer` header)
but is never counted against the address below — only an actual wrong
password or token is. This matters because a same-origin page in your
browser can otherwise fire credential-less requests at this daemon on its
own; only [cross-site ones](#cross-site-requests) are refused outright.

Repeated *wrong* passwords or tokens from the same address are throttled and
escalated: 5 in a row locks that address out for 5 minutes, then 15, then 60,
then capped at 24 hours if it keeps coming back after each lockout passes —
including a subsequently correct password or token, not only further wrong
ones — and told how long to wait (`429 Too Many Requests`, with a
`Retry-After` header). A correct credential clears an address's count, so a
few mistyped attempts do not linger against you. Separately, more than 50
wrong passwords in 10 minutes from *any* addresses combined pauses password
authentication for everyone until the window passes (an API token keeps
working); both kinds of lockout are logged.

Behind a reverse proxy or an SSH tunnel, the allow-list and this throttle see
the proxy's own address, not the original client's — every tunneled client
looks the same to them. Stellarshot does not read `X-Forwarded-For` (a
proxy's own access control should sit in front of it instead, since that
header is easy to spoof from anywhere else).

## The IP allow-list

Addresses or CIDR ranges (for example `192.168.1.0/24`) allowed to reach the
web interface, on top of whatever the network scope itself already allows.
Empty means every private address the scope allows (see
[Turning it on](#turning-it-on)) — never literally everyone, even in the LAN
scope, which binds every network interface on this machine. Checked before
authentication: a request from an address not on the list never reaches far
enough to try a password or token at all.

## Cross-site requests

A request whose `Sec-Fetch-Site` header says it did not come from this same
origin — or, for a browser old enough not to send that header, whose
`Origin` does not match one of this daemon's own addresses — is refused with
`403 Forbidden`, before authentication is even attempted. This is what keeps
a page open in your browser, on some other site, from using your own
still-valid session against this API: there are no cookies and no CORS
headers here for it to ride on in the first place, but a same-origin
`fetch()` would otherwise still be free to try. A request with neither
header at all (`curl`, a script, the examples in this document) is let
through — that is what makes those examples work.

## The daemon

`stellarshot-web` runs as a per-user systemd service
(`stellarshot-web.service`), the same kind of unit Stellarshot already uses
for scheduled backups. Settings shows its status — **Running**, **Stopped**,
**Failed to start**, or **Not installed** — and three buttons:

- **Start** — installs the service if needed, and starts it.
- **Stop** — stops and disables it. Nothing is left running or configured to
  start again on its own.
- **Restart** — the button to press after changing the port or the TLS
  certificate, or to recover from **Failed to start**.

You can also control it directly:

```
systemctl --user status stellarshot-web.service
systemctl --user restart stellarshot-web.service
journalctl --user -u stellarshot-web.service
```

## The REST API

Every route is under `/api/v1/`, and every route — including `/api/v1/health`
— needs to pass the IP allow-list and authentication like anything else.

### `GET /api/v1/health`

Confirms the daemon is up and reachable.

```json
{ "ok": true }
```

### `GET /api/v1/backups`

Every configured backup's current status.

```json
[
  {
    "profile_id": "a1b2c3",
    "name": "Home",
    "running": false,
    "last_success": 1735689600,
    "failed": false,
    "overdue": false
  }
]
```

### `GET /api/v1/backups/{id}/snapshots`

That backup's snapshots, newest first.

```json
[
  {
    "id": "4f9e2a...",
    "time": 1735689600,
    "paths": ["/home/alex"],
    "hostname": "alex-desktop",
    "files_new": 12,
    "files_changed": 3,
    "files_unmodified": 4021,
    "data_added": 8388608
  }
]
```

### `GET /api/v1/backups/{id}/snapshots/{snapshot}/browse?path=/some/folder`

A folder's contents inside that snapshot. `snapshot` may be a full ID, a
unique prefix, or `latest`. `path` defaults to `/` (the snapshot's root).

```json
[
  {
    "name": "example.txt",
    "path": "/home/alex/example.txt",
    "kind": "file",
    "size": 1024,
    "modified": 1735689600
  }
]
```

`kind` is one of `file`, `directory`, `symlink`, `other`.

Opening a repository loads its whole index, so this route and the
snapshots one above it share a small cap on how many may have one open at
once; past it, a request gets `503` with `Retry-After` immediately rather
than queuing behind a slow remote.

### `POST /api/v1/backups/{id}/run`

Starts that backup now, the same thing "Back Up Now" does on the desktop.
Answers immediately, before the backup itself finishes — poll
`GET /api/v1/backups` to see when it completes. Does not create a new
backup, and does not restore one.

```json
{ "started": true }
```

The run appears on the History page, marked as started from the web.

### Errors

A failure comes back as JSON with an HTTP status matching what went wrong
(`401` for bad credentials, `400` for a malformed path, `404` for an unknown
backup ID, `409` for a backup already running or a repository password
problem, `503` for an unreachable destination, `500` otherwise) and a body
describing it:

```json
{
  "kind": "not-a-repository",
  "message": "no backup, snapshot or path matches this request",
  "request_id": "b6c1f6b0-2a9e-4b3a-9b7a-1e6b0a2f9c3d"
}
```

`message` is a stable, generic description — never the technical detail a
failure inside the engine actually carried (a password command's stderr,
rclone's own stderr, a local path), which stays out of every response and
goes only to this daemon's own log, tied to `request_id`.

`429 Too Many Requests` (with a `Retry-After` header, no JSON body) means
your address is locked out after repeated failures — see
[Authentication](#authentication).

## Troubleshooting

- **Nothing answers at the address Settings shows.** Check the daemon's
  status in Settings, or `systemctl --user status stellarshot-web.service`.
  A port already in use by something else, or an invalid custom TLS
  certificate path, both show up there.
- **A setting I just changed does not seem to apply.** The network scope
  applies immediately; the port, TLS certificate, and authentication
  settings need an explicit Restart.
- **My browser refuses to connect at all**, rather than showing a
  certificate warning. That usually means nothing is listening yet — check
  the daemon's status — rather than a certificate problem, which shows as a
  warning you can inspect and choose to bypass, not a refused connection.
- **I locked myself out with too many wrong attempts.** Wait out the 5
  minutes, or press Restart in Settings — the lockout is kept only in the
  daemon's own memory, so restarting it clears every address's count.
