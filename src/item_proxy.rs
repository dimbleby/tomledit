use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyIterator;
use toml_edit::DocumentMut as DocumentRs;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::dict_ops;
use crate::dict_proxy::DictProxy;
use crate::document::Document;
use crate::equality;
use crate::item::Item;
use crate::item_ops::{self, Affected, Key};
use crate::list_ops;
use crate::list_proxy::ListProxy;
use crate::scalar_proxy::ScalarProxy;

/// If `value` is an [`ItemProxy`] (or subclass such as `ScalarItem`), resolve
/// it to its underlying Python value so that subsequent operations can compare
/// plain Python objects without re-borrowing the document through dunder
/// methods. Returns `None` when `value` is not a proxy.
pub(crate) fn resolve_proxy(
    _py: Python<'_>,
    value: &Bound<'_, PyAny>,
) -> PyResult<Option<Py<PyAny>>> {
    if let Ok(proxy) = value.cast::<ItemProxy>() {
        Ok(Some(proxy.borrow().value(value.py())?))
    } else {
        Ok(None)
    }
}

/// A live reference to a TOML value inside a Document.
///
/// Items are obtained by indexing a Document or another Item:
///
///     port = doc["server"]["port"]
///     doc["server"]["port"] = 8080
///
/// Every Item is one of three concrete subtypes — ``DictItem`` for tables,
/// ``ListItem`` for arrays, or ``ScalarItem`` for plain values — which can
/// be narrowed with ``isinstance`` if needed.  Without narrowing, the full
/// set of dict-like, list-like, and scalar methods is available directly
/// on Item and will raise at runtime if called on the wrong kind.
///
/// Under the hood, an Item is really a path into the Document. An Item can
/// become stale when a mutation to the Document changes the value that the
/// Item points at.  Using a stale Item raises a ``RuntimeError``.
#[pyclass(name = "Item", module = "tomledit", subclass)]
pub(crate) struct ItemProxy {
    pub(crate) document: Py<Document>,
    pub(crate) path: Vec<Key>,
    revision: u64,
}

impl ItemProxy {
    pub(crate) fn new(document: Py<Document>, path: Vec<Key>, revision: u64) -> Self {
        Self {
            document,
            path,
            revision,
        }
    }

    /// Check that no mutation has occurred along this proxy's path since
    /// it was created.
    pub(crate) fn check_fresh(&self, doc: &Document) -> PyResult<()> {
        doc.check_fresh(&self.path, self.revision)
    }

    /// Record a mutation at a child key under this proxy's path.
    /// The proxy itself stays valid (only the child node is bumped).
    pub(crate) fn bump_child(&self, doc: &mut Document, child_key: Key) {
        doc.bump_at_child(&self.path, &child_key);
    }

    /// Record a structural mutation at this proxy's own path (e.g. clear).
    /// The proxy self-updates to stay valid.
    pub(crate) fn bump_self(&mut self, doc: &mut Document) {
        doc.bump_at(&self.path);
        self.revision = doc.revision;
    }

    /// Stamp each index in `from..to` as changed. The proxy (the array
    /// itself) stays valid.
    pub(crate) fn bump_range(&mut self, doc: &mut Document, from: usize, to: usize) {
        doc.bump_range(&self.path, from, to);
        self.revision = doc.revision;
    }

    /// Invalidation dispatch based on the `Affected` descriptor returned
    /// by list mutation helpers.
    pub(crate) fn bump_affected(&mut self, doc: &mut Document, affected: Affected) {
        match affected {
            Affected::Child(k) => self.bump_child(doc, k),
            Affected::Range { from, to } => self.bump_range(doc, from, to),
        }
    }

