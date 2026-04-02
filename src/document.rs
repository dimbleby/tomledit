use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PyTuple};

use toml_edit::DocumentMut as DocumentRs;

use crate::dict_ops;
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
    pub(crate) revision: u64,
    trie: MutationTrie,
}

impl Document {
    pub(crate) fn from_inner(inner: DocumentRs) -> Self {
        Self {
            inner,
            revision: 0,
            trie: MutationTrie::new(),
        }
    }

    fn make_proxy(slf: &Bound<'_, Self>, key: &str) -> ItemProxy {
        let doc = slf.borrow();
        let document_py: Py<Document> = slf.clone().unbind();
        ItemProxy::new(document_py, vec![Key::Str(key.to_owned())], doc.revision)
    }

    /// Record a mutation at the given path, incrementing the document revision.
    pub(crate) fn bump_at(&mut self, path: &[Key]) {
        self.revision += 1;
        self.trie.stamp(path, self.revision);
    }

    /// Record a mutation at `path + [child]` without cloning the path.
    pub(crate) fn bump_at_child(&mut self, path: &[Key], child: &Key) {
        self.revision += 1;
        self.trie.stamp_child(path, child, self.revision);
    }

    /// Stamp each index in `from..to` as changed at `path`.
    pub(crate) fn bump_range(&mut self, path: &[Key], from: usize, to: usize) {
        self.revision += 1;
        for i in from..to {
            self.trie.stamp_child(path, &Key::Int(i), self.revision);
        }
    }

    /// Check whether a proxy at `path` created at `revision` is still fresh.
    pub(crate) fn check_fresh(&self, path: &[Key], revision: u64) -> PyResult<()> {
        if self.trie.is_valid(path, revision) {
            Ok(())
        } else {
            Err(pyo3::exceptions::PyRuntimeError::new_err(
                "stale: the document has been modified since this reference was created",
            ))
        }
    }
}

#[pymethods]
impl Document {
    #[new]
    #[pyo3(signature = (data=None))]
    fn new(data: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        match data {
            None => Ok(Self::from_inner(DocumentRs::new())),
            Some(obj) => {
                let table: Table = obj.extract().map_err(|_| {
                    PyTypeError::new_err("Document() argument must be a mapping or None")
                })?;
                Ok(Self::from_inner(DocumentRs::from(table.0)))
            }
        }
    }

