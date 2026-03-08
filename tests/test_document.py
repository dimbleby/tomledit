"""Tests for the Document class: dict protocol, constructor, value, and completeness."""

from __future__ import annotations

import pytest

from tests.conftest import make_doc
from tomledit import Document

# ---------------------------------------------------------------------------
# Dict-like protocol on Document
# ---------------------------------------------------------------------------


class TestDocumentDictProtocol:
    def test_contains(self) -> None:
        doc = make_doc()
        assert "title" in doc
        assert "nonexistent" not in doc

    def test_len(self) -> None:
        doc = make_doc()
        assert len(doc) == 4  # title, owner, database, servers

    def test_iter(self) -> None:
        doc = make_doc()
        keys = list(doc)
        assert "title" in keys
        assert "owner" in keys

    def test_keys(self) -> None:
        doc = make_doc()
        assert set(doc.keys()) == {"title", "owner", "database", "servers"}

    def test_del(self) -> None:
        doc = make_doc()
        del doc["title"]
        assert "title" not in doc

    def test_pop(self) -> None:
        doc = make_doc()
        doc.pop("title")
        assert "title" not in doc

    def test_clear(self) -> None:
        doc = make_doc()
        doc.clear()
        assert len(doc) == 0

    def test_get_existing(self) -> None:
        doc = make_doc()
        item = doc.get("title")
        assert item is not None

    def test_get_missing(self) -> None:
        doc = make_doc()
        assert doc.get("nope") is None

    def test_getitem_missing_raises(self) -> None:
        doc = make_doc()
        with pytest.raises(KeyError):
            doc["nope"]


# ---------------------------------------------------------------------------
# Document constructor
# ---------------------------------------------------------------------------


class TestDocumentConstructor:
    """Document() can create empty or from a table."""

    def test_empty(self) -> None:
        doc = Document()
        assert len(doc) == 0
        assert not str(doc)

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

    def test_simple(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        v = doc.value
        assert v == {"a": 1, "b": 2}
        assert type(v) is dict

    def test_nested(self) -> None:
        doc = Document.parse("[section]\nx = 1\ny = 2\n")
        assert doc.value == {"section": {"x": 1, "y": 2}}

    def test_empty(self) -> None:
        doc = Document()
        assert doc.value == {}

    def test_complex(self) -> None:
        doc = make_doc()
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
# Document completeness (repr, bool, eq, del, update, setdefault)
# ---------------------------------------------------------------------------


class TestDocumentCompleteness:
    """Document should have a complete dict-like API."""

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

    def test_delitem_raises_key_error(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(KeyError):
            del doc["nonexistent"]

    def test_delitem_existing_key(self) -> None:
        doc = Document.parse("x = 1\ny = 2\n")
        del doc["x"]
        assert "x" not in doc
        assert "y" in doc

    def test_update(self) -> None:
        doc = Document.parse("x = 1\n")
        doc.update({"x": 10, "y": 20})
        assert doc["x"] == 10
        assert doc["y"] == 20

    def test_setdefault_missing(self) -> None:
        doc = Document.parse("x = 1\n")
        result = doc.setdefault("y", 42)
        assert result == 42
        assert doc["y"] == 42

    def test_setdefault_existing(self) -> None:
        doc = Document.parse("x = 1\n")
        result = doc.setdefault("x", 99)
        assert result == 1


# ---------------------------------------------------------------------------
# Document.fmt()
# ---------------------------------------------------------------------------


class TestDocumentFmt:
    def test_fmt_normalizes_root_whitespace(self) -> None:
        doc = Document.parse("  x  =  1  \n")
        doc.fmt()
        assert str(doc) == "x = 1\n"

    def test_fmt_does_not_touch_table_internals(self) -> None:
        """fmt() only reformats root-level decor, not inside tables."""
        doc = Document.parse("[t]\n  x  =  1\n")
        doc.fmt()
        assert str(doc) == "[t]\n  x  =  1\n"