    /// Clone the toml_edit item at this proxy's path.
    ///
    /// For array elements and inline-table values the inline comment is stored
    /// externally (slot system).  It is embedded into the cloned value's decor
    /// suffix so that it travels with the value when inserted elsewhere.
    pub(crate) fn clone_item(&self, py: Python<'_>) -> PyResult<ItemRs> {
        let doc = self.document.borrow(py);
        self.check_fresh(&doc)?;
        let item = self.navigate(&doc.inner)?;
        let mut cloned = item.clone();
        let slot_comment = match self.path.last() {
            Some(Key::Int(idx)) if self.path.len() >= 2 => {
                let parent = self.navigate_parent(&doc.inner)?;
                comments::get_array_item_comment(parent, *idx)
            }
            Some(Key::Str(key)) if self.path.len() >= 2 => {
                let parent = self.navigate_parent(&doc.inner)?;
                parent
                    .as_inline_table()
                    .and_then(|it| comments::get_it_item_comment(it, key))
            }
            _ => None,
        };
        if let Some(comment) = slot_comment
            && let Some(v) = cloned.as_value_mut()
        {
            v.decor_mut().set_suffix(format!(" {comment}"));
        }
        Ok(cloned)
    }

    pub(crate) fn navigate<'a>(&self, doc: &'a DocumentRs) -> PyResult<&'a ItemRs> {
        item_ops::navigate_path(doc, &self.path)
    }

    pub(crate) fn navigate_mut<'a>(&self, doc: &'a mut DocumentRs) -> PyResult<&'a mut ItemRs> {
        item_ops::navigate_path_mut(doc, &self.path)
    }

    /// Build a child proxy as the correct Python subclass (DictItem,
    /// ListItem, or ScalarItem) by inspecting the TOML node type.
    /// Returns `Py<PyAny>` since the concrete type varies.
    pub(crate) fn child_proxy_typed(&self, py: Python<'_>, key: Key) -> PyResult<Py<PyAny>> {
        let revision = {
            let doc = self.document.bind(py).borrow();
            doc.revision
        };
        Self::make_child_typed(&self.document, &self.path, revision, py, key)
    }

    /// Build a typed child proxy from constituent parts.
    pub(crate) fn make_child_typed(
        document: &Py<Document>,
        path: &[Key],
        revision: u64,
        py: Python<'_>,
        child_key: Key,
    ) -> PyResult<Py<PyAny>> {
        let mut child_path = path.to_vec();
        child_path.push(child_key);
        let base = ItemProxy::new(document.clone_ref(py), child_path, revision);
        Self::into_typed(py, base)
    }

    /// Wrap an already-constructed ItemProxy base into the right Python
    /// subclass.  Used by Document::__getitem__ and friends.
    pub(crate) fn into_typed(py: Python<'_>, base: ItemProxy) -> PyResult<Py<PyAny>> {
        let kind = {
            let doc = base.document.borrow(py);
            let item = base.navigate(&doc.inner)?;
            item_kind(item)
        };
        into_typed_proxy(py, base, kind)
    }

    /// Navigate to the parent item (all path segments except the last).
    fn navigate_parent<'a>(&self, doc: &'a DocumentRs) -> PyResult<&'a ItemRs> {
        item_ops::navigate_path(doc, &self.path[..self.path.len() - 1])
    }

    fn navigate_parent_mut<'a>(&self, doc: &'a mut DocumentRs) -> PyResult<&'a mut ItemRs> {
        item_ops::navigate_path_mut(doc, &self.path[..self.path.len() - 1])
    }
}

// ---------------------------------------------------------------------------
// Subclass dispatch helpers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum ItemKind {
    Dict,
    List,
    Scalar,
}

fn item_kind(item: &ItemRs) -> ItemKind {
    if matches!(
        item,
        ItemRs::Table(_) | ItemRs::Value(ValueRs::InlineTable(_))
    ) {
        ItemKind::Dict
    } else if matches!(
        item,
        ItemRs::Value(ValueRs::Array(_)) | ItemRs::ArrayOfTables(_)
    ) {
        ItemKind::List
    } else {
        ItemKind::Scalar
    }
}

fn into_typed_proxy(py: Python<'_>, base: ItemProxy, kind: ItemKind) -> PyResult<Py<PyAny>> {
    use pyo3::PyClassInitializer;
    match kind {
        ItemKind::Dict => {
            let init = PyClassInitializer::from(base).add_subclass(DictProxy);
            Ok(Py::new(py, init)?.into_any())
        }
        ItemKind::List => {
            let init = PyClassInitializer::from(base).add_subclass(ListProxy);
            Ok(Py::new(py, init)?.into_any())
        }
        ItemKind::Scalar => {
            let init = PyClassInitializer::from(base).add_subclass(ScalarProxy);
            Ok(Py::new(py, init)?.into_any())
        }
    }
}

