use anyhow::Result;
use clap::Parser;
use log_sniffer::decoder::decoder_task;
use log_sniffer::mcap::mcap_writer_task;
use log_sniffer::serial::serial_port_task;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "log-sniffer",
    about = "Decode defmt logs from USB CDC and write to MCAP"
)]
struct Args {
    /// Serial port, e.g. /dev/ttyACM2
    #[arg(short, long)]
    port: String,

    /// Path to the firmware ELF file of the device
    #[arg(short, long)]
    elf: PathBuf,

    /// Path to the output MCAP file
    #[arg(short, long)]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    let (bytes_tx, bytes_rx) = tokio::sync::mpsc::channel(100);
    let (log_tx, log_rx) = tokio::sync::mpsc::channel(100);

    let mut writer_handle = tokio::spawn(mcap_writer_task(args.output, log_rx));
    let mut decoder_handle = tokio::spawn(decoder_task(args.elf, bytes_rx, log_tx));

    // Shutdown sequencing: when ctrl+c or serial fires, the inline `serial_port_task` future
    // is dropped, which drops `bytes_tx`. That closes `bytes_rx`, the decoder loop exits
    // and drops `log_tx`, the writer's `log_rx` closes, `writer.finish()` is called,
    // and both awaits return cleanly.
    tokio::select!(
        _ = serial_port_task(args.port, 115_200, bytes_tx) => {},
        _ = &mut decoder_handle => {},
        _ = &mut writer_handle => {},
        _ = tokio::signal::ctrl_c() => {},
    );

    writer_handle.await??;
    decoder_handle.await??;
    Ok(())
}
