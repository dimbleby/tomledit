use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use toml_edit::DocumentMut as DocumentRs;

use crate::dict_ops;
use crate::document::Document;
use crate::item::Item;
use crate::item_ops::{self, Key};
use crate::item_proxy::ItemProxy;
use crate::views::{ItemsView, KeysView, ValuesView};

/// A TOML table or inline table.
///
/// ``isinstance(item, DictItem)`` and
/// ``isinstance(item, MutableMapping)`` both work.
#[pyclass(name = "DictItem", module = "tomledit", extends = ItemProxy)]
pub(crate) struct DictProxy;

#[pymethods]
impl DictProxy {
    #[staticmethod]
    fn parse(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        crate::item_proxy::parse_as::<DictProxy>(py, text, "DictItem", "table")
    }

    pub fn keys(self_: PyRef<'_, Self>, py: Python<'_>) -> PyResult<KeysView> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        Ok(KeysView::new(
            base.document.clone_ref(py),
            base.path.clone(),
            doc.revision,
        ))
    }

    pub fn values(self_: PyRef<'_, Self>, py: Python<'_>) -> PyResult<ValuesView> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        Ok(ValuesView::new(
            base.document.clone_ref(py),
            base.path.clone(),
            doc.revision,
        ))
    }

    pub fn items(self_: PyRef<'_, Self>, py: Python<'_>) -> PyResult<ItemsView> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        Ok(ItemsView::new(
            base.document.clone_ref(py),
            base.path.clone(),
            doc.revision,
        ))
    }

    #[pyo3(signature = (key, default=None, /))]
    pub fn get(
        self_: PyRef<'_, Self>,
        py: Python<'_>,
        key: &Bound<'_, PyAny>,
        default: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let Some(key) = item_ops::extract_key_str(key)? else {
            return Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()));
        };
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        let item = base.navigate(&doc.inner)?;
        if dict_ops::item_has_key(item, &key)? {
            base.child_proxy_typed(py, Key::Str(key))
        } else {
            Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()))
        }
    }

    #[pyo3(signature = (key, /, *default))]
    pub fn pop(
        self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        key: &Bound<'_, PyAny>,
        default: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        let default_val = dict_ops::extract_pop_default(default)?;

        let Some(key_str) = item_ops::extract_key_str(key)? else {
            return match default_val {
                Some(d) => Ok(d),
                None => Err(PyKeyError::new_err(key.repr()?.to_string())),
            };
        };

        let base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;

        match dict_ops::table_pop(item, &key_str) {
            Ok((removed, affected_key)) => {
                base.bump_child(&mut doc, affected_key);
                let result = item_ops::item_to_py(&removed.0, py)?;
                Ok(result)
            }
            Err(e) if default_val.is_some() && e.is_instance_of::<PyKeyError>(py) => {
                Ok(default_val.unwrap())
            }
            Err(e) => Err(e),
        }
    }

    pub fn popitem(self_: PyRefMut<'_, Self>, py: Python<'_>) -> PyResult<(String, Py<PyAny>)> {
        let base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let (key, removed) = dict_ops::item_popitem(item)?;
        base.bump_child(&mut doc, Key::Str(key.clone()));
        let py_val = item_ops::item_to_py(&removed, py)?;
        Ok((key, py_val))
    }

    pub fn __or__(
        self_: PyRef<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        if !dict_ops::is_mapping_like(other) {
            return Ok(py.NotImplemented());
        }
        let base = self_.into_super();
        let mut new_doc = {
            let doc = base.document.bind(py).borrow();
            base.check_fresh(&doc)?;
            let item = base.navigate(&doc.inner)?;
            let mut nd = DocumentRs::new();
            nd["_"] = item.clone();
            nd
        };
        dict_ops::merge_other_into(&mut new_doc["_"], other, py)?;
        let doc_py = Py::new(py, Document::from_inner(new_doc))?;
        let proxy = ItemProxy::new(doc_py, vec![Key::Str("_".to_owned())], 0);
        ItemProxy::into_typed(py, proxy)
    }

    pub fn __ror__(
        self_: PyRef<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        if !dict_ops::is_mapping_like(other) {
            return Ok(py.NotImplemented());
        }
        // LHS is a plain mapping → result should be a plain dict.
        // Pass LHS values through verbatim (no TOML round-trip) so that
        // non-TOML-compatible values like None are preserved.
        let dict = dict_ops::copy_mapping_to_pydict(other, py)?;
        let base = self_.into_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        let item = base.navigate(&doc.inner)?;
        let tbl = item
            .as_table_like()
            .ok_or_else(|| item_ops::unsupported_op(item, "|"))?;
        for (k, v) in tbl.iter() {
            dict.set_item(k, item_ops::item_to_py(v, py)?)?;
        }
        Ok(dict.into_any().unbind())
    }

    pub fn __ior__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        Self::update(slf, py, Some(other), None)
    }

    #[pyo3(signature = (other=None, /, **kwargs))]
    pub fn update(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        // Extract the document reference, then drop the shared borrow
        // before resolve_update — this avoids a cell-level double-borrow
        // panic when `other` is the same proxy as `slf`.
        let self_doc_py = {
            let r = slf.borrow();
            r.as_super().document.clone_ref(py)
        };
        let self_doc = self_doc_py.bind(py);
        let update = other
            .map(|obj| dict_ops::resolve_update(obj, self_doc))
            .transpose()?;
        let kwarg_pairs = dict_ops::extract_kwargs(kwargs)?;
        let self_mut = slf.borrow_mut();
        let base = self_mut.into_super();
        let mut doc = self_doc.borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let mut replaced = match update {
            Some(u) => u.apply(item)?,
            None => Vec::new(),
        };
        if !kwarg_pairs.is_empty() {
            replaced.extend(dict_ops::apply_update_pairs(item, kwarg_pairs)?);
        }
        for key in replaced {
            base.bump_child(&mut doc, Key::Str(key));
        }
        Ok(())
    }

    #[pyo3(signature = (key, default=None, /))]
    pub fn setdefault(
        self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        key: &Bound<'_, PyAny>,
        default: Option<Item>,
    ) -> PyResult<Py<PyAny>> {
        let key = item_ops::extract_key_str(key)?
            .ok_or_else(|| pyo3::exceptions::PyTypeError::new_err("keys must be strings"))?;
        let base = self_.into_super();
        {
            let mut doc = base.document.bind(py).borrow_mut();
            base.check_fresh(&doc)?;
            let item = base.navigate_mut(&mut doc.inner)?;

            if !dict_ops::item_has_key(item, &key)? {
                let default = default.ok_or_else(|| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "setdefault() requires a default value: TOML has no null type",
                    )
                })?;
                dict_ops::set_with_decor_preservation(item, &key, default);
            }
        }
        base.child_proxy_typed(py, Key::Str(key))
    }

    /// Whether this table's header is implicit (suppressed in TOML output).
    ///
    /// An implicit table like ``[a]`` in ``[a.b]\nx = 1`` has no ``[a]``
    /// header — it exists only because ``a.b`` requires it.  Inline tables
    /// are never implicit; this always returns ``False`` for them.
    ///
    /// Setting to ``True`` suppresses the header; setting to ``False``
    /// makes it explicit.  Silently ignored on inline tables.
    #[getter]
    pub fn get_implicit(self_: PyRef<'_, Self>, py: Python<'_>) -> PyResult<bool> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        let item = base.navigate(&doc.inner)?;
        match item {
            toml_edit::Item::Table(tbl) => Ok(tbl.is_implicit()),
            _ => Ok(false),
        }
    }

    #[setter]
    pub fn set_implicit(self_: PyRef<'_, Self>, py: Python<'_>, implicit: bool) -> PyResult<()> {
        let base = self_.as_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        if let toml_edit::Item::Table(tbl) = item {
            tbl.set_implicit(implicit);
        }
        Ok(())
    }
}
