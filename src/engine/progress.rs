// SPDX-License-Identifier: GPL-3.0-only

//! Progress reporting from rustic to whoever is watching.
//!
//! rustic asks a [`ProgressBars`] for a progress handle per phase and then calls
//! `inc` on it from its worker threads, often once per blob. Forwarding every
//! call would flood the UI (and, for the `--run` child, stdout), so each handle
//! throttles to [`PROGRESS_INTERVAL`] and always reports its final state.
//!
//! The sink is attached per operation through a [`SinkSlot`], because rustic
//! fixes its progress bars when the repository is constructed, before anyone
//! knows which operation will follow.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use rustic_core::{Progress, ProgressBars, ProgressType, RusticProgress};
use serde::{Deserialize, Serialize};

use crate::constants::PROGRESS_INTERVAL;

/// Which part of an operation is running. Mapped from rustic's own phase
/// titles so the UI can show a localized label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// Reading the index, scanning sources, and other set-up work.
    Preparing,
    /// Reading and storing file contents.
    BackingUp,
    /// Writing restored file contents.
    Restoring,
    /// Verifying the repository.
    Checking,
}

impl Phase {
    fn from_title(title: &str) -> Self {
        if title.starts_with("backing up") {
            Self::BackingUp
        } else if title.starts_with("restoring") {
            Self::Restoring
        } else if title.starts_with("checking") {
            Self::Checking
        } else {
            Self::Preparing
        }
    }
}

/// One progress report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub phase: Phase,
    /// Units done so far: bytes when `bytes` is set, otherwise items.
    pub done: u64,
    /// Total units, once known.
    pub total: Option<u64>,
    pub bytes: bool,
    /// Bytes stored at the destination so far, for storage that uploads
    /// packs in the background (see [`super::uploads`]). Stored bytes are
    /// compressed and deduplicated, so they do not add up to `done`.
    #[serde(default)]
    pub uploaded: Option<u64>,
}

/// Receives progress reports. Called from rustic's worker threads.
pub trait ProgressSink: Send + Sync {
    fn update(&self, event: &ProgressEvent);
}

/// A sink that discards everything.
pub struct NoProgress;

impl ProgressSink for NoProgress {
    fn update(&self, _event: &ProgressEvent) {}
}

/// Where progress for the current operation goes. Cloned into every handle
/// rustic creates, and into the upload workers; empty between operations.
#[derive(Clone, Default)]
pub(crate) struct SinkSlot(Arc<SlotState>);

#[derive(Default)]
struct SlotState {
    sink: RwLock<Option<Arc<dyn ProgressSink>>>,
    /// Uploads are counted: the storage uploads in the background.
    counts_uploads: AtomicBool,
    /// Bytes uploaded during the current operation.
    uploaded: AtomicU64,
    /// The last event sent, repeated with a new upload figure when a pack
    /// arrives while nothing else moves.
    last: Mutex<Option<ProgressEvent>>,
}

impl SinkSlot {
    /// Send progress to `sink` until the returned guard is dropped.
    pub(crate) fn attach(&self, sink: Arc<dyn ProgressSink>) -> SlotGuard {
        if let Ok(mut slot) = self.0.sink.write() {
            *slot = Some(sink);
        }
        self.0.uploaded.store(0, Ordering::Relaxed);
        if let Ok(mut last) = self.0.last.lock() {
            *last = None;
        }
        SlotGuard(self.clone())
    }

    /// Include the bytes uploaded in every event from now on.
    pub(crate) fn count_uploads(&self) {
        self.0.counts_uploads.store(true, Ordering::Relaxed);
    }

    /// A pack of `bytes` has been stored at the destination.
    pub(crate) fn uploaded(&self, bytes: u64) {
        self.0.uploaded.fetch_add(bytes, Ordering::Relaxed);
        let last = self.0.last.lock().ok().and_then(|last| last.clone());
        if let Some(last) = last {
            self.send(&last);
        }
    }

    fn send(&self, event: &ProgressEvent) {
        let mut event = event.clone();
        if self.0.counts_uploads.load(Ordering::Relaxed) {
            event.uploaded = Some(self.0.uploaded.load(Ordering::Relaxed));
        }
        if let Ok(mut last) = self.0.last.lock() {
            *last = Some(event.clone());
        }
        if let Ok(slot) = self.0.sink.read()
            && let Some(sink) = slot.as_ref()
        {
            sink.update(&event);
        }
    }
}

