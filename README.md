# kdlith

kdlith provides Rust primitives for typed, streaming KDL processing and Rust
processor generation.

A dialect definition describes a family of KDL documents; a schema definition
describes one kind of data document within that dialect. kdlith reads these
definitions and either processes data documents directly or generates a typed
Rust processor for them.

> Status: early development, pre-release. Every `kdlith` command (`gen`,
> `vet`, `export`, `inspect`, `list`, `read`, `cache`, `changelog`) is
> implemented, and `gen` generates compilable processor crates from dialect
> and schema definitions. Interfaces may still change between releases.

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

## License

MIT. See [LICENSE](LICENSE).
