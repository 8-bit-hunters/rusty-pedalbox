use anyhow::Context;

/// A fully decoded log message shaped to the `foxglove.Log` schema.
///
/// `publish_time` (host wall-clock) and `log_time` (device uptime) are carried for the MCAP
/// message header but excluded from the serialized JSON body via `#[serde(skip)]`.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct LogMessage {
    /// Host wall-clock time in nanoseconds since UNIX epoch. Used as MCAP `publish_time`.
    #[serde(skip)]
    pub publish_time: u64,
    /// Device uptime in nanoseconds at the moment the log was emitted. Used as MCAP `log_time`.
    #[serde(skip)]
    pub log_time: u64,
    /// Host wall-clock time of receipt, split for the `foxglove.Log` timestamp field.
    pub timestamp: Timestamp,
    /// Numeric log level: 1=DEBUG, 2=INFO, 3=WARNING, 4=ERROR, 5=FATAL.
    pub level: LogLevel,
    pub message: String,
    /// Firmware module path, used as the process/node name in the log panel.
    pub module: String,
    pub location: Location,
}

/// Wall-clock timestamp split into whole seconds and nanosecond remainder,
/// matching the `foxglove.Log` schema format.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Timestamp {
    pub sec: u64,
    pub nsec: u32,
}

impl From<u64> for Timestamp {
    fn from(ns: u64) -> Self {
        Self {
            sec: ns / 1_000_000_000,
            nsec: (ns % 1_000_000_000) as u32,
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Fatal,
    Unknown,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct Location {
    pub file: String,
    pub line: u64,
}

/// Builds a [`LogMessage`] in two stages.
///
/// Frame-derived fields (level, message, location, log_time) are populated via
/// `From<FrameData>`. The host-side received timestamp is added separately via
/// [`received_at_ns`](LogMessageBuilder::received_at_ns), reflecting that these two concerns
/// are resolved at different points in the decoding pipeline.
#[derive(Debug, Default)]
pub struct LogMessageBuilder {
    pub publish_time: Option<u64>,
    pub log_time: Option<u64>,
    pub timestamp: Option<Timestamp>,
    pub level: Option<LogLevel>,
    pub message: Option<String>,
    pub module: Option<String>,
    pub location: Option<Location>,
}

impl LogMessageBuilder {
    /// Sets the host-side received timestamp and derives the `foxglove.Log` timestamp from it.
    pub fn received_at_ns(mut self, ns: u64) -> Self {
        self.publish_time = Some(ns);
        self.timestamp = Some(Timestamp::from(ns));
        self
    }

    pub fn log_time(mut self, ns: u64) -> Self {
        self.log_time = Some(ns);
        self
    }

    pub fn level(mut self, level: LogLevel) -> Self {
        self.level = Some(level);
        self
    }

    pub fn message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }

    pub fn location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    pub fn module(mut self, name: String) -> Self {
        self.module = Some(name);
        self
    }

    pub fn build(self) -> anyhow::Result<LogMessage> {
        let publish_time = self.publish_time.context("Missing publish time")?;
        let log_time = self.log_time.context("Missing log time")?;
        let timestamp = self.timestamp.context("Missing timestamp")?;
        let level = self.level.context("Missing level")?;
        let message = self.message.context("Missing message")?;
        let name = self.module.context("Missing name")?;
        let location = self.location.context("Missing location")?;

        Ok(LogMessage {
            publish_time,
            log_time,
            timestamp,
            level,
            message,
            module: name,
            location,
        })
    }
}
