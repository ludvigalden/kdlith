# Changelog

All notable changes to kdlith are recorded in this archive. The
`kdlith changelog` command prints it exactly as committed. Versions
follow the workspace version; entries are grouped by kind of change.

## 0.1.0 (unreleased)

The first development line of the workspace. Nothing has been
published yet.

### Added

- Cargo workspace with the library crates `kdlith`, `kdlith-parse`,
  `kdlith-model`, `kdlith-runtime`, `kdlith-diagnostic`,
  `kdlith-engine`, `kdlith-codegen`, `kdlith-superproc`, and
  `kdlith-cli`, plus the `kdlith` command-line tool.
- KDL v2 parsing through a pull-based event surface with source
  coordinates, verified against the official KDL v2 conformance suite.
- Canonical dialect definition files for types, bindings, documents,
  and context, with fixture entrypoints that generate the dialect.
- A dialect design record describing schemas, bindings, context,
  imports, and provenance.
- Runtime sessions with bounded binding frames and context effects.
- Engine schema compilation with type and binding catalogs, linking,
  import resolution, and a compiled processing plan.
- A typed binder producing bound item streams, with data imports and
  handler effects executed during a reading.
- A hand-maintained reference processor for schema definitions, used
  to compile definitions through its typed reader.
- Deterministic generation of Rust processor crates from compiled
  definitions.
- A verb-first command tree: `gen`, `vet`, `export`, `inspect`,
  `list`, `read`, `cache`, `changelog`, and `help`, behind the
  `--verbosity`, `--debug`, and `--stdout-format` global options.
- `gen` with dry-run planning, drift checking, and refusal to
  overwrite files it does not own.
- `vet` and `inspect` for dialect definitions, schema definitions, and
  data documents, and `read` for one definition or indexed data item.
- `export` with entry and type selection, atomic or provisional
  delivery, provenance on every exported record, and an output policy
  for sensitive fields.
- `list` for declared types, nodes, profiles, and indexed data items.
- `cache` to inspect, refresh, and clean a versioned derived-state
  directory whose entries record what derived them.
- `changelog` to print this archive.
- A committed fixture suite and the `kdlith-fixtures` harness that
  drives the built command-line tool, including byte-exact goldens,
  isolated scratch directories, and a generate, compare, compile, and
  exercise oracle for generated crates.
- A contribution-level application scope: the canonical dialect names
  scope terms with the `ScopeName` vocabulary type, which changes the
  canonical dialect digest. A schema types a contribution's `scope`
  field with a reference type over its own scope declarations, so the
  link stage resolves it and an unknown or duplicated scope fails
  closed with the reference diagnostics.
- The shipped `layered` resolver partitions resolution by scope: layer
  precedence applies within each target-and-scope partition, and each
  `resolution` item carries its scope.

### Fixed

- Schema roots without a version are rejected on every reading path.
- Overlapping argument bindings are reported on every definition
  path, with one diagnostic code per rejected construct.
