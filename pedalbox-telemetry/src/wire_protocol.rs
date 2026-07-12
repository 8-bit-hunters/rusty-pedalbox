//! The multiplexed serial transport frame: `[0xC0 0xFE][TYPE][LEN u16 LE][PAYLOAD]`.
//!
//! `TYPE` selects the stream on the shared USB pipe (`0x01` defmt log, `0x02` sensor).
//! The payload is opaque here; for sensor frames it is a postcard-encoded
//! [`crate::SensorSample`].

use crate::SensorSample;
#[cfg(feature = "std")]
use alloc::vec::Vec;

pub const MAX_SENSOR_PAYLOAD: usize = 16; //SensorSample is at most ~11 postcard bytes
// (u32 varint ≤5, channel 1, value = 1 tag + ≤4), so 16 is safe headroom.
pub const MAX_PAYLOAD: usize = 256;

/// A stateful accumulator that recovers whole frames from a byte stream via [`parse_frame`].
///
/// The backing [`Buffer`] is selected by the `std` feature: a fixed-capacity array by default
/// (allocation-free, drops its backlog on overflow) or a growable [`Vec`] under `std` (never
/// drops — for hosts that read in large chunks). A yielded [`Frame`] borrows the buffer, so it
/// must be dropped before the next `push`/`next_frame` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameReader {
    storage: Buffer,
    pending_consume: usize,
}

impl FrameReader {
    pub fn new() -> Self {
        Self {
            storage: Buffer::new(),
            pending_consume: 0,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        // Reclaim the previously yielded frame before appending. Its borrow has ended (this
        // takes `&mut self`), so dropping its bytes now keeps offsets valid and leaves no stale
        // `pending_consume` for a following overflow-reset to mis-apply to the new data.
        self.reclaim();
        self.storage.extend(bytes);
    }

    pub fn next_frame(&mut self) -> Option<Frame<'_>> {
        self.reclaim();
        self.advance_to_frame()?;
        self.take_frame_at_front()
    }

    fn reclaim(&mut self) {
        let consumed = core::mem::take(&mut self.pending_consume);
        self.storage.drop_front(consumed);
    }

    fn advance_to_frame(&mut self) -> Option<()> {
        loop {
            match parse_frame(self.storage.as_bytes()) {
                ParseOutcome::Frame { .. } => return Some(()),
                ParseOutcome::NeedMore => return None,
                ParseOutcome::Resync { skip } => self.storage.drop_front(skip),
            }
        }
    }

    fn take_frame_at_front(&mut self) -> Option<Frame<'_>> {
        // Borrowing `self.storage` (one field) leaves `self.pending_consume` (another) free to
        // write — the disjoint-field borrow that a `&self` helper would have blocked.
        match parse_frame(self.storage.as_bytes()) {
            ParseOutcome::Frame { frame, consumed } => {
                self.pending_consume = consumed;
                Some(frame)
            }
            _ => None,
        }
    }
}

impl Default for FrameReader {
    fn default() -> Self {
        Self::new()
    }
}

/// The byte buffer behind a [`FrameReader`]. Two implementations back the two feature modes;
/// [`FrameReader`] drives them through this trait so the framing logic stays single-source.
trait Storage {
    /// The buffered bytes, oldest first.
    fn as_bytes(&self) -> &[u8];
    /// Appends `bytes`. The fixed buffer drops its backlog (or an oversized push) on overflow;
    /// the growable buffer always keeps them.
    fn extend(&mut self, bytes: &[u8]);
    /// Removes the first `n` buffered bytes (clamped to what is buffered).
    fn drop_front(&mut self, n: usize);
}

#[cfg(not(feature = "std"))]
type Buffer = FixedBuffer;
#[cfg(feature = "std")]
type Buffer = GrowableBuffer;

/// Fixed-capacity, allocation-free buffer (default / no_std). Comfortably larger than one
/// maximal frame ([`Header::LEN`] + [`MAX_PAYLOAD`]); copes with overflow by dropping.
#[cfg(not(feature = "std"))]
const CAPACITY: usize = 512;

#[cfg(not(feature = "std"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FixedBuffer {
    buf: [u8; CAPACITY],
    filled: usize,
}

#[cfg(not(feature = "std"))]
impl FixedBuffer {
    fn new() -> Self {
        Self {
            buf: [0u8; CAPACITY],
            filled: 0,
        }
    }
}

