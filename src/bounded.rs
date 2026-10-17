// SPDX-License-Identifier: GPL-3.0-only

//! Reading a child process's output without letting it use unbounded memory.
//!
//! A hook, a password command or an rclone listing can write as much as it
//! likes; buffering all of it (to put in an error, or to parse) would let one
//! chatty process take gigabytes. These read to the end, so the child never
//! blocks on a full pipe, but keep only what is needed: the last bytes for
//! diagnostics, the first bytes for a value that must fit.

use std::io::Read;

use tokio::io::{AsyncRead, AsyncReadExt};

const CHUNK: usize = 8 * 1024;

/// Read `reader` to the end, keeping only the last `limit` bytes.
pub fn read_tail(mut reader: impl Read, limit: usize) -> Vec<u8> {
    let mut tail = Vec::new();
    let mut buffer = [0u8; CHUNK];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => keep_tail(&mut tail, &buffer[..n], limit),
        }
    }
    tail
}

/// [`read_tail`], for an async reader.
pub async fn read_tail_async(mut reader: impl AsyncRead + Unpin, limit: usize) -> Vec<u8> {
    let mut tail = Vec::new();
    let mut buffer = [0u8; CHUNK];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(n) => keep_tail(&mut tail, &buffer[..n], limit),
        }
    }
    tail
}

fn keep_tail(tail: &mut Vec<u8>, chunk: &[u8], limit: usize) {
    tail.extend_from_slice(chunk);
    if tail.len() > limit {
        let excess = tail.len() - limit;
        tail.drain(..excess);
    }
}

/// Read `reader` to the end, keeping only the first `limit` bytes. The flag
/// says whether there was more than that.
pub fn read_head(mut reader: impl Read, limit: usize) -> (Vec<u8>, bool) {
    let mut head = Vec::new();
    let mut truncated = false;
    let mut buffer = [0u8; CHUNK];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => truncated |= keep_head(&mut head, &buffer[..n], limit),
        }
    }
    (head, truncated)
}

fn keep_head(head: &mut Vec<u8>, chunk: &[u8], limit: usize) -> bool {
    let room = limit.saturating_sub(head.len());
    head.extend_from_slice(&chunk[..room.min(chunk.len())]);
    chunk.len() > room
}

/// Read `reader` to the end into a buffer that is wiped when dropped, and that
/// never leaves a stray copy behind: a `Vec` that grows in place frees its old
/// allocation with the bytes still in it, so this grows by copying into a new
/// wiped buffer and wiping the old one. For input that holds a password (the
/// job a `--run` child reads). Fails past `limit` bytes.
pub fn read_secret(
    mut reader: impl Read,
    limit: usize,
) -> std::io::Result<zeroize::Zeroizing<Vec<u8>>> {
    let mut buffer = zeroize::Zeroizing::new(Vec::with_capacity(CHUNK * 8));
    let mut chunk = zeroize::Zeroizing::new([0u8; CHUNK]);
    loop {
        let n = reader.read(&mut chunk[..])?;
        if n == 0 {
            return Ok(buffer);
        }
        if buffer.len() + n > limit {
            return Err(std::io::Error::other("more input than allowed"));
        }
        if buffer.len() + n > buffer.capacity() {
            let mut larger = zeroize::Zeroizing::new(Vec::with_capacity(
                (buffer.capacity() * 2).max(buffer.len() + n),
            ));
            larger.extend_from_slice(&buffer);
            // The old buffer is wiped as it drops here.
            buffer = larger;
        }
        buffer.extend_from_slice(&chunk[..n]);
    }
}

/// `value` as JSON in a buffer that is wiped when dropped, sized exactly
/// before anything is written, so no partial copy is left in freed memory.
pub fn json_secret<T: serde::Serialize>(
    value: &T,
) -> serde_json::Result<zeroize::Zeroizing<Vec<u8>>> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value)?;
    let mut buffer = zeroize::Zeroizing::new(Vec::with_capacity(counter.0));
    serde_json::to_writer(&mut *buffer, value)?;
    Ok(buffer)
}

/// The most recent `limit` bytes of `text`, starting on a `char` boundary,
/// for whatever goes into an error message or the log.
pub fn tail_str(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let start = text.len() - limit;
    let boundary = (start..=text.len())
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(text.len());
    &text[boundary..]
}