    /// Parse a TOML string into a Document, preserving formatting.
    ///
    /// This is the main entry point for editing existing TOML files:
    /// comments, whitespace, and style are retained so that only the
    /// values you change are affected when you call ``doc.as_toml()``.
    #[staticmethod]
    fn parse(text: &str) -> PyResult<Self> {
        let document_rs = text
            .parse::<DocumentRs>()
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Self::from_inner(document_rs))
    }

    pub fn __contains__(&self, key: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Some(key) = item_ops::extract_key_str(key)? else {
            return Ok(false);
        };
        Ok(self.inner.contains_key(&key))
    }

    pub fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let list = PyList::empty(py);
        for (k, _) in self.inner.iter() {
            list.append(k)?;
        }
        Ok(list.try_iter()?.unbind())
    }

    pub fn keys(slf: &Bound<'_, Self>) -> KeysView {
        let doc = slf.borrow();
        KeysView::new(slf.clone().unbind(), vec![], doc.revision)
    }

    pub fn items(slf: &Bound<'_, Self>) -> ItemsView {
        let doc = slf.borrow();
        ItemsView::new(slf.clone().unbind(), vec![], doc.revision)
    }

    pub fn values(slf: &Bound<'_, Self>) -> ValuesView {
        let doc = slf.borrow();
        ValuesView::new(slf.clone().unbind(), vec![], doc.revision)
    }

    pub fn __len__(&self) -> usize {
        self.inner.len()
    }

    #[pyo3(signature = (key, default=None, /))]
    pub fn get(
        slf: &Bound<'_, Self>,
        key: &Bound<'_, PyAny>,
        default: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let Some(key) = item_ops::extract_key_str(key)? else {
            return Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()));
        };
        let doc = slf.borrow();
        if doc.inner.get(&key).is_some() {
            let proxy = Self::make_proxy(slf, &key);
            ItemProxy::into_typed(py, proxy)
        } else {
            Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()))
        }
    }

    pub fn __getitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let Some(key) = item_ops::extract_key_str(key)? else {
            return Err(PyKeyError::new_err(key.repr()?.to_string()));
        };
        let proxy = {
            let doc = slf.borrow();
            if !doc.inner.contains_key(&key) {
                return Err(PyKeyError::new_err(key.clone()));
            }
            Self::make_proxy(slf, &key)
        };
        let py = slf.py();
        ItemProxy::into_typed(py, proxy)
    }

    pub fn __setitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>, value: Item) -> PyResult<()> {
        let Some(key) = item_ops::extract_key_str(key)? else {
            return Err(PyTypeError::new_err("keys must be strings"));
        };
        let mut doc = slf.borrow_mut();
        let replaced = doc.inner.contains_key(&key);
        dict_ops::set_with_decor_preservation(doc.inner.as_item_mut(), &key, value);
        if replaced {
            doc.bump_at(&[Key::Str(key)]);
        }
        Ok(())
    }

    pub fn __delitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(key) = item_ops::extract_key_str(key)? else {
            return Err(PyKeyError::new_err(key.repr()?.to_string()));
        };
        let mut doc = slf.borrow_mut();
        if doc.inner.remove(&key).is_none() {
            return Err(PyKeyError::new_err(key));
        }
        doc.bump_at(&[Key::Str(key)]);
        Ok(())
    }

    #[pyo3(signature = (key, /, *default))]
    pub fn pop(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        key: &Bound<'_, PyAny>,
        default: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        let default = dict_ops::extract_pop_default(default)?;

        let Some(key) = item_ops::extract_key_str(key)? else {
            return match default {
                Some(d) => Ok(d),
                None => Err(PyKeyError::new_err(key.repr()?.to_string())),
            };
        };

        let mut doc = slf.borrow_mut();
        match doc.inner.remove(&key) {
            Some(item) => {
                doc.bump_at(&[Key::Str(key)]);
                item_ops::item_to_py(&item, py)
            }
            None => match default {
                Some(d) => Ok(d),
                None => Err(PyKeyError::new_err(key)),
            },
        }
    }

    pub fn popitem(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<(String, Py<PyAny>)> {
        let mut doc = slf.borrow_mut();
        let (key, removed) = dict_ops::item_popitem(doc.inner.as_item_mut())?;
        doc.bump_at(&[Key::Str(key.clone())]);
        let py_val = item_ops::item_to_py(&removed, py)?;
        Ok((key, py_val))
    }

    pub fn __or__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        if !dict_ops::is_mapping_like(other) {
            return Ok(py.NotImplemented());
        }
        let mut new_inner = slf.borrow().inner.clone();
        dict_ops::merge_other_into(new_inner.as_item_mut(), other, py)?;
        let doc = Self::from_inner(new_inner);
        Ok(Py::new(py, doc)?.into_any())
    }

    pub fn __ror__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        if !dict_ops::is_mapping_like(other) {
            return Ok(py.NotImplemented());
        }
        // LHS is a plain mapping → result should be a plain dict.
        // Pass LHS values through verbatim (no TOML round-trip) so that
        // non-TOML-compatible values like None are preserved.
        let dict = dict_ops::copy_mapping_to_pydict(other, py)?;
        let doc = slf.borrow();
        for (k, v) in doc.inner.iter() {
            dict.set_item(k, item_ops::item_to_py(v, py)?)?;
        }
        Ok(dict.into_any().unbind())
    }

    pub fn __ior__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<()> {
        Self::update(slf, Some(other), None)
    }

    #[pyo3(signature = (other=None, /, **kwargs))]
    pub fn update(
        slf: &Bound<'_, Self>,
        other: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let update = other
            .map(|obj| dict_ops::resolve_update(obj, slf))
            .transpose()?;
        let kwarg_pairs = dict_ops::extract_kwargs(kwargs)?;
        let mut doc = slf.borrow_mut();
        let mut replaced = match update {
            Some(u) => u.apply(doc.inner.as_item_mut())?,
            None => Vec::new(),
        };
        if !kwarg_pairs.is_empty() {
            replaced.extend(dict_ops::apply_update_pairs(
                doc.inner.as_item_mut(),
                kwarg_pairs,
            )?);
        }
        for key in replaced {
            doc.bump_at(&[Key::Str(key)]);
        }
        Ok(())
    }

    #[pyo3(signature = (key, default=None, /))]
    pub fn setdefault(
        slf: &Bound<'_, Self>,
        key: &Bound<'_, PyAny>,
        default: Option<Item>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let key = item_ops::extract_key_str(key)?
            .ok_or_else(|| pyo3::exceptions::PyTypeError::new_err("keys must be strings"))?;
        {
            let doc = slf.borrow();
            if doc.inner.contains_key(&key) {
                let proxy = Self::make_proxy(slf, &key);
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
            dict_ops::set_with_decor_preservation(doc.inner.as_item_mut(), &key, default);
        }
        let proxy = Self::make_proxy(slf, &key);
        ItemProxy::into_typed(py, proxy)
    }

    pub fn clear(&mut self) {
        self.inner.clear();
        self.bump_at(&[]);
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
        } else {
            equality::table_eq(self.inner.as_table(), other)
        }
    }

    pub fn __copy__(&self) -> Self {
        Self::from_inner(self.inner.clone())
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
