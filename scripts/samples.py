import argparse
import hashlib
import sys
import tomllib
import urllib.request
from pathlib import Path

SAMPLES_DIR = Path(__file__).resolve().parent.parent / "tests" / "samples"
MANIFEST = SAMPLES_DIR / "manifest.toml"


def sha256_hex(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def download(url: str, target: Path) -> None:
    with urllib.request.urlopen(url, timeout=60) as response:
        target.write_bytes(response.read())


def verify(name: str, target: Path, expected: str) -> bool:
    actual = sha256_hex(target)
    if not expected:
        print(f"{name}: no pinned hash, record sha256 = \"{actual}\"")
        return True
    if actual == expected:
        print(f"{name}: ok")
        return True
    target.unlink()
    print(f"{name}: sha256 mismatch, expected {expected} got {actual}; deleted", file=sys.stderr)
    return False


def fetch(entry: dict[str, object], check_only: bool) -> bool:
    name = str(entry["name"])
    target = SAMPLES_DIR / name
    expected = str(entry.get("sha256", ""))
    if not target.exists() and check_only:
        print(f"{name}: not downloaded")
        return True
    if not target.exists():
        download(str(entry["url"]), target)
    return verify(name, target, expected)


def main() -> int:
    parser = argparse.ArgumentParser(description="Fetch and verify public sample drawings")
    parser.add_argument("--check", action="store_true", help="verify files already present, download nothing")
    args = parser.parse_args()
    manifest = tomllib.loads(MANIFEST.read_text())
    entries: list[dict[str, object]] = manifest.get("sample", [])
    results = [fetch(entry, args.check) for entry in entries]
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
