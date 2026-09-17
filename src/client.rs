// TCP client for the RCT Power serial protocol (port 8899).
//
// SAFETY: the inverter accepts exactly one protocol client at a time. Ensure no
// other connection (RCT app, HA integration, OpenWB, EVCC, ...) is active while
// writing. Writes change plant behaviour — all risk lies with the operator.
//
// Connection handling: the socket is kept open for the client's lifetime and is
// closed on drop(). Reconnects happen only when the inverter dropped the
// connection or an exchange failed.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::codec::{decode_value, encode_for};
use crate::error::RctError;
use crate::frame::{make_frame, ReceiveFrame};
use crate::registry::ObjectInfo;
use crate::types::{DataValue, DataType};

pub const DEFAULT_PORT: u16 = 8899;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub port: u16,
    pub timeout: Duration,
    pub retries: usize,
    pub retry_delay: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            port: DEFAULT_PORT,
            timeout: Duration::from_secs(5),
            retries: 3,
            retry_delay: Duration::from_secs(1),
        }
    }
}

pub struct Client {
    addr: String,
    cfg: ClientConfig,
    /// Held open for the client's lifetime; closed on drop().
    conn: RefCell<Option<TcpStream>>,
}

impl Client {
    pub fn new(host: impl Into<String>, cfg: ClientConfig) -> Self {
        Client {
            addr: host.into(),
            cfg,
            conn: RefCell::new(None),
        }
    }

    /// Read an object.
    pub fn read(&self, obj: &ObjectInfo) -> Result<DataValue, RctError> {
        let frame = make_frame(crate::types::Command::Read, obj.object_id, &[], 0, crate::types::FrameType::Standard)?;
        self.exchange(&frame, obj.response_data_type)
    }

    /// Write a value; the device answers with the stored value.
    pub fn write(&self, obj: &ObjectInfo, value: &DataValue) -> Result<DataValue, RctError> {
        let payload = encode_for(obj.request_data_type, value)?;
        let frame = make_frame(crate::types::Command::Write, obj.object_id, &payload, 0, crate::types::FrameType::Standard)?;
        self.exchange(&frame, obj.response_data_type)
    }

    /// Write a value and read it back.
    ///
    /// Real devices sometimes apply a WRITE but never answer it (the ack is
    /// simply lost — see python-rctclient docs). The value is still stored and
    /// readable on the next READ, so this method treats a write timeout as
    /// normal: it retries the write, then verifies via read-back. `Ok` means
    /// the device answered with the value OR the read-back matches; only if the
    /// read-back differs (or also times out) is it an error.
    pub fn write_and_verify(&self, obj: &ObjectInfo, value: &DataValue) -> Result<DataValue, RctError> {
        let payload = encode_for(obj.request_data_type, value)?;
        let frame = make_frame(crate::types::Command::Write, obj.object_id, &payload, 0, crate::types::FrameType::Standard)?;
        // try the normal path first: device may answer directly
        match self.try_exchange(&frame, obj.response_data_type) {
            Ok(v) => return Ok(v),
            Err(e) if matches!(e, RctError::Timeout | RctError::EmptyPayload) => {}
            Err(e) => return Err(e),
        }
        for attempt in 0..self.cfg.retries.max(1) {
            if attempt > 0 {
                std::thread::sleep(self.cfg.retry_delay);
                // plain send, tolerate lost acks
                let _ = self.try_exchange(&frame, obj.response_data_type);
            }
            match self.read(obj) {
                Ok(v) if value == &v => return Ok(v),
                Ok(_) => continue, // not applied yet (or wrong) — rewrite and re-read
                Err(_) => continue,
            }
        }
        Err(RctError::Timeout)
    }

    fn exchange(&self, frame: &[u8], resp_type: DataType) -> Result<DataValue, RctError> {
        let mut last_err = RctError::Timeout;
        for attempt in 0..self.cfg.retries.max(1) {
            if attempt > 0 {
                std::thread::sleep(self.cfg.retry_delay);
            }
            match self.try_exchange(frame, resp_type) {
                Ok(v) => return Ok(v),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    fn try_exchange(&self, frame: &[u8], resp_type: DataType) -> Result<DataValue, RctError> {
        let mut guard = self.conn.borrow_mut();
        let sock = match guard.as_mut() {
            Some(sock) => sock,
            None => {
                let sock_addr = (self.addr.as_str(), self.cfg.port)
                    .to_socket_addrs()?
                    .next()
                    .ok_or(RctError::Timeout)?;
                let sock = TcpStream::connect_timeout(&sock_addr, self.cfg.timeout)?;
                guard.insert(sock)
            }
        };
        sock.set_read_timeout(Some(self.cfg.timeout))?;
        sock.set_write_timeout(Some(self.cfg.timeout))?;
        sock.write_all(frame)?;

        let mut rx = ReceiveFrame::new(false);
        let mut buf = [0u8; 256];
        let deadline = Instant::now() + self.cfg.timeout;
        while !rx.complete() {
            if Instant::now() > deadline {
                return Err(RctError::Timeout);
            }
            match sock.read(&mut buf) {
                Ok(0) => return Err(RctError::Timeout),
                Ok(n) => {
                    rx.consume(&buf[..n])?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e.into()),
            }
        }
        if rx.data().is_empty() {
            return Err(RctError::EmptyPayload);
        }
        let value = decode_value(resp_type, rx.data());
        if value.is_err() {
            // unknown frame state -> force reconnect on next call
            *guard = None;
        }
        value
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(sock) = self.conn.get_mut() {
            let _ = sock.shutdown(std::net::Shutdown::Both);
        }
    }
}
