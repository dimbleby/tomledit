use pyo3::exceptions::{PyAttributeError, PyKeyError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PySlice, PyTuple};
use toml_edit::DocumentMut as DocumentRs;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::document::Document;
use crate::equality;
use crate::item::Item;
use crate::item_ops::{self, Key};
use crate::views::{ItemsView, KeysView, ValuesView};

/// If `value` is an [`ItemProxy`] (or subclass such as `ScalarItem`), resolve
/// it to its underlying Python value so that subsequent operations can compare
/// plain Python objects without re-borrowing the document through dunder
/// methods. Returns `None` when `value` is not a proxy.
fn resolve_proxy(_py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Option<Py<PyAny>>> {
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
/// An Item becomes stale when the part of the Document it points to (or an
/// ancestor) is modified through a different reference; using a stale Item
/// raises ``RuntimeError``.  Mutations to unrelated subtrees do **not**
/// invalidate this Item.
#[pyclass(name = "Item", module = "tomledit", subclass)]
pub(crate) struct ItemProxy {
    document: Py<Document>,
    path: Vec<Key>,
    generation: u64,
}

/// A TOML table or inline table.
///
/// ``isinstance(item, DictItem)`` and
/// ``isinstance(item, MutableMapping)`` both work.
#[pyclass(name = "DictItem", module = "tomledit", extends = ItemProxy)]
pub(crate) struct DictProxy;

/// A TOML array or array of tables.
///
/// ``isinstance(item, ListItem)`` and
/// ``isinstance(item, MutableSequence)`` both work.
#[pyclass(name = "ListItem", module = "tomledit", extends = ItemProxy)]
pub(crate) struct ListProxy;

/// A scalar TOML value (string, integer, float, boolean, datetime, date, or time).
#[pyclass(name = "ScalarItem", module = "tomledit", extends = ItemProxy)]
pub(crate) struct ScalarProxy;

impl ItemProxy {
    pub(crate) fn new(document: Py<Document>, path: Vec<Key>, generation: u64) -> Self {
        Self {
            document,
            path,
            generation,
        }
    }

    /// Check that no mutation has occurred along this proxy's path since
    /// it was created.
    fn check_generation(&self, doc: &Document) -> PyResult<()> {
        if !doc.trie.is_valid(&self.path, self.generation) {
            Err(PyRuntimeError::new_err(
                "this Item is stale: the document has been modified since it was created",
            ))
        } else {
            Ok(())
        }
    }

    /// Record a mutation at a child key under this proxy's path.
    /// The proxy itself stays valid (only the child node is bumped).
    fn bump_child(&self, doc: &mut Document, child_key: Key) {
        let mut child_path = self.path.clone();
        child_path.push(child_key);
        doc.trie.bump_at(&child_path);
    }

    /// Record a structural mutation at this proxy's own path (e.g. clear,
    /// array insert/remove). The proxy self-updates to stay valid.
    fn bump_self(&mut self, doc: &mut Document) {
        doc.trie.bump_at(&self.path);
        self.generation = doc.trie.clock;
    }

    /// Clone the toml_edit item at this proxy's path.
    ///
    /// For array elements and inline-table values the inline comment is stored
    /// externally (slot system).  It is embedded into the cloned value's decor
    /// suffix so that it travels with the value when inserted elsewhere.
    pub(crate) fn clone_item(&self, py: Python<'_>) -> PyResult<ItemRs> {
        let doc = self.document.borrow(py);
        self.check_generation(&doc)?;
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

    fn navigate<'a>(&self, doc: &'a DocumentRs) -> PyResult<&'a ItemRs> {
        item_ops::navigate_path(doc, &self.path)
    }

    fn navigate_mut<'a>(&self, doc: &'a mut DocumentRs) -> PyResult<&'a mut ItemRs> {
        item_ops::navigate_path_mut(doc, &self.path)
    }

    /// Build a child proxy as the correct Python subclass (DictItem,
    /// ListItem, or ScalarItem) by inspecting the TOML node type.
    /// Returns `Py<PyAny>` since the concrete type varies.
    fn child_proxy_typed(&self, py: Python<'_>, key: Key) -> PyResult<Py<PyAny>> {
        let generation = {
            let doc = self.document.bind(py).borrow();
            doc.trie.clock
        };
        Self::make_child_typed(&self.document, &self.path, generation, py, key)
    }

    /// Build a typed child proxy from constituent parts.
    pub(crate) fn make_child_typed(
        document: &Py<Document>,
        path: &[Key],
        generation: u64,
        py: Python<'_>,
        child_key: Key,
    ) -> PyResult<Py<PyAny>> {
        let mut child_path = path.to_vec();
        child_path.push(child_key);
        let base = ItemProxy::new(document.clone_ref(py), child_path, generation);
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
        let py = key.py();

        // Slice support: return a list of child proxies.
        if let Ok(slice) = key.cast::<PySlice>() {
            let doc = self.document.bind(py).borrow();
            self.check_generation(&doc)?;
            let item = self.navigate(&doc.inner)?;
            let len = item_ops::require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            let indices = item_ops::collect_slice_indices(si.start, si.stop, si.step);
            let proxies: PyResult<Vec<Py<PyAny>>> = indices
                .into_iter()
                .map(|i| self.child_proxy_typed(py, Key::Int(i)))
                .collect();
            return Ok(proxies?.into_pyobject(py)?.into_any().unbind());
        }

        let new_key = {
            let doc = self.document.bind(py).borrow();
            self.check_generation(&doc)?;
            let item = self.navigate(&doc.inner)?;
            item_ops::item_getitem(item, key)?
        };

        self.child_proxy_typed(py, new_key)
    }

    pub fn __setitem__(
        &mut self,
        key: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let py = key.py();

        if let Ok(slice) = key.cast::<PySlice>() {
            let values: Vec<Item> = value
                .try_iter()?
                .map(|r| r.and_then(|v| v.extract::<Item>()))
                .collect::<PyResult<_>>()?;

            let mut doc = self.document.bind(py).borrow_mut();
            self.check_generation(&doc)?;
            let item = self.navigate_mut(&mut doc.inner)?;
            let len = item_ops::require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            item_ops::item_setitem_slice(item, si.start, si.stop, si.step, values)?;
            self.bump_self(&mut doc);
            return Ok(());
        }

        let value: Item = value.extract()?;
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        if let Some(replaced_key) = item_ops::item_setitem(item, key, value)? {
            self.bump_child(&mut doc, replaced_key);
        }
        Ok(())
    }

    pub fn __delitem__(&mut self, key: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = key.py();

        if let Ok(slice) = key.cast::<PySlice>() {
            let mut doc = self.document.bind(py).borrow_mut();
            self.check_generation(&doc)?;
            let item = self.navigate_mut(&mut doc.inner)?;
            let len = item_ops::require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            let indices = item_ops::collect_slice_indices(si.start, si.stop, si.step);
            item_ops::item_delitem_slice(item, &indices)?;
            self.bump_self(&mut doc);
            return Ok(());
        }

        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        let deleted_key = item_ops::item_delitem(item, key)?;
        match deleted_key {
            Key::Str(_) => self.bump_child(&mut doc, deleted_key),
            Key::Int(_) => self.bump_self(&mut doc),
        }
        Ok(())
    }

    pub fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
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
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;

        match item_ops::item_iter_info(item)? {
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
        // Resolve ItemProxy to its Python value so equality comparison doesn't
        // re-borrow the document through dunder methods.
        let resolved = resolve_proxy(py, value)?;
        let value = resolved.as_ref().map_or(value, |v| v.bind(py));
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_contains(item, value)
    }

    pub fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        Ok(item_ops::item_bool(item))
    }

    pub fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_str(item, py)
    }

    pub fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        Ok(item_ops::item_repr(item))
    }

    /// Return the TOML representation of this item.
    pub fn as_toml(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        Ok(item.to_string().trim().to_owned())
    }

    pub fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = other.py();

        // Proxy-vs-proxy: compare underlying items directly in Rust.
        if let Ok(other_proxy) = other.cast::<Self>() {
            let other_proxy = other_proxy.borrow();
            let doc = self.document.bind(py).borrow();
            self.check_generation(&doc)?;
            let self_item = self.navigate(&doc.inner)?;
            let other_doc = other_proxy.document.bind(py).borrow();
            other_proxy.check_generation(&other_doc)?;
            let other_item = other_proxy.navigate(&other_doc.inner)?;
            return Ok(equality::items_structural_eq(self_item, other_item));
        }

        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        equality::item_eq(item, other)
    }

    /// The underlying data as a native Python object (int, str, list, dict, etc).
    #[getter]
    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
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
        self.check_generation(&doc)?;
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
        self.check_generation(&doc)?;
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
        self.check_generation(&doc)?;
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
        self.check_generation(&doc)?;
        // Slot-based paths (arrays, inline tables) need pre-validated raw format.
        let raw = || -> PyResult<String> {
            Ok(match value {
                Some(text) => comments::validate_inline_comment(text)?,
                None => String::new(),
            })
        };
        if let Some(Key::Int(idx)) = self.path.last() {
            let raw = raw()?;
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
                comments::set_it_item_comment(it, key, &raw()?);
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
        self.check_generation(&doc)?;
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
        self.check_generation(&doc)?;
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
    fn parse(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        let value: ValueRs = text
            .parse()
            .map_err(|e: toml_edit::TomlError| PyValueError::new_err(e.to_string()))?;
        let mut doc_rs = DocumentRs::new();
        doc_rs["_"] = ItemRs::Value(value);
        let doc = Py::new(
            py,
            Document {
                inner: doc_rs,
                trie: crate::trie::MutationTrie::new(),
            },
        )?;
        let base = Self {
            document: doc,
            path: vec![Key::Str("_".to_owned())],
            generation: 0,
        };
        Self::into_typed(py, base)
    }
}

