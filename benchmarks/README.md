# Benchmarks

## Memory budget

`memory.py` measures peak resident memory of two runs on one generated DXF.

- Input: `crates/cadboss-testkit/examples/gen_large.rs` streams `realistic_drawing(N)` to disk. Entities carry handles, owners, `AcDbEntity` and subclass markers, three-coordinate full-precision floats, and a mix of 70% LINE, 15% LWPOLYLINE, 10% TEXT, 5% CIRCLE (about 14 bytes per group-code pair).
- `cli-text`: the release CLI binary runs `cadboss text FILE`. The CLI maps the input, so RSS counts the file itself; mapped clean pages are evictable and do not drive OOM kills. Anonymous memory is peak RSS minus the input size. Gates: anonymous at most 1.0x the input, peak RSS at most 2.0x.
- `py-open-markdown`: `uv run maturin develop --release`, then the environment's interpreter runs `cadboss.Document.open(PATH).markdown()`. The Python path reads the input into one owned buffer instead of mapping it, so the buffer itself is anonymous memory (1.0x) and the tape adds about 0.6x; the whole peak RSS counts as anonymous. Gates: anonymous at most 2.0x, peak RSS at most 2.5x.
- Measured with `/usr/bin/time -l` (macOS, `maximum resident set size`) or `/usr/bin/time -v` (Linux, `Maximum resident set size`), plus wall time. The script exits 1 when either run exceeds its gates.
- Output: `results-<YYYY-MM-DD>.json` with one entry per run (benchmark name, entity count, input bytes, peak RSS, anonymous bytes, both ratios, both gates, seconds) and the hardware (OS, architecture, CPU, memory).

Run it:

```
uv run python benchmarks/memory.py --entities 1700000
```

Use `--out PATH` to write the JSON elsewhere.

## Criterion benches

`crates/cadboss-dxf/benches/dxf.rs` has groups `lex`, `document` and `text` over `realistic_drawing(10_000)`, `realistic_drawing(100_000)` and `multi_layout(50)`, and `decode` over one LWPOLYLINE with 200 000 vertices.

```
make bench
```

## Rule

Numbers are recorded verbatim with the hardware they were measured on. A result is never edited, rounded up or replaced by a run from other hardware; a new run gets a new dated file.
