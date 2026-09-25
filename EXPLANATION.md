# Why a first Google Drive backup failed

Two screenshots on 2026-09-24 around 23:23 show a first backup to Google
Drive failing partway through, with this dialog:

> **Something went wrong**
> The snapshot could not be created.
>
> Details: `rustic_core` experienced an error related to `the backend`.
>
> Message:
> Backoff failed, please check the logs for more information.
>
> Caused by:
> request or response body error for url (http://127.0.0.1:38081/data/...)
> : (source: send failed because receiver is gone)

## What the message means

Stellarshot never talks to Google Drive directly. rustic starts `rclone
serve restic` as a small local server on `127.0.0.1` and speaks its own
protocol to that instead, with rclone doing the actual translation to
Google's API underneath. "Backoff failed" is rustic's own retry wrapper
giving up: it tries a request up to five times with a growing delay between
attempts, and every one of the five failed the same way here — "send failed
because receiver is gone", meaning the local rclone process closed the
connection while data was still being sent to it. Five identical failures in
a row point at something that was consistently broken for the whole
attempt, not a single dropped packet a retry would routinely paper over.

## What could not be confirmed, and why

The obvious next step was rclone's own log, since the dialog says to check
it. There was none. Stellarshot's logging setup (`app::settings::set_logger`)
built a `tracing` subscriber, but rustic_core, rustic_backend and the rclone
process they run only ever log through the separate `log` crate, and nothing
bridged the two — every line any of them ever logged went nowhere. The
dialog's own advice was not actually possible to follow. That gap is now
fixed (see CHANGELOG.md and VALIDATION.md), so a repeat of this failure will
leave something to check.

Without that log, the specific cause of this one failure cannot be stated as
fact. The most likely explanation, consistent with the evidence but not
confirmed by it: Google's API enforces a quota per OAuth client, and
Stellarshot's Google Drive sign-in uses rclone's own shared client ID unless
a user provides their own — the same one every other rclone user who has not
set up their own credentials shares. Stellarshot also uploads four packs at
once by default (added in 0.1.1, for speed). Four concurrent uploads on a
quota shared across everyone using the default client is a plausible way to
get rate-limited or have a connection reset by Google's side, which would
surface locally exactly as rclone's bridge dropping the connection.

## What this changes

- The logging gap is fixed, so a repeat of this failure will have rclone's
  own explanation to read, at `/tmp/stellarshot-backend.log`.
- **Use my own Google API credentials…**, in the wizard's Google Drive step
  since 0.2, gives a way to sign in with a client that is not shared with
  every other rclone user, which would rule the quota out entirely if that
  is in fact the cause.
- If the failure recurs even on a private client, the more conservative
  thing to try is lowering the default of four parallel uploads, tracked in
  ROADMAP.md's 0.4 section alongside Stellarshot's own bundled Google
  client — the planned way to stop depending on the shared one at all.
