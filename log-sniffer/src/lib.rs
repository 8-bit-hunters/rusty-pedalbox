//! Reads defmt frames from a USB CDC serial port and writes them as [`foxglove.Log`] messages
//! to an MCAP file.
//!
//! # Pipeline
//!
//! ```text
//! serial_port_task  ──ConnectionEvent──▶  decoder_task  ──LogMessage──▶  mcap_writer_task
//!   (serial)                               (decoder)                        (mcap)
//! ```
//!
//! - [`serial::serial_port_task`] owns the serial port, reconnects on failure, and forwards
//!   raw bytes plus reconnection signals through a [`ConnectionEvent`] channel.
//! - [`decoder::decoder_task`] parses defmt frames from raw bytes, resets its stream state on
//!   reconnection, and forwards decoded [`records::LogMessage`]s.
//! - [`mcap::mcap_writer_task`] serialises each message as JSON and writes it to an MCAP file
//!   using the [`foxglove.Log`](mcap::Log) schema.
//!
//! [`foxglove.Log`]: mcap::Log

pub mod decoder;
pub mod mcap;
pub mod records;
pub mod serial;

/// Events produced by [`serial::serial_port_task`] and consumed by [`decoder::decoder_task`].
pub enum ConnectionEvent {
    /// Raw bytes read from the serial port.
    Data(Vec<u8>),
    /// Signals that the serial port was (re)opened. The decoder resets its defmt stream state
    /// on receipt so that stale frame boundaries from the previous connection are discarded.
    Reconnected,
}
