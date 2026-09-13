// CRC16 (CCITT) and value encoding/decoding.
//
// Ported from python-rctclient src/rctclient/utils.py,
// Copyright 2020 Peter Oberhofer (pob90), 2020-2026 Stefan Valouch (svalouch),
// SPDX-License-Identifier: GPL-3.0-only

use crate::error::RctError;
use crate::types::DataValue;
use crate::types::DataType;

/// CRC16 CCITT (polynomial 0x1021, initial 0xFFFF).
/// Matches python-rctclient utils.CRC16 exactly, including the quirk that an
/// odd-length input gets a zero byte appended before calculation.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crcsum: u32 = 0xFFFF;
    let polynom: u32 = 0x1021;

    // skip start token quirk: pad odd input with 0
    let even_pad = data.len() & 0x01 == 1;
    let mut iter = data.iter().copied().chain(if even_pad { Some(0) } else { None });

    for byte in iter {
        crcsum ^= (byte as u32) << 8;
        for _ in 0..8 {
            crcsum <<= 1;
            if crcsum & 0x7FFF_0000 != 0 {
                crcsum = (crcsum & 0x0000_FFFF) ^ polynom;
            }
        }
    }
    crcsum as u16
}

/// Encode a value into wire bytes (big-endian), like utils.encode_value.
pub fn encode_value(value: &DataValue) -> Vec<u8> {
    match value {
        DataValue::Bool(b) => vec![*b as u8],
        DataValue::U8(v) => vec![*v],
        DataValue::I8(v) => vec![*v as u8],
        DataValue::U16(v) => v.to_be_bytes().to_vec(),
        DataValue::I16(v) => v.to_be_bytes().to_vec(),
        DataValue::U32(v) => v.to_be_bytes().to_vec(),
        DataValue::I32(v) => v.to_be_bytes().to_vec(),
        DataValue::F32(v) => v.to_be_bytes().to_vec(),
        DataValue::Str(s) => s.as_bytes().to_vec(),
        DataValue::Raw(b) => b.clone(),
    }
}

/// Convenience: encode a Rust-native value for a given wire data type,
/// like utils.encode_value(data_type=..., value=...).
pub fn encode_for(data_type: DataType, value: &DataValue) -> Result<Vec<u8>, RctError> {
    // Normalize into the wire representation of data_type
    let v = match data_type {
        DataType::Bool => DataValue::Bool(match value {
            DataValue::Bool(b) => *b,
            DataValue::I8(i) => *i != 0,
            DataValue::U8(i) => *i != 0,
            other => return Err(RctError::InvalidValue { name: String::new(), value: other.to_string() }),
        }),
        DataType::Uint8 | DataType::Enum => DataValue::U8(to_i64(value)? as u8),
        DataType::Int8 => DataValue::I8(to_i64(value)? as i8),
        DataType::Uint16 => DataValue::U16(to_i64(value)? as u16),
        DataType::Int16 => DataValue::I16(to_i64(value)? as i16),
        DataType::Uint32 => DataValue::U32(to_i64(value)? as u32),
        DataType::Int32 => DataValue::I32(to_i64(value)? as i32),
        DataType::Float => DataValue::F32(match value {
            DataValue::F32(f) => *f,
            other => to_i64(other).map(|i| i as f32)?,
        }),
        DataType::String => DataValue::Str(match value {
            DataValue::Str(s) => s.clone(),
            other => return Err(RctError::InvalidValue { name: String::new(), value: other.to_string() }),
        }),
        other => return Err(RctError::UnknownDataType(other.name().to_string())),
    };
    Ok(encode_value(&v))
}

fn to_i64(v: &DataValue) -> Result<i64, RctError> {
    Ok(match v {
        DataValue::Bool(b) => *b as i64,
        DataValue::I8(i) => *i as i64,
        DataValue::U8(i) => *i as i64,
        DataValue::I16(i) => *i as i64,
        DataValue::U16(i) => *i as i64,
        DataValue::I32(i) => *i as i64,
        DataValue::U32(i) => *i as i64,
        DataValue::F32(f) => *f as i64,
        _ => return Err(RctError::InvalidValue { name: String::new(), value: v.to_string() }),
    })
}

