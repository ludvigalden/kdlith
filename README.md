# kdlith

kdlith provides Rust primitives for typed, streaming KDL processing and Rust
processor generation.

A dialect definition describes a family of KDL documents; a schema definition
describes one kind of data document within that dialect. kdlith reads these
definitions and either processes data documents directly or generates a typed
Rust processor for them.

> Status: early development. The workspace layout is in place; most crates
> are skeletons.

## Workspace layout

| Path                          | Package             | Role                                                               |
| ----------------------------- | ------------------- | ------------------------------------------------------------------ |
| `pkg/rs/kdlith`               | `kdlith-kdlith`     | Public facade over the library crates                              |
| `pkg/rs/parse`                | `kdlith-parse`      | KDL event adapter and source coordinates                           |
| `pkg/rs/model`                | `kdlith-model`      | Type descriptors, identities, origins, and processing items        |
| `pkg/rs/runtime`              | `kdlith-runtime`    | Binding frames, context operations, sessions, processor interfaces |
| `pkg/rs/diagnostic`           | `kdlith-diagnostic` | Diagnostic codes, labels, and presentation-neutral structures      |
| `pkg/rs/engine`               | `kdlith-engine`     | Dialect and schema validation, linking, compiled processing plans  |
| `pkg/rs/codegen`              | `kdlith-codegen`    | Deterministic Rust crate generation from compiled definitions      |
| `pkg/rs/superproc`            | `kdlith-superproc`  | Hand-maintained reference processor for schema definitions         |
| `pkg/rs/cli`                  | `kdlith-cli`        | Commands, arguments, and output conventions                        |
| `tools/kdlith`                | `kdlith`            | Command-line tool                                                  |
| `tests/tools/kdlith-fixtures` | `kdlith-fixtures`   | Fixture-suite harness                                              |

## Building

Requires Rust 1.85 or newer.

```sh
cargo build --workspace
```

To build only the command-line tool and the fixture harness:

```sh
cargo build -p kdlith -p kdlith-fixtures
```

## Verification

kdlith is verified through a committed fixture suite under `tests/fixtures`,
run by the `kdlith-fixtures` harness against the built `kdlith` binary:

```sh
target/debug/kdlith-fixtures --fx-suite tests/fixtures --fx-kdlith target/debug/kdlith
```

## License

MIT. See [LICENSE](LICENSE).