/// Detaches the sink when dropped, so progress never leaks into the next
/// operation.
pub(crate) struct SlotGuard(SinkSlot);

impl Drop for SlotGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = (self.0).0.sink.write() {
            *slot = None;
        }
    }
}

/// The [`ProgressBars`] rustic is given.
#[derive(Clone, Default)]
pub(crate) struct SinkBars {
    pub(crate) slot: SinkSlot,
}

impl fmt::Debug for SinkBars {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SinkBars")
    }
}

impl ProgressBars for SinkBars {
    fn progress(&self, progress_type: ProgressType, prefix: &str) -> Progress {
        Progress::new(Handle::new(
            self.slot.clone(),
            prefix,
            matches!(progress_type, ProgressType::Bytes),
        ))
    }
}

struct HandleState {
    phase: Phase,
    done: u64,
    total: Option<u64>,
    last_sent: Option<Instant>,
}

/// One progress handle: a phase with a counter.
struct Handle {
    slot: SinkSlot,
    bytes: bool,
    state: Mutex<HandleState>,
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Handle")
    }
}

impl Handle {
    fn new(slot: SinkSlot, title: &str, bytes: bool) -> Self {
        Self {
            slot,
            bytes,
            state: Mutex::new(HandleState {
                phase: Phase::from_title(title),
                done: 0,
                total: None,
                last_sent: None,
            }),
        }
    }

    /// Report the current state if enough time has passed, or always when
    /// `force` is set.
    fn report(&self, state: &mut HandleState, force: bool) {
        let due = state
            .last_sent
            .is_none_or(|sent| sent.elapsed() >= PROGRESS_INTERVAL);
        if !(force || due) {
            return;
        }
        state.last_sent = Some(Instant::now());
        self.slot.send(&ProgressEvent {
            phase: state.phase,
            done: state.done,
            total: state.total,
            bytes: self.bytes,
            uploaded: None,
        });
    }
}

impl RusticProgress for Handle {
    fn is_hidden(&self) -> bool {
        false
    }

    fn set_length(&self, len: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.total = Some(len);
            self.report(&mut state, false);
        }
    }

    fn set_title(&self, title: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.phase = Phase::from_title(title);
        }
    }

    fn inc(&self, inc: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.done = state.done.saturating_add(inc);
            self.report(&mut state, false);
        }
    }

    fn finish(&self) {
        if let Ok(mut state) = self.state.lock() {
            self.report(&mut state, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<ProgressEvent>>);

    impl ProgressSink for Recorder {
        fn update(&self, event: &ProgressEvent) {
            self.0.lock().unwrap().push(event.clone());
        }
    }

    #[test]
    fn throttle_sends_first_and_final() {
        let bars = SinkBars::default();
        let recorder = Arc::new(Recorder::default());
        let _guard = bars.slot.attach(recorder.clone());

        let progress = bars.progress(ProgressType::Bytes, "backing up...");
        for _ in 0..1000 {
            progress.inc(1);
        }
        progress.finish();

        let events = recorder.0.lock().unwrap();
        assert!(
            events.len() >= 2,
            "the first and final events are always sent"
        );
        assert!(
            events.len() < 100,
            "1000 increments produced {} events; throttling is not working",
            events.len()
        );
        let last = events.last().unwrap();
        assert_eq!(last.done, 1000);
        assert_eq!(last.phase, Phase::BackingUp);
        assert!(last.bytes);
    }

    #[test]
    fn a_finished_upload_repeats_the_last_event_with_the_bytes_stored() {
        let bars = SinkBars::default();
        bars.slot.count_uploads();
        let recorder = Arc::new(Recorder::default());
        let _guard = bars.slot.attach(recorder.clone());

        let progress = bars.progress(ProgressType::Bytes, "backing up...");
        progress.inc(100);
        bars.slot.uploaded(40);
        bars.slot.uploaded(2);

        let events = recorder.0.lock().unwrap();
        let last = events.last().unwrap();
        assert_eq!(last.done, 100, "reading has not moved");
        assert_eq!(last.uploaded, Some(42), "uploading has");
        assert_eq!(events.first().unwrap().uploaded, Some(0));
    }

    #[test]
    fn nothing_is_sent_once_the_guard_is_dropped() {
        let bars = SinkBars::default();
        let recorder = Arc::new(Recorder::default());
        drop(bars.slot.attach(recorder.clone()));

        let progress = bars.progress(ProgressType::Counter, "");
        progress.inc(5);
        progress.finish();

        assert!(recorder.0.lock().unwrap().is_empty());
    }
}
