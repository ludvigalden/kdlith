# Workspace architecture and crate boundaries

## The crate stack

Every edge below is a `[dependencies]` entry in a member manifest; a
crate depends only on crates beneath it.

```
diagnostic
  └─ model
       ├─ parse
       │    └─ format          (consumed by cli)
       └─ runtime
            └─ engine              (also parse)
                 └─ schema-kdl     (also runtime)
                      └─ codegen
                           └─ cli  (also engine, format, model, parse)
                                ├─ kdlith-bin       (binary kdlith)
                                └─ kdlith-fixtures  (also codegen)
```

| Crate               | Path                          | Role                                                                                                                                                                                                                                            |
| ------------------- | ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `kdlith-diagnostic` | `pkg/rs/diagnostic`           | The diagnostic record, the code registry (`KDLITH-<AREA><NNN>`; a code is never renumbered or reused), the response envelope, and the exit strata. Encodes nothing of its own.                                                                  |
| `kdlith-model`      | `pkg/rs/model`                | Data-only vocabulary: source coordinates, node and symbol identities, origins, bound values, and the items a session yields. No processing logic.                                                                                                |
| `kdlith-parse`      | `pkg/rs/parse`                | The streaming KDL v2 pull parser and writer, with fail-closed limits, and the syntax tree (`tree`) that keeps what the event stream drops — comments, empty lines, slashdashed text, spellings — for the formatter; it accepts exactly what the pull parser accepts, with the same errors. Derived from just-kdl and kdlite; attribution in `src/NOTICE.md` and `src/upstream/`.                                                                                     |
| `kdlith-format`     | `pkg/rs/format`               | The canonical layout `kdlith fmt` writes: a document's syntax tree (from `kdlith-parse`'s `tree`) rewritten by an optional `Policy` and laid out under the line limits, every result read back and compared with its document before it is returned. It knows no schema: `kdlith-cli` derives the `Policy` from a compiled definition. |
| `kdlith-runtime`    | `pkg/rs/runtime`              | What every processor shares, hand-written or generated: binding frames, context operations and handlers, sessions and the import invocation stack, provenance, export delivery, and the derived-state store. It has no parser.                  |
| `kdlith-engine`     | `pkg/rs/engine`               | The shared processor core: compiles definitions once into catalogs and a processing plan, binds documents through that plan, reads data (`DataReader`), links references, exports, and hosts the typed-reader machinery (`src/typed/`).        |
| `kdlith-schema-kdl` | `pkg/rs/schema-kdl`           | The hand-maintained reference processor for schema definitions, and the host of the definition loaders (`seed`, `load_meta_schema`, `load_schema`, and their `_sources` and `_with` forms).                                                    |
| `kdlith-codegen`    | `pkg/rs/codegen`              | Deterministic crate generation: one emission model per definition, written under a pinned writer profile, with the ownership manifest that guards writes and reports drift.                                                                      |
| `kdlith-cli`        | `pkg/rs/cli`                  | The clap command tree, one module per verb (`generate`, `vet`, `fmt`, `export`, `inspect`, `list`, `read`, `cache`, `sources`, `changelog`) beside the shared usage, handler, values-file, and reporting modules.                                       |
| `kdlith-bin`        | `tools/kdlith`                | The binary `kdlith`: a `main.rs` over `kdlith_cli::run()`.                                                                                                                                                                                      |
| `kdlith-fixtures`   | `tests/tools/kdlith-fixtures` | The fixture harness. It depends on `kdlith-cli` only for its embedded-sources helper (`run_with_sources`) and on `kdlith-codegen` only for the generated-marker constant its oracle comparison excludes; it never interprets its own expectations through the processor under test. |

There is no facade crate: a tool that reads a schema depends on that
schema's generated crate, which re-exports the reading, linking, and
diagnostic surface it needs.

## Where the cycle breaks

The engine compiles definition input that a reader produces; it does
not read definition syntax itself. Reading a definition needs a
processor for the meta-schema, so the loaders live one crate up, in
`schema-kdl`, and `codegen` sits above it because it loads definitions
through those loaders before it emits. The one place the engine reads
definition syntax directly is the seed bootstrap
(`compile_meta_schema`), which compiles the four canonical documents
`schema-kdl`'s build script copies in from `pkg/kdl/schemas/schema/`.
`40_engine_contract.md` ("Self-hosted compilation") states the
equivalence the seed is held to.

## Boundary rules

- **Reusable logic lives in the engine.** Anything both the reference
  processor and a generated processor need — catalogs, binding,
  linking, the data reader, the processor/definition conformance check
  (`KDLITH-GEN003`), the duplicate-argument-binding check
  (`overlap.rs`), the typed reader — belongs in `engine` or `runtime`,
  so every generated processor inherits the same behavior.
  `schema-kdl` holds only the meta-schema's own declarations plus
  glue, and it is the code-generation oracle: generating from
  `pkg/kdl/schemas/schema/schema.kdl` must reproduce it
  (`codegen/gen-meta-schema-oracle`).
- **One data path.** `TypedReader` is a typing adapter over the
  engine's `DataReader`, the reader every data verb uses, so imports,
  profiles, handlers, output policy, and the bound-result cache are
  decided once. Linking has one implementation as well: `vet data`
  and a linked typed reading call `link_reading`, and `export` calls
  `check_reading`, the same pass reporting only the failed links.
- **The formatter knows no schema.** `kdlith-format` depends on the
  parser alone; `pkg/rs/cli/src/layout.rs` turns a compiled
  definition's `layout` rules and binding conventions into the
  `Policy` it applies, and the one rule that changes a reading —
  prose folding — lives in the engine's decoder, so a reading and the
  formatter agree on it.
- **Verbs are thin.** Each `pkg/rs/cli/src/<verb>.rs` loads
  definitions, calls one engine or codegen surface, and maps the
  outcome to an exit stratum. Data goes to standard output and
  diagnostics to standard error; a data verb mirrors its diagnostics to
  standard output as JSON lines only when asked (`--diagnostics
  json`).
