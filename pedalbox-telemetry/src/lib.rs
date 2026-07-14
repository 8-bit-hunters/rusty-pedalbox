#![no_std]
//! Shared wire types for the pedalbox telemetry link.
//!
//! This crate is the single source of truth for everything that travels over the composite
//! USB-CDC pipe between the firmware and the host [`log-sniffer`]. Both ends depend on it so
//! that the frame layout and the sensor-sample encoding can never drift apart.
//!
//! Two concerns live here:
//!
//! - **Data model** — [`SensorSample`] and [`Value`], the telemetry payload, serialized with
//!   [`postcard`](https://docs.rs/postcard).
//! - **Framing** — the [`wire_protocol`] module, which multiplexes defmt logs and telemetry
//!   onto one byte stream and recovers frame boundaries on the receiving side.
//!
//! The crate is `#![no_std]` so the firmware can use it directly; the host links it as an
//! ordinary `std` dependency.
//!
//! # Example
//!
//! Round-trip a frame through the encoder and the parser:
//!
//! ```
//! use pedalbox_telemetry::wire_protocol::{write_frame, parse_frame, Frame, FrameType, ParseOutcome};
//!
//! // Firmware side: wrap a defmt-log payload in a frame.
//! let mut buf = [0u8; 16];
//! let len = write_frame(FrameType::Log, &[0x11, 0x22, 0x33], &mut buf).unwrap();
//!
//! // Host side: recover it from the byte stream.
//! assert_eq!(
//!     parse_frame(&buf[..len]),
//!     ParseOutcome::Frame {
//!         frame: Frame { frame_type: FrameType::Log, payload: &[0x11, 0x22, 0x33] },
//!         consumed: len,
//!     },
//! );
//! ```
//!
//! [`log-sniffer`]: ../log_sniffer/index.html

#[cfg(feature = "std")]
extern crate alloc;

use serde::{Deserialize, Serialize};

pub mod wire_protocol;

/// A single sensor reading, tagged with its numeric type.
///
/// Each variant carries the raw value in its natural representation so the host can plot it
/// without a lossy widening at the source. Raw ADC/load-cell readings and calibrated axis
/// values therefore use different variants (e.g. [`U16`](Value::U16) for analog counts,
/// [`I16`](Value::I16) for a calibrated axis).
///
/// Serialized with postcard as a one-byte variant tag followed by the value's little-endian
/// bytes (varint-encoded for the integer variants).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Unsigned 16-bit — e.g. analog ADC counts.
    U16(u16),
    /// Unsigned 32-bit.
    U32(u32),
    /// Signed 16-bit — e.g. a calibrated axis value.
    I16(i16),
    /// Signed 32-bit — e.g. raw load-cell counts.
    I32(i32),
    /// 32-bit float.
    F32(f32),
}

/// One telemetry point emitted by the firmware: a [`Value`] on a numbered channel, stamped
/// with the device uptime at which it was sampled.
///
/// Sent as the payload of a [`Telemetry`](wire_protocol::FrameType::Telemetry) frame. A pedal
/// contributes several channels (e.g. a raw and a calibrated `channel_id`); the host maps each
/// `channel_id` to a named signal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SensorSample {
    /// Device uptime in milliseconds at the moment the reading was taken.
    pub timestamp_ms: u32,
    /// Identifies which signal this sample belongs to (raw vs. calibrated, per pedal).
    pub channel_id: u8,
    /// The reading itself.
    pub value: Value,
}

/// Decodes a [`Telemetry`](wire_protocol::FrameType::Telemetry) frame's postcard payload back
/// into a `SensorSample` — the receiving-side counterpart to
/// [`write_sensor_frame`](wire_protocol::write_sensor_frame)'s encoding.
///
/// `value` is the frame *payload* (as handed out by
/// [`parse_frame`](wire_protocol::parse_frame) or
/// [`FrameReader`](wire_protocol::FrameReader)), not a whole frame. Returns a
/// [`postcard::Error`] if the bytes are not a valid `SensorSample` encoding.
impl TryFrom<&[u8]> for SensorSample {
    type Error = postcard::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        postcard::from_bytes::<Self>(value)
    }
}
