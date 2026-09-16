// Protocol multiplexer: exposes a TCP port for many downstream clients and
// forwards their RCT frames over ONE inverter connection, serialized.
//
// Feature-gated behind `async`. The inverter accepts exactly one protocol
// client, so downstream requests are queued through a single upstream socket
// (one exchange per lock hold keeps request/response pairing correct). The
// upstream socket is kept open and re-established only after an error.
//
// Downstream wire format is the plain RCT protocol, so any RCT client can be
// pointed at the proxy port. Frames are forwarded byte-transparent (wire
// bytes as received, escaping included).
//
// SAFETY: writes forwarded through the proxy change plant behaviour — all
// risk lies with the operator.

use std::sync::Arc;
use std::time::Instant;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::client::ClientConfig;
use crate::error::RctError;
use crate::frame::ReceiveFrame;

pub struct Proxy {
    listener: TcpListener,
    inverter_addr: String,
    cfg: ClientConfig,
    upstream: Arc<Mutex<Option<TcpStream>>>,
}

impl Proxy {
    /// Bind the downstream listener on `cfg.port`; `inverter_addr` is
    /// host[:port] of the inverter (falls back to `DEFAULT_PORT`).
    pub async fn bind(
        inverter_addr: impl Into<String>,
        cfg: ClientConfig,
    ) -> Result<Proxy, RctError> {
        let listener = TcpListener::bind(("0.0.0.0", cfg.port)).await?;
        Ok(Proxy {
            listener,
            inverter_addr: inverter_addr.into(),
            cfg,
            upstream: Arc::new(Mutex::new(None)),
        })
    }

    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.listener.local_addr().expect("bound listener")
    }

    /// Accept-loop: one task per downstream client, serialized upstream.
    pub async fn serve(self) -> Result<(), RctError> {
        let Self { listener, inverter_addr, cfg, upstream } = self;
        loop {
            let (sock, _peer) = listener.accept().await?;
            let upstream = upstream.clone();
            let addr = inverter_addr.clone();
            let cfg = cfg.clone();
            tokio::spawn(async move {
                println!("Client Connected");
                let _ = serve_client(sock, upstream, addr, cfg).await;
            });
        }
    }
}

/// Run one request frame through the shared upstream connection and return
/// the raw response frame bytes. `conn` is the pooled upstream socket.
async fn exchange(
    upstream: &Arc<Mutex<Option<TcpStream>>>,
    addr: &str,
    cfg: &ClientConfig,
    wire_request: &[u8],
) -> Result<Vec<u8>, RctError> {
    let mut guard = upstream.lock().await;
    if guard.is_none() {
        let sock = TcpStream::connect(addr).await?;
        *guard = Some(sock);
    }
    let sock = guard.as_mut().expect("upstream connected");

    let result = exchange_on(sock, wire_request, cfg).await;
    if result.is_err() {
        // unknown state -> reconnect on next request
        if let Some(s) = guard.take() {
            drop(s);
        }
    }
    result
}

async fn exchange_on(
    sock: &mut TcpStream,
    wire_request: &[u8],
    cfg: &ClientConfig,
) -> Result<Vec<u8>, RctError> {
    tokio::time::timeout(cfg.timeout, sock.write_all(wire_request))
        .await
        .map_err(|_| RctError::Timeout)??;

    let mut rx = ReceiveFrame::new(false);
    let mut buf = [0u8; 256];
    let deadline = Instant::now() + cfg.timeout;
    while !rx.complete() {
        if Instant::now() > deadline {
            return Err(RctError::Timeout);
        }
        let n = tokio::time::timeout(
            deadline.saturating_duration_since(Instant::now()),
            sock.read(&mut buf),
        )
        .await
        .map_err(|_| RctError::Timeout)??;
        if n == 0 {
            return Err(RctError::Timeout);
        }
        rx.consume(&buf[..n])?;
    }
    Ok(rx.wire_bytes())
}

