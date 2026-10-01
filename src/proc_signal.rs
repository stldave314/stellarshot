// SPDX-License-Identifier: GPL-3.0-only

//! Running a backup's `After` hooks when the process is told to stop.
//!
//! `runner::AfterHookGuard` runs them on every path that *returns*: a
//! wrong password, an unreachable destination, a panic. It cannot run on a
//! signal — SIGKILL gives no notice at all, and an unhandled SIGTERM ends
//! the process without unwinding. Cancel in the window used to send SIGKILL
//! to the whole group, and `systemctl --user stop` (or logout, or shutdown)
//! sends SIGTERM to a `--scheduled` run that had no handler for it, so a
//! `Before` hook that stopped a database left it stopped, every time.
//!
//! This installs the one thing that can catch a SIGTERM and still run real
//! code: block the signal in the main thread before any other thread
//! exists (every thread started afterward inherits the mask, so the signal
//! can only ever be delivered to one place), and have one thread wait for
//! it with `sigwait`. That thread is an ordinary thread, not a signal
//! handler, so it may run hooks, lock a mutex and write to stdout. When
//! SIGTERM arrives it runs whatever [`arm`] was last given, reports the run
//! as canceled, tells the rest of its own process group (rustic's `rclone
//! serve` child, which no destructor will reach) to stop, and exits.
//!
//! The window and systemd both send SIGTERM first now and SIGKILL only
//! after `constants::TERM_GRACE`, which is what makes any of this
//! reachable: see `app::child::ChildHandle::cancel` and
//! `timers::service_text`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use crate::debug::ENGINE;
use crate::hooks;
use crate::profile::Hook;
use crate::{debug_log, error_log};

/// What a SIGTERM should do, from now until the run finishes on its own.
pub struct Armed {
    /// The run's hooks; the `After` ones run, as a failure.
    pub hooks: Vec<Hook>,
    /// Reports the run as canceled to whoever is listening: the window,
    /// over the child's stdout. Called once, after the hooks.
    pub report: Box<dyn FnOnce() + Send>,
}

impl std::fmt::Debug for Armed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Armed")
            .field("hooks", &self.hooks.len())
            .finish_non_exhaustive()
    }
}

/// Identifies one [`arm`] call, so [`claim_after_hooks`] only ever answers
/// for its own run: a process that ran several backups one after another
/// (or side by side, as a test binary could) must not have the first one's
/// claim decide the second one's hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticket(u64);

struct State {
    /// What SIGTERM would run right now, and which [`arm`] call it is for.
    armed: Mutex<Option<(Ticket, Armed)>>,
    next_ticket: AtomicU64,
    /// Set by the SIGTERM thread, under `armed`'s lock, before it takes
    /// anything: from then on no claim succeeds, since the process is on
    /// its way out and the SIGTERM thread is the one running the hooks.
    terminating: AtomicBool,
    /// Held while `AfterHookGuard` runs the After hooks itself. The SIGTERM
    /// thread takes it before exiting, so a SIGTERM that lands halfway
    /// through a normal run of the hooks waits for them rather than
    /// `_exit`ing out from under them.
    hooks_running: Mutex<()>,
}

static STATE: OnceLock<State> = OnceLock::new();

fn state() -> &'static State {
    STATE.get_or_init(|| State {
        armed: Mutex::new(None),
        next_ticket: AtomicU64::new(0),
        terminating: AtomicBool::new(false),
        hooks_running: Mutex::new(()),
    })
}

/// Block SIGTERM for this thread — and so for every thread it goes on to
/// start — and start the one thread that will receive it instead. Call
/// first thing in `main`, before anything can have started a thread:
/// a thread already running when this is called keeps the default
/// disposition, and a SIGTERM delivered there ends the process the old way.
pub fn install() {
    let state = state();
    // SAFETY: plain libc calls on a `sigset_t` this function owns and
    // initializes before use. `pthread_sigmask` changes only the calling
    // thread's mask (and, by inheritance, threads it creates afterward).
    let set = unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&raw mut set);
        libc::sigaddset(&raw mut set, libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_BLOCK, &raw const set, std::ptr::null_mut());
        set
    };
    let spawned = std::thread::Builder::new()
        .name("sigterm".to_owned())
        .spawn(move || {
            let mut signal: libc::c_int = 0;
            // SAFETY: `set` is a valid, initialized `sigset_t`; `signal`
            // is a valid out-pointer for the duration of the call.
            while unsafe { libc::sigwait(&raw const set, &raw mut signal) } != 0 {}
            on_term(state);
        });
    if let Err(err) = spawned {
        error_log!(ENGINE, "could not start the SIGTERM thread: {err}");
    }
}

