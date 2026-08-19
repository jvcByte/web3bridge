//! Week 5 · Day 3 — a hand-rolled length-prefix codec.
//!
//! `LinesCodec` already exists and Days 4–5 use it. This module exists so the
//! abstraction is not magic: implementing `Decoder` and `Encoder` once makes it
//! obvious what `Framed` is doing on your behalf.
//!
//! # Wire format
//!
//! ```text
//! +--------+--------+--------+--------+---------------------------+
//! |            length (u32, big-endian)        |     payload      |
//! +--------+--------+--------+--------+---------------------------+
//! ```
//!
//! Big-endian because that is network byte order, and every protocol you will
//! meet in Phase Three uses it.
//!
//! # The two rules of writing a decoder
//!
//! 1. **You will be called with partial data.** `decode` runs every time bytes
//!    arrive. Return `Ok(None)` to mean "not enough yet, call me again" — do not
//!    block and do not error.
//! 2. **Bound the allocation before you make it.** The length prefix is attacker-
//!    controlled. A peer sending `0xFFFFFFFF` must get an error, not a 4 GiB
//!    reservation. This is the single most common vulnerability in hand-written
//!    length-prefix codecs.

use std::io;

use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

/// Largest frame we will accept, in bytes.
pub const MAX_FRAME: usize = 64 * 1024;

const HEADER: usize = 4;

#[derive(Debug, Default)]
pub struct LengthPrefixed;

#[derive(Debug)]
pub enum FrameError {
    /// The peer announced a frame larger than [`MAX_FRAME`].
    FrameTooLarge { announced: usize },
    /// The payload was not valid UTF-8.
    NotUtf8,
    Io(io::Error),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::FrameTooLarge { announced } => write!(
                f,
                "peer announced a {announced}-byte frame; max is {MAX_FRAME}"
            ),
            FrameError::NotUtf8 => write!(f, "payload was not valid UTF-8"),
            FrameError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for FrameError {}

// `Framed` requires the codec's error type to be constructible from io::Error,
// since the underlying socket can always fail independently of the protocol.
impl From<io::Error> for FrameError {
    fn from(e: io::Error) -> Self {
        FrameError::Io(e)
    }
}

impl Decoder for LengthPrefixed {
    type Item = String;
    type Error = FrameError;

    /// Called every time bytes arrive. `src` is a rolling buffer that persists
    /// across calls, so partial frames accumulate there between invocations.
    ///
    /// Three possible outcomes:
    ///   - `Ok(None)`      — incomplete, call me again when more arrives
    ///   - `Ok(Some(item))`— one complete frame, consumed from `src`
    ///   - `Err(_)`        — malformed; the connection is usually unrecoverable
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<String>, FrameError> {
        // Not even the header yet.
        if src.len() < HEADER {
            return Ok(None);
        }

        // Peek at the length without consuming it — if the body has not arrived,
        // we must leave the buffer exactly as we found it so the next call sees
        // the header again.
        let mut header = [0u8; HEADER];
        header.copy_from_slice(&src[..HEADER]);
        let len = u32::from_be_bytes(header) as usize;

        // Bound the allocation BEFORE reserving. `len` comes from the network and
        // is entirely attacker-controlled: a peer sending 0xFFFFFFFF would
        // otherwise have us reserve 4 GiB on their say-so.
        if len > MAX_FRAME {
            return Err(FrameError::FrameTooLarge { announced: len });
        }

        // Header has arrived but the body has not. Reserve what we know we will
        // need — an optimisation, not a correctness requirement — and wait.
        if src.len() < HEADER + len {
            src.reserve(HEADER + len - src.len());
            return Ok(None);
        }

        // A complete frame. Consume the header, then split off exactly `len`
        // bytes; whatever remains in `src` is the start of the next frame and
        // must be left alone.
        src.advance(HEADER);
        let payload = src.split_to(len);

        String::from_utf8(payload.to_vec())
            .map(Some)
            .map_err(|_| FrameError::NotUtf8)
    }
}

impl Encoder<String> for LengthPrefixed {
    type Error = FrameError;

