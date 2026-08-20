//! Week 5 · Day 4 — the codec that lets a client survive its own mistakes.
//!
//! Day 3 drew a line between two kinds of failure: a *protocol* error, where the
//! peer sent something malformed but the socket is fine, and a *connection*
//! error, where the socket itself is gone. The promise was that Day 4 would make
//! that distinction explicit. Here it is — and it turns out `LinesCodec` alone
//! cannot keep it.
//!
//! # The problem
//!
//! `Framed` **terminates its stream after the decoder returns an error.** Once
//! `decode` reports `Err`, the next `poll_next` yields `None` and the stream is
//! finished, whatever the codec's own opinion about recovering
//! (<https://github.com/tokio-rs/tokio/issues/3976> — the alternative is worse,
//! because for most codecs a decode error means the byte stream is desynchronised
//! and every later frame is garbage).
//!
//! `LinesCodec` reports an over-long line as `Err(MaxLineLengthExceeded)`. So the
//! obvious server — `Framed<TcpStream, LinesCodec>`, match on the error, reply,
//! keep looping — reads like it recovers, compiles, and then quietly drops the
//! client on the next iteration. There is a test below that pins that behaviour,
//! because "obviously it keeps going" is exactly the assumption worth checking.
//!
//! # The fix
//!
//! If the connection is supposed to survive an event, that event is not an error.
//! So this codec has an error type of `io::Error` — genuine connection failures,
//! nothing else — and reports an over-long line as a *successful* decode of
//! [`Line::TooLong`].
//!
//! `Framed` never sees a decoder error, so it never fuses. `LinesCodec` still
//! does the work underneath, including discarding the rest of the over-long line
//! up to the next newline.
//!
//! That is a design rule worth taking to Phase Three: **your `Decoder::Error`
//! type is a statement that the stream is unusable.** Anything the peer can do
//! wrong and then recover from belongs in `Item`, not in `Error`.

use std::io;

use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder, LinesCodec, LinesCodecError};

use crate::protocol::MAX_LINE;

/// One thing read from a client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A complete line, newline already stripped.
    Ok(String),
    /// The peer sent more than [`MAX_LINE`] bytes without a newline. Their input
    /// is gone — there is no way to know where the line was meant to end — but
    /// the connection is healthy and the next line will decode normally.
    TooLong,
}

/// `LinesCodec` with the length limit reclassified from error to event.
#[derive(Debug)]
pub struct ChatLines {
    inner: LinesCodec,
}

impl ChatLines {
    pub fn new() -> ChatLines {
        ChatLines::with_max_length(MAX_LINE)
    }

    /// The bound is not optional on a public port. Without one, a peer that
    /// opens a connection and never sends a newline makes the server buffer
    /// their bytes until the process dies — one connection, no authentication,
    /// whole server down.
    pub fn with_max_length(max: usize) -> ChatLines {
        ChatLines {
            inner: LinesCodec::new_with_max_length(max),
        }
    }
}

impl Default for ChatLines {
    fn default() -> Self {
        ChatLines::new()
    }
}

/// Maps `LinesCodec`'s two failures onto our one: I/O stays fatal, length
/// becomes an ordinary item.
fn reclassify(
    result: Result<Option<String>, LinesCodecError>,
) -> Result<Option<Line>, io::Error> {
    match result {
        Ok(Some(line)) => Ok(Some(Line::Ok(line))),
        Ok(None) => Ok(None),
        Err(LinesCodecError::MaxLineLengthExceeded) => Ok(Some(Line::TooLong)),
        Err(LinesCodecError::Io(e)) => Err(e),
    }
}

impl Decoder for ChatLines {
    type Item = Line;
    /// Deliberately `io::Error` and not a protocol error type: the only thing
    /// that should end this stream is the socket.
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Line>, io::Error> {
        reclassify(self.inner.decode(src))
    }

    /// Delegated rather than left to the default, so a client that sends a final
    /// line without a trailing newline and then closes still has it delivered.
    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Line>, io::Error> {
        reclassify(self.inner.decode_eof(src))
    }
}