/// What a SIGTERM should do from now on, replacing whatever was armed
/// before; the returned [`Ticket`] is what [`claim_after_hooks`] needs. A
/// process that never called [`install`] keeps this and forgets it,
/// harmlessly.
pub fn arm(armed: Armed) -> Ticket {
    let state = state();
    let ticket = Ticket(state.next_ticket.fetch_add(1, Ordering::Relaxed));
    *state.armed.lock().unwrap_or_else(PoisonError::into_inner) = Some((ticket, armed));
    ticket
}

/// Never returns once a SIGTERM is being handled, so the caller cannot
/// report an outcome or end the process while the signal thread is still
/// running the After hooks; that thread ends the process itself. Returns at
/// once otherwise.
pub fn wait_if_terminating() {
    if !state().terminating.load(Ordering::SeqCst) {
        return;
    }
    debug_log!(
        ENGINE,
        "SIGTERM is being handled; waiting for it to end the process"
    );
    loop {
        std::thread::park();
    }
}

/// Claims the running of `ticket`'s After hooks for the caller, who must
/// run them while holding what this returns. `None` means a SIGTERM got
/// there first and is running them itself (the process is on its way out).
/// Disarms `ticket` too, if it is still what SIGTERM would run: a run whose
/// hooks have already been dealt with must not be reported canceled by a
/// later SIGTERM as if they had not. Leaves anything armed since by a
/// different run alone.
pub fn claim_after_hooks(ticket: Ticket) -> Option<MutexGuard<'static, ()>> {
    let state = state();
    let mut armed = state.armed.lock().unwrap_or_else(PoisonError::into_inner);
    if state.terminating.load(Ordering::SeqCst) {
        return None;
    }
    if armed.as_ref().is_some_and(|(armed, _)| *armed == ticket) {
        *armed = None;
    }
    // Taken before `armed` is let go of, so the SIGTERM thread — which
    // takes `armed` first, then this — cannot slip in between, find nothing
    // armed and nothing running, and exit under the hooks about to run.
    Some(
        state
            .hooks_running
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    )
}

fn on_term(state: &State) {
    debug_log!(ENGINE, "SIGTERM: stopping");
    // A Before hook may be running: it is in its own process group, which
    // the group-wide SIGTERM below does not reach.
    hooks::kill_running();
    let armed = {
        let mut armed = state.armed.lock().unwrap_or_else(PoisonError::into_inner);
        state.terminating.store(true, Ordering::SeqCst);
        armed.take()
    };
    match armed {
        Some((_, armed)) => {
            for result in hooks::run_after(&armed.hooks, false) {
                if !result.ok {
                    error_log!(ENGINE, "hook \"{}\" failed: {}", result.name, result.detail);
                }
            }
            (armed.report)();
        }
        // Nothing armed: no backup is between its Before and After hooks,
        // or one is running its After hooks itself right now. Wait for
        // those to finish rather than exit under them; they are bounded by
        // `HOOK_TIMEOUT` each, and the window and systemd both allow
        // `TERM_GRACE` before they stop waiting.
        None => drop(
            state
                .hooks_running
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        ),
    }
    // `_exit` runs no destructors, so nothing would otherwise tell rustic's
    // `rclone serve` child to go: it is in this process's own group, so a
    // SIGTERM to the group reaches it. This process is in that group too,
    // but has SIGTERM blocked on every thread, so the extra one just stays
    // pending for the few instructions until `_exit`.
    //
    // SAFETY: `kill(0, ..)` signals the calling process's own group; `_exit`
    // never returns.
    unsafe {
        libc::kill(0, libc::SIGTERM);
        libc::_exit(128 + libc::SIGTERM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armed() -> Armed {
        Armed {
            hooks: Vec::new(),
            report: Box::new(|| {}),
        }
    }

    /// Both tests read and write the one process-wide state; run them one
    /// at a time, or one could re-arm while the other is asserting.
    fn serial() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn armed_ticket() -> Option<Ticket> {
        state()
            .armed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|(ticket, _)| *ticket)
    }

    /// Before tickets, one process-wide "the After hooks have run" flag
    /// meant the second backup a process ran never got its After hooks at
    /// all: the first run's claim had already set it for good.
    #[test]
    fn each_run_claims_its_own_after_hooks() {
        let _serial = serial();
        let first = arm(armed());
        assert!(claim_after_hooks(first).is_some());
        let second = arm(armed());
        assert!(
            claim_after_hooks(second).is_some(),
            "a second run in the same process must still get its own After hooks"
        );
    }

    #[test]
    fn a_claim_never_disarms_a_different_run() {
        let _serial = serial();
        let earlier = arm(armed());
        let later = arm(armed());

        let claimed = claim_after_hooks(earlier);

        assert!(claimed.is_some());
        drop(claimed);
        assert_eq!(
            armed_ticket(),
            Some(later),
            "the later run is still what a SIGTERM would stop"
        );
        let claimed = claim_after_hooks(later);
        assert!(claimed.is_some());
        drop(claimed);
        assert_ne!(armed_ticket(), Some(later), "and its own claim disarms it");
    }
}
