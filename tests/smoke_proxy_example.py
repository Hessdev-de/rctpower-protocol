#!/usr/bin/env python3
"""Smoke test: examples/rct_proxy CLI binary in front of tests/simulator.py.

Starts the simulator, starts the proxy example on an ephemeral port pointing at
it, reads battery.soc through the proxy with a raw client (parsed via
python-rctclient), and once through the rct example CLI itself.
"""
import socket, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SIM = ROOT / "tests" / "simulator.py"
BIN = ROOT / "target" / "debug" / "examples" / "rct_proxy"
RCT = ROOT / "target" / "debug" / "examples" / "rct"

sys.path.insert(0, str(ROOT / "vendor" / "python-rctclient" / "src"))
from rctclient.frame import ReceiveFrame, make_frame          # noqa: E402
from rctclient.types import Command, FrameType                 # noqa: E402

sim = subprocess.Popen([sys.executable, str(SIM)], stdout=subprocess.PIPE, text=True)
sim_port = int(sim.stdout.readline().split()[1])

proxy = subprocess.Popen(
    [str(BIN), "--port", "18899", "--host", "127.0.0.1", "--inverter-port", str(sim_port)],
    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
print("proxy:", proxy.stdout.readline().strip())

req = make_frame(Command.READ, 0x959930BF, b"", 0, FrameType.STANDARD)

def read(sock):
    sock.settimeout(8)
    sock.sendall(req)
    rx = ReceiveFrame()
    while not rx.complete():
        b = sock.recv(1024)
        assert b, "proxy closed the connection"
        rx.consume(b)
    from rctclient.utils import decode_value
    from rctclient.types import DataType
    return decode_value(DataType.FLOAT, rx.data)

for i in (1, 2):
    c = socket.create_connection(("127.0.0.1", 18899))
    v = read(c)
    print(f"client{i} through proxy: {v}")
    assert abs(v - 0.42) < 1e-6, f"client{i}: wrong value {v}"
    c.close()

# through the rct example CLI (uses default port 8899 -> point proxy there too)
proxy.kill()
proxy = subprocess.Popen(
    [str(BIN), "--port", "8899", "--host", "127.0.0.1", "--inverter-port", str(sim_port)],
    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
proxy.stdout.readline()
out = subprocess.run([str(RCT), "get", "battery.soc", "--host", "127.0.0.1"],
                     capture_output=True, text=True, timeout=10)
print("rct-cli-through-proxy:", out.stdout.strip() or out.stderr.strip())
assert "0.42" in out.stdout, "CLI through proxy failed"

print("SMOKE OK")
proxy.kill(); sim.kill()
