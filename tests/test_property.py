"""Hypothesis property tests: arbitrary document mutations always produce valid TOML."""

from __future__ import annotations

import contextlib
import copy
from collections.abc import Callable

import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from tomledit import Document

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

# Rich initial values: include nested tables and AoTs alongside scalars
initial_values: st.SearchStrategy[object] = st.one_of(
    toml_scalars,
    st.dictionaries(toml_keys, toml_scalars, min_size=1, max_size=3),
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
    st.dictionaries(toml_keys, toml_scalars, max_size=3),
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
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _collect_comments(doc: Document) -> dict[str, tuple[str | None, str | None]]:
    """Snapshot block + inline comments for every top-level key.

    Block comments (`.comment`) work on all item types including AoT.
    Inline comments are not supported on AoT, so we collect None for those.
    """
    result: dict[str, tuple[str | None, str | None]] = {}
    for key in doc:
        item = doc[key]
        block = item.comment
        try:
            inline = item.inline_comment
        except TypeError:
            inline = None
        result[str(key)] = (block, inline)
    return result


# ---------------------------------------------------------------------------
# Property tests
# ---------------------------------------------------------------------------


@pytest.mark.slow
class TestRoundtripProperty:
    """No matter what mutations we apply, the result is always valid TOML
    whose values and comments survive a parse round-trip."""

    @given(
        initial=st.dictionaries(toml_keys, initial_values, max_size=6),
        comments=st.lists(
            st.tuples(toml_keys, inline_comments, inline_comments),
            max_size=4,
        ),
        ops=st.lists(mutations, min_size=1, max_size=10),
    )
    @settings(
        max_examples=2000,
        suppress_health_check=[HealthCheck.too_slow],
    )
    def test_roundtrip(
        self,
        initial: dict[str, object],
        comments: list[tuple[str, str, str]],
        ops: list[Mutation],
    ) -> None:
        doc = Document(initial)
        for key, block, inline in comments:
            if key in doc:
                doc[key].comment = block
                with contextlib.suppress(TypeError):
                    doc[key].inline_comment = inline
        for op in ops:
            op(doc)

        toml_text = doc.as_toml()
        comments_before = _collect_comments(doc)
        reparsed = Document.parse(toml_text)
        assert reparsed.value == doc.value
        assert _collect_comments(reparsed) == comments_before
