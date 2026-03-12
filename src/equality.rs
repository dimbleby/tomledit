use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyDate, PyDateAccess, PyDateTime, PyDict, PyList, PyString, PyTime, PyTimeAccess,
};
use toml_edit::Item as ItemRs;

use crate::value::Datetime;

/// Semantically compare two toml_edit Datetimes, treating Offset::Z and
/// Offset::Custom { minutes: 0 } as equivalent, and normalizing optional
/// second/nanosecond fields (None == Some(0)).
fn datetime_eq(a: &toml_edit::Datetime, b: &toml_edit::Datetime) -> bool {
    use toml_edit::Offset;

    fn normalize_offset(o: &Option<Offset>) -> Option<i16> {
        o.map(|off| match off {
            Offset::Z => 0,
            Offset::Custom { minutes } => minutes,
        })
    }

    fn time_eq(a: &Option<toml_edit::Time>, b: &Option<toml_edit::Time>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => {
                a.hour == b.hour
                    && a.minute == b.minute
                    && a.second.unwrap_or(0) == b.second.unwrap_or(0)
                    && a.nanosecond.unwrap_or(0) == b.nanosecond.unwrap_or(0)
            }
            (None, None) => true,
            _ => false,
        }
    }

    a.date == b.date
        && time_eq(&a.time, &b.time)
        && normalize_offset(&a.offset) == normalize_offset(&b.offset)
}

/// Compare two toml_edit Values structurally (pure Rust, no Python allocation).
fn values_structural_eq(a: &toml_edit::Value, b: &toml_edit::Value) -> bool {
    match (a, b) {
        (toml_edit::Value::String(a), toml_edit::Value::String(b)) => a.value() == b.value(),
        (toml_edit::Value::Integer(a), toml_edit::Value::Integer(b)) => a.value() == b.value(),
        (toml_edit::Value::Float(a), toml_edit::Value::Float(b)) => a.value() == b.value(),
        (toml_edit::Value::Boolean(a), toml_edit::Value::Boolean(b)) => a.value() == b.value(),
        (toml_edit::Value::Datetime(a), toml_edit::Value::Datetime(b)) => {
            datetime_eq(a.value(), b.value())
        }
        (toml_edit::Value::Array(a), toml_edit::Value::Array(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(va, vb)| values_structural_eq(va, vb))
        }
        (toml_edit::Value::InlineTable(a), toml_edit::Value::InlineTable(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|bv| values_structural_eq(v, bv)))
        }
        _ => false,
    }
}

fn tables_structural_eq(a: &toml_edit::Table, b: &toml_edit::Table) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(k, v)| b.get(k).is_some_and(|bv| items_structural_eq(v, bv)))
}

pub(crate) fn items_structural_eq(a: &ItemRs, b: &ItemRs) -> bool {
    match (a, b) {
        (ItemRs::Value(va), ItemRs::Value(vb)) => values_structural_eq(va, vb),
        (ItemRs::Table(ta), ItemRs::Table(tb)) => tables_structural_eq(ta, tb),
        (ItemRs::ArrayOfTables(aa), ItemRs::ArrayOfTables(ab)) => {
            aa.len() == ab.len()
                && aa
                    .iter()
                    .zip(ab.iter())
                    .all(|(ta, tb)| tables_structural_eq(ta, tb))
        }
        _ => false,
    }
}

