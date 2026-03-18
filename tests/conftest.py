"""Shared fixtures for tomledit tests."""

from __future__ import annotations

import textwrap

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


@pytest.fixture
def doc() -> Document:
    return Document.parse(SAMPLE)
