use crate::records::{LogLevel, LogMessage, Timestamp as LogTimestamp};
use anyhow::Context;
use mcap::records::MessageHeader;
use serde::Serializer;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{info, instrument};

/// Receives decoded log messages and writes them to an MCAP file.
///
/// Exits and finalizes the file when `log_rx` is closed — i.e., when [`decoder_task`] finishes.
#[instrument(skip(log_rx))]
pub async fn mcap_writer_task(
    output_path: PathBuf,
    mut log_rx: mpsc::Receiver<LogMessage>,
) -> anyhow::Result<()> {
    let file = std::fs::File::create(&output_path)
        .with_context(|| format!("failed to create MCAP file: {}", output_path.display()))?;
    let mut writer = mcap::WriteOptions::default()
        .create(file)
        .context("failed to create MCAP writer")?;

    let schema_id = writer.add_schema(Log::name(), Log::encoding(), Log::json_schema())?;
    let channel_id = writer.add_channel(
        schema_id,
        Log::topic(),
        Log::message_encoding(),
        &BTreeMap::new(),
    )?;

    let mut sequence: u32 = 0;
    while let Some(msg) = log_rx.recv().await {
        let log_time = msg.log_time;
        let publish_time = msg.publish_time;
        let log = Log::from(msg);
        let data = serde_json::to_vec(&log).context("failed to serialize log message")?;
        writer
            .write_to_known_channel(
                &MessageHeader {
                    channel_id,
                    log_time,
                    publish_time,
                    sequence,
                },
                &data,
            )
            .context("failed to write MCAP message")?;
        sequence = sequence.wrapping_add(1);
    }

    writer.finish().context("failed to finalize MCAP file")?;
    info!("MCAP writer finished");
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Log {
    pub timestamp: Timestamp,
    pub level: Level,
    pub message: String,
    pub name: String,
    pub file: String,
    pub line: u64,
}

impl Log {
    pub fn json_schema() -> &'static [u8] {
        LOG_SCHEMA
    }

    pub fn name() -> &'static str {
        "foxglove.Log"
    }

    pub fn encoding() -> &'static str {
        "jsonschema"
    }

    pub fn topic() -> &'static str {
        "/rosout"
    }

    pub fn message_encoding() -> &'static str {
        "json"
    }
}

impl From<LogMessage> for Log {
    fn from(value: LogMessage) -> Self {
        Self {
            timestamp: value.timestamp.into(),
            level: value.level.into(),
            message: value.message,
            name: value.module,
            file: value.location.file,
            line: value.location.line,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Timestamp {
    pub sec: u64,
    pub nsec: u32,
}

impl From<LogTimestamp> for Timestamp {
    fn from(value: LogTimestamp) -> Self {
        Self {
            sec: value.sec,
            nsec: value.nsec,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum Level {
    Unknown = 0,
    Debug = 1,
    Info = 2,
    Warning = 3,
    Error = 4,
    Fatal = 5,
}

impl serde::Serialize for Level {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl From<LogLevel> for Level {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Debug => Self::Debug,
            LogLevel::Info => Self::Info,
            LogLevel::Warning => Self::Warning,
            LogLevel::Error => Self::Error,
            LogLevel::Fatal => Self::Fatal,
            LogLevel::Unknown => Self::Unknown,
        }
    }
}

const LOG_SCHEMA: &[u8] = br#"{
  "title": "foxglove.Log",
  "description": "A log message",
  "type": "object",
  "properties": {
    "timestamp": {
      "type": "object",
      "properties": {
        "sec":  { "type": "integer", "minimum": 0 },
        "nsec": { "type": "integer", "minimum": 0, "maximum": 999999999 }
      }
    },
    "level":   { "oneOf": [{"title": "UNKNOWN","const": 0},{"title": "DEBUG","const": 1},{"title": "INFO","const": 2},{"title": "WARNING","const": 3},{"title": "ERROR","const": 4},{"title": "FATAL","const": 5}] },
    "message": { "type": "string" },
    "name":    { "type": "string" },
    "file":    { "type": "string" },
    "line":    { "type": "integer", "minimum": 0 }
  },
  "required": ["timestamp", "level", "message", "name", "file", "line"]
}"#;
