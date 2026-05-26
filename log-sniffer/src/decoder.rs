use anyhow::Context;
use defmt_decoder::{DecodeError, Locations, StreamDecoder, Table};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{debug, instrument, warn};

/// Receives raw bytes from the serial channel, decodes them as defmt frames, and prints them.
///
/// Exits when `bytes_rx` is closed — i.e., when [`serial_port_task`] stops sending.
#[instrument(skip_all)]
pub async fn decoder_task(
    elf_path: PathBuf,
    mut bytes_rx: mpsc::Receiver<Vec<u8>>,
) -> anyhow::Result<()> {
    let elf_bytes = std::fs::read(&elf_path)
        .with_context(|| format!("failed to read ELF: {}", elf_path.display()))?;
    let table = Table::parse(&elf_bytes)
        .context("Failed to parse defmt from ELF")?
        .context("ELF contains no defmt data - was it built with defmt?")?;
    let mut decoder = Decoder::new(&table, elf_bytes)?;
    while let Some(bytes) = bytes_rx.recv().await {
        decoder.decode(&bytes);
    }
    Ok(())
}

/// Wraps a defmt stream decoder and source location map.
///
/// Borrows from a [`Table`] that must outlive this struct - typically owned by [`decoder_task`].
pub struct Decoder<'a> {
    stream: Box<dyn StreamDecoder + 'a>,
    #[allow(dead_code)]
    locations: Locations,
}

impl<'a> Decoder<'a> {
    pub fn new(table: &'a Table, elf_bytes: Vec<u8>) -> anyhow::Result<Self> {
        let locations = table.get_locations(&elf_bytes)?;
        let stream = table.new_stream_decoder();
        Ok(Self { stream, locations })
    }

    /// Feeds `bytes` into the stream decoder and drains all complete frames.
    ///
    /// `UnexpectedEof` is not an error — it means the frame isn't complete yet and
    /// more bytes are needed before the next frame can be emitted.
    pub fn decode(&mut self, bytes: &[u8]) {
        self.stream.received(bytes);
        loop {
            match self.stream.decode() {
                Ok(frame) => {
                    println!("{}", frame.display(true));
                }
                Err(DecodeError::UnexpectedEof) => {
                    debug!("Unexpected end of file, need more bytes");
                    break;
                }
                Err(DecodeError::Malformed) => {
                    warn!("Malformed defmt frame, skipping");
                    break;
                }
            }
        }
    }
}
