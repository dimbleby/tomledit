"""Tests for error handling: no panics, only proper Python exceptions."""

from __future__ import annotations

import pytest

from tests.conftest import toml_literal
from tomledit import Document

# ---------------------------------------------------------------------------
# Parse errors
# ---------------------------------------------------------------------------


class TestParseErrors:
    def test_duplicate_key_raises(self) -> None:
        with pytest.raises(ValueError, match="duplicate"):
            Document.parse(
                toml_literal("""
                x = 1
                x = 2
            """)
            )


# ---------------------------------------------------------------------------
# Unsupported Python types assigned to TOML keys
# ---------------------------------------------------------------------------


class TestUnsupportedValueTypes:
    """Python types without obvious TOML semantics should be rejected."""

    def test_none_rejected(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match="None is not a valid TOML value"):
            doc["x"] = None

    def test_bytearray_rejected(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError):
            doc["x"] = bytearray(b"hello")


# ---------------------------------------------------------------------------
# Operations on wrong item types
# ---------------------------------------------------------------------------


class TestWrongTypeErrors:
    """Each method should raise TypeError when called on the wrong item type."""

    # -- subscript on scalars --

    def test_setitem_on_bool_raises(self) -> None:
        doc = Document.parse("flag = true\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["flag"][0] = False

    def test_setitem_on_float_raises(self) -> None:
        doc = Document.parse("val = 1.5\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["val"][0] = 2

    def test_getitem_int_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["x"][0]

    def test_getitem_str_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["x"]["y"]

    def test_delitem_on_scalar_raises(self) -> None:
        doc = Document.parse("val = 42\n")
        with pytest.raises(TypeError):
            del doc["val"][0]

    def test_contains_on_scalar_raises(self) -> None:
        doc = Document.parse("val = 42\n")
        with pytest.raises(TypeError):
            assert 1 in doc["val"]

    def test_slice_del_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="does not support slice deletion"):
            del doc["x"][0:1]

    def test_slice_assign_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="does not support slice assignment"):
            doc["x"][0:1] = [1]

    # -- wrong key type --

    def test_getitem_float_key_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [tbl]
            x = 1
        """)
        )
        with pytest.raises(KeyError):
            doc["tbl"][1.5]  # type: ignore[call-overload]  # ty: ignore[invalid-argument-type]

    def test_setitem_int_key_on_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(TypeError, match="strings, not integers"):
            doc["t"][0] = 99

    def test_setitem_str_key_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(TypeError, match="integers, not strings"):
            doc["arr"]["x"] = 99

    def test_delitem_int_key_on_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(KeyError):
            del doc["t"][0]

    def test_delitem_str_key_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(TypeError, match="integers, not strings"):
            del doc["arr"]["x"]

    def test_setitem_float_key_on_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(TypeError, match="indices must be integers or strings"):
            doc["t"][1.5] = 99  # type: ignore[index]  # ty: ignore[invalid-assignment]

    def test_delitem_float_key_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(TypeError, match="indices must be integers or strings"):
            del doc["arr"][1.5]  # type: ignore[arg-type]  # ty: ignore[invalid-argument-type]

    def test_delitem_float_key_on_table_raises_key_error(self) -> None:
        """del table[1.5] should raise KeyError, matching Python dict."""
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(KeyError):
            del doc["t"][1.5]  # type: ignore[arg-type]  # ty: ignore[invalid-argument-type]

    def test_delitem_float_key_on_inline_table_raises_key_error(self) -> None:
        """del inline_table[1.5] should raise KeyError, matching Python dict."""
        doc = Document.parse("t = {a = 1}\n")
        with pytest.raises(KeyError):
            del doc["t"][1.5]  # type: ignore[arg-type]  # ty: ignore[invalid-argument-type]

    def test_getitem_float_key_on_scalar_raises(self) -> None:
        doc = Document.parse("val = 42\n")
        with pytest.raises(TypeError, match="indices must be integers or strings"):
            doc["val"][1.5]  # type: ignore[call-overload]  # ty: ignore[invalid-argument-type]

    def test_delitem_int_key_on_inline_table_raises(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        with pytest.raises(KeyError):
            del doc["t"][0]

    # -- missing keys --

    def test_getitem_missing_nested_key_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(KeyError, match="nonexistent"):
            doc["t"]["nonexistent"]

    def test_delitem_inline_table_missing_key_raises(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        with pytest.raises(KeyError):
            del doc["t"]["nonexistent"]

    # -- dict methods on wrong types --

    def test_get_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(AttributeError):
            doc["arr"].get("x")

    def test_get_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(AttributeError):
            doc["x"].get("y")

    def test_pop_noarg_on_inline_table_raises(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        with pytest.raises(TypeError, match="missing 1 required positional argument"):
            doc["t"].pop()

    # -- list methods on wrong types --

    def test_extend_on_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(AttributeError):
            doc["t"].extend([1, 2])

    # -- view methods on wrong types --


# ---------------------------------------------------------------------------
# Proxy keys/indices: using a ScalarItem as a key or index must not panic
# ---------------------------------------------------------------------------


class TestProxyAsKey:
    """Using a ScalarItem proxy as a key/index in mutating operations must
    not trigger a double-borrow panic.  The proxy's __index__ protocol
    re-borrows the document, so the key must be resolved before taking
    a mutable borrow.
    """

    def test_setitem_array_with_proxy_index(self) -> None:
        doc = Document.parse("idx = 1\narr = [10, 20, 30]\n")
        idx = doc["idx"]
        doc["arr"][idx] = 99  # type: ignore[index]  # ty: ignore[invalid-assignment]
        assert doc["arr"][1] == 99

    def test_delitem_array_with_proxy_index(self) -> None:
        doc = Document.parse("idx = 1\narr = [10, 20, 30]\n")
        idx = doc["idx"]
        del doc["arr"][idx]  # type: ignore[arg-type]  # ty: ignore[invalid-argument-type]
        assert list(doc["arr"]) == [10, 30]

    def test_list_pop_with_proxy_index(self) -> None:
        doc = Document.parse("idx = 1\narr = [10, 20, 30]\n")
        idx = doc["idx"]
        result = doc["arr"].pop(idx)
        assert result == 20
        assert list(doc["arr"]) == [10, 30]

    def test_setitem_table_with_proxy_str_key(self) -> None:
        doc = Document.parse('key = "port"\n[server]\nport = 80\n')
        key = doc["key"]
        doc["server"][key] = 9090  # type: ignore[index]  # ty: ignore[invalid-assignment]
        assert doc["server"]["port"] == 9090

    def test_delitem_table_with_proxy_str_key(self) -> None:
        doc = Document.parse('key = "port"\n[server]\nport = 80\n')
        key = doc["key"]
        del doc["server"][key]  # type: ignore[arg-type]  # ty: ignore[invalid-argument-type]
        assert "port" not in doc["server"]

    def test_setitem_table_with_int_proxy_gives_type_error(self) -> None:
        """An integer proxy used as a table key should raise TypeError,
        not panic from a double borrow in the error-handling path."""
        doc = Document.parse("idx = 1\n[server]\nport = 80\n")
        idx = doc["idx"]
        with pytest.raises(TypeError, match="strings, not integers"):
            doc["server"][idx] = "test"  # type: ignore[index]  # ty: ignore[invalid-assignment]

    def test_delitem_table_with_int_proxy_gives_key_error(self) -> None:
        doc = Document.parse("idx = 1\n[server]\nport = 80\n")
        idx = doc["idx"]
        with pytest.raises(KeyError):
            del doc["server"][idx]  # type: ignore[arg-type]  # ty: ignore[invalid-argument-type]


# ---------------------------------------------------------------------------
# Atomicity: a bad element must not leave the collection partially mutated
# ---------------------------------------------------------------------------


class TestAtomicMutation:
    """Conversion errors must be raised *before* any mutation happens.

    Each test supplies a mix of valid and invalid values so that without
    up-front validation the collection would be left in a half-modified state.
    """

    # -- array-of-tables --

    def test_aot_setitem_slice_contiguous_rolls_back(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
            [[items]]
            name = "c"
        """)
        )
        with pytest.raises(TypeError, match="cannot append"):
            doc["items"][0:2] = [{"name": "x"}, 42]
        assert len(doc["items"]) == 3
        assert doc["items"][0]["name"] == "a"
        assert doc["items"][1]["name"] == "b"

    def test_aot_setitem_slice_extended_rolls_back(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
            [[items]]
            name = "c"
        """)
        )
        with pytest.raises(TypeError, match="cannot append"):
            doc["items"][0:3:2] = [{"name": "x"}, 42]
        assert len(doc["items"]) == 3
        assert doc["items"][0]["name"] == "a"
        assert doc["items"][2]["name"] == "c"

    def test_aot_extend_rolls_back(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        with pytest.raises(TypeError, match="cannot append"):
            doc["items"].extend([{"name": "b"}, 42])
        assert len(doc["items"]) == 1
        assert doc["items"][0]["name"] == "a"

    # -- regular arrays --

    def test_array_setitem_slice_contiguous_rolls_back(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(TypeError, match="not a valid TOML value"):
            doc["arr"][0:2] = [10, None]
        assert list(doc["arr"]) == [1, 2, 3]

    def test_array_setitem_slice_extended_rolls_back(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(TypeError, match="not a valid TOML value"):
            doc["arr"][0:3:2] = [10, None]
        assert list(doc["arr"]) == [1, 2, 3]

    def test_array_extend_rolls_back(self) -> None:
        doc = Document.parse("arr = [1]\n")
        with pytest.raises(TypeError, match="not a valid TOML value"):
            doc["arr"].extend([2, None])
        assert list(doc["arr"]) == [1]


class TestAotTypeValidation:
    """Assigning a non-table to an array-of-tables index should raise TypeError."""

    def test_aot_setitem_rejects_scalar(self) -> None:
        doc = Document.parse("[[items]]\na = 1\n")
        with pytest.raises(TypeError):
            doc["items"][0] = 42

    def test_aot_setitem_rejects_array(self) -> None:
        doc = Document.parse("[[items]]\na = 1\n")
        with pytest.raises(TypeError):
            doc["items"][0] = [1, 2, 3]

    def test_aot_setitem_accepts_table(self) -> None:
        """Sanity: assigning a table should still work."""
        doc = Document.parse("[[items]]\na = 1\n")
        doc["items"][0] = {"b": 2}
        assert doc["items"][0]["b"] == 2


class TestDictProxyPopDoubleBorrow:
    """DictProxy.pop with a proxy key must not panic from double borrow."""

    def test_pop_with_proxy_str_key(self) -> None:
        doc = Document.parse('key = "port"\n[server]\nport = 80\n')
        key = doc["key"]
        result = doc["server"].pop(key)
        assert result == 80
        assert "port" not in doc["server"]

    def test_document_delitem_with_proxy_key(self) -> None:
        doc = Document.parse('key = "b"\na = 1\nb = 2\n')
        key = doc["key"]
        del doc[key]  # type: ignore[arg-type]  # ty: ignore[invalid-argument-type]
        assert "b" not in doc

    def test_document_pop_with_proxy_key(self) -> None:
        doc = Document.parse('key = "b"\na = 1\nb = 2\n')
        key = doc["key"]
        result = doc.pop(key)  # type: ignore[call-overload]  # ty: ignore[invalid-argument-type]
        assert result == 2
        assert "b" not in doc


class TestPopNonStringKey:
    """pop() with non-string keys should behave like dict, not raise TypeError."""

    def test_document_pop_int_key_with_default(self) -> None:
        doc = Document({"a": 1})
        assert doc.pop(42, "fallback") == "fallback"  # type: ignore[call-overload]  # ty: ignore[no-matching-overload]

    def test_document_pop_int_key_no_default(self) -> None:
        doc = Document({"a": 1})
        with pytest.raises(KeyError):
            doc.pop(42)  # type: ignore[call-overload]  # ty: ignore[invalid-argument-type]

    def test_dict_proxy_pop_int_key_with_default(self) -> None:
        doc = Document.parse("[s]\na = 1\n")
        assert doc["s"].pop(42, "nope") == "nope"

    def test_dict_proxy_pop_int_key_no_default(self) -> None:
        doc = Document.parse("[s]\na = 1\n")
        with pytest.raises(KeyError):
            doc["s"].pop(42)


class TestNonStringKeyErrors:
    """Document __getitem__/__delitem__ raise KeyError for non-string keys."""

    def test_document_getitem_int_raises_key_error(self) -> None:
        doc = Document({"a": 1})
        with pytest.raises(KeyError):
            doc[42]  # type: ignore[index]  # ty: ignore[invalid-argument-type]

    def test_document_delitem_int_raises_key_error(self) -> None:
        doc = Document({"a": 1})
        with pytest.raises(KeyError):
            del doc[42]  # type: ignore[arg-type]  # ty: ignore[invalid-argument-type]

    def test_document_setitem_int_raises_type_error(self) -> None:
        doc = Document({"a": 1})
        with pytest.raises(TypeError, match="keys must be strings"):
            doc[42] = "x"  # type: ignore[index]  # ty: ignore[invalid-assignment]