/// Reap `child`, already killed, within `limit`: whether it went. A process
/// stuck in the kernel (on a hung NFS share or FUSE mount) ignores even
/// SIGKILL until that returns, and a plain `wait` would hold up whatever
/// waits on it, a backup's repository lock included. One left behind stays
/// a zombie until this process ends, which is the lesser harm.
pub fn reap_within(child: &mut std::process::Child, limit: std::time::Duration) -> bool {
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return true,
            Ok(None) if started.elapsed() >= limit => return false,
            Ok(None) => std::thread::sleep(crate::constants::PROCESS_POLL_INTERVAL),
        }
    }
}

/// Run `command` to the end, as `Command::output` does, but give up after
/// `limit`: its whole process group is then killed and this returns
/// `TimedOut`. For the small helpers Stellarshot runs (`systemctl`, `dconf`,
/// `fusermount`, `rclone version`), any of which can hang on a wedged
/// service or mount and would otherwise hold up whatever waits on it, a
/// scheduled run included. Each pipe keeps at most `HELPER_OUTPUT_MAX`
/// bytes from its start.
pub fn output_within(
    command: &mut std::process::Command,
    limit: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    use crate::constants::{HELPER_OUTPUT_MAX, PROCESS_POLL_INTERVAL};
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let reader = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            pipe.map(|pipe| read_head(pipe, HELPER_OUTPUT_MAX).0)
                .unwrap_or_default()
        })
    };
    let stdout = reader(
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let stderr = reader(
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= limit {
            if let Some(group) = rustix::process::Pid::from_raw(child.id().cast_signed()) {
                let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
            }
            reap_within(&mut child, crate::constants::DRAIN_AFTER_EXIT);
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("no answer within {} seconds", limit.as_secs()),
            ));
        }
        std::thread::sleep(PROCESS_POLL_INTERVAL);
    };
    Ok(std::process::Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaping_gives_up_on_a_process_that_will_not_go() {
        let mut alive = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        assert!(!reap_within(
            &mut alive,
            std::time::Duration::from_millis(300)
        ));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        let _ = alive.kill();
        assert!(reap_within(&mut alive, std::time::Duration::from_secs(5)));
    }

    #[test]
    fn a_helper_that_answers_is_read_and_one_that_hangs_is_stopped() {
        let output = output_within(
            std::process::Command::new("sh").args(["-c", "echo hello; echo oops >&2"]),
            std::time::Duration::from_secs(10),
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"hello\n");
        assert_eq!(output.stderr, b"oops\n");

        let started = std::time::Instant::now();
        let err = output_within(
            std::process::Command::new("sleep").arg("300"),
            std::time::Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn the_tail_keeps_only_the_last_bytes_however_much_was_written() {
        let data: Vec<u8> = (0..100_000u32).map(|n| (n % 251) as u8).collect();

        let tail = read_tail(&data[..], 1000);

        assert_eq!(tail, data[data.len() - 1000..]);
    }

    #[test]
    fn the_head_keeps_the_first_bytes_and_says_when_there_was_more() {
        let data = vec![7u8; 20_000];

        let (head, truncated) = read_head(&data[..], 100);
        assert_eq!(head.len(), 100);
        assert!(truncated);

        let (head, truncated) = read_head(&data[..100], 100);
        assert_eq!(head.len(), 100);
        assert!(!truncated, "exactly the limit is not more than it");
    }

    #[test]
    fn a_tail_of_text_never_splits_a_character() {
        let text = "ééééé";
        let tail = tail_str(text, 3);
        assert!(tail.len() <= 3);
        assert!(text.ends_with(tail));
    }

    #[test]
    fn a_secret_is_read_whole_and_refused_past_its_limit() {
        let data: Vec<u8> = (0..200_000u32).map(|n| (n % 251) as u8).collect();
        assert_eq!(*read_secret(&data[..], 1 << 20).unwrap(), data);
        assert!(read_secret(&data[..], 1000).is_err());
    }

    #[test]
    fn json_is_written_into_a_buffer_of_exactly_its_size() {
        let value = vec!["password"; 100];
        let buffer = json_secret(&value).unwrap();
        assert_eq!(buffer.len(), buffer.capacity());
        assert_eq!(*buffer, serde_json::to_vec(&value).unwrap());
    }
}
