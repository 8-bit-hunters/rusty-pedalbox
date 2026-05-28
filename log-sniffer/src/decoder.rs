use crate::ConnectionEvent;
use crate::records::{Location, LogLevel, LogMessage, LogMessageBuilder};
use anyhow::Context;
use defmt_decoder::{
    DecodeError, Frame, Location as DefmtLocation, Locations, StreamDecoder, Table,
};
use defmt_parser::{Level as DefmtLogLevel, Level};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, info, instrument, warn};

/// Receives [`ConnectionEvent`]s from the serial channel, decodes defmt frames, and forwards
/// [`LogMessage`]s to the MCAP writer.
///
/// On [`ConnectionEvent::Reconnected`] the defmt stream decoder is reset so that stale frame
/// state from the previous connection does not corrupt the new byte stream.
///
/// Exits when `bytes_rx` is closed — i.e., when [`serial_port_task`] stops sending.
#[instrument(skip_all)]
pub async fn decoder_task(
    elf_path: PathBuf,
    mut bytes_rx: mpsc::Receiver<ConnectionEvent>,
    log_tx: mpsc::Sender<LogMessage>,
) -> anyhow::Result<()> {
    let elf_bytes = std::fs::read(&elf_path)
        .with_context(|| format!("failed to read ELF: {}", elf_path.display()))?;
    let table = Table::parse(&elf_bytes)
        .context("Failed to parse defmt from ELF")?
        .context("ELF contains no defmt data - was it built with defmt?")?;

    let mut decoder = Decoder::new(&table, elf_bytes)?;

    while let Some(event) = bytes_rx.recv().await {
        match event {
            ConnectionEvent::Data(bytes) => {
                for msg in decoder.decode(&bytes) {
                    log_tx.send(msg).await?;
                }
            }
            ConnectionEvent::Reconnected => decoder.reset(&table),
        }
    }
    Ok(())
}

/// Wraps a defmt stream decoder and source location map.
///
/// Borrows from a [`Table`] that must outlive this struct - typically owned by [`decoder_task`].
pub struct Decoder<'a> {
    stream: Box<dyn StreamDecoder + Send + 'a>,
    locations: Locations,
}

impl<'a> Decoder<'a> {
    /// Creates a new decoder from a parsed defmt `table` and the raw `elf_bytes` used to
    /// extract source locations.
    pub fn new(table: &'a Table, elf_bytes: Vec<u8>) -> anyhow::Result<Self> {
        let locations = table.get_locations(&elf_bytes)?;
        let stream = table.new_stream_decoder();
        Ok(Self { stream, locations })
    }

    /// Discards all buffered state and starts a fresh stream decoder.
    ///
    /// Called on [`ConnectionEvent::Reconnected`] so that leftover bytes from the previous
    /// connection do not cause spurious `Malformed` errors on the new stream.
    pub fn reset(&mut self, table: &'a Table) {
        self.stream = table.new_stream_decoder();
    }

    /// Feeds `bytes` into the stream decoder and drains all complete frames.
    ///
    /// `UnexpectedEof` is not an error — it means the frame is incomplete and more bytes are
    /// needed. `Malformed` means one frame was corrupt; decoding continues from the next byte
    /// so that a single bad frame does not stall the rest of the stream.
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

                    info!(frame = %frame.display(true), "decoded frame");

                    let result = FrameData::from_frame_and_locations(frame, &self.locations)
                        .and_then(|fd| {
                            LogMessageBuilder::from(fd)
                                .received_at_ns(received_at_ns)
                                .build()
                        });
                    match result {
                        Ok(msg) => messages.push(msg),
                        Err(e) => warn!("Failed to build log message: {e}"),
                    }
                }
                Err(DecodeError::UnexpectedEof) => {
                    debug!("Unexpected end of file, need more bytes");
                    break;
                }
                Err(DecodeError::Malformed) => {
                    warn!("Malformed defmt frame, skipping");
                    continue;
                }
            }
        }
        messages
    }
}

/// Intermediate representation of a decoded defmt [`Frame`].
///
/// Extracts only the fields needed for a [`LogMessage`], acting as the boundary between
/// defmt types and the rest of the pipeline. Constructed via
/// [`FrameData::from_frame_and_locations`] and consumed by [`LogMessageBuilder`].
struct FrameData {
    pub level: Level,
    pub timestamp: u64,
    pub message: String,
    pub location: DefmtLocation,
}

impl FrameData {
    /// Extracts frame data from a decoded defmt frame.
    ///
    /// Returns an error if the frame is missing a source location (not present in the
    /// `locations` map) or a log level — both are required for a valid [`LogMessage`].
    pub fn from_frame_and_locations(frame: Frame, locations: &Locations) -> anyhow::Result<Self> {
        let location = locations
            .get(&frame.index())
            .context("Missing location from defmt")?
            .clone();
        let level = frame.level().context("Missing level from defmt frame")?;
        let message = frame.display_message().to_string();
        let timestamp = frame
            .display_timestamp()
            .and_then(|t| firmware_timestamp_to_ns(t.to_string()))
            .context("Failed to get timestamp from defmt frame")?;

        Ok(Self {
            level,
            timestamp,
            message,
            location,
        })
    }
}

impl From<FrameData> for LogMessageBuilder {
    fn from(frame: FrameData) -> Self {
        LogMessageBuilder::default()
            .level(frame.level.into())
            .log_time(frame.timestamp)
            .message(frame.message)
            .module(
                frame
                    .location
                    .module
                    .split("::")
                    .next()
                    .unwrap_or(&frame.location.module)
                    .to_string(),
            )
            .location(frame.location.into())
    }
}

