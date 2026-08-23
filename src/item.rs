use crate::document::Document;
use crate::item_ops;
use crate::item_proxy::ItemProxy;
use crate::value::{self, Table, Value};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::sync::RwLockExt;
use pyo3::types::{PyList, PyMapping};
use toml_edit::{Item as ItemRs, Value as ValueRs};

pub(crate) struct Item(pub(crate) ItemRs);

impl<'py> FromPyObject<'_, 'py> for Item {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        if let Ok(proxy) = obj.cast::<ItemProxy>() {
            let item_rs = proxy.get().clone_item(obj.py())?;
            return Ok(Self(item_rs));
        }

        if let Ok(doc) = obj.cast::<Document>() {
            let doc = doc.get();
            let inner = doc.inner.read_py_attached(obj.py());
            return Ok(Self(inner.as_item().clone()));
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

        // A list becomes an array of tables if every element is one, and a
        // plain array otherwise.  Extract the elements once and then decide:
        // speculatively extracting as an array of tables and falling back
        // would traverse the whole subtree twice at every level of nesting.
        if obj.is_instance_of::<PyList>() {
            let items = value::extract_sequence_items(obj)?;
            let item = if !items.is_empty() && items.iter().all(item_ops::is_table) {
                ItemRs::ArrayOfTables(value::items_into_array_of_tables(items)?)
            } else {
                ItemRs::Value(ValueRs::Array(value::items_into_array(items)?))
            };
            return Ok(Self(item));
        }

        let value = Value::extract(obj)?;
        Ok(Self(ItemRs::Value(value.0)))
    }
}
