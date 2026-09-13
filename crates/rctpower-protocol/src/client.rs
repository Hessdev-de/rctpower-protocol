// Minimal sync TCP client for the RCT Power serial protocol (port 8899).
//
// SAFETY: the inverter accepts exactly one protocol client at a time. Ensure no
// other connection (RCT app, HA integration, OpenWB, EVCC, ...) is active while
// writing. Writes change plant behaviour — all risk lies with the operator.

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
}

impl Client {
    pub fn new(host: impl Into<String>, cfg: ClientConfig) -> Self {
        Client { addr: host.into(), cfg }
    }

    /// Read an object (one-shot connection, like rct.py get).
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

    fn exchange(&self, frame: &[u8], resp_type: DataType) -> Result<DataValue, RctError> {
        let mut last_err = RctError::Timeout;
        for _ in 0..self.cfg.retries.max(1) {
            match self.try_exchange(frame, resp_type) {
                Ok(v) => return Ok(v),
                Err(e) => last_err = e,
            }
            std::thread::sleep(self.cfg.retry_delay);
        }
        Err(last_err)
    }

    fn try_exchange(&self, frame: &[u8], resp_type: DataType) -> Result<DataValue, RctError> {
        let sock_addr = (self.addr.as_str(), self.cfg.port)
            .to_socket_addrs()?
            .next()
            .ok_or(RctError::Timeout)?;
        let mut sock = TcpStream::connect(sock_addr)?;
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
        decode_value(resp_type, rx.data())
    }
}