// ---------------------------------------------------------------------------
// DictProxy (DictItem) — dict-like methods
// ---------------------------------------------------------------------------

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
        base.check_generation(&doc)?;
        Ok(KeysView::new(
            base.document.clone_ref(py),
            base.path.clone(),
        ))
    }

    pub fn values(self_: PyRef<'_, Self>, py: Python<'_>) -> PyResult<ValuesView> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_generation(&doc)?;
        Ok(ValuesView::new(
            base.document.clone_ref(py),
            base.path.clone(),
        ))
    }

    pub fn items(self_: PyRef<'_, Self>, py: Python<'_>) -> PyResult<ItemsView> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_generation(&doc)?;
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
        base.check_generation(&doc)?;
        let item = base.navigate(&doc.inner)?;
        if item_ops::item_has_key(item, key)? {
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

        let base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_generation(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;

        match item_ops::item_pop(item, Some(key)) {
            Ok(removed) => {
                let result = item_ops::item_to_py(&removed.0, py)?;
                let popped_key: String = key.extract()?;
                base.bump_child(&mut doc, Key::Str(popped_key));
                Ok(result)
            }
            Err(e) if default_val.is_some() && e.is_instance_of::<PyKeyError>(py) => {
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
        let base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_generation(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let replaced_keys = item_ops::apply_update_pairs(item, pairs)?;
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
            base.check_generation(&doc)?;
            let item = base.navigate_mut(&mut doc.inner)?;

            if !item_ops::item_has_key(item, key)? {
                let default = default.ok_or_else(|| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "setdefault() requires a default value: TOML has no null type",
                    )
                })?;
                item_ops::set_with_decor_preservation(item, key, default);
            }
        }
        base.child_proxy_typed(py, Key::Str(key.to_owned()))
    }
}

