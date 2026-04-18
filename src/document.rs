use parking_lot::RwLock;

use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PyTuple};

use toml_edit::DocumentMut as DocumentRs;

use crate::dict_ops;
use crate::equality;
use crate::item::Item;
use crate::item_ops::{self, Key, table_to_pydict};
use crate::item_proxy::{ProxyParts, with_resolved_item};
use crate::trie::MutationTrie;
use crate::value::Table;
use crate::views::{ItemsView, KeysView, ValuesView};

/// A TOML document that preserves formatting when edited.
///
/// Create an empty document with ``Document()``, or from a dict with
/// ``Document({"key": "value"})``.  To round-trip an existing TOML file
/// use ``Document.parse(text)`` which retains comments and whitespace.
#[pyclass(frozen, mapping, module = "tomledit")]
pub(crate) struct Document {
    pub(crate) inner: RwLock<DocumentRs>,
    pub(crate) trie: RwLock<MutationTrie>,
}

impl Document {
    pub(crate) fn from_inner(inner: DocumentRs) -> Self {
        Self {
            inner: RwLock::new(inner),
            trie: RwLock::new(MutationTrie::new()),
        }
    }

    /// The current document revision (read from the trie).
    pub(crate) fn revision(&self) -> u64 {
        self.trie.read().revision()
    }

    /// Snapshot the parts needed to build a typed proxy for the root
    /// entry at `key`.  Caller must hold `inner`; revision is sampled
    /// internally so it stays consistent with the held guard.
    ///
    /// Mutators stamp the trie before releasing `inner.write()`, so any
    /// later mutation will produce a strictly greater revision and a proxy
    /// minted from these parts will correctly invalidate.
    pub(crate) fn snapshot_child(&self, inner: &DocumentRs, key: String) -> PyResult<ProxyParts> {
        ProxyParts::snapshot(inner, vec![Key::Str(key)], self.revision())
    }

    /// Record a mutation at the given path. Returns the new revision.
    pub(crate) fn bump_at(&self, path: &[Key]) -> u64 {
        self.trie.write().stamp(path)
    }

    /// Record a mutation at `path + [child]` without cloning the path.
    /// Returns the new revision.
    pub(crate) fn bump_at_child(&self, path: &[Key], child: &Key) -> u64 {
        self.trie.write().stamp_child(path, child)
    }

    /// Stamp each index in `from..to` as changed at `path`.
    /// Returns the new revision.
    pub(crate) fn bump_range(&self, path: &[Key], from: usize, to: usize) -> u64 {
        self.trie.write().stamp_range(path, from, to)
    }

    /// Check whether a proxy at `path` created at `revision` is still fresh.
    pub(crate) fn check_fresh(&self, path: &[Key], revision: u64) -> PyResult<()> {
        if self.trie.read().is_valid(path, revision) {
            Ok(())
        } else {
            Err(pyo3::exceptions::PyRuntimeError::new_err(
                "stale: the document has been modified since this reference was created",
            ))
        }
    }

    /// Acquire `inner.read()` and then verify the proxy is still fresh.
    ///
    /// Locking first closes the TOCTOU window between the freshness check
    /// and the read: mutations take `inner.write()` while they stamp the
    /// trie, so once we hold `inner.read()` the trie has already recorded
    /// any invalidating mutation that could affect us.
    pub(crate) fn read_checked(
        &self,
        path: &[Key],
        revision: u64,
    ) -> PyResult<parking_lot::RwLockReadGuard<'_, DocumentRs>> {
        let guard = self.inner.read();
        self.check_fresh(path, revision)?;
        Ok(guard)
    }

    /// Acquire `inner.write()` and then verify the proxy is still fresh.
    pub(crate) fn write_checked(
        &self,
        path: &[Key],
        revision: u64,
    ) -> PyResult<parking_lot::RwLockWriteGuard<'_, DocumentRs>> {
        let guard = self.inner.write();
        self.check_fresh(path, revision)?;
        Ok(guard)
    }
}

#[pymethods]
impl Document {
    #[new]
    #[pyo3(signature = (data=None))]
    fn new(data: Option<Table>) -> Self {
        match data {
            None => Self::from_inner(DocumentRs::new()),
            Some(table) => Self::from_inner(DocumentRs::from(table.0)),
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
        Ok(self.inner.read().contains_key(&key))
    }

    pub fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let list = PyList::empty(py);
        for (k, _) in self.inner.read().iter() {
            list.append(k)?;
        }
        Ok(list.try_iter()?.unbind())
    }

    pub fn keys(slf: &Bound<'_, Self>) -> KeysView {
        let doc = slf.get();
        KeysView::new(slf.clone().unbind(), vec![], doc.revision())
    }

    pub fn items(slf: &Bound<'_, Self>) -> ItemsView {
        let doc = slf.get();
        ItemsView::new(slf.clone().unbind(), vec![], doc.revision())
    }

    pub fn values(slf: &Bound<'_, Self>) -> ValuesView {
        let doc = slf.get();
        ValuesView::new(slf.clone().unbind(), vec![], doc.revision())
    }

