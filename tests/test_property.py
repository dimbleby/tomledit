"""Hypothesis property tests: arbitrary document mutations always produce valid TOML."""

from __future__ import annotations

import contextlib
from typing import Protocol

import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from tomledit import Document


class Mutation(Protocol):
    """Any object with an .apply(doc) method."""

    def apply(self, doc: Document) -> None: ...


# ---------------------------------------------------------------------------
# Strategies
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
inline_comments = st.text(
    st.characters(
        whitelist_categories=("L", "M", "N", "P", "S", "Z"),
        blacklist_characters=("\n", "\r", "\x00"),
    ),
    min_size=0,
    max_size=30,
).map(lambda s: f"# {s}")


# ---------------------------------------------------------------------------
# Mutation commands — a small DSL for random document edits
# ---------------------------------------------------------------------------


class SetKey:
    """Set a top-level key to a value."""

    def __init__(self, key: str, value: object) -> None:
        self.key = key
        self.value = value

    def apply(self, doc: Document) -> None:
        doc[self.key] = self.value


class DelKey:
    """Delete a top-level key (if it exists)."""

    def __init__(self, key: str) -> None:
        self.key = key

    def apply(self, doc: Document) -> None:
        if self.key in doc:
            del doc[self.key]


class SetComment:
    """Set a block comment on a top-level key (if it exists)."""

    def __init__(self, key: str, comment: str) -> None:
        self.key = key
        self.comment = comment

    def apply(self, doc: Document) -> None:
        if self.key in doc:
            doc[self.key].comment = self.comment


class SetInlineComment:
    """Set an inline comment on a top-level key (if it exists)."""

    def __init__(self, key: str, comment: str) -> None:
        self.key = key
        self.comment = comment

    def apply(self, doc: Document) -> None:
        if self.key in doc:
            with contextlib.suppress(TypeError):
                doc[self.key].inline_comment = self.comment


class UpdateDict:
    """Merge a dict into the document."""

    def __init__(self, data: dict[str, object]) -> None:
        self.data = data

    def apply(self, doc: Document) -> None:
        doc.update(self.data)


class ArrayAppend:
    """Set a key to a list, then append a value."""

    def __init__(self, key: str, initial: list[object], extra: object) -> None:
        self.key = key
        self.initial = initial
        self.extra = extra

    def apply(self, doc: Document) -> None:
        doc[self.key] = self.initial
        doc[self.key].append(self.extra)


class ArrayInsert:
    """Set a key to a list, then insert at a position."""

    def __init__(
        self, key: str, initial: list[object], index: int, value: object
    ) -> None:
        self.key = key
        self.initial = initial
        self.index = index
        self.value = value

    def apply(self, doc: Document) -> None:
        doc[self.key] = self.initial
        if self.initial:
            idx = self.index % (len(self.initial) + 1)
            doc[self.key].insert(idx, self.value)


class ArrayRemove:
    """Set a key to a list, then remove by index."""

    def __init__(self, key: str, initial: list[object], index: int) -> None:
        self.key = key
        self.initial = initial
        self.index = index

    def apply(self, doc: Document) -> None:
        doc[self.key] = self.initial
        if self.initial:
            idx = self.index % len(self.initial)
            del doc[self.key][idx]


class ArraySliceAssign:
    """Set a key to a list, then replace a slice."""

    def __init__(
        self,
        key: str,
        initial: list[object],
        start: int,
        stop: int,
        replacement: list[object],
    ) -> None:
        self.key = key
        self.initial = initial
        self.start = start
        self.stop = stop
        self.replacement = replacement

    def apply(self, doc: Document) -> None:
        doc[self.key] = self.initial
        n = len(self.initial)
        if n == 0:
            return
        lo = min(self.start % (n + 1), n)
        hi = min(max(self.stop % (n + 1), lo), n)
        doc[self.key][lo:hi] = self.replacement


# Strategies for mutation commands
set_key_cmd = st.builds(SetKey, toml_keys, toml_values)
del_key_cmd = st.builds(DelKey, toml_keys)
set_comment_cmd = st.builds(SetComment, toml_keys, inline_comments)
set_inline_comment_cmd = st.builds(SetInlineComment, toml_keys, inline_comments)
update_cmd = st.builds(
    UpdateDict,
    st.dictionaries(toml_keys, toml_scalars, max_size=4),
)
array_append_cmd = st.builds(
    ArrayAppend,
    toml_keys,
    st.lists(toml_scalars, min_size=0, max_size=5),
    toml_scalars,
)
array_insert_cmd = st.builds(
    ArrayInsert,
    toml_keys,
    st.lists(toml_scalars, min_size=1, max_size=5),
    st.integers(min_value=-10, max_value=10),
    toml_scalars,
)
array_remove_cmd = st.builds(
    ArrayRemove,
    toml_keys,
    st.lists(toml_scalars, min_size=1, max_size=5),
    st.integers(min_value=-10, max_value=10),
)
array_slice_cmd = st.builds(
    ArraySliceAssign,
    toml_keys,
    st.lists(toml_scalars, min_size=1, max_size=5),
    st.integers(min_value=-10, max_value=10),
    st.integers(min_value=-10, max_value=10),
    st.lists(toml_scalars, min_size=0, max_size=4),
)

mutations = st.one_of(
    set_key_cmd,
    del_key_cmd,
    set_comment_cmd,
    set_inline_comment_cmd,
    update_cmd,
    array_append_cmd,
    array_insert_cmd,
    array_remove_cmd,
    array_slice_cmd,
)


# ---------------------------------------------------------------------------
# Property tests
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


@pytest.mark.slow
class TestRoundtripProperty:
    """No matter what mutations we apply, the result is always valid TOML
    whose values and comments survive a parse round-trip."""

    @given(
        initial=st.dictionaries(toml_keys, toml_scalars, max_size=6),
        comments=st.lists(
            st.tuples(toml_keys, inline_comments, inline_comments),
            max_size=4,
        ),
        ops=st.lists(mutations, min_size=1, max_size=10),
    )
    @settings(
        max_examples=1000,
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
                doc[key].inline_comment = inline
        for op in ops:
            op.apply(doc)

        toml_text = doc.as_toml()
        comments_before = _collect_comments(doc)
        reparsed = Document.parse(toml_text)
        assert reparsed.value == doc.value
        assert _collect_comments(reparsed) == comments_before
