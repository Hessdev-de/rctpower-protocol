//! Example CLI: rctpower-protocol TCP proxy (feature-gated: `cli`).
//!
//! Listens on a local TCP port and multiplexes many downstream clients onto
//! the single inverter protocol connection (requests are serialized).
//!
//! Run with:
//!   cargo run --features cli --example rct_proxy -- --port 18899 --host 192.168.1.50
//!   cargo run --features cli --example rct_proxy -- --port 18899 --host 192.168.1.50 --inverter-port 8899
//!
//! SAFETY: the proxy forwards writes unchanged — they change plant behaviour.
//! The inverter allows only ONE protocol client, which is exactly why this
//! proxy exists: point RCT apps / HA / OpenWB / EVCC at the proxy port.

use anyhow::Result;
use clap::Parser;

use rctpower_protocol::client::ClientConfig;
use rctpower_protocol::proxy::Proxy;

#[derive(Parser)]
#[command(name = "rct_proxy", about = "RCT Power protocol TCP multiplexer")]
struct Cli {
    /// Local TCP port the proxy listens on (clients connect here)
    #[arg(long)]
    port: u16,
    /// Host of the real inverter
    #[arg(long)]
    host: String,
    /// TCP port of the inverter (default: 8899)
    #[arg(long, default_value_t = 8899)]
    inverter_port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = ClientConfig {
        port: cli.port,
        ..ClientConfig::default()
    };
    let proxy = Proxy::bind(format!("{}:{}", cli.host, cli.inverter_port), cfg).await?;
    println!(
        "proxy listening on :{} -> {}:{}",
        proxy.local_addr().port(),
        cli.host,
        cli.inverter_port
    );
    proxy.serve().await?;
    Ok(())
}
