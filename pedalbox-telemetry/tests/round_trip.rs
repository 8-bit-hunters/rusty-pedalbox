//! End-to-end wire round-trips exercised through the *public* API only, the way a host
//! consumes the stream: the firmware side encodes with [`write_sensor_frame`], the host side
//! accumulates bytes in a [`FrameReader`] and decodes each payload back into a [`SensorSample`]
//! via `SensorSample::try_from`.
//!
//! Unit tests in `wire_protocol` already cover `parse_frame` and `FrameReader` piecewise; these
//! validate the whole seam end-to-end through the stateful reader (the host's real entry point),
//! under whichever buffer the active feature set selects (fixed vs. `std` growable).

use pedalbox_telemetry::wire_protocol::{FrameReader, FrameType, write_sensor_frame};
use pedalbox_telemetry::{SensorSample, Value};

/// Encodes `sample` into a fresh scratch buffer, returning the framed bytes.
fn frame(sample: &SensorSample) -> Vec<u8> {
    let mut buf = [0u8; 64];
    let len = write_sensor_frame(sample, &mut buf).expect("sample should encode");
    buf[..len].to_vec()
}

#[test]
fn a_sensor_frame_survives_the_reader_round_trip() {
    // Given a telemetry sample framed the way the firmware sends it
    let sample = SensorSample {
        timestamp_ms: 1_234,
        channel_id: 3,
        value: Value::I16(-42),
    };
    let bytes = frame(&sample);

    // When the host feeds the bytes through its reader
    let mut reader = FrameReader::new();
    reader.push(&bytes);
    let decoded = reader
        .next_frame()
        .expect("a complete frame should be available");

    // Then it is a telemetry frame whose payload decodes back to the original sample
    assert_eq!(decoded.frame_type, FrameType::Telemetry);
    let sample_back = SensorSample::try_from(decoded.payload).expect("payload should decode");
    assert_eq!(sample_back, sample);
    assert!(reader.next_frame().is_none(), "only one frame was pushed");
}

#[test]
fn two_back_to_back_frames_each_decode() {
    // Given two samples concatenated into a single byte stream
    let first = SensorSample {
        timestamp_ms: 10,
        channel_id: 0,
        value: Value::U16(500),
    };
    let second = SensorSample {
        timestamp_ms: 20,
        channel_id: 1,
        value: Value::I32(-100_000),
    };
    let mut bytes = frame(&first);
    bytes.extend_from_slice(&frame(&second));

    let mut reader = FrameReader::new();
    reader.push(&bytes);

    // When each frame is drained in turn, it decodes to the matching sample.
    // (Decode immediately — the borrowed payload must be consumed before the next call.)
    let f = reader.next_frame().expect("first frame");
    assert_eq!(
        SensorSample::try_from(f.payload).expect("first decodes"),
        first
    );

    let s = reader.next_frame().expect("second frame");
    assert_eq!(
        SensorSample::try_from(s.payload).expect("second decodes"),
        second
    );

    assert!(
        reader.next_frame().is_none(),
        "exactly two frames were pushed"
    );
}

#[test]
fn a_frame_split_across_two_pushes_reassembles() {
    // Given a framed sample cut in half mid-frame
    let sample = SensorSample {
        timestamp_ms: 7,
        channel_id: 5,
        value: Value::F32(3.5),
    };
    let bytes = frame(&sample);
    let split = bytes.len() / 2;

    let mut reader = FrameReader::new();

    // When only the first half has arrived, no frame is available yet
    reader.push(&bytes[..split]);
    assert!(reader.next_frame().is_none(), "frame is still incomplete");

    // And once the remainder arrives, the reassembled frame decodes to the original
    reader.push(&bytes[split..]);
    let decoded = reader
        .next_frame()
        .expect("frame completes after second push");
    assert_eq!(
        SensorSample::try_from(decoded.payload).expect("payload should decode"),
        sample
    );
}
