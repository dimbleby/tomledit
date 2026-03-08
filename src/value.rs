//! Python → toml_edit extraction for all TOML types.

use crate::error::TomlError;
use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyDateAccess, PyDateTime, PyDelta, PyDeltaAccess, PyFloat, PyInt, PyList, PyMapping,
    PySequence, PyString, PyTimeAccess, PyTuple,
};
use toml_edit::{
    Array as ArrayRs, ArrayOfTables as ArrayOfTablesRs, Date, Datetime as DatetimeRs,
    InlineTable as InlineTableRs, Offset, Table as TableRs, Time, Value as ValueRs,
};

// ---------------------------------------------------------------------------
// Datetime
// ---------------------------------------------------------------------------

pub(crate) struct Datetime(pub(crate) DatetimeRs);

impl From<Borrowed<'_, '_, PyDateTime>> for Datetime {
    fn from(py_datetime: Borrowed<'_, '_, PyDateTime>) -> Self {
        let date = Date {
            year: py_datetime.get_year() as u16,
            month: py_datetime.get_month(),
            day: py_datetime.get_day(),
        };
        let time = Time {
            hour: py_datetime.get_hour(),
            minute: py_datetime.get_minute(),
            second: Some(py_datetime.get_second()),
            nanosecond: Some(1000 * py_datetime.get_microsecond()),
        };

        // TOML only supports minute-precision UTC offsets; any sub-minute
        // component of the Python tzinfo is truncated by integer division.
        let offset = py_datetime
            .call_method0("utcoffset")
            .ok()
            .and_then(|delta| {
                delta.cast::<PyDelta>().ok().map(|d| {
                    let days = d.get_days();
                    let seconds = d.get_seconds();
                    let minutes = ((60 * 24 * days) + (seconds / 60)) as i16;
                    Offset::Custom { minutes }
                })
            });

        Self(DatetimeRs {
            date: Some(date),
            time: Some(time),
            offset,
        })
    }
}

impl<'py> FromPyObject<'_, 'py> for Datetime {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py_datetime = obj.cast::<PyDateTime>()?;
        Ok(Self::from(py_datetime))
    }
}

// ---------------------------------------------------------------------------
// Array
// ---------------------------------------------------------------------------

pub(crate) struct Array(pub(crate) ArrayRs);

impl<'py> FromPyObject<'_, 'py> for Array {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py_sequence = obj.cast::<PySequence>()?;
        let len = py_sequence.len()?;
        let mut values: Vec<ValueRs> = Vec::with_capacity(len);
        for py_value in py_sequence.try_iter()? {
            let value: Value = py_value?.extract()?;
            values.push(value.0);
        }
        Ok(Self(ArrayRs::from_iter(values)))
    }
}

// ---------------------------------------------------------------------------
// InlineTable / Table (shared helper)
// ---------------------------------------------------------------------------

fn extract_mapping_pairs(py_mapping: &Bound<'_, PyMapping>) -> PyResult<Vec<(String, ValueRs)>> {
    let len = py_mapping.len()?;
    let mut pairs = Vec::with_capacity(len);
    for item in py_mapping.items()? {
        let py_tuple = item.cast::<PyTuple>()?;
        let key: String = py_tuple.get_item(0)?.extract()?;
        let value: Value = py_tuple.get_item(1)?.extract()?;
        pairs.push((key, value.0));
    }
    Ok(pairs)
}

pub(crate) struct InlineTable(pub(crate) InlineTableRs);

impl<'py> FromPyObject<'_, 'py> for InlineTable {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py_mapping = obj.cast::<PyMapping>()?;
        let pairs = extract_mapping_pairs(&py_mapping)?;
        Ok(Self(InlineTableRs::from_iter(pairs)))
    }
}

pub(crate) struct Table(pub(crate) TableRs);

impl Table {
    pub(crate) fn extract_from_dict(dict: &Bound<'_, pyo3::types::PyDict>) -> PyResult<Self> {
        let py_mapping = dict.cast::<PyMapping>()?;
        let pairs = extract_mapping_pairs(py_mapping)?;
        Ok(Self(TableRs::from_iter(pairs)))
    }
}

impl<'py> FromPyObject<'_, 'py> for Table {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py_mapping = obj.cast::<PyMapping>()?;
        let pairs = extract_mapping_pairs(&py_mapping)?;
        Ok(Self(TableRs::from_iter(pairs)))
    }
}

// ---------------------------------------------------------------------------
// ArrayOfTables
// ---------------------------------------------------------------------------

pub(crate) struct ArrayOfTables(pub(crate) ArrayOfTablesRs);

impl<'py> FromPyObject<'_, 'py> for ArrayOfTables {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py_sequence = obj.cast::<PySequence>()?;
        let len = py_sequence.len()?;
        let mut tables: Vec<TableRs> = Vec::with_capacity(len);
        for py_table in py_sequence.try_iter()? {
            let table: Table = py_table?.extract()?;
            tables.push(table.0);
        }
        Ok(Self(ArrayOfTablesRs::from_iter(tables)))
    }
}

// ---------------------------------------------------------------------------
// Value (top-level, tries all of the above)
// ---------------------------------------------------------------------------

pub(crate) struct Value(pub(crate) ValueRs);

impl<'py> FromPyObject<'_, 'py> for Value {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        if let Ok(py_datetime) = obj.cast::<PyDateTime>() {
            let datetime: Datetime = py_datetime.extract()?;
            return Ok(Self(ValueRs::from(datetime.0)));
        }

        if let Ok(py_str) = obj.cast::<PyString>() {
            let s: &str = py_str.extract()?;
            return Ok(Self(ValueRs::from(s)));
        }

        // Check bool before int (Python bool is a subclass of int)
        if let Ok(py_bool) = obj.cast::<PyBool>() {
            let b: bool = py_bool.extract()?;
            return Ok(Self(ValueRs::from(b)));
        }

        if let Ok(py_int) = obj.cast::<PyInt>() {
            let i: i64 = py_int.extract()?;
            return Ok(Self(ValueRs::from(i)));
        }

        if let Ok(py_float) = obj.cast::<PyFloat>() {
            let f: f64 = py_float.extract()?;
            return Ok(Self(ValueRs::from(f)));
        }

        if let Ok(py_mapping) = obj.cast::<PyMapping>() {
            let inline_table: InlineTable = py_mapping.extract()?;
            return Ok(Self(ValueRs::from(inline_table.0)));
        }

        // Only accept list and tuple as TOML arrays.  Other sequence types
        // (bytes, bytearray, memoryview, range, …) don't have obvious TOML
        // semantics.  Users can wrap them with list() if needed.
        if obj.is_instance_of::<PyList>() || obj.is_instance_of::<PyTuple>() {
            let py_sequence = obj.cast::<PySequence>()?;
            let array: Array = py_sequence.extract()?;
            return Ok(Self(ValueRs::from(array.0)));
        }

        let name = obj.get_type().name()?;
        let text = format!(
            "Could not convert object of type '{}' to value",
            name.to_str()?
        );
        Err(TomlError::new_err(text))
    }
}