#[pymethods]
impl ItemProxy {
    // ---- core protocol ----

    pub fn __getitem__(&self, key: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        use item_ops::SubscriptKey;
        let py = key.py();
        let resolved = item_ops::resolve_subscript_key(py, key)?;
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        let item = self.navigate(&doc.inner)?;

        match resolved {
            SubscriptKey::Slice(slice) => {
                let target = list_ops::as_array_like(item, "slicing")?;
                let si = slice.indices(target.len() as isize)?;
                let indices = list_ops::collect_slice_indices(si.start, si.stop, si.step);
                let proxies: PyResult<Vec<Py<PyAny>>> = indices
                    .into_iter()
                    .map(|i| self.child_proxy_typed(py, Key::Int(i)))
                    .collect();
                Ok(proxies?.into_pyobject(py)?.into_any().unbind())
            }
            SubscriptKey::Str(k) => {
                if !dict_ops::item_has_key(item, &k)? {
                    return Err(PyKeyError::new_err(k));
                }
                self.child_proxy_typed(py, Key::Str(k))
            }
            SubscriptKey::Int(i) => {
                let idx = list_ops::require_array_index(item, i)?;
                self.child_proxy_typed(py, Key::Int(idx))
            }
            SubscriptKey::Other(bad_key) => {
                // Match Python dict: unknown key types raise KeyError for
                // tables, TypeError for arrays.
                match item {
                    ItemRs::Table(_) | ItemRs::Value(ValueRs::InlineTable(_)) => {
                        Err(PyKeyError::new_err(bad_key.repr()?.to_string()))
                    }
                    _ => Err(item_ops::bad_key_type(&bad_key)),
                }
            }
        }
    }

    pub fn __setitem__(
        &mut self,
        key: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        use item_ops::SubscriptKey;
        let py = key.py();
        let resolved = item_ops::resolve_subscript_key(py, key)?;

        match resolved {
            SubscriptKey::Slice(slice) => {
                let values: Vec<Item> = value
                    .try_iter()?
                    .map(|r| r.and_then(|v| v.extract::<Item>()))
                    .collect::<PyResult<_>>()?;
                let mut doc = self.document.bind(py).borrow_mut();
                self.check_fresh(&doc)?;
                let item = self.navigate_mut(&mut doc.inner)?;
                let target = list_ops::as_array_like_mut(item, "slice assignment")?;
                let si = slice.indices(target.len() as isize)?;
                let old_len = target.len();
                let indices = list_ops::collect_slice_indices(si.start, si.stop, si.step);
                let new_count = values.len();
                list_ops::item_setitem_slice(target, si.start, si.stop, si.step, values)?;
                if new_count == indices.len() {
                    // Same-length: stamp only the replaced indices.
                    for &i in &indices {
                        self.bump_child(&mut doc, Key::Int(i));
                    }
                } else {
                    // Different length: everything from first affected onward may have shifted.
                    let from = indices.iter().min().copied().unwrap_or(si.start as usize);
                    self.bump_range(&mut doc, from, old_len);
                }
                Ok(())
            }
            SubscriptKey::Str(k) => {
                let value: Item = value.extract()?;
                let mut doc = self.document.bind(py).borrow_mut();
                self.check_fresh(&doc)?;
                let item = self.navigate_mut(&mut doc.inner)?;
                if let Some(replaced_key) = item_ops::item_setitem_str(item, k, value)? {
                    self.bump_child(&mut doc, replaced_key);
                }
                Ok(())
            }
            SubscriptKey::Int(i) => {
                let value: Item = value.extract()?;
                let mut doc = self.document.bind(py).borrow_mut();
                self.check_fresh(&doc)?;
                let item = self.navigate_mut(&mut doc.inner)?;
                let replaced_key = item_ops::item_setitem_int(item, i, value)?;
                self.bump_child(&mut doc, replaced_key);
                Ok(())
            }
            SubscriptKey::Other(bad_key) => Err(item_ops::bad_key_type(&bad_key)),
        }
    }

