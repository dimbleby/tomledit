"""Tests for stale-proxy detection."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from tests.conftest import toml_literal
from tomledit import Document

if TYPE_CHECKING:
    from tomledit import Item


class TestStaleProxyDetection:
    """Proxies created before a mutation raise RuntimeError on access."""

    def test_sibling_valid_after_delitem_on_doc(self) -> None:
        doc = Document.parse(
            toml_literal("""
            x = 1
            y = 2
        """)
        )
        proxy = doc["y"]
        del doc["x"]
        assert proxy.value == 2

    def test_stale_after_pop_on_doc(self) -> None:
        doc = Document.parse(
            toml_literal("""
            x = 1
            y = 2
        """)
        )
        proxy = doc["x"]
        doc.pop("x")
        with pytest.raises(RuntimeError, match="stale"):
            _ = proxy.value

    def test_stale_after_popitem_on_doc(self) -> None:
        doc = Document.parse("only = 1")
        proxy = doc["only"]
        doc.popitem()
        with pytest.raises(RuntimeError, match="stale"):
            _ = proxy.value

    def test_sibling_valid_after_pop_on_doc(self) -> None:
        doc = Document.parse(
            toml_literal("""
            x = 1
            y = 2
        """)
        )
        proxy = doc["y"]
        doc.pop("x")
        assert proxy.value == 2

    def test_valid_after_additive_update_on_doc(self) -> None:
        doc = Document.parse("x = 1")
        proxy = doc["x"]
        doc.update({"y": 2})
        assert proxy.value == 1

    def test_stale_after_replacing_update_on_doc(self) -> None:
        doc = Document.parse("x = 1")
        proxy = doc["x"]
        doc.update({"x": 2})
        with pytest.raises(RuntimeError, match="stale"):
            _ = proxy.value

    def test_valid_after_setdefault_new_key_on_doc(self) -> None:
        """setdefault with a new key doesn't replace anything — no paths break."""
        doc = Document.parse("x = 1")
        proxy = doc["x"]
        doc.setdefault("y", 2)
        assert proxy.value == 1

    def test_valid_after_setdefault_existing_key(self) -> None:
        doc = Document.parse("x = 1")
        proxy = doc["x"]
        doc.setdefault("x", 99)  # no-op, key exists
        assert proxy.value == 1  # should NOT raise


