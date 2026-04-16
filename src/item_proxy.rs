use std::sync::atomic::{AtomicU64, Ordering};

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use toml_edit::DocumentMut as DocumentRs;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::dict_proxy::DictProxy;
use crate::document::Document;
use crate::equality;
use crate::item::Item;
use crate::item_ops::{self, Affected, Key};
use crate::list_proxy::ListProxy;
use crate::scalar_proxy::ScalarProxy;

/// If `value` is an [`ItemProxy`] (or subclass such as `ScalarItem`), resolve
/// it to its underlying Python value so that subsequent operations can compare
/// plain Python objects without locking the document through dunder methods.
/// Returns `None` when `value` is not a proxy.
pub(crate) fn resolve_proxy(value: &Bound<'_, PyAny>) -> PyResult<Option<Py<PyAny>>> {
    if let Ok(proxy) = value.cast::<ItemProxy>() {
        Ok(Some(proxy.get().value(value.py())?))
    } else {
        Ok(None)
    }
}

/// If `other` is an [`ItemProxy`], resolve it to the underlying `toml_edit::Item`
/// and call `f` with a reference to it.  Returns `None` when `other` is not a
/// proxy.  This avoids repeating the cast → get → check_fresh → read → navigate
/// sequence at every call site.
pub(crate) fn with_proxy_item<R>(
    other: &Bound<'_, PyAny>,
    f: impl FnOnce(&toml_edit::Item) -> R,
) -> PyResult<Option<R>> {
    resolve_proxy_item(other, None, f)
}

/// Read context that carries an existing `inner` read guard reference.
///
/// When a method holds `doc.inner.read()` (or `.write()`) and needs to compare
/// against a value that may be a proxy or `Document` from the same document,
/// this context allows [`resolve_other_item`] to reuse the guard instead of
/// acquiring a nested read lock (which would deadlock under a write guard).
pub(crate) struct ReadCtx<'a> {
    doc: &'a Document,
    inner: &'a DocumentRs,
}

impl<'a> ReadCtx<'a> {
    pub(crate) fn new(doc: &'a Document, inner: &'a DocumentRs) -> Self {
        Self { doc, inner }
    }
}

/// Resolve `other` as either an [`ItemProxy`] or a [`Document`], calling `f`
/// with the underlying `toml_edit::Item`.  Returns `None` when `other` is
/// neither.
///
/// Same-document guards are reused via `ctx` to avoid nested locking.
pub(crate) fn resolve_other_item<R>(
    other: &Bound<'_, PyAny>,
    ctx: &ReadCtx<'_>,
    f: impl Fn(&toml_edit::Item) -> R,
) -> PyResult<Option<R>> {
    if let Some(r) = resolve_proxy_item(other, Some(ctx), &f)? {
        return Ok(Some(r));
    }
    resolve_doc_item(other, ctx, f)
}

/// Resolve a [`Document`] to its root item, reusing the guard from `ctx`
/// when it is the same document.
fn resolve_doc_item<R>(
    other: &Bound<'_, PyAny>,
    ctx: &ReadCtx<'_>,
    f: impl FnOnce(&toml_edit::Item) -> R,
) -> PyResult<Option<R>> {
    let Ok(doc_bound) = other.cast::<Document>() else {
        return Ok(None);
    };
    let doc = doc_bound.get();
    if std::ptr::eq(ctx.doc, doc) {
        Ok(Some(f(ctx.inner.as_item())))
    } else {
        let inner = doc.inner.read();
        Ok(Some(f(inner.as_item())))
    }
}

