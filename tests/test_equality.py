"""Tests for equality semantics."""

from __future__ import annotations

from datetime import date, datetime, time, timedelta, timezone

from tomledit import Document

# ---------------------------------------------------------------------------
# Equality semantics
# ---------------------------------------------------------------------------


class TestEquality:
    def test_int_eq(self, doc: Document) -> None:
        assert doc["owner"]["age"] == 30
        assert doc["owner"]["age"] != 31

    def test_str_eq(self, doc: Document) -> None:
        assert doc["owner"]["name"] == "Alice"
        assert doc["owner"]["name"] != "Bob"

    def test_bool_eq(self, doc: Document) -> None:
        assert doc["owner"]["active"] == True  # noqa: E712
        assert doc["owner"]["active"] != False  # noqa: E712

    def test_float_eq(self) -> None:
        doc = Document.parse("val = 2.5\n")
        assert doc["val"] == 2.5

    def test_type_mismatch_not_equal(self, doc: Document) -> None:
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

    def test_reverse_equality(self, doc: Document) -> None:
        """Python falls back to proxy's __eq__ in both directions."""
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


# ---------------------------------------------------------------------------
# Proxy-vs-proxy structural equality for various types
# ---------------------------------------------------------------------------


class TestProxyStructuralEquality:
    """Exercise values_structural_eq for types beyond integers."""

    def test_proxy_vs_proxy_bool(self) -> None:
        doc = Document.parse("a = true\nb = true\n")
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_bool_different(self) -> None:
        doc = Document.parse("a = true\nb = false\n")
        assert doc["a"] != doc["b"]

    def test_proxy_vs_proxy_float(self) -> None:
        doc = Document.parse("a = 1.5\nb = 1.5\n")
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_float_different(self) -> None:
        doc = Document.parse("a = 1.5\nb = 2.5\n")
        assert doc["a"] != doc["b"]

    def test_proxy_vs_proxy_string(self) -> None:
        doc = Document.parse('a = "hi"\nb = "hi"\n')
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_string_different(self) -> None:
        doc = Document.parse('a = "hi"\nb = "bye"\n')
        assert doc["a"] != doc["b"]

    def test_proxy_vs_proxy_datetime(self) -> None:
        doc = Document.parse(
            "a = 2024-01-15T10:30:00Z\nb = 2024-01-15T10:30:00+00:00\n"
        )
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_datetime_different(self) -> None:
        doc = Document.parse("a = 2024-01-15T10:30:00Z\nb = 2025-01-01T00:00:00Z\n")
        assert doc["a"] != doc["b"]

    def test_proxy_vs_proxy_date_only(self) -> None:
        doc = Document.parse("a = 2024-01-15\nb = 2024-01-15\n")
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_date_only_different(self) -> None:
        doc = Document.parse("a = 2024-01-15\nb = 2025-06-01\n")
        assert doc["a"] != doc["b"]

    def test_proxy_vs_proxy_array(self) -> None:
        doc = Document.parse("a = [1, 2, 3]\nb = [1, 2, 3]\n")
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_array_different(self) -> None:
        doc = Document.parse("a = [1, 2, 3]\nb = [1, 2, 4]\n")
        assert doc["a"] != doc["b"]

    def test_proxy_vs_proxy_inline_table(self) -> None:
        doc = Document.parse("a = {x = 1}\nb = {x = 1}\n")
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_inline_table_different(self) -> None:
        doc = Document.parse("a = {x = 1}\nb = {x = 2}\n")
        assert doc["a"] != doc["b"]

    def test_proxy_vs_proxy_array_of_tables(self) -> None:
        toml = "[[a]]\nx = 1\n[[a]]\nx = 2\n[[b]]\nx = 1\n[[b]]\nx = 2\n"
        doc = Document.parse(toml)
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_array_of_tables_different(self) -> None:
        toml = "[[a]]\nx = 1\n[[b]]\nx = 2\n"
        doc = Document.parse(toml)
        assert doc["a"] != doc["b"]

    def test_proxy_vs_proxy_different_types(self) -> None:
        """Table vs ArrayOfTables should not be equal."""
        doc = Document.parse("[a]\nx = 1\n[[b]]\nx = 1\n")
        assert doc["a"] != doc["b"]


# ---------------------------------------------------------------------------
# Equality edge cases
# ---------------------------------------------------------------------------


