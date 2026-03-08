mod comments;
mod document;
mod equality;
mod error;
mod item;
mod item_proxy;
mod value;

use document::Document;
use error::TomlError;
use item_proxy::ItemProxy;
use pyo3::prelude::*;

#[pymodule]
fn tomledit(py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Document>()?;
    m.add_class::<ItemProxy>()?;
    m.add("TomlError", py.get_type::<TomlError>())?;
    Ok(())
}
