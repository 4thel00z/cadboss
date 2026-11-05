# Changelog

## 0.1.0 (2026-11-05)


### Features

* **cli:** info, text, md, json, q, layouts, layers, obj and skill ([cee5479](https://github.com/4thel00z/cadboss/commit/cee54798cdb9048272c14a688ba666c619f90e29))
* **cli:** tui subcommand ([f5223a1](https://github.com/4thel00z/cadboss/commit/f5223a1a397a72c97d10ea152deed63377a57d78))
* **core:** point, bbox, transform and arc geometry ([c6fab06](https://github.com/4thel00z/cadboss/commit/c6fab0637450c7c6df0bd448cecc3a6b08df4ee1))
* **core:** source, error, report and budget types ([35fff0f](https://github.com/4thel00z/cadboss/commit/35fff0f2cfc874aaef50b602e85dd127e83e0fa5))
* **core:** translate transform ([7da3078](https://github.com/4thel00z/cadboss/commit/7da3078099b895b57976004120391e1704b16c9b))
* DXF reader, CLI and Python bindings ([65902ca](https://github.com/4thel00z/cadboss/commit/65902ca2bf65dc679913a393a229c91df73f2ff4))
* **dxf:** assemble the unified document ([0e6d6fe](https://github.com/4thel00z/cadboss/commit/0e6d6fe682f29f8ab69f81103c154da0c7972f6d))
* **dxf:** binary dxf lexing into the same tape ([900df44](https://github.com/4thel00z/cadboss/commit/900df446ce31de0d89e65f45f3f013c492243079))
* **dxf:** blocks, layouts and the document with insert walking ([7866982](https://github.com/4thel00z/cadboss/commit/7866982346e6eea2ab323c92e8479c865f270409))
* **dxf:** byte range of a token range ([833c1ad](https://github.com/4thel00z/cadboss/commit/833c1ade5e8205512c8918c1578f32df21bd4cd8))
* **dxf:** entity decoder with mtext stripping and budgets ([ae5fedc](https://github.com/4thel00z/cadboss/commit/ae5fedc4ebef546ed874839aa2a3123d765d0e14))
* **dxf:** header variables, codepage decoding and tables ([2c639d3](https://github.com/4thel00z/cadboss/commit/2c639d31ab9f5ebf916913565f76c637ab3b2502))
* **dxf:** keep the PE_URL hyperlink on the entity ([efca48d](https://github.com/4thel00z/cadboss/commit/efca48dde12ed80ff4b6c656836b10045bbdee1d))
* **dxf:** layout_extents_reporting twin ([7e157dd](https://github.com/4thel00z/cadboss/commit/7e157dd937076318b380f9fd12a93414e6ccec22))
* **dxf:** located entities and block header ranges ([ec12df3](https://github.com/4thel00z/cadboss/commit/ec12df3e8a593ea1da980940cd4bb69c4ed4f73f))
* **dxf:** tape lexer with section index and resync ([bdf5d19](https://github.com/4thel00z/cadboss/commit/bdf5d1937b8751df8709cc2ff3342445e2ed6d47))
* **dxf:** text groups with title-block detection ([5c569ff](https://github.com/4thel00z/cadboss/commit/5c569ff0a41b284a95e59b9f388f81498aa099e9))
* **model:** text items carry their entity handle ([2b3ea74](https://github.com/4thel00z/cadboss/commit/2b3ea7496e046f91567c636bd8e8c640b582fff2))
* **model:** unified document, entity and text-group types ([55efad7](https://github.com/4thel00z/cadboss/commit/55efad73311a2ff7ff3092ead3dd33f46199d94b))
* **output:** json and markdown writers ([bc418f4](https://github.com/4thel00z/cadboss/commit/bc418f4c853e4baaf40e01ff5bcd247390d91ff4))
* **py:** document bindings, stubs and wheel ci ([07197b7](https://github.com/4thel00z/cadboss/commit/07197b7bddaa58ebc2c7ccf4bef21018319c6be9))
* **testkit:** dxf builders and committed fixtures ([0d704c0](https://github.com/4thel00z/cadboss/commit/0d704c06b43db647bd152bb60497250d986a630a))
* **testkit:** image entities backed by shared IMAGEDEF objects ([1832e5b](https://github.com/4thel00z/cadboss/commit/1832e5b85d46affa782bd189f000793875f25898))
* **testkit:** realistic drawing generator and gen_large example ([5b2a29c](https://github.com/4thel00z/cadboss/commit/5b2a29c2f5c1843a558755997805f238735fb29d))
* **tui:** app state with search, hex and DXF source views and copy ([6f32627](https://github.com/4thel00z/cadboss/commit/6f326271785c773bd4b64b4389945adcc4742449))
* **tui:** crate skeleton with theme, keys and clipboard ([534cc57](https://github.com/4thel00z/cadboss/commit/534cc5746898146521494dec70cd27e7a1059e33))
* **tui:** outline with lazy entity pages and record index ([c604bee](https://github.com/4thel00z/cadboss/commit/c604bee60286c82e6702f0d73668c5ae47d41e1e))
* **tui:** rendering, event loop and frame snapshots ([d4422be](https://github.com/4thel00z/cadboss/commit/d4422beaaecd76be8d8a85931cf3be968b07a14f))


### Bug Fixes

* **cli:** layout-scoped text groups, typed obj values and readable jq errors ([39af0f4](https://github.com/4thel00z/cadboss/commit/39af0f473f8e338748d9c3ca4b98c24e42a9f5ca))
* **cli:** one line per text item and unknown layouts exit 2 ([1d69297](https://github.com/4thel00z/cadboss/commit/1d692978e9c604e3c355c5e9d8519d15405ee882))
* **dxf:** bounded layout collection ([e63e502](https://github.com/4thel00z/cadboss/commit/e63e502b263fc365297f3ce297d8ac95d95ec803))
* **dxf:** count each skipped record once in the document report ([211c061](https://github.com/4thel00z/cadboss/commit/211c0615273b7e4edaa9b952afb7d3333f2a0590))
* **dxf:** detect one-byte group codes in r12 binary dxf ([308b9bc](https://github.com/4thel00z/cadboss/commit/308b9bc98408b1914cd6a0fc05cad29eebb19a69))
* **dxf:** mtext breaks, stripped dimension overrides and byte attributes ([1bbe0e4](https://github.com/4thel00z/cadboss/commit/1bbe0e4baf45c6f1b6451d162f4921a1a90235a8))
* **dxf:** owner handles, string budget, vertex resync and mtext paragraphs ([28343b7](https://github.com/4thel00z/cadboss/commit/28343b792c85fae4957271392c690c2f61370635))
* **dxf:** scaled text heights, dimension positions and text-group tests ([9c78e33](https://github.com/4thel00z/cadboss/commit/9c78e33556772bf518cafda960f5ee38e0e5af43))
* **dxf:** token budget, iterator cursor and section end at eof ([6940869](https://github.com/4thel00z/cadboss/commit/6940869d2b68006a3f81b36cf589274c8806d378))
* **dxf:** treat infinite and NaN numbers as missing ([ff8fdd2](https://github.com/4thel00z/cadboss/commit/ff8fdd2755e8f3fb3a1165486612368e839e8bf1))
* **dxf:** walk reports carry walk-level notes only ([94fba4c](https://github.com/4thel00z/cadboss/commit/94fba4c09392e5bb0acc88b224ecd0a5d43d4dfa))
* **dxf:** walk-wide entity budget, model layout synthesis and early layout filter ([b7bd24d](https://github.com/4thel00z/cadboss/commit/b7bd24d3504bca8a157f837d6fb3117f7aa506c4))
* **model:** count POLYLINE apart from LWPOLYLINE ([30384b7](https://github.com/4thel00z/cadboss/commit/30384b75f34c59b6c8e22a98a5e78e6554293922))
* **output:** no overflow on huge julian days ([4048b62](https://github.com/4thel00z/cadboss/commit/4048b6220aa4da913ed133a21dcdcbff0d8ef99d))
* **output:** omit empty names, count user blocks and report lines ([3482bd9](https://github.com/4thel00z/cadboss/commit/3482bd9399ec720a7d33921dc42718825446e163))
* **py:** test the built wheel in ci and tighten title-block test ([31ba42b](https://github.com/4thel00z/cadboss/commit/31ba42bbd26ff46bb5981e784bf35ef225e649ae))
* **release:** publishable manifests and gated release jobs ([024c67a](https://github.com/4thel00z/cadboss/commit/024c67aed7f02de2e85bab4f7d793014f6275338))
* **release:** simple release type for the workspace-inherited version ([420b034](https://github.com/4thel00z/cadboss/commit/420b034df9411ecf18fa5f98db17ea90a4807e6d))
* **testkit:** simple drawing in binary form and structural tests ([9bd64c2](https://github.com/4thel00z/cadboss/commit/9bd64c24287e11eee21fa54deb132f78e1e4c317))
* **tui:** entity rows read their record from their own tokens ([5125b02](https://github.com/4thel00z/cadboss/commit/5125b023b2248a15cf43aceea3f5c8872ce9c3c4))
* **tui:** help ignores key releases, view toggles round-trip, one value display helper ([3142911](https://github.com/4thel00z/cadboss/commit/31429113b5230cdb6bd0e405d33181bdd590521b))
* **tui:** hex fits the pane, whole-record DXF copy, match position, block counts ([08a5e3e](https://github.com/4thel00z/cadboss/commit/08a5e3e8c92a15ea27be45bf1ea77f7360d0eae9))
* **tui:** zero-width panes, inspector module, status bar hints ([754fff7](https://github.com/4thel00z/cadboss/commit/754fff767923f4b716ed547ec398ff9a99e78f64))


### Performance Improvements

* **cli:** stream json entities and build q input without a JSON string ([89c78a4](https://github.com/4thel00z/cadboss/commit/89c78a4b63eb8ddc1438ad9d261da1f2755de701))
* **core:** own input bytes once and check max_bytes before reading ([225e39f](https://github.com/4thel00z/cadboss/commit/225e39f2081dba31282225e7571425a69893ddaa))
* **dxf:** bucket ENTITIES once and walk each layout once ([b6fb513](https://github.com/4thel00z/cadboss/commit/b6fb513c1f9f9a69d678c9ad3d007b63afdf0698))
* **dxf:** memory budget proof, criterion benches and fuzz target ([66b360b](https://github.com/4thel00z/cadboss/commit/66b360b377271d5ab0eec3d77b150354938a65bc))
* **dxf:** rank text in place and place model text at once ([e9fe5bd](https://github.com/4thel00z/cadboss/commit/e9fe5bde25b43f7c4d8db8cf57e33e9a070af5ac))
* **dxf:** read entity, table and object records in place on the tape ([fe99ac3](https://github.com/4thel00z/cadboss/commit/fe99ac3645aca1342cfe7e3a1d3e6d21270c2d04))
* **output:** group markdown annotations by layer in one pass ([10a3e83](https://github.com/4thel00z/cadboss/commit/10a3e83c4a411d64839ef4f885c60f7b95fe721b))

## Changelog

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
