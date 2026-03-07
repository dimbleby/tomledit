use pyo3::exceptions::{PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator};

use toml_edit::DocumentMut as DocumentRs;

use crate::error::TomlErrorWrapper;
use crate::item::Item;
use crate::item_proxy::{self, ItemProxy, Key};
use crate::value::Table;

#[pyclass(mapping, module = "tomledit")]
pub(crate) struct Document(pub(crate) DocumentRs);

impl Document {
    fn make_proxy(slf: &Bound<'_, Self>, key: &str) -> ItemProxy {
        let document_py: Py<Document> = slf.clone().unbind();
        ItemProxy::new(document_py, vec![Key::Str(key.to_owned())])
    }
}

#[pymethods]
impl Document {
    #[new]
    #[pyo3(signature = (data=None))]
    fn new(data: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        match data {
            None => Ok(Self(DocumentRs::new())),
            Some(obj) => {
                if let Ok(dict) = obj.cast::<PyDict>() {
                    let table = Table::extract_from_dict(dict)?;
                    Ok(Self(DocumentRs::from(table.0)))
                } else {
                    Err(PyTypeError::new_err(
                        "Document() argument must be a dict or None",
                    ))
                }
            }
        }
    }

    #[staticmethod]
    fn parse(text: &str) -> PyResult<Self> {
        let document_rs = text.parse::<DocumentRs>().map_err(TomlErrorWrapper::from)?;
        Ok(Self(document_rs))
    }

    pub fn __contains__(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let keys: Vec<String> = self.0.iter().map(|(k, _)| k.to_owned()).collect();
        let list = keys.into_pyobject(py)?;
        Ok(list.try_iter()?.unbind())
    }

    pub fn keys(&self) -> Vec<String> {
        self.0.iter().map(|(k, _)| k.to_owned()).collect()
    }

    pub fn items(slf: &Bound<'_, Self>) -> Vec<(String, ItemProxy)> {
        let doc = slf.borrow();
        doc.0
            .iter()
            .map(|(k, _)| (k.to_owned(), Self::make_proxy(slf, k)))
            .collect()
    }

    pub fn values(slf: &Bound<'_, Self>) -> Vec<ItemProxy> {
        let doc = slf.borrow();
        doc.0
            .iter()
            .map(|(k, _)| Self::make_proxy(slf, k))
            .collect()
    }

    pub fn __len__(&self) -> usize {
        self.0.len()
    }

    #[pyo3(signature = (key, default=None))]
    pub fn get(
        slf: &Bound<'_, Self>,
        key: &str,
        default: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let doc = slf.borrow();
        if doc.0.get(key).is_some() {
            Ok(Self::make_proxy(slf, key)
                .into_pyobject(py)?
                .into_any()
                .unbind())
        } else {
            Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()))
        }
    }

    pub fn __getitem__(slf: &Bound<'_, Self>, key: &str) -> PyResult<ItemProxy> {
        {
            let doc = slf.borrow();
            if !doc.0.contains_key(key) {
                return Err(PyKeyError::new_err(key.to_owned()));
            }
        }
        Ok(Self::make_proxy(slf, key))
    }

    pub fn __setitem__(&mut self, key: &str, value: Item) {
        item_proxy::set_with_decor_preservation(self.0.as_item_mut(), key, value);
    }

    pub fn __delitem__(&mut self, key: &str) -> PyResult<()> {
        if self.0.remove(key).is_none() {
            return Err(PyKeyError::new_err(key.to_owned()));
        }
        Ok(())
    }

    #[pyo3(signature = (key, default=None))]
    pub fn pop(
        &mut self,
        py: Python<'_>,
        key: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match self.0.remove(key) {
            Some(item) => item_proxy::item_to_py(&item, py),
            None => match default {
                Some(d) => Ok(d),
                None => Err(PyKeyError::new_err(key.to_owned())),
            },
        }
    }

    pub fn update(&mut self, other: &Bound<'_, PyAny>) -> PyResult<()> {
        item_proxy::item_update(self.0.as_item_mut(), other)
    }

    pub fn setdefault(slf: &Bound<'_, Self>, key: &str, default: Item) -> ItemProxy {
        {
            let mut doc = slf.borrow_mut();
            if !doc.0.contains_key(key) {
                item_proxy::set_with_decor_preservation(doc.0.as_item_mut(), key, default);
            }
        }
        Self::make_proxy(slf, key)
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    pub fn __str__(&self) -> String {
        self.0.to_string()
    }

    pub fn __repr__(&self) -> String {
        format!("Document({} keys)", self.0.len())
    }

    pub fn __bool__(&self) -> bool {
        !self.0.is_empty()
    }

    pub fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        if let Ok(other_doc) = other.cast::<Self>() {
            let other_doc = other_doc.borrow();
            let py = other.py();
            let self_dict = PyDict::new(py);
            for (k, v) in self.0.iter() {
                self_dict.set_item(k, item_proxy::item_to_py(v, py)?)?;
            }
            let other_dict = PyDict::new(py);
            for (k, v) in other_doc.0.iter() {
                other_dict.set_item(k, item_proxy::item_to_py(v, py)?)?;
            }
            self_dict.eq(&other_dict).map_err(PyErr::from)
        } else if let Ok(other_dict) = other.cast::<PyDict>() {
            item_proxy::doc_entries_eq(self.0.iter(), self.0.len(), other_dict)
        } else {
            Ok(false)
        }
    }

    pub fn fmt(&mut self) {
        self.0.fmt()
    }

    /// The entire document as a native Python dict.
    #[getter]
    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        for (k, v) in self.0.iter() {
            dict.set_item(k, item_proxy::item_to_py(v, py)?)?;
        }
        Ok(dict.into_any().unbind())
    }
}
