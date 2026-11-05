---
name: cadboss
description: Use when reading, extracting text or metadata from, or exploring CAD files with the cadboss CLI.
---

# cadboss

Reads DXF files (ASCII and binary) and prints metadata, text, Markdown or JSON. Every command accepts `--budget KEY=VALUE` (repeatable; keys: `max_bytes`, `max_entities`, `max_layouts`, `max_nesting`, `max_string_bytes`, `max_tokens`) and `--threads N` (reserved, currently unused). Skipped content is reported on stderr as `warning: ...`. The CLI memory-maps the input, so do not run it on a file another program is still writing. Exit code 1 means a bad input file, 2 means bad arguments or a bad jq program; a `--layout` that names no layout exits 2 and lists the available ones.

## Commands

- `cadboss info FILE` prints format, units, counts, layouts and extents.
  Example: `cadboss info plan.dxf`
- `cadboss text FILE [--layout NAME] [--group title-block|annotations|names|attributes|references|all]` prints one text item per line (line breaks inside an item become spaces), grouped under `# group` headers when the group is `all` (which also prints a `# properties` group). With `--layout`, only title-block, annotations, attributes and references are shown.
  Example: `cadboss text plan.dxf --group title-block`
- `cadboss md FILE [--title T]` prints a Markdown summary; the title defaults to the file stem.
  Example: `cadboss md plan.dxf --title "Pump plan"`
- `cadboss json FILE [--compact] [--entities] [--layout NAME]` prints the unified document as JSON; `--entities` adds the entity list of each layout, written as it is decoded.
  Example: `cadboss json plan.dxf --entities --layout Model`
- `cadboss q FILE PROGRAM [-r] [--entities] [--layout NAME]` runs a jq program over the same JSON as `json`, including `--entities` and `--layout`; `-r` prints strings raw. `q` holds the whole JSON tree in memory, so `q --entities` materializes every entity of the selected layouts; prefer `json --entities` piped to a streaming tool for large files.
  Example: `cadboss q plan.dxf '.metadata.version' -r`
- `cadboss layouts FILE` prints `name`, `kind` and entity count per layout, tab separated.
  Example: `cadboss layouts plan.dxf`
- `cadboss layers FILE` prints `name`, `color` and flags (`frozen`, `off`, `locked` or `-`) per layer, tab separated.
  Example: `cadboss layers plan.dxf`
- `cadboss obj FILE HANDLE` prints the raw group code and value pairs of the entity or object with that hex handle.
  Example: `cadboss obj plan.dxf 1F`
- `cadboss tui FILE` opens the interactive explorer; it needs a terminal and is not for agents.
- `cadboss skill install [-g]` writes this file to `./.claude/skills/cadboss/SKILL.md`, or to `~/.claude/skills/cadboss/SKILL.md` with `-g`; `cadboss skill show` prints it.
  Example: `cadboss skill install -g`

## Python

- Install: `pip install cadboss` (or `uv add cadboss`).
- `from cadboss import Document; doc = Document.open("plan.dxf")` (also `Document.load(bytes)`); read `doc.layers`, `doc.layouts`, `doc.units`.
- `doc.text("title-block")` returns `TextItem` objects; groups are title-block, annotations, names, attributes, references, properties, all.
- `doc.markdown(title="Pump plan")` and `doc.json(compact=True)` return strings; `CadError` signals an unreadable file.
