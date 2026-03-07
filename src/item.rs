use crate::error::TomlError;
use crate::ops;
use crate::value::{ArrayOfTables, Table, Value};
use pyo3::exceptions::{PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyIterator, PySlice};
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
            let item = ItemRs::None;
            return Ok(Self(item));
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

#[pymethods]
impl Item {
    // ---- core protocol ----

    pub fn __getitem__(&self, key: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = key.py();

        // Slice support: return a list of cloned items.
        if let Ok(slice) = key.cast::<PySlice>() {
            let len = ops::require_array_like_len(&self.0)?;
            let si = slice.indices(len as isize)?;
            let indices = ops::collect_slice_indices(si.start, si.stop, si.step);
            let items: Vec<Item> = indices
                .iter()
                .filter_map(|&i| self.0.get(i).map(|v| Item(v.clone())))
                .collect();
            return Ok(items.into_pyobject(py)?.into_any().unbind());
        }

        let item_rs = if let Ok(idx) = key.extract::<i64>() {
            let len = ops::item_len(&self.0).ok_or_else(|| {
                PyTypeError::new_err(format!("'{}' is not subscriptable", self.0.type_name()))
            })?;
            let resolved = ops::resolve_index(idx, len)?;
            self.0.get(resolved)
        } else if let Ok(key_str) = key.extract::<String>() {
            self.0.get(key_str)
        } else {
            return Err(ops::bad_key_type(key));
        };

        match item_rs {
            Some(item_rs) => Ok(Item(item_rs.clone()).into_pyobject(py)?.into_any().unbind()),
            None => Err(PyKeyError::new_err(format!("{key}"))),
        }
    }

    pub fn __setitem__(
        &mut self,
        key: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        if let Ok(slice) = key.cast::<PySlice>() {
            let len = ops::require_array_like_len(&self.0)?;
            let si = slice.indices(len as isize)?;
            let values: Vec<Item> = value
                .try_iter()?
                .map(|r| r.and_then(|v| v.extract::<Item>()))
                .collect::<PyResult<_>>()?;
            return ops::item_setitem_slice(&mut self.0, si.start, si.stop, si.step, values);
        }

        let value: Item = value.extract()?;
        ops::item_setitem(&mut self.0, key, value)
    }

    pub fn __delitem__(&mut self, key: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(slice) = key.cast::<PySlice>() {
            let len = ops::require_array_like_len(&self.0)?;
            let si = slice.indices(len as isize)?;
            let indices = ops::collect_slice_indices(si.start, si.stop, si.step);
            return ops::item_delitem_slice(&mut self.0, &indices);
        }

        ops::item_delitem(&mut self.0, key)
    }

    pub fn __len__(&self) -> PyResult<usize> {
        ops::item_len(&self.0)
            .ok_or_else(|| PyTypeError::new_err(format!("'{}' has no len()", self.0.type_name())))
    }

    pub fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        match ops::item_iter_info(&self.0)? {
            ops::IterKind::TableKeys(keys) => {
                let list = keys.into_pyobject(py)?;
                Ok(list.try_iter()?.unbind())
            }
            ops::IterKind::ArrayLen(len) => {
                let items: Vec<Item> = (0..len)
                    .filter_map(|i| self.0.get(i).map(|v| Item(v.clone())))
                    .collect();
                let list = items.into_pyobject(py)?;
                Ok(list.try_iter()?.unbind())
            }
        }
    }

    pub fn __contains__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        ops::item_contains(&self.0, value)
    }

    pub fn __bool__(&self) -> bool {
        ops::item_bool(&self.0)
    }

    pub fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        ops::item_str(&self.0, py)
    }

    pub fn __repr__(&self) -> String {
        ops::item_repr(&self.0, "Item")
    }

    pub fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        ops::item_eq(&self.0, other)
    }

    /// The underlying data as a native Python object (int, str, list, dict, etc).
    #[getter]
    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        ops::item_to_py(&self.0, py)
    }

    // ---- dict-like methods ----

    pub fn keys(&self) -> PyResult<Vec<String>> {
        ops::item_keys(&self.0)
    }

    pub fn values(&self) -> PyResult<Vec<Item>> {
        let keys = ops::item_keys(&self.0)?;
        Ok(keys
            .into_iter()
            .filter_map(|k| self.0.get(&k).map(|v| Item(v.clone())))
            .collect())
    }

    pub fn items(&self) -> PyResult<Vec<(String, Item)>> {
        let keys = ops::item_keys(&self.0)?;
        Ok(keys
            .into_iter()
            .filter_map(|k| self.0.get(&k).map(|v| (k, Item(v.clone()))))
            .collect())
    }

    #[pyo3(signature = (key, default=None))]
    pub fn get(
        &self,
        py: Python<'_>,
        key: &str,
        default: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        if ops::item_has_key(&self.0, key)?
            && let Some(v) = self.0.get(key)
        {
            return Ok(Item(v.clone()).into_pyobject(py)?.into_any().unbind());
        }
        Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()))
    }

    #[pyo3(signature = (key=None))]
    pub fn pop(&mut self, key: Option<&Bound<'_, PyAny>>) -> PyResult<Item> {
        ops::item_pop(&mut self.0, key)
    }

    pub fn update(&mut self, other: &Bound<'_, PyAny>) -> PyResult<()> {
        ops::item_update(&mut self.0, other)
    }

    pub fn setdefault(&mut self, key: &str, default: Item) -> PyResult<Item> {
        if !ops::item_has_key(&self.0, key)? {
            ops::set_with_decor_preservation(&mut self.0, key, default);
        }
        self.0
            .get(key)
            .map(|v| Item(v.clone()))
            .ok_or_else(|| PyKeyError::new_err(key.to_owned()))
    }

    // ---- list-like methods ----

    pub fn append(&mut self, value: Item) -> PyResult<()> {
        ops::item_append(&mut self.0, value)
    }

    pub fn insert(&mut self, index: usize, value: Item) -> PyResult<()> {
        ops::item_insert(&mut self.0, index, value)
    }

    pub fn remove(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        ops::item_remove(&mut self.0, value)
    }

    pub fn extend(&mut self, values: &Bound<'_, PyAny>) -> PyResult<()> {
        let items: Vec<Item> = values
            .try_iter()?
            .map(|r| r.and_then(|v| v.extract::<Item>()))
            .collect::<PyResult<_>>()?;
        ops::item_extend(&mut self.0, items)
    }

    // ---- shared ----

    pub fn clear(&mut self) -> PyResult<()> {
        ops::item_clear(&mut self.0)
    }
}
