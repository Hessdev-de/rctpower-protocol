// Async TCP client for the RCT Power serial protocol (port 8899), tokio-based.
// Feature-gated behind `async` (and used by the `cli` example).
//
// SAFETY: the inverter accepts exactly one protocol client at a time. Ensure no
// other connection (RCT app, HA integration, OpenWB, EVCC, ...) is active while
// writing. Writes change plant behaviour — all risk lies with the operator.
//
// Unlike the sync client (one-shot connection per call), this client keeps the
// connection open across calls (connection pooling style, like python-rctclient's
// Transceiver). The inverter may drop idle connections, so reconnect-on-error is
// built in.

use std::sync::Mutex;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::client::ClientConfig;
use crate::codec::{decode_value, encode_for};
use crate::error::RctError;
use crate::frame::{make_frame, ReceiveFrame};
use crate::registry::ObjectInfo;
use crate::types::{Command, DataValue, DataType};

pub use crate::client::DEFAULT_PORT;

pub struct AsyncClient {
    addr: String,
    cfg: ClientConfig,
    /// Held open between calls; reconnected on demand.
    conn: Mutex<Option<TcpStream>>,
}

impl AsyncClient {
    pub fn new(host: impl Into<String>, cfg: ClientConfig) -> Self {
        AsyncClient {
            addr: host.into(),
            cfg,
            conn: Mutex::new(None),
        }
    }

    /// Read an object.
    pub async fn read(&self, obj: &ObjectInfo) -> Result<DataValue, RctError> {
        let frame = make_frame(
            Command::Read,
            obj.object_id,
            &[],
            0,
            crate::types::FrameType::Standard,
        )?;
        self.exchange(&frame, obj.response_data_type).await
    }

    /// Write a value; the device answers with the stored value.
    pub async fn write(&self, obj: &ObjectInfo, value: &DataValue) -> Result<DataValue, RctError> {
        let payload = encode_for(obj.request_data_type, value)?;
        let frame = make_frame(
            Command::Write,
            obj.object_id,
            &payload,
            0,
            crate::types::FrameType::Standard,
        )?;
        self.exchange(&frame, obj.response_data_type).await
    }

    async fn exchange(&self, frame: &[u8], resp_type: DataType) -> Result<DataValue, RctError> {
        let mut last_err = RctError::Timeout;
        for _ in 0..self.cfg.retries.max(1) {
            match self.try_exchange(frame, resp_type).await {
                Ok(v) => return Ok(v),
                Err(e) => last_err = e,
            }
            tokio::time::sleep(self.cfg.retry_delay).await;
        }
        Err(last_err)
    }

    async fn try_exchange(&self, frame: &[u8], resp_type: DataType) -> Result<DataValue, RctError> {
        let mut guard = self.conn.lock().expect("async client conn lock");
        let sock = match guard.as_mut() {
            Some(sock) => sock,
            None => {
                let sock = tokio::time::timeout(
                    self.cfg.timeout,
                    TcpStream::connect((self.addr.as_str(), self.cfg.port)),
                )
                .await
                .map_err(|_| RctError::Timeout)?
                .map_err(RctError::from)?;
                guard.insert(sock)
            }
        };

        let result = Self::exchange_on(sock, frame, resp_type, self.cfg.timeout).await;
        if result.is_err() {
            // Connection may be in an unknown state; drop it so the next call reconnects.
            if let Some(sock) = guard.take() {
                drop(sock);
            }
        }
        result
    }

    async fn exchange_on(
        sock: &mut TcpStream,
        frame: &[u8],
        resp_type: DataType,
        timeout: Duration,
    ) -> Result<DataValue, RctError> {
        tokio::time::timeout(timeout, sock.write_all(frame))
            .await
            .map_err(|_| RctError::Timeout)??;

        let mut rx = ReceiveFrame::new(false);
        let mut buf = [0u8; 256];
        while !rx.complete() {
            let n = tokio::time::timeout(timeout, sock.read(&mut buf))
                .await
                .map_err(|_| RctError::Timeout)??;
            if n == 0 {
                return Err(RctError::Timeout);
            }
            rx.consume(&buf[..n])?;
        }
        if rx.data().is_empty() {
            return Err(RctError::EmptyPayload);
        }
        decode_value(resp_type, rx.data())
    }
}

impl Drop for AsyncClient {
    fn drop(&mut self) {
        // tokio sockets close their fd on drop; take it explicitly so the
        // connection is torn down when the client goes out of scope.
        if let Ok(mut guard) = self.conn.lock() {
            if let Some(sock) = guard.take() {
                drop(sock);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::registry;

    // End-to-end against the Python simulator (same vectors as the sync client).
    #[tokio::test]
    #[ignore = "requires python3 simulator"]
    async fn e2e_async_read_float() {
        let (port, mut child) = start_simulator().await;
        let cfg = ClientConfig {
            port,
            timeout: Duration::from_secs(5),
            retries: 1,
            retry_delay: Duration::from_millis(100),
        };
        let client = AsyncClient::new("127.0.0.1", cfg);
        let soc = registry().get_by_name("battery.soc").unwrap();
        let v = client.read(soc).await.expect("read from simulator");
        assert_eq!(v, DataValue::F32(0.42));
        // second read reuses the pooled connection
        let v2 = client.read(soc).await.expect("read on kept-alive conn");
        assert_eq!(v2, DataValue::F32(0.42));
        let _ = child.kill().await;
    }

    #[tokio::test]
    #[ignore = "requires python3 simulator"]
    async fn e2e_async_write_float_roundtrip() {
        let (port, mut child) = start_simulator().await;
        let cfg = ClientConfig {
            port,
            timeout: Duration::from_secs(5),
            retries: 1,
            retry_delay: Duration::from_millis(100),
        };
        let client = AsyncClient::new("127.0.0.1", cfg);
        let soc = registry().get_by_name("battery.soc").unwrap();
        let v = client
            .write(soc, &DataValue::F32(0.87))
            .await
            .expect("write to simulator");
        assert_eq!(v, DataValue::F32(0.87));
        let _ = child.kill().await;
    }

    async fn start_simulator() -> (u16, tokio::process::Child) {
        use tokio::io::AsyncBufReadExt;
        let sim = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/simulator.py");
        let mut child = tokio::process::Command::new("python3")
            .arg(&sim)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("spawn simulator");
        let stdout = child.stdout.take().expect("stdout piped");
        let mut lines = tokio::io::BufReader::new(stdout).lines();
        let line = lines
            .next_line()
            .await
            .expect("read PORT line")
            .expect("PORT line");
        let port: u16 = line
            .trim()
            .strip_prefix("PORT ")
            .expect("PORT line")
            .parse()
            .expect("port number");
        (port, child)
    }
}
