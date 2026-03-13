"""Tests for proxy generation counter (stale-proxy detection)."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from tomledit import Document

if TYPE_CHECKING:
    from tomledit import Item


class TestStaleProxyDetection:
    """Proxies created before a mutation raise RuntimeError on access."""

    def test_stale_after_setitem_on_doc(self) -> None:
        doc = Document.parse("x = 1\ny = 2")
        proxy = doc["x"]
        doc["x"] = 99
        with pytest.raises(RuntimeError, match="stale"):
            _ = proxy.value

    def test_stale_after_delitem_on_doc(self) -> None:
        doc = Document.parse("x = 1\ny = 2")
        proxy = doc["y"]
        del doc["x"]
        with pytest.raises(RuntimeError, match="stale"):
            _ = proxy.value

    def test_stale_after_pop_on_doc(self) -> None:
        doc = Document.parse("x = 1\ny = 2")
        proxy = doc["y"]
        doc.pop("x")
        with pytest.raises(RuntimeError, match="stale"):
            _ = proxy.value

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

    def test_stale_after_clear_on_doc(self) -> None:
        doc = Document.parse("x = 1")
        proxy = doc["x"]
        doc.clear()
        with pytest.raises(RuntimeError, match="stale"):
            _ = proxy.value

    def test_valid_after_setdefault_new_key_on_doc(self) -> None:
        """setdefault with a new key doesn't replace anything — no paths break."""
        doc = Document.parse("x = 1")
        proxy = doc["x"]
        doc.setdefault("y", 2)
        assert proxy.value == 1

    def test_not_stale_after_setdefault_existing_key(self) -> None:
        doc = Document.parse("x = 1")
        proxy = doc["x"]
        doc.setdefault("x", 99)  # no-op, key exists
        assert proxy.value == 1  # should NOT raise


