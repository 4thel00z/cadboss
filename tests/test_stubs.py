import ast
from pathlib import Path

import cadboss

STUB = Path(__file__).parent.parent / "python" / "cadboss" / "_cadboss.pyi"


def test_every_exported_name_is_stubbed() -> None:
    tree = ast.parse(STUB.read_text())
    defined = {
        node.name
        for node in tree.body
        if isinstance(node, (ast.ClassDef, ast.FunctionDef))
    }
    annotated = {
        node.target.id
        for node in tree.body
        if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name)
    }
    for name in cadboss.__all__:
        if name == "__version__":
            assert name in annotated
            continue
        assert name in defined, name