// ---------------------------------------------------------------------------
// ListProxy (ListItem) — list-like methods
// ---------------------------------------------------------------------------

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
        base.check_generation(&doc)?;
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
        base.check_generation(&doc)?;
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
        base.check_generation(&doc)?;
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
        base.check_generation(&doc)?;
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
        base.check_generation(&doc)?;
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
        base.check_generation(&doc)?;
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
        base.check_generation(&doc)?;
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
        base.check_generation(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        item_ops::item_set_multiline(item, indent)
    }
}

// ---------------------------------------------------------------------------
// ScalarProxy (ScalarItem) — forward operations to the Python value
// ---------------------------------------------------------------------------

/// Invoke a binary operator from Python's `operator` module (e.g. "add", "sub").
fn py_binop(
    py: Python<'_>,
    op: &str,
    lhs: &Bound<'_, PyAny>,
    rhs: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    py.import("operator")?
        .getattr(op)?
        .call1((lhs, rhs))
        .map(Bound::unbind)
}

/// Invoke a unary operator from Python's `operator` module (e.g. "neg", "pos").
fn py_unop(py: Python<'_>, op: &str, operand: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    py.import("operator")?
        .getattr(op)?
        .call1((operand,))
        .map(Bound::unbind)
}

impl ScalarProxy {
    /// Resolve the underlying Python value from the TOML document.
    fn resolve(self_: &PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        self_.as_super().value(self_.py())
    }
}

#[pymethods]
impl ScalarProxy {
    #[staticmethod]
    fn parse(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        let result = ItemProxy::parse(py, text)?;
        if result.bind(py).is_instance_of::<ScalarProxy>() {
            Ok(result)
        } else {
            Err(PyValueError::new_err(format!(
                "ScalarItem.parse() requires a scalar value, got {}",
                result.bind(py).get_type().qualname()?,
            )))
        }
    }

