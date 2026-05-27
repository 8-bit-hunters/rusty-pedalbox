use anyhow::Context;
use defmt_decoder::{DecodeError, Frame, Location, Locations, StreamDecoder, Table};
use defmt_parser::Level;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, info, instrument, warn};

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
                    break;
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
    pub location: Location,
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
            .unwrap_or_default();

        Ok(Self {
            level,
            timestamp,
            message,
            location,
        })
    }
}

/// A fully decoded log message, ready for serialization and storage.
///
/// Carries two timestamps: `firmware_timestamp` is the device uptime in nanoseconds at the
/// moment the log was emitted (0 if the firmware provides none), and `received_at_ns` is the
/// host wall-clock time the bytes arrived.
#[derive(Debug, serde::Serialize)]
pub struct LogMessage {
    #[serde(skip)]
    pub received_at_ns: u64,
    /// Firmware-side timestamp (device uptime) as reported by defmt.
    pub firmware_timestamp: u64,
    pub level: String,
    pub message: String,
    /// Source location formatted as `file:line`.
    pub location: String,
}

/// Builds a [`LogMessage`] in two stages.
///
/// Frame-derived fields (level, message, location, firmware timestamp) are populated via
/// `From<FrameData>`. The host-side received timestamp is added separately via
/// [`received_at_ns`](LogMessageBuilder::received_at_ns), reflecting that these two concerns
/// are resolved at different points in the decoding pipeline.
#[derive(Debug, Default)]
struct LogMessageBuilder {
    pub received_at_ns: Option<u64>,
    pub firmware_timestamp: Option<u64>,
    pub level: Option<String>,
    pub message: Option<String>,
    pub location: Option<String>,
}

impl LogMessageBuilder {
    /// Sets the host-side received timestamp in nanoseconds since UNIX epoch.
    pub fn received_at_ns(mut self, timestamp: u64) -> Self {
        self.received_at_ns = Some(timestamp);
        self
    }

    pub fn firmware_timestamp(mut self, timestamp: u64) -> Self {
        self.firmware_timestamp = Some(timestamp);
        self
    }

    pub fn level(mut self, level: Level) -> Self {
        let level = level.as_str();
        self.level = Some(level.to_string());
        self
    }

    pub fn message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }

    pub fn location(mut self, location: Location) -> Self {
        let location = format!("{}:{}", location.file.display(), location.line);
        self.location = Some(location);
        self
    }

    pub fn build(self) -> anyhow::Result<LogMessage> {
        let received_at_ns = self.received_at_ns.context("Missing received time")?;
        let firmware_timestamp = self
            .firmware_timestamp
            .context("Missing firmware timestamp")?;
        let level = self.level.context("Missing level")?;
        let message = self.message.context("Missing message")?;
        let location = self.location.context("Missing location")?;

        Ok(LogMessage {
            received_at_ns,
            firmware_timestamp,
            level,
            message,
            location,
        })
    }
}

impl From<FrameData> for LogMessageBuilder {
    fn from(frame: FrameData) -> Self {
        LogMessageBuilder::default()
            .level(frame.level)
            .firmware_timestamp(frame.timestamp)
            .message(frame.message)
            .location(frame.location)
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
/// by left-aligning it to 9 digits.
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

    #[test]
    fn when_converting_frame_data_to_log_message_builder() {
        // Given
        let location = Location {
            file: PathBuf::from("/foo/bar.rs"),
            line: 69,
            module: "my_module".to_string(),
        };
        let frame_data = FrameData {
            level: Level::Debug,
            timestamp: 1,
            message: "hello".to_string(),
            location,
        };

        // When
        let result = LogMessageBuilder::from(frame_data);

        // Then
        assert_eq!(result.level, Some(Level::Debug.as_str().to_string()));
        assert_eq!(result.location, Some("/foo/bar.rs:69".to_string()));
        assert_eq!(result.firmware_timestamp, Some(1));
        assert_eq!(result.message, Some("hello".to_string()));
        assert!(result.received_at_ns.is_none());
    }

    #[test]
    fn when_provided_by_received_time() {
        // Given
        let received_at = 1_000_000_000;
        let builder = LogMessageBuilder::default();

        // When
        let result = builder.received_at_ns(received_at);

        // Then
        assert_eq!(result.received_at_ns, Some(received_at));
    }

    #[test]
    fn when_received_timestamp_is_not_provided() {
        // Given
        let location = Location {
            file: PathBuf::from("/foo/bar.rs"),
            line: 69,
            module: "my_module".to_string(),
        };
        let frame_data = FrameData {
            level: Level::Debug,
            timestamp: 1,
            message: "hello".to_string(),
            location,
        };
        let builder = LogMessageBuilder::from(frame_data);

        // When
        let result = builder.build();

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn when_build_with_the_two_stage_method() {
        // Given
        let location = Location {
            file: PathBuf::from("/foo/bar.rs"),
            line: 69,
            module: "my_module".to_string(),
        };
        let frame_data = FrameData {
            level: Level::Debug,
            timestamp: 1,
            message: "hello".to_string(),
            location,
        };

        // When
        let result = LogMessageBuilder::from(frame_data)
            .received_at_ns(0)
            .build()
            .expect("Failed to build log message");

        // Then
        assert_eq!(result.level, "debug".to_string());
        assert_eq!(result.location, "/foo/bar.rs:69".to_string());
        assert_eq!(result.firmware_timestamp, 1);
        assert_eq!(result.message, "hello".to_string());
        assert_eq!(result.received_at_ns, 0);
    }
}
