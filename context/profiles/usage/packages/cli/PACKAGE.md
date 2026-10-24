---
description: >-
  Invoking the kdlith command-line tool: the verb tree, the shared
  inputs every schema- and data-reading verb takes, exit codes, and
  the seven inspect views with their output formats.
binds_to_depth: main...
---

# kdlith CLI usage

kdlith is a single binary with eight verbs, all implemented:

- `gen dialect|schema` — generate a Rust processor crate from a
  dialect or schema definition
- `vet dialect|schema|data` — compile a definition or bind data
  documents against a schema and report every diagnostic
- `export` — write typed records as NDJSON
- `inspect dialect|schema|data` — show a reading seven ways with
  `--view`
- `list types|nodes|items` — list a definition's types and nodes, or
  a data reading's items
- `read data` — read one record, optionally from a cached snapshot
- `cache refresh` — cache bound results and context snapshots for a
  state directory
- `changelog` — print the embedded changelog archive
- `help` — verb help

Definitions and data are KDL v2 documents. `vet` and `inspect` print
their item streams to standard output and their diagnostics to
standard error; an error exits nonzero after everything printable has
been shown.

Read on demand:

- every verb's arguments and the exit-code strata —
  `reference/verbs.md`
- the inputs shared across verbs (`--schema`, `--root`, `--source`,
  `--alias`, `--profiles`, `--scope`, `--max-invocations`,
  `--resolve-from`, and the export sensitivity policy) —
  `reference/host_inputs.md`
- the seven `--view` outputs, their applicability, and ordering —
  `reference/inspect_views.md`
- the key tables for decoding each view's JSON lines —
  `reference/inspect_view_formats.md`

Minimal authored documents are in the `authoring` and `data`
packages' `examples/` directories.
