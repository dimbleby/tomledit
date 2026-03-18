use crate::document::Document;
use crate::item_proxy::ItemProxy;
use crate::value::{ArrayOfTables, Table, Value};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyMapping, PyTuple};
use toml_edit::Item as ItemRs;

pub(crate) struct Item(pub(crate) ItemRs);

impl<'py> FromPyObject<'_, 'py> for Item {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        if let Ok(proxy) = obj.cast::<ItemProxy>() {
            let item_rs = proxy.borrow().clone_item(obj.py())?;
            return Ok(Self(item_rs));
        }

        if let Ok(doc) = obj.cast::<Document>() {
            let doc = doc.borrow();
            return Ok(Self(doc.inner.as_item().clone()));
        }

        if obj.is_none() {
            return Err(PyTypeError::new_err(
                "None is not a valid TOML value (TOML has no null type)",
            ));
        }

        if obj.cast::<PyMapping>().is_ok() {
            let table = Table::extract(obj)?;
            return Ok(Self(ItemRs::Table(table.0)));
        }

        if (obj.is_instance_of::<PyList>() || obj.is_instance_of::<PyTuple>())
            && obj.len().is_ok_and(|n| n > 0)
            && let Ok(array_of_tables) = ArrayOfTables::extract(obj)
        {
            return Ok(Self(ItemRs::ArrayOfTables(array_of_tables.0)));
        }

        let value = Value::extract(obj)?;
        Ok(Self(ItemRs::Value(value.0)))
    }
}