    // ---- attribute forwarding ----

    /// Forward attribute access to the underlying Python value.
    ///
    /// This makes scalar items feel like their native Python types:
    /// a string item supports `.upper()`, `.startswith()`, etc.; an int item
    /// supports `.bit_length()`; a datetime supports `.isoformat()`.
    ///
    /// Only triggered as a fallback — Item-level attributes like `.value`,
    /// `.comment`, and `.inline_comment` are resolved through normal lookup
    /// first and are never forwarded.
    fn __getattr__(self_: PyRef<'_, Self>, name: &str) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let py_value = Self::resolve(&self_)?;
        let bound = py_value.bind(py);
        bound.getattr(name).map(|a| a.unbind()).map_err(|_| {
            let type_name = bound
                .get_type()
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "unknown".to_owned());
            PyAttributeError::new_err(format!(
                "'ScalarItem' wrapping {type_name} has no attribute '{name}'"
            ))
        })
    }

    // ---- containment ----

    fn __contains__(self_: PyRef<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = self_.py();
        let resolved = Self::resolve(&self_)?;
        py_binop(py, "contains", resolved.bind(py), value)?.extract::<bool>(py)
    }

    // ---- comparison ----

    fn __eq__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        self_.as_super().__eq__(other)
    }

    fn __lt__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Self::resolve(&self_)?.bind(self_.py()).lt(other)
    }

    fn __le__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Self::resolve(&self_)?.bind(self_.py()).le(other)
    }

    fn __gt__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Self::resolve(&self_)?.bind(self_.py()).gt(other)
    }

    fn __ge__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Self::resolve(&self_)?.bind(self_.py()).ge(other)
    }

    // ---- type conversion ----

    fn __int__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py.import("builtins")?
            .getattr("int")?
            .call1((val.bind(py),))
            .map(Bound::unbind)
    }

    fn __float__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py.import("builtins")?
            .getattr("float")?
            .call1((val.bind(py),))
            .map(Bound::unbind)
    }

    fn __index__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_unop(py, "index", val.bind(py))
    }

    fn __hash__(self_: PyRef<'_, Self>) -> PyResult<isize> {
        let py = self_.py();
        Self::resolve(&self_)?.bind(py).hash()
    }

    // ---- binary arithmetic ----

    fn __add__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "add", val.bind(py), other)
    }

    fn __radd__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "add", other, val.bind(py))
    }

    fn __sub__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "sub", val.bind(py), other)
    }

    fn __rsub__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "sub", other, val.bind(py))
    }

    fn __mul__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "mul", val.bind(py), other)
    }

    fn __rmul__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "mul", other, val.bind(py))
    }

    fn __truediv__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "truediv", val.bind(py), other)
    }

    fn __rtruediv__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "truediv", other, val.bind(py))
    }

    fn __floordiv__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "floordiv", val.bind(py), other)
    }

    fn __rfloordiv__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "floordiv", other, val.bind(py))
    }

    fn __mod__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "mod", val.bind(py), other)
    }

    fn __rmod__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "mod", other, val.bind(py))
    }

    fn __pow__(
        self_: PyRef<'_, Self>,
        exp: &Bound<'_, PyAny>,
        modulo: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        let pow_fn = py.import("builtins")?.getattr("pow")?;
        match modulo {
            Some(m) => pow_fn.call1((val.bind(py), exp, m)),
            None => pow_fn.call1((val.bind(py), exp)),
        }
        .map(Bound::unbind)
    }

    fn __rpow__(
        self_: PyRef<'_, Self>,
        base: &Bound<'_, PyAny>,
        modulo: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        let pow_fn = py.import("builtins")?.getattr("pow")?;
        match modulo {
            Some(m) => pow_fn.call1((base, val.bind(py), m)),
            None => pow_fn.call1((base, val.bind(py))),
        }
        .map(Bound::unbind)
    }

    // ---- unary operators ----

    fn __neg__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_unop(py, "neg", val.bind(py))
    }

    fn __pos__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_unop(py, "pos", val.bind(py))
    }

    fn __abs__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_unop(py, "abs", val.bind(py))
    }

    fn __invert__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_unop(py, "invert", val.bind(py))
    }

    // ---- formatting ----

    fn __format__(self_: PyRef<'_, Self>, spec: &str) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        val.bind(py)
            .call_method1("__format__", (spec,))
            .map(|a| a.unbind())
    }
}
