"""Hypothesis property tests: arbitrary document mutations always produce valid TOML."""

from __future__ import annotations

import contextlib
import copy
from collections.abc import Callable
from typing import TYPE_CHECKING

import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from tests.conftest import toml_literal
from tomledit import DictItem, Document, ListItem

if TYPE_CHECKING:
    from tomledit import Item

# ---------------------------------------------------------------------------
# Strategies for TOML values and keys
# ---------------------------------------------------------------------------

# TOML-safe scalars (no inf/nan — TOML doesn't support them)
toml_scalars = st.one_of(
    st.text(
        st.characters(blacklist_categories=["Cs"], blacklist_characters=("\x00",)),
        max_size=20,
    ),
    st.integers(min_value=-(2**53), max_value=2**53),
    st.floats(allow_nan=False, allow_infinity=False),
    st.booleans(),
)

# Flat TOML values (scalars + small lists/dicts of scalars)
toml_values: st.SearchStrategy[object] = st.recursive(
    toml_scalars,
    lambda children: st.one_of(
        st.lists(children, max_size=5),
        st.dictionaries(
            st.text(
                st.characters(
                    whitelist_categories=("L", "N"),
                    min_codepoint=ord("a"),
                    max_codepoint=ord("z"),
                ),
                min_size=1,
                max_size=8,
            ),
            children,
            max_size=4,
        ),
    ),
    max_leaves=10,
)

# TOML key names (bare keys: alphanumeric + dash + underscore)
toml_keys = st.text(
    st.sampled_from("abcdefghijklmnopqrstuvwxyz_-0123456789"),
    min_size=1,
    max_size=10,
).filter(lambda s: s[0].isalpha())

# Comments: valid `# ...` strings (no newlines, starts with #, printable only)
# TOML forbids control characters U+0000-U+0008, U+000A-U+001F, U+007F in comments.
_comment_chars = st.characters(
    whitelist_categories=("L", "M", "N", "P", "S", "Z"),
    blacklist_characters=("\n", "\r", "\x00"),
)
inline_comments = st.text(_comment_chars, min_size=0, max_size=30).map(
    lambda s: f"# {s}"
)

# Multi-line block comments: 1-3 comment lines joined by newlines.
block_comments = st.lists(
    st.text(_comment_chars, min_size=0, max_size=30).map(lambda s: f"# {s}"),
    min_size=1,
    max_size=3,
).map("\n".join)

# Mixed inner values: scalars, lists-of-scalars, and dicts-of-scalars so that
# dict/list containers can hold heterogeneous children.
_inner_values = st.one_of(
    toml_scalars,
    st.lists(toml_scalars, min_size=1, max_size=3),
    st.dictionaries(toml_keys, toml_scalars, min_size=1, max_size=2),
)

# Rich initial values: include nested tables, dicts-with-lists, and AoTs
initial_values: st.SearchStrategy[object] = st.one_of(
    toml_scalars,
    st.dictionaries(toml_keys, _inner_values, min_size=1, max_size=3),
    st.lists(toml_scalars, min_size=1, max_size=4),
    # AoT: list of dicts
    st.lists(
        st.dictionaries(toml_keys, toml_scalars, min_size=1, max_size=2),
        min_size=1,
        max_size=2,
    ),
)

# Type-change values: all structural types including AoTs
type_change_values = st.one_of(
    toml_scalars,
    st.dictionaries(toml_keys, _inner_values, max_size=3),
    st.lists(toml_scalars, min_size=1, max_size=4),
    st.lists(
        st.dictionaries(toml_keys, toml_scalars, min_size=1, max_size=3),
        min_size=1,
        max_size=3,
    ),
)

# Type alias for mutation callables
Mutation = Callable[[Document], None]


# ---------------------------------------------------------------------------
# Mutation strategies — each draws its own parameters and returns a callable
# ---------------------------------------------------------------------------


@st.composite
def set_key(draw: st.DrawFn) -> Mutation:
    key, value = draw(toml_keys), draw(toml_values)

    def apply(doc: Document) -> None:
        doc[key] = value

    return apply


@st.composite
def del_key(draw: st.DrawFn) -> Mutation:
    key = draw(toml_keys)

    def apply(doc: Document) -> None:
        if key in doc:
            del doc[key]

    return apply


