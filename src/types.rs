// Type declarations for the RCT Power serial protocol.
//
// Ported from python-rctclient (https://github.com/svalouch/python-rctclient),
// Copyright 2020 Peter Oberhofer (pob90), 2020-2026 Stefan Valouch (svalouch),
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt::{self, Display};

/// Commands that can be used with send/receive frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    /// Read command
    Read = 0x01,
    /// Write command
    Write = 0x02,
    /// Long write command (for variables > 251 bytes)
    LongWrite = 0x03,
    /// Response to a read or write command
    Response = 0x05,
    /// Long response (for variables > 251 bytes)
    LongResponse = 0x06,
    /// Periodic reading
    ReadPeriodically = 0x08,
    /// Extension, not parseable by ReceiveFrame
    Extension = 0x3c,
}

impl Command {
    /// Plant variant: bit 6 is set on plant commands.
    pub const PLANT_BIT: u8 = 0x40;

    /// Raw byte value of the command.
    #[inline]
    pub fn byte(self) -> u8 {
        self as u8
    }

    /// Decode a raw command byte (plant bit included in the value).
    pub fn from_byte(b: u8) -> Option<Command> {
        // plant commands carry the same base value plus PLANT_BIT
        let base = b & !Self::PLANT_BIT;
        let cmd = match base {
            0x01 => Command::Read,
            0x02 => Command::Write,
            0x03 => Command::LongWrite,
            0x05 => Command::Response,
            0x06 => Command::LongResponse,
            0x08 => Command::ReadPeriodically,
            0x3c => Command::Extension,
            _ => return None,
        };
        Some(cmd)
    }

    /// Whether this raw command byte belongs to plant communication.
    #[inline]
    pub fn is_plant_byte(b: u8) -> bool {
        b & Self::PLANT_BIT != 0
    }

    #[inline]
    pub fn is_long(self) -> bool {
        matches!(self, Command::LongWrite | Command::LongResponse)
    }

    #[inline]
    pub fn is_write(self) -> bool {
        matches!(self, Command::Write | Command::LongWrite)
    }

    #[inline]
    pub fn is_response(self) -> bool {
        matches!(self, Command::Response | Command::LongResponse)
    }
}

static NAMES: [&str; 37] = [
    "RB485",
    "ENERGY",
    "GRID_MON",
    "TEMPERATURE",
    "BATTERY",
    "CS_NEG",
    "HW_TEST",
    "G_SYNC",
    "LOGGER",
    "WIFI",
    "ADC",
    "NET",
    "ACC_CONV",
    "DC_CONV",
    "NSM",
    "IO_BOARD",
    "FLASH_RTC",
    "POWER_MNG",
    "BUF_V_CONTROL",
    "DB",
    "SWITCH_ON_COND",
    "P_REC",
    "MODBUS",
    "BAT_MNG_STRUCT",
    "ISO_STRUCT",
    "GRID_LT",
    "CAN_BUS",
    "DISPLAY_STRUCT",
    "FLASH_PARAM",
    "FAULT",
    "PRIM_SM",
    "CS_MAP",
    "LINE_MON",
    "OTHERS",
    "BATTERY_PLACEHOLDER",
    "FRT",
    "PARTITION"
];

/// Grouping information for object IDs (not used by the protocol itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ObjectGroup {
    Rb485 = 0,
    Energy = 1,
    GridMon = 2,
    Temperature = 3,
    Battery = 4,
    CsNeg = 5,
    HwTest = 6,
    GSync = 7,
    Logger = 8,
    Wifi = 9,
    Adc = 10,
    Net = 11,
    AccConv = 12,
    DcConv = 13,
    Nsm = 14,
    IoBoard = 15,
    FlashRtc = 16,
    PowerMng = 17,
    BufVControl = 18,
    Db = 19,
    SwitchOnCond = 20,
    PRec = 21,
    Modbus = 22,
    BatMngStruct = 23,
    IsoStruct = 24,
    GridLt = 25,
    CanBus = 26,
    DisplayStruct = 27,
    FlashParam = 28,
    Fault = 29,
    PrimSm = 30,
    CsMap = 31,
    LineMon = 32,
    Others = 33,
    BatteryPlaceholder = 34,
    Frt = 35,
    Partition = 36,
}

impl Display for ObjectGroup {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", NAMES[*self as usize])
    }
}

