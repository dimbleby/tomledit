mod comments;
mod document;
mod equality;
mod item;
mod item_ops;
mod item_proxy;
mod value;

use document::Document;
use item_proxy::ItemProxy;
use pyo3::prelude::*;

#[pymodule]
fn tomledit(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Document>()?;
    m.add_class::<ItemProxy>()?;
    Ok(())
}
