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

// ---------------------------------------------------------------------------
// Resolving a Python value to a `toml_edit::Item`
//
// Five public shapes, distinguished by what the caller needs and what locks
// it already holds.  They share a few private pieces below.
//
// * `resolve_proxy`           — flatten an `ItemProxy` to its underlying
//                               Python value.  Used when we need a Python
//                               object (e.g. to call `__index__` on it).
// * `with_proxy_item`         — borrow the `Item` behind an `ItemProxy`
//                               under a brief read lock on the proxy's
//                               document.  Proxy only; doesn't handle
//                               `Document` or arbitrary Python values.
// * `with_proxy_or_doc_item`  — borrow the `Item` behind an `ItemProxy` or
//                               a `Document` under a brief read lock on
//                               its owning document.  Returns `None` for
//                               plain Python values.
// * `extract_owned_item`      — own an `Item` from a proxy, a `Document`,
//                               or any TOML-convertible Python value.  No
//                               source lock is retained, so the caller can
//                               then take a write lock on the same document.
// * `with_resolved_item`      — borrow the `Item` behind a value while a
//                               destination read guard is held.  Reuses
//                               the destination guard for same-document
//                               proxies/Documents — this is a correctness
//                               requirement, not just an optimisation:
//                               `parking_lot::RwLock` reads are not
//                               reentrant, and under free-threaded Python
//                               a writer queued between the two reads
//                               would deadlock.
//
// The four "no dest lock" helpers above (`with_proxy_item`,
// `with_proxy_or_doc_item`, `extract_owned_item`, and the proxy arm of
// `resolve_proxy`) all briefly take a read lock on the *source* document
// and so **must not be called while the caller holds a lock on any
// document** — same reentrancy concern.
//
// All return `None` (or `Ok(None)`) when the value is not representable
// in the requested form, so callers can treat the absence as "not a proxy",
// "not a TOML value", "not in the array", etc.
// ---------------------------------------------------------------------------

/// Flatten an `ItemProxy` to its underlying Python value.  Returns `None`
/// when `value` is not a proxy.
pub(crate) fn resolve_proxy(value: &Bound<'_, PyAny>) -> PyResult<Option<Py<PyAny>>> {
    if let Ok(proxy) = value.cast::<ItemProxy>() {
        Ok(Some(proxy.get().value(value.py())?))
    } else {
        Ok(None)
    }
}

/// Borrow the `Item` behind an `ItemProxy` under its own brief read lock,
/// calling `f` with the borrow.  Returns `None` for non-proxies.
pub(crate) fn with_proxy_item<R>(
    value: &Bound<'_, PyAny>,
    f: impl FnOnce(&ItemRs) -> R,
) -> PyResult<Option<R>> {
    let Ok(proxy) = value.cast::<ItemProxy>() else {
        return Ok(None);
    };
    let proxy = proxy.get();
    let (_doc, inner) = proxy.read_checked(value.py())?;
    let item = proxy.navigate(&inner)?;
    Ok(Some(f(item)))
}

/// Borrow the `Item` behind an `ItemProxy` or a `Document` under a brief
/// read lock on its owning document, calling `f` with the borrow.  Returns
/// `None` for plain Python values.
pub(crate) fn with_proxy_or_doc_item<R>(
    value: &Bound<'_, PyAny>,
    f: impl FnOnce(&ItemRs) -> R,
) -> PyResult<Option<R>> {
    if let Ok(proxy) = value.cast::<ItemProxy>() {
        let proxy = proxy.get();
        let (_doc, inner) = proxy.read_checked(value.py())?;
        let item = proxy.navigate(&inner)?;
        return Ok(Some(f(item)));
    }
    if let Ok(doc_bound) = value.cast::<Document>() {
        let inner = doc_bound.get().inner.read();
        return Ok(Some(f(inner.as_item())));
    }
    Ok(None)
}

/// Produce an owned `Item` from `value`, handling proxies, `Document`s, and
/// arbitrary TOML-convertible Python values.  Returns `None` for objects
/// with no TOML representation (caller typically treats as "not found").
pub(crate) fn extract_owned_item(value: &Bound<'_, PyAny>) -> PyResult<Option<ItemRs>> {
    if let Some(item) = with_proxy_or_doc_item(value, ItemRs::clone)? {
        return Ok(Some(item));
    }
    Ok(try_extract_item(value)?.map(|item| item.0))
}

/// Borrow the `Item` behind `value` while a destination read guard is held,
/// reusing the guard for same-document proxies/Documents.
///
/// Fast path: if `value` is a proxy or `Document`, `f` runs without cloning.
/// For same-document needles the destination's existing guard is reused
/// (avoiding a recursive read that could deadlock under a queued writer).
/// Slow path: the value is extracted to an owned `Item` (no lock held), then
/// `f` runs under a fresh destination read lock.
///
/// `check_fresh` is invoked **under** every destination read lock taken,
/// closing the TOCTOU window between freshness check and read.
///
/// Returns `None` when `value` has no TOML representation (caller treats as
/// "not found" / "not equal").
pub(crate) fn with_resolved_item<R>(
    value: &Bound<'_, PyAny>,
    doc: &Document,
    check_fresh: impl Fn(&Document) -> PyResult<()>,
    f: impl Fn(&DocumentRs, &ItemRs) -> PyResult<R>,
) -> PyResult<Option<R>> {
    {
        let inner = doc.inner.read();
        check_fresh(doc)?;
        let ctx = ReadCtx { doc, inner: &inner };
        if let Some(result) = resolve_other_item(value, &ctx, |item| f(&inner, item))? {
            return result.map(Some);
        }
    }
    let Some(extracted) = try_extract_item(value)? else {
        return Ok(None);
    };
    let inner = doc.inner.read();
    check_fresh(doc)?;
    f(&inner, &extracted.0).map(Some)
}