@st.composite
def set_comment(draw: st.DrawFn) -> Mutation:
    key = draw(toml_keys)
    comment = draw(st.one_of(inline_comments, block_comments))

    def apply(doc: Document) -> None:
        if key in doc:
            doc[key].comment = comment

    return apply


@st.composite
def set_inline_comment(draw: st.DrawFn) -> Mutation:
    key, comment = draw(toml_keys), draw(inline_comments)

    def apply(doc: Document) -> None:
        if key in doc:
            with contextlib.suppress(TypeError):
                doc[key].inline_comment = comment

    return apply


@st.composite
def clear_comment(draw: st.DrawFn) -> Mutation:
    key = draw(toml_keys)

    def apply(doc: Document) -> None:
        if key in doc:
            doc[key].comment = None
            with contextlib.suppress(TypeError):
                doc[key].inline_comment = None

    return apply


@st.composite
def update_dict(draw: st.DrawFn) -> Mutation:
    data = draw(st.dictionaries(toml_keys, toml_scalars, max_size=4))

    def apply(doc: Document) -> None:
        doc.update(data)

    return apply


@st.composite
def pop_key(draw: st.DrawFn) -> Mutation:
    key = draw(toml_keys)

    def apply(doc: Document) -> None:
        if key in doc:
            doc.pop(key)

    return apply


@st.composite
def set_default(draw: st.DrawFn) -> Mutation:
    key, value = draw(toml_keys), draw(toml_scalars)

    def apply(doc: Document) -> None:
        doc.setdefault(key, value)

    return apply


@st.composite
def copy_roundtrip(draw: st.DrawFn) -> Mutation:
    deep = draw(st.booleans())

    def apply(doc: Document) -> None:
        copied = copy.deepcopy(doc) if deep else copy.copy(doc)
        Document.parse(copied.as_toml())

    return apply


@st.composite
def set_nested_key(draw: st.DrawFn) -> Mutation:
    outer, inner = draw(toml_keys), draw(toml_keys)
    value = draw(toml_scalars)

    def apply(doc: Document) -> None:
        if outer in doc:
            with contextlib.suppress(TypeError, KeyError):
                doc[outer][inner] = value

    return apply


@st.composite
def set_nested_comment(draw: st.DrawFn) -> Mutation:
    outer, inner = draw(toml_keys), draw(toml_keys)
    comment = draw(inline_comments)

    def apply(doc: Document) -> None:
        if outer in doc:
            with contextlib.suppress(TypeError, KeyError, RuntimeError):
                doc[outer][inner].comment = comment

    return apply


@st.composite
def array_append(draw: st.DrawFn) -> Mutation:
    key = draw(toml_keys)
    initial = draw(st.lists(toml_scalars, min_size=0, max_size=5))
    extra = draw(toml_scalars)

    def apply(doc: Document) -> None:
        doc[key] = initial
        doc[key].append(extra)

    return apply


@st.composite
def array_insert(draw: st.DrawFn) -> Mutation:
    key = draw(toml_keys)
    initial = draw(st.lists(toml_scalars, min_size=1, max_size=5))
    index = draw(st.integers(min_value=-10, max_value=10))
    value = draw(toml_scalars)

    def apply(doc: Document) -> None:
        doc[key] = initial
        idx = index % (len(initial) + 1)
        doc[key].insert(idx, value)

    return apply


@st.composite
def array_remove(draw: st.DrawFn) -> Mutation:
    key = draw(toml_keys)
    initial = draw(st.lists(toml_scalars, min_size=1, max_size=5))
    index = draw(st.integers(min_value=-10, max_value=10))

    def apply(doc: Document) -> None:
        doc[key] = initial
        del doc[key][index % len(initial)]

    return apply


@st.composite
def array_slice_assign(draw: st.DrawFn) -> Mutation:
    key = draw(toml_keys)
    initial = draw(st.lists(toml_scalars, min_size=1, max_size=5))
    start = draw(st.integers(min_value=-10, max_value=10))
    stop = draw(st.integers(min_value=-10, max_value=10))
    replacement = draw(st.lists(toml_scalars, min_size=0, max_size=4))

    def apply(doc: Document) -> None:
        doc[key] = initial
        n = len(initial)
        lo = min(start % (n + 1), n)
        hi = min(max(stop % (n + 1), lo), n)
        doc[key][lo:hi] = replacement

    return apply


