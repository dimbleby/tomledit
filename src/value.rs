//! Python → toml_edit extraction for all TOML types.

use crate::item::Item;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyDate, PyDateAccess, PyDateTime, PyDelta, PyDeltaAccess, PyFloat, PyInt, PyList,
    PyMapping, PySequence, PyString, PyTime, PyTimeAccess, PyTuple,
};
use toml_edit::{
    Array as ArrayRs, ArrayOfTables as ArrayOfTablesRs, Date as DateRs, Datetime as DatetimeRs,
    InlineTable as InlineTableRs, Offset as OffsetRs, Table as TableRs, Time as TimeRs,
    Value as ValueRs,
};

// ---------------------------------------------------------------------------
// Datetime
// ---------------------------------------------------------------------------

pub(crate) struct Datetime(pub(crate) DatetimeRs);

impl<'py> FromPyObject<'_, 'py> for Datetime {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py_datetime = obj.cast::<PyDateTime>()?;
        let microsecond = py_datetime.get_microsecond();

        let date = DateRs {
            year: py_datetime.get_year() as u16,
            month: py_datetime.get_month(),
            day: py_datetime.get_day(),
        };
        let time = TimeRs {
            hour: py_datetime.get_hour(),
            minute: py_datetime.get_minute(),
            second: Some(py_datetime.get_second()),
            nanosecond: (microsecond != 0).then_some(1000 * microsecond),
        };

        // TOML only supports minute-precision UTC offsets; any sub-minute
        // component of the Python tzinfo is truncated by integer division.
        let offset = py_datetime
            .call_method0("utcoffset")?
            .extract::<Option<Bound<'py, PyDelta>>>()?
            .map(|delta| {
                let days = delta.get_days();
                let seconds = delta.get_seconds();
                let minutes = ((60 * 24 * days) + (seconds / 60)) as i16;
                OffsetRs::Custom { minutes }
            });

        Ok(Self(DatetimeRs {
            date: Some(date),
            time: Some(time),
            offset,
        }))
    }
}

// ---------------------------------------------------------------------------
// Date
// ---------------------------------------------------------------------------

pub(crate) struct Date(pub(crate) DateRs);

impl<'py> FromPyObject<'_, 'py> for Date {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py_date = obj.cast::<PyDate>()?;
        Ok(Self(DateRs {
            year: py_date.get_year() as u16,
            month: py_date.get_month(),
            day: py_date.get_day(),
        }))
    }
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

pub(crate) struct Time(pub(crate) TimeRs);

impl<'py> FromPyObject<'_, 'py> for Time {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py_time = obj.cast::<PyTime>()?;
        let microsecond = py_time.get_microsecond();
        if py_time
            .call_method0("utcoffset")?
            .extract::<Option<Bound<'py, PyDelta>>>()?
            .is_some()
        {
            return Err(PyTypeError::new_err(
                "TOML local times cannot have timezone information",
            ));
        }

        Ok(Self(TimeRs {
            hour: py_time.get_hour(),
            minute: py_time.get_minute(),
            second: Some(py_time.get_second()),
            nanosecond: (microsecond != 0).then_some(1000 * microsecond),
        }))
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

fn extract_mapping_pairs<'py, V>(py_mapping: &Bound<'py, PyMapping>) -> PyResult<Vec<(String, V)>>
where
    for<'a> V: FromPyObject<'a, 'py, Error = PyErr>,
{
    let len = py_mapping.len()?;
    let mut pairs = Vec::with_capacity(len);
    for pair in py_mapping.items()? {
        let py_tuple = pair.cast::<PyTuple>()?;
        let key: String = py_tuple.get_item(0)?.extract()?;
        let value: V = py_tuple.get_item(1)?.extract()?;
        pairs.push((key, value));
    }
    Ok(pairs)
}

pub(crate) struct InlineTable(pub(crate) InlineTableRs);

impl<'py> FromPyObject<'_, 'py> for InlineTable {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py_mapping = obj.cast::<PyMapping>()?;
        let pairs: Vec<(String, Value)> = extract_mapping_pairs(&py_mapping)?;
        Ok(Self(InlineTableRs::from_iter(
            pairs.into_iter().map(|(k, v)| (k, v.0)),
        )))
    }
}

pub(crate) struct Table(pub(crate) TableRs);

impl<'py> FromPyObject<'_, 'py> for Table {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py_mapping = obj.cast::<PyMapping>()?;
        let pairs: Vec<(String, Item)> = extract_mapping_pairs(&py_mapping)?;
        let mut table = TableRs::from_iter(pairs.into_iter().map(|(k, v)| (k, v.0)));
        if !table.is_empty() {
            table.set_implicit(true);
        }
        Ok(Self(table))
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
        // Check datetime before date (Python datetime is a subclass of date)
        if let Ok(py_datetime) = obj.cast::<PyDateTime>() {
            let datetime: Datetime = py_datetime.extract()?;
            return Ok(Self(ValueRs::from(datetime.0)));
        }

        if let Ok(py_date) = obj.cast::<PyDate>() {
            let date: Date = py_date.extract()?;
            return Ok(Self(ValueRs::from(DatetimeRs {
                date: Some(date.0),
                time: None,
                offset: None,
            })));
        }

        if let Ok(py_time) = obj.cast::<PyTime>() {
            let time: Time = py_time.extract()?;
            return Ok(Self(ValueRs::from(DatetimeRs {
                date: None,
                time: Some(time.0),
                offset: None,
            })));
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
            "Could not convert object of type '{}' to TOML value",
            name.to_str()?
        );
        Err(PyTypeError::new_err(text))
    }
}