/// Compare a toml_edit Value to a Python object for equality.
pub(crate) fn value_eq(value: &toml_edit::Value, other: &Bound<'_, PyAny>) -> PyResult<bool> {
    match value {
        toml_edit::Value::Boolean(b) => {
            if let Ok(other_b) = other.extract::<bool>() {
                return Ok(*b.value() == other_b);
            }
        }
        toml_edit::Value::Integer(i) => {
            if other.cast::<PyBool>().is_err() {
                if let Ok(other_i) = other.extract::<i64>() {
                    return Ok(*i.value() == other_i);
                }
                if let Ok(other_f) = other.extract::<f64>() {
                    return Ok((*i.value() as f64) == other_f);
                }
            }
        }
        toml_edit::Value::Float(f) => {
            if other.cast::<PyBool>().is_err()
                && let Ok(other_f) = other.extract::<f64>()
            {
                return Ok(*f.value() == other_f);
            }
        }
        toml_edit::Value::String(s) => {
            if let Ok(other_s) = other.cast::<PyString>() {
                return Ok(other_s.to_str().is_ok_and(|o| s.value() == o));
            }
        }
        toml_edit::Value::Datetime(dt) => {
            if let Ok(py_dt) = other.cast::<PyDateTime>() {
                let other_dt: Datetime = py_dt.extract()?;
                return Ok(datetime_eq(dt.value(), &other_dt.0));
            }
            if let Ok(py_date) = other.cast::<PyDate>() {
                if let (Some(d), None, None) =
                    (&dt.value().date, &dt.value().time, &dt.value().offset)
                {
                    return Ok(d.year == py_date.get_year() as u16
                        && d.month == py_date.get_month()
                        && d.day == py_date.get_day());
                }
                return Ok(false);
            }
            if let Ok(py_time) = other.cast::<PyTime>() {
                if let (None, Some(t), None) =
                    (&dt.value().date, &dt.value().time, &dt.value().offset)
                {
                    return Ok(t.hour == py_time.get_hour()
                        && t.minute == py_time.get_minute()
                        && t.second.unwrap_or(0) == py_time.get_second()
                        && t.nanosecond.unwrap_or(0) == (py_time.get_microsecond() * 1000));
                }
                return Ok(false);
            }
        }
        toml_edit::Value::Array(arr) => {
            if let Ok(other_list) = other.cast::<PyList>() {
                if arr.len() != other_list.len() {
                    return Ok(false);
                }
                for (i, v) in arr.iter().enumerate() {
                    let other_elem = other_list.get_item(i)?;
                    if !value_eq(v, &other_elem)? {
                        return Ok(false);
                    }
                }
                return Ok(true);
            }
        }
        toml_edit::Value::InlineTable(it) => {
            if let Ok(other_dict) = other.cast::<PyDict>() {
                if it.len() != other_dict.len() {
                    return Ok(false);
                }
                for (k, v) in it.iter() {
                    let Some(other_v) = other_dict.get_item(k)? else {
                        return Ok(false);
                    };
                    if !value_eq(v, &other_v)? {
                        return Ok(false);
                    }
                }
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Compare a toml_edit Table's entries to a Python dict for equality.
pub(crate) fn table_entries_eq<'a>(
    iter: impl Iterator<Item = (&'a str, &'a ItemRs)>,
    len: usize,
    other_dict: &Bound<'_, PyDict>,
) -> PyResult<bool> {
    if len != other_dict.len() {
        return Ok(false);
    }
    for (k, v) in iter {
        let Some(other_v) = other_dict.get_item(k)? else {
            return Ok(false);
        };
        if !item_eq(v, &other_v)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Compare a toml_edit Item to a Python object for equality.
pub(crate) fn item_eq(item: &ItemRs, other: &Bound<'_, PyAny>) -> PyResult<bool> {
    match item {
        ItemRs::Value(value) => value_eq(value, other),
        ItemRs::Table(table) => {
            let Ok(other_dict) = other.cast::<PyDict>() else {
                return Ok(false);
            };
            table_entries_eq(table.iter(), table.len(), other_dict)
        }
        ItemRs::ArrayOfTables(aot) => {
            let Ok(other_list) = other.cast::<PyList>() else {
                return Ok(false);
            };
            if aot.len() != other_list.len() {
                return Ok(false);
            }
            for (i, table) in aot.iter().enumerate() {
                let other_elem = other_list.get_item(i)?;
                let Ok(other_dict) = other_elem.cast::<PyDict>() else {
                    return Ok(false);
                };
                if !table_entries_eq(table.iter(), table.len(), other_dict)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}
