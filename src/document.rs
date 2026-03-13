use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyTuple};

use toml_edit::DocumentMut as DocumentRs;

use crate::equality;
use crate::item::Item;
use crate::item_ops::{self, Key};
use crate::item_proxy::ItemProxy;
use crate::value::Table;

/// A TOML document that preserves formatting when edited.
///
/// Create an empty document with ``Document()``, or from a dict with
/// ``Document({"key": "value"})``.  To round-trip an existing TOML file
/// use ``Document.parse(text)`` which retains comments and whitespace.
#[pyclass(mapping, module = "tomledit")]
pub(crate) struct Document {
    pub(crate) inner: DocumentRs,
    pub(crate) generation: u64,
}

impl Document {
    fn make_proxy(slf: &Bound<'_, Self>, key: &str) -> ItemProxy {
        let doc = slf.borrow();
        let document_py: Py<Document> = slf.clone().unbind();
        ItemProxy::new(document_py, vec![Key::Str(key.to_owned())], doc.generation)
    }

    /// Bump the generation counter. Call this on every mutation.
    pub(crate) fn bump(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

#[pymethods]
impl Document {
    #[new]
    #[pyo3(signature = (data=None))]
    fn new(data: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        match data {
            None => Ok(Self {
                inner: DocumentRs::new(),
                generation: 0,
            }),
            Some(obj) => {
                if let Ok(dict) = obj.cast::<PyDict>() {
                    let table: Table = dict.extract()?;
                    Ok(Self {
                        inner: DocumentRs::from(table.0),
                        generation: 0,
                    })
                } else {
                    Err(PyTypeError::new_err(
                        "Document() argument must be a dict or None",
                    ))
                }
            }
        }
    }

    /// Parse a TOML string into a Document, preserving formatting.
    ///
    /// This is the main entry point for editing existing TOML files:
    /// comments, whitespace, and style are retained so that only the
    /// values you change are affected when you call ``str(doc)``.
    #[staticmethod]
    fn parse(text: &str) -> PyResult<Self> {
        let document_rs = text
            .parse::<DocumentRs>()
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            inner: document_rs,
            generation: 0,
        })
    }

    pub fn __contains__(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    pub fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let keys: Vec<&str> = self.inner.iter().map(|(k, _)| k).collect();
        let list = keys.into_pyobject(py)?;
        Ok(list.try_iter()?.unbind())
    }

    pub fn keys(&self) -> Vec<&str> {
        self.inner.iter().map(|(k, _)| k).collect()
    }

    pub fn items(slf: &Bound<'_, Self>) -> Vec<(String, ItemProxy)> {
        let doc = slf.borrow();
        doc.inner
            .iter()
            .map(|(k, _)| (k.to_owned(), Self::make_proxy(slf, k)))
            .collect()
    }

    pub fn values(slf: &Bound<'_, Self>) -> Vec<ItemProxy> {
        let doc = slf.borrow();
        doc.inner
            .iter()
            .map(|(k, _)| Self::make_proxy(slf, k))
            .collect()
    }

    pub fn __len__(&self) -> usize {
        self.inner.len()
    }

    #[pyo3(signature = (key, default=None, /))]
    pub fn get(
        slf: &Bound<'_, Self>,
        key: &str,
        default: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let doc = slf.borrow();
        if doc.inner.get(key).is_some() {
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
            if !doc.inner.contains_key(key) {
                return Err(PyKeyError::new_err(key.to_owned()));
            }
        }
        Ok(Self::make_proxy(slf, key))
    }

    pub fn __setitem__(slf: &Bound<'_, Self>, key: &str, value: Item) {
        let mut doc = slf.borrow_mut();
        let replaced = doc.inner.contains_key(key);
        item_ops::set_with_decor_preservation(doc.inner.as_item_mut(), key, value);
        if replaced {
            doc.bump();
        }
    }

    pub fn __delitem__(&mut self, key: &str) -> PyResult<()> {
        if self.inner.remove(key).is_none() {
            return Err(PyKeyError::new_err(key.to_owned()));
        }
        self.bump();
        Ok(())
    }

    #[pyo3(signature = (key, /, *default))]
    pub fn pop(
        &mut self,
        py: Python<'_>,
        key: &str,
        default: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        if default.len() > 1 {
            return Err(PyTypeError::new_err(format!(
                "pop expected at most 2 arguments, got {}",
                1 + default.len()
            )));
        }

        let default = if default.is_empty() {
            None
        } else {
            Some(default.get_item(0)?.unbind())
        };

        match self.inner.remove(key) {
            Some(item) => {
                self.bump();
                item_ops::item_to_py(&item, py)
            }
            None => match default {
                Some(d) => Ok(d),
                None => Err(PyKeyError::new_err(key.to_owned())),
            },
        }
    }

    #[pyo3(signature = (other=None, /, **kwargs))]
    pub fn update(
        slf: &Bound<'_, Self>,
        other: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let mut pairs = match other {
            Some(obj) => item_ops::extract_update_pairs(obj)?,
            None => Vec::new(),
        };
        if let Some(kw) = kwargs {
            for (k, v) in kw.iter() {
                let key: String = k.extract()?;
                let val: Item = v.extract()?;
                pairs.push((key, val));
            }
        }
        let mut doc = slf.borrow_mut();
        item_ops::apply_update_pairs(doc.inner.as_item_mut(), pairs)?;
        doc.bump();
        Ok(())
    }

    #[pyo3(signature = (key, default, /))]
    pub fn setdefault(slf: &Bound<'_, Self>, key: &str, default: Item) -> ItemProxy {
        {
            let mut doc = slf.borrow_mut();
            if !doc.inner.contains_key(key) {
                item_ops::set_with_decor_preservation(doc.inner.as_item_mut(), key, default);
            }
        }
        Self::make_proxy(slf, key)
    }

    pub fn clear(&mut self) {
        self.inner.clear();
        self.bump();
    }

    pub fn __str__(&self) -> String {
        self.inner.to_string()
    }

    pub fn __repr__(&self) -> String {
        format!("Document({} keys)", self.inner.len())
    }

    pub fn __bool__(&self) -> bool {
        !self.inner.is_empty()
    }

    pub fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        if let Ok(other_doc) = other.cast::<Self>() {
            let other_doc = other_doc.borrow();
            Ok(equality::items_structural_eq(
                self.inner.as_item(),
                other_doc.inner.as_item(),
            ))
        } else if let Ok(other_dict) = other.cast::<PyDict>() {
            equality::table_entries_eq(self.inner.iter(), self.inner.len(), other_dict)
        } else {
            Ok(false)
        }
    }

    pub fn __copy__(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            generation: 0,
        }
    }

    #[pyo3(signature = (_memo=None))]
    pub fn __deepcopy__(&self, _memo: Option<&Bound<'_, PyAny>>) -> Self {
        self.__copy__()
    }

    /// Normalize the formatting of the entire document.
    ///
    /// Re-applies standard TOML whitespace and trailing-comma rules.
    /// Useful after a series of mutations that may leave inconsistent
    /// spacing. Note: this removes comments.
    pub fn fmt(&mut self) {
        self.inner.fmt();
    }

    /// The entire document as a native Python dict.
    #[getter]
    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        for (k, v) in self.inner.iter() {
            dict.set_item(k, item_ops::item_to_py(v, py)?)?;
        }
        Ok(dict.into_any().unbind())
    }
}
