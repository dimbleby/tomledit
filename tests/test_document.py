"""Tests for the Document class: constructor, value, identity, fmt, copy."""

from __future__ import annotations

import copy

import pytest

from tomledit import Document

# ---------------------------------------------------------------------------
# Document constructor
# ---------------------------------------------------------------------------


class TestDocumentConstructor:
    """Document() can create empty or from a table."""

    def test_empty(self) -> None:
        doc = Document()
        assert len(doc) == 0
        assert not doc.as_toml()

    def test_empty_then_populate(self) -> None:
        doc = Document()
        doc["name"] = "test"
        doc["version"] = 1
        assert doc["name"] == "test"
        assert doc["version"] == 1

    def test_from_dict(self) -> None:
        doc = Document({"a": 1, "b": "two"})
        assert doc["a"] == 1
        assert doc["b"] == "two"

    def test_from_popped_table(self) -> None:
        src = Document.parse("[project]\nname = 'hello'\nversion = '1.0'\n")
        data = src.pop("project")
        doc = Document(data)
        assert doc["name"] == "hello"
        assert doc["version"] == "1.0"

    def test_non_dict_raises(self) -> None:
        with pytest.raises(TypeError, match="must be a dict"):
            Document(42)  # type: ignore[arg-type]

    def test_none_is_empty(self) -> None:
        doc = Document(None)
        assert len(doc) == 0


# ---------------------------------------------------------------------------
# Document.value
# ---------------------------------------------------------------------------


class TestDocumentValue:
    """Document.value returns the entire document as a native Python dict."""

    def test_value_returns_flat_native_dict(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        v = doc.value
        assert v == {"a": 1, "b": 2}
        assert type(v) is dict

    def test_value_with_nested_table(self) -> None:
        doc = Document.parse("[section]\nx = 1\ny = 2\n")
        assert doc.value == {"section": {"x": 1, "y": 2}}

    def test_empty(self) -> None:
        doc = Document()
        assert doc.value == {}

    def test_value_on_complex_fixture(self, doc: Document) -> None:
        v = doc.value
        assert v["title"] == "Example"
        assert v["owner"] == {"name": "Alice", "age": 30, "active": True}
        assert v["database"]["ports"] == [8001, 8001, 8002]

    def test_value_is_a_copy(self) -> None:
        doc = Document.parse("x = 1\n")
        v = doc.value
        v["x"] = 999
        assert doc["x"] == 1


# ---------------------------------------------------------------------------
# Document identity (repr, bool, equality)
# ---------------------------------------------------------------------------


class TestDocumentIdentity:
    """Document repr, bool, and equality."""

    def test_repr(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        assert repr(doc) == "Document(2 keys)"

    def test_repr_empty(self) -> None:
        doc = Document.parse("")
        assert repr(doc) == "Document(0 keys)"

    def test_bool_nonempty(self) -> None:
        doc = Document.parse("x = 1\n")
        assert bool(doc) is True

    def test_bool_empty(self) -> None:
        doc = Document.parse("")
        assert bool(doc) is False

    def test_eq_same_content(self) -> None:
        a = Document.parse("x = 1\ny = 2\n")
        b = Document.parse("x = 1\ny = 2\n")
        assert a == b

    def test_eq_different_content(self) -> None:
        a = Document.parse("x = 1\n")
        b = Document.parse("x = 2\n")
        assert a != b

    def test_eq_dict(self) -> None:
        doc = Document.parse("x = 1\ny = 2\n")
        assert doc == {"x": 1, "y": 2}
        assert doc != {"x": 1}

    def test_eq_unrelated_type(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc != 42
        assert doc != "x = 1"

    def test_eq_doc_vs_doc_type_strict(self) -> None:
        """Document-to-Document equality should respect TOML types."""
        a = Document.parse("x = true\n")
        b = Document.parse("x = 1\n")
        assert a != b


# ---------------------------------------------------------------------------
# Document.fmt()
# ---------------------------------------------------------------------------


class TestDocumentFmt:
    def test_fmt_normalizes_root_whitespace(self) -> None:
        doc = Document.parse("  x  =  1  \n")
        doc.fmt()
        assert doc.as_toml() == "x = 1\n"

    def test_fmt_does_not_touch_table_internals(self) -> None:
        """fmt() only reformats root-level decor, not inside tables."""
        doc = Document.parse("[t]\n  x  =  1\n")
        doc.fmt()
        assert doc.as_toml() == "[t]\n  x  =  1\n"

    def test_fmt_strips_comments(self) -> None:
        doc = Document.parse("# comment\na = 1 # inline\n")
        doc.fmt()
        assert doc.as_toml() == "a = 1\n"


# ---------------------------------------------------------------------------
# copy / deepcopy
# ---------------------------------------------------------------------------


class TestDocumentCopy:
    def test_copy_produces_independent_document(self) -> None:
        doc = Document.parse("x = 1\n")
        doc2 = copy.copy(doc)
        doc2["x"] = 2
        assert doc["x"] == 1
        assert doc2["x"] == 2

    def test_deepcopy_produces_independent_document(self) -> None:
        doc = Document.parse('[t]\nk = "v"\n')
        doc2 = copy.deepcopy(doc)
        doc2["t"]["k"] = "changed"
        assert doc["t"]["k"] == "v"
        assert doc2["t"]["k"] == "changed"

    def test_copy_preserves_formatting(self) -> None:
        text = "  x  =  1  \n\n[section]\n  key  =  'val'\n"
        doc = Document.parse(text)
        doc2 = copy.copy(doc)
        assert doc2.as_toml() == text
