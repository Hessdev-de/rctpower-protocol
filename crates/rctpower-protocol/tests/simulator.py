#!/usr/bin/env python3
"""Tiny inverter simulator for rct-core integration tests.

Listens on an ephemeral port, prints "PORT <n>" once ready, then answers each
Read/Write request frame with a Response frame carrying a fixed float payload.
Uses the vendored python-rctclient framing so the Rust side is tested against
the reference implementation itself.
"""

import socket
import struct
import sys
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent.parent / "vendor" / "python-rctclient" / "src"))

from rctclient.frame import make_frame  # noqa: E402
from rctclient.types import Command, DataType, FrameType  # noqa: E402
from rctclient.utils import CRC16, decode_value  # noqa: E402

RESPONSE_FLOAT = 0.42


def parse_request(data: bytes):
    """Minimal request parse: unescape after start token, read id."""
    assert data[0:1] == b"+"
    raw = bytearray()
    esc = False
    for b in data[1:]:
        if b == 0x2D and not esc:
            esc = True
            continue
        esc = False
        raw.append(b)
    command = raw[0]
    oid = struct.unpack(">I", bytes(raw[2:6]))[0]
    payload = bytes(raw[6:-2])
    return command, oid, payload


def handle(sock: socket.socket) -> None:
    data = sock.recv(1024)
    if not data:
        return
    command, oid, payload = parse_request(data)
    if command == Command.WRITE:
        val = decode_value(DataType.FLOAT, payload)
    else:
        val = RESPONSE_FLOAT
    resp = make_frame(Command.RESPONSE, oid, struct.pack(">f", float(val)), 0, FrameType.STANDARD)
    sock.sendall(resp)
    sock.close()


def main() -> int:
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", 0))
    srv.listen(4)
    print(f"PORT {srv.getsockname()[1]}", flush=True)
    import time
    deadline = time.time() + 30
    while time.time() < deadline:
        srv.settimeout(1.0)
        try:
            c, _ = srv.accept()
        except socket.timeout:
            continue
        threading.Thread(target=handle, args=(c,), daemon=True).start()
    return 0


if __name__ == "__main__":
    sys.exit(main())
