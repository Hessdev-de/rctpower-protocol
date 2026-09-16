# RUST implementation to work with RCT inverters

I own a RCT invertet and used Home Assistant with the RCT Power Integration [rct-power-integration]
but wanted to build my hems on RUST with [energy2mqtt].

This code base is something like a wrapper to [python-rctclient] and [rctpower-writesupport] and
implements a clean interface to RUST applications.

The code contains an example client to read and set fields. The client connect to the inverter
to port 8899.

> [!WARNING]
> This library is provided as is, I do not take responsabilities for any issues or errors. Changing
> parameters can lead to major issues, kill your cat or burn down your house.
> !!! Do not use unless you know what you are doing !!!


## Client implementations

Two clients are available:

- `client::Client` — synchronous. Keeps the connection open for its lifetime and
  closes it on `drop()`; reconnects only after an error.
- `async_client::AsyncClient` — tokio-based, feature `async`. Same connection
  handling: kept open, closed on `drop()`, reconnect-on-error.

As standalone tool (example, feature `cli`):

```
cargo run --features cli --example rct_proxy -- --port 18899 --host 192.168.1.50 [--inverter-port 8899]
```

As Docker container (GitHub Actions builds `linux/amd64` + `linux/arm64/v8` → GHCR):

```
docker run --rm -p 18899:18899 ghcr.io/Hessdev-de/rctpower-protocol:main \
  --port 18899 --host 192.168.1.50
```

```toml
[dependencies]
rctpower_protocol = { version = "0.0.1", features = ["async"] }
```

## Proxy usage

This crate contains a proxy (if build with async), which Listens on a TCP port and multiplexes
multiple downstream clients onto the single inverter connection (requests are serialized
and forwarded as plain protocol frames, so any RCT client can connect to the proxy port).


## Updating from mainline python implementation


1. `git submodule update --remote vendor/python-rctclient` — update to the latest upstream update 
2. `python3 tools/generate_registry.py`— build a new registry.csv from upstream
3. `git diff data/registry.csv vendor/python-rctclient` — review the data-only
   delta plus the new pinned submodule commit
4. `cargo test` — golden vectors + conformance must work after the update
5. `git commit` — pins the upstream commit via the submodule gitlink


In most cases no updates to the Rust code should be needed. If RCT or python mainline add new DataTypes
or ObjectGroup entries, we need to handle those in `types.rs`

## Testing

```
cargo test                                  # unit + golden vectors
cargo test -- --ignored                     # e2e vs python simulator
```

Golden vectors taken from python-rctclient tests. Thank you guys!

## Safety

- The inverter serves exactly ONE protocol client.
  Close RCT app / Home Assistant / OpenWB / EVCC before writing.
- `rct set` refuses to write without `--yes` (stricter than rct.py).
- Values are validated against the rct.py ruleset before sending.
- All risk from writes lies with the operator.

## License

GPL-3.0-only (derived from python-rctclient, GPL-3.0; write rules from
rctpower_writesupport, MIT — see NOTICE).

---
[energy2mqtt]: https://energy2mqtt.org
[python-rctclient]: https://github.com/svalouch/python-rctclient
[rctpower-writesupport]: https://github.com/do-gooder/rctpower_writesupport
[rct-power-integration]: https://github.com/weltenwort/home-assistant-rct-power-integration/