    pub fn __len__(&self) -> usize {
        self.inner.read().len()
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
        let doc = slf.get();
        let parts = {
            let inner = doc.inner.read();
            if inner.get(&key).is_none() {
                return Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()));
            }
            doc.snapshot_child(&inner, key)?
        };
        parts.build(slf.as_unbound(), py)
    }

    pub fn __getitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let Some(key) = item_ops::extract_key_str(key)? else {
            return Err(PyKeyError::new_err(key.repr()?.to_string()));
        };
        let doc = slf.get();
        let parts = {
            let inner = doc.inner.read();
            if !inner.contains_key(&key) {
                return Err(PyKeyError::new_err(key));
            }
            doc.snapshot_child(&inner, key)?
        };
        parts.build(slf.as_unbound(), slf.py())
    }

    pub fn __setitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>, value: Item) -> PyResult<()> {
        let Some(key) = item_ops::extract_key_str(key)? else {
            return Err(PyTypeError::new_err("keys must be strings"));
        };
        let doc = slf.get();
        let mut inner = doc.inner.write();
        let replaced = inner.contains_key(&key);
        dict_ops::set_with_decor_preservation(inner.as_item_mut(), &key, value);
        if replaced {
            doc.bump_at(&[Key::Str(key)]);
        }
        Ok(())
    }

    pub fn __delitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(key) = item_ops::extract_key_str(key)? else {
            return Err(PyKeyError::new_err(key.repr()?.to_string()));
        };
        let doc = slf.get();
        let mut inner = doc.inner.write();
        if inner.remove(&key).is_none() {
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

        let doc = slf.get();
        let removed = {
            let mut inner = doc.inner.write();
            match inner.remove(&key) {
                Some(item) => {
                    doc.bump_at(&[Key::Str(key)]);
                    item
                }
                None => return default.ok_or_else(|| PyKeyError::new_err(key)),
            }
        };
        item_ops::item_to_py(&removed, py)
    }

    pub fn popitem(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<(String, Py<PyAny>)> {
        let doc = slf.get();
        let (key, removed) = {
            let mut inner = doc.inner.write();
            let (key, removed) = dict_ops::item_popitem(inner.as_item_mut())?;
            doc.bump_at(&[Key::Str(key.clone())]);
            (key, removed)
        };
        let py_val = item_ops::item_to_py(&removed, py)?;
        Ok((key, py_val))
    }

    pub fn __or__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        if !dict_ops::is_mapping_like(other) {
            return Ok(py.NotImplemented());
        }
        let mut new_inner = slf.get().inner.read().clone();
        dict_ops::merge_other_into(new_inner.as_item_mut(), other)?;
        let doc = Self::from_inner(new_inner);
        Ok(Py::new(py, doc)?.into_any())
    }

    pub fn __ror__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        if !dict_ops::is_mapping_like(other) {
            return Ok(py.NotImplemented());
        }
        let dict = dict_ops::copy_mapping_to_pydict(other, py)?;
        let inner = slf.get().inner.read();
        for (k, v) in inner.iter() {
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
        let doc = slf.get();
        // Resolve before write lock — iteration may read inner.
        let update = other.map(|obj| dict_ops::resolve_update(obj)).transpose()?;
        let kwarg_pairs = dict_ops::extract_kwargs(kwargs)?;
        let mut inner = doc.inner.write();
        let mut replaced = match update {
            Some(u) => u.apply(inner.as_item_mut())?,
            None => Vec::new(),
        };
        if !kwarg_pairs.is_empty() {
            replaced.extend(dict_ops::apply_update_pairs(
                inner.as_item_mut(),
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
        let doc = slf.get();
        let parts = {
            let mut inner = doc.inner.write();
            if !inner.contains_key(&key) {
                let default = default.ok_or_else(|| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "setdefault() requires a default value: TOML has no null type",
                    )
                })?;
                dict_ops::set_with_decor_preservation(inner.as_item_mut(), &key, default);
            }
            doc.snapshot_child(&inner, key)?
        };
        parts.build(slf.as_unbound(), py)
    }

    pub fn clear(&self) {
        let mut inner = self.inner.write();
        inner.clear();
        self.bump_at(&[]);
    }

    pub fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let dict = table_to_pydict(self.inner.read().iter(), py)?;
        dict.str().map(|s| s.to_string())
    }

    pub fn __repr__(&self) -> String {
        format!("Document({} keys)", self.inner.read().len())
    }

    /// Return the document serialised as a TOML string.
    pub fn as_toml(&self) -> String {
        self.inner.read().to_string()
    }

    pub fn __bool__(&self) -> bool {
        !self.inner.read().is_empty()
    }

    pub fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(with_resolved_item(
            other,
            self,
            |_| Ok(()),
            |inner, needle| Ok(equality::items_structural_eq(inner.as_item(), needle)),
        )?
        .unwrap_or(false))
    }

    pub fn __copy__(&self) -> Self {
        Self::from_inner(self.inner.read().clone())
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
    pub fn fmt(&self) {
        self.inner.write().fmt();
    }

    /// The entire document as a native Python dict.
    #[getter]
    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        for (k, v) in self.inner.read().iter() {
            dict.set_item(k, item_ops::item_to_py(v, py)?)?;
        }
        Ok(dict.into_any().unbind())
    }
}