class TestStaleProxyViaProxy:
    """Mutations through a proxy invalidate sibling proxies."""

    def test_sibling_proxy_stale_after_setitem(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2")
        b = doc["t"]["b"]
        a_table = doc["t"]
        a_table["a"] = 99
        with pytest.raises(RuntimeError, match="stale"):
            _ = b.value

    def test_sibling_proxy_stale_after_delitem(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2")
        b = doc["t"]["b"]
        t = doc["t"]
        del t["a"]
        with pytest.raises(RuntimeError, match="stale"):
            _ = b.value

    def test_valid_after_array_append(self) -> None:
        """append doesn't invalidate siblings — no paths break."""
        doc = Document.parse("arr = [1, 2]")
        item = doc["arr"][0]
        arr = doc["arr"]
        arr.append(3)
        assert item.value == 1
        assert arr.value == [1, 2, 3]

    def test_negative_index_stable_after_append(self) -> None:
        """Negative indices are resolved at lookup time, so append can't shift them."""
        doc = Document.parse("arr = [1, 2, 3]")
        last = doc["arr"][-1]
        assert last.value == 3
        doc["arr"].append(4)
        assert last.value == 3  # still index 2, not shifted to 4

    def test_stale_after_array_insert(self) -> None:
        doc = Document.parse("arr = [1, 2]")
        item = doc["arr"][0]
        arr = doc["arr"]
        arr.insert(0, 99)
        with pytest.raises(RuntimeError, match="stale"):
            _ = item.value

    def test_stale_after_array_remove(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        item = doc["arr"][2]
        arr = doc["arr"]
        arr.remove(1)
        with pytest.raises(RuntimeError, match="stale"):
            _ = item.value

    def test_valid_after_array_extend(self) -> None:
        """extend doesn't invalidate siblings — no paths break."""
        doc = Document.parse("arr = [1]")
        item = doc["arr"][0]
        arr = doc["arr"]
        arr.extend([2, 3])
        assert item.value == 1
        assert arr.value == [1, 2, 3]

    def test_stale_after_proxy_clear(self) -> None:
        doc = Document.parse("[t]\na = 1")
        a = doc["t"]["a"]
        t = doc["t"]
        t.clear()
        with pytest.raises(RuntimeError, match="stale"):
            _ = a.value

    def test_stale_after_proxy_pop(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]")
        item = doc["arr"][0]
        arr = doc["arr"]
        arr.pop()
        with pytest.raises(RuntimeError, match="stale"):
            _ = item.value

    def test_valid_after_additive_proxy_update(self) -> None:
        doc = Document.parse("[t]\na = 1")
        a = doc["t"]["a"]
        t = doc["t"]
        t.update({"b": 2})
        assert a.value == 1

    def test_stale_after_replacing_proxy_update(self) -> None:
        doc = Document.parse("[t]\na = 1")
        a = doc["t"]["a"]
        t = doc["t"]
        t.update({"a": 2})
        with pytest.raises(RuntimeError, match="stale"):
            _ = a.value


class TestMutatorProxyStaysValid:
    """The proxy that performs a mutation stays valid afterward."""

    def test_setitem_through_proxy(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2")
        t = doc["t"]
        t["a"] = 99
        assert t["a"] == 99  # mutating proxy is still valid

    def test_delitem_through_proxy(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2")
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
        doc = Document.parse("[t]\na = 1\nb = 2")
        t = doc["t"]
        t.clear()
        assert len(t) == 0

    def test_update_through_proxy(self) -> None:
        doc = Document.parse("[t]\na = 1")
        t = doc["t"]
        t.update({"b": 2})
        assert t["b"] == 2

    def test_multiple_mutations_through_same_proxy(self) -> None:
        doc = Document.parse("arr = [1]")
        arr = doc["arr"]
        arr.append(2)
        arr.append(3)
        arr.append(4)
        assert len(arr) == 4

    def test_setdefault_through_proxy(self) -> None:
        doc = Document.parse("[t]\na = 1")
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


class TestCommentMutationsPreserveProxies:
    """Setting comments is decoration-only, so proxies stay valid."""

    def test_set_comment_does_not_invalidate_sibling(self) -> None:
        doc = Document.parse("a = 1\nb = 2")
        b = doc["b"]
        a = doc["a"]
        a.comment = "# hello"
        assert b.value == 2

    def test_set_inline_comment_does_not_invalidate_sibling(self) -> None:
        doc = Document.parse("a = 1\nb = 2")
        b = doc["b"]
        a = doc["a"]
        a.inline_comment = "# hello"
        assert b.value == 2


class TestFreshProxiesAfterMutation:
    """New proxies created after a mutation work fine."""

    def test_new_proxy_after_doc_mutation(self) -> None:
        doc = Document.parse("x = 1\ny = 2")
        old = doc["x"]
        doc["x"] = 99
        new = doc["x"]
        assert new.value == 99
        with pytest.raises(RuntimeError, match="stale"):
            _ = old.value

    def test_new_child_after_proxy_mutation(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2")
        t = doc["t"]
        old_a = doc["t"]["a"]
        t["a"] = 99
        new_a = t["a"]
        assert new_a.value == 99
        with pytest.raises(RuntimeError, match="stale"):
            _ = old_a.value


class TestReadMethodsCheckGeneration:
    """All read-path methods should check the generation counter."""

    @pytest.fixture
    def stale_proxy(self) -> tuple[Item, Item]:
        doc = Document.parse("arr = [1, 2]\n\n[t]\na = 1\nb = 2")
        proxy_t = doc["t"]
        proxy_arr = doc["arr"]
        doc["t"]["a"] = 99  # replace existing value to invalidate all other proxies
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


class TestDocumentFmtPreservesProxies:
    """Document.fmt() only changes whitespace, so proxies stay valid."""

    def test_fmt_does_not_invalidate(self) -> None:
        doc = Document.parse("x=1")
        proxy = doc["x"]
        doc.fmt()
        assert proxy.value == 1