@st.composite
def ior_merge(draw: st.DrawFn) -> Mutation:
    data = draw(st.dictionaries(toml_keys, toml_scalars, max_size=4))

    def apply(doc: Document) -> None:
        doc |= Document(data)

    return apply


@st.composite
def clear_collection(draw: st.DrawFn) -> Mutation:
    """Clear a dict-like value. Skips lists — an empty AoT has no TOML
    representation, and we can't distinguish AoT from regular array."""
    key = draw(toml_keys)

    def apply(doc: Document) -> None:
        if key in doc:
            with contextlib.suppress(TypeError, AttributeError):
                doc[key].keys()
                doc[key].clear()

    return apply


@st.composite
def extend_list(draw: st.DrawFn) -> Mutation:
    key = draw(toml_keys)
    initial = draw(st.lists(toml_scalars, min_size=0, max_size=3))
    extra = draw(st.lists(toml_scalars, min_size=1, max_size=4))

    def apply(doc: Document) -> None:
        doc[key] = initial
        doc[key].extend(extra)

    return apply


@st.composite
def array_element_comment(draw: st.DrawFn) -> Mutation:
    key = draw(toml_keys)
    initial = draw(st.lists(toml_scalars, min_size=1, max_size=5))
    index = draw(st.integers(min_value=-10, max_value=10))
    comment = draw(inline_comments)

    def apply(doc: Document) -> None:
        doc[key] = initial
        doc[key][index % len(initial)].comment = comment

    return apply


@st.composite
def type_change(draw: st.DrawFn) -> Mutation:
    """Overwrite a key with a different structural type (scalar↔table↔list)."""
    key = draw(toml_keys)
    value = draw(type_change_values)

    def apply(doc: Document) -> None:
        if key in doc:
            doc[key].comment = "# before type change"
        doc[key] = value

    return apply


@st.composite
def array_add(draw: st.DrawFn) -> Mutation:
    """list + list concatenation — result must be valid TOML."""
    key = draw(toml_keys)
    initial = draw(st.lists(toml_scalars, min_size=0, max_size=4))
    extra = draw(st.lists(toml_scalars, min_size=0, max_size=4))

    def apply(doc: Document) -> None:
        doc[key] = initial
        result = doc[key] + extra
        doc[key] = result.value

    return apply


@st.composite
def array_radd(draw: st.DrawFn) -> Mutation:
    """list + ListItem concatenation (radd path)."""
    key = draw(toml_keys)
    initial = draw(st.lists(toml_scalars, min_size=0, max_size=4))
    prefix = draw(st.lists(toml_scalars, min_size=0, max_size=4))

    def apply(doc: Document) -> None:
        doc[key] = initial
        result = prefix + doc[key]
        doc[key] = result.value

    return apply


@st.composite
def array_mul(draw: st.DrawFn) -> Mutation:
    """list * n repetition — result must be valid TOML."""
    key = draw(toml_keys)
    initial = draw(st.lists(toml_scalars, min_size=0, max_size=3))
    n = draw(st.integers(min_value=-1, max_value=4))

    def apply(doc: Document) -> None:
        doc[key] = initial
        result = doc[key] * n
        doc[key] = result.value

    return apply


@st.composite
def array_imul(draw: st.DrawFn) -> Mutation:
    """list *= n in-place repetition."""
    key = draw(toml_keys)
    initial = draw(st.lists(toml_scalars, min_size=0, max_size=3))
    n = draw(st.integers(min_value=0, max_value=4))

    def apply(doc: Document) -> None:
        doc[key] = initial
        doc[key] *= n

    return apply


@st.composite
def aot_entry_inline_comment(draw: st.DrawFn) -> Mutation:
    """Set an inline comment on an AoT entry header."""
    key = draw(toml_keys)
    tables = draw(
        st.lists(
            st.dictionaries(toml_keys, toml_scalars, min_size=1, max_size=2),
            min_size=1,
            max_size=3,
        )
    )
    index = draw(st.integers(min_value=0, max_value=10))
    comment = draw(st.one_of(inline_comments, st.none()))

    def apply(doc: Document) -> None:
        doc[key] = tables
        doc[key][index % len(tables)].inline_comment = comment

    return apply