#[cfg(not(feature = "std"))]
impl Storage for FixedBuffer {
    fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.filled]
    }

    fn extend(&mut self, bytes: &[u8]) {
        if self.filled + bytes.len() > CAPACITY {
            self.filled = 0; // overflow: drop the incomplete backlog to stay bounded
        }
        let Some(slot) = self.buf.get_mut(self.filled..self.filled + bytes.len()) else {
            return; // a single push larger than the buffer is dropped whole
        };
        slot.copy_from_slice(bytes);
        self.filled += bytes.len();
    }

    fn drop_front(&mut self, n: usize) {
        let n = n.min(self.filled);
        self.buf.copy_within(n..self.filled, 0);
        self.filled -= n;
    }
}

/// Growable, heap-backed buffer (`std` feature). Never drops: a large read is buffered whole.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GrowableBuffer {
    buf: Vec<u8>,
}

#[cfg(feature = "std")]
impl GrowableBuffer {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }
}

#[cfg(feature = "std")]
impl Storage for GrowableBuffer {
    fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    fn extend(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    fn drop_front(&mut self, n: usize) {
        self.buf.drain(..n.min(self.buf.len()));
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FrameType {
    Log = 0x01,
    Telemetry = 0x02,
}

impl TryFrom<u8> for FrameType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(FrameType::Log),
            0x02 => Ok(FrameType::Telemetry),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Frame<'a> {
    pub frame_type: FrameType,
    pub payload: &'a [u8],
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ParseOutcome<'a> {
    Frame { frame: Frame<'a>, consumed: usize },
    NeedMore,
    Resync { skip: usize },
}

pub fn parse_frame(buf: &[u8]) -> ParseOutcome<'_> {
    let Some(header_bytes) = buf.first_chunk::<{ Header::LEN }>() else {
        return ParseOutcome::NeedMore;
    };

    match find_magic(buf) {
        Some(0) => {}
        Some(index) => return ParseOutcome::Resync { skip: index },
        None => {
            return ParseOutcome::Resync {
                skip: buf.len() - 1,
            };
        }
    }

    let Some(header) = Header::from_bytes(header_bytes) else {
        return ParseOutcome::Resync { skip: 1 };
    };
    let Some(payload) = buf.get(Header::LEN..Header::LEN + header.payload_length) else {
        return ParseOutcome::NeedMore;
    };

    ParseOutcome::Frame {
        frame: Frame {
            frame_type: header.frame_type,
            payload,
        },
        consumed: Header::LEN + header.payload_length,
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Header {
    frame_type: FrameType,
    payload_length: usize,
}

impl Header {
    pub const LEN: usize = 5; // 2 magic + 1 type + 2 len
    const MAGIC_BYTES: [u8; 2] = [0xC0, 0xFE];

    fn from_bytes(bytes: &[u8; Self::LEN]) -> Option<Self> {
        let frame_type = FrameType::try_from(bytes[2]).ok()?;
        let payload_length = u16::from_le_bytes([bytes[3], bytes[4]]) as usize;
        if payload_length > MAX_PAYLOAD {
            return None;
        }

        Some(Self {
            frame_type,
            payload_length,
        })
    }
}

fn find_magic(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == Header::MAGIC_BYTES)
}

pub fn write_frame(frame_type: FrameType, payload: &[u8], out: &mut [u8]) -> Option<usize> {
    let size_to_write = Header::LEN + payload.len();
    if size_to_write > out.len() {
        return None;
    }
    out[0..Header::LEN].copy_from_slice(&frame_header(frame_type, payload.len()));
    out[Header::LEN..size_to_write].copy_from_slice(payload);

    Some(size_to_write)
}

pub fn write_sensor_frame(sample: &SensorSample, out: &mut [u8]) -> Option<usize> {
    let mut payload = [0u8; MAX_SENSOR_PAYLOAD];
    let encode = postcard::to_slice(sample, &mut payload).ok()?;
    write_frame(FrameType::Telemetry, encode, out)
}

pub fn frame_header(frame_type: FrameType, payload_length: usize) -> [u8; Header::LEN] {
    let payload_len = (payload_length as u16).to_le_bytes();
    [
        Header::MAGIC_BYTES[0],
        Header::MAGIC_BYTES[1],
        frame_type as u8,
        payload_len[0],
        payload_len[1],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    mod test_write_frame {
        use super::*;

        #[test]
        fn when_writing_a_frame() {
            // Given
            let payload = [0xDE, 0xAD, 0xBE, 0xEF];
            let mut out = [0u8; 16];
            let frame_type = FrameType::Telemetry;

            // When
            let written = write_frame(frame_type, &payload, &mut out);

            // Then
            assert_eq!(
                written,
                Some(Header::LEN + payload.len()),
                "should return the total number of bytes written (header + payload)"
            );
            let len = written.unwrap();
            assert_eq!(
                &out[..len],
                &[0xC0, 0xFE, 0x02, 0x04, 0x00, 0xDE, 0xAD, 0xBE, 0xEF],
                "frame should be [magic][type][len LE][payload]"
            );
        }

        #[test]
        fn when_out_buffer_is_too_small() {
            // Given
            let payload = [0xDE, 0xAD, 0xBE, 0xEF];
            let mut out = [0u8; 8]; // one short of Header::LEN(5) + 4

            // When
            let written = write_frame(FrameType::Telemetry, &payload, &mut out);

            // Then
            assert_eq!(written, None, "Too-small out buffer should return None");
        }

        #[test]
        fn when_payload_is_empty() {
            // Given
            let payload: [u8; 0] = [];
            let mut out = [0u8; 16];

            // When
            let written = write_frame(FrameType::Telemetry, &payload, &mut out);

            // Then
            assert_eq!(
                written,
                Some(Header::LEN),
                "empty payload should yield a header-only frame"
            );
            assert_eq!(
                &out[..Header::LEN],
                &[0xC0, 0xFE, 0x02, 0x00, 0x00],
                "header should carry a LEN field of 0"
            );
        }

        #[test]
        fn when_payload_is_longer_than_255_bytes() {
            // Given
            let payload = [0xABu8; 300];
            let mut out = [0u8; 512];

            // When
            let written = write_frame(FrameType::Telemetry, &payload, &mut out);

            // Then
            assert_eq!(written, Some(Header::LEN + payload.len()));
            let len = written.unwrap();
            assert_eq!(
                &out[3..5],
                &[0x2C, 0x01],
                "Should contain the right length headers"
            );
            assert_eq!(
                &out[Header::LEN..len],
                &payload,
                "payload should be copied verbatim"
            );
        }
    }

    mod test_write_sensor_frame {
        use super::*;
        use crate::Value;

        #[test]
        fn when_writing_a_sensor_frame() {
            // Given
            let sample = SensorSample {
                timestamp_ms: 1_000,
                channel_id: 3,
                value: Value::U16(2048),
            };
            let mut out = [0u8; 64];

            // When
            let written = write_sensor_frame(&sample, &mut out);

            // Then
            let len = written.expect("frame should fit");
            assert_eq!(
                &out[0..3],
                &[0xC0, 0xFE, 0x02],
                "frame should start with [magic][telemetry type]"
            );
            let payload = &out[Header::LEN..len];
            let decoded: SensorSample =
                postcard::from_bytes(payload).expect("payload should be a valid SensorSample");
            assert_eq!(
                decoded, sample,
                "framed payload should deserialize back to the original sample"
            );
        }

        #[test]
        fn when_out_buffer_is_too_small() {
            // Given
            let sample = SensorSample {
                timestamp_ms: 1_000,
                channel_id: 3,
                value: Value::U16(2048),
            };
            let mut out = [0u8; 4]; // smaller than even the frame header

            // When
            let written = write_sensor_frame(&sample, &mut out);

            // Then
            assert_eq!(written, None, "too-small out buffer should return None");
        }

        #[test]
        fn when_sample_is_maximal() {
            // Given
            let sample = SensorSample {
                timestamp_ms: u32::MAX,
                channel_id: u8::MAX,
                value: Value::U32(u32::MAX),
            };
            let mut out = [0u8; 64];

            // When
            let written = write_sensor_frame(&sample, &mut out);

            // Then
            let len = written.expect("maximal sample should still fit MAX_SENSOR_PAYLOAD");
            assert!(
                len - Header::LEN <= MAX_SENSOR_PAYLOAD,
                "serialized payload ({} bytes) should not exceed MAX_SENSOR_PAYLOAD ({})",
                len - Header::LEN,
                MAX_SENSOR_PAYLOAD
            );
            let decoded: SensorSample = postcard::from_bytes(&out[Header::LEN..len])
                .expect("payload should be a valid SensorSample");
            assert_eq!(decoded, sample, "maximal sample should round-trip");
        }
    }

    mod test_parse_frame {
        use super::*;
        use crate::Value;

        #[test]
        fn when_the_buffer_is_empty() {
            // Given
            let frame: [u8; 0] = [];

            // When
            let outcome = parse_frame(&frame);

            // Then
            assert!(
                matches!(outcome, ParseOutcome::NeedMore),
                "an empty buffer should not yield a frame yet"
            );
        }

        #[test]
        fn when_only_a_partial_header_is_present() {
            // Given fewer than Header::LEN(5) bytes have arrived
            let buf = [
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
            ];

            // When
            let outcome = parse_frame(&buf);

            // Then
            assert!(
                matches!(outcome, ParseOutcome::NeedMore),
                "a header that is not yet complete should not be parsed as a frame"
            );
        }

        #[test]
        fn when_the_payload_is_incomplete() {
            // Given
            let buf = [
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
                0x04,
                0x00,
                0xDE,
                0xAD,
            ];

            // When
            let outcome = parse_frame(&buf);

            // Then
            assert!(
                matches!(outcome, ParseOutcome::NeedMore),
                "a frame missing part of its payload should not be parsed yet"
            );
        }

        #[test]
        fn when_a_complete_telemetry_frame_is_parsed() {
            // Given
            let buf = [
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
                0x04,
                0x00,
                0xDE,
                0xAD,
                0xBE,
                0xEF,
            ];

            // When
            let outcome = parse_frame(&buf);

            // Then
            assert_eq!(
                outcome,
                ParseOutcome::Frame {
                    frame: Frame {
                        frame_type: FrameType::Telemetry,
                        payload: &[0xDE, 0xAD, 0xBE, 0xEF],
                    },
                    consumed: Header::LEN + 4,
                },
                "a complete telemetry frame should parse into its payload and report the bytes consumed"
            );
        }

        #[test]
        fn when_a_complete_log_frame_is_parsed() {
            // Given
            let buf = [
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Log as u8,
                0x03,
                0x00,
                0x11,
                0x22,
                0x33,
            ];

            // When
            let outcome = parse_frame(&buf);

            // Then
            assert_eq!(
                outcome,
                ParseOutcome::Frame {
                    frame: Frame {
                        frame_type: FrameType::Log,
                        payload: &[0x11, 0x22, 0x33],
                    },
                    consumed: Header::LEN + 3,
                },
                "the type byte should select the Log stream"
            );
        }

        #[test]
        fn when_two_frames_are_back_to_back() {
            // Given
            let buf = [
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
                0x02,
                0x00,
                0xAA,
                0xBB, // frame #1: telemetry, LEN=2
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Log as u8,
                0x01,
                0x00,
                0xCC, // frame #2: log, LEN=1
            ];

            // When
            let outcome = parse_frame(&buf);

            // Then
            assert_eq!(
                outcome,
                ParseOutcome::Frame {
                    frame: Frame {
                        frame_type: FrameType::Telemetry,
                        payload: &[0xAA, 0xBB],
                    },
                    consumed: Header::LEN + 2,
                },
                "consumed should cover only the first frame, not the trailing bytes"
            );
        }

        #[test]
        fn when_leading_garbage_precedes_the_magic() {
            // Given
            let buf = [
                0x11,
                0x22,
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
                0x01,
                0x00,
                0xAA,
            ];

            // When
            let outcome = parse_frame(&buf);

            // Then
            assert_eq!(
                outcome,
                ParseOutcome::Resync { skip: 2 },
                "leading garbage should be skipped up to the next magic, not parsed as a header"
            );
        }

        #[test]
        fn when_the_magic_is_further_than_a_header_length_into_the_buffer() {
            // Given: more than Header::LEN(5) junk bytes (none of them a magic) before
            // the real frame, so the magic sits past the first header-sized window
            let buf = [
                0x11,
                0x22,
                0x33,
                0x44,
                0x55,
                0x66,
                0x77, // 7 junk bytes
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
                0x01,
                0x00,
                0xAA, // valid frame at index 7
            ];

            // When
            let outcome = parse_frame(&buf);

            // Then
            assert_eq!(
                outcome,
                ParseOutcome::Resync { skip: 7 },
                "the magic search should cover the whole buffer, not only the first header window"
            );
        }

        #[test]
        fn when_there_is_no_magic_anywhere() {
            // Given
            let buf = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];

            // When
            let outcome = parse_frame(&buf);

            // Then
            assert_eq!(
                outcome,
                ParseOutcome::Resync {
                    skip: buf.len() - 1
                },
                "all but the final byte should be discarded"
            );
        }

        #[test]
        fn when_the_buffer_ends_mid_magic() {
            // Given
            let buf = [0x11, 0x22, 0x33, 0x44, Header::MAGIC_BYTES[0]];

            // When
            let outcome = parse_frame(&buf);

            // Then
            assert_eq!(
                outcome,
                ParseOutcome::Resync {
                    skip: buf.len() - 1
                },
                "a trailing lone 0xC0 should be kept as a possible partial magic, not discarded"
            );
        }

        #[test]
        fn when_the_magic_is_followed_by_an_unknown_type() {
            // Given
            let buf = [
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                0x7F,
                0x00,
                0x00,
            ];

            // When
            let outcome = parse_frame(&buf);

            // Then
            assert_eq!(
                outcome,
                ParseOutcome::Resync { skip: 1 },
                "an unknown type byte should be treated as a false magic, not parsed"
            );
        }

        #[test]
        fn when_the_length_exceeds_the_maximum() {
            // Given
            let buf = [
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
                0x2C,
                0x01,
            ]; // 0x012C = 300 is larger
            // than any real payload

            // When
            let outcome = parse_frame(&buf);

            // Then
            assert_eq!(
                outcome,
                ParseOutcome::Resync { skip: 1 },
                "an implausibly large LEN should be treated as a false magic, not NeedMore"
            );
        }

        #[test]
        fn when_a_large_log_frame_is_parsed() {
            // Given
            let payload = [0xABu8; 200];
            let mut buf = [0u8; Header::LEN + 200];
            let len =
                write_frame(FrameType::Log, &payload, &mut buf).expect("failed to write frame");

            // When
            let outcome = parse_frame(&buf[..len]);

            // Then
            assert_eq!(
                outcome,
                ParseOutcome::Frame {
                    frame: Frame {
                        frame_type: FrameType::Log,
                        payload: &payload,
                    },
                    consumed: Header::LEN + 200,
                },
                "a log payload larger than a sensor payload should still parse"
            );
        }

        #[test]
        fn when_parsing_a_frame_written_by_write_sensor_frame() {
            // Given
            let sample = SensorSample {
                timestamp_ms: 1_000,
                channel_id: 3,
                value: Value::I16(-42),
            };
            let mut buf = [0u8; 64];
            let len = write_sensor_frame(&sample, &mut buf).unwrap();

            // When
            let outcome = parse_frame(&buf[..len]);

            // Then
            match outcome {
                ParseOutcome::Frame { frame, consumed } => {
                    assert_eq!(
                        frame.frame_type,
                        FrameType::Telemetry,
                        "a sensor frame should decode as Telemetry"
                    );
                    assert_eq!(consumed, len, "consumed should cover the whole frame");
                    let decoded: SensorSample = postcard::from_bytes(frame.payload)
                        .expect("payload should be a valid SensorSample");
                    assert_eq!(
                        decoded, sample,
                        "the round-tripped sample should equal the original"
                    );
                }
                other => panic!("expected a parsed frame, got {other:?}"),
            }
        }
    }

    // ------------------------------------------------------------------
    // FrameReader — stateful accumulator over parse_frame (no_std, fixed buffer).
    //   push(&[u8]); repeatedly next() -> Option<Frame> draining complete frames.
    // ------------------------------------------------------------------
    mod test_frame_reader {
        use super::*;

        #[test]
        fn when_a_frame_arrives_split_across_two_pushes() {
            // Given
            let mut reader = FrameReader::new();

            // When
            reader.push(&[
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
                0x04,
                0x00,
                0xDE,
            ]);

            // Then
            assert!(
                reader.next_frame().is_none(),
                "a partial frame should not be yielded until the rest arrives"
            );

            // When
            reader.push(&[0xAD, 0xBE, 0xEF]);

            // Then
            assert_eq!(
                reader.next_frame(),
                Some(Frame {
                    frame_type: FrameType::Telemetry,
                    payload: &[0xDE, 0xAD, 0xBE, 0xEF],
                }),
                "once the payload completes, the whole frame should be returned"
            );
            assert!(
                reader.next_frame().is_none(),
                "there should not be a second frame in the buffer"
            );
        }

        #[test]
        fn when_two_frames_are_pushed_at_once() {
            // Given
            let mut reader = FrameReader::new();
            reader.push(&[
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
                0x02,
                0x00,
                0xAA,
                0xBB, // frame A: telemetry, LEN=2
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Log as u8,
                0x01,
                0x00,
                0xCC, // frame B: log, LEN=1
            ]);

            // When / Then
            assert_eq!(
                reader.next_frame(),
                Some(Frame {
                    frame_type: FrameType::Telemetry,
                    payload: &[0xAA, 0xBB],
                }),
                "the first frame should be drained first"
            );

            // When / Then
            assert_eq!(
                reader.next_frame(),
                Some(Frame {
                    frame_type: FrameType::Log,
                    payload: &[0xCC],
                }),
                "the second frame should be drained after the first is reclaimed"
            );

            // When / Then
            assert!(
                reader.next_frame().is_none(),
                "the buffer should be empty after both frames are drained"
            );
        }

        #[test]
        fn when_garbage_precedes_a_valid_frame() {
            // Given
            let mut reader = FrameReader::new();
            reader.push(&[
                0x11,
                0x22,
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
                0x01,
                0x00,
                0xAA,
            ]);

            // When / Then
            assert_eq!(
                reader.next_frame(),
                Some(Frame {
                    frame_type: FrameType::Telemetry,
                    payload: &[0xAA],
                }),
                "the reader should skip leading garbage and return the following frame"
            );
            assert!(
                reader.next_frame().is_none(),
                "nothing should remain after the frame is drained"
            );
        }

        // The fixed-capacity reader (default, no_std) copes with overflow by dropping. Under
        // the `std` feature the buffer grows instead, so these two drop-on-overflow contracts
        // apply only without it — see `when_a_read_exceeds_the_fixed_capacity` for the std side.
        #[cfg(not(feature = "std"))]
        #[test]
        fn when_the_buffer_fills_without_a_complete_frame() {
            // Given
            let mut reader = FrameReader::new();
            let garbage = [0x00u8; CAPACITY];

            // When
            reader.push(&garbage);
            reader.push(&garbage);

            // Then
            reader.push(&[
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
                0x01,
                0x00,
                0xAA,
            ]);
            assert_eq!(
                reader.next_frame(),
                Some(Frame {
                    frame_type: FrameType::Telemetry,
                    payload: &[0xAA],
                }),
                "after overflowing with garbage the reader should reset and still parse a later frame"
            );
        }

        #[cfg(not(feature = "std"))]
        #[test]
        fn when_a_single_push_exceeds_capacity() {
            // Given
            let mut reader = FrameReader::new();
            let oversized = [0x00u8; CAPACITY + 1];

            // When
            reader.push(&oversized);

            // Then
            reader.push(&[
                Header::MAGIC_BYTES[0],
                Header::MAGIC_BYTES[1],
                FrameType::Telemetry as u8,
                0x01,
                0x00,
                0xAA,
            ]);
            assert_eq!(
                reader.next_frame(),
                Some(Frame {
                    frame_type: FrameType::Telemetry,
                    payload: &[0xAA],
                }),
                "a push larger than the buffer should be dropped, leaving the reader usable"
            );
        }

        // With the `std` feature the buffer grows instead of dropping: a single read far larger
        // than the fixed no_std capacity (hosts read in multi-KiB chunks) must keep every frame.
        #[cfg(feature = "std")]
        #[test]
        fn when_a_read_exceeds_the_fixed_capacity() {
            // Given
            const FRAME_LEN: usize = Header::LEN + 1; // telemetry frame, 1-byte payload
            const FRAME_COUNT: usize = 200; // 200 * 6 = 1200 bytes
            let mut stream = [0u8; FRAME_LEN * FRAME_COUNT];
            for chunk in stream.chunks_mut(FRAME_LEN) {
                write_frame(FrameType::Telemetry, &[0xAA], chunk).expect("failed to build frame");
            }

            let mut reader = FrameReader::new();

            // When
            reader.push(&stream);

            // Then
            let mut drained = 0;
            while reader.next_frame().is_some() {
                drained += 1;
            }
            assert_eq!(
                drained, FRAME_COUNT,
                "with the std feature a read larger than the fixed capacity must not drop frames"
            );
        }
    }
}