async fn serve_client(
    mut sock: TcpStream,
    upstream: Arc<Mutex<Option<TcpStream>>>,
    addr: String,
    cfg: ClientConfig,
) -> Result<(), RctError> {
    let mut rx = ReceiveFrame::new(false);
    let mut buf = [0u8; 1024];
    loop {
        let n = sock.read(&mut buf).await?;
        if n == 0 {
            return Ok(()); // downstream disconnected
        }
        // feed byte-by-byte so every complete frame is forwarded whole; wire
        // bytes per frame are tracked via a parallel raw buffer
        let mut frame_start: Option<usize> = None;
        for (i, &c) in buf[..n].iter().enumerate() {
            if rx.no_bytes() {
                frame_start = Some(i);
            }
            match rx.consume(std::slice::from_ref(&c)) {
                Ok(_) if rx.complete() => {
                    let wire = buf[frame_start.take().unwrap()..=i].to_vec();
                    match exchange(&upstream, &addr, &cfg, &wire).await {
                        Ok(resp) => {
                            if sock.write_all(&resp).await.is_err() {
                                return Ok(()); // downstream gone
                            }
                        }
                        Err(_) => return Err(RctError::Timeout),
                    }
                    rx = ReceiveFrame::new(false);
                }
                Ok(_) => {}
                Err(_) => {
                    // desync within this chunk; re-sync on next start token
                    rx = ReceiveFrame::new(false);
                    frame_start = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::make_frame;
    use crate::types::{Command, DataType, DataValue, FrameType};

    // Proxy in front of the python simulator: two sequential downstream
    // clients plus interleaved traffic through the single upstream conn.
    #[tokio::test]
    #[ignore = "requires python3 simulator"]
    async fn proxy_multiplexes_two_downstream_clients() {
        let (sim_port, mut child) = start_simulator().await;
        let cfg = ClientConfig {
            port: 0, // ephemeral downstream port
            timeout: std::time::Duration::from_secs(5),
            retries: 1,
            retry_delay: std::time::Duration::from_millis(100),
        };
        let proxy = Proxy::bind(format!("127.0.0.1:{sim_port}"), cfg)
            .await
            .expect("bind proxy");
        let proxy_port = proxy.local_addr().port();
        let handle = tokio::spawn(proxy.serve());

        let read_req =
            make_frame(Command::Read, 0x959930bf, &[], 0, FrameType::Standard).unwrap();
        let write_payload = 0.87f32.to_be_bytes();
        let write_req =
            make_frame(Command::Write, 0x959930bf, &write_payload, 0, FrameType::Standard)
                .unwrap();

        // two downstream connections used concurrently (interleaved)
        let mut c1 = TcpStream::connect(("127.0.0.1", proxy_port)).await.unwrap();
        let mut c2 = TcpStream::connect(("127.0.0.1", proxy_port)).await.unwrap();

        c1.write_all(&read_req).await.unwrap();
        c2.write_all(&write_req).await.unwrap();

        assert_eq!(read_float(&mut c1).await, DataValue::F32(0.42));
        assert_eq!(read_float(&mut c2).await, DataValue::F32(0.87));
        // second read on c1 reuses the single upstream connection
        c1.write_all(&read_req).await.unwrap();
        assert_eq!(read_float(&mut c1).await, DataValue::F32(0.42));

        handle.abort();
        let _ = child.kill().await;
    }

    async fn read_float(sock: &mut TcpStream) -> DataValue {
        let mut rx = ReceiveFrame::new(false);
        let mut buf = [0u8; 256];
        while !rx.complete() {
            let n = sock.read(&mut buf).await.unwrap();
            assert!(n > 0, "proxy closed connection");
            rx.consume(&buf[..n]).unwrap();
        }
        crate::codec::decode_value(DataType::Float, rx.data()).unwrap()
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
        let line = lines.next_line().await.unwrap().unwrap();
        let port: u16 = line.trim().strip_prefix("PORT ").unwrap().parse().unwrap();
        (port, child)
    }
}