impl ObjectGroup {
    pub fn from_u8(v: u8) -> Option<ObjectGroup> {
        if v <= 36 {
            // SAFETY-safe mapping via match below
            Some(match v {
                0 => ObjectGroup::Rb485,
                1 => ObjectGroup::Energy,
                2 => ObjectGroup::GridMon,
                3 => ObjectGroup::Temperature,
                4 => ObjectGroup::Battery,
                5 => ObjectGroup::CsNeg,
                6 => ObjectGroup::HwTest,
                7 => ObjectGroup::GSync,
                8 => ObjectGroup::Logger,
                9 => ObjectGroup::Wifi,
                10 => ObjectGroup::Adc,
                11 => ObjectGroup::Net,
                12 => ObjectGroup::AccConv,
                13 => ObjectGroup::DcConv,
                14 => ObjectGroup::Nsm,
                15 => ObjectGroup::IoBoard,
                16 => ObjectGroup::FlashRtc,
                17 => ObjectGroup::PowerMng,
                18 => ObjectGroup::BufVControl,
                19 => ObjectGroup::Db,
                20 => ObjectGroup::SwitchOnCond,
                21 => ObjectGroup::PRec,
                22 => ObjectGroup::Modbus,
                23 => ObjectGroup::BatMngStruct,
                24 => ObjectGroup::IsoStruct,
                25 => ObjectGroup::GridLt,
                26 => ObjectGroup::CanBus,
                27 => ObjectGroup::DisplayStruct,
                28 => ObjectGroup::FlashParam,
                29 => ObjectGroup::Fault,
                30 => ObjectGroup::PrimSm,
                31 => ObjectGroup::CsMap,
                32 => ObjectGroup::LineMon,
                33 => ObjectGroup::Others,
                34 => ObjectGroup::BatteryPlaceholder,
                35 => ObjectGroup::Frt,
                36 => ObjectGroup::Partition,
                _ => return None,
            })
        } else {
            None
        }
    }

    pub fn from_string(entry: &str) -> Option<ObjectGroup> {
        for (e, item) in NAMES.iter().enumerate() {
            if *item == entry {
                return ObjectGroup::from_u8(e as u8);
            }
        }

        None
    }
}

/// Wire data types used to select the encode/decode mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DataType {
    Unknown = 0,
    Bool = 1,
    Uint8 = 2,
    Int8 = 3,
    Uint16 = 4,
    Int16 = 5,
    Uint32 = 6,
    Int32 = 7,
    Enum = 8,
    Float = 9,
    String = 10,
    // Composite, decode-only (phase 2):
    Timeseries = 20,
    EventTable = 21,
    BatteryModuleStatus = 22,
    BatteryModuleStatistics = 23,
    BatteryModuleResistance = 24,
}

impl DataType {
    pub fn from_str_name(s: &str) -> Option<DataType> {
        Some(match s {
            "UNKNOWN" => DataType::Unknown,
            "BOOL" => DataType::Bool,
            "UINT8" => DataType::Uint8,
            "INT8" => DataType::Int8,
            "UINT16" => DataType::Uint16,
            "INT16" => DataType::Int16,
            "UINT32" => DataType::Uint32,
            "INT32" => DataType::Int32,
            "ENUM" => DataType::Enum,
            "FLOAT" => DataType::Float,
            "STRING" => DataType::String,
            "TIMESERIES" => DataType::Timeseries,
            "EVENT_TABLE" => DataType::EventTable,
            "BATTERY_MODULE_STATUS" => DataType::BatteryModuleStatus,
            "BATTERY_MODULE_STATISTICS" => DataType::BatteryModuleStatistics,
            "BATTERY_MODULE_RESISTANCE" => DataType::BatteryModuleResistance,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            DataType::Unknown => "UNKNOWN",
            DataType::Bool => "BOOL",
            DataType::Uint8 => "UINT8",
            DataType::Int8 => "INT8",
            DataType::Uint16 => "UINT16",
            DataType::Int16 => "INT16",
            DataType::Uint32 => "UINT32",
            DataType::Int32 => "INT32",
            DataType::Enum => "ENUM",
            DataType::Float => "FLOAT",
            DataType::String => "STRING",
            DataType::Timeseries => "TIMESERIES",
            DataType::EventTable => "EVENT_TABLE",
            DataType::BatteryModuleStatus => "BATTERY_MODULE_STATUS",
            DataType::BatteryModuleStatistics => "BATTERY_MODULE_STATISTICS",
            DataType::BatteryModuleResistance => "BATTERY_MODULE_RESISTANCE",
        }
    }
}

/// Frame types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    Standard = 4,
    Plant = 8,
}

/// A decoded/encodable protocol value.
#[derive(Debug, Clone, PartialEq)]
pub enum DataValue {
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    F32(f32),
    Str(String),
    /// Raw payload for composite types not yet decoded.
    Raw(Vec<u8>),
}

impl fmt::Display for DataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataValue::Bool(b) => write!(f, "{b}"),
            DataValue::I8(v) => write!(f, "{v}"),
            DataValue::U8(v) => write!(f, "{v}"),
            DataValue::I16(v) => write!(f, "{v}"),
            DataValue::U16(v) => write!(f, "{v}"),
            DataValue::I32(v) => write!(f, "{v}"),
            DataValue::U32(v) => write!(f, "{v}"),
            DataValue::F32(v) => write!(f, "{v}"),
            DataValue::Str(s) => write!(f, "{s}"),
            DataValue::Raw(b) => write!(f, "0x{}", hex(b)),
        }
    }
}

pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