class TestEqualityEdgeCases:
    def test_table_extra_key_not_equal(self) -> None:
        """Table with keys {a, b} should != dict with keys {a, c}."""
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        assert doc["t"] != {"a": 1, "c": 2}

    def test_table_vs_non_dict(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        assert doc["t"] != [1, 2]
        assert doc["t"] != "hello"
        assert doc["t"] != 42

    def test_inline_table_length_mismatch(self) -> None:
        doc = Document.parse("t = {a = 1, b = 2}\n")
        assert doc["t"] != {"a": 1}

    def test_inline_table_key_mismatch(self) -> None:
        doc = Document.parse("t = {a = 1, b = 2}\n")
        assert doc["t"] != {"a": 1, "c": 2}

    def test_inline_table_value_mismatch(self) -> None:
        doc = Document.parse("t = {a = 1, b = 2}\n")
        assert doc["t"] != {"a": 1, "b": 99}

    def test_inline_table_vs_non_dict(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        assert doc["t"] != [1]

    def test_aot_equality_with_list_of_dicts(self) -> None:
        doc = Document.parse(
            '[[items]]\nname = "a"\nvalue = 1\n[[items]]\nname = "b"\nvalue = 2\n'
        )
        expected = [{"name": "a", "value": 1}, {"name": "b", "value": 2}]
        assert doc["items"] == expected

    def test_aot_inequality_with_wrong_list(self) -> None:
        doc = Document.parse(
            '[[items]]\nname = "a"\nvalue = 1\n[[items]]\nname = "b"\nvalue = 2\n'
        )
        assert doc["items"] != [{"name": "a", "value": 1}]

    def test_aot_inequality_with_non_list(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\nvalue = 1\n')
        assert doc["items"] != "not a list"

    def test_aot_entry_value_mismatch(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n[[items]]\nname = "b"\n')
        assert doc["items"] != [{"name": "a"}, {"name": "WRONG"}]

    def test_aot_entry_non_dict_in_list(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n')
        assert doc["items"] != ["not a dict"]

    def test_float_not_equal_to_string(self) -> None:
        doc = Document.parse("val = 1.5\n")
        assert doc["val"] != "1.5"

    def test_datetime_not_equal_to_string(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00Z\n")
        assert doc["dt"] != "2024-01-15"

    def test_date_only_not_equal_to_string(self) -> None:
        doc = Document.parse("d = 2024-01-15\n")
        assert doc["d"] != "2024-01-15"

    def test_time_only_not_equal_to_int(self) -> None:
        doc = Document.parse("t = 10:30:00\n")
        assert doc["t"] != 1030

    def test_full_datetime_not_equal_to_date(self) -> None:
        """A TOML datetime (with time component) should not equal a Python date."""
        doc = Document.parse("dt = 2024-01-15T10:30:00\n")
        assert doc["dt"] != date(2024, 1, 15)

    def test_full_datetime_not_equal_to_time(self) -> None:
        """A TOML datetime (with date component) should not equal a Python time."""
        doc = Document.parse("dt = 2024-01-15T10:30:00\n")
        assert doc["dt"] != time(10, 30, 0)

    def test_array_not_equal_to_string(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        assert doc["arr"] != "not a list"

    def test_bool_not_equal_to_string(self) -> None:
        doc = Document.parse("flag = true\n")
        assert doc["flag"] != "true"

    def test_date_only_equals_python_date(self) -> None:
        doc = Document.parse("d = 2024-01-15\n")
        assert doc["d"] == date(2024, 1, 15)

    def test_date_only_not_equal_to_different_date(self) -> None:
        doc = Document.parse("d = 2024-01-15\n")
        assert doc["d"] != date(2025, 6, 1)

    def test_time_only_equals_python_time(self) -> None:
        doc = Document.parse("t = 10:30:00\n")
        assert doc["t"] == time(10, 30, 0)

    def test_time_only_not_equal_to_different_time(self) -> None:
        doc = Document.parse("t = 10:30:00\n")
        assert doc["t"] != time(11, 0, 0)

    def test_reverse_date_equality(self) -> None:
        doc = Document.parse("d = 2024-01-15\n")
        assert date(2024, 1, 15) == doc["d"]

    def test_reverse_time_equality(self) -> None:
        doc = Document.parse("t = 10:30:00\n")
        assert time(10, 30, 0) == doc["t"]

    def test_datetime_equality_with_non_utc_offset(self) -> None:
        """Proxy datetime == Python datetime with matching non-UTC offset."""
        doc = Document.parse("dt = 2024-01-15T10:30:00+05:30\n")
        tz = timezone(timedelta(hours=5, minutes=30))
        expected = datetime(2024, 1, 15, 10, 30, 0, tzinfo=tz)
        assert doc["dt"] == expected


class TestNumericTowerEquality:
    """Python's numeric tower: 1 == 1.0 == True.

    TOML integer and float proxies should follow Python semantics when compared
    to Python objects, even across numeric types.
    """

    def test_int_proxy_eq_python_float(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc["x"] == 1.0

    def test_int_proxy_eq_python_float_zero(self) -> None:
        doc = Document.parse("x = 0\n")
        assert doc["x"] == 0.0

    def test_float_proxy_eq_python_int(self) -> None:
        doc = Document.parse("x = 1.0\n")
        assert doc["x"] == 1

    def test_float_proxy_not_equal_to_python_int(self) -> None:
        doc = Document.parse("x = 1.5\n")
        assert doc["x"] != 1

    def test_float_in_int_array(self) -> None:
        """1.0 in [1, 2, 3] is True in Python."""
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert 1.0 in doc["arr"]

    def test_int_in_float_array(self) -> None:
        """1 in [1.0, 2.0] is True in Python."""
        doc = Document.parse("arr = [1.0, 2.0]\n")
        assert 1 in doc["arr"]

    def test_cross_type_proxy_eq(self) -> None:
        """Two proxies: int 1 vs float 1.0 — structural eq stays strict."""
        doc = Document.parse("x = 1\ny = 1.0\n")
        # Proxy-to-proxy uses TOML structural equality (type-aware)
        assert doc["x"] != doc["y"]
