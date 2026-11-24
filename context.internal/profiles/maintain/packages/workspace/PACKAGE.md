---
description: >-
  The kdlith repository for its maintainers: the layout, the workspace
  conventions, how a change is verified, where new context belongs, and
  which design-record file to read for each part of the workspace.
binds_to_depth: main...
---

# Working on kdlith

kdlith reads schema definitions written in the meta-schema (the schema
named `schema`, which describes itself), binds data documents against
them, and generates typed Rust processor crates from them. The design
record under `context.internal/reference/` describes the workspace as
built; the committed code, the canonical meta-schema files, and
`tests/fixtures/COVERAGE.md` win wherever a description disagrees.

## Layout

```
pkg/rs/{diagnostic,model,parse,format,runtime,engine,schema-kdl,codegen,cli}/
                                  library crates
tools/kdlith/                     the CLI binary (package kdlith-bin)
pkg/kdl/schemas/schema/           the canonical meta-schema documents and
                                  their entrypoint schema.kdl
pkg/kdl/schemas/sources/          the sources-manifest schema
tests/tools/kdlith-fixtures/      the fixture harness
tests/fixtures/                   the fixture suite; COVERAGE.md is its census
context/                          the consumer module (usage profile)
context.internal/                 this development module
map/, changelog/                  relmap's map and changelog records
CHANGELOG.md                      relmap's markdown rendering of changelog/
```

## Workspace conventions

- Edition 2024, `resolver = "2"`, `rust-version = "1.85"`, every
  third-party dependency exact-pinned (`=x.y.z`) in
  `[workspace.dependencies]`, and `Cargo.lock` committed.
- Every member crate inherits the workspace clippy lints through
  `[lints] workspace = true` and sets `autotests = false`.
- There are no unit tests: no `#[test]` or `#[cfg(test)]` anywhere.
  Behavior is verified by the committed fixture suite alone, and the
  suite runs only when someone runs it; no hook runs tests.
- Commit subjects are conventional and lowercase, at most 72
  characters, with no body and no trailers (`map/git.kdl` declares
  the rule).

## Verifying a change

- `relmap check`, run from the repository root, runs every check the
  map declares (`map/run.kdl`): clippy, rustfmt, the lockfile, taplo,
  the hygiene checks, the secrets scan, and relmap's own vetting of the
  map and the changelog. The relmap-backed tasks need relmap on `PATH`
  or named by `RELMAP_BIN`; without it they are reported as skipped,
  never as passing.
- `relmap test` builds `kdlith` and the harness and runs the fixture
  suite against the binary just built; `--with heavy` adds the heavy
  codegen scenarios. The direct invocation and the scenario format are
  in `tests/fixtures/README.md`.
- A new or renamed scenario updates `tests/fixtures/COVERAGE.md` in the
  same commit.
- A change to the canonical meta-schema files follows
  `context.internal/reference/90_changing_the_meta_schema.md`.

## Where context belongs

Both context trees are committed and public, so both are written for
an outside reader.

- `context/` is the consumer module: running the CLI, authoring
  schemas and data, the typed reader, and the diagnostic surface. It
  describes only what a consumer can observe.
- `context.internal/` is for kdlith's own development: mechanism
  detail, engine contracts, generation internals, and registry
  rationale, as two-digit numbered files under `reference/`.

A feature lands with the context pages that describe it, in the tree
its audience reads.

## Read on demand

Under `context.internal/reference/`:

- the crate stack, each crate's role, and the boundary rules —
  `05_workspace_architecture.md`
- the canonical meta-schema files and their fixture coverage —
  `10_meta_schema_files.md`
- the mechanism tables: types, bindings, context, documents —
  `20_mechanisms_types.md` to `23_mechanisms_documents.md`
- what the engine and the reference processor guarantee every
  processor — `40_engine_contract.md`, `41_binder_semantics.md`
- what `gen` emits and how it is packaged —
  `42_generated_crate_contract.md`
- what `fmt` guarantees, how the layout is decided, and where the
  schema's rules enter — `43_formatter_contract.md`
- one code per rejected construct, rendering, and the registry
  history — `50_diagnostic_registry.md`
- contribution scope, merge, and payload resolution —
  `60_contribution_scope.md` to `63_path_typed_payloads.md`
- the value-resolution hook and snapshot restore —
  `70_value_resolution.md`, `80_snapshot_restore.md`
