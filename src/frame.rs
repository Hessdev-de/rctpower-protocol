// Frame construction and incremental receive parsing.
//
// Ported from python-rctclient src/rctclient/frame.py,
// Copyright 2020 Peter Oberhofer (pob90), 2020-2026 Stefan Valouch (svalouch),
// SPDX-License-Identifier: GPL-3.0-only

use crate::codec::crc16;
use crate::error::RctError;
use crate::types::{hex, Command, FrameType};

/// Token that starts a frame ('+')
pub const START_TOKEN: u8 = b'+';
/// Token that escapes the next value ('-')
pub const ESCAPE_TOKEN: u8 = b'-';

/// Build the escaped byte stream for a frame, ready to send (make_frame).
/// `payload` is ignored for READ, `address` for STANDARD frames.
pub fn make_frame(
    command: Command,
    id: u32,
    payload: &[u8],
    address: u32,
    frame_type: FrameType,
) -> Result<Vec<u8>, RctError> {
    let _plant = Command::is_plant_byte(command.byte());
    let mut buf: Vec<u8> = Vec::new();
    buf.push(command.byte());
    if command.is_long() {
        buf.extend_from_slice(&((frame_type as u16 + payload.len() as u16).to_be_bytes()));
    } else {
        let len = frame_type as u8 + payload.len() as u8;
        buf.push(len);
    }
    if frame_type == FrameType::Plant {
        buf.extend_from_slice(&address.to_be_bytes());
    }
    // the object ID is always part of the frame
    buf.extend_from_slice(&id.to_be_bytes());
    // READ frames carry no payload
    if command != Command::Read {
        buf.extend_from_slice(payload);
    }
    buf.extend_from_slice(&crc16(&buf).to_be_bytes());

    let mut out: Vec<u8> = Vec::with_capacity(buf.len() + 8);
    out.push(START_TOKEN);
    for b in buf {
        if b == START_TOKEN || b == ESCAPE_TOKEN {
            out.push(ESCAPE_TOKEN);
        }
        out.push(b);
    }
    Ok(out)
}

/// Incremental receive-frame parser (one frame per instance).
#[derive(Debug)]
pub struct ReceiveFrame {
    complete: bool,
    crc_ok: bool,
    escaping: bool,
    crc16_recv: u16,
    frame_length: usize,
    command: Option<Command>,
    buffer: Vec<u8>,
    /// Raw wire bytes of the current frame INCLUDING start/escape tokens
    /// (consume() strips escapes from `buffer`, so wire transparency needs a
    /// parallel record — the proxy forwards frames byte-for-byte).
    raw: Vec<u8>,
    consumed_bytes: usize,
    frame_header_length: usize,
    id: u32,
    address: u32,
    data: Vec<u8>,
    ignore_crc_mismatch: bool,
}

impl ReceiveFrame {
    pub fn new(ignore_crc_mismatch: bool) -> Self {
        ReceiveFrame {
            complete: false,
            crc_ok: false,
            escaping: false,
            crc16_recv: 0,
            frame_length: 0,
            command: None,
            buffer: Vec::new(),
            raw: Vec::new(),
            consumed_bytes: 0,
            // start(1) + command(1) + length(1) + no address + id(4)
            frame_header_length: 1 + 1 + 1 + 4,
            id: 0,
            address: 0,
            data: Vec::new(),
            ignore_crc_mismatch,
        }
    }

    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn crc_ok(&self) -> bool {
        self.crc_ok
    }
    pub fn consumed_bytes(&self) -> usize {
        self.consumed_bytes
    }
    /// True when no bytes are buffered yet (parser at frame start).
    pub fn no_bytes(&self) -> bool {
        self.buffer.is_empty()
    }
    /// Raw wire bytes of the current frame, escaping included (as received).
    pub fn wire_bytes(&self) -> Vec<u8> {
        self.raw.clone()
    }
    pub fn command(&self) -> Option<Command> {
        self.command
    }
    pub fn id(&self) -> u32 {
        self.id
    }
    pub fn address(&self) -> u32 {
        self.address
    }
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    pub fn data_hex(&self) -> String {
        hex(&self.data)
    }

