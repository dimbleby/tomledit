mod comments;
mod dict_ops;
mod dict_proxy;
mod document;
mod equality;
mod item;
mod item_ops;
mod item_proxy;
mod list_ops;
mod list_proxy;
mod py_pairs;
mod scalar_proxy;
mod trie;
mod value;
mod views;

use dict_proxy::DictProxy;
use document::Document;
use item_proxy::ItemProxy;
use list_proxy::ListProxy;
use pyo3::prelude::*;
use pyo3::types::IntoPyDict;
use scalar_proxy::ScalarProxy;
use views::{ItemsView, KeysView, ValuesView};

#[pymodule]
fn tomledit(py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Document>()?;
    m.add_class::<ItemProxy>()?;
    m.add_class::<DictProxy>()?;
    m.add_class::<ListProxy>()?;
    m.add_class::<ScalarProxy>()?;
    m.add_class::<KeysView>()?;
    m.add_class::<ValuesView>()?;
    m.add_class::<ItemsView>()?;

    // Register as collections.abc subclasses so isinstance() checks work.
    py.run(
        pyo3::ffi::c_str!(
            "from collections.abc import KeysView, ValuesView, ItemsView, MutableMapping, MutableSequence\n\
             KeysView.register(_KV)\n\
             ValuesView.register(_VV)\n\
             ItemsView.register(_IV)\n\
             MutableMapping.register(_Doc)\n\
             MutableMapping.register(_DI)\n\
             MutableSequence.register(_LI)\n"
        ),
        Some(
            &[
                ("_KV", m.getattr("KeysView")?),
                ("_VV", m.getattr("ValuesView")?),
                ("_IV", m.getattr("ItemsView")?),
                ("_Doc", m.getattr("Document")?),
                ("_DI", m.getattr("DictItem")?),
                ("_LI", m.getattr("ListItem")?),
            ]
            .into_py_dict(py)?,
        ),
        None,
    )?;
    Ok(())
}