class TestStaleProxyViaProxy:
    """Mutations through a proxy invalidate sibling proxies."""

    def test_sibling_proxy_valid_after_delitem(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        b = doc["t"]["b"]
        t = doc["t"]
        del t["a"]
        assert b.value == 2

    def test_valid_after_array_append(self) -> None:
        """append doesn't invalidate siblings — no paths break."""
        doc = Document.parse("arr = [1, 2]")
        item = doc["arr"][0]
        arr = doc["arr"]
        arr.append(3)
        assert item.value == 1
        assert arr.value == [1, 2, 3]

    def test_valid_after_append_with_negative_index_proxy(self) -> None:
        """Negative indices are resolved at lookup time, so append can't shift them."""
        doc = Document.parse("arr = [1, 2, 3]")
        last = doc["arr"][-1]
        assert last.value == 3
        doc["arr"].append(4)
        assert last.value == 3  # still index 2, not shifted to 4

    def test_valid_after_array_extend(self) -> None:
        """extend doesn't invalidate siblings — no paths break."""
        doc = Document.parse("arr = [1]")
        item = doc["arr"][0]
        arr = doc["arr"]
        arr.extend([2, 3])
        assert item.value == 1
        assert arr.value == [1, 2, 3]

    def test_stale_after_proxy_clear(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        a = doc["t"]["a"]
        t = doc["t"]
        t.clear()
        with pytest.raises(RuntimeError, match="stale"):
            _ = a.value

    def test_stale_after_imul_zero(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        arr = doc["arr"]
        elem = arr[0]
        arr *= 0
        with pytest.raises(RuntimeError, match="stale"):
            _ = elem.value

    def test_stale_after_imul_negative(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        arr = doc["arr"]
        elem = arr[1]
        arr *= -5
        with pytest.raises(RuntimeError, match="stale"):
            _ = elem.value

    def test_imul_positive_preserves_existing_proxies(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        arr = doc["arr"]
        elem = arr[0]
        arr *= 3
        assert elem.value == 1  # index 0 is untouched

    def test_stale_after_proxy_pop(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        a = doc["t"]["a"]
        t = doc["t"]
        t.pop("a")
        with pytest.raises(RuntimeError, match="stale"):
            _ = a.value

    def test_stale_after_proxy_popitem(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            only = 1
        """)
        )
        only = doc["t"]["only"]
        t = doc["t"]
        t.popitem()
        with pytest.raises(RuntimeError, match="stale"):
            _ = only.value

    def test_valid_after_additive_proxy_update(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        a = doc["t"]["a"]
        t = doc["t"]
        t.update({"b": 2})
        assert a.value == 1


class TestMutatorProxyStaysValid:
    """The proxy that performs a mutation stays valid afterward."""

    def test_setitem_through_proxy(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        t = doc["t"]
        t["a"] = 99
        assert t["a"] == 99  # mutating proxy is still valid

    def test_delitem_through_proxy(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        t = doc["t"]
        del t["a"]
        assert t["b"] == 2

    def test_append_through_proxy(self) -> None:
        doc = Document.parse("arr = [1]")
        arr = doc["arr"]
        arr.append(2)
        assert len(arr) == 2

    def test_pop_through_proxy(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        arr = doc["arr"]
        arr.pop()
        assert len(arr) == 2

    def test_clear_through_proxy(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        t = doc["t"]
        t.clear()
        assert len(t) == 0

    def test_update_through_proxy(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        t = doc["t"]
        t.update({"b": 2})
        assert t["b"] == 2

    def test_setdefault_through_proxy(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        t = doc["t"]
        t.setdefault("b", 42)
        assert t["b"] == 42

    def test_insert_through_proxy(self) -> None:
        doc = Document.parse("arr = [1, 3]")
        arr = doc["arr"]
        arr.insert(1, 2)
        assert len(arr) == 3

    def test_extend_through_proxy(self) -> None:
        doc = Document.parse("arr = [1]")
        arr = doc["arr"]
        arr.extend([2, 3])
        assert len(arr) == 3

    def test_remove_through_proxy(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        arr = doc["arr"]
        arr.remove(2)
        assert len(arr) == 2


class TestStaleProxyAsMappingKey:
    """A stale ScalarItem string used as a key should still raise RuntimeError."""

    def test_document_contains_raises_for_stale_string_proxy_key(self) -> None:
        doc = Document.parse('key = "x"\nx = 1\n')
        key = doc["key"]
        del doc["key"]
        with pytest.raises(RuntimeError, match="stale"):
            assert key in doc

    def test_document_get_raises_for_stale_string_proxy_key(self) -> None:
        doc = Document.parse('key = "x"\nx = 1\n')
        key = doc["key"]
        del doc["key"]
        with pytest.raises(RuntimeError, match="stale"):
            doc.get(key)  # type: ignore[call-overload]  # ty: ignore[invalid-argument-type]

    def test_document_setitem_raises_for_stale_string_proxy_key(self) -> None:
        doc = Document.parse('key = "x"\nx = 1\n')
        key = doc["key"]
        del doc["key"]
        with pytest.raises(RuntimeError, match="stale"):
            doc[key] = 2  # type: ignore[index]  # ty: ignore[invalid-assignment]

    def test_dict_get_raises_for_stale_string_proxy_key(self) -> None:
        doc = Document.parse('key = "x"\n[t]\nx = 1\n')
        key = doc["key"]
        del doc["key"]
        with pytest.raises(RuntimeError, match="stale"):
            doc["t"].get(key)

    def test_items_view_contains_raises_for_stale_string_proxy_key(self) -> None:
        doc = Document.parse('key = "x"\n[t]\nx = 1\n')
        key = doc["key"]
        del doc["key"]
        with pytest.raises(RuntimeError, match="stale"):
            assert (key, 1) in doc["t"].items()

    def test_keys_view_set_ops_raise_for_stale_string_proxy_key(self) -> None:
        doc = Document.parse('key = "x"\nx = 1\n')
        key = doc["key"]
        del doc["key"]
        with pytest.raises(RuntimeError, match="stale"):
            doc.keys() & [key]
        with pytest.raises(RuntimeError, match="stale"):
            doc.keys() - [key]

    def test_update_iterable_of_pairs_raises_for_stale_string_proxy_key(self) -> None:
        doc = Document.parse('key = "x"\nx = 1\n')
        key = doc["key"]
        del doc["key"]
        with pytest.raises(RuntimeError, match="stale"):
            doc.update([(key, 2)])  # type: ignore[list-item]  # ty: ignore[no-matching-overload]

    def test_update_raises_for_stale_proxy_source(self) -> None:
        doc = Document.parse("[dst]\na = 1\n[src]\nb = 2\n")
        source = doc["src"]
        del doc["src"]
        with pytest.raises(RuntimeError, match="stale"):
            doc["dst"].update(source)


class TestCommentMutationsPreserveProxies:
    """Setting comments is decoration-only, so proxies stay valid."""

    def test_set_comment_does_not_invalidate_sibling(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        b = doc["b"]
        a = doc["a"]
        a.comment = "# hello"
        assert b.value == 2

    def test_set_inline_comment_does_not_invalidate_sibling(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        b = doc["b"]
        a = doc["a"]
        a.inline_comment = "# hello"
        assert b.value == 2


class TestFreshProxiesAfterMutation:
    """New proxies created after a mutation work fine."""

    def test_new_proxy_after_doc_mutation(self) -> None:
        doc = Document.parse(
            toml_literal("""
            x = 1
            y = 2
        """)
        )
        old = doc["x"]
        doc["x"] = 99
        new = doc["x"]
        assert new.value == 99
        with pytest.raises(RuntimeError, match="stale"):
            _ = old.value

    def test_new_child_after_proxy_mutation(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        t = doc["t"]
        old_a = doc["t"]["a"]
        t["a"] = 99
        new_a = t["a"]
        assert new_a.value == 99
        with pytest.raises(RuntimeError, match="stale"):
            _ = old_a.value


class TestReadMethodsCheckFreshness:
    """All read-path methods should check that the proxy is fresh."""

    @pytest.fixture
    def stale_proxy(self) -> tuple[Item, Item]:
        doc = Document.parse(
            toml_literal("""
            arr = [1, 2]

            [t]
            a = 1
            b = 2
        """)
        )
        proxy_t = doc["t"]
        proxy_arr = doc["arr"]
        doc["t"] = {"c": 3}  # replace entire table → proxy_t is stale
        doc["arr"] = [10, 20]  # replace entire array → proxy_arr is stale
        return proxy_t, proxy_arr

    def test_getitem(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            _ = t["a"]

    def test_len(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            len(t)

    def test_iter(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            iter(t)

    def test_contains(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            _ = "a" in t

    def test_bool(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            bool(t)

    def test_str(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            str(t)

    def test_repr(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            repr(t)

    def test_eq(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            _ = t == {"a": 1, "b": 2}

    def test_value(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            _ = t.value

    def test_comment(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            _ = t.comment

    def test_inline_comment(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            _ = t.inline_comment

    def test_keys(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            t.keys()

    def test_values(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            t.values()

    def test_items(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            t.items()

    def test_get(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            t.get("a")

    def test_count(self, stale_proxy: tuple[Item, Item]) -> None:
        _, arr = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            arr.count(1)

    def test_index(self, stale_proxy: tuple[Item, Item]) -> None:
        _, arr = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            arr.index(1)

    def test_fmt(self, stale_proxy: tuple[Item, Item]) -> None:
        t, _ = stale_proxy
        with pytest.raises(RuntimeError, match="stale"):
            t.fmt()


class TestPreciseInvalidation:
    """Path-based trie only invalidates proxies at or below the mutated path."""

    def test_top_level_sibling_unaffected(self) -> None:
        doc = Document.parse(
            toml_literal("""
            x = 1
            y = 2
            z = 3
        """)
        )
        px = doc["x"]
        py = doc["y"]
        pz = doc["z"]
        doc["x"] = 99
        with pytest.raises(RuntimeError, match="stale"):
            _ = px.value
        assert py.value == 2
        assert pz.value == 3

    def test_nested_sibling_unaffected(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
            c = 3
        """)
        )
        pa = doc["t"]["a"]
        pb = doc["t"]["b"]
        pc = doc["t"]["c"]
        doc["t"]["a"] = 99
        with pytest.raises(RuntimeError, match="stale"):
            _ = pa.value
        assert pb.value == 2
        assert pc.value == 3

    def test_replacing_table_invalidates_descendants(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        pa = doc["t"]["a"]
        pb = doc["t"]["b"]
        pt = doc["t"]
        doc["t"] = {"c": 3}
        with pytest.raises(RuntimeError, match="stale"):
            _ = pt.value
        with pytest.raises(RuntimeError, match="stale"):
            _ = pa.value
        with pytest.raises(RuntimeError, match="stale"):
            _ = pb.value

    def test_array_structural_change_invalidates_elements(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        e0 = doc["arr"][0]
        e1 = doc["arr"][1]
        arr = doc["arr"]
        arr.insert(0, 99)
        # array proxy itself stays valid (self-update)
        assert len(arr) == 4
        # element proxies are stale
        with pytest.raises(RuntimeError, match="stale"):
            _ = e0.value
        with pytest.raises(RuntimeError, match="stale"):
            _ = e1.value

    def test_array_remove_invalidates_shifted_elements(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        e2 = doc["arr"][2]
        arr = doc["arr"]
        arr.remove(2)  # removes value 2 at index 1
        assert len(arr) == 2
        with pytest.raises(RuntimeError, match="stale"):
            _ = e2.value

    def test_clear_invalidates_everything(self) -> None:
        doc = Document.parse(
            toml_literal("""
            x = 1
            y = 2
            [t]
            a = 1
        """)
        )
        px = doc["x"]
        py = doc["y"]
        pa = doc["t"]["a"]
        pt = doc["t"]
        doc.clear()
        for proxy in (px, py, pa, pt):
            with pytest.raises(RuntimeError, match="stale"):
                _ = proxy.value

    def test_update_only_invalidates_replaced_keys(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        pa = doc["t"]["a"]
        pb = doc["t"]["b"]
        t = doc["t"]
        t.update({"a": 99, "c": 3})  # replaces "a", adds "c"
        with pytest.raises(RuntimeError, match="stale"):
            _ = pa.value
        assert pb.value == 2  # "b" untouched
        assert t["c"] == 3  # new key accessible


class TestDocumentFmtPreservesProxies:
    """Document.fmt() only changes whitespace, so proxies stay valid."""

    def test_fmt_does_not_invalidate(self) -> None:
        doc = Document.parse("x=1")
        proxy = doc["x"]
        doc.fmt()
        assert proxy.value == 1


class TestEndOfArrayOptimizations:
    """End-of-array mutations should NOT invalidate sibling element proxies."""

    def test_pop_last_preserves_earlier_proxies(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        first = doc["arr"][0]
        second = doc["arr"][1]
        doc["arr"].pop()
        assert first.value == 1
        assert second.value == 2

    def test_pop_middle_invalidates_later_proxies(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        last = doc["arr"][2]
        doc["arr"].pop(0)
        with pytest.raises(RuntimeError):
            _ = last.value

    def test_del_last_preserves_earlier_proxies(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        first = doc["arr"][0]
        del doc["arr"][-1]
        assert first.value == 1

    def test_del_middle_invalidates_later_proxies(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        last = doc["arr"][2]
        del doc["arr"][1]
        with pytest.raises(RuntimeError):
            _ = last.value

    def test_insert_at_end_preserves_existing_proxies(self) -> None:
        doc = Document.parse("arr = [1, 2]")
        first = doc["arr"][0]
        second = doc["arr"][1]
        doc["arr"].insert(99, 3)  # clamps to end
        assert first.value == 1
        assert second.value == 2
        assert doc["arr"][2].value == 3

    def test_insert_at_middle_invalidates_proxies(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        last = doc["arr"][2]
        doc["arr"].insert(1, 99)
        with pytest.raises(RuntimeError):
            _ = last.value

    def test_remove_last_preserves_earlier_proxies(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        first = doc["arr"][0]
        doc["arr"].remove(3)
        assert first.value == 1

    def test_remove_first_invalidates_later_proxies(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        last = doc["arr"][2]
        doc["arr"].remove(1)
        with pytest.raises(RuntimeError):
            _ = last.value

    def test_pop_last_invalidates_popped_proxy(self) -> None:
        """The removed element's proxy should still be stale."""
        doc = Document.parse("arr = [1, 2, 3]")
        third = doc["arr"][2]
        doc["arr"].pop()
        with pytest.raises((RuntimeError, IndexError)):
            _ = third.value


class TestPreciseArrayShiftInvalidation:
    """Mid-array mutations invalidate only indices at and after the shift point."""

    def test_pop_middle_preserves_earlier(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 4, 5]")
        p0 = doc["arr"][0]
        p1 = doc["arr"][1]
        p3 = doc["arr"][3]
        p4 = doc["arr"][4]
        doc["arr"].pop(2)  # remove index 2 → [1, 2, 4, 5]
        assert p0.value == 1
        assert p1.value == 2
        with pytest.raises(RuntimeError, match="stale"):
            _ = p3.value
        with pytest.raises(RuntimeError, match="stale"):
            _ = p4.value

    def test_del_middle_preserves_earlier(self) -> None:
        doc = Document.parse("arr = [10, 20, 30, 40]")
        p0 = doc["arr"][0]
        p3 = doc["arr"][3]
        del doc["arr"][1]  # → [10, 30, 40]
        assert p0.value == 10
        with pytest.raises(RuntimeError, match="stale"):
            _ = p3.value

    def test_insert_middle_preserves_earlier(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        p0 = doc["arr"][0]
        p2 = doc["arr"][2]
        doc["arr"].insert(1, 99)  # → [1, 99, 2, 3]
        assert p0.value == 1
        with pytest.raises(RuntimeError, match="stale"):
            _ = p2.value

    def test_remove_by_value_preserves_earlier(self) -> None:
        doc = Document.parse("arr = [10, 20, 30, 40]")
        p0 = doc["arr"][0]
        p3 = doc["arr"][3]
        doc["arr"].remove(20)  # removes index 1 → [10, 30, 40]
        assert p0.value == 10
        with pytest.raises(RuntimeError, match="stale"):
            _ = p3.value

    def test_multiple_shifts_accumulate(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 4, 5]")
        p0 = doc["arr"][0]
        p1 = doc["arr"][1]
        doc["arr"].pop(3)  # shift from 3 → [1, 2, 3, 5]
        p1_fresh = doc["arr"][1]  # created after first shift
        doc["arr"].pop(1)  # shift from 1 → [1, 3, 5]
        assert p0.value == 1  # index 0 below min threshold
        with pytest.raises(RuntimeError, match="stale"):
            _ = p1.value  # old proxy at index 1, stale
        with pytest.raises(RuntimeError, match="stale"):
            _ = p1_fresh.value  # proxy at index 1 after first shift, stale from second

    def test_slice_del_preserves_earlier(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 4, 5]")
        p0 = doc["arr"][0]
        p4 = doc["arr"][4]
        del doc["arr"][2:4]  # remove indices 2,3 → [1, 2, 5]
        assert p0.value == 1
        with pytest.raises(RuntimeError, match="stale"):
            _ = p4.value

    def test_slice_assign_preserves_earlier(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 4, 5]")
        p0 = doc["arr"][0]
        p1 = doc["arr"][1]
        p2 = doc["arr"][2]
        doc["arr"][2:4] = [30, 40, 50]  # replace indices 2,3 with 3 values
        assert p0.value == 1
        assert p1.value == 2
        with pytest.raises(RuntimeError, match="stale"):
            _ = p2.value

    def test_non_overlapping_shifts_preserve_between(self) -> None:
        doc = Document.parse("arr = [0, 1, 2, 3, 4, 5, 6, 7]")
        doc["arr"].pop(1)  # shift from 1 → [0, 2, 3, 4, 5, 6, 7]
        p3 = doc["arr"][3]  # proxy at index 3, created after first shift
        doc["arr"].pop(5)  # shift from 5 → [0, 2, 3, 4, 5, 7]
        # p3 is between the two shift thresholds: index 3 < 5
        # and it was created after the first shift. Should be valid.
        assert p3.value == 4

    def test_replace_array_clears_old_shifts(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        doc["arr"].pop(0)  # shift from 0 — all indices affected
        doc["arr"] = [10, 20, 30, 40]  # replace entire array
        p1 = doc["arr"][1]  # proxy created after replacement
        doc["arr"].pop(3)  # shift from 3
        # p1 at index 1 < 3, so it should be valid.
        # (Bug was: old shift from 0 survived the replacement and poisoned
        # the new shift via min(0, 3) = 0, making index 1 falsely stale.)
        assert p1.value == 20

    def test_negative_step_slice_assign_invalidates_all_changed(self) -> None:
        doc = Document.parse("arr = [0, 1, 2, 3, 4]")
        p0 = doc["arr"][0]
        p2 = doc["arr"][2]
        p4 = doc["arr"][4]
        doc["arr"][::-1] = [4, 3, 2, 1, 0]
        with pytest.raises(RuntimeError, match="stale"):
            _ = p0.value
        with pytest.raises(RuntimeError, match="stale"):
            _ = p2.value
        with pytest.raises(RuntimeError, match="stale"):
            _ = p4.value


class TestViewStaleness:
    """Views go stale when their path is invalidated, just like proxies."""

    def test_keys_view_stale_after_path_replaced(self) -> None:
        doc = Document.parse("[foo]\na = 1\nb = 2")
        view = doc["foo"].keys()
        assert set(view) == {"a", "b"}
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            list(view)

    def test_values_view_stale_after_path_replaced(self) -> None:
        doc = Document.parse("[foo]\na = 1")
        view = doc["foo"].values()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            list(view)

    def test_items_view_stale_after_path_replaced(self) -> None:
        doc = Document.parse("[foo]\na = 1")
        view = doc["foo"].items()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            list(view)

    def test_view_stale_len(self) -> None:
        doc = Document.parse("[foo]\na = 1")
        view = doc["foo"].keys()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            len(view)

    def test_view_stale_contains(self) -> None:
        doc = Document.parse("[foo]\na = 1")
        view = doc["foo"].keys()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            "a" in view  # noqa: B015

    def test_view_stale_repr(self) -> None:
        doc = Document.parse("[foo]\na = 1")
        view = doc["foo"].keys()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            repr(view)

    def test_view_stale_after_path_deleted(self) -> None:
        doc = Document.parse("[foo]\na = 1")
        view = doc["foo"].keys()
        del doc["foo"]
        with pytest.raises(RuntimeError, match="stale"):
            list(view)

    def test_root_view_live_after_child_mutation(self) -> None:
        """Root views stay live when children are modified."""
        doc = Document.parse("a = 1\nb = 2")
        view = doc.keys()
        assert set(view) == {"a", "b"}
        doc["c"] = 3
        assert set(view) == {"a", "b", "c"}

    def test_root_view_live_after_child_replacement(self) -> None:
        doc = Document.parse("a = 1\nb = 2")
        view = doc.keys()
        doc["a"] = 99
        assert set(view) == {"a", "b"}

    def test_root_view_stale_after_clear(self) -> None:
        doc = Document.parse("a = 1\nb = 2")
        view = doc.keys()
        doc.clear()
        with pytest.raises(RuntimeError, match="stale"):
            list(view)

    def test_view_survives_sibling_mutation(self) -> None:
        """A view on one subtree is not invalidated by changes to a sibling."""
        doc = Document.parse("[foo]\na = 1\n[bar]\nb = 2")
        view = doc["foo"].keys()
        doc["bar"] = "replaced"
        assert set(view) == {"a"}

    def test_view_stale_set_operations(self) -> None:
        doc = Document.parse("[foo]\na = 1\nb = 2")
        view = doc["foo"].keys()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            view & {"a"}
        with pytest.raises(RuntimeError, match="stale"):
            view | {"a"}
        with pytest.raises(RuntimeError, match="stale"):
            view - {"a"}
        with pytest.raises(RuntimeError, match="stale"):
            view ^ {"a"}

    def test_items_view_stale_contains(self) -> None:
        doc = Document.parse("[foo]\na = 1")
        view = doc["foo"].items()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            ("a", 1) in view  # noqa: B015

    def test_items_view_stale_eq(self) -> None:
        doc = Document.parse("[foo]\na = 1")
        view = doc["foo"].items()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            view == {("a", 1)}  # noqa: B015

    def test_keys_view_stale_reversed(self) -> None:
        doc = Document.parse("[foo]\na = 1\nb = 2")
        view = doc["foo"].keys()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            list(reversed(view))

    def test_values_view_stale_reversed(self) -> None:
        doc = Document.parse("[foo]\na = 1\nb = 2")
        view = doc["foo"].values()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            list(reversed(view))

    def test_items_view_stale_reversed(self) -> None:
        doc = Document.parse("[foo]\na = 1\nb = 2")
        view = doc["foo"].items()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            list(reversed(view))

    def test_keys_view_stale_eq(self) -> None:
        doc = Document.parse("[foo]\na = 1")
        view = doc["foo"].keys()
        doc["foo"] = "bar"
        with pytest.raises(RuntimeError, match="stale"):
            view == {"a"}  # noqa: B015


class TestSliceOverInvalidation:
    """Same-length slice replacement should not invalidate unaffected proxies."""

    def test_same_length_slice_does_not_stale_later_proxy(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 4, 5]\n")
        proxy_at_3 = doc["arr"][3]
        assert proxy_at_3.value == 4
        doc["arr"][1:3] = [20, 30]
        # Indices 3 and 4 did not shift — proxy should still be valid
        assert proxy_at_3.value == 4

    def test_same_length_slice_does_stale_replaced_proxy(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 4, 5]\n")
        proxy_at_1 = doc["arr"][1]
        assert proxy_at_1.value == 2
        doc["arr"][1:3] = [20, 30]
        # Index 1 was replaced — proxy should be stale
        with pytest.raises(RuntimeError, match="stale"):
            proxy_at_1.value  # noqa: B018
