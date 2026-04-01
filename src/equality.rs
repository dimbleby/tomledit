use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyDate, PyDateAccess, PyDateTime, PyList, PyString, PyTime, PyTimeAccess,
    PyTzInfoAccess,
};
use toml_edit::Item as ItemRs;

use crate::dict_ops;
use crate::item_ops::datetime_to_py;
use crate::item_proxy::ItemProxy;

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

/// Compare a Table with an InlineTable by walking their entries directly.
fn table_inline_eq(table: &toml_edit::Table, it: &toml_edit::InlineTable) -> bool {
    table.len() == it.len()
        && table
            .iter()
            .all(|(k, item)| it.get(k).is_some_and(|v| item_value_eq(item, v)))
}

/// Compare an Item with a Value across the Table/InlineTable and AoT/Array
/// boundaries without cloning.
fn item_value_eq(item: &ItemRs, value: &toml_edit::Value) -> bool {
    match item {
        ItemRs::Value(v) => values_structural_eq(v, value),
        ItemRs::Table(t) => {
            matches!(value, toml_edit::Value::InlineTable(it) if table_inline_eq(t, it))
        }
        ItemRs::ArrayOfTables(aot) => {
            matches!(value, toml_edit::Value::Array(arr) if aot_array_eq(aot, arr))
        }
        _ => false,
    }
}

/// Compare an AoT with an Array of inline tables directly.
fn aot_array_eq(aot: &toml_edit::ArrayOfTables, arr: &toml_edit::Array) -> bool {
    aot.len() == arr.len()
        && aot
            .iter()
            .zip(arr.iter())
            .all(|(t, v)| matches!(v, toml_edit::Value::InlineTable(it) if table_inline_eq(t, it)))
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
        // Cross-type: Table ↔ InlineTable
        (ItemRs::Table(t), ItemRs::Value(toml_edit::Value::InlineTable(it)))
        | (ItemRs::Value(toml_edit::Value::InlineTable(it)), ItemRs::Table(t)) => {
            table_inline_eq(t, it)
        }
        // Cross-type: AoT ↔ Array
        (ItemRs::ArrayOfTables(aot), ItemRs::Value(toml_edit::Value::Array(arr)))
        | (ItemRs::Value(toml_edit::Value::Array(arr)), ItemRs::ArrayOfTables(aot)) => {
            aot_array_eq(aot, arr)
        }
        _ => false,
    }
}

/// Compare a toml_edit Value to a Python object that may be an [`ItemProxy`].
///
/// Proxy fast path stays in Rust via [`values_structural_eq`]; plain Python
/// objects are compared by extracting the appropriate Python type.
pub(crate) fn value_eq(value: &toml_edit::Value, other: &Bound<'_, PyAny>) -> PyResult<bool> {
    if let Ok(proxy) = other.cast::<ItemProxy>() {
        let py = other.py();
        let proxy = proxy.borrow();
        let doc = proxy.document.bind(py).borrow();
        proxy.check_fresh(&doc)?;
        let other_item = proxy.navigate(&doc.inner)?;
        return Ok(match other_item {
            ItemRs::Value(v) => values_structural_eq(value, v),
            // Cross-type: proxy is a Table or AoT.
            other => item_value_eq(other, value),
        });
    }
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
                let py_int = i.value().into_pyobject(other.py())?;
                return py_int.into_any().eq(other);
            }
        }
        toml_edit::Value::Float(f) => {
            if other.cast::<PyBool>().is_err() {
                let py_float = f.value().into_pyobject(other.py())?;
                return py_float.into_any().eq(other);
            }
        }
        toml_edit::Value::String(s) => {
            if let Ok(other_s) = other.cast::<PyString>() {
                return Ok(other_s.to_str().is_ok_and(|o| s.value() == o));
            }
        }
        toml_edit::Value::Datetime(dt) => {
            if let Ok(py_dt) = other.cast::<PyDateTime>() {
                let toml_py = datetime_to_py(dt.value(), other.py())?;
                return toml_py.bind(other.py()).eq(py_dt);
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
                    if py_time.get_tzinfo().is_some() {
                        return Ok(false);
                    }
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
            return mapping_eq(it.iter(), it.len(), other, value_eq);
        }
    }
    Ok(false)
}

/// Compare a TOML mapping (inline table or regular table) entry-by-entry
/// against a Python Mapping.
fn mapping_eq<'a, V>(
    entries: impl Iterator<Item = (&'a str, V)>,
    len: usize,
    other: &Bound<'_, PyAny>,
    eq: impl Fn(V, &Bound<'_, PyAny>) -> PyResult<bool>,
) -> PyResult<bool> {
    if !dict_ops::is_mapping_like(other) {
        return Ok(false);
    }
    let other_len: usize = other.len()?;
    if len != other_len {
        return Ok(false);
    }
    for (k, v) in entries {
        let Ok(other_v) = other.get_item(k) else {
            return Ok(false);
        };
        if !eq(v, &other_v)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Compare a toml_edit Table to a Python object that may be an [`ItemProxy`].
///
/// Proxy fast path stays in Rust via [`tables_structural_eq`]; plain Python
/// dicts and other Mappings are compared entry-by-entry.
pub(crate) fn table_eq(table: &toml_edit::Table, other: &Bound<'_, PyAny>) -> PyResult<bool> {
    if let Ok(proxy) = other.cast::<ItemProxy>() {
        let py = other.py();
        let proxy = proxy.borrow();
        let doc = proxy.document.bind(py).borrow();
        proxy.check_fresh(&doc)?;
        let other_item = proxy.navigate(&doc.inner)?;
        return Ok(match other_item {
            ItemRs::Table(t) => tables_structural_eq(table, t),
            // Cross-type: proxy is an InlineTable value.
            ItemRs::Value(toml_edit::Value::InlineTable(it)) => table_inline_eq(table, it),
            _ => false,
        });
    }
    mapping_eq(table.iter(), table.len(), other, item_eq)
}

/// Compare a toml_edit Item to a Python object that may be an [`ItemProxy`].
///
/// Proxy fast path stays in Rust; plain Python objects are compared
/// element-wise.
pub(crate) fn item_eq(item: &ItemRs, other: &Bound<'_, PyAny>) -> PyResult<bool> {
    if let Ok(proxy) = other.cast::<ItemProxy>() {
        let py = other.py();
        let proxy = proxy.borrow();
        let doc = proxy.document.bind(py).borrow();
        proxy.check_fresh(&doc)?;
        let other_item = proxy.navigate(&doc.inner)?;
        return Ok(items_structural_eq(item, other_item));
    }
    match item {
        ItemRs::Value(value) => value_eq(value, other),
        ItemRs::Table(table) => table_eq(table, other),
        ItemRs::ArrayOfTables(aot) => {
            let Ok(other_list) = other.cast::<PyList>() else {
                return Ok(false);
            };
            if aot.len() != other_list.len() {
                return Ok(false);
            }
            for (i, table) in aot.iter().enumerate() {
                let other_elem = other_list.get_item(i)?;
                if !table_eq(table, &other_elem)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}
