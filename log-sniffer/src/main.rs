use anyhow::Result;
use clap::Parser;
use log_sniffer::decoder::decoder_task;
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
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    let (bytes_tx, bytes_rx) = tokio::sync::mpsc::channel(100);
    let (_, decoder_result) = tokio::join!(
        serial_port_task(args.port, 115_200, bytes_tx),
        decoder_task(args.elf, bytes_rx)
    );
    decoder_result?;
    Ok(())
}
