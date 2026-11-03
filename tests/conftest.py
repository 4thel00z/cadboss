from pathlib import Path

import pytest

FIXTURES = Path(__file__).parent / "fixtures"


@pytest.fixture
def simple_dxf_path() -> Path:
    return FIXTURES / "simple.dxf"


@pytest.fixture
def simple_dxf_bytes(simple_dxf_path: Path) -> bytes:
    return simple_dxf_path.read_bytes()
