use crate::error::TomlError;
use crate::value::{ArrayOfTables, Table, Value};
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

        if obj.is_none() {
            return Err(TomlError::new_err(
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
        Err(TomlError::new_err(text))
    }
}
