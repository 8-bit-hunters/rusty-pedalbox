//! The multiplexed serial transport frame: `[0xB5 0x62][TYPE][LEN u16 LE][PAYLOAD]`.
//!
//! `TYPE` selects the stream on the shared USB pipe (`0x01` defmt log, `0x02` sensor).
//! The payload is opaque here; for sensor frames it is a postcard-encoded
//! [`pedalbox_telemetry::SensorSample`].

use pedalbox_telemetry::SensorSample;

pub const HEADER_LEN: usize = 5; // 2 magic + 1 type + 2 len
pub const MAX_SENSOR_PAYLOAD: usize = 16; //SensorSample is at most ~11 postcard bytes
// (u32 varint ≤5, channel 1, value = 1 tag + ≤4), so 16 is safe headroom.

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FrameType {
    Log = 0x01,
    Telemetry = 0x02,
}

pub fn write_frame(frame_type: FrameType, payload: &[u8], out: &mut [u8]) -> Option<usize> {
    let size_to_write = HEADER_LEN + payload.len();
    if size_to_write > out.len() {
        return None;
    }
    let payload_len = (payload.len() as u16).to_le_bytes();
    let header: [u8; HEADER_LEN] = [0xB5, 0x62, frame_type as u8, payload_len[0], payload_len[1]];
    out[0..HEADER_LEN].copy_from_slice(&header);
    out[HEADER_LEN..size_to_write].copy_from_slice(payload);

    Some(size_to_write)
}

pub fn write_sensor_frame(sample: &SensorSample, out: &mut [u8]) -> Option<usize> {
    let mut payload = [0u8; MAX_SENSOR_PAYLOAD];
    let encode = postcard::to_slice(sample, &mut payload).ok()?;
    write_frame(FrameType::Telemetry, encode, out)
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
                Some(HEADER_LEN + payload.len()),
                "should return the total number of bytes written (header + payload)"
            );
            let len = written.unwrap();
            assert_eq!(
                &out[..len],
                &[0xB5, 0x62, 0x02, 0x04, 0x00, 0xDE, 0xAD, 0xBE, 0xEF],
                "frame should be [magic][type][len LE][payload]"
            );
        }

        #[test]
        fn when_out_buffer_is_too_small() {
            // Given
            let payload = [0xDE, 0xAD, 0xBE, 0xEF];
            let mut out = [0u8; 8]; // one short of HEADER_LEN(5) + 4

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
                Some(HEADER_LEN),
                "empty payload should yield a header-only frame"
            );
            assert_eq!(
                &out[..HEADER_LEN],
                &[0xB5, 0x62, 0x02, 0x00, 0x00],
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
            assert_eq!(written, Some(HEADER_LEN + payload.len()));
            let len = written.unwrap();
            assert_eq!(
                &out[3..5],
                &[0x2C, 0x01],
                "Should contain the right length headers"
            );
            assert_eq!(&out[HEADER_LEN..len], &payload, "payload copied verbatim");
        }
    }

    mod test_write_sensor_frame {
        use super::*;
        use pedalbox_telemetry::Value;

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
                &[0xB5, 0x62, 0x02],
                "frame should start with [magic][telemetry type]"
            );
            let payload = &out[HEADER_LEN..len];
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
            // Given: the largest a SensorSample can postcard-encode to
            let sample = SensorSample {
                timestamp_ms: u32::MAX,
                channel_id: u8::MAX,
                value: Value::U32(u32::MAX),
            };
            let mut out = [0u8; 64];

            // When
            let written = write_sensor_frame(&sample, &mut out);

            // Then: it fits the scratch buffer, so write_sensor_frame never panics
            let len = written.expect("maximal sample must still fit MAX_SENSOR_PAYLOAD");
            assert!(
                len - HEADER_LEN <= MAX_SENSOR_PAYLOAD,
                "serialized payload ({} bytes) must not exceed MAX_SENSOR_PAYLOAD ({})",
                len - HEADER_LEN,
                MAX_SENSOR_PAYLOAD
            );
            let decoded: SensorSample = postcard::from_bytes(&out[HEADER_LEN..len])
                .expect("payload should be a valid SensorSample");
            assert_eq!(decoded, sample, "maximal sample should round-trip");
        }
    }
}
