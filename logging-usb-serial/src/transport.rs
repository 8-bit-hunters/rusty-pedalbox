//! Turns buffered defmt bytes and sensor samples into framed USB packets, written through
//! an abstract [`PacketSink`]. Keeping the sink abstract lets the framing/chunking logic be
//! unit-tested off-target with a fake sink, leaving only the embassy scheduling and the real
//! USB `Sender` implementation for on-device verification.

use crate::controller::Controller;
use crate::framing;
use crate::framing::{FrameType, HEADER_LEN, MAX_SENSOR_PAYLOAD, frame_header, write_sensor_frame};
use core::cmp::max;
use pedalbox_telemetry::SensorSample;

pub trait PacketSink {
    type Error;
    async fn write_packet(&mut self, data: &[u8]) -> Result<(), Self::Error>;
    fn max_packet_size(&self) -> usize;
}

pub async fn send_sensor_frame<S: PacketSink>(
    sample: &SensorSample,
    sink: &mut S,
) -> Result<(), S::Error> {
    let mut frame = [0u8; HEADER_LEN + MAX_SENSOR_PAYLOAD];
    if let Some(length) = write_sensor_frame(sample, &mut frame) {
        sink.write_packet(&frame[..length]).await?;
    }
    Ok(())
}

pub async fn flush_defmt<S: PacketSink>(
    controller: &Controller,
    sink: &mut S,
) -> Result<(), S::Error> {
    let max = sink.max_packet_size();
    controller
        .flush(async |bytes| {
            sink.write_packet(&frame_header(FrameType::Log, bytes.len()))
                .await?;
            let mut last_full = false;
            for chunk in bytes.chunks(max) {
                last_full = chunk.len() == max;
                sink.write_packet(chunk).await?;
            }
            if last_full {
                sink.write_packet(&[]).await?;
            }
            Ok(())
        })
        .await
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::framing::HEADER_LEN;
    use pedalbox_telemetry::Value;
    use std::vec::Vec;

    /// A [`PacketSink`] that records every packet it is asked to write, and can be told to
    /// fail so error propagation can be exercised.
    struct FakeSink {
        packets: Vec<Vec<u8>>,
        max_packet_size: usize,
        fail: bool,
    }

    impl FakeSink {
        fn new(max_packet_size: usize) -> Self {
            Self {
                packets: Vec::new(),
                max_packet_size,
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                fail: true,
                ..Self::new(64)
            }
        }
    }

    impl PacketSink for FakeSink {
        type Error = ();

        async fn write_packet(&mut self, data: &[u8]) -> Result<(), Self::Error> {
            if self.fail {
                return Err(());
            }
            self.packets.push(data.to_vec());
            Ok(())
        }

        fn max_packet_size(&self) -> usize {
            self.max_packet_size
        }
    }

    mod test_send_sensor_frame {
        use super::*;

        #[test]
        fn when_sending_a_sensor_frame() {
            // Given
            let sample = SensorSample {
                timestamp_ms: 1_000,
                channel_id: 3,
                value: Value::U16(2048),
            };
            let mut sink = FakeSink::new(64);

            // When
            let result = pollster::block_on(send_sensor_frame(&sample, &mut sink));

            // Then
            assert!(result.is_ok(), "sending should succeed");
            assert_eq!(sink.packets.len(), 1, "one sensor frame is one packet");
            let packet = &sink.packets[0];
            assert_eq!(
                &packet[0..3],
                &[0xC0, 0xFE, 0x02],
                "packet is a telemetry frame"
            );
            let decoded: SensorSample = postcard::from_bytes(&packet[HEADER_LEN..])
                .expect("payload should be a valid SensorSample");
            assert_eq!(
                decoded, sample,
                "payload round-trips to the original sample"
            );
        }

        #[test]
        fn when_the_sink_returns_an_error() {
            // Given
            let sample = SensorSample {
                timestamp_ms: 1_000,
                channel_id: 3,
                value: Value::U16(2048),
            };
            let mut sink = FakeSink::failing();

            // When
            let result = pollster::block_on(send_sensor_frame(&sample, &mut sink));

            // Then
            assert_eq!(result, Err(()), "sink error should propagate to the caller");
            assert!(
                sink.packets.is_empty(),
                "no packet should be recorded when the sink fails"
            );
        }
    }

    mod test_flush_defmt {
        use super::*;

        #[test]
        fn when_there_is_no_flushing_buffer() {
            // Given
            let controller = Controller::new();
            let mut sink = FakeSink::new(4);

            // When
            let result = pollster::block_on(flush_defmt(&controller, &mut sink));

            // Then
            assert!(result.is_ok(), "flushing nothing should succeed");
            assert!(
                sink.packets.is_empty(),
                "with no flushing buffer, no packets should be written"
            );
        }

        #[test]
        fn when_flushing_a_small_buffer() {
            // Given: a flushing buffer holding fewer bytes than one packet
            let data = [0x11, 0x22, 0x33];
            let controller = Controller::new();
            controller.write(&data);
            controller.swap_buffers(); // mark the active buffer flushing
            let mut sink = FakeSink::new(4);

            // When
            let result = pollster::block_on(flush_defmt(&controller, &mut sink));

            // Then
            assert!(result.is_ok(), "flushing should succeed");
            assert_eq!(
                sink.packets.len(),
                2,
                "one header packet followed by one payload packet"
            );
            assert_eq!(
                sink.packets[0],
                [0xC0, 0xFE, 0x01, 0x03, 0x00],
                "first packet is the 0x01 frame header with LEN=3 (u16 LE)"
            );
            assert_eq!(sink.packets[1], data, "second packet carries the payload");
        }

        #[test]
        fn when_the_buffer_spans_multiple_packets() {
            // Given: a flushing buffer larger than one packet, not an exact multiple
            let data: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
            let controller = Controller::new();
            controller.write(&data);
            controller.swap_buffers();
            let mut sink = FakeSink::new(4);

            // When
            let result = pollster::block_on(flush_defmt(&controller, &mut sink));

            // Then
            assert!(result.is_ok(), "flushing should succeed");
            assert_eq!(
                sink.packets.len(),
                4,
                "header + three payload chunks (4 + 4 + 2)"
            );
            assert_eq!(
                sink.packets[0],
                [0xC0, 0xFE, 0x01, 0x0A, 0x00],
                "header carries LEN=10 (u16 LE)"
            );
            assert_eq!(sink.packets[1], [1, 2, 3, 4]);
            assert_eq!(sink.packets[2], [5, 6, 7, 8]);
            assert_eq!(
                sink.packets[3], [9, 10],
                "last chunk is short, so no trailing zero-length packet"
            );
        }

        #[test]
        fn when_the_last_chunk_fills_a_packet() {
            // Given: a flushing buffer that is an exact multiple of the packet size
            let data: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
            let controller = Controller::new();
            controller.write(&data);
            controller.swap_buffers();
            let mut sink = FakeSink::new(4);

            // When
            let result = pollster::block_on(flush_defmt(&controller, &mut sink));

            // Then
            assert!(result.is_ok(), "flushing should succeed");
            assert_eq!(
                sink.packets.len(),
                4,
                "header + two full chunks + a trailing zero-length packet"
            );
            assert_eq!(
                sink.packets[0],
                [0xC0, 0xFE, 0x01, 0x08, 0x00],
                "header carries LEN=8 (u16 LE)"
            );
            assert_eq!(sink.packets[1], [1, 2, 3, 4]);
            assert_eq!(sink.packets[2], [5, 6, 7, 8]);
            assert!(
                sink.packets[3].is_empty(),
                "a full final chunk is followed by a zero-length packet to end the transfer"
            );
        }
    }
}
