# cadboss

A from-scratch Rust reader for CAD files. It extracts text, metadata and structure, and renders drawings. It ships as a command-line tool and as Python bindings. DXF (ASCII and binary, R12 through R2018) is supported today.

## Install

```sh
cargo install cadboss-cli
```

```sh
pip install cadboss
```

`uv add cadboss` works too. The cargo package installs the `cadboss` binary.

## Usage

```sh
cadboss info plan.dxf
cadboss text plan.dxf --group title-block
cadboss md plan.dxf --title "Pump plan"
cadboss json plan.dxf --entities --layout Model
cadboss q plan.dxf '.metadata.version' -r
cadboss layouts plan.dxf
cadboss layers plan.dxf
cadboss obj plan.dxf 1F
```

Every command accepts `--budget KEY=VALUE` (`max_bytes`, `max_entities`, `max_layouts`, `max_nesting`, `max_string_bytes`, `max_tokens`) and `--threads N`. Skipped content is reported on stderr.

```python
from cadboss import Document

doc = Document.open("plan.dxf")
print(doc.layers, doc.layouts, doc.units)
for item in doc.text("title-block"):
    print(item)
```

Text-format input is read from a memory map. In the memory gate run in `benchmarks/results-2026-11-03.json`, a 300 MB DXF with 1.7 million entities was processed in 2.3 seconds with anonymous memory at 0.826x the input size and peak RSS at 1.826x. The gates are 1.0x and 2.0x.

## Development

Run `make help` to list every target. `make ci` runs the checks CI runs and `make test-py` runs the Python tests.

- `make samples` downloads the public sample drawings listed in `tests/samples/manifest.toml` and verifies their SHA-256 hashes. `cargo test -p cadboss-dxf --test samples` opens every downloaded sample and skips the rest.
- `make bench` runs the criterion benchmarks. `benchmarks/README.md` describes the memory gate.
- `make fuzz` runs the DXF fuzz target for `FUZZ_SECONDS` (default 60) and skips without a nightly toolchain and `cargo-fuzz`.

## License

Licensed under either of MIT or Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
