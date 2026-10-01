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

#[cfg(test)]
mod tests {
    use super::*;

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
}
