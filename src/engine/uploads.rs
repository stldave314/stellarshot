// SPDX-License-Identifier: GPL-3.0-only

//! Several pack uploads at once, for storage behind a slow connection.
//!
//! rustic writes one pack file at a time and waits for each upload to finish
//! before handing over the next. On a folder or a drive that costs nothing;
//! on Google Drive, where every upload waits on the network and on Google's
//! own request pacing, it leaves the connection idle most of the time and the
//! backup appears to stall between packs. restic, which Déjà Dup runs, keeps
//! several uploads going instead.
//!
//! [`ParallelUploads`] wraps the storage rustic is given, without changing
//! rustic: a pack write returns as soon as a worker has taken it, and up to
//! [`UPLOAD_CONNECTIONS`] packs travel at once.
//!
//! The repository stays exactly as safe as with one upload at a time. rustic
//! only refers to a pack from an index file, and to an index from a snapshot,
//! each written after what it refers to. So every operation other than a
//! pack write — writing an index or a snapshot, reading, listing, removing —
//! first waits for all uploads in flight, and fails if any of them failed.
//! An index can therefore never name a pack that did not arrive; a failed
//! upload fails the backup, and leaves only an unreferenced pack behind, as
//! an interrupted backup does.

use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use bytes::Bytes;
use rustic_core::{
    BytesList, ErrorKind, FileType, Id, ReadBackend, RusticError, RusticResult, WriteBackend,
};

use super::progress::SinkSlot;
use crate::constants::UPLOAD_CONNECTIONS;
use crate::debug::ENGINE;
use crate::debug_log;

/// One pack waiting for a worker.
struct Upload {
    id: Id,
    cacheable: bool,
    content: BytesList,
}

/// What the workers share with the writer.
#[derive(Default)]
struct State {
    /// Uploads handed to a worker and not yet finished.
    in_flight: usize,
    /// The first upload that failed, until it is reported.
    failure: Option<Box<RusticError>>,
    /// An upload failed and has been reported; every later operation fails
    /// too, so nothing is ever written on top of a missing pack.
    failed: bool,
}

/// Storage that uploads packs concurrently. See the module documentation.
pub(crate) struct ParallelUploads {
    inner: Arc<dyn WriteBackend>,
    state: Arc<(Mutex<State>, Condvar)>,
    /// Hands a pack to the next free worker, waiting until one is free, so
    /// no more than [`UPLOAD_CONNECTIONS`] packs are held in memory. Taken
    /// on drop, which closes it and lets the workers end.
    queue: Option<SyncSender<Upload>>,
    workers: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for ParallelUploads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParallelUploads")
            .field("location", &self.inner.location())
            .finish()
    }
}

impl ParallelUploads {
    /// Wrap `inner`, reporting the bytes of each finished upload to `slot`.
    pub(crate) fn new(inner: Arc<dyn WriteBackend>, slot: SinkSlot) -> Self {
        Self::with_connections(inner, slot, UPLOAD_CONNECTIONS)
    }

    fn with_connections(inner: Arc<dyn WriteBackend>, slot: SinkSlot, connections: usize) -> Self {
        let (queue, uploads) = sync_channel::<Upload>(0);
        let uploads = Arc::new(Mutex::new(uploads));
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        slot.count_uploads();
        let workers = (0..connections.max(1))
            .filter_map(|_| {
                let (inner, uploads, state, slot) =
                    (inner.clone(), uploads.clone(), state.clone(), slot.clone());
                std::thread::Builder::new()
                    .name("stellarshot-upload".to_owned())
                    .spawn(move || work(&*inner, &uploads, &state, &slot))
                    .ok()
            })
            .collect();
        Self {
            inner,
            state,
            queue: Some(queue),
            workers,
        }
    }

    /// Wait for every upload in flight, then fail if any of them failed.
    fn settle(&self) -> RusticResult<()> {
        let (lock, finished) = &*self.state;
        let mut state = lock.lock().map_err(|_| poisoned())?;
        while state.in_flight > 0 {
            state = finished.wait(state).map_err(|_| poisoned())?;
        }
        if let Some(failure) = state.failure.take() {
            state.failed = true;
            return Err(failure);
        }
        if state.failed {
            return Err(RusticError::new(
                ErrorKind::Backend,
                "An earlier upload failed, so nothing more is written.",
            ));
        }
        Ok(())
    }
}

fn poisoned() -> Box<RusticError> {
    RusticError::new(ErrorKind::Internal, "An upload worker panicked.")
}

