use anyhow::Result;
use tokio::io::AsyncReadExt;
use tokio::time::{Duration, timeout};
use tokio_serial::{SerialPort, SerialStream};
use tracing::{debug, info, instrument, warn};

/// Runs forever: opens the serial port, reads until the connection is lost, then reconnects.
///
/// Retries every second on open failure or any read error (including EOF and the
/// 5-second silence timeout). Intended to be spawned as a [`tokio::task`].
#[instrument]
pub async fn serial_port_task(port: String, baudrate: u32) {
    loop {
        match SerialReader::new(&port, baudrate) {
            Ok(mut reader) => {
                log::info!("Listening on {}", reader.port.name().unwrap_or_default());
                if let Err(e) = reader.run().await {
                    warn!("Connection lost: {e}");
                }
            }
            Err(e) => {
                warn!("Failed to open {port}: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Owns a [`SerialStream`] and an internal read buffer.
#[derive(Debug)]
pub struct SerialReader {
    port: SerialStream,
    buf: Buffer,
}

impl SerialReader {
    pub fn new(port: &String, baudrate: u32) -> Result<Self> {
        let port = SerialStream::open(&tokio_serial::new(port, baudrate))?;

        info!("Opened serial port '{}'", port.name().unwrap_or_default());
        Ok(SerialReader {
            port,
            buf: Buffer::new(),
        })
    }

    /// Reads continuously until an error occurs or no data arrives for 5 seconds.
    ///
    /// Any `Err` signals `serial_port_task` to drop the port and reconnect.
    pub async fn run(&mut self) -> Result<()> {
        loop {
            match timeout(Duration::from_secs(5), self.read()).await {
                Ok(Ok(0)) => continue,
                Ok(Ok(n)) => {
                    debug!("Received {} bytes: {:02x?}", n, self.buffer().last_n(n));
                    self.buffer_mut().clear();
                }
                Ok(Err(e)) => {
                    warn!("Error: {e}");
                    return Err(e);
                }
                Err(_) => {
                    warn!("Timeout");
                    return Err(anyhow::anyhow!("No data received for 5 seconds"));
                }
            }
        }
    }

    /// Reads available bytes into the internal buffer.
    ///
    /// `Ok(0)` from the underlying async read means EOF (device disconnected),
    /// so it is promoted to `Err` rather than silently treated as "no data".
    pub async fn read(&mut self) -> Result<usize> {
        let mut tmp = [0u8; 64];
        match self.port.read(&mut tmp).await {
            Ok(0) => Err(anyhow::anyhow!("Serial port closed (EOF)")),
            Ok(n) => {
                self.buf.extend_from_slice(&tmp[..n]);
                Ok(n)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buf
    }

    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buf
    }
}

impl Drop for SerialReader {
    fn drop(&mut self) {
        warn!(
            "Dropping serial port '{}'",
            self.port.name().unwrap_or_default()
        );
    }
}

/// Growable byte buffer for accumulating serial reads before processing.
#[derive(Debug, Clone, PartialEq)]
pub struct Buffer {
    buf: Vec<u8>,
}

impl Buffer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn extend_from_slice(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn last_n(&self, n: usize) -> &[u8] {
        &self.buf[self.buf.len() - n..]
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}
