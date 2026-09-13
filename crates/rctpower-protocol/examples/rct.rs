//! Example CLI: rctpower-protocol get/set/params (feature-gated: `cli`).
//!
//! Run with:
//!   cargo run --features cli --example rct -- params
//!   cargo run --features cli --example rct -- get battery.soc --host 192.168.1.50
//!   cargo run --features cli --example rct -- set power_mng.soc_strategy 2 --host 192.168.1.50 --yes
//!
//! SAFETY: writes change plant behaviour. The inverter allows only ONE client —
//! disconnect RCT app / HA integration / OpenWB / EVCC first. Use at own risk.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use rctpower_protocol::client::{Client, ClientConfig};
use rctpower_protocol::registry::registry;
use rctpower_protocol::writable;

#[derive(Parser)]
#[command(name = "rct", about = "RCT Power inverter control (experimental, use at own risk)")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Read a parameter from the inverter
    Get { parameter: String, #[arg(long)] host: String },
    /// Write a parameter (validates against the known rule set)
    Set {
        parameter: String,
        value: String,
        #[arg(long)]
        host: String,
        /// Required confirmation for writes (safety gate)
        #[arg(long)]
        yes: bool,
    },
    /// List writable parameters and their validation rules
    Params,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let reg = registry();
    match cli.command {
        Cmd::Params => {
            println!("{:<34} {:>10}  {}", "PARAMETER", "RANGE", "DECIMALS");
            for r in writable::WRITABLE {
                let range = if r.min.is_nan() {
                    "bool".to_string()
                } else {
                    format!("{}..{}", r.min, r.max)
                };
                println!("{:<34} {:>10}  {}", r.name, range, r.decimals);
            }
        }
        Cmd::Get { parameter, host } => {
            let obj = reg
                .get_by_name(&parameter)
                .ok_or_else(|| anyhow::anyhow!("Error: Invalid parameter '{parameter}'"))?;
            let client = Client::new(host.clone(), ClientConfig::default());
            match client.read(obj) {
                Ok(v) => println!("*** READ SUCCESS: {parameter} = {v}"),
                Err(e) => println!("### ERROR ### Failed to read parameter '{parameter}': {e}"),
            }
        }
        Cmd::Set { parameter, value, host, yes } => {
            if !yes {
                bail!("refusing to write without --yes (safety gate). Re-run with --yes to confirm.");
            }
            let obj = reg
                .get_by_name(&parameter)
                .ok_or_else(|| anyhow::anyhow!("Error: Invalid parameter '{parameter}'"))?;
            let v = writable::parse_value(&parameter, &value)?;
            writable::validate(&parameter, &v)?;
            let client = Client::new(host.clone(), ClientConfig::default());
            client.write(obj, &v)?;
            println!("*** SET SUCCESS: {parameter} = {v} on {host}");
        }
    }
    Ok(())
}
