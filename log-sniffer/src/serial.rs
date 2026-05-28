use crate::ConnectionEvent;
use anyhow::Result;
use std::ops::{Deref, DerefMut};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};
use tokio_serial::{SerialPort, SerialStream};
use tracing::{debug, info, instrument, warn};

/// Runs forever: opens the serial port, reads until the connection is lost, then reconnects.
///
/// On each (re)open, sends [`ConnectionEvent::Reconnected`] so the decoder can reset its
/// defmt stream state before the first bytes of the new connection arrive.
///
/// Retries every second on open failure or any read error (including EOF and the
/// 5-second silence timeout). Intended to be run inline in a `tokio::select!`.
#[instrument(skip(tx))]
pub async fn serial_port_task(port: String, baudrate: u32, tx: mpsc::Sender<ConnectionEvent>) {
    loop {
        match SerialReader::new(&port, baudrate) {
            Ok(mut reader) => {
                info!("Listening on {}", reader.port_name);
                let _ = tx.send(ConnectionEvent::Reconnected).await;
                if let Err(e) = reader.run(&tx).await {
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

/// Owns an async serial port and accumulates bytes into an internal [`Buffer`].
///
/// Generic over `P` so that tests can substitute a mock reader without a real serial port.
#[derive(Debug)]
pub struct SerialReader<P = SerialStream> {
    port: P,
    port_name: String,
    buf: Buffer,
}

impl SerialReader<SerialStream> {
    #[instrument(skip(port, baudrate))]
    pub fn new(port: &str, baudrate: u32) -> Result<Self> {
        let stream = SerialStream::open(&tokio_serial::new(port, baudrate))?;

        stream.clear(tokio_serial::ClearBuffer::Input)?;
        debug!("Discard stale kernel data");

        let port_name = stream.name().unwrap_or_default();
        info!("Opened serial port '{}'", port_name);

        Ok(SerialReader {
            port: stream,
            port_name,
            buf: Buffer::new(),
        })
    }
}

impl<P: AsyncRead + Unpin> SerialReader<P> {
    /// Reads continuously until an error occurs or no data arrives for 5 seconds.
    ///
    /// Any `Err` signals `serial_port_task` to drop the port and reconnect.
    pub async fn run(&mut self, output_channel: &mpsc::Sender<ConnectionEvent>) -> Result<()> {
        loop {
            match timeout(Duration::from_secs(5), self.read()).await {
                Ok(Ok(0)) => continue,
                Ok(Ok(n)) => {
                    debug!("Received {} bytes: {:02x?}", n, self.buffer().last_n(n));
                    output_channel
                        .send(ConnectionEvent::Data(self.buffer().as_slice().to_vec()))
                        .await?;
                    self.buffer_mut().clear();
                }
                Ok(Err(e)) => {
                    warn!("Error: {e}");
                    return Err(e);
                }
                Err(_) => {
                    warn!("No data received for 5 seconds");
                    return Err(anyhow::anyhow!("timeout"));
                }
            }
        }
    }

    /// Reads available bytes into the internal buffer.
    ///
    /// ## Returns
    ///
    /// The number of bytes read.
    ///
    /// `Ok(0)` from the underlying async read means EOF (device disconnected),
    /// so it is promoted to `Err` rather than silently treated as "no data".
    pub async fn read(&mut self) -> Result<usize> {
        let mut temp_buffer = [0u8; 4096];
        match self.port.read(&mut temp_buffer).await {
            Ok(0) => Err(anyhow::anyhow!("Serial port closed (EOF)")),
            Ok(n) => {
                self.buf.extend_from_slice(&temp_buffer[..n]);
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

impl<P> Drop for SerialReader<P> {
    fn drop(&mut self) {
        warn!("Dropping serial port '{}'", self.port_name);
    }
}

#[cfg(test)]
impl<P> SerialReader<P> {
    fn with_port(port: P) -> Self {
        SerialReader {
            port,
            port_name: "test".to_string(),
            buf: Buffer::new(),
        }
    }
}

/// Growable byte buffer for accumulating serial reads before processing.
#[derive(Debug, Clone, PartialEq)]
pub struct Buffer(Vec<u8>);

impl Deref for Buffer {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Buffer {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Returns a slice of the last `n` bytes. Panics if `n > self.len()`.
    pub fn last_n(&self, n: usize) -> &[u8] {
        &self.0[self.len() - n..]
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod test_buffer {
        use super::*;

        #[test]
        fn when_last_n_returns_tail() {
            // Given
            let mut buf = Buffer::new();

            // When
            buf.extend_from_slice(&[1, 2, 3, 4, 5]);

            // Then
            assert_eq!(buf.last_n(3), &[3, 4, 5]);
        }

        #[test]
        fn when_emptying_buffer() {
            // Given
            let mut buf = Buffer::new();
            buf.extend_from_slice(&[1, 2, 3]);

            // When
            buf.clear();

            // then
            assert_eq!(buf.as_slice(), &[] as &[u8]);
        }
    }

    mod test_serial_reader {
        use super::*;
        use anyhow::Context;

        #[tokio::test]
        async fn when_read_receives_eof() {
            // Given
            let mock = tokio_test::io::Builder::new().build(); // immediately EOF
            let mut reader = SerialReader::with_port(mock);

            // When
            let result = reader.read().await;

            // Then
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn when_read_receives_data() {
            // Given
            let mock = tokio_test::io::Builder::new().read(b"\x01\x02\x03").build();
            let mut reader = SerialReader::with_port(mock);

            // When
            let result = reader.read().await;

            // Then
            assert_eq!(result.unwrap(), 3);
            assert_eq!(reader.buffer().as_slice(), &[0x01, 0x02, 0x03]);
        }

        #[tokio::test]
        async fn when_run_receives_bytes() {
            // Given
            let mock = tokio_test::io::Builder::new().read(b"\x01\x02\x03").build();
            let mut reader = SerialReader::with_port(mock);
            let (tx, mut rx) = mpsc::channel(8);

            // When
            let _ = reader.run(&tx).await.context("Failed to run reader");

            // Then
            let received = rx
                .recv()
                .await
                .context("Failed to receive message")
                .unwrap();
            let received_data = match received {
                ConnectionEvent::Data(data) => data,
                ConnectionEvent::Reconnected => panic!("Did not expect reconnection"),
            };
            assert_eq!(received_data, vec![0x01, 0x02, 0x03]);
        }
    }
}
