# Changelog

All notable changes to this project are recorded here. Releases are managed automatically from conventional commits.

## Unreleased

- Core types: points, bounding boxes, transforms, arc geometry, byte-budgeted sources, errors and skip reports.
- Unified document model with entities, layers, blocks, layouts and grouped text.
- DXF tape lexer for ASCII and binary files with section indexing and resynchronisation after damage.
- DXF header variables, codepage decoding, tables, blocks, layouts and an entity decoder with MTEXT stripping.
- Text extraction into title-block, annotation, name, attribute, reference and property groups.
- JSON and Markdown writers.
- `cadboss` CLI with info, text, md, json, q, layouts, layers, obj and skill commands.
- Python bindings with type stubs, built and tested as a wheel in CI.
- Criterion benchmarks, a memory-budget proof and a fuzz target for the DXF reader.
- Public sample drawings manifest with hash-verified downloads and an integration test over them.