/// A worker: take packs off the queue and upload them until it closes.
fn work(
    inner: &dyn WriteBackend,
    uploads: &Mutex<Receiver<Upload>>,
    state: &(Mutex<State>, Condvar),
    slot: &SinkSlot,
) {
    loop {
        // The receiver is locked only while waiting for the next pack.
        let next = match uploads.lock() {
            Ok(uploads) => uploads.recv(),
            Err(_) => return,
        };
        let Ok(upload) = next else {
            return;
        };
        let size = upload.content.size() as u64;
        let result =
            inner.write_bytes(FileType::Pack, &upload.id, upload.cacheable, upload.content);
        // Reported before the upload counts as finished, and outside the
        // lock, so progress output never holds up the other workers.
        if result.is_ok() {
            slot.uploaded(size);
        }
        let (lock, finished) = state;
        if let Ok(mut state) = lock.lock() {
            state.in_flight -= 1;
            if let Err(err) = result {
                debug_log!(ENGINE, "upload of pack {} failed: {err}", upload.id);
                if state.failure.is_none() && !state.failed {
                    state.failure = Some(err);
                }
            }
        }
        finished.notify_all();
    }
}

impl ReadBackend for ParallelUploads {
    fn location(&self) -> String {
        self.inner.location()
    }

    fn list_with_size(&self, tpe: FileType) -> RusticResult<Vec<(Id, u32)>> {
        self.settle()?;
        self.inner.list_with_size(tpe)
    }

    fn list(&self, tpe: FileType) -> RusticResult<Vec<Id>> {
        self.settle()?;
        self.inner.list(tpe)
    }

    fn read_full(&self, tpe: FileType, id: &Id) -> RusticResult<Bytes> {
        self.settle()?;
        self.inner.read_full(tpe, id)
    }

    fn read_partial(
        &self,
        tpe: FileType,
        id: &Id,
        cacheable: bool,
        offset: u32,
        length: u32,
    ) -> RusticResult<Bytes> {
        self.settle()?;
        self.inner.read_partial(tpe, id, cacheable, offset, length)
    }

    fn warmup_path(&self, tpe: FileType, id: &Id) -> String {
        self.inner.warmup_path(tpe, id)
    }

    fn needs_warm_up(&self) -> bool {
        self.inner.needs_warm_up()
    }

    fn warm_up(&self, tpe: FileType, id: &Id) -> RusticResult<()> {
        self.inner.warm_up(tpe, id)
    }
}

impl WriteBackend for ParallelUploads {
    fn create(&self) -> RusticResult<()> {
        self.settle()?;
        self.inner.create()
    }

    fn write_bytes(
        &self,
        tpe: FileType,
        id: &Id,
        cacheable: bool,
        content: BytesList,
    ) -> RusticResult<()> {
        // Everything but a pack refers to packs, or is read back at once.
        if tpe != FileType::Pack {
            self.settle()?;
            return self.inner.write_bytes(tpe, id, cacheable, content);
        }
        {
            let mut state = self.state.0.lock().map_err(|_| poisoned())?;
            // Stop at the first failure rather than uploading the rest.
            if let Some(failure) = state.failure.take() {
                state.failed = true;
                return Err(failure);
            }
            state.in_flight += 1;
        }
        let upload = Upload {
            id: *id,
            cacheable,
            content,
        };
        let sent = self
            .queue
            .as_ref()
            .is_some_and(|queue| queue.send(upload).is_ok());
        if !sent {
            if let Ok(mut state) = self.state.0.lock() {
                state.in_flight -= 1;
            }
            self.state.1.notify_all();
            return Err(RusticError::new(
                ErrorKind::Internal,
                "No upload worker is running.",
            ));
        }
        Ok(())
    }

    fn remove(&self, tpe: FileType, id: &Id, cacheable: bool) -> RusticResult<()> {
        self.settle()?;
        self.inner.remove(tpe, id, cacheable)
    }
}