/// Shared implementation for [`with_proxy_item`] and [`resolve_other_item`].
///
/// When `ctx` is `Some` and the proxy targets the same document, the existing
/// guard is reused.  Otherwise a fresh read lock is acquired.
fn resolve_proxy_item<R>(
    other: &Bound<'_, PyAny>,
    ctx: Option<&ReadCtx<'_>>,
    f: impl FnOnce(&toml_edit::Item) -> R,
) -> PyResult<Option<R>> {
    let Ok(proxy) = other.cast::<ItemProxy>() else {
        return Ok(None);
    };
    let proxy = proxy.get();
    let doc = proxy.document.bind(other.py()).get();
    if let Some(ctx) = ctx.filter(|c| std::ptr::eq(c.doc, doc)) {
        // Reuse the caller's guard: any invalidating mutation was stamped
        // into the trie before the guard was taken, so `check_fresh` is
        // coherent.  Also avoids a nested read that would deadlock if the
        // caller holds a write guard.
        proxy.check_fresh(doc)?;
        let item = proxy.navigate(ctx.inner)?;
        return Ok(Some(f(item)));
    }
    let (_doc, inner) = proxy.read_checked(other.py())?;
    let item = proxy.navigate(&inner)?;
    Ok(Some(f(item)))
}

/// Try to extract a Python object as a `toml_edit::Item` for equality
/// comparison.
///
/// Returns `None` for objects that are not representable as TOML values
/// (the caller should treat this as "not equal").
///
/// Only `TypeError` is caught — other exceptions from the extraction
/// (e.g. a mapping whose `items()` raises) propagate to the caller.
fn extract_for_eq(other: &Bound<'_, PyAny>) -> PyResult<Option<Item>> {
    match other.extract::<Item>() {
        Ok(item) => Ok(Some(item)),
        Err(e) if e.is_instance_of::<PyTypeError>(other.py()) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Resolve a Python value to a `toml_edit::Item` and call `f` with it.
///
/// Fast path: if `value` is a proxy or `Document`, `f` runs under the
/// existing read lock — zero cloning.  Slow path: the value is extracted
/// to an owned `Item` (no lock held), then `f` runs under a fresh lock.
///
/// The `proxy` freshness check is performed **under** each read lock this
/// function takes, closing the TOCTOU window between check and read.
///
/// Returns `None` when the value is not representable as a TOML item
/// (the caller should treat this as "not found" / "not equal").
pub(crate) fn with_resolved_item<R>(
    value: &Bound<'_, PyAny>,
    doc: &Document,
    check_fresh: impl Fn(&Document) -> PyResult<()>,
    f: impl Fn(&DocumentRs, &ItemRs) -> PyResult<R>,
) -> PyResult<Option<R>> {
    {
        let inner = doc.inner.read();
        check_fresh(doc)?;
        let ctx = ReadCtx::new(doc, &inner);
        if let Some(result) = resolve_other_item(value, &ctx, |item| f(&inner, item))? {
            return result.map(Some);
        }
    }
    let Some(extracted) = extract_for_eq(value)? else {
        return Ok(None);
    };
    let inner = doc.inner.read();
    check_fresh(doc)?;
    f(&inner, &extracted.0).map(Some)
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
#[pyclass(frozen, name = "Item", module = "tomledit", subclass)]
pub(crate) struct ItemProxy {
    pub(crate) document: Py<Document>,
    pub(crate) path: Vec<Key>,
    revision: AtomicU64,
}

impl ItemProxy {
    pub(crate) fn new(document: Py<Document>, path: Vec<Key>, revision: u64) -> Self {
        Self {
            document,
            path,
            revision: AtomicU64::new(revision),
        }
    }

    /// Check that no mutation has occurred along this proxy's path since
    /// it was created.
    pub(crate) fn check_fresh(&self, doc: &Document) -> PyResult<()> {
        doc.check_fresh(&self.path, self.revision.load(Ordering::Relaxed))
    }

    /// Bind the owning document.  Prefer [`read_checked`] / [`write_checked`]
    /// when a lock is about to be taken — they perform the freshness check
    /// **after** acquiring the lock, eliminating a TOCTOU window between
    /// check and read.
    ///
    /// [`read_checked`]: Self::read_checked
    /// [`write_checked`]: Self::write_checked
    pub(crate) fn doc<'py>(&'py self, py: Python<'py>) -> &'py Document {
        self.document.bind(py).get()
    }

    /// Acquire `inner.read()` on the owning document, then verify this
    /// proxy is still fresh.  Returns both for use under the guard.
    pub(crate) fn read_checked<'py>(
        &'py self,
        py: Python<'py>,
    ) -> PyResult<(&'py Document, parking_lot::RwLockReadGuard<'py, DocumentRs>)> {
        let doc = self.doc(py);
        let guard = doc.read_checked(&self.path, self.revision.load(Ordering::Relaxed))?;
        Ok((doc, guard))
    }

    /// Acquire `inner.write()` on the owning document, then verify this
    /// proxy is still fresh.
    pub(crate) fn write_checked<'py>(
        &'py self,
        py: Python<'py>,
    ) -> PyResult<(
        &'py Document,
        parking_lot::RwLockWriteGuard<'py, DocumentRs>,
    )> {
        let doc = self.doc(py);
        let guard = doc.write_checked(&self.path, self.revision.load(Ordering::Relaxed))?;
        Ok((doc, guard))
    }

    /// Record a mutation at a child key under this proxy's path.
    /// The proxy itself stays valid (only the child node is bumped).
    pub(crate) fn bump_child(&self, doc: &Document, child_key: Key) {
        doc.bump_at_child(&self.path, &child_key);
    }

    /// Record a structural mutation at this proxy's own path (e.g. clear).
    /// The proxy self-updates to stay valid.
    pub(crate) fn bump_self(&self, doc: &Document) {
        let rev = doc.bump_at(&self.path);
        self.revision.store(rev, Ordering::Relaxed);
    }

    /// Stamp each index in `from..to` as changed. The proxy (the array
    /// itself) stays valid.
    pub(crate) fn bump_range(&self, doc: &Document, from: usize, to: usize) {
        let rev = doc.bump_range(&self.path, from, to);
        self.revision.store(rev, Ordering::Relaxed);
    }

    /// Invalidation dispatch based on the `Affected` descriptor returned
    /// by list mutation helpers.
    pub(crate) fn bump_affected(&self, doc: &Document, affected: Affected) {
        match affected {
            Affected::Child(k) => self.bump_child(doc, k),
            Affected::Range { from, to } => self.bump_range(doc, from, to),
        }
    }

    /// Clone the toml_edit item at this proxy's path.
    ///
    /// For array elements and inline-table entries the inline comment is stored
    /// externally (in the next element's prefix).  It is embedded into the
    /// cloned value's decor suffix so that it travels with the value.
    pub(crate) fn clone_item(&self, py: Python<'_>) -> PyResult<ItemRs> {
        let (_doc, inner) = self.read_checked(py)?;
        let item = self.navigate(&inner)?;
        let mut cloned = item.clone();
        if let Some(comment) = self.element_inline_comment(&inner)?
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
    ///
    /// Takes `inner.read()` once and performs the parent freshness check,
    /// the revision sample and the kind lookup under that single guard —
    /// so child-proxy minting is atomic w.r.t. mutations.
    pub(crate) fn child_proxy_typed(&self, py: Python<'_>, key: Key) -> PyResult<Py<PyAny>> {
        let mut child_path = self.path.clone();
        child_path.push(key);
        let (revision, kind) = {
            let (doc, inner) = self.read_checked(py)?;
            let kind = item_kind(item_ops::navigate_path(&inner, &child_path)?);
            (doc.revision(), kind)
        };
        let base = ItemProxy::new(self.document.clone_ref(py), child_path, revision);
        into_typed_proxy(py, base, kind)
    }

    /// Build a typed child proxy from constituent parts, without a freshness
    /// check on the parent (caller must have verified the context is safe).
    /// Used by views that have already taken `inner.read()` and sampled a
    /// coherent revision.
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
            let doc = base.document.bind(py).get();
            let inner = doc.inner.read();
            let item = base.navigate(&inner)?;
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

    /// Get the inline comment for an element whose comment lives externally
    /// (array elements store it in the next element's prefix; inline-table
    /// entries store it in the next key's prefix or the trailing string).
    ///
    /// Returns `None` if this is not an element path or the parent is not
    /// an array/inline-table.
    fn element_inline_comment(&self, doc: &DocumentRs) -> PyResult<Option<String>> {
        let last = match self.path.last() {
            Some(k) if self.path.len() >= 2 => k,
            _ => return Ok(None),
        };
        let parent = self.navigate_parent(doc)?;
        match last {
            Key::Int(idx) if parent.is_value() => {
                Ok(comments::get_array_inline_comment(parent, *idx))
            }
            Key::Str(key) => Ok(parent
                .as_inline_table()
                .and_then(|it| comments::get_inline_table_inline_comment(it, key))),
            _ => Ok(None),
        }
    }

    /// Resolve the block-comment location for this proxy.
    ///
    /// For string-keyed paths and first-AoT-entry paths, the comment lives
    /// on a key's decor in an ancestor item — returns `Some((ancestor_path,
    /// key))`.  For plain array elements the comment lives in the element's
    /// own decor — returns `None`.
    fn block_comment_target<'a>(
        &'a self,
        doc: &DocumentRs,
    ) -> PyResult<Option<(&'a [Key], &'a str)>> {
        match self.path.last() {
            Some(Key::Str(key)) => Ok(Some((&self.path[..self.path.len() - 1], key))),
            Some(Key::Int(0)) if self.path.len() >= 2 => {
                if self.navigate_parent(doc)?.is_array_of_tables()
                    && let Some(Key::Str(aot_key)) = self.path.get(self.path.len() - 2)
                {
                    return Ok(Some((&self.path[..self.path.len() - 2], aot_key)));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
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

    pub fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        let (_doc, inner) = self.read_checked(py)?;
        let item = self.navigate(&inner)?;
        Ok(item_ops::item_bool(item))
    }

    pub fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let (_doc, inner) = self.read_checked(py)?;
        let item = self.navigate(&inner)?;
        item_ops::item_str(item, py)
    }

    pub fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let (_doc, inner) = self.read_checked(py)?;
        let item = self.navigate(&inner)?;
        Ok(item_ops::item_repr(item))
    }

    /// Return the TOML representation of this item.
    pub fn as_toml(&self, py: Python<'_>) -> PyResult<String> {
        let (_doc, inner) = self.read_checked(py)?;
        let item = self.navigate(&inner)?;
        Ok(item.to_string().trim().to_owned())
    }

    pub fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let doc = self.doc(other.py());
        Ok(with_resolved_item(
            other,
            doc,
            |d| self.check_fresh(d),
            |inner, needle| {
                let item = self.navigate(inner)?;
                Ok(equality::items_structural_eq(item, needle))
            },
        )?
        .unwrap_or(false))
    }

    /// The underlying data as a native Python object (int, str, list, dict, etc).
    #[getter]
    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let (_doc, inner) = self.read_checked(py)?;
        let item = self.navigate(&inner)?;
        item_ops::item_to_py(item, py)
    }

    // ---- comment access ----

    /// The comment lines before this entry, or None.
    #[getter]
    pub fn comment(&self, py: Python<'_>) -> PyResult<Option<String>> {
        if self.path.is_empty() {
            return Ok(None);
        }
        let (_doc, inner) = self.read_checked(py)?;
        if let Some((ancestor_path, key)) = self.block_comment_target(&inner)? {
            let ancestor = item_ops::navigate_path(&inner, ancestor_path)?;
            Ok(comments::get_block_comment(ancestor, key))
        } else {
            let item = self.navigate(&inner)?;
            let decor = item
                .as_value()
                .map(|v| v.decor())
                .or(item.as_table().map(|t| t.decor()));
            Ok(decor.and_then(comments::get_element_block_comment))
        }
    }

    /// Set or clear the block comment above this entry.
    ///
    /// Each non-empty line must start with ``#``.  Pass ``None`` to remove
    /// the comment.  Empty lines in the string produce blank lines above
    /// the entry.
    #[setter]
    pub fn set_comment(&self, py: Python<'_>, value: Option<&str>) -> PyResult<()> {
        if self.path.is_empty() {
            return Err(PyTypeError::new_err("cannot set comment on root"));
        }
        let (_doc, mut inner) = self.write_checked(py)?;
        if let Some((ancestor_path, key)) = self.block_comment_target(&inner)? {
            let key = key.to_owned();
            let ancestor = item_ops::navigate_path_mut(&mut inner, ancestor_path)?;
            comments::set_block_comment(ancestor, &key, value)?;
        } else {
            let item = self.navigate_mut(&mut inner)?;
            let decor = match item {
                ItemRs::Value(v) => v.decor_mut(),
                ItemRs::Table(t) => t.decor_mut(),
                _ => return Ok(()),
            };
            comments::set_element_block_comment(decor, value)?;
        }
        Ok(())
    }

    /// The inline comment after this value (e.g. `key = 1 # this part`), or None.
    #[getter]
    pub fn inline_comment(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let (_doc, inner) = self.read_checked(py)?;
        if let Some(comment) = self.element_inline_comment(&inner)? {
            return Ok(Some(comment));
        }
        let item = self.navigate(&inner)?;
        Ok(comments::get_inline_comment(item))
    }

    /// Set or clear the inline comment on this entry.
    ///
    /// The value must start with ``#`` (e.g. ``"# my note"``).
    /// Pass ``None`` to remove the comment.
    #[setter]
    pub fn set_inline_comment(&self, py: Python<'_>, value: Option<&str>) -> PyResult<()> {
        let (_doc, mut inner) = self.write_checked(py)?;
        let raw = match value {
            Some(text) => comments::validate_inline_comment(text)?,
            None => String::new(),
        };
        if let Some(Key::Int(idx)) = self.path.last() {
            let parent = self.navigate_parent_mut(&mut inner)?;
            if let Some(array) = parent.as_value_mut().and_then(|v| v.as_array_mut()) {
                comments::set_array_inline_comment(array, *idx, &raw);
                return Ok(());
            }
        }
        if let Some(Key::Str(key)) = self.path.last()
            && self.path.len() >= 2
        {
            let parent = self.navigate_parent_mut(&mut inner)?;
            if let Some(it) = parent.as_value_mut().and_then(|v| v.as_inline_table_mut()) {
                comments::set_inline_table_inline_comment(it, key, &raw);
                return Ok(());
            }
        }
        let item = self.navigate_mut(&mut inner)?;
        comments::set_inline_comment(item, value)?;
        Ok(())
    }

    // ---- shared methods ----

    pub fn clear(&self, py: Python<'_>) -> PyResult<()> {
        let (doc, mut inner) = self.write_checked(py)?;
        let item = self.navigate_mut(&mut inner)?;
        item_ops::item_clear(item)?;
        self.bump_self(doc);
        Ok(())
    }

    /// Normalize formatting of this item (spacing, trailing commas, etc.).
    ///
    /// Useful after mutations that leave behind awkward whitespace.
    /// This is shallow - it formats the item itself, not nested sub-tables.
    /// Note: any comments on the formatted item will be removed.
    pub fn fmt(&self, py: Python<'_>) -> PyResult<()> {
        let (_doc, mut inner) = self.write_checked(py)?;
        let item = self.navigate_mut(&mut inner)?;
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
            revision: AtomicU64::new(0),
        };
        Self::into_typed(py, base)
    }
}

/// Parse a TOML value fragment and verify it produces the expected subclass.
pub(crate) fn parse_as<T: pyo3::type_object::PyTypeInfo>(
    py: Python<'_>,
    text: &str,
    class_name: &str,
    expected: &str,
) -> PyResult<Py<PyAny>> {
    let result = ItemProxy::parse(py, text)?;
    if result.bind(py).is_instance_of::<T>() {
        Ok(result)
    } else {
        Err(PyValueError::new_err(format!(
            "{class_name}.parse() requires a {expected} value, got {}",
            result.bind(py).get_type().qualname()?,
        )))
    }
}
