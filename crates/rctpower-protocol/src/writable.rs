// Write-side validation rules, ported from rctpower_writesupport rct.py
// (https://github.com/do-gooder/rctpower_writesupport), MIT License,
// Copyright (c) 2024 do-gooder.
//
// Kept separate from the GPL codec on purpose: this is the MIT-derived layer,
// and new writable parameters are pure data.

use crate::error::RctError;
use crate::registry::registry;
use crate::types::{DataValue, DataType};

#[derive(Debug, Clone, Copy)]
pub struct Rule {
    pub name: &'static str,
    pub min: f64,
    pub max: f64,
    pub decimals: u32,
}

/// Parameters accepted by the write-support tool with their validation rules
/// (1:1 from rct.py set_value()).
pub const WRITABLE: &[Rule] = &[
    Rule { name: "power_mng.soc_strategy", min: 0.0, max: 5.0, decimals: 0 },
    Rule { name: "power_mng.soc_target_set", min: 0.0, max: 1.0, decimals: 2 },
    Rule { name: "power_mng.battery_power_extern", min: -6000.0, max: 6000.0, decimals: 2 },
    Rule { name: "power_mng.soc_min", min: 0.05, max: 1.0, decimals: 2 },
    Rule { name: "power_mng.soc_max", min: 0.0, max: 1.0, decimals: 2 },
    Rule { name: "power_mng.soc_charge_power", min: -999999.0, max: 999999.0, decimals: 2 },
    Rule { name: "power_mng.soc_charge", min: -999999.0, max: 999999.0, decimals: 2 },
    Rule { name: "p_rec_lim[1]", min: 0.0, max: 6000.0, decimals: 2 },
    // power_mng.use_grid_power_enable: bool, range N/A
    Rule { name: "power_mng.use_grid_power_enable", min: f64::NAN, max: f64::NAN, decimals: 0 },
    Rule { name: "buf_v_control.power_reduction", min: 0.0, max: 1.0, decimals: 3 },
];

pub fn rule_for(name: &str) -> Option<&'static Rule> {
    WRITABLE.iter().find(|r| r.name == name)
}

/// Validate value against the rct.py rules and resolve it via the registry.
pub fn validate(name: &str, value: &DataValue) -> Result<(), RctError> {
    let rule = rule_for(name)
        .ok_or_else(|| RctError::InvalidValue { name: name.into(), value: value.to_string() })?;
    // must exist in the protocol registry as well
    registry()
        .get_by_name(name)
        .ok_or_else(|| RctError::InvalidValue { name: name.into(), value: value.to_string() })?;

    if rule.min.is_nan() {
        // bool parameter
        if matches!(value, DataValue::Bool(_)) {
            return Ok(());
        }
        return Err(RctError::InvalidValue { name: name.into(), value: value.to_string() });
    }

    let v = match value {
        DataValue::F32(f) => *f as f64,
        DataValue::I8(i) => *i as f64,
        DataValue::U8(i) => *i as f64,
        DataValue::I16(i) => *i as f64,
        DataValue::U16(i) => *i as f64,
        DataValue::I32(i) => *i as f64,
        DataValue::U32(i) => *i as f64,
        _ => return Err(RctError::InvalidValue { name: name.into(), value: value.to_string() }),
    };
    if !(rule.min..=rule.max).contains(&v) {
        return Err(RctError::ValueOutOfRange { name: name.into(), value: v, min: rule.min, max: rule.max });
    }
    // decimal precision check like validate_float(): count decimals of input
    let s = value.to_string();
    if let Some((_, frac)) = s.split_once('.') {
        if frac.len() as u32 > rule.decimals {
            return Err(RctError::InvalidValue { name: name.into(), value: s });
        }
    }
    Ok(())
}

/// Parse a CLI string value for a parameter (bool vs number), like rct.py main.
pub fn parse_value(name: &str, raw: &str) -> Result<DataValue, RctError> {
    let rule = rule_for(name)
        .ok_or_else(|| RctError::InvalidValue { name: name.into(), value: raw.into() })?;
    let obj = registry().get_by_name(name).unwrap();
    if obj.request_data_type == DataType::Bool {
        return match raw.to_ascii_lowercase().as_str() {
            "true" => Ok(DataValue::Bool(true)),
            "false" => Ok(DataValue::Bool(false)),
            _ => Err(RctError::InvalidValue { name: name.into(), value: raw.into() }),
        };
    }
    let _ = rule;
    if raw.contains('.') {
        let f: f32 = raw.parse().map_err(|_| RctError::InvalidValue { name: name.into(), value: raw.into() })?;
        Ok(DataValue::F32(f))
    } else {
        let i: i64 = raw.parse().map_err(|_| RctError::InvalidValue { name: name.into(), value: raw.into() })?;
        Ok(match obj.request_data_type {
            DataType::Float => DataValue::F32(i as f32),
            DataType::Enum | DataType::Uint8 => DataValue::U8(i as u8),
            DataType::Int8 => DataValue::I8(i as i8),
            DataType::Uint16 => DataValue::U16(i as u16),
            DataType::Int16 => DataValue::I16(i as i16),
            DataType::Uint32 => DataValue::U32(i as u32),
            _ => DataValue::I32(i as i32),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_paths() {
        assert!(validate("power_mng.soc_strategy", &DataValue::U8(4)).is_ok());
        assert!(validate("power_mng.soc_target_set", &DataValue::F32(0.55)).is_ok());
        assert!(validate("power_mng.battery_power_extern", &DataValue::I32(-6000)).is_ok());
        assert!(validate("p_rec_lim[1]", &DataValue::F32(700.0)).is_ok());
        assert!(validate("power_mng.use_grid_power_enable", &DataValue::Bool(true)).is_ok());
        assert!(validate("buf_v_control.power_reduction", &DataValue::F32(0.123)).is_ok());
    }

    #[test]
    fn out_of_range() {
        // rct.py rejects 7000 for p_rec_lim[1] (max 6000)
        assert!(validate("p_rec_lim[1]", &DataValue::F32(7000.0)).is_err());
        assert!(validate("power_mng.soc_strategy", &DataValue::U8(6)).is_err());
        assert!(validate("power_mng.soc_min", &DataValue::F32(0.04)).is_err());
    }

    #[test]
    fn decimals_enforced() {
        assert!(validate("buf_v_control.power_reduction", &DataValue::F32(0.1234)).is_err());
        assert!(validate("power_mng.soc_target_set", &DataValue::F32(0.555)).is_err());
    }

    #[test]
    fn unknown_param_rejected() {
        assert!(validate("battery.soc", &DataValue::F32(0.5)).is_err()); // readable but not in writable set
    }
}
