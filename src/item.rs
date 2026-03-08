use crate::document::Document;
use crate::item_proxy::ItemProxy;
use crate::value::{ArrayOfTables, Table, Value};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use toml_edit::Item as ItemRs;

#[pyclass(module = "tomledit")]
pub(crate) struct Item(pub(crate) ItemRs);

impl<'py> FromPyObject<'_, 'py> for Item {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        // If it's already an Item pyclass, extract directly.
        if let Ok(item) = obj.cast::<Self>() {
            return Ok(Self(item.borrow().0.clone()));
        }

        // If it's an ItemProxy (the Python-visible "Item"), resolve the path
        // and clone the underlying toml_edit item.
        if let Ok(proxy) = obj.cast::<ItemProxy>() {
            let item_rs = proxy.borrow().clone_item(obj.py())?;
            return Ok(Self(item_rs));
        }

        // A Document is structurally a table — extract it as one.
        if let Ok(doc) = obj.cast::<Document>() {
            let doc = doc.borrow();
            return Ok(Self(doc.inner.as_item().clone()));
        }

        if obj.is_none() {
            return Err(PyTypeError::new_err(
                "None is not a valid TOML value (TOML has no null type)",
            ));
        }

        if let Ok(table) = Table::extract(obj) {
            let item = ItemRs::Table(table.0);
            return Ok(Self(item));
        }

        if let Ok(array_of_tables) = ArrayOfTables::extract(obj) {
            let item = ItemRs::ArrayOfTables(array_of_tables.0);
            return Ok(Self(item));
        }

        if let Ok(value) = Value::extract(obj) {
            let item = ItemRs::Value(value.0);
            return Ok(Self(item));
        }

        let name = obj.get_type().name()?;
        let string = name.to_str()?;
        let text = format!("Could not convert object of type '{string}' to item");
        Err(PyTypeError::new_err(text))
    }
}