    pub fn __delitem__(&mut self, key: &Bound<'_, PyAny>) -> PyResult<()> {
        use item_ops::SubscriptKey;
        let py = key.py();
        let resolved = item_ops::resolve_subscript_key(py, key)?;

        match resolved {
            SubscriptKey::Slice(slice) => {
                let mut doc = self.document.bind(py).borrow_mut();
                self.check_fresh(&doc)?;
                let item = self.navigate_mut(&mut doc.inner)?;
                let target = list_ops::as_array_like_mut(item, "slice deletion")?;
                let si = slice.indices(target.len() as isize)?;
                let indices = list_ops::collect_slice_indices(si.start, si.stop, si.step);
                if let Some(&min_idx) = indices.iter().min() {
                    let old_len = target.len();
                    list_ops::item_delitem_slice(target, &indices)?;
                    self.bump_range(&mut doc, min_idx, old_len);
                }
                Ok(())
            }
            SubscriptKey::Str(k) => {
                let mut doc = self.document.bind(py).borrow_mut();
                self.check_fresh(&doc)?;
                let item = self.navigate_mut(&mut doc.inner)?;
                let deleted = item_ops::item_delitem_str(item, &k)?;
                self.bump_affected(&mut doc, deleted);
                Ok(())
            }
            SubscriptKey::Int(i) => {
                let mut doc = self.document.bind(py).borrow_mut();
                self.check_fresh(&doc)?;
                let item = self.navigate_mut(&mut doc.inner)?;
                let deleted = item_ops::item_delitem_int(item, i)?;
                self.bump_affected(&mut doc, deleted);
                Ok(())
            }
            SubscriptKey::Other(bad_key) => Err(item_ops::bad_key_type(&bad_key)),
        }
    }