impl From<DefmtLogLevel> for LogLevel {
    fn from(value: DefmtLogLevel) -> Self {
        match value {
            Level::Trace => LogLevel::Unknown,
            Level::Debug => LogLevel::Debug,
            Level::Info => LogLevel::Info,
            Level::Warn => LogLevel::Warning,
            Level::Error => LogLevel::Error,
        }
    }
}

impl From<DefmtLocation> for Location {
    fn from(value: DefmtLocation) -> Self {
        Self {
            file: value
                .file
                .file_name()
                .unwrap_or(value.file.as_os_str())
                .to_string_lossy()
                .into_owned(),
            line: value.line,
        }
    }
}

/// Converts a defmt-formatted firmware timestamp string to nanoseconds.
///
/// Handles all defmt display formats:
/// - `"4827.595306"` — seconds.micros (Seconds hint, Micros precision)
/// - `"4827.595"`    — seconds.millis (Seconds hint, Millis precision)
/// - `"01:20:27.595306"` — HH:MM:SS.micros (Time hint)
/// - `"0:01:20:27.595306"` — D:HH:MM:SS.micros (Time hint, with days)
///
/// Returns `None` if the string doesn't match any known format.
pub fn firmware_timestamp_to_ns(s: String) -> Option<u64> {
    let (time_part, frac_ns) = match s.split_once('.') {
        Some((t, f)) => (t, fraction_to_ns(f)?),
        None => (s.as_str(), 0u64),
    };

    let parts: Vec<&str> = time_part.split(':').collect();
    let total_secs: u64 = match parts.as_slice() {
        [secs] => secs.parse().ok()?,
        [mins, secs] => mins.parse::<u64>().ok()? * 60 + secs.parse::<u64>().ok()?,
        [h, m, s] => {
            h.parse::<u64>().ok()? * 3600 + m.parse::<u64>().ok()? * 60 + s.parse::<u64>().ok()?
        }
        [d, h, m, s] => {
            d.parse::<u64>().ok()? * 86400
                + h.parse::<u64>().ok()? * 3600
                + m.parse::<u64>().ok()? * 60
                + s.parse::<u64>().ok()?
        }
        _ => return None,
    };

    Some(total_secs * 1_000_000_000 + frac_ns)
}

/// Parses a fractional-seconds string (e.g. `"595306"` or `"595"`) into nanoseconds
/// by appending trailing zeros to 9 digits.
fn fraction_to_ns(frac: &str) -> Option<u64> {
    if frac.is_empty() {
        return Some(0);
    }
    let normalized = if frac.len() >= 9 {
        frac[..9].to_string()
    } else {
        format!("{:0<9}", frac)
    };
    normalized.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_location(file: &str, line: u64, module: &str) -> DefmtLocation {
        DefmtLocation {
            file: PathBuf::from(file),
            line,
            module: module.to_string(),
        }
    }

    #[test]
    fn when_converting_frame_data_to_log_message_builder() {
        // Given
        let frame_data = FrameData {
            level: Level::Debug,
            timestamp: 1,
            message: "hello".to_string(),
            location: make_location("/foo/bar.rs", 69, "my_module"),
        };

        // When
        let result = LogMessageBuilder::from(frame_data);

        // Then
        assert_eq!(result.level, Some(LogLevel::Debug));
        assert_eq!(
            result.location,
            Some(Location {
                file: "bar.rs".to_string(),
                line: 69
            })
        );
        assert_eq!(result.module, Some("my_module".to_string()));
        assert_eq!(result.log_time, Some(1));
        assert_eq!(result.message, Some("hello".to_string()));
        assert!(result.publish_time.is_none());
        assert!(result.timestamp.is_none());
    }

    #[test]
    fn when_provided_by_received_time() {
        // Given / When
        let result = LogMessageBuilder::default().received_at_ns(1_000_000_000);

        // Then
        assert_eq!(result.publish_time, Some(1_000_000_000));
        let ts = result.timestamp.unwrap();
        assert_eq!(ts.sec, 1);
        assert_eq!(ts.nsec, 0);
    }

    #[test]
    fn when_received_timestamp_is_not_provided() {
        // Given
        let builder = LogMessageBuilder::from(FrameData {
            level: Level::Debug,
            timestamp: 1,
            message: "hello".to_string(),
            location: make_location("/foo/bar.rs", 69, "my_module"),
        });

        // When / Then
        assert!(builder.build().is_err());
    }

    #[test]
    fn when_build_with_the_two_stage_method() {
        // Given
        let frame_data = FrameData {
            level: Level::Debug,
            timestamp: 1_000_000_000,
            message: "hello".to_string(),
            location: make_location("/foo/bar.rs", 69, "my_module"),
        };

        // When
        let result = LogMessageBuilder::from(frame_data)
            .received_at_ns(2_000_000_000)
            .build()
            .expect("Failed to build log message");

        // Then
        assert_eq!(result.level, LogLevel::Debug);
        assert_eq!(result.location.file, "bar.rs");
        assert_eq!(result.location.line, 69);
        assert_eq!(result.module, "my_module");
        assert_eq!(result.log_time, 1_000_000_000);
        assert_eq!(result.message, "hello");
        assert_eq!(result.publish_time, 2_000_000_000);
        assert_eq!(result.timestamp.sec, 2);
        assert_eq!(result.timestamp.nsec, 0);
    }
}
