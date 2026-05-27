use crate::decoder::LogMessage;
use anyhow::Context;
use log::info;
use mcap::records::MessageHeader;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::instrument;

const LOG_SCHEMA: &[u8] = br#"{
  "type": "object",
  "properties": {
    "firmware_timestamp": { "type": "integer" },
    "level":              { "type": "string" },
    "message":            { "type": "string" },
    "location":           { "type": "string" }
  },
  "required": ["message"]
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
    let channel_id = writer.add_channel(schema_id, "/rosout", "json", &BTreeMap::new())?;

    let mut sequence: u32 = 0;
    while let Some(msg) = log_rx.recv().await {
        let data = serde_json::to_vec(&msg).context("failed to serialize log message")?;
        writer
            .write_to_known_channel(
                &MessageHeader {
                    channel_id,
                    log_time: msg.firmware_timestamp,
                    publish_time: msg.received_at_ns,
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
