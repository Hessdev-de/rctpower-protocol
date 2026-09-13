// Error types for rct-core.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RctError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("frame CRC mismatch (received 0x{received:04x}, calculated 0x{calculated:04x}), consumed {consumed} bytes")]
    FrameCrcMismatch {
        received: u16,
        calculated: u16,
        consumed: usize,
    },

    #[error("frame consumed more bytes than its length ({consumed})")]
    FrameLengthExceeded { consumed: usize },

    #[error("invalid command byte 0x{cmd:02x} (consumed {consumed} bytes)")]
    InvalidCommand { cmd: u8, consumed: usize },

    #[error("unknown data type {0}")]
    UnknownDataType(String),

    #[error("payload too short for data type {ty}: got {len} bytes, need {need}")]
    PayloadTooShort { ty: &'static str, len: usize, need: usize },

    #[error("value {value} for parameter '{name}' is out of range [{min}, {max}]")]
    ValueOutOfRange {
        name: String,
        value: f64,
        min: f64,
        max: f64,
    },

    #[error("invalid value '{value}' for parameter '{name}'")]
    InvalidValue { name: String, value: String },

    #[error("response timeout")]
    Timeout,

    #[error("no response payload")]
    EmptyPayload,
}
