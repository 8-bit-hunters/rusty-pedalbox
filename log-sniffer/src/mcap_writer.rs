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
        let log_time = msg
            .firmware_timestamp
            .as_deref()
            .and_then(firmware_timestamp_to_ns)
            .unwrap_or(msg.received_at_ns);
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

/// Converts a defmt-formatted firmware timestamp string to nanoseconds.
///
/// Handles all defmt display formats:
/// - `"4827.595306"` — seconds.micros (Seconds hint, Micros precision)
/// - `"4827.595"`    — seconds.millis (Seconds hint, Millis precision)
/// - `"01:20:27.595306"` — HH:MM:SS.micros (Time hint)
/// - `"0:01:20:27.595306"` — D:HH:MM:SS.micros (Time hint, with days)
///
/// Returns `None` if the string doesn't match any known format.
fn firmware_timestamp_to_ns(s: &str) -> Option<u64> {
    let (time_part, frac_ns) = match s.split_once('.') {
        Some((t, f)) => (t, fraction_to_ns(f)?),
        None => (s, 0u64),
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