    pub fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_len(item).ok_or_else(|| {
            PyTypeError::new_err(format!(
                "TOML {} item has no len() (use .value to get the Python object)",
                item.type_name()
            ))
        })
    }

    pub fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        let item = self.navigate(&doc.inner)?;

        match item_ops::item_iter_kind(item)? {
            item_ops::IterKind::TableKeys(keys) => {
                let list = keys.into_pyobject(py)?;
                Ok(list.try_iter()?.unbind())
            }
            item_ops::IterKind::ArrayLen(len) => {
                let proxies: PyResult<Vec<Py<PyAny>>> = (0..len)
                    .map(|i| self.child_proxy_typed(py, Key::Int(i)))
                    .collect();
                let list = proxies?.into_pyobject(py)?;
                Ok(list.try_iter()?.unbind())
            }
        }
    }

    pub fn __contains__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = value.py();
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_contains(item, value)
    }

    pub fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        let item = self.navigate(&doc.inner)?;
        Ok(item_ops::item_bool(item))
    }

    pub fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_str(item, py)
    }

    pub fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        let item = self.navigate(&doc.inner)?;
        Ok(item_ops::item_repr(item))
    }

    /// Return the TOML representation of this item.
    pub fn as_toml(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        let item = self.navigate(&doc.inner)?;
        Ok(item.to_string().trim().to_owned())
    }

    pub fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = other.py();
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        let item = self.navigate(&doc.inner)?;
        equality::item_eq(item, other)
    }

    /// The underlying data as a native Python object (int, str, list, dict, etc).
    #[getter]
    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_to_py(item, py)
    }

    // ---- comment access ----

    /// The comment lines before this entry, or None.
    #[getter]
    pub fn comment(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let Some(last_key) = self.path.last() else {
            return Ok(None);
        };
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        match last_key {
            Key::Str(key_str) => {
                let parent = self.navigate_parent(&doc.inner)?;
                Ok(comments::get_key_prefix_comment(parent, key_str))
            }
            Key::Int(_) => {
                let item = self.navigate(&doc.inner)?;
                Ok(comments::get_value_prefix_comment(item))
            }
        }
    }

    /// Set or clear the block comment above this entry.
    ///
    /// Each non-empty line must start with ``#``.  Pass ``None`` to remove
    /// the comment.  Empty lines in the string produce blank lines above
    /// the entry.
    #[setter]
    pub fn set_comment(&self, py: Python<'_>, value: Option<&str>) -> PyResult<()> {
        let Some(last_key) = self.path.last() else {
            return Err(PyTypeError::new_err("cannot set comment on root"));
        };
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_fresh(&doc)?;
        match last_key {
            Key::Str(key_str) => {
                let parent = self.navigate_parent_mut(&mut doc.inner)?;
                comments::set_key_prefix_comment(parent, key_str, value)?;
            }
            Key::Int(_) => {
                let item = self.navigate_mut(&mut doc.inner)?;
                comments::set_value_prefix_comment(item, value)?;
            }
        }
        Ok(())
    }

    /// The inline comment after this value (e.g. `key = 1 # this part`), or None.
    #[getter]
    pub fn inline_comment(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let doc = self.document.bind(py).borrow();
        self.check_fresh(&doc)?;
        if let Some(Key::Int(idx)) = self.path.last() {
            let parent = self.navigate_parent(&doc.inner)?;
            return Ok(comments::get_array_item_comment(parent, *idx));
        }
        // Inline-table values: comment lives in next key's prefix / trailing.
        if let Some(Key::Str(key)) = self.path.last()
            && self.path.len() >= 2
        {
            let parent = self.navigate_parent(&doc.inner)?;
            if let Some(it) = parent.as_inline_table() {
                return Ok(comments::get_it_item_comment(it, key));
            }
        }
        let item = self.navigate(&doc.inner)?;
        Ok(comments::get_suffix_comment(item))
    }

    /// Set or clear the inline comment on this entry.
    ///
    /// The value must start with ``#`` (e.g. ``"# my note"``).
    /// Pass ``None`` to remove the comment.
    #[setter]
    pub fn set_inline_comment(&self, py: Python<'_>, value: Option<&str>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_fresh(&doc)?;
        // Slot-based paths (arrays, inline tables) need pre-validated raw format.
        let raw = match value {
            Some(text) => comments::validate_inline_comment(text)?,
            None => String::new(),
        };
        if let Some(Key::Int(idx)) = self.path.last() {
            let parent = self.navigate_parent_mut(&mut doc.inner)?;
            let array = parent
                .as_value_mut()
                .and_then(|v| v.as_array_mut())
                .ok_or_else(|| PyTypeError::new_err("parent is not an array"))?;
            comments::set_array_item_comment(array, *idx, &raw);
            return Ok(());
        }
        if let Some(Key::Str(key)) = self.path.last()
            && self.path.len() >= 2
        {
            let parent = self.navigate_parent_mut(&mut doc.inner)?;
            if let Some(it) = parent.as_value_mut().and_then(|v| v.as_inline_table_mut()) {
                comments::set_it_item_comment(it, key, &raw);
                return Ok(());
            }
        }
        let item = self.navigate_mut(&mut doc.inner)?;
        comments::set_suffix_comment(item, value)?;
        Ok(())
    }

    // ---- shared methods ----

    pub fn clear(&mut self, py: Python<'_>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_fresh(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_ops::item_clear(item)?;
        self.bump_self(&mut doc);
        Ok(())
    }

    /// Normalize formatting of this item (spacing, trailing commas, etc.).
    ///
    /// Useful after mutations that leave behind awkward whitespace.
    /// This is shallow - it formats the item itself, not nested sub-tables.
    /// Note: any comments on the formatted item will be removed.
    pub fn fmt(&self, py: Python<'_>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_fresh(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_ops::item_fmt(item);
        Ok(())
    }

    /// Parse a TOML value fragment, preserving its representation.
    ///
    /// Use this when you need a specific TOML representation that can't be
    /// expressed through plain Python types, e.g. hex integers or literal strings:
    ///
    ///     doc["mask"] = Item.parse("0xFF")
    ///     doc["msg"]  = Item.parse("'''multi\nline'''")
    #[staticmethod]
    pub(crate) fn parse(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        let value: ValueRs = text
            .parse()
            .map_err(|e: toml_edit::TomlError| PyValueError::new_err(e.to_string()))?;
        let mut doc_rs = DocumentRs::new();
        doc_rs["_"] = ItemRs::Value(value);
        let doc = Py::new(py, Document::from_inner(doc_rs))?;
        let base = Self {
            document: doc,
            path: vec![Key::Str("_".to_owned())],
            revision: 0,
        };
        Self::into_typed(py, base)
    }
}
