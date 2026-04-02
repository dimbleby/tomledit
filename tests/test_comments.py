"""Tests for the .comment (block) and .inline_comment (inline) properties."""

from __future__ import annotations

import pytest

from tests.conftest import toml_literal
from tomledit import Document


class TestComment:
    # ---- reading inline comment ----

    def test_read_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # inline note\n")
        assert doc["key"].inline_comment == "# inline note"

    def test_read_no_inline_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        assert doc["key"].inline_comment is None

    def test_read_inline_on_table_value(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [section]
            x = 1 # inside
        """)
        )
        assert doc["section"]["x"].inline_comment == "# inside"

    # ---- setting inline comment ----

    def test_clear_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # remove me\n")
        doc["key"].inline_comment = None
        assert doc["key"].inline_comment is None
        assert doc.as_toml() == "key = 42\n"

    def test_inline_comment_preserves_value(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].inline_comment = "# note"
        assert doc["key"] == 42

    # ---- reading comment ----

    def test_read_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # before
            key = 42
        """)
        )
        assert doc["key"].comment == "# before"

    def test_read_no_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        assert doc["key"].comment is None

    def test_read_comment_nested(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [section]
            # before x
            x = 1
        """)
        )
        assert doc["section"]["x"].comment == "# before x"

    # ---- setting comment ----

    def test_clear_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # remove
            key = 42
        """)
        )
        doc["key"].comment = None
        assert doc["key"].comment is None
        assert doc.as_toml() == "key = 42\n"

    # ---- both together ----

    def test_both_comments(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "# above"
        doc["key"].inline_comment = "# inline"
        assert doc.as_toml() == toml_literal("""
            # above
            key = 42 # inline
        """)
        assert doc["key"].comment == "# above"
        assert doc["key"].inline_comment == "# inline"

    # ---- round-trip ----

    def test_comment_roundtrip(self) -> None:
        toml = toml_literal("""
            # important
            key = 42 # magic number
        """)
        doc = Document.parse(toml)
        assert doc.as_toml() == toml

    # ---- array element comments ----

    def test_set_inline_comment_on_array_element(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1,
              2,
              3,
            ]
        """)
        )
        doc["arr"][0].inline_comment = "# first"
        doc["arr"][2].inline_comment = "# last"
        assert doc.as_toml() == toml_literal("""
            arr = [
              1, # first
              2,
              3, # last
            ]
        """)
        doc2 = Document.parse(doc.as_toml())
        assert doc2["arr"][0].inline_comment == "# first"
        assert doc2["arr"][1].inline_comment is None
        assert doc2["arr"][2].inline_comment == "# last"

    def test_array_inline_comment_roundtrip(self) -> None:
        toml = toml_literal("""
            arr = [
              "a", # one
              "b", # two
              "c", # three
            ]
        """)
        doc = Document.parse(toml)
        assert doc.as_toml() == toml
        assert doc["arr"][0].inline_comment == "# one"
        assert doc["arr"][1].inline_comment == "# two"
        assert doc["arr"][2].inline_comment == "# three"

    def test_array_inline_comment_clear(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1,
              2,
              3,
            ]
        """)
        )
        doc["arr"][1].inline_comment = "# temp"
        assert doc["arr"][1].inline_comment == "# temp"
        doc["arr"][1].inline_comment = None
        assert doc["arr"][1].inline_comment is None
        assert doc.as_toml() == toml_literal("""
            arr = [
              1,
              2,
              3,
            ]
        """)

    def test_replace_array_inline_comment(self) -> None:
        toml = toml_literal("""
            arr = [
              1, # old
              2,
            ]
        """)
        doc = Document.parse(toml)
        assert doc["arr"][0].inline_comment == "# old"
        doc["arr"][0].inline_comment = "# new"
        assert doc["arr"][0].inline_comment == "# new"
        assert doc.as_toml() == toml_literal("""
            arr = [
              1, # new
              2,
            ]
        """)

    def test_array_inline_comment_preserves_values(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1,
              2,
            ]
        """)
        )
        doc["arr"][0].inline_comment = "# note"
        assert doc["arr"][0] == 1
        assert doc["arr"][1] == 2

    def test_array_inline_comment_preserves_indentation(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
                1,
                2,
            ]
        """)
        )
        doc["arr"][0].inline_comment = "# wide indent"
        assert doc.as_toml() == toml_literal("""
            arr = [
                1, # wide indent
                2,
            ]
        """)

    def test_array_comment_preserves_indentation(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
                1,
                2,
            ]
        """)
        )
        doc["arr"][1].comment = "# about two"
        assert doc.as_toml() == toml_literal("""
            arr = [
                1,
                # about two
                2,
            ]
        """)

    def test_array_comment_clear(self) -> None:
        toml = toml_literal("""
            arr = [
              1,
              # note
              2,
            ]
        """)
        doc = Document.parse(toml)
        doc["arr"][1].comment = None
        assert doc.as_toml() == toml_literal("""
            arr = [
              1,
              2,
            ]
        """)

    def test_array_inline_comment_on_single_element(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1,
            ]
        """)
        )
        doc["arr"][0].inline_comment = "# only"
        assert doc.as_toml() == toml_literal("""
            arr = [
              1, # only
            ]
        """)
        doc2 = Document.parse(doc.as_toml())
        assert doc2["arr"][0].inline_comment == "# only"

    def test_inline_comment_stable_after_append(self) -> None:
        doc = Document()
        doc["array"] = ["zero", "one"]
        doc["array"][1].inline_comment = "# better than zero"
        doc["array"].append("two")
        assert doc["array"][1].inline_comment == "# better than zero"
        assert doc["array"][2].inline_comment is None

    def test_inline_comment_stable_after_append_multiline(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              "a",
              "b", # note
            ]
        """)
        )
        assert doc["arr"][1].inline_comment == "# note"
        doc["arr"].append("c")
        assert doc["arr"][1].inline_comment == "# note"
        assert doc["arr"][2].inline_comment is None

    def test_inline_comment_stable_after_extend(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1, # first
            ]
        """)
        )
        doc["arr"].extend([2, 3])
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment is None
        assert doc["arr"][2].inline_comment is None

    def test_inline_comment_stable_after_iadd(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1, # first
            ]
        """)
        )
        doc["arr"] += [2, 3]
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment is None

    # ---- inline comments transfer across arrays ----

    def test_extend_transfers_inline_comment(self) -> None:
        doc = Document()
        doc["dst"] = ["a"]
        doc["src"] = ["b", "c"]
        doc["src"][0].inline_comment = "# from b"
        doc["src"][1].inline_comment = "# from c"
        doc["dst"].extend(doc["src"])
        assert doc["dst"][1].inline_comment == "# from b"
        assert doc["dst"][2].inline_comment == "# from c"

    def test_append_transfers_inline_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            src = [
              1, # noted
            ]
            dst = [0]
        """)
        )
        doc["dst"].append(doc["src"][0])
        assert doc["dst"][1].inline_comment == "# noted"

    def test_insert_transfers_inline_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            src = [
              1, # noted
            ]
            dst = [0, 2]
        """)
        )
        doc["dst"].insert(1, doc["src"][0])
        assert doc["dst"][0].inline_comment is None
        assert doc["dst"][1].inline_comment == "# noted"
        assert doc["dst"][2].inline_comment is None

    def test_setitem_transfers_inline_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            src = [
              1, # noted
            ]
            dst = [0, 2]
        """)
        )
        doc["dst"][0] = doc["src"][0]
        assert doc["dst"][0].inline_comment == "# noted"
        assert doc["dst"][1].inline_comment is None

    def test_setitem_preserves_existing_when_source_has_none(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1, # keep
            ]
            plain = [99]
        """)
        )
        doc["arr"][0] = doc["plain"][0]
        assert doc["arr"][0].inline_comment == "# keep"

    def test_extended_slice_transfers_inline_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            src = [
              1, # s1
              2, # s2
            ]
            dst = [0, 0, 0]
        """)
        )
        doc["dst"][::2] = [doc["src"][0], doc["src"][1]]
        assert doc["dst"][0].inline_comment == "# s1"
        assert doc["dst"][1].inline_comment is None
        assert doc["dst"][2].inline_comment == "# s2"

    def test_iadd_transfers_inline_comment(self) -> None:
        doc = Document()
        doc["dst"] = [0]
        doc["src"] = [1]
        doc["src"][0].inline_comment = "# tag"
        doc["dst"] += doc["src"]
        assert doc["dst"][1].inline_comment == "# tag"

    def test_scalar_inline_comment_transfers_to_array(self) -> None:
        doc = Document()
        doc["val"] = 42
        doc["val"].inline_comment = "# answer"
        doc["arr"] = [1]
        doc["arr"].append(doc["val"])
        assert doc["arr"][1].inline_comment == "# answer"

    # ---- insert preserves comments ----

    def test_insert_middle_preserves_prev_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1, # first
              2, # second
              3, # third
            ]
        """)
        )
        doc["arr"].insert(1, 99)
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment is None
        assert doc["arr"][2].inline_comment == "# second"
        assert doc["arr"][3].inline_comment == "# third"

    def test_insert_at_start_no_comment_shift(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1, # first
              2, # second
            ]
        """)
        )
        doc["arr"].insert(0, 99)
        assert doc["arr"][0].inline_comment is None
        assert doc["arr"][1].inline_comment == "# first"
        assert doc["arr"][2].inline_comment == "# second"

    def test_insert_at_end_preserves_prev_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1, # first
              2, # second
            ]
        """)
        )
        doc["arr"].insert(2, 99)
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment == "# second"
        assert doc["arr"][2].inline_comment is None

    # ---- insert does not inherit block comments ----

    def test_insert_at_start_no_block_comment_inheritance(self) -> None:
        """New element at position 0 must not inherit the old first
        element's block comment (regression)."""
        doc = Document.parse(
            toml_literal("""
            arr = [
                # section A
                "apple",
                # section B
                "carrot",
                "potato",
            ]
        """)
        )
        doc["arr"].insert(0, "banana")
        assert doc["arr"][0].comment is None  # banana (new)
        assert doc["arr"][1].comment == "# section A"  # apple
        assert doc["arr"][2].comment == "# section B"  # carrot

    def test_insert_middle_no_block_comment_inheritance(self) -> None:
        """New element inserted in the middle must not inherit any
        existing element's block comment (regression)."""
        doc = Document.parse(
            toml_literal("""
            arr = [
                # section A
                "apple",
                # section B
                "carrot",
            ]
        """)
        )
        doc["arr"].insert(1, "banana")
        assert doc["arr"][0].comment == "# section A"  # apple
        assert doc["arr"][1].comment is None  # banana (new)
        assert doc["arr"][2].comment == "# section B"  # carrot

    def test_insert_no_mixed_comment_inheritance(self) -> None:
        """When elements have both block and inline comments, inserting a
        new element must not inherit either block comment (regression)."""
        doc = Document.parse(
            toml_literal("""
            vals = [
                # section A
                10, # ten
                # section B
                20, # twenty
                30, # thirty
            ]
        """)
        )
        doc["vals"].insert(1, 15)
        assert doc["vals"][0].comment == "# section A"
        assert doc["vals"][0].inline_comment == "# ten"
        assert doc["vals"][1].comment is None  # 15 (new)
        assert doc["vals"][1].inline_comment is None
        assert doc["vals"][2].comment == "# section B"
        assert doc["vals"][2].inline_comment == "# twenty"
        assert doc["vals"][3].inline_comment == "# thirty"

    # ---- removal preserves comments ----

    def test_pop_last_drops_its_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1,
              2,
              3, # last
            ]
        """)
        )
        doc["arr"].pop()
        assert doc["arr"][0].inline_comment is None
        assert doc["arr"][1].inline_comment is None

    def test_pop_last_preserves_prev_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1,
              2, # second
              3,
            ]
        """)
        )
        doc["arr"].pop()
        assert doc["arr"][1].inline_comment == "# second"

    def test_remove_middle_preserves_prev_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1, # first
              2,
              3,
            ]
        """)
        )
        doc["arr"].remove(2)
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment is None

    def test_del_middle_all_commented(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1, # first
              2, # second
              3, # third
            ]
        """)
        )
        del doc["arr"][1]
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment == "# third"

    def test_pop_first_drops_its_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1, # first
              2, # second
              3, # third
            ]
        """)
        )
        doc["arr"].pop(0)
        assert doc["arr"][0].inline_comment == "# second"
        assert doc["arr"][1].inline_comment == "# third"

    def test_slice_del_preserves_survivor_comments(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1, # first
              2, # second
              3, # third
            ]
        """)
        )
        del doc["arr"][0:2]
        assert doc["arr"][0].inline_comment == "# third"

    def test_array_comment_valid_toml(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1,
              2,
              3,
            ]
        """)
        )
        doc["arr"][0].inline_comment = "# first"
        doc["arr"][1].comment = "# block"
        doc["arr"][2].inline_comment = "# last"
        Document.parse(doc.as_toml())  # should not raise

    def test_array_inline_and_block_coexist(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
              1,
              2,
              3,
            ]
        """)
        )
        doc["arr"][0].inline_comment = "# inline on first"
        doc["arr"][1].comment = "# block before second"
        assert doc.as_toml() == toml_literal("""
            arr = [
              1, # inline on first
              # block before second
              2,
              3,
            ]
        """)
        assert doc["arr"][0].inline_comment == "# inline on first"
        assert doc["arr"][1].comment == "# block before second"
        Document.parse(doc.as_toml())  # valid TOML

    def test_array_inline_and_block_native_roundtrip(self) -> None:
        toml = toml_literal("""
            arr = [
              1, # inline
              # block
              2,
            ]
        """)
        doc = Document.parse(toml)
        assert doc.as_toml() == toml
        assert doc["arr"][0].inline_comment == "# inline"
        assert doc["arr"][1].comment == "# block"

    # ---- blank lines ----

    def test_blank_line_before_comment_roundtrip(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1

            # note
            b = 2
        """)
        )
        assert doc["b"].comment == "\n# note"
        assert doc.as_toml() == toml_literal("""
            a = 1

            # note
            b = 2
        """)

    def test_multiple_blank_lines_before_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1


            # note
            b = 2
        """)
        )
        assert doc["b"].comment == "\n\n# note"

    def test_blank_line_between_comment_lines(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        doc["b"].comment = "# first\n\n# second"
        assert doc.as_toml() == toml_literal("""
            a = 1
            # first

            # second
            b = 2
        """)
        assert doc["b"].comment == "# first\n\n# second"

    # ---- validation (consolidated) ----

    def test_inline_comment_rejects_newlines(self) -> None:
        """Both scalar and array inline comments must reject newlines."""
        doc = Document.parse(
            toml_literal("""
            key = 42
            arr = [
              1,
              2,
            ]
        """)
        )
        with pytest.raises(ValueError, match="newlines"):
            doc["key"].inline_comment = "# line1\nline2"
        with pytest.raises(ValueError, match="newlines"):
            doc["arr"][0].inline_comment = "# a\nb"

    def test_inline_comment_requires_hash(self) -> None:
        """Both scalar and array inline comments must start with #."""
        doc = Document.parse(
            toml_literal("""
            key = 42
            arr = [
              1,
              2,
            ]
        """)
        )
        with pytest.raises(ValueError, match="#"):
            doc["key"].inline_comment = "no hash"
        with pytest.raises(ValueError, match="#"):
            doc["arr"][0].inline_comment = "no hash"

    def test_comment_requires_hash(self) -> None:
        """Both scalar and array block comments must start with #."""
        doc = Document.parse(
            toml_literal("""
            key = 42
            arr = [
              1,
              2,
            ]
        """)
        )
        with pytest.raises(ValueError, match="#"):
            doc["key"].comment = "no hash"
        with pytest.raises(ValueError, match="#"):
            doc["arr"][1].comment = "no hash"

    def test_lone_hash_inline(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].inline_comment = "#"
        assert doc.as_toml() == "key = 42 #\n"
        assert doc["key"].inline_comment == "#"

    def test_lone_hash_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "#"
        assert doc.as_toml() == toml_literal("""
            #
            key = 42
        """)
        assert doc["key"].comment == "#"

    def test_control_char_in_inline_comment_rejected(self) -> None:
        doc = Document.parse("key = 42\n")
        with pytest.raises(ValueError, match="invalid character"):
            doc["key"].inline_comment = "# \x1f"

    def test_control_char_in_block_comment_rejected(self) -> None:
        doc = Document.parse("key = 42\n")
        with pytest.raises(ValueError, match="invalid character"):
            doc["key"].comment = "# hello\x07"

    def test_tab_in_comment_allowed(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].inline_comment = "#\tindented"
        assert doc["key"].inline_comment == "#\tindented"

    def test_inline_comment_on_aot_rejected(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[s]]
            name = 'a'
            [[s]]
            name = 'b'
        """)
        )
        with pytest.raises(TypeError, match="does not support"):
            doc["s"].inline_comment = "# nope"

    # ---- table section block comments ----

    def test_read_table_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # before table
            [section]
            x = 1
        """)
        )
        assert doc["section"].comment == "# before table"

    def test_read_table_no_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [section]
            x = 1
        """)
        )
        assert doc["section"].comment is None

    def test_set_table_comment_from_scratch(self) -> None:
        doc = Document()
        doc["foo"] = {"this": "that"}
        doc["foo"].comment = "# Hello"
        assert doc.as_toml() == toml_literal("""
            # Hello
            [foo]
            this = "that"
        """)
        doc2 = Document.parse(doc.as_toml())
        assert doc2["foo"]["this"] == "that"
        assert doc2["foo"].comment == "# Hello"

    def test_clear_table_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # remove
            [section]
            x = 1
        """)
        )
        doc["section"].comment = None
        assert doc.as_toml() == "\n[section]\nx = 1\n"
        assert doc["section"].comment is None

    def test_multiline_table_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [section]
            x = 1
        """)
        )
        doc["section"].comment = "# line A\n# line B"
        assert doc.as_toml() == toml_literal("""
            # line A
            # line B
            [section]
            x = 1
        """)
        assert doc["section"].comment == "# line A\n# line B"

    def test_table_comment_roundtrip(self) -> None:
        toml = toml_literal("""
            # important
            [section]
            key = 42
        """)
        doc = Document.parse(toml)
        assert doc["section"].comment == "# important"
        assert doc.as_toml() == toml

    def test_table_both_comments(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [section]
            x = 1
        """)
        )
        doc["section"].comment = "# above"
        doc["section"].inline_comment = "# inline"
        assert doc.as_toml() == toml_literal("""
            # above
            [section] # inline
            x = 1
        """)
        assert doc["section"].comment == "# above"
        assert doc["section"].inline_comment == "# inline"

    # ---- table section inline comments ----

    def test_read_table_section_inline_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [section] # my note
            x = 1
        """)
        )
        assert doc["section"].inline_comment == "# my note"

    def test_set_table_section_inline_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [section]
            x = 1
        """)
        )
        doc["section"].inline_comment = "# added"
        assert doc.as_toml() == toml_literal("""
            [section] # added
            x = 1
        """)
        assert doc["section"].inline_comment == "# added"

    def test_clear_table_section_inline_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [section] # comment
            x = 1
        """)
        )
        doc["section"].inline_comment = None
        assert doc["section"].inline_comment is None
        assert doc.as_toml() == toml_literal("""
            [section]
            x = 1
        """)

    def test_table_without_inline_comment_returns_none(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [section]
            x = 1
        """)
        )
        assert doc["section"].inline_comment is None

    # ---- inline table key comments ----

    def test_read_inline_table_key_comment_none(self) -> None:
        doc = Document.parse("t = {x = 1, y = 2}\n")
        assert doc["t"]["x"].comment is None

    def test_set_inline_table_key_comment(self) -> None:
        """Setting a block comment on an inline table key produces a
        multi-line inline table (valid TOML 1.1)."""
        doc = Document.parse("t = {x = 1, y = 2}\n")
        doc["t"]["x"].comment = "# note"
        assert doc.as_toml() == toml_literal("""
            t = {
             # note
             x = 1, y = 2}
        """)
        doc2 = Document.parse(doc.as_toml())
        assert doc2["t"]["x"] == 1
        assert doc2["t"]["y"] == 2
        assert doc2["t"]["x"].comment == "# note"

    def test_set_compact_single_key_inline_table_comment(self) -> None:
        """A compact single-key inline table falls back to canonical spacing."""
        doc = Document.parse("t = {x = 1}\n")
        doc["t"]["x"].comment = "# note"
        assert doc.as_toml() == toml_literal("""
            t = {
             # note
             x = 1}
        """)
        doc2 = Document.parse(doc.as_toml())
        assert doc2["t"]["x"] == 1
        assert doc2["t"]["x"].comment == "# note"

    def test_set_inline_table_non_first_key_comment(self) -> None:
        """Block comment on a non-first inline table key."""
        doc = Document.parse("t = {x = 1, y = 2}\n")
        doc["t"]["y"].comment = "# note"
        assert doc.as_toml() == toml_literal("""
            t = {x = 1,
             # note
             y = 2}
        """)
        doc2 = Document.parse(doc.as_toml())
        assert doc2["t"]["x"] == 1
        assert doc2["t"]["y"] == 2
        assert doc2["t"]["y"].comment == "# note"

    def test_clear_inline_table_key_comment(self) -> None:
        doc = Document.parse("t = {x = 1, y = 2}\n")
        doc["t"]["x"].comment = None
        assert doc["t"]["x"].comment is None
        assert doc.as_toml() == "t = {x = 1, y = 2}\n"

    def test_set_inline_comment_on_inline_table_value(self) -> None:
        """Setting inline comment on inline table value produces multiline."""
        doc = Document.parse("t = {x = 1, y = 2}\n")
        doc["t"]["x"].inline_comment = "# boom"
        result = doc.as_toml()
        assert "# boom" in result
        doc2 = Document.parse(result)
        assert doc2["t"]["x"] == 1
        assert doc2["t"]["y"] == 2
        assert doc2["t"]["x"].inline_comment == "# boom"

    def test_read_inline_comment_on_inline_table_value(self) -> None:
        """Inline comments on inline table values can be read."""
        doc = Document.parse(
            toml_literal("""
            t = {
              x = 1, # note on x
              y = 2,
            }
        """)
        )
        assert doc["t"]["x"].inline_comment == "# note on x"
        assert doc["t"]["y"].inline_comment is None

    def test_read_inline_comment_all_values(self) -> None:
        """All inline table values can have inline comments."""
        src = toml_literal("""
            t = {
              a = 1, # ca
              b = 2, # cb
              c = 3, # cc
            }
        """)
        doc = Document.parse(src)
        assert doc["t"]["a"].inline_comment == "# ca"
        assert doc["t"]["b"].inline_comment == "# cb"
        assert doc["t"]["c"].inline_comment == "# cc"

    def test_set_and_clear_inline_comment_on_inline_table_value(self) -> None:
        """Set then clear inline comment."""
        doc = Document.parse(
            toml_literal("""
            t = {
              x = 1,
              y = 2,
            }
        """)
        )
        doc["t"]["x"].inline_comment = "# hi"
        assert doc["t"]["x"].inline_comment == "# hi"
        doc["t"]["x"].inline_comment = None
        assert doc["t"]["x"].inline_comment is None

    def test_clear_inline_comment_on_inline_table_value_allowed(self) -> None:
        """Clearing (None) is always safe, even inside inline tables."""
        doc = Document.parse("t = {x = 1, y = 2}\n")
        doc["t"]["x"].inline_comment = None
        assert doc.as_toml() == "t = {x = 1, y = 2}\n"

    def test_inline_comment_single_key_inline_table(self) -> None:
        """Single-key inline table: comment goes to trailing."""
        doc = Document.parse(
            toml_literal("""
            t = {
              x = 1,
            }
        """)
        )
        doc["t"]["x"].inline_comment = "# only"
        assert doc["t"]["x"].inline_comment == "# only"
        result = doc.as_toml()
        doc2 = Document.parse(result)
        assert doc2["t"]["x"].inline_comment == "# only"

    def test_inline_comment_coexists_with_block_comment(self) -> None:
        """Inline comment on x and block comment on y are independent."""
        src = toml_literal("""
            t = {
              x = 1, # inline on x
              # block on y
              y = 2,
            }
        """)
        doc = Document.parse(src)
        assert doc["t"]["x"].inline_comment == "# inline on x"
        assert doc["t"]["y"].comment == "# block on y"
        assert doc["t"]["y"].inline_comment is None
        assert doc["t"]["x"].comment is None

    def test_delete_key_preserves_sibling_inline_comments(self) -> None:
        """Deleting middle key preserves other keys' inline comments."""
        src = toml_literal("""
            t = {
              a = 1, # ca
              b = 2, # cb
              c = 3, # cc
            }
        """)
        doc = Document.parse(src)
        del doc["t"]["b"]
        assert doc["t"]["a"].inline_comment == "# ca"
        assert doc["t"]["c"].inline_comment == "# cc"

    def test_delete_first_key_preserves_inline_comments(self) -> None:
        """Deleting first key preserves remaining comments."""
        src = toml_literal("""
            t = {
              a = 1, # ca
              b = 2, # cb
            }
        """)
        doc = Document.parse(src)
        del doc["t"]["a"]
        assert doc["t"]["b"].inline_comment == "# cb"

    def test_delete_last_key_preserves_inline_comments(self) -> None:
        """Deleting last key preserves remaining comments."""
        src = toml_literal("""
            t = {
              a = 1, # ca
              b = 2, # cb
            }
        """)
        doc = Document.parse(src)
        del doc["t"]["b"]
        assert doc["t"]["a"].inline_comment == "# ca"

    def test_pop_key_preserves_inline_comments(self) -> None:
        """pop() preserves sibling inline comments."""
        src = toml_literal("""
            t = {
              a = 1, # ca
              b = 2, # cb
              c = 3, # cc
            }
        """)
        doc = Document.parse(src)
        val = doc["t"].pop("b")
        assert val == 2
        assert doc["t"]["a"].inline_comment == "# ca"
        assert doc["t"]["c"].inline_comment == "# cc"

    def test_add_key_preserves_inline_comments(self) -> None:
        """Adding a new key preserves existing inline comments."""
        src = toml_literal("""
            t = {
              a = 1, # ca
              b = 2, # cb
            }
        """)
        doc = Document.parse(src)
        doc["t"]["c"] = 3
        assert doc["t"]["a"].inline_comment == "# ca"
        assert doc["t"]["b"].inline_comment == "# cb"
        assert doc["t"]["c"].inline_comment is None

    def test_update_preserves_inline_comments(self) -> None:
        """update() with new keys preserves existing inline comments."""
        src = toml_literal("""
            t = {
              a = 1, # ca
            }
        """)
        doc = Document.parse(src)
        doc["t"].update({"b": 2})
        assert doc["t"]["a"].inline_comment == "# ca"

    def test_setdefault_preserves_inline_comments(self) -> None:
        """setdefault() with new key preserves existing inline comments."""
        src = toml_literal("""
            t = {
              a = 1, # ca
            }
        """)
        doc = Document.parse(src)
        doc["t"].setdefault("b", 2)
        assert doc["t"]["a"].inline_comment == "# ca"

    def test_clone_inline_table_value_carries_inline_comment(self) -> None:
        """Assigning a value from an inline table copies its inline comment."""
        src = toml_literal("""
            t = {
              x = 1, # travel
              y = 2,
            }
            arr = [0]
        """)
        doc = Document.parse(src)
        doc["arr"][0] = doc["t"]["x"]
        assert doc["arr"][0].inline_comment == "# travel"

    def test_inline_table_comment_not_misattributed(self) -> None:
        """An inline comment after x must not appear as y's block comment."""
        doc = Document.parse(
            toml_literal("""
            t = {
              x = 1, # note on x
              y = 2,
            }
        """)
        )
        assert doc["t"]["x"].comment is None
        assert doc["t"]["y"].comment is None

    def test_inline_table_set_comment_preserves_prev_inline(self) -> None:
        """Setting y's block comment must not clobber x's inline comment."""
        src = toml_literal("""
            t = {
              x = 1, # note on x
              y = 2,
            }
        """)
        doc = Document.parse(src)
        doc["t"]["y"].comment = "# block on y"
        result = doc.as_toml()
        # x's inline comment survives in the serialised output
        assert "# note on x" in result
        assert "# block on y" in result
        # roundtrip: y's block comment reads back correctly
        doc2 = Document.parse(result)
        assert doc2["t"]["y"].comment == "# block on y"

    def test_inline_table_clear_comment_preserves_prev_inline(self) -> None:
        """Clearing y's block comment preserves x's inline comment."""
        src = toml_literal("""
            t = {
              x = 1, # note on x
              # block on y
              y = 2,
            }
        """)
        doc = Document.parse(src)
        assert doc["t"]["y"].comment == "# block on y"
        doc["t"]["y"].comment = None
        result = doc.as_toml()
        assert "# note on x" in result
        doc2 = Document.parse(result)
        assert doc2["t"]["y"].comment is None

    def test_inline_table_both_inline_and_block_separated(self) -> None:
        """When the prefix has both an inline comment from the previous
        element and a block comment for this key, only the block portion
        is returned by .comment."""
        src = toml_literal("""
            t = {
              x = 1, # inline on x
              # block on y
              y = 2,
            }
        """)
        doc = Document.parse(src)
        assert doc["t"]["x"].comment is None
        assert doc["t"]["y"].comment == "# block on y"

    # ---- array comment edge cases ----

    def test_multiline_array_with_blank_line_between_comments(self) -> None:
        """Tests split_prefix where block has multiple newlines."""
        toml = toml_literal("""
            arr = [
              # first
              1,

              # after blank
              2,
            ]
        """)
        doc = Document.parse(toml)
        # blank line is preserved as part of the block comment
        assert doc["arr"][1].comment == "\n# after blank"

    def test_set_comment_on_multiline_array_element(self) -> None:
        """Set a block comment on an element in a multiline array."""
        toml = toml_literal("""
            arr = [
              1,
              2,
              3,
            ]
        """)
        doc = Document.parse(toml)
        doc["arr"][1].comment = "# middle"
        assert doc.as_toml() == toml_literal("""
            arr = [
              1,
              # middle
              2,
              3,
            ]
        """)
        assert doc["arr"][1].comment == "# middle"

    def test_blank_line_between_elements_no_comment(self) -> None:
        """Blank line between elements with no comment - exercises
        split_prefix block.is_empty() branch."""
        doc = Document.parse(
            toml_literal("""
            arr = [
              1,

              2,
            ]
        """)
        )
        assert doc["arr"][0].comment is None
        assert doc["arr"][0].inline_comment is None

    def test_compact_array_comment_returns_none(self) -> None:
        """Compact single-line array - prefix has no newline at all,
        exercises split_prefix no-newline fallback."""
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"][0].comment is None
        assert doc["arr"][1].comment is None
        assert doc["arr"][0].inline_comment is None

    # ---- comments on AoT children ----

    def test_set_inline_comment_on_aot_entry_child(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        doc["items"][0]["name"].inline_comment = "# first"
        assert doc["items"][0]["name"].inline_comment == "# first"
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a" # first
            [[items]]
            name = "b"
        """)

    def test_set_comment_on_aot_entry_child(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        doc["items"][1]["name"].comment = "# second item name"
        assert doc["items"][1]["name"].comment == "# second item name"
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            # second item name
            name = "b"
        """)

    # ---- comment edge cases ----

    def test_inline_comment_on_aot_itself_is_none(self) -> None:
        """AoT item has no decor suffix — inline_comment returns None."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        assert doc["items"].inline_comment is None

    def test_comment_on_aot_itself_is_none(self) -> None:
        """AoT with no preceding comment returns None."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        assert doc["items"].comment is None

    def test_set_comment_on_aot(self) -> None:
        """Setting a block comment on an AoT should work and round-trip."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        doc["items"].comment = "# about items"
        assert doc["items"].comment == "# about items"
        reparsed = Document.parse(doc.as_toml())
        assert reparsed["items"].comment == "# about items"

    def test_read_existing_comment_on_aot(self) -> None:
        """A comment already present before [[aot]] should be readable."""
        doc = Document.parse(
            toml_literal("""
            # about items
            [[items]]
            name = "a"
        """)
        )
        assert doc["items"].comment == "# about items"

    def test_clear_comment_on_aot(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # about items
            [[items]]
            name = "a"
        """)
        )
        doc["items"].comment = None
        assert doc["items"].comment is None

    # ---- comments survive mutations ----

    def test_comments_preserved_on_scalar_to_table(self) -> None:
        """Replacing a scalar with a fresh table preserves the block comment."""
        doc = Document({"a": "hello"})
        doc["a"].comment = "# about a"
        doc["a"].inline_comment = "# note"
        doc["a"] = {"x": 1}
        assert doc["a"].comment == "# about a"

    def test_comments_preserved_on_scalar_to_aot(self) -> None:
        """Replacing a scalar with a fresh AoT preserves the block comment."""
        doc = Document({"a": "hello"})
        doc["a"].comment = "# about a"
        doc["a"] = [{"x": 1}]
        assert doc["a"].comment == "# about a"

    def test_comments_preserved_on_table_to_scalar(self) -> None:
        """Replacing a table with a fresh scalar preserves the block comment."""
        doc = Document.parse("# about b\n[b]\nx = 1\n")
        assert doc["b"].comment == "# about b"
        doc["b"] = "flat"
        assert doc["b"].comment == "# about b"

    def test_source_comment_wins_over_target(self) -> None:
        """When the source already has a comment, it takes precedence."""
        src = Document.parse("# source\n[b]\nx = 1\n")
        dst = Document.parse("# target\n[a]\ny = 2\n")
        dst["a"] = src["b"]
        assert dst["a"].comment == "# source"

    def test_source_comment_preserved_on_cross_doc_copy(self) -> None:
        """Copying a commented table from another doc keeps its comment."""
        src = Document.parse("# source comment\n[b]\nx = 1\n")
        dst = Document({"a": "hello"})
        dst["a"] = src["b"]
        assert dst["a"].comment is not None
        assert "source comment" in dst["a"].comment

    def test_comments_preserved_on_scalar_to_scalar(self) -> None:
        """Replacing a scalar with another scalar preserves comments."""
        doc = Document({"a": "hello"})
        doc["a"].comment = "# about a"
        doc["a"].inline_comment = "# note"
        doc["a"] = "world"
        assert doc["a"].comment == "# about a"
        assert doc["a"].inline_comment == "# note"

    def test_inline_comment_preserved_on_top_level_update(self) -> None:
        toml = 'title = "old" # important note\n'
        doc = Document.parse(toml)
        doc["title"] = "new"
        assert doc.as_toml() == 'title = "new" # important note\n'

    def test_inline_comment_preserved_on_nested_update(self) -> None:
        toml = toml_literal("""
            [owner]
            name = "Tom"  # the owner name
            age = 30
        """)
        doc = Document.parse(toml)
        doc["owner"]["name"] = "Bob"
        assert doc.as_toml() == toml_literal("""
            [owner]
            name = "Bob"  # the owner name
            age = 30
        """)


