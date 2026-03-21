use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::item::Item;
use crate::item_ops::{self};
use crate::item_proxy::{ItemProxy, resolve_proxy};

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
        let mut base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;

        match item_ops::item_pop(item, index) {
            Ok(removed) => {
                let result = item_ops::item_to_py(&removed.0, py)?;
                base.bump_self(&mut doc);
                Ok(result)
            }
            Err(e) => Err(e),
        }
    }

    #[pyo3(signature = (value, /))]
    pub fn append(self_: PyRefMut<'_, Self>, py: Python<'_>, value: Item) -> PyResult<()> {
        let base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        item_ops::item_append(item, value)?;
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
        item_ops::item_insert(item, index, value)?;
        base.bump_self(&mut doc);
        Ok(())
    }

    #[pyo3(signature = (value, /))]
    pub fn remove(
        self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        // Resolve ItemProxy to its Python value before taking the mutable
        // borrow — otherwise equality comparison triggers dunder methods on the
        // proxy that try to re-borrow the same document and panic.
        let resolved = resolve_proxy(py, value)?;
        let value = resolved.as_ref().map_or(value, |v| v.bind(py));
        let mut base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        item_ops::item_remove(item, value)?;
        base.bump_self(&mut doc);
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
        item_ops::item_extend(item, items)?;
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
        item_ops::item_count(item, value)
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
        item_ops::item_index(item, value, start, stop)
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
        item_ops::item_set_multiline(item, indent)
    }
}
