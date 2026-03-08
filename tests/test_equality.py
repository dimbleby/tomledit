"""Tests for equality semantics and string representation."""

from __future__ import annotations

from datetime import datetime, timezone

from tests.conftest import SAMPLE, make_doc
from tomledit import Document

# ---------------------------------------------------------------------------
# String representation (__str__)
# ---------------------------------------------------------------------------


class TestStr:
    def test_document_roundtrip(self) -> None:
        doc = Document.parse(SAMPLE)
        assert str(doc) == SAMPLE

    def test_proxy_str_scalar(self) -> None:
        doc = Document.parse("x = 42\n")
        assert str(doc["x"]) == "42"

    def test_proxy_str_string(self) -> None:
        doc = Document.parse('name = "hello"\n')
        assert str(doc["name"]) == "hello"

    def test_proxy_str_int(self) -> None:
        doc = make_doc()
        assert str(doc["owner"]["age"]) == "30"

    def test_proxy_str_after_mutation(self) -> None:
        doc = make_doc()
        doc["owner"]["age"] = 99
        assert str(doc["owner"]["age"]) == "99"


# ---------------------------------------------------------------------------
# Equality semantics
# ---------------------------------------------------------------------------


class TestEquality:
    def test_int_eq(self) -> None:
        doc = make_doc()
        assert doc["owner"]["age"] == 30
        assert doc["owner"]["age"] != 31

    def test_str_eq(self) -> None:
        doc = make_doc()
        assert doc["owner"]["name"] == "Alice"
        assert doc["owner"]["name"] != "Bob"

    def test_bool_eq(self) -> None:
        doc = make_doc()
        assert doc["owner"]["active"] == True  # noqa: E712
        assert doc["owner"]["active"] != False  # noqa: E712

    def test_float_eq(self) -> None:
        doc = Document.parse("val = 2.5\n")
        assert doc["val"] == 2.5

    def test_type_mismatch_not_equal(self) -> None:
        doc = make_doc()
        assert doc["owner"]["age"] != "30"
        assert doc["owner"]["name"] != 42

    def test_datetime_proxy_eq(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00Z\n")
        expected = datetime(2024, 1, 15, 10, 30, 0, tzinfo=timezone.utc)
        assert doc["dt"] == expected

    def test_datetime_ne(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00Z\n")
        wrong = datetime(2000, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
        assert doc["dt"] != wrong

    def test_array_equals_list(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"] == [1, 2, 3]

    def test_array_not_equals_different_list(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"] != [1, 2, 4]

    def test_array_not_equals_different_length(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"] != [1, 2]

    def test_empty_array_equals_empty_list(self) -> None:
        doc = Document.parse("arr = []\n")
        assert doc["arr"] == []

    def test_table_equals_dict(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        assert doc["t"] == {"a": 1, "b": 2}

    def test_table_not_equals_different_dict(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        assert doc["t"] != {"a": 2}

    def test_inline_table_equals_dict(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        assert doc["meta"] == {"x": 1, "y": 2}

    def test_nested_array_equals_nested_list(self) -> None:
        doc = Document.parse("arr = [[1, 2], [3, 4]]\n")
        assert doc["arr"] == [[1, 2], [3, 4]]

    def test_string_array_equals_list(self) -> None:
        doc = Document.parse('arr = ["a", "b"]\n')
        assert doc["arr"] == ["a", "b"]

    def test_reverse_equality(self) -> None:
        """Python falls back to proxy's __eq__ in both directions."""
        doc = make_doc()
        assert 30 == doc["owner"]["age"]  # noqa: SIM300
        assert [8001, 8001, 8002] == doc["database"]["ports"]  # noqa: SIM300

    def test_bool_not_equal_to_int(self) -> None:
        """TOML types are strict: bool != int, even though Python's True == 1."""
        doc = Document.parse("count = 1\nflag = true\n")
        assert doc["count"] != True  # noqa: E712
        assert doc["flag"] != 1

    def test_bool_not_equal_to_float(self) -> None:
        doc = Document.parse("val = 1.0\n")
        assert doc["val"] != True  # noqa: E712

    def test_proxy_vs_proxy_same_value(self) -> None:
        """Two proxies pointing at equal values should compare equal."""
        doc = Document.parse("a = 42\nb = 42\n")
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_different_value(self) -> None:
        doc = Document.parse("a = 42\nb = 99\n")
        assert doc["a"] != doc["b"]

    def test_proxy_self_equality(self) -> None:
        doc = Document.parse("a = 42\n")
        assert doc["a"] == doc["a"]

    def test_proxy_vs_proxy_nested(self) -> None:
        doc = Document.parse("[t1]\nx = 1\n\n[t2]\nx = 1\n")
        assert doc["t1"] == doc["t2"]

    def test_proxy_vs_proxy_type_strict(self) -> None:
        """Proxy-vs-proxy should preserve TOML type strictness."""
        doc = Document.parse("count = 1\nflag = true\n")
        assert doc["count"] != doc["flag"]
