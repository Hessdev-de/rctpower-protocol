//! End-to-end test: Rust client against the Python reference simulator
//! (tests/simulator.py), which frames responses with python-rctclient itself.
//!
//! Ignored by default (needs python3); run with:
//!   cargo test -p rct-core -- --ignored e2e

use std::io::{BufRead, BufReader};
use std::process::{Command as ProcCommand, Stdio};
use std::time::Duration;

use rctpower_protocol::client::{Client, ClientConfig};
use rctpower_protocol::registry::registry;
use rctpower_protocol::types::DataValue;

fn start_simulator() -> (u16, std::process::Child) {
    let sim = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/simulator.py");
    let mut child = ProcCommand::new("python3")
        .arg(&sim)
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn simulator");
    let stdout = child.stdout.take().expect("stdout piped");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read PORT line");
    let port: u16 = line
        .trim()
        .strip_prefix("PORT ")
        .expect("PORT line")
        .parse()
        .expect("port number");
    (port, child)
}

#[test]
#[ignore = "requires python3 simulator"]
fn e2e_read_float() {
    let (port, mut child) = start_simulator();
    let cfg = ClientConfig {
        port,
        timeout: Duration::from_secs(5),
        retries: 1,
        retry_delay: Duration::from_millis(100),
    };
    let client = Client::new("127.0.0.1", cfg);
    let soc = registry().get_by_name("battery.soc").unwrap();
    let v = client.read(soc).expect("read from simulator");
    assert_eq!(v, DataValue::F32(0.42));
    // second read must reuse the pooled connection (simulator loops per connection)
    let v2 = client.read(soc).expect("read on kept-alive conn");
    assert_eq!(v2, DataValue::F32(0.42));
    let _ = child.kill();
}

#[test]
#[ignore = "requires python3 simulator"]
fn e2e_write_float_roundtrip() {
    let (port, mut child) = start_simulator();
    let cfg = ClientConfig {
        port,
        timeout: Duration::from_secs(5),
        retries: 1,
        retry_delay: Duration::from_millis(100),
    };
    let client = Client::new("127.0.0.1", cfg);
    let soc = registry().get_by_name("battery.soc").unwrap();
    // simulator echoes whatever float we send back in a RESPONSE frame
    let v = client.write(soc, &DataValue::F32(0.87)).expect("write to simulator");
    assert_eq!(v, DataValue::F32(0.87));
    let _ = child.kill();
}
