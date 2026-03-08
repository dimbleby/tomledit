"""Shared helpers for tomledit tests."""

from __future__ import annotations

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


def make_doc() -> Document:
    return Document.parse(SAMPLE)
