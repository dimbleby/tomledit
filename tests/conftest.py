"""Shared fixtures for tomledit tests."""

from __future__ import annotations

import textwrap
from collections.abc import Mapping

import pytest

from tomledit import Document

SAMPLE = """\
title = "Example"

[owner]
name = "Alice"
age = 30
active = true

[database]
ports = [8001, 8001, 8002]
connection_max = 5000
enabled = true

[servers]

[servers.alpha]
ip = "10.0.0.1"
role = "frontend"

[servers.beta]
ip = "10.0.0.2"
role = "backend"
"""


def toml_literal(text: str) -> str:
    """Dedent a triple-quoted TOML string for comparison with ``doc.as_toml()``.

    Usage::

        assert doc.as_toml() == toml_literal(\"""
            [foo]
            bar = 1
        \""")
    """
    return textwrap.dedent(text).strip() + "\n"


class ItemsMapping:
    """Mapping helper with a configurable items() result."""

    def __init__(
        self,
        data: dict[str, object],
        items_override: object | None = None,
    ) -> None:
        self._data = data
        self._items_override = items_override

    def keys(self) -> object:
        return self._data.keys()

    def items(self) -> object:
        if self._items_override is not None:
            return self._items_override
        return self._data.items()

    def __getitem__(self, key: str) -> object:
        return self._data[key]

    def __len__(self) -> int:
        return len(self._data)

    def __iter__(self) -> object:
        return iter(self._data)


Mapping.register(ItemsMapping)  # ty: ignore[unresolved-attribute]


@pytest.fixture
def doc() -> Document:
    return Document.parse(SAMPLE)
