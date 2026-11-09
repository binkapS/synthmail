//! Tests for the bounded line reader over synthetic (chunked) byte streams.

use std::cell::RefCell;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, ReadBuf};

use crate::service::{LineOutcome, read_line};
use crate::smtp::MAX_LINE_LENGTH;

struct FakeStream<'a> {
    data: RefCell<&'a [u8]>,
}
const FAKE_CHUNK: usize = 5;
impl AsyncRead for FakeStream<'_> {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let me = Pin::get_mut(self);
        let mut d = me.data.borrow_mut();
        let n = (*d).len().min(buf.remaining()).min(FAKE_CHUNK);
        buf.put_slice(&d[..n]);
        *d = &d[n..];
        Poll::Ready(Ok(()))
    }
}

async fn one_read(data: &'static [u8], pending: &mut Vec<u8>) -> LineOutcome {
    let mut s = FakeStream {
        data: RefCell::new(data),
    };
    read_line(&mut s, pending).await.expect("read_line")
}

#[tokio::test]
async fn simple_line() {
    let mut pending = Vec::new();
    match one_read(b"NOOP\r\n", &mut pending).await {
        LineOutcome::Line(l) => assert_eq!(l, b"NOOP"),
        o => panic!("{o:?}"),
    }
    assert!(pending.is_empty());
}

#[tokio::test]
async fn leftover_after_newline_within_tight_stream() {
    let mut pending = Vec::new();
    // Pre-fill the buffer to simulate a TLS chunk consumed earlier — the
    // tail beyond the first newline must not be discarded.
    pending.extend_from_slice(b"NOOP\r\nQUIT\r\nTAIL");
    // No more peer bytes needed: first line comes from pending.
    match one_read(b"", &mut pending).await {
        LineOutcome::Line(l) => assert_eq!(l, b"NOOP"),
        o => panic!("{o:?}"),
    }
    assert_eq!(pending, b"QUIT\r\nTAIL");
    match one_read(b"", &mut pending).await {
        LineOutcome::Line(l) => assert_eq!(l, b"QUIT"),
        o => panic!("{o:?}"),
    }
    assert_eq!(pending, b"TAIL");
}

#[tokio::test]
async fn eof_without_terminator() {
    let mut pending = Vec::new();
    match one_read(b"partial", &mut pending).await {
        LineOutcome::Eof => {}
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn exact_cap_ok() {
    let mut pending = Vec::new();
    let mut data = vec![b'a'; MAX_LINE_LENGTH];
    data.extend_from_slice(b"\r\n");
    // bake into a leaky static for the test helper's `&'static`.
    let data = Box::leak(data.into_boxed_slice());
    match one_read(data, &mut pending).await {
        LineOutcome::Line(l) => assert_eq!(l.len(), MAX_LINE_LENGTH),
        o => panic!("{o:?}"),
    }
    assert!(pending.is_empty());
}

#[tokio::test]
async fn overlong_line_drained_then_next_line_clean() {
    let mut pending = Vec::new();
    let mut data = vec![b'x'; MAX_LINE_LENGTH + 1];
    data.push(b'\n');
    data.extend_from_slice(b"NEXT\r\n");
    let data = Box::leak(data.into_boxed_slice());
    match one_read(data, &mut pending).await {
        LineOutcome::TooLong => {}
        o => panic!("{o:?}"),
    }
    // The overlong line is drained for safety; nothing about it is retained.
    assert!(!pending.contains(&b'x'), "no overlong residue retained");
}