impl Drop for ParallelUploads {
    /// Let uploads in flight finish rather than cutting them off, then end
    /// the workers. They hold the storage too, and it must close here, not
    /// on a worker thread: closing rclone's storage stops the rclone process,
    /// and a process that exits first leaves rclone running.
    fn drop(&mut self) {
        let _ = self.settle();
        drop(self.queue.take());
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    /// Storage that takes `delay` per write and records what arrived.
    #[derive(Default)]
    struct Slow {
        delay: Duration,
        files: Mutex<Vec<(FileType, Id, usize)>>,
        /// Fail the pack write with this number (counting from 1).
        fail_pack: Option<usize>,
        packs: AtomicUsize,
        busy: AtomicUsize,
        most_busy: AtomicUsize,
    }

    impl ReadBackend for Slow {
        fn location(&self) -> String {
            "slow".to_owned()
        }
        fn list_with_size(&self, tpe: FileType) -> RusticResult<Vec<(Id, u32)>> {
            let files = self.files.lock().unwrap();
            Ok(files
                .iter()
                .filter(|(kind, _, _)| *kind == tpe)
                .map(|(_, id, size)| (*id, *size as u32))
                .collect())
        }
        fn read_full(&self, _: FileType, _: &Id) -> RusticResult<Bytes> {
            Ok(Bytes::new())
        }
        fn read_partial(
            &self,
            _: FileType,
            _: &Id,
            _: bool,
            _: u32,
            _: u32,
        ) -> RusticResult<Bytes> {
            Ok(Bytes::new())
        }
        fn warmup_path(&self, _: FileType, _: &Id) -> String {
            String::new()
        }
    }

    impl WriteBackend for Slow {
        fn write_bytes(
            &self,
            tpe: FileType,
            id: &Id,
            _: bool,
            content: BytesList,
        ) -> RusticResult<()> {
            let busy = self.busy.fetch_add(1, Ordering::SeqCst) + 1;
            self.most_busy.fetch_max(busy, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            self.busy.fetch_sub(1, Ordering::SeqCst);
            if tpe == FileType::Pack {
                let number = self.packs.fetch_add(1, Ordering::SeqCst) + 1;
                if Some(number) == self.fail_pack {
                    return Err(RusticError::new(
                        ErrorKind::Backend,
                        "the network went away",
                    ));
                }
            }
            self.files.lock().unwrap().push((tpe, *id, content.size()));
            Ok(())
        }
        fn remove(&self, _: FileType, _: &Id, _: bool) -> RusticResult<()> {
            Ok(())
        }
    }

    fn id(n: u8) -> Id {
        Id::new([n; 32])
    }

    fn pack(n: u8) -> BytesList {
        BytesList::from(vec![n; 1000])
    }

    #[test]
    fn packs_upload_side_by_side() {
        let slow = Arc::new(Slow {
            delay: Duration::from_millis(200),
            ..Slow::default()
        });
        let uploads = ParallelUploads::with_connections(slow.clone(), SinkSlot::default(), 4);

        let started = Instant::now();
        for n in 0..8 {
            uploads
                .write_bytes(FileType::Pack, &id(n), false, pack(n))
                .unwrap();
        }
        uploads
            .write_bytes(FileType::Index, &id(100), true, pack(100))
            .unwrap();
        let elapsed = started.elapsed();

        assert_eq!(slow.most_busy.load(Ordering::SeqCst), 4);
        assert!(
            elapsed < Duration::from_millis(1000),
            "8 uploads of 200 ms, 4 at a time, took {elapsed:?}"
        );
        assert_eq!(slow.files.lock().unwrap().len(), 9);
    }

    #[test]
    fn an_index_is_written_only_after_every_pack_arrived() {
        let slow = Arc::new(Slow {
            delay: Duration::from_millis(50),
            ..Slow::default()
        });
        let uploads = ParallelUploads::with_connections(slow.clone(), SinkSlot::default(), 4);
        for n in 0..6 {
            uploads
                .write_bytes(FileType::Pack, &id(n), false, pack(n))
                .unwrap();
        }

        uploads
            .write_bytes(FileType::Index, &id(100), true, pack(100))
            .unwrap();

        let packs = uploads.list_with_size(FileType::Pack).unwrap();
        assert_eq!(packs.len(), 6, "all packs were stored before the index");
    }

    #[test]
    fn a_failed_upload_fails_the_index_and_everything_after_it() {
        let slow = Arc::new(Slow {
            delay: Duration::from_millis(20),
            fail_pack: Some(3),
            ..Slow::default()
        });
        let uploads = ParallelUploads::with_connections(slow.clone(), SinkSlot::default(), 2);
        // A write may be refused as soon as the failure is known, or accepted
        // before it is; either way the index must not be written.
        for n in 0..5 {
            let _ = uploads.write_bytes(FileType::Pack, &id(n), false, pack(n));
        }

        let index = uploads.write_bytes(FileType::Index, &id(100), true, pack(100));
        assert!(index.is_err(), "the index must not name a missing pack");
        let snapshot = uploads.write_bytes(FileType::Snapshot, &id(101), true, pack(101));
        assert!(snapshot.is_err(), "nor may anything be written after it");

        let files = slow.files.lock().unwrap();
        assert!(files.iter().all(|(kind, _, _)| *kind == FileType::Pack));
    }
}
