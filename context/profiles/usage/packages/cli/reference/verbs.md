# Verbs and exit codes

## Definition verbs

`kdlith vet schema FILES...` compiles schema-definition documents
under the bundled meta-schema, or under the one `--meta-schema FILE`
names. The meta-schema itself, the schema named `schema`, is vetted
the same way and is recognised by its root: `kdlith vet schema
pkg/kdl/schemas/schema/schema.kdl`. Compilation resolves imports,
links every reference, and walks the documents through the
meta-schema's own bindings; any error exits 1.

`kdlith list types|nodes SCHEMA` prints a definition's types or node
declarations. `kdlith list items` lists the items a data reading
yields.

## Data verbs

Every data verb takes `--schema SCHEMA` and one or more entry
documents:

```sh
kdlith vet data --schema shop.kdl orders.kdl
kdlith inspect data --schema shop.kdl --view bound orders.kdl
kdlith read data --schema shop.kdl --entry catalog.kdl orders.kdl
kdlith cache refresh --state .kdlith --schema shop.kdl orders.kdl
kdlith export --schema shop.kdl --emit roots --format ndjson orders.kdl
```

`export --lazy` binds only the nodes the emitted type can come from
and links nothing, so it requires `--emit TYPE` and `--scope local`;
without either it is `KDLITH-CLI001`. Every data verb takes
`--threads N`.

`read data` reads the last entry document and prints one record
identified by `--record`; earlier entries bind so imports and
numbering match. `--state DIR` restores the record from a snapshot
`cache refresh` wrote instead of replaying. `--scope local|complete|NAME`
(default `complete`) selects the partition the lookup consults: every
`(target, scope)` partition the resolver settled under `complete`, the
scope-less partition under `local`, and only the partitions of scope
`NAME` under any other kebab-form name (a malformed name is
`KDLITH-CLI001`); a record is found under each, and the scope never
gates the exit.

## fmt

`kdlith fmt file|schema|data|auto FILES...` writes documents in the
canonical layout, replacing each file whose text differs. `file` needs
no schema; `schema` formats schema definitions under the layout rules
of the bundled meta-schema (or `--meta-schema FILE`); `data --schema
SCHEMA` formats data documents under that schema's. A directory names
every `.kdl` file under it, its hidden entries passed over; `-` reads
standard input and prints to standard output.

