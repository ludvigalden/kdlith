# Generated crates

## Generating

`gen dialect` and `gen schema` both require `--crate NAME` (the
published package name) and `--out DIR`, before the definition
documents. A writing run records every path it owns in
`DIR/kdlith-manifest` and refuses to overwrite an existing file the
manifest does not own (`KDLITH-GEN001`, nothing written);
generation never deletes anything.

- `--dry-run` prints the plan and each owned path, writing nothing
- `--check` compares the generated tree against `DIR`: absent is
  missing, differing bytes are changed, and a manifest-owned path
  the tree no longer generates is stale — each one `KDLITH-GEN002`
  record, any finding exits 1
- the two modes exclude each other (`KDLITH-CLI002`)

`gen schema` also takes `--root DIR` and `--source NAME=DIR` for the
schema's own import locations, `--handler NAME` to admit a built-in
context handler the schema names, and the packaging flags:
`--kdlith-path PATH` roots the crate's kdlith path dependencies at
PATH, `--kdlith-version VER` writes them as registry dependencies.
Either value is written verbatim; the two flags are mutually
exclusive. Without either, the manifest keeps sibling paths.

## Naming limits

Generation renders every declared name as a Rust identifier
(PascalCase types, snake_case fields). It fails closed, naming the
name and the reason, when a name is no Rust identifier, two of one
declaration spell the same identifier, or a type is named `Self`.
Generic instances generate under mangled names
(`Pair<String, Mode>` becomes `PairStringMode`), and two
instantiations spelling one mangled name fail closed the same way.
Such a definition still vets: the limit is the generated crate's.

## Two forms

`gen dialect` regenerates a reference-processor-shaped crate;
`gen schema` generates a domain crate. The dialect form takes none
of the schema-only flags.