// ---------------------------------------------------------------------------
// Private pieces shared by `with_resolved_item`'s same-doc-reuse path.
// ---------------------------------------------------------------------------

/// Carries an existing destination guard so that same-document proxy/Document
/// resolution can reuse it instead of acquiring a (non-reentrant) nested read.
struct ReadCtx<'a> {
    doc: &'a Document,
    inner: &'a DocumentRs,
}

/// Resolve `value` as an `ItemProxy` or `Document` under the destination
/// guard `ctx`, calling `f` with the borrow.  Returns `None` when `value` is
/// neither.  Same-document needles reuse `ctx`'s guard.
fn resolve_other_item<R>(
    value: &Bound<'_, PyAny>,
    ctx: &ReadCtx<'_>,
    f: impl Fn(&ItemRs) -> R,
) -> PyResult<Option<R>> {
    if let Ok(proxy) = value.cast::<ItemProxy>() {
        let proxy = proxy.get();
        let doc = proxy.document.bind(value.py()).get();
        if std::ptr::eq(ctx.doc, doc) {
            // Same document: reuse the caller's guard.  `check_fresh` is
            // coherent because any invalidating mutation would have been
            // stamped into the trie before the guard was taken.
            proxy.check_fresh(doc)?;
            let item = proxy.navigate(ctx.inner)?;
            return Ok(Some(f(item)));
        }
        let (_doc, inner) = proxy.read_checked(value.py())?;
        let item = proxy.navigate(&inner)?;
        return Ok(Some(f(item)));
    }
    if let Ok(doc_bound) = value.cast::<Document>() {
        let doc = doc_bound.get();
        if std::ptr::eq(ctx.doc, doc) {
            return Ok(Some(f(ctx.inner.as_item())));
        }
        let inner = doc.inner.read();
        return Ok(Some(f(inner.as_item())));
    }
    Ok(None)
}

/// Try to extract a Python object as a `toml_edit::Item`.  Returns `None`
/// for objects that have no TOML representation (caller treats as "not found"
/// / "not equal").
fn try_extract_item(value: &Bound<'_, PyAny>) -> PyResult<Option<Item>> {
    match value.extract::<Item>() {
        Ok(item) => Ok(Some(item)),
        Err(e) if e.is_instance_of::<PyTypeError>(value.py()) => Ok(None),
        Err(e) => Err(e),
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

    /// Snapshot the parts needed to build a typed child proxy at
    /// `self.path + [key]` under an existing `inner` guard.
    ///
    /// Revision is sampled internally so it stays consistent with the
    /// held guard: callers cannot accidentally read it before locking.
    pub(crate) fn snapshot_child(
        &self,
        doc: &Document,
        inner: &DocumentRs,
        key: Key,
    ) -> PyResult<ProxyParts> {
        let mut path = self.path.clone();
        path.push(key);
        ProxyParts::snapshot(inner, path, doc.revision())
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

/// A snapshot of everything needed to mint a typed proxy at a given path,
/// captured under a single `inner.read()` (or write) guard.
///
/// The two-phase split is deliberate: [`snapshot`] is the only step that
/// needs the document lock, and it returns plain data; [`build`] performs
/// the Python allocation after the lock has been released.  Composing
/// existence checks and proxy minting is therefore TOCTOU-free without
/// holding the document lock across `Py::new`.
///
/// [`snapshot`]: Self::snapshot
/// [`build`]: Self::build
pub(crate) struct ProxyParts {
    path: Vec<Key>,
    revision: u64,
    kind: ItemKind,
}

impl ProxyParts {
    /// Snapshot the parts needed to build a typed proxy at `path`.
    ///
    /// The caller must hold a read or write guard on `inner`, and
    /// `revision` must have been sampled under that same guard so that
    /// the resulting proxy's revision is consistent with the navigated
    /// item.
    pub(crate) fn snapshot(inner: &DocumentRs, path: Vec<Key>, revision: u64) -> PyResult<Self> {
        let kind = item_kind(item_ops::navigate_path(inner, &path)?);
        Ok(Self {
            path,
            revision,
            kind,
        })
    }

    /// Build the typed Python proxy.  Holds no document locks.
    pub(crate) fn build(self, document: &Py<Document>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let base = ItemProxy::new(document.clone_ref(py), self.path, self.revision);
        into_typed_proxy(py, base, self.kind)
    }

    /// Wrap a freshly-constructed `DocumentRs` (whose sole root entry is at
    /// `"_"`) as the appropriate typed Python proxy.
    ///
    /// Used by helpers that materialise standalone values into a new
    /// document, e.g. `Item.parse`, `DictItem.__or__`, and `ListItem`'s
    /// arithmetic operators.
    pub(crate) fn wrap_fresh(doc_rs: DocumentRs, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let path = vec![Key::Str("_".to_owned())];
        let parts = Self::snapshot(&doc_rs, path, 0)?;
        let doc = Py::new(py, Document::from_inner(doc_rs))?;
        parts.build(&doc, py)
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
                .map(ValueRs::decor)
                .or_else(|| item.as_table().map(toml_edit::Table::decor));
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
        ProxyParts::wrap_fresh(doc_rs, py)
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
