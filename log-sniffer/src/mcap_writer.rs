use crate::decoder::LogMessage;
use anyhow::Context;
use mcap::records::MessageHeader;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::instrument;

const LOG_SCHEMA: &[u8] = br#"{
  "type": "object",
  "properties": {
    "received_at":        { "type": "string" },
    "firmware_timestamp": { "type": "string" },
    "level":              { "type": "string" },
    "message":            { "type": "string" },
    "location":           { "type": "string" }
  },
  "required": ["received_at", "message"]
}"#;

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

    let schema_id = writer.add_schema("LogMessage", "jsonschema", LOG_SCHEMA)?;
    let channel_id = writer.add_channel(schema_id, "/logs", "json", &BTreeMap::new())?;

    let mut sequence: u32 = 0;
    while let Some(msg) = log_rx.recv().await {
        let log_time = msg.received_at_ns;
        let data = serde_json::to_vec(&msg).context("failed to serialize log message")?;
        writer
            .write_to_known_channel(
                &MessageHeader {
                    channel_id,
                    log_time,
                    publish_time: log_time,
                    sequence,
                },
                &data,
            )
            .context("failed to write MCAP message")?;
        sequence = sequence.wrapping_add(1);
    }

    writer.finish().context("failed to finalize MCAP file")?;
    Ok(())
}
