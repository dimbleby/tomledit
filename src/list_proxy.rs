use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::item::Item;
use crate::item_ops;
use crate::item_proxy::{ItemProxy, resolve_proxy};
use crate::list_ops;

/// A TOML array or array of tables.
///
/// ``isinstance(item, ListItem)`` and
/// ``isinstance(item, MutableSequence)`` both work.
#[pyclass(name = "ListItem", module = "tomledit", extends = ItemProxy)]
pub(crate) struct ListProxy;

#[pymethods]
impl ListProxy {
    #[staticmethod]
    fn parse(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        let result = ItemProxy::parse(py, text)?;
        if result.bind(py).is_instance_of::<ListProxy>() {
            Ok(result)
        } else {
            Err(PyValueError::new_err(format!(
                "ListItem.parse() requires an array value, got {}",
                result.bind(py).get_type().qualname()?,
            )))
        }
    }

    pub fn __iadd__(self_: PyRefMut<'_, Self>, values: &Bound<'_, PyAny>) -> PyResult<()> {
        Self::extend(self_, values.py(), values)
    }

    #[pyo3(signature = (index=None, /))]
    pub fn pop(
        self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        index: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        // Resolve proxy index before the mutable borrow — extract() on a
        // ScalarItem triggers __index__ which re-borrows the document.
        let resolved_index = index.map(|i| resolve_proxy(py, i)).transpose()?.flatten();
        let index = match (&resolved_index, index) {
            (Some(resolved), _) => Some(resolved.bind(py) as &Bound<'_, PyAny>),
            (None, orig) => orig,
        };
        let mut base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;

        let (removed, affected_key) = item_ops::item_pop(item, index)?;
        let result = item_ops::item_to_py(&removed.0, py)?;
        base.bump_affected(&mut doc, affected_key);
        Ok(result)
    }

    #[pyo3(signature = (value, /))]
    pub fn append(self_: PyRefMut<'_, Self>, py: Python<'_>, value: Item) -> PyResult<()> {
        let base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let target = list_ops::as_array_like_mut(item, "append()")?;
        list_ops::item_append(target, value)?;
        Ok(())
    }

    #[pyo3(signature = (index, value, /))]
    pub fn insert(
        self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        index: i64,
        value: Item,
    ) -> PyResult<()> {
        let mut base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let target = list_ops::as_array_like_mut(item, "insert()")?;
        let at_end = list_ops::item_insert(target, index, value)?;
        if !at_end {
            base.bump_self(&mut doc);
        }
        Ok(())
    }

    #[pyo3(signature = (value, /))]
    pub fn remove(
        self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let resolved = resolve_proxy(py, value)?;
        let value = resolved.as_ref().map_or(value, |v| v.bind(py));
        let mut base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let target = list_ops::as_array_like_mut(item, "remove()")?;
        let affected_key = list_ops::item_remove(target, value)?;
        base.bump_affected(&mut doc, affected_key);
        Ok(())
    }

    #[pyo3(signature = (values, /))]
    pub fn extend(
        self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        values: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let items: Vec<Item> = values
            .try_iter()?
            .map(|r| r.and_then(|v| v.extract::<Item>()))
            .collect::<PyResult<_>>()?;

        let base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let target = list_ops::as_array_like_mut(item, "extend()")?;
        list_ops::item_extend(target, items)?;
        Ok(())
    }

    #[pyo3(signature = (value, /))]
    pub fn count(
        self_: PyRef<'_, Self>,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<usize> {
        let resolved = resolve_proxy(py, value)?;
        let value = resolved.as_ref().map_or(value, |v| v.bind(py));
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        let item = base.navigate(&doc.inner)?;
        let target = list_ops::as_array_like(item, "count()")?;
        list_ops::item_count(target, value)
    }

    #[pyo3(signature = (value, start=None, stop=None, /))]
    pub fn index(
        self_: PyRef<'_, Self>,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
        start: Option<i64>,
        stop: Option<i64>,
    ) -> PyResult<usize> {
        let resolved = resolve_proxy(py, value)?;
        let value = resolved.as_ref().map_or(value, |v| v.bind(py));
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        let item = base.navigate(&doc.inner)?;
        let target = list_ops::as_array_like(item, "index()")?;
        list_ops::item_index(target, value, start, stop)
    }

    /// Format the array as multiline.
    ///
    /// Each element is placed on its own line, indented by *indent*
    /// spaces, with a trailing comma after the last element.
    /// Use ``.fmt()`` to collapse back to a single line.
    ///
    /// No-op on empty arrays.  Any comments on the array elements will
    /// be removed.
    #[pyo3(signature = (*, indent=4))]
    pub fn set_multiline(self_: PyRefMut<'_, Self>, py: Python<'_>, indent: usize) -> PyResult<()> {
        let base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let target = list_ops::as_array_like_mut(item, "set_multiline()")?;
        list_ops::item_set_multiline(target, indent)
    }
}