    /// Feed received bytes; returns consumed count. Errors carry the consumed
    /// offset so callers can drop those bytes and re-sync with a new parser.
    pub fn consume(&mut self, data: &[u8]) -> Result<usize, RctError> {
        let mut i = 0usize;
        for &c in data {
            self.consumed_bytes += 1;
            i += 1;

            // sync to start token
            if self.buffer.is_empty() {
                if c == START_TOKEN {
                    self.raw.clear();
                    self.raw.push(c);
                    self.buffer.push(c);
                }
                continue;
            }

            // every byte from the start token on belongs to the wire bytes,
            // escape tokens included (buffer strips them, raw must not)
            self.raw.push(c);

            if self.escaping {
                self.escaping = false;
            } else if c == ESCAPE_TOKEN {
                self.escaping = true;
                continue;
            }
            self.buffer.push(c);

            let blen = self.buffer.len();

            if blen == 2 {
                // command byte
                let raw = self.buffer[1];
                let cmd = Command::from_byte(raw)
                    .ok_or(RctError::InvalidCommand { cmd: raw, consumed: i })?;
                if cmd == Command::Extension {
                    return Err(RctError::InvalidCommand { cmd: raw, consumed: i });
                }
                self.command = Some(cmd);
                if Command::is_plant_byte(raw) {
                    self.frame_header_length += 4;
                }
                if cmd.is_long() {
                    self.frame_header_length += 1;
                }
            } else if blen == self.frame_header_length && self.command.is_some() && self.frame_length == 0 {
                let cmd = self.command.unwrap();
                let (data_length, oid_idx) = if cmd.is_long() {
                    (
                        u16::from_be_bytes([self.buffer[2], self.buffer[3]]) as usize,
                        4usize,
                    )
                } else {
                    (self.buffer[2] as usize, 3usize)
                };
                if Command::is_plant_byte(cmd.byte()) {
                    // length field includes address + id (8 bytes)
                    self.frame_length =
                        (self.frame_header_length - 8) + data_length + 2;
                    self.address = u32::from_be_bytes([
                        self.buffer[oid_idx],
                        self.buffer[oid_idx + 1],
                        self.buffer[oid_idx + 2],
                        self.buffer[oid_idx + 3],
                    ]);
                } else {
                    // length field includes id (4 bytes)
                    self.frame_length =
                        (self.frame_header_length - 4) + data_length + 2;
                }
                let oid_idx = if Command::is_plant_byte(cmd.byte()) { oid_idx + 4 } else { oid_idx };
                self.id = u32::from_be_bytes([
                    self.buffer[oid_idx],
                    self.buffer[oid_idx + 1],
                    self.buffer[oid_idx + 2],
                    self.buffer[oid_idx + 3],
                ]);
            } else if self.frame_length > 0 && blen == self.frame_length {
                self.complete = true;
                self.crc16_recv =
                    u16::from_be_bytes([self.buffer[blen - 2], self.buffer[blen - 1]]);
                let calc = crc16(&self.buffer[1..blen - 2]);
                self.crc_ok = self.crc16_recv == calc;
                let hdr = self.header_complete_at();
                self.data = self.buffer[hdr..blen - 2].to_vec();
                if !self.crc_ok && !self.ignore_crc_mismatch {
                    return Err(RctError::FrameCrcMismatch {
                        received: self.crc16_recv,
                        calculated: calc,
                        consumed: i,
                    });
                }
                return Ok(i);
            } else if self.frame_length > 0 && blen > self.frame_length {
                return Err(RctError::FrameLengthExceeded { consumed: i });
            }
        }
        Ok(i)
    }

