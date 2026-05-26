//! Log sniffer: reads raw defmt frames from a USB CDC serial port and decodes them.

pub mod decoder;
pub mod mcap_writer;
pub mod serial;
