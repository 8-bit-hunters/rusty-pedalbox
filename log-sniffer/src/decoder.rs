use anyhow::Context;
use defmt_decoder::{DecodeError, Locations, StreamDecoder, Table};
use defmt_parser::Level;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::mpsc;
use tracing::{debug, instrument, warn};

#[derive(Debug, serde::Serialize)]
pub struct LogMessage {
    #[serde(skip)]
    pub received_at_ns: u64,
    pub received_at: String,
    pub firmware_timestamp: Option<String>,
    pub level: Option<String>,
    pub message: String,
    pub location: Option<String>,
}

/// Receives raw bytes from the serial channel, decodes them as defmt frames, and prints them.
///
/// Exits when `bytes_rx` is closed — i.e., when [`serial_port_task`] stops sending.
#[instrument(skip_all)]
pub async fn decoder_task(
    elf_path: PathBuf,
    mut bytes_rx: mpsc::Receiver<Vec<u8>>,
    log_tx: mpsc::Sender<LogMessage>,
) -> anyhow::Result<()> {
    let elf_bytes = std::fs::read(&elf_path)
        .with_context(|| format!("failed to read ELF: {}", elf_path.display()))?;
    let table = Table::parse(&elf_bytes)
        .context("Failed to parse defmt from ELF")?
        .context("ELF contains no defmt data - was it built with defmt?")?;
    let mut decoder = Decoder::new(&table, elf_bytes)?;
    while let Some(bytes) = bytes_rx.recv().await {
        for msg in decoder.decode(&bytes) {
            log_tx.send(msg).await?;
        }
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
    pub fn decode(&mut self, bytes: &[u8]) -> Vec<LogMessage> {
        self.stream.received(bytes);
        let mut messages = Vec::new();
        loop {
            match self.stream.decode() {
                Ok(frame) => {
                    let received_at_ns = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as u64;
                    let received_at =
                        OffsetDateTime::from_unix_timestamp_nanos(received_at_ns as i128)
                            .map(|dt| dt.format(&Rfc3339).unwrap_or_default())
                            .unwrap_or_default();
                    println!("{}", frame.display(true));
                    let location = self
                        .locations
                        .get(&frame.index())
                        .map(|loc| format!("{}:{}", loc.file.display(), loc.line));
                    let level = frame.level().map(|l| {
                        match l {
                            Level::Trace => "TRACE",
                            Level::Debug => "DEBUG",
                            Level::Info => "INFO",
                            Level::Warn => "WARN",
                            Level::Error => "ERROR",
                        }
                        .to_string()
                    });
                    let firmware_timestamp = frame.display_timestamp().map(|t| t.to_string());
                    messages.push(LogMessage {
                        received_at_ns,
                        received_at: received_at.clone(),
                        firmware_timestamp,
                        level,
                        message: frame.display_message().to_string(),
                        location,
                    })
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
        messages
    }
}