mutations = st.one_of(
    set_key(),
    del_key(),
    set_comment(),
    set_inline_comment(),
    clear_comment(),
    update_dict(),
    pop_key(),
    set_default(),
    copy_roundtrip(),
    set_nested_key(),
    set_nested_comment(),
    array_append(),
    array_insert(),
    array_remove(),
    array_slice_assign(),
    ior_merge(),
    clear_collection(),
    extend_list(),
    array_element_comment(),
    type_change(),
    array_add(),
    array_radd(),
    array_mul(),
    array_imul(),
    aot_entry_inline_comment(),
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


CommentTree = dict[str, "str | None | CommentTree"]


def _children(item: Document | Item) -> list[tuple[str, Item]]:
    """Return (key, child) pairs for dict-like or list-like items."""
    if isinstance(item, (Document, DictItem)):
        return [(str(k), item[k]) for k in item]
    if isinstance(item, ListItem):
        return [(str(i), item[i]) for i in range(len(item))]
    return []


def _collect_comments(item: Document | Item) -> CommentTree:
    """Recursively collect block comments, mirroring the document structure."""
    result: CommentTree = {}
    for key, child in _children(item):
        result[key] = child.comment
        nested = _collect_comments(child)
        if nested:
            result[f"{key}."] = nested
    return result


def _collect_inline_comments(item: Document | Item) -> CommentTree:
    """Recursively collect inline comments, mirroring the document structure."""
    result: CommentTree = {}
    for key, child in _children(item):
        try:
            result[key] = child.inline_comment
        except TypeError:
            result[key] = None
        nested = _collect_inline_comments(child)
        if nested:
            result[f"{key}."] = nested
    return result


# ---------------------------------------------------------------------------
# Starting documents — either constructed from a random dict or parsed from
# one of several TOML snippets with pre-existing comments and formatting.
# ---------------------------------------------------------------------------

_PARSED_SOURCES = [
    toml_literal("""
        # Top-level comment
        title = "Example" # inline

        # Owner section
        [owner]
        name = "Alice"
        score = 42 # the answer

        # Server list
        [[servers]]
        host = "alpha"

        [[servers]]
        host = "beta"

        [settings]
        debug = true
        rates = [1, 2, 3]
    """),
    toml_literal("""
        [package]
        name = "demo"
        version = "0.1.0"

        [package.metadata]
        # nested comment
        key = "value" # inline
    """),
    toml_literal("""
        # standalone scalars
        flag = true
        count = 99
        ratio = 3.14
        label = "hello"
    """),
]


@st.composite
def starting_document(draw: st.DrawFn) -> Document:
    """Either build from a random dict or parse a pre-existing TOML source."""
    use_parsed = draw(st.booleans())
    if use_parsed:
        source = draw(st.sampled_from(_PARSED_SOURCES))
        return Document.parse(source)
    initial = draw(st.dictionaries(toml_keys, initial_values, max_size=6))
    doc = Document(initial)
    comments = draw(
        st.lists(st.tuples(toml_keys, inline_comments, inline_comments), max_size=4)
    )
    for key, block, inline in comments:
        if key in doc:
            doc[key].comment = block
            with contextlib.suppress(TypeError):
                doc[key].inline_comment = inline
    return doc


# ---------------------------------------------------------------------------
# Property tests
# ---------------------------------------------------------------------------


@pytest.mark.slow
class TestRoundtripProperty:
    """No matter what mutations we apply, the result is always valid TOML
    whose values and comments survive a parse round-trip."""

    @given(
        doc=starting_document(),
        ops=st.lists(mutations, min_size=1, max_size=10),
    )
    @settings(
        max_examples=2000,
        suppress_health_check=[HealthCheck.too_slow],
    )
    def test_roundtrip(self, doc: Document, ops: list[Mutation]) -> None:
        for op in ops:
            op(doc)

        # repr must not crash
        str(doc)

        # Value extraction round-trip: .value produces something that can
        # construct an equivalent document
        assert Document(doc.value).value == doc.value

        # TOML round-trip: parse(as_toml()) preserves values and comments
        toml_text = doc.as_toml()
        block_before = _collect_comments(doc)
        inline_before = _collect_inline_comments(doc)
        reparsed = Document.parse(toml_text)
        assert reparsed.value == doc.value
        assert _collect_comments(reparsed) == block_before
        assert _collect_inline_comments(reparsed) == inline_before

        # Double round-trip: TOML text is stable after one parse
        assert reparsed.as_toml() == toml_text
