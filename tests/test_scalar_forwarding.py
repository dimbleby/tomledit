"""Tests for ScalarItem attribute and dunder forwarding."""

from __future__ import annotations

from datetime import date, time

import pytest

from tomledit import Document

SAMPLE = """\
name = "Alice"
port = 8080
rate = 3.14
enabled = true
birthday = 1990-05-15
alarm = 07:30:00
created = 1990-05-15T10:30:00Z
"""


@pytest.fixture
def doc() -> Document:
    return Document.parse(SAMPLE)


class TestStringForwarding:
    def test_upper(self, doc: Document) -> None:
        assert doc["name"].upper() == "ALICE"

    def test_lower(self, doc: Document) -> None:
        assert doc["name"].lower() == "alice"

    def test_startswith(self, doc: Document) -> None:
        assert doc["name"].startswith("Ali")
        assert not doc["name"].startswith("Bob")

    def test_endswith(self, doc: Document) -> None:
        assert doc["name"].endswith("ice")

    def test_replace(self, doc: Document) -> None:
        assert doc["name"].replace("Alice", "Bob") == "Bob"

    def test_split(self, doc: Document) -> None:
        assert doc["name"].split("l") == ["A", "ice"]

    def test_strip(self) -> None:
        doc2 = Document.parse('padded = "  hello  "')
        assert doc2["padded"].strip() == "hello"

    def test_title(self) -> None:
        doc2 = Document.parse('msg = "hello world"')
        assert doc2["msg"].title() == "Hello World"

    def test_isalpha(self, doc: Document) -> None:
        assert doc["name"].isalpha()


class TestIntForwarding:
    def test_bit_length(self, doc: Document) -> None:
        assert doc["port"].bit_length() == 13  # 8080 needs 13 bits

    def test_to_bytes(self, doc: Document) -> None:
        assert doc["port"].to_bytes(2, "big") == (8080).to_bytes(2, "big")


class TestFloatForwarding:
    def test_is_integer(self, doc: Document) -> None:
        assert not doc["rate"].is_integer()

    def test_as_integer_ratio(self, doc: Document) -> None:
        assert doc["rate"].as_integer_ratio() == (3.14).as_integer_ratio()

    def test_hex(self, doc: Document) -> None:
        assert doc["rate"].hex() == (3.14).hex()


class TestBoolForwarding:
    def test_bit_length(self, doc: Document) -> None:
        assert doc["enabled"].bit_length() == 1


class TestDatetimeForwarding:
    def test_date_isoformat(self, doc: Document) -> None:
        assert doc["birthday"].isoformat() == "1990-05-15"

    def test_time_isoformat(self, doc: Document) -> None:
        assert doc["alarm"].isoformat() == "07:30:00"

    def test_datetime_isoformat(self, doc: Document) -> None:
        assert doc["created"].isoformat() == "1990-05-15T10:30:00+00:00"

    def test_date_year(self, doc: Document) -> None:
        assert doc["birthday"].year == 1990

    def test_date_month(self, doc: Document) -> None:
        assert doc["birthday"].month == 5

    def test_time_hour(self, doc: Document) -> None:
        assert doc["alarm"].hour == 7

    def test_datetime_date(self, doc: Document) -> None:
        assert doc["created"].date() == date(1990, 5, 15)

    def test_datetime_time(self, doc: Document) -> None:
        assert doc["created"].time() == time(10, 30)


class TestExistingAttrsNotShadowed:
    """Ensure Item-level attributes are not forwarded."""

    def test_value_property(self, doc: Document) -> None:
        assert doc["name"].value == "Alice"
        assert doc["port"].value == 8080

    def test_comment_property(self, doc: Document) -> None:
        assert doc["name"].comment is None

    def test_inline_comment_property(self, doc: Document) -> None:
        assert doc["name"].inline_comment is None

    def test_eq(self, doc: Document) -> None:
        assert doc["name"] == "Alice"
        assert doc["port"] == 8080

    def test_bool(self, doc: Document) -> None:
        assert bool(doc["name"]) is True


