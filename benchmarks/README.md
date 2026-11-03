# Benchmarks

## Memory budget

`memory.py` measures peak resident memory of `cadboss text FILE` on a generated DXF.

- Input: `crates/cadboss-testkit/examples/gen_large.rs` streams `realistic_drawing(N)` to disk. Entities carry handles, owners, `AcDbEntity` and subclass markers, three-coordinate full-precision floats, and a mix of 70% LINE, 15% LWPOLYLINE, 10% TEXT, 5% CIRCLE (about 14 bytes per group-code pair).
- Measured: the release CLI binary under `/usr/bin/time -l` (macOS, `maximum resident set size`) or `/usr/bin/time -v` (Linux, `Maximum resident set size`), plus wall time.
- Gate: the script exits 1 when anonymous memory (peak RSS minus input size) exceeds 1.0x the input, or peak RSS exceeds 2.0x the input. The input is memory-mapped, so RSS counts the file itself; mapped clean pages are evictable and do not drive OOM kills, so the anonymous ratio is the real budget and the RSS ratio a sanity ceiling.
- Output: `results-<YYYY-MM-DD>.json` with the benchmark name, entity count, input bytes, peak RSS, anonymous bytes, anonymous ratio, RSS ratio, seconds and hardware (OS, architecture, CPU, memory).

Run it:

```
uv run python benchmarks/memory.py --entities 1700000
```

Use `--out PATH` to write the JSON elsewhere.

## Criterion benches

`crates/cadboss-dxf/benches/dxf.rs` has groups `lex`, `document` and `text` over `realistic_drawing(10_000)`, `realistic_drawing(100_000)` and `multi_layout(50)`.

```
make bench
```

## Rule

Numbers are recorded verbatim with the hardware they were measured on. A result is never edited, rounded up or replaced by a run from other hardware; a new run gets a new dated file.
