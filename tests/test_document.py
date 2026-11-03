import json
import tomllib
from pathlib import Path
from threading import Thread

import pytest

import cadboss
from cadboss import CadError, Document

ROOT = Path(__file__).parent.parent


def test_open_fixture_lists_layers_and_layouts(simple_dxf_path: Path) -> None:
    doc = Document.open(simple_dxf_path)
    assert doc.format == "DXF"
    assert doc.layers
    assert doc.layouts
    assert doc.layouts[0].kind == "Model"
    assert doc.layers[0].name == "0"
    assert isinstance(doc.entity_counts, list)
    assert "Layer(" in repr(doc.layers[0])


def test_load_bytes_equals_open(simple_dxf_path: Path, simple_dxf_bytes: bytes) -> None:
    opened = Document.open(str(simple_dxf_path))
    loaded = Document.load(simple_dxf_bytes)
    assert loaded.json(compact=True) == opened.json(compact=True)
    assert Document.load(bytearray(simple_dxf_bytes)).layers == loaded.layers
    assert Document.load(memoryview(simple_dxf_bytes)).layers == loaded.layers


def test_text_title_block_group(simple_dxf_bytes: bytes) -> None:
    doc = Document.load(simple_dxf_bytes)
    items = doc.text("title-block")
    everything = doc.text()
    assert len(everything) >= len(items)
    assert all(item in everything for item in items)


def test_markdown_starts_with_title(simple_dxf_bytes: bytes) -> None:
    doc = Document.load(simple_dxf_bytes)
    assert doc.markdown(title="Plan").startswith("# Plan")


def test_json_is_parseable(simple_dxf_bytes: bytes) -> None:
    doc = Document.load(simple_dxf_bytes)
    assert json.loads(doc.json())["metadata"]["format"] == "Dxf"
    assert "\n" not in doc.json(compact=True)


def test_garbage_bytes_raise_cad_error() -> None:
    with pytest.raises(CadError):
        Document.load(b"garbage")


def test_empty_bytes_raise_cad_error() -> None:
    with pytest.raises(CadError):
        Document.load(b"")


def test_bad_group_raises_value_error(simple_dxf_bytes: bytes) -> None:
    doc = Document.load(simple_dxf_bytes)
    with pytest.raises(ValueError):
        doc.text("nonsense")


def test_bad_budget_and_type_errors(simple_dxf_bytes: bytes) -> None:
    with pytest.raises(ValueError, match="unknown budget key"):
        Document.load(simple_dxf_bytes, budget={"max_nothing": 1})
    with pytest.raises(ValueError):
        Document.load(simple_dxf_bytes, budget={"max_bytes": -1})
    with pytest.raises(CadError):
        Document.load(simple_dxf_bytes, budget={"max_bytes": 10})
    with pytest.raises(TypeError):
        Document.load("text")  # type: ignore[arg-type]


def test_document_is_usable_from_threads(simple_dxf_bytes: bytes) -> None:
    doc = Document.load(simple_dxf_bytes)
    results: list[str] = []

    def work() -> None:
        results.append(doc.markdown())

    threads = [Thread(target=work) for _ in range(2)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert len(results) == 2
    assert results[0] == results[1]


def test_version_matches_cargo() -> None:
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())
    assert cadboss.__version__ == cargo["workspace"]["package"]["version"]