class TestErrorCases:
    def test_nonexistent_attr(self, doc: Document) -> None:
        with pytest.raises(AttributeError, match=r"ScalarItem.*str.*nonexistent"):
            _ = doc["name"].nonexistent

    def test_wrong_type_method(self, doc: Document) -> None:
        with pytest.raises(AttributeError, match=r"ScalarItem.*int.*upper"):
            doc["port"].upper()

    def test_stale_proxy(self, doc: Document) -> None:
        name = doc["name"]
        doc["name"] = "Bob"
        with pytest.raises(RuntimeError, match="stale"):
            name.upper()


class TestDictListNotAffected:
    """DictItem and ListItem should not gain forwarding."""

    def test_dict_no_forwarding(self) -> None:
        doc = Document.parse("[server]\nhost = 'x'\n")
        with pytest.raises(AttributeError):
            doc["server"].upper()

    def test_list_no_forwarding(self) -> None:
        doc = Document.parse("ports = [1, 2, 3]\n")
        with pytest.raises(AttributeError):
            doc["ports"].upper()


# -----------------------------------------------------------------------
# Dunder method forwarding
# -----------------------------------------------------------------------


class TestArithmetic:
    """Binary and unary arithmetic forwarded to the underlying value."""

    def test_add_int(self, doc: Document) -> None:
        assert doc["port"] + 1 == 8081

    def test_radd_int(self, doc: Document) -> None:
        assert 1 + doc["port"] == 8081

    def test_add_str(self, doc: Document) -> None:
        assert doc["name"] + " Smith" == "Alice Smith"

    def test_radd_str(self, doc: Document) -> None:
        assert "Hello " + doc["name"] == "Hello Alice"

    def test_sub(self, doc: Document) -> None:
        assert doc["port"] - 80 == 8000

    def test_rsub(self, doc: Document) -> None:
        assert 10000 - doc["port"] == 1920

    def test_mul_int(self) -> None:
        doc = Document.parse("n = 6")
        assert doc["n"] * 7 == 42

    def test_rmul_int(self) -> None:
        doc = Document.parse("n = 6")
        assert 7 * doc["n"] == 42

    def test_mul_str(self) -> None:
        doc = Document.parse('ch = "ab"')
        assert doc["ch"] * 3 == "ababab"

    def test_rmul_str(self) -> None:
        doc = Document.parse('ch = "ab"')
        assert 3 * doc["ch"] == "ababab"

    def test_truediv(self, doc: Document) -> None:
        assert doc["port"] / 2 == 4040.0

    def test_rtruediv(self) -> None:
        doc = Document.parse("n = 4")
        assert 20 / doc["n"] == 5.0

    def test_floordiv(self, doc: Document) -> None:
        assert doc["port"] // 3 == 2693

    def test_rfloordiv(self, doc: Document) -> None:
        assert 100000 // doc["port"] == 12

    def test_mod(self, doc: Document) -> None:
        assert doc["port"] % 1000 == 80

    def test_rmod(self, doc: Document) -> None:
        assert 10000 % doc["port"] == 1920

    def test_mod_str_format(self) -> None:
        doc = Document.parse('fmt = "hello %s"')
        assert doc["fmt"] % "world" == "hello world"

    def test_pow(self) -> None:
        doc = Document.parse("n = 3")
        assert doc["n"] ** 4 == 81

    def test_rpow(self) -> None:
        doc = Document.parse("n = 3")
        assert 2 ** doc["n"] == 8

    def test_rpow_with_modulo(self) -> None:
        doc = Document.parse("x = 3")
        assert pow(2, doc["x"], 5) == 3  # type: ignore[misc]  # 2**3 % 5 = 3

    def test_pow_modulo(self) -> None:
        doc = Document.parse("n = 5")
        assert pow(doc["n"], 3, 7) == pow(5, 3, 7)

    def test_neg(self, doc: Document) -> None:
        assert -doc["port"] == -8080

    def test_pos(self, doc: Document) -> None:
        assert +doc["port"] == 8080

    def test_abs(self) -> None:
        doc = Document.parse("n = -42")
        assert abs(doc["n"]) == 42

    def test_invert(self) -> None:
        doc = Document.parse("n = 0")
        assert ~doc["n"] == -1

    def test_add_float(self, doc: Document) -> None:
        assert doc["rate"] + 1.0 == pytest.approx(4.14)

    def test_sub_float(self, doc: Document) -> None:
        assert doc["rate"] - 0.14 == pytest.approx(3.0)


