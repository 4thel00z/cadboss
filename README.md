# cadboss

A from-scratch Rust reader for CAD files. It extracts text, metadata and structure. It ships as a command-line tool and as Python bindings. DXF (ASCII and binary, R12 through R2018) is supported today.

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

Every command accepts `--budget KEY=VALUE` (`max_bytes`, `max_entities`, `max_layouts`, `max_nesting`, `max_string_bytes`, `max_tokens`). `--threads N` is reserved and currently unused. Skipped content is reported on stderr.

The CLI memory-maps the input file. Truncating or rewriting that file while `cadboss` runs can crash the process, so do not point it at a file another program is still writing; copy the file first.

```python
from cadboss import Document

doc = Document.open("plan.dxf")
print(doc.layers, doc.layouts, doc.units)
for item in doc.text("title-block"):
    print(item)
```

The Python package never maps a file: `Document.open(path)` reads the file into one buffer after checking its size against `max_bytes`, and `Document.load(data)` copies the bytes it is given.

In the memory gate run in `benchmarks/results-2026-11-03.json`, a 300 MB DXF with 1.7 million entities was processed by `cadboss text` in 1.32 seconds with anonymous memory at 0.758x the input size and peak RSS at 1.758x (gates 1.0x and 2.0x), and by `Document.open(path).markdown()` in 1.40 seconds with peak RSS at 1.866x (gates 2.0x and 2.5x). `benchmarks/README.md` explains the two gates.

## Development

Run `make help` to list every target. `make ci` runs the checks CI runs and `make test-py` runs the Python tests.

- `make samples` downloads the public sample drawings listed in `tests/samples/manifest.toml` and verifies their SHA-256 hashes. `cargo test -p cadboss-dxf --test samples` opens every downloaded sample and skips the rest.
- `make bench` runs the criterion benchmarks. `benchmarks/README.md` describes the memory gate.
- `make fuzz` seeds `fuzz/corpus/dxf_load` from `tests/fixtures`, runs the DXF fuzz target for `FUZZ_SECONDS` (default 60) with a 10 second per-input timeout and a 1024 MB RSS limit, and skips without a nightly toolchain and `cargo-fuzz`.

## License

Licensed under either of MIT or Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
