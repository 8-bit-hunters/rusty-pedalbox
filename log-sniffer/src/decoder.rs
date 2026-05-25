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
    let decoder = Decoder::new(elf_path)?;
    let mut stream = decoder.new_stream_decoder();
    while let Some(bytes) = bytes_rx.recv().await {
        decoder.decode(&mut *stream, &bytes);
    }
    Ok(())
}

/// Holds the defmt symbol table and source location map loaded from a firmware ELF.
pub struct Decoder {
    table: Table,
    locations: Locations,
}

impl Decoder {
    pub fn new(elf_file: PathBuf) -> anyhow::Result<Self> {
        let elf_bytes = std::fs::read(&elf_file)
            .with_context(|| format!("failed to read ELF: {}", elf_file.display()))?;
        let table = Table::parse(&elf_bytes)
            .context("Failed to parse defmt from ELF")?
            .context("ELF contains no defmt data - was it built with defmt?")?;
        let locations = table.get_locations(&elf_bytes)?;
        Ok(Self { table, locations })
    }

    /// Creates a stream decoder tied to this table's lifetime.
    ///
    /// Must be reset (dropped and recreated) on reconnect — a broken byte stream
    /// corrupts the decoder's internal framing state.
    pub fn new_stream_decoder(&self) -> Box<dyn StreamDecoder + '_> {
        self.table.new_stream_decoder()
    }

    /// Feeds `bytes` into the stream decoder and drains all complete frames.
    ///
    /// `UnexpectedEof` is not an error — it means the frame isn't complete yet and
    /// more bytes are needed before the next frame can be emitted.
    pub fn decode(&self, stream: &mut dyn StreamDecoder, bytes: &[u8]) {
        stream.received(bytes);
        loop {
            match stream.decode() {
                Ok(frame) => {
                    let _loc = self.locations.get(&frame.index());
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