    // The header-complete length is dynamic; helper keeps consume() readable.
    #[inline]
    fn header_complete_at(&self) -> usize {
        self.frame_header_length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes_from_hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    // Golden vectors from python tests/test_sendframe.py / test_bytecode_generation.py
    #[test]
    fn golden_write_standard_nopayload() {
        for (id_in, data_out) in
            [(0x0u32, "2b0204000000000c56"), (0xc0de, "2b02040000c0de30b1"), (0xffffffff, "2b0204ffffffff9599")]
        {
            let f = make_frame(Command::Write, id_in, &[], 0, FrameType::Standard).unwrap();
            assert_eq!(hex(&f), data_out);
        }
    }

    #[test]
    fn golden_read_standard_nopayload() {
        for (id_in, data_out) in
            [(0x0u32, "2b010400000000c2b6"), (0xc0de, "2b01040000c0defe51"), (0xffffffff, "2b0104ffffffff5b79")]
        {
            let f = make_frame(Command::Read, id_in, &[], 0, FrameType::Standard).unwrap();
            assert_eq!(hex(&f), data_out);
        }
    }

    #[test]
    fn golden_longresponse_standard_nopayload() {
        for (id_in, data_out) in
            [(0x0u32, "2b06000400000000b754"), (0xc0de, "2b0600040000c0dea78b"), (0xffffffff, "2b060004ffffffff6ac4")]
        {
            let f = make_frame(Command::LongResponse, id_in, &[], 0, FrameType::Standard).unwrap();
            assert_eq!(hex(&f), data_out);
        }
    }

    #[test]
    fn golden_response_standard_nopayload() {
        for (id_in, data_out) in
            [(0x0u32, "2b050400000000c417"), (0xc0de, "2b05040000c0def8f0"), (0xffffffff, "2b0504ffffffff5dd8")]
        {
            let f = make_frame(Command::Response, id_in, &[], 0, FrameType::Standard).unwrap();
            assert_eq!(hex(&f), data_out);
        }
    }

    #[test]
    fn read_frame_roundtrip() {
        // craft a response frame with float payload and parse it back
        let payload = 0.75f32.to_be_bytes();
        let frame = make_frame(Command::Response, 0x959930bf, &payload, 0, FrameType::Standard).unwrap();
        let mut rx = ReceiveFrame::new(false);
        rx.consume(&frame).unwrap();
        assert!(rx.complete());
        assert!(rx.crc_ok());
        assert_eq!(rx.id(), 0x959930bf);
        assert_eq!(rx.data(), &payload);
    }

    #[test]
    fn escape_tokens_are_unescaped_on_parse() {
        // id chosen so the frame contains a '+' (0x2b) byte in payload
        let payload = [0x2bu8, 0x2d, 0x01];
        let frame = make_frame(Command::Write, 0x11223344, &payload, 0, FrameType::Standard).unwrap();
        assert!(frame[1..].contains(&ESCAPE_TOKEN));
        let mut rx = ReceiveFrame::new(false);
        rx.consume(&frame).unwrap();
        assert!(rx.complete());
        assert_eq!(rx.data(), &payload);
    }

    #[test]
    fn crc_mismatch_error_carries_consumed() {
        let mut frame = bytes_from_hex("2b01040000c0defe51");
        frame[5] ^= 0xff; // corrupt id byte -> crc mismatch
        let mut rx = ReceiveFrame::new(false);
        let err = rx.consume(&frame).unwrap_err();
        assert!(matches!(err, RctError::FrameCrcMismatch { .. }));
    }

    #[test]
    fn garbage_before_start_is_skipped() {
        let mut stream = b"junk\xff\x00".to_vec();
        stream.extend(bytes_from_hex("2b01040000c0defe51"));
        let mut rx = ReceiveFrame::new(false);
        rx.consume(&stream).unwrap();
        assert!(rx.complete());
        assert_eq!(rx.id(), 0xc0de);
    }
}