class TestAotEntryComments:
    """Comments on individual AoT entries (``[[section]]`` headers).

    Regular ``[table]`` comments are readable and settable via ``.comment``.
    The same should work for each ``[[array-of-tables]]`` entry.
    """

    def test_read_comment_on_aot_entry(self) -> None:
        """A comment above an AoT entry header should be readable."""
        doc = Document.parse(
            toml_literal("""
            # first entry
            [[items]]
            name = "a"

            # second entry
            [[items]]
            name = "b"
        """)
        )
        assert doc["items"][0].comment == "# first entry"
        assert doc["items"][1].comment == "# second entry"

    def test_set_comment_on_aot_entry(self) -> None:
        """Setting a comment on an AoT entry should round-trip."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        doc["items"][0].comment = "# about first"
        assert doc["items"][0].comment == "# about first"
        assert doc.as_toml() == toml_literal("""
            # about first
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)

    def test_clear_comment_on_aot_entry(self) -> None:
        """Setting comment to None should remove it."""
        doc = Document.parse(
            toml_literal("""
            # will be removed
            [[items]]
            name = "a"
        """)
        )
        doc["items"][0].comment = None
        assert doc["items"][0].comment is None

    def test_aot_entry_comment_survives_reparse(self) -> None:
        """Comments set on AoT entries survive a round-trip through TOML."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        doc["items"][0].comment = "# annotated"
        reparsed = Document.parse(doc.as_toml())
        assert reparsed["items"][0].comment == "# annotated"

    def test_aot_entry_block_comment_is_none_when_absent(self) -> None:
        """AoT entries with no preceding comment return None."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        assert doc["items"][0].comment is None
        assert doc["items"][1].comment is None

    def test_aot_entry_inline_comment_roundtrip(self) -> None:
        """AoT entries support inline comments on the ``[[header]]``."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        assert doc["items"][0].inline_comment is None
        assert doc["items"][1].inline_comment is None
        doc["items"][0].inline_comment = "# first"
        doc["items"][1].inline_comment = "# second"
        assert doc["items"][0].inline_comment == "# first"
        assert doc["items"][1].inline_comment == "# second"
        assert doc.as_toml() == toml_literal("""
            [[items]] # first
            name = "a"
            [[items]] # second
            name = "b"
        """)

    def test_aot_entry_clear_inline_comment(self) -> None:
        doc = Document.parse("[[t]] # note\nk = 1\n")
        assert doc["t"][0].inline_comment == "# note"
        doc["t"][0].inline_comment = None
        assert doc["t"][0].inline_comment is None
        assert doc.as_toml() == "[[t]]\nk = 1\n"

    # ---- AoT entry comments survive mutations ----

    def test_replace_entry_0_preserves_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # first
            [[items]]
            name = "a"

            # second
            [[items]]
            name = "b"
        """)
        )
        doc["items"][0] = {"name": "replaced"}
        assert doc["items"][0].comment == "# first"
        assert doc["items"][1].comment == "# second"
        assert doc.as_toml() == toml_literal("""
            # first
            [[items]]
            name = "replaced"

            # second
            [[items]]
            name = "b"
        """)

    def test_replace_entry_0_preserves_compact_spacing(self) -> None:
        """Replacing entry 0 in a compact AoT should not inject a blank line."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        doc["items"][0] = {"name": "replaced"}
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "replaced"
            [[items]]
            name = "b"
        """)

    def test_replace_middle_entry_preserves_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # first
            [[items]]
            name = "a"

            # second
            [[items]]
            name = "b"

            # third
            [[items]]
            name = "c"
        """)
        )
        doc["items"][1] = {"name": "replaced"}
        assert doc["items"][0].comment == "# first"
        assert doc["items"][1].comment == "# second"
        assert doc["items"][2].comment == "# third"

    def test_replace_last_entry_preserves_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # first
            [[items]]
            name = "a"

            # last
            [[items]]
            name = "b"
        """)
        )
        doc["items"][1] = {"name": "replaced"}
        assert doc["items"][1].comment == "# last"

    def test_remove_entry_0_cleans_new_first_prefix(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # first
            [[items]]
            name = "a"

            # second
            [[items]]
            name = "b"

            # third
            [[items]]
            name = "c"
        """)
        )
        del doc["items"][0]
        assert doc["items"][0].comment == "# second"
        assert doc["items"][1].comment == "# third"

    def test_remove_entry_0_no_comments(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        del doc["items"][0]
        assert doc["items"][0].comment is None
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "b"
        """)

    def test_contiguous_slice_replace_preserves_first_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # first
            [[items]]
            name = "a"

            # second
            [[items]]
            name = "b"

            # third
            [[items]]
            name = "c"
        """)
        )
        doc["items"][0:2] = [{"name": "x"}]
        assert doc["items"][0].comment == "# first"
        assert doc["items"][1].comment == "# third"

    def test_slice_del_entry_0_cleans_prefix(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # first
            [[items]]
            name = "a"

            # second
            [[items]]
            name = "b"

            # third
            [[items]]
            name = "c"
        """)
        )
        del doc["items"][0:2]
        assert doc["items"][0].comment == "# third"

    def test_extended_slice_replace_preserves_first_comment(self) -> None:
        doc = Document.parse(
            toml_literal("""
            # first
            [[items]]
            name = "a"

            # second
            [[items]]
            name = "b"

            # third
            [[items]]
            name = "c"
        """)
        )
        doc["items"][::2] = [{"name": "x"}, {"name": "z"}]
        assert doc["items"][0].comment == "# first"
        assert doc["items"][1].comment == "# second"

    def test_set_comment_on_non_first_aot_entry(self) -> None:
        """Setting a comment on entry 1+ uses element decor, not key decor."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        doc["items"][1].comment = "# second"
        assert doc["items"][1].comment == "# second"
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"

            # second
            [[items]]
            name = "b"
        """)

    def test_contiguous_slice_on_commentless_aot(self) -> None:
        """Contiguous slice starting at 0 on AoT with no comments."""
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
        doc["items"][0:2] = [{"name": "x"}]
        assert doc["items"][0].comment is None
        assert doc["items"][1].comment is None
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "x"
            [[items]]
            name = "c"
        """)

    def test_contiguous_slice_replace_multiple(self) -> None:
        """Replace [0:2] with two entries — exercises the spacing loop."""
        doc = Document.parse(
            toml_literal("""
            # first
            [[items]]
            name = "a"

            # second
            [[items]]
            name = "b"

            # third
            [[items]]
            name = "c"
        """)
        )
        doc["items"][0:2] = [{"name": "x"}, {"name": "y"}]
        assert doc["items"][0].comment == "# first"
        assert doc["items"][2].comment == "# third"

    def test_replace_commentless_entry(self) -> None:
        """Replacing an entry that had no comment preserves the no-comment state."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        doc["items"][1] = {"name": "replaced"}
        assert doc["items"][1].comment is None
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "replaced"
        """)


class TestIorPreservesComments:
    """|= preserves comments from both target and source."""

    def test_ior_preserves_target_comments_on_overlap(self) -> None:
        target = Document.parse("# target comment\na = 1\n")
        source = Document.parse("# source comment\na = 99\n")
        target |= source
        assert target.as_toml() == toml_literal("""
            # target comment
            a = 99
        """)

    def test_ior_preserves_source_block_comment(self) -> None:
        target = Document.parse("a = 1\n")
        source = Document.parse("# block comment\nb = 2\n")
        target |= source
        assert target.as_toml() == toml_literal("""
            a = 1
            # block comment
            b = 2
        """)

    def test_ior_preserves_source_inline_comment(self) -> None:
        target = Document.parse("a = 1\n")
        source = Document.parse("b = 2 # inline note\n")
        target |= source
        assert target.as_toml() == toml_literal("""
            a = 1
            b = 2 # inline note
        """)

    def test_ior_dict_proxy_preserves_source_comment(self) -> None:
        target = Document.parse("[section]\na = 1\n")
        source = Document.parse("[section]\n# new key comment\nb = 2\n")
        target["section"] |= source["section"]
        assert target.as_toml() == toml_literal("""
            [section]
            a = 1
            # new key comment
            b = 2
        """)

    def test_ior_inline_table_preserves_source_comment(self) -> None:
        target = Document.parse("dst = { a = 1 }\n")
        source = Document.parse("src = { b = 2 }\n")
        source["src"]["b"].comment = "# new key comment"
        target["dst"] |= source["src"]
        assert target.as_toml() == toml_literal("""
            dst = { a = 1,
             # new key comment
             b = 2 }
        """)
        assert target["dst"]["b"].comment == "# new key comment"


class TestUpdatePreservesComments:
    """update() preserves comments from both target and source."""

    def test_update_preserves_target_comments_on_overlap(self) -> None:
        target = Document.parse("# target comment\na = 1\n")
        source = Document.parse("# source comment\na = 99\n")
        target.update(source)
        assert target.as_toml() == toml_literal("""
            # target comment
            a = 99
        """)

    def test_update_preserves_source_block_comment(self) -> None:
        target = Document.parse("a = 1\n")
        source = Document.parse("# block comment\nb = 2\n")
        target.update(source)
        assert target.as_toml() == toml_literal("""
            a = 1
            # block comment
            b = 2
        """)

    def test_update_proxy_preserves_source_comment(self) -> None:
        target = Document.parse("[section]\na = 1\n")
        source = Document.parse("[section]\n# new key comment\nb = 2\n")
        target["section"].update(source["section"])
        assert target.as_toml() == toml_literal("""
            [section]
            a = 1
            # new key comment
            b = 2
        """)

    def test_update_inline_table_preserves_source_comment(self) -> None:
        target = Document.parse("dst = { a = 1 }\n")
        source = Document.parse("src = { b = 2 }\n")
        source["src"]["b"].comment = "# new key comment"
        target["dst"].update(source["src"])
        assert target.as_toml() == toml_literal("""
            dst = { a = 1,
             # new key comment
             b = 2 }
        """)
        assert target["dst"]["b"].comment == "# new key comment"


class TestOrPreservesComments:
    """| preserves source comments in the returned mapping."""

    def test_or_inline_table_preserves_source_comment(self) -> None:
        target = Document.parse("dst = { a = 1 }\n")
        source = Document.parse("src = { b = 2 }\n")
        source["src"]["b"].comment = "# new key comment"
        result = target["dst"] | source["src"]
        assert result.as_toml() == toml_literal("""
            { a = 1,
             # new key comment
             b = 2 }
        """).rstrip("\n")
        assert result["b"].comment == "# new key comment"


class TestCommentIdempotency:
    """Setting a comment to its current value should be a no-op."""

    def test_set_array_inline_comment_idempotent(self) -> None:
        doc = Document.parse("arr = [1, 2, 3] # note\n")
        original = doc.as_toml()
        doc["arr"][0].inline_comment = doc["arr"][0].inline_comment
        assert doc.as_toml() == original
