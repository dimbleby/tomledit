use pyo3::exceptions::{PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyTuple};
use toml_edit::DocumentMut as DocumentRs;

use crate::dict_ops;
use crate::item::Item;
use crate::item_ops::{self, Key};
use crate::item_proxy::{ItemProxy, ProxyParts};
use crate::views::{ItemsView, KeysView, ValuesView};

/// A TOML table or inline table.
///
/// ``isinstance(item, DictItem)`` and
/// ``isinstance(item, MutableMapping)`` both work.
#[pyclass(frozen, name = "DictItem", module = "tomledit", mapping, extends = ItemProxy)]
pub(crate) struct DictProxy;

#[pymethods]
impl DictProxy {
    #[staticmethod]
    fn parse(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        crate::item_proxy::parse_as::<DictProxy>(py, text, "DictItem", "table")
    }

    // ---- container protocol ----

    pub fn __getitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = key.py();
        let Some(key_str) = item_ops::extract_key_str(key)? else {
            return Err(PyTypeError::new_err("TOML table keys must be strings"));
        };
        let base = slf.as_super().get();
        let parts = {
            let (doc, inner) = base.read_checked(py)?;
            let item = base.navigate(&inner)?;
            if !dict_ops::item_has_key(item, &key_str)? {
                return Err(PyKeyError::new_err(key_str));
            }
            base.snapshot_child(doc, &inner, Key::Str(key_str))?
        };
        parts.build(&base.document, py)
    }

    pub fn __setitem__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: &Bound<'_, PyAny>,
        value: Item,
    ) -> PyResult<()> {
        let key_str = item_ops::extract_key_str(key)?
            .ok_or_else(|| PyTypeError::new_err("TOML table keys must be strings"))?;
        let base = slf.as_super().get();
        let (doc, mut inner) = base.write_checked(py)?;
        let item = base.navigate_mut(&mut inner)?;
        if let Some(replaced_key) = dict_ops::item_setitem_str(item, key_str, value) {
            base.bump_child(doc, replaced_key);
        }
        Ok(())
    }

    pub fn __delitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = key.py();
        let Some(key_str) = item_ops::extract_key_str(key)? else {
            return Err(PyTypeError::new_err("TOML table keys must be strings"));
        };
        let base = slf.as_super().get();
        let (doc, mut inner) = base.write_checked(py)?;
        let item = base.navigate_mut(&mut inner)?;
        let (_removed, k) = dict_ops::table_pop(item, &key_str)?;
        base.bump_child(doc, k);
        Ok(())
    }

    pub fn __len__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<usize> {
        let base = slf.as_super().get();
        let (_doc, inner) = base.read_checked(py)?;
        let item = base.navigate(&inner)?;
        Ok(dict_ops::as_dict_like(item, "__len__")?.len())
    }

    pub fn __iter__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let base = slf.as_super().get();
        let (_doc, inner) = base.read_checked(py)?;
        let item = base.navigate(&inner)?;
        let tbl = dict_ops::as_dict_like(item, "__iter__")?;
        let keys: Vec<&str> = tbl.iter().map(|(k, _)| k).collect();
        let list = keys.into_pyobject(py)?;
        Ok(list.try_iter()?.unbind())
    }

    pub fn __contains__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = key.py();
        let Some(key_str) = item_ops::extract_key_str(key)? else {
            return Ok(false);
        };
        let base = slf.as_super().get();
        let (_doc, inner) = base.read_checked(py)?;
        let item = base.navigate(&inner)?;
        Ok(dict_ops::as_dict_like(item, "'in'")?.contains_key(&key_str))
    }

    // ---- dict-specific methods ----

    pub fn keys(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<KeysView> {
        let base = slf.as_super().get();
        let doc = base.doc(py);
        // Sample the revision *before* check_fresh so any mutation that
        // invalidates the parent after this read would stamp with a
        // revision > `rev`, and the new view's later check_fresh would
        // detect it.
        let rev = doc.revision();
        base.check_fresh(doc)?;
        Ok(KeysView::new(
            base.document.clone_ref(py),
            base.path.clone(),
            rev,
        ))
    }

    pub fn values(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<ValuesView> {
        let base = slf.as_super().get();
        let doc = base.doc(py);
        let rev = doc.revision();
        base.check_fresh(doc)?;
        Ok(ValuesView::new(
            base.document.clone_ref(py),
            base.path.clone(),
            rev,
        ))
    }

    pub fn items(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<ItemsView> {
        let base = slf.as_super().get();
        let doc = base.doc(py);
        let rev = doc.revision();
        base.check_fresh(doc)?;
        Ok(ItemsView::new(
            base.document.clone_ref(py),
            base.path.clone(),
            rev,
        ))
    }

    #[pyo3(signature = (key, default=None, /))]
    pub fn get(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: &Bound<'_, PyAny>,
        default: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let Some(key) = item_ops::extract_key_str(key)? else {
            return Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()));
        };
        let base = slf.as_super().get();
        let parts = {
            let (doc, inner) = base.read_checked(py)?;
            let item = base.navigate(&inner)?;
            if !dict_ops::item_has_key(item, &key)? {
                return Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()));
            }
            base.snapshot_child(doc, &inner, Key::Str(key))?
        };
        parts.build(&base.document, py)
    }

    #[pyo3(signature = (key, /, *default))]
    pub fn pop(
        slf: &Bound<'_, Self>,
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

        let base = slf.as_super().get();
        let pop_result = {
            let (doc, mut inner) = base.write_checked(py)?;
            let item = base.navigate_mut(&mut inner)?;
            dict_ops::table_pop(item, &key_str).map(|(removed, affected_key)| {
                base.bump_child(doc, affected_key);
                removed
            })
        };
        match pop_result {
            Ok(removed) => item_ops::item_to_py(&removed.0, py),
            Err(e) if default_val.is_some() && e.is_instance_of::<PyKeyError>(py) => {
                Ok(default_val.unwrap())
            }
            Err(e) => Err(e),
        }
    }

    pub fn popitem(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<(String, Py<PyAny>)> {
        let base = slf.as_super().get();
        let (key, removed) = {
            let (doc, mut inner) = base.write_checked(py)?;
            let item = base.navigate_mut(&mut inner)?;
            let (key, removed) = dict_ops::item_popitem(item)?;
            base.bump_child(doc, Key::Str(key.clone()));
            (key, removed)
        };
        let py_val = item_ops::item_to_py(&removed, py)?;
        Ok((key, py_val))
    }

    pub fn __or__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        if !dict_ops::is_mapping_like(other) {
            return Ok(py.NotImplemented());
        }
        let base = slf.as_super().get();
        let mut new_doc = {
            let (_doc, inner) = base.read_checked(py)?;
            let item = base.navigate(&inner)?;
            let mut nd = DocumentRs::new();
            nd["_"] = item.clone();
            nd
        };
        dict_ops::merge_other_into(&mut new_doc["_"], other)?;
        ProxyParts::wrap_fresh(new_doc, py)
    }

    pub fn __ror__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        if !dict_ops::is_mapping_like(other) {
            return Ok(py.NotImplemented());
        }
        let dict = dict_ops::copy_mapping_to_pydict(other, py)?;
        let base = slf.as_super().get();
        let (_doc, inner) = base.read_checked(py)?;
        let item = base.navigate(&inner)?;
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
        // Resolve before write lock — iteration may read inner.
        let update = other.map(|obj| dict_ops::resolve_update(obj)).transpose()?;
        let kwarg_pairs = dict_ops::extract_kwargs(kwargs)?;
        let base = slf.as_super().get();
        let (doc, mut inner) = base.write_checked(py)?;
        let item = base.navigate_mut(&mut inner)?;
        let mut replaced = match update {
            Some(u) => u.apply(item)?,
            None => Vec::new(),
        };
        if !kwarg_pairs.is_empty() {
            replaced.extend(dict_ops::apply_update_pairs(item, kwarg_pairs)?);
        }
        for key in replaced {
            base.bump_child(doc, Key::Str(key));
        }
        Ok(())
    }

    #[pyo3(signature = (key, default=None, /))]
    pub fn setdefault(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: &Bound<'_, PyAny>,
        default: Option<Item>,
    ) -> PyResult<Py<PyAny>> {
        let key = item_ops::extract_key_str(key)?
            .ok_or_else(|| pyo3::exceptions::PyTypeError::new_err("keys must be strings"))?;
        let base = slf.as_super().get();
        let parts = {
            let (doc, mut inner) = base.write_checked(py)?;
            let item = base.navigate_mut(&mut inner)?;
            if !dict_ops::item_has_key(item, &key)? {
                let default = default.ok_or_else(|| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "setdefault() requires a default value: TOML has no null type",
                    )
                })?;
                dict_ops::set_with_decor_preservation(item, &key, default);
            }
            base.snapshot_child(doc, &inner, Key::Str(key))?
        };
        parts.build(&base.document, py)
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
    pub fn get_implicit(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<bool> {
        let base = slf.as_super().get();
        let (_doc, inner) = base.read_checked(py)?;
        let item = base.navigate(&inner)?;
        match item {
            toml_edit::Item::Table(tbl) => Ok(tbl.is_implicit()),
            _ => Ok(false),
        }
    }

    #[setter]
    pub fn set_implicit(slf: &Bound<'_, Self>, py: Python<'_>, implicit: bool) -> PyResult<()> {
        let base = slf.as_super().get();
        let (_doc, mut inner) = base.write_checked(py)?;
        let item = base.navigate_mut(&mut inner)?;
        if let toml_edit::Item::Table(tbl) = item {
            tbl.set_implicit(implicit);
        }
        Ok(())
    }
}
