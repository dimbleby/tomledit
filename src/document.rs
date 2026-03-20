use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyTuple};

use toml_edit::DocumentMut as DocumentRs;

use crate::equality;
use crate::item::Item;
use crate::item_ops::{self, Key, table_to_pydict};
use crate::item_proxy::ItemProxy;
use crate::trie::MutationTrie;
use crate::value::Table;
use crate::views::{ItemsView, KeysView, ValuesView};

/// A TOML document that preserves formatting when edited.
///
/// Create an empty document with ``Document()``, or from a dict with
/// ``Document({"key": "value"})``.  To round-trip an existing TOML file
/// use ``Document.parse(text)`` which retains comments and whitespace.
#[pyclass(mapping, module = "tomledit")]
pub(crate) struct Document {
    pub(crate) inner: DocumentRs,
    pub(crate) trie: MutationTrie,
}

impl Document {
    fn make_proxy(slf: &Bound<'_, Self>, key: &str) -> ItemProxy {
        let doc = slf.borrow();
        let document_py: Py<Document> = slf.clone().unbind();
        ItemProxy::new(document_py, vec![Key::Str(key.to_owned())], doc.trie.clock)
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
                trie: MutationTrie::new(),
            }),
            Some(obj) => {
                if let Ok(dict) = obj.cast::<PyDict>() {
                    let table: Table = dict.extract()?;
                    Ok(Self {
                        inner: DocumentRs::from(table.0),
                        trie: MutationTrie::new(),
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
            trie: MutationTrie::new(),
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

    pub fn keys(slf: &Bound<'_, Self>) -> KeysView {
        KeysView::new(slf.clone().unbind(), vec![])
    }

    pub fn items(slf: &Bound<'_, Self>) -> ItemsView {
        ItemsView::new(slf.clone().unbind(), vec![])
    }

    pub fn values(slf: &Bound<'_, Self>) -> ValuesView {
        ValuesView::new(slf.clone().unbind(), vec![])
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
            let proxy = Self::make_proxy(slf, key);
            ItemProxy::into_typed(py, proxy)
        } else {
            Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()))
        }
    }

    pub fn __getitem__(slf: &Bound<'_, Self>, key: &str) -> PyResult<Py<PyAny>> {
        let proxy = {
            let doc = slf.borrow();
            if !doc.inner.contains_key(key) {
                return Err(PyKeyError::new_err(key.to_owned()));
            }
            Self::make_proxy(slf, key)
        };
        let py = slf.py();
        ItemProxy::into_typed(py, proxy)
    }

    pub fn __setitem__(slf: &Bound<'_, Self>, key: &str, value: Item) {
        let mut doc = slf.borrow_mut();
        let replaced = doc.inner.contains_key(key);
        item_ops::set_with_decor_preservation(doc.inner.as_item_mut(), key, value);
        if replaced {
            doc.trie.bump_at(&[Key::Str(key.to_owned())]);
        }
    }

    pub fn __delitem__(&mut self, key: &str) -> PyResult<()> {
        if self.inner.remove(key).is_none() {
            return Err(PyKeyError::new_err(key.to_owned()));
        }
        self.trie.bump_at(&[Key::Str(key.to_owned())]);
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
                self.trie.bump_at(&[Key::Str(key.to_owned())]);
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
        let replaced_keys = item_ops::apply_update_pairs(doc.inner.as_item_mut(), pairs)?;
        for key in replaced_keys {
            doc.trie.bump_at(&[Key::Str(key)]);
        }
        Ok(())
    }

    #[pyo3(signature = (key, default=None, /))]
    pub fn setdefault(
        slf: &Bound<'_, Self>,
        key: &str,
        default: Option<Item>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        {
            let doc = slf.borrow();
            if doc.inner.contains_key(key) {
                let proxy = Self::make_proxy(slf, key);
                return ItemProxy::into_typed(py, proxy);
            }
        }
        let default = default.ok_or_else(|| {
            pyo3::exceptions::PyTypeError::new_err(
                "setdefault() requires a default value: TOML has no null type",
            )
        })?;
        {
            let mut doc = slf.borrow_mut();
            item_ops::set_with_decor_preservation(doc.inner.as_item_mut(), key, default);
        }
        let proxy = Self::make_proxy(slf, key);
        ItemProxy::into_typed(py, proxy)
    }

    pub fn clear(&mut self) {
        self.inner.clear();
        self.trie.bump_root();
    }

    pub fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let dict = table_to_pydict(self.inner.iter(), py)?;
        dict.str().map(|s| s.to_string())
    }

    pub fn __repr__(&self) -> String {
        format!("Document({} keys)", self.inner.len())
    }

    /// Return the document serialised as a TOML string.
    pub fn as_toml(&self) -> String {
        self.inner.to_string()
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
            trie: MutationTrie::new(),
        }
    }

    #[pyo3(signature = (_memo=None))]
    pub fn __deepcopy__(&self, _memo: Option<&Bound<'_, PyAny>>) -> Self {
        self.__copy__()
    }

    /// Normalize formatting of the document's top-level entries.
    ///
    /// This only reformats root-level key/value entries. It does not recurse
    /// into nested tables or arrays; call `.fmt()` on a nested `Item` if you
    /// want to reformat that value specifically.
    /// Useful after mutations that leave awkward top-level whitespace.
    /// Note: comments on formatted root-level entries are removed.
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
