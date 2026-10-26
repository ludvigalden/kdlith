# kdlith

kdlith provides Rust primitives for typed, streaming KDL processing and Rust
processor generation.

A dialect definition describes a family of KDL documents; a schema definition
describes one kind of data document within that dialect. kdlith reads these
definitions and either processes data documents directly or generates a typed
Rust processor for them.

> Status: early development, pre-release. Every `kdlith` command (`gen`,
> `vet`, `export`, `inspect`, `list`, `read`, `cache`, `changelog`, `help`) is
> implemented, and `gen` generates compilable processor crates from dialect
> and schema definitions. Interfaces may still change between releases.

## What it does

- **Validate.** `kdlith vet dialect|schema|data` compiles a definition and
  binds data documents against it. Under the default complete scope,
  `vet data` also links every reference a declared reference type looks
  up and reports references that are unresolved, ambiguous, or missing a
  required qualifier, declarations that are duplicated, and deferred
  payloads that do not fit the declaration their reference resolved to.
- **Inspect.** `kdlith inspect --view` shows a reading seven ways:
  source, bound, context, effective, origins, plan, and references.
- **Export.** `kdlith export` writes typed records as NDJSON. Each record
  carries a provenance envelope, and an explicit policy decides how
  sensitive fields are emitted. The reading is linked under
  `--scope local|complete` (default `complete`): an unresolved reference
  fails the run, while `--scope local` exports the data as written.
- **Cache and restore.** `kdlith cache refresh` keeps cached bound results
  and per-record context snapshots in a state directory you name, and
  `kdlith read data --state` restores one record from its snapshot.
- **Generate.** `kdlith gen` emits a Rust crate whose typed reader runs the
  same reading as `vet data`. It can link a reading, resolve references
  by site, and attach each record's origin and captured context. The
  crate re-exports everything a tool needs, so a tool depends on it alone.

Usage and feature documentation live in the committed context module
under [`context/`](context); development-facing design notes live in
`context.internal/`.

## Workspace layout

| Path                          | Package             | Role                                                               |
| ----------------------------- | ------------------- | ------------------------------------------------------------------ |
| `pkg/rs/parse`                | `kdlith-parse`      | Streaming KDL v2 pull parser and source coordinates                |
| `pkg/rs/model`                | `kdlith-model`      | Identities, origins, bound values, and processing items            |
| `pkg/rs/runtime`              | `kdlith-runtime`    | Binding frames, context operations, sessions, processor interfaces |
| `pkg/rs/diagnostic`           | `kdlith-diagnostic` | Diagnostic codes, labels, and presentation-neutral structures      |
| `pkg/rs/engine`               | `kdlith-engine`     | Dialect and schema validation, linking, compiled processing plans  |
| `pkg/rs/codegen`              | `kdlith-codegen`    | Deterministic Rust crate generation from compiled definitions      |
| `pkg/rs/superproc`            | `kdlith-superproc`  | Hand-maintained reference processor for schema definitions         |
| `pkg/rs/cli`                  | `kdlith-cli`        | Commands, arguments, and output conventions                        |
| `tools/kdlith`                | `kdlith-bin`        | Command-line tool                                                  |
| `tests/tools/kdlith-fixtures` | `kdlith-fixtures`   | Fixture-suite harness                                              |

## Building

Requires Rust 1.85 or newer.

```sh
cargo build --workspace
```

To build only the command-line tool and the fixture harness:

```sh
cargo build -p kdlith-bin -p kdlith-fixtures
```

## Verification

kdlith is verified through a committed fixture suite under `tests/fixtures`,
run by the `kdlith-fixtures` harness against the built `kdlith` binary:

```sh
target/debug/kdlith-fixtures --fx-suite tests/fixtures --fx-kdlith target/debug/kdlith
```

Heavy scenarios — the codegen family's scratch-workspace compiles, marked
`heavy` in their manifests — are excluded from that default run; pass
`--fx-heavy` (or name them with an explicit `--fx-filter`) to run them
deliberately.

The suite holds 1151 scenarios: the 243 positive cases of the official KDL v2
test suite and 908 kdlith scenarios. There are no unit tests.
[`tests/fixtures/COVERAGE.md`](tests/fixtures/COVERAGE.md) maps every
construct, diagnostic code, and command contract to the scenarios that
cover it.

## License

MIT. See [LICENSE](LICENSE).
