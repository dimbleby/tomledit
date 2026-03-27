use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use crate::dict_ops;
use crate::item::Item;
use crate::item_ops::{self, Key};
use crate::item_proxy::{self, ItemProxy};
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
        let result = ItemProxy::parse(py, text)?;
        if result.bind(py).is_instance_of::<DictProxy>() {
            Ok(result)
        } else {
            Err(PyValueError::new_err(format!(
                "DictItem.parse() requires a table value, got {}",
                result.bind(py).get_type().qualname()?,
            )))
        }
    }

    pub fn keys(self_: PyRef<'_, Self>, py: Python<'_>) -> PyResult<KeysView> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        Ok(KeysView::new(
            base.document.clone_ref(py),
            base.path.clone(),
        ))
    }

    pub fn values(self_: PyRef<'_, Self>, py: Python<'_>) -> PyResult<ValuesView> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        Ok(ValuesView::new(
            base.document.clone_ref(py),
            base.path.clone(),
        ))
    }

    pub fn items(self_: PyRef<'_, Self>, py: Python<'_>) -> PyResult<ItemsView> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        Ok(ItemsView::new(
            base.document.clone_ref(py),
            base.path.clone(),
        ))
    }

    #[pyo3(signature = (key, default=None, /))]
    pub fn get(
        self_: PyRef<'_, Self>,
        py: Python<'_>,
        key: &str,
        default: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        let item = base.navigate(&doc.inner)?;
        if dict_ops::item_has_key(item, key)? {
            base.child_proxy_typed(py, Key::Str(key.to_owned()))
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
        if default.len() > 1 {
            return Err(PyTypeError::new_err(format!(
                "pop expected at most 2 arguments, got {}",
                1 + default.len()
            )));
        }
        let default_val = if default.is_empty() {
            None
        } else {
            Some(default.get_item(0)?.unbind())
        };

        let resolved = item_proxy::resolve_proxy(py, key)?;
        let key = resolved.as_ref().map_or(key, |v| v.bind(py));

        let mut base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;

        match item_ops::item_pop(item, Some(key)) {
            Ok((removed, affected_key)) => {
                let result = item_ops::item_to_py(&removed.0, py)?;
                base.bump_affected(&mut doc, affected_key);
                Ok(result)
            }
            Err(e)
                if default_val.is_some()
                    && e.is_instance_of::<pyo3::exceptions::PyKeyError>(py) =>
            {
                Ok(default_val.unwrap())
            }
            Err(e) => Err(e),
        }
    }

    #[pyo3(signature = (other=None, /, **kwargs))]
    pub fn update(
        self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        other: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let mut pairs = match other {
            Some(obj) => dict_ops::extract_update_pairs(obj)?,
            None => Vec::new(),
        };
        if let Some(kw) = kwargs {
            for (k, v) in kw.iter() {
                let key: String = k.extract()?;
                let val: Item = v.extract()?;
                pairs.push((key, val));
            }
        }
        let base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let replaced_keys = dict_ops::apply_update_pairs(item, pairs)?;
        for key in replaced_keys {
            base.bump_child(&mut doc, Key::Str(key));
        }
        Ok(())
    }

    #[pyo3(signature = (key, default=None, /))]
    pub fn setdefault(
        self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        key: &str,
        default: Option<Item>,
    ) -> PyResult<Py<PyAny>> {
        let base = self_.into_super();
        {
            let mut doc = base.document.bind(py).borrow_mut();
            base.check_fresh(&doc)?;
            let item = base.navigate_mut(&mut doc.inner)?;

            if !dict_ops::item_has_key(item, key)? {
                let default = default.ok_or_else(|| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "setdefault() requires a default value: TOML has no null type",
                    )
                })?;
                dict_ops::set_with_decor_preservation(item, key, default);
            }
        }
        base.child_proxy_typed(py, Key::Str(key.to_owned()))
    }
}