/// Decode a payload per data type, like utils.decode_value (native types only).
pub fn decode_value(data_type: DataType, data: &[u8]) -> Result<DataValue, RctError> {
    let need = match data_type {
        DataType::Bool | DataType::Uint8 | DataType::Int8 => 1,
        DataType::Uint16 | DataType::Int16 => 2,
        DataType::Enum | DataType::Uint32 | DataType::Int32 | DataType::Float => 4,
        DataType::String => 1,
        // Composite types (TIMESERIES..): returned raw for now (phase 2).
        _ => return Ok(DataValue::Raw(data.to_vec())),
    };
    if data.len() < need {
        return Err(RctError::PayloadTooShort { ty: data_type.name(), len: data.len(), need });
    }
    Ok(match data_type {
        DataType::Bool => DataValue::Bool(data[0] != 0),
        DataType::Uint8 => DataValue::U8(data[0]),
        DataType::Int8 => DataValue::I8(data[0] as i8),
        DataType::Uint16 => DataValue::U16(u16::from_be_bytes([data[0], data[1]])),
        DataType::Int16 => DataValue::I16(i16::from_be_bytes([data[0], data[1]])),
        DataType::Enum => DataValue::U32(u32::from_be_bytes([data[0], data[1], data[2], data[3]])),
        DataType::Uint32 => DataValue::U32(u32::from_be_bytes([data[0], data[1], data[2], data[3]])),
        DataType::Int32 => DataValue::I32(i32::from_be_bytes([data[0], data[1], data[2], data[3]])),
        DataType::Float => DataValue::F32(f32::from_be_bytes([data[0], data[1], data[2], data[3]])),
        DataType::String => {
            let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
            DataValue::Str(String::from_utf8_lossy(&data[..end]).to_string())
        }
        _ => unreachable!(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_even_input() {
        // python: CRC16(bytes.fromhex('02040000c0de')) == 0x30b1 (frame without start token)
        assert_eq!(crc16(&[0x02, 0x04, 0x00, 0x00, 0xc0, 0xde]), 0x30b1);
    }

    #[test]
    fn crc16_odd_input_pads_zero() {
        // quirk: odd input is padded with a zero byte
        assert_eq!(crc16(&[0x02]), crc16(&[0x02, 0x00]));
    }

    // Vectors from python tests/test_decode_value.py
    #[test]
    fn decode_bool_python_vectors() {
        for (input, expected) in [([0x00], false), ([0x01], true), ([0x02], true), ([0xff], true)] {
            assert_eq!(decode_value(DataType::Bool, &input).unwrap(), DataValue::Bool(expected));
        }
    }

    #[test]
    fn decode_uint8_python_vectors() {
        for (input, expected) in [([0x00], 0u8), ([0x01], 1), ([0x02], 2), ([0xff], 255)] {
            assert_eq!(decode_value(DataType::Uint8, &input).unwrap(), DataValue::U8(expected));
        }
    }

    #[test]
    fn decode_string_null_terminated() {
        let mut data = b"PS 6.0 BA3L".to_vec();
        data.extend(std::iter::repeat(0u8).take(40));
        assert_eq!(decode_value(DataType::String, &data).unwrap(), DataValue::Str("PS 6.0 BA3L".into()));
    }

    #[test]
    fn decode_string_no_null() {
        let data = b"PS 6.0 BA3L";
        assert_eq!(decode_value(DataType::String, data).unwrap(), DataValue::Str("PS 6.0 BA3L".into()));
    }

    #[test]
    fn encode_decode_roundtrip_float() {
        let enc = encode_for(DataType::Float, &DataValue::F32(0.5)).unwrap();
        assert_eq!(enc.len(), 4);
        assert_eq!(decode_value(DataType::Float, &enc).unwrap(), DataValue::F32(0.5));
    }
}
