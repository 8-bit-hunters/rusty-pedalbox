//! Log sniffer: reads raw defmt frames from a USB CDC serial port and decodes them.

pub mod decoder;
pub mod mcap;
pub mod records;
pub mod serial;

pub enum ConnectionEvent {
    Data(Vec<u8>),
    Reconnected,
}
