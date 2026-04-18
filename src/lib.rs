mod comments;
mod datetime_compat;
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
use scalar_proxy::ScalarProxy;
use views::{ItemsView, KeysView, ValuesView};

#[pymodule]
fn tomledit(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Document>()?;
    m.add_class::<ItemProxy>()?;
    m.add_class::<DictProxy>()?;
    m.add_class::<ListProxy>()?;
    m.add_class::<ScalarProxy>()?;
    m.add_class::<KeysView>()?;
    m.add_class::<ValuesView>()?;
    m.add_class::<ItemsView>()?;

    // Register as collections.abc subclasses so isinstance() checks work.
    let abc = m.py().import("collections.abc")?;
    for (abc_name, our_name) in [
        ("KeysView", "KeysView"),
        ("ValuesView", "ValuesView"),
        ("ItemsView", "ItemsView"),
        ("MutableMapping", "Document"),
        ("MutableMapping", "DictItem"),
        ("MutableSequence", "ListItem"),
    ] {
        abc.getattr(abc_name)?
            .call_method1("register", (m.getattr(our_name)?,))?;
    }
    Ok(())
}