impl Encoder<String> for ChatLines {
    type Error = io::Error;

    fn encode(&mut self, item: String, dst: &mut BytesMut) -> Result<(), io::Error> {
        self.inner.encode(item, dst).map_err(|e| match e {
            LinesCodecError::Io(e) => e,
            // Unreachable in practice: the server's own output is short. Mapped
            // rather than unwrapped because a panic here would take down a
            // client task for a message we chose to send.
            LinesCodecError::MaxLineLengthExceeded => {
                io::Error::new(io::ErrorKind::InvalidData, "outgoing line too long")
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use tokio::io::{AsyncWriteExt, DuplexStream};
    use tokio_util::codec::Framed;

    /// Two writes: something far too long, then a perfectly good line. Both are
    /// already sitting in the buffer before anything is read.
    ///
    /// Returns the peer end as well as the server end, and the caller must hold
    /// on to it — dropping the peer closes the connection, and the tests here
    /// are about what happens on a connection that is still open.
    async fn overlong_then_good() -> (DuplexStream, DuplexStream) {
        let (mut peer, server) = tokio::io::duplex(8 * 1024);
        peer.write_all(&vec![b'x'; MAX_LINE + 10]).await.unwrap();
        peer.write_all(b"\nhello\n").await.unwrap();
        peer.flush().await.unwrap();
        (peer, server)
    }

    /// The behaviour that makes `ChatLines` necessary.
    ///
    /// If this test ever starts failing because `Framed` learned to continue
    /// after a decoder error, `ChatLines` becomes redundant — which is worth
    /// knowing, and is why the assumption is written down as a test instead of a
    /// comment.
    #[tokio::test]
    async fn framed_stops_yielding_after_a_codec_error() {
        let (_peer, server) = overlong_then_good().await;
        let mut framed = Framed::new(server, LinesCodec::new_with_max_length(MAX_LINE));

        assert!(matches!(
            framed.next().await,
            Some(Err(LinesCodecError::MaxLineLengthExceeded))
        ));

        // "hello" is in the buffer, and the peer is still connected. The stream
        // is over anyway.
        assert!(
            framed.next().await.is_none(),
            "Framed is fused after a decoder error — this is why an over-long \
             line disconnects a client that uses LinesCodec directly"
        );
    }

    #[tokio::test]
    async fn chat_lines_reports_the_overlong_line_and_keeps_going() {
        let (_peer, server) = overlong_then_good().await;
        let mut framed = Framed::new(server, ChatLines::new());

        assert_eq!(framed.next().await.unwrap().unwrap(), Line::TooLong);
        assert_eq!(
            framed.next().await.unwrap().unwrap(),
            Line::Ok("hello".to_string()),
            "the line after an over-long one must still arrive"
        );
    }

    #[tokio::test]
    async fn ordinary_lines_decode_unchanged() {
        let (mut peer, server) = tokio::io::duplex(1024);
        peer.write_all(b"one\ntwo\nthree\n").await.unwrap();
        peer.flush().await.unwrap();
        drop(peer);

        let mut framed = Framed::new(server, ChatLines::new());
        let mut seen = Vec::new();
        while let Some(Ok(Line::Ok(line))) = framed.next().await {
            seen.push(line);
        }

        assert_eq!(seen, ["one", "two", "three"]);
    }

    /// A client that closes without a trailing newline still gets its last line
    /// delivered — the reason `decode_eof` is delegated rather than defaulted.
    #[tokio::test]
    async fn a_final_line_without_a_newline_still_arrives() {
        let (mut peer, server) = tokio::io::duplex(1024);
        peer.write_all(b"/quit").await.unwrap();
        peer.flush().await.unwrap();
        drop(peer);

        let mut framed = Framed::new(server, ChatLines::new());
        assert_eq!(
            framed.next().await.unwrap().unwrap(),
            Line::Ok("/quit".to_string())
        );
    }
}
