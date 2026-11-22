from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any

from griffe import Extension, merge_stubs, visit
from typing_extensions import override

if TYPE_CHECKING:
    from griffe import GriffeLoader, Module


class TomleditStubs(Extension):
    """Supplement inspected PyO3 docstrings with the public stub's types."""

    @override
    def on_package(self, *, pkg: Module, loader: GriffeLoader, **kwargs: Any) -> None:
        if pkg.name != "tomledit":
            return

        path = Path(__file__).resolve().parents[1] / "tomledit.pyi"
        stub = visit(
            pkg.name,
            path,
            path.read_text(encoding="utf-8"),
            modules_collection=loader.modules_collection,
        )
        merge_stubs(pkg, stub)

        # The stub can declare a subclass property on Item for use after indexing.
        for class_name, stub_class in stub.classes.items():
            attributes = pkg.classes[class_name].attributes
            for name, inherited in stub_class.inherited_members.items():
                prop = attributes.get(name)
                if (
                    prop is not None
                    and prop.runtime
                    and "property" in prop.labels
                    and prop.annotation is None
                    and inherited.is_attribute
                ):
                    prop.annotation = inherited.annotation