    fn encode(&mut self, item: String, dst: &mut BytesMut) -> Result<(), FrameError> {
        let bytes = item.as_bytes();

        // Refuse to send what we would refuse to receive. Producing frames a
        // conforming peer must reject is a bug on our side.
        if bytes.len() > MAX_FRAME {
            return Err(FrameError::FrameTooLarge {
                announced: bytes.len(),
            });
        }

        dst.reserve(HEADER + bytes.len());
        dst.put_u32(bytes.len() as u32); // big-endian by default
        dst.put_slice(bytes);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips one message through encode and decode.
    #[test]
    fn round_trips_a_single_frame() {
        let mut codec = LengthPrefixed;
        let mut buf = BytesMut::new();

        codec.encode("hello".to_string(), &mut buf).unwrap();
        assert_eq!(buf.len(), HEADER + 5);

        let decoded = codec.decode(&mut buf).unwrap();
        assert_eq!(decoded, Some("hello".to_string()));
        assert!(buf.is_empty(), "a complete frame must be fully consumed");
    }

    /// The core property: a decoder must tolerate being called with partial data.
    /// This is the test that would catch a decoder written as if `decode` were
    /// only ever handed whole frames.
    #[test]
    fn returns_none_until_the_whole_frame_arrives() {
        let mut codec = LengthPrefixed;
        let mut buf = BytesMut::new();

        // One byte of the header.
        buf.extend_from_slice(&[0]);
        assert_eq!(codec.decode(&mut buf).unwrap(), None);

        // Rest of the header: a 5-byte payload is coming.
        buf.extend_from_slice(&[0, 0, 5]);
        assert_eq!(codec.decode(&mut buf).unwrap(), None);

        // Part of the payload.
        buf.extend_from_slice(b"hel");
        assert_eq!(codec.decode(&mut buf).unwrap(), None);

        // The rest.
        buf.extend_from_slice(b"lo");
        assert_eq!(codec.decode(&mut buf).unwrap(), Some("hello".to_string()));
    }

    /// The other half of the framing problem: several messages in one read.
    #[test]
    fn decodes_several_frames_from_one_buffer() {
        let mut codec = LengthPrefixed;
        let mut buf = BytesMut::new();

        codec.encode("one".to_string(), &mut buf).unwrap();
        codec.encode("two".to_string(), &mut buf).unwrap();
        codec.encode("three".to_string(), &mut buf).unwrap();

        assert_eq!(codec.decode(&mut buf).unwrap(), Some("one".into()));
        assert_eq!(codec.decode(&mut buf).unwrap(), Some("two".into()));
        assert_eq!(codec.decode(&mut buf).unwrap(), Some("three".into()));
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
    }

    /// A trailing partial frame must survive the call untouched.
    #[test]
    fn leaves_a_trailing_partial_frame_in_the_buffer() {
        let mut codec = LengthPrefixed;
        let mut buf = BytesMut::new();

        codec.encode("complete".to_string(), &mut buf).unwrap();
        buf.extend_from_slice(&[0, 0, 0, 99]); // header promising 99 bytes
        buf.extend_from_slice(b"partial");

        assert_eq!(codec.decode(&mut buf).unwrap(), Some("complete".into()));
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        assert_eq!(buf.len(), HEADER + 7, "the partial frame must be preserved");
    }

    /// The security test. Without the bound in `decode`, this reserves 4 GiB.
    #[test]
    fn rejects_an_absurd_length_prefix() {
        let mut codec = LengthPrefixed;
        let mut buf = BytesMut::new();

        buf.extend_from_slice(&u32::MAX.to_be_bytes());

        let err = codec.decode(&mut buf).unwrap_err();
        assert!(
            matches!(err, FrameError::FrameTooLarge { .. }),
            "a hostile length prefix must be rejected, not honoured"
        );
    }

    #[test]
    fn refuses_to_encode_an_oversized_frame() {
        let mut codec = LengthPrefixed;
        let mut buf = BytesMut::new();

        let huge = "x".repeat(MAX_FRAME + 1);
        assert!(codec.encode(huge, &mut buf).is_err());
    }

    #[test]
    fn empty_frames_round_trip() {
        let mut codec = LengthPrefixed;
        let mut buf = BytesMut::new();

        codec.encode(String::new(), &mut buf).unwrap();
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(String::new()));
    }

    /// Length-prefix framing's advantage over delimiters: the payload may contain
    /// anything at all, including the byte a line protocol would frame on.
    #[test]
    fn payload_may_contain_newlines() {
        let mut codec = LengthPrefixed;
        let mut buf = BytesMut::new();

        let awkward = "line one\nline two\nline three".to_string();
        codec.encode(awkward.clone(), &mut buf).unwrap();

        assert_eq!(codec.decode(&mut buf).unwrap(), Some(awkward));
    }

    #[test]
    fn rejects_invalid_utf8() {
        let mut codec = LengthPrefixed;
        let mut buf = BytesMut::new();

        buf.extend_from_slice(&2u32.to_be_bytes());
        buf.extend_from_slice(&[0xff, 0xfe]);

        assert!(matches!(
            codec.decode(&mut buf).unwrap_err(),
            FrameError::NotUtf8
        ));
    }
}