`auto` formats documents of many schemas in one run, as a tool that
knows which schema governs which file asks (relmap's `core.kdl-format`
check, from a map's classes). Each document is read under the schema
the nearest `--schema=FILE` before it names, and `--schema=` names
none; a document under no schema is formatted as a schema definition
when its first node is `schema`, else by the layout alone. Each schema
is compiled once. Options come before the documents: any later
argument that starts with `-` and is neither `-` nor `--schema=FILE`
is `KDLITH-FMT004`.

```sh
kdlith fmt file map/
kdlith fmt data --schema shop.kdl --check orders.kdl
kdlith fmt schema --stdout shop.kdl
kdlith fmt auto --check --schema=shop.kdl orders.kdl --schema= shop.kdl notes.kdl
```

- `--check` writes nothing and reports each document whose text
  differs (`KDLITH-FMT001`, exit 1); `--stdout` prints each formatted
  document instead of writing it. The two exclude each other.
- `--width N` and `--content-width N` set the line limits for the
  whole run, over the schema's: a line at indentation `i` ends by
  column `min(width, i + content-width)` wherever a break can fall
  (defaults 100 and 80).

The layout depends only on what the document holds: its nodes,
values, and comments, where its empty lines fell, and which strings
were written bare. Formatting is idempotent, never changes a value,
and is verified before anything is written: the result is read back
and compared with the document, and a layout that would not read back
as the document is reported (`KDLITH-FMT002`) and nothing is written.
A file is replaced in one step (written beside it, then renamed over
it); a write that fails is `KDLITH-FMT003`. A document that is not
KDL v2 fails as `read file` fails it. The layout itself, and the
schema rules that steer it, are in the `authoring` package's
`reference/layout.md`.

## sources

`kdlith sources plan --sources FILE [--store DIR]` prints one JSON line
per origin source: `source`, `origin` (`git`, `rev`, `path`), `pinned`
(its digest or `false`), `target` (its path under the store), and
`materialised`. `sources verify` re-hashes each stored origin tree,
one line per origin with `status` `verified`, `unpinned`, `mismatch`
(`KDLITH-LOC007`, exit 1), or `absent` (`KDLITH-LOC006`, exit 1) and
the tree's `digest`. `sources list` prints every source with its
`kind`, who `declared` it (`manifest` or `host`), `pinned`, the
`digest` of the tree a reading reads (the value to pin it by), and
`overridden`. Nothing is fetched.

## gen

`gen schema` requires `--out DIR` before the definition documents;
`--crate NAME` names the published package and defaults to the
schema's name with the `-kdl` suffix. `--dry-run` prints the
plan and writes nothing; `--check` compares the generated tree
against `--out` and reports drift (`KDLITH-GEN002`) without writing;
the two modes exclude each other. A write over a path the run's
manifest does not own is refused (`KDLITH-GEN001`). Both forms take
the packaging flags `--kdlith-path PATH` / `--kdlith-version VER`
(mutually exclusive) and `--kdlith-workspace`, which excludes the
other two; with no packaging flag the manifest names the kdlith
crates as sibling paths beside `--out`, and a writing run whose
sibling layout resolves to no kdlith crate is refused before
anything is written (`KDLITH-GEN008`, one record per unresolvable
dependency). The flagless default is a contract, not a fallback: it
resolves only when the generated crate sits beside the kdlith crates
themselves, and a consumer generating anywhere else passes
`--kdlith-path` to point at them (the refusal message says so).
`gen schema` additionally takes `--root`, `--source`, and
`--handler`.

`gen workspace --sources FILE [--root DIR] [--kdlith-source NAME]`
checks a consuming cargo workspace against the kdlith source the
sources manifest declares (default name `kdlith`): it prints the
`[workspace.dependencies]` lines a generated schema reader needs, each
`kdlith-<crate> = { git = "<git>", rev = "<rev>" }`, reports a line
that is missing, different, or local (`KDLITH-GEN006`), and writes the
owned `.cargo/kdlith-local.toml`, which cargo applies only through
`cargo --config`. A `.cargo/config.toml` an earlier kdlith wrote is
warned about (`KDLITH-GEN009`) and left alone. `--dry-run` and
`--check` behave as on the other forms; see the typed reader's
generated-crates reference for the patch file and the one-time cache
seed.

## changelog and --version

`kdlith changelog [--target TARGET]` prints one of the relmap export
targets the binary embedded when it was built, and `kdlith --version`
prints the version it presents. The build asks relmap for both when it
finds relmap (`RELMAP_BIN`, or `relmap` on `PATH`; `RELMAP_BIN=` set
empty builds without it): the embedded targets are relmap's full export
of the repository's `changelog/` records — `markdown` (the default,
the same text as the root `CHANGELOG.md`), `ndjson` (one JSON record
per line), `index` (`changelog.index.json`), and `metadata`
(`changelog.metadata.json`) — and the version is what
`relmap version tools/kdlith` reports. Each target is written out
unchanged, byte for byte. An unknown target is an argument fault
(exit 2). A build without relmap still succeeds: `--version` prints
the Cargo version, and `changelog` fails with `KDLITH-CHG001`
(exit 1) on every target. A relmap that is found but fails fails the
build.

## Exit strata

- 0 — success, including warnings-only runs (a warning fails the run
  under `--warnings-as-errors`; a note never does)
- 1 — a diagnostic or gate failure the run reports (binding, link,
  export delivery, drift check, generation refusal, cache fault,
  changelog data unavailable, an unformatted document under
  `fmt --check`)
- 2 — an argument fault (`KDLITH-CLI001` through `KDLITH-CLI004`),
  reported before any document is read

Inspect views keep the show-then-fail rule: every printable item or
view line is written, then the exit follows the diagnostics.
