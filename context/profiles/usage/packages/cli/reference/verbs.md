# Verbs and exit codes

## Definition verbs

`kdlith vet dialect FILES...` compiles dialect-definition documents.
`kdlith vet schema --dialect D FILES...` (or `--schema` alone when the
schema names its dialect) compiles schema documents. Compilation
resolves imports, links every reference, and walks the documents
through the dialect's own bindings; any error exits 1.

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

`read data` reads the last entry document and prints one record
identified by `--record`; earlier entries bind so imports and
numbering match. `--state DIR` restores the record from a snapshot
`cache refresh` wrote instead of replaying. `--scope local|complete`
(default `complete`) selects the partition the lookup consults: every
`(target, scope)` partition the resolver settled under `complete`, the
scope-less partition under `local`; a record is found under both, and
the scope never gates the exit.

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

`gen dialect` and `gen schema` both require `--crate NAME` and
`--out DIR` before the definition documents. `--dry-run` prints the
plan and writes nothing; `--check` compares the generated tree
against `--out` and reports drift (`KDLITH-GEN002`) without writing;
the two modes exclude each other. A write over a path the run's
manifest does not own is refused (`KDLITH-GEN001`). `gen schema`
additionally takes `--root`, `--source`, `--handler`, and the
packaging flags `--kdlith-path` / `--kdlith-version`; both forms take
`--kdlith-workspace`, which excludes the other two.

`gen workspace --sources FILE [--root DIR] [--kdlith-source NAME]`
checks a consuming cargo workspace against the kdlith source the
sources manifest declares (default name `kdlith`): it prints the
`[workspace.dependencies]` lines a generated schema reader needs, each
`kdlith-<crate> = { git = "<git>", rev = "<rev>" }`, reports a line
that is missing, different, or local (`KDLITH-GEN006`), and writes the
owned `.cargo/config.toml`. `--dry-run` and `--check` behave as on the
other forms; see the typed reader's generated-crates reference for the
patch file and the one-time cache seed.

## Exit strata

- 0 — success, including warnings-only runs (a warning fails the run
  under `--warnings-as-errors`; a note never does)
- 1 — a diagnostic or gate failure the run reports (binding, link,
  export delivery, drift check, generation refusal, cache fault)
- 2 — an argument fault (`KDLITH-CLI001` through `KDLITH-CLI004`),
  reported before any document is read

Inspect views keep the show-then-fail rule: every printable item or
view line is written, then the exit follows the diagnostics.