class TestAugmentedAssignment:
    """Test += and friends on subscript targets."""

    def test_iadd_str(self) -> None:
        doc = Document.parse('name = "Alice"')
        doc["name"] += " Smith"
        assert doc["name"] == "Alice Smith"

    def test_iadd_int(self) -> None:
        doc = Document.parse("port = 8080")
        doc["port"] += 1
        assert doc["port"] == 8081

    def test_isub_int(self) -> None:
        doc = Document.parse("port = 8080")
        doc["port"] -= 80
        assert doc["port"] == 8000

    def test_imul_int(self) -> None:
        doc = Document.parse("n = 6")
        doc["n"] *= 7
        assert doc["n"] == 42

    def test_imul_str(self) -> None:
        doc = Document.parse('s = "ab"')
        doc["s"] *= 3
        assert doc["s"] == "ababab"


class TestComparison:
    def test_lt(self, doc: Document) -> None:
        assert doc["port"] < 9000
        assert not doc["port"] < 8080

    def test_le(self, doc: Document) -> None:
        assert doc["port"] <= 8080
        assert doc["port"] <= 9000

    def test_gt(self, doc: Document) -> None:
        assert doc["port"] > 80
        assert not doc["port"] > 8080

    def test_ge(self, doc: Document) -> None:
        assert doc["port"] >= 8080
        assert doc["port"] >= 80

    def test_str_comparison(self, doc: Document) -> None:
        assert doc["name"] < "Bob"
        assert doc["name"] > "ALICE"

    def test_float_comparison(self, doc: Document) -> None:
        assert doc["rate"] < 4.0
        assert doc["rate"] > 3.0

    def test_eq_still_works(self, doc: Document) -> None:
        assert doc["port"] == 8080
        assert doc["name"] == "Alice"
        assert doc["port"] != 9999

    def test_ne_still_works(self, doc: Document) -> None:
        assert doc["port"] != 9999
        assert doc["port"] == 8080

    def test_cross_scalar_comparison(self) -> None:
        """Comparing two ScalarItems."""
        doc = Document.parse("a = 1\nb = 2\n")
        assert doc["a"] < doc["b"]
        assert doc["b"] > doc["a"]


class TestTypeConversion:
    def test_int_from_int(self, doc: Document) -> None:
        assert int(doc["port"]) == 8080

    def test_int_from_float(self, doc: Document) -> None:
        assert int(doc["rate"]) == 3

    def test_int_from_bool(self, doc: Document) -> None:
        assert int(doc["enabled"]) == 1

    def test_float_from_float(self, doc: Document) -> None:
        assert float(doc["rate"]) == 3.14

    def test_float_from_int(self, doc: Document) -> None:
        assert float(doc["port"]) == 8080.0

    def test_index(self) -> None:
        """__index__ allows using ScalarItem as a list index."""
        items = ["a", "b", "c"]
        doc = Document.parse("i = 1")
        assert items[doc["i"]] == "b"

    def test_hash(self, doc: Document) -> None:
        assert hash(doc["name"]) == hash("Alice")
        assert hash(doc["port"]) == hash(8080)

    def test_hashable_in_set(self, doc: Document) -> None:
        s = {doc["name"], doc["port"]}
        assert "Alice" in s or doc["name"] in s

    def test_hashable_as_dict_key(self, doc: Document) -> None:
        port = doc["port"]
        d = {port: "web"}
        assert d[port] == "web"


class TestFormatting:
    def test_format_str(self, doc: Document) -> None:
        assert f"{doc['name']:>10}" == "     Alice"

    def test_format_int(self, doc: Document) -> None:
        assert f"{doc['port']:06d}" == "008080"

    def test_format_float(self, doc: Document) -> None:
        assert f"{doc['rate']:.1f}" == "3.1"

    def test_format_empty_spec(self, doc: Document) -> None:
        assert f"{doc['name']}" == "Alice"
