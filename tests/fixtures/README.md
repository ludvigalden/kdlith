# kdlith fixture suite

This directory is the committed fixture suite for kdlith. The
`kdlith-fixtures` harness (`tests/tools/kdlith-fixtures`) runs every scenario
in it against a built `kdlith` binary. The harness, not kdlith, judges each
run: kdlith's output is evidence and never interprets an expectation.
[COVERAGE.md](COVERAGE.md) is the census of record for the suite: which
dialect constructs, diagnostic codes, and verb contracts the scenarios
exercise, and which gaps are declared elisions.

## Running the suite

Build the binary under test and the harness:

```sh
cargo build -p kdlith-bin -p kdlith-fixtures
```

Then run the suite from the repository root:

```sh
target/debug/kdlith-fixtures --fx-suite tests/fixtures --fx-kdlith target/debug/kdlith
```

Harness options:

| Option                    | Meaning                                                                      |
| ------------------------- | ---------------------------------------------------------------------------- |
| `--fx-suite <dir>`        | Suite root to discover scenarios under (required)                            |
| `--fx-kdlith <path>`      | The `kdlith` binary under test (required)                                    |
| `--fx-scratch-root <dir>` | Where runs are materialized (default `target/scratch`, relative to the cwd)  |
| `--fx-filter <substring>` | Run only scenarios whose name contains the substring                         |
| `--fx-list`               | Print the selected scenario names and their count, then exit without running |
| `--fx-keep-failed`        | Keep the scratch copy of each failed scenario and print where it is          |

Value options accept `--fx-x value` or `--fx-x=value`. An unknown `--fx-*`
option, a missing or empty value, or a repeated option is a usage error. Every
other argument goes to the underlying libtest-style trial runner unchanged, so
its positional filter, `--skip`, and `--list` also work.

Exit codes: `0` when every selected scenario passes, `1` on any failure, and
`2` on a usage error. The run fails closed: it exits `1` when the suite root is
missing or unreadable, when the suite has zero scenarios, when `--fx-filter`
selects none, or when zero scenarios end up running. An empty run never passes.

## Layout

A scenario is a directory that contains a `scenario.json` manifest. Discovery
walks the suite root recursively:

- A directory with `scenario.json` is a scenario. The harness does not look
  inside it for further scenarios.
- A directory without `scenario.json` that has subdirectories is a category.
  The harness descends into it.
- A directory with neither a manifest nor subdirectories is reported as "not a
  scenario" on stderr and not run. An empty category holding only a
  `.gitkeep` is reported this way.

A scenario's name is its path relative to the suite root, joined with `/`
(for example `processing/smoke-help`). Names are the trial names, the
`--fx-filter` match target, and the scratch subpath. Scenarios run in sorted
name order. Symlinked directories are not followed.

Categories:

| Directory     | Intended use                                                                   |
| ------------- | ------------------------------------------------------------------------------ |
| `parse/`      | KDL v2 syntax: documents `kdlith read` must accept                             |
| `dialect/`    | Dialect definitions: loading, validating, and linking dialect constructs       |
| `schemas/`    | Schema definitions: types, bindings, context effects, and processing rules     |
| `processing/` | Processing data documents end to end, plus CLI smoke scenarios                 |
| `codegen/`    | Processor generation: generated output, ownership, and drift checks            |
| `invalid/`    | Negative controls: inputs that must be rejected with declared diagnostic codes |

`processing/smoke-help` asserts that `kdlith --help` exits `0` and prints its
usage text. It keeps the suite non-empty and exercises the harness end to end.

`dialect/canonical-read/` holds one scenario per canonical dialect definition
file under `pkg/kdl/schemas/dialect/`. Each runs `kdlith read` on the
committed file through `{fx-suite}` and must exit `0`. Adding a canonical
dialect file means adding its scenario here.

`dialect/entrypoint-read/` holds one scenario per dialect-generation fixture
entrypoint under `contracts/super-schemas/`. An entrypoint selects the
canonical dialect definition files through `import` nodes and declares
nothing of its own. Each scenario runs `kdlith read` on the committed
entrypoint through `{fx-suite}` and must exit `0`; it proves the entrypoint
parses, not that its imports resolve.

`dialect/canonical-vet/` and `dialect/entrypoint-vet/` run `kdlith vet
dialect` on the same committed files. Each canonical file must pass
`--scope local` alone. The four files together, and the entrypoint
through its imports, must pass complete checking and print the same
digest. `schemas/vet/` holds schema definitions that must pass
`kdlith vet schema` under the canonical dialect. `invalid/dialect/` and
`invalid/schemas/` hold broken definitions, each rejected with its
declared code.

`dialect/canonical-bind/` and `dialect/entrypoint-bind/` run `kdlith vet
data` with the canonical dialect as the schema, binding each committed
file as a data document of the dialect. `processing/bind/` holds small
schemas and scenarios that bind data documents against them, each asserting
the ordered JSON-line item stream on stdout; most scenarios share one
`schema.kdl`, while `processing/bind/nullable/` and
`processing/bind/value-expr/` carry their own schemas for the `nullable<T>`
type constructor and the `ValueExpr` scalar. `invalid/data/` holds data
documents bound against the shared schema, each rejected with its declared
code.

`dialect/canonical-inspect/`, `dialect/entrypoint-inspect/`, and
`schemas/inspect/` run `kdlith inspect`, which reads definitions through
the reference processor and prints each typed item as one JSON line.
Each scenario asserts ordered runs of the expected declaration items and
scope completions. `invalid/schemas/inspect-*` hold schema definitions
the reference processor rejects while binding them, each with its
declared code. `processing/inspect-accepts-*` pin the reader-path
decision those negatives imply: each reads, through `inspect schema`,
the same input a `vet schema` scenario rejects with a compiler-only
code, and asserts the reading completes and accepts it.

`processing/sch003-span-vet/` and `processing/sch003-span-inspect/` pin the
span of one diagnostic: an overlapping binding (`KDLITH-SCH003`) is reported
at the node declaration, at the same span on both reading paths, because the
reader-path binder check fires first in `vet`.

`processing/cli/` holds the CLI-surface contracts: the five stub verbs
(`gen`, `export`, `list`, `cache`, `changelog`) each fail closed with exit 1
and a "not implemented yet" message, `--version`, the `help` verb, the
exit-2 rejections of a non-text `--stdout-format` and an out-of-band
`--verbosity`, and the bare no-arguments invocation.

`processing/round-trip/` runs the full chain entrypoint to dialect to
schema to data over the committed catalog schema: `vet data` and
`inspect data` each bind a catalog-shaped data document end to end.

`processing/read-data-entry/` pins the `read data --entry` form: the entry
document's declarations compose with the read document, observed through
the bound record's source index.

`schemas/vet/import-repeat-coalesce/` and
`schemas/vet/import-repeat-instantiate/` pin the two non-default
repeated-import policies on the schema path;
`invalid/dialect/import-repeated-property/` pins the v0 limitation that the
dialect self-description rejects any `repeated=` property.

`parse/kdl-spec/` and `invalid/kdl-spec/` hold the official KDL v2 test suite,
one scenario per upstream case: valid documents must exit `0` under
`kdlith read input.kdl`, and invalid ones must be rejected with the declared
code. Each negative control keeps the upstream input byte-for-byte in
`input.kdl` and declares its codes in a separate `expect-invalid.txt`, so no
header line alters the document. The pinned upstream commit, the derivation,
and the declared elisions are in `parse/kdl-spec/PROVENANCE.md`.

## Codegen: declared elision

`codegen/` holds the reference-generation scenarios, and codegen is
skeleton-only. The generation oracle comparison — generate from
`contracts/super-schemas/dialect.kdl` into scratch, compare against
`pkg/rs/superproc` excluding only the generated marker, compile, and
exercise — is not implemented and is never reported passed. These scenarios
pin the steps that exist: the entrypoint parses and vets (to the canonical
digest), and the `gen` verb fails closed with exit 1 and a "not implemented
yet" message, writing nothing anywhere. [COVERAGE.md](COVERAGE.md) carries
the full elision table with its reopen conditions.

## Scenario format v0

`scenario.json` is a JSON object parsed strictly: an unknown key at any level
is a load error, and a load error fails the scenario rather than skipping it.

| Key              | Required | Type             | Meaning                                            |
| ---------------- | -------- | ---------------- | -------------------------------------------------- |
| `binary`         | yes      | string           | Must be `"kdlith"`, the only supported value at v0 |
| `description`    | no       | string           | Free-form description                              |
| `args`           | yes      | array of strings | The arguments passed to the binary (see below)     |
| `expect`         | one of   | object           | Positive control                                   |
| `expect-invalid` | one of   | object           | Negative control                                   |

Exactly one of `expect` and `expect-invalid` must be present.

### Argument placeholder: `{fx-suite}`

An `args` entry may contain `{fx-suite}`. The harness replaces every
occurrence with the canonicalized suite root before running the binary, so a
scenario can name a committed file outside its own directory instead of
carrying a copy of it. A copy of a canonical file would drift from the file
it stands for; the placeholder makes the scenario read the committed file
itself. For example, with the suite at `tests/fixtures`:

```json
"args": ["read", "{fx-suite}/../../pkg/kdl/schemas/dialect/types.kdl"]
```

`{fx-suite}` is the only placeholder. Any other `{fx-` token in `args` is a
load error, so a misspelled placeholder fails the scenario instead of
reaching the binary as a literal path. The expansion is plain text
substitution: the harness interprets nothing else in `args`.

### Positive control: `expect`

```json
{
  "binary": "kdlith",
  "args": ["--help"],
  "expect": { "exit": 0, "stdout-contains": ["Usage: kdlith"] }
}
```

| Key               | Default | Meaning                                   |
| ----------------- | ------- | ----------------------------------------- |
| `exit`            | `0`     | The exact exit code the run must produce  |
| `stdout-contains` | `[]`    | Substrings that must all appear in stdout |
| `stderr-contains` | `[]`    | Substrings that must all appear in stderr |

### Negative control: `expect-invalid`

```json
{
  "binary": "kdlith",
  "args": ["validate", "input.kdl"],
  "expect-invalid": { "input": "input.kdl" }
}
```

`input` is a relative path inside the scenario directory. It must not be
empty, absolute, or contain `..`. The first line of that file declares the
codes the rejection must report:

```kdl
# expect-invalid: KDLITH-SYN001, KDLITH-SYN002
```

The harness reads that line as raw text. It does not use a KDL parser, so a
malformed input still declares its codes. The rules:

- The first line must start with exactly `# expect-invalid:`. A trailing `\r`
  is tolerated.
- What follows is a comma-separated list of codes, each trimmed of
  whitespace. The list must be non-empty and contain no empty entries.
- The run must exit nonzero, and every declared code must appear in stderr.

A missing, unreadable, or malformed header is a load error, and the scenario
fails.

## How a scenario runs

For each run, the harness creates `<scratch root>/<UTC timestamp>-<pid>/`.
Each scenario directory is copied to `scenarios/<name>/` inside it, refusing
symlinks and special files. The binary then runs:

- with that copy as its working directory,
- with stdin closed,
- with the run's `bin/` helper directory first on `PATH`,
- and with `KDLITH_FX_SUITE`, `KDLITH_FX_KDLITH`, `KDLITH_FX_SCRATCH`, and
  `KDLITH_FX_SCENARIO` exported.

The `fx-env` helper in `bin/` prints those resolved paths. It fails when run
outside a harness run.

A failure report lists every unmet assertion, followed by the captured stdout
and stderr. After the run, the harness removes the scratch directories, except
any failed scenario kept with `--fx-keep-failed`.

## Adding a scenario

1. Choose the category, then create `<category>/<scenario-name>/`.
2. Write `scenario.json` with `binary`, `args`, and either `expect` or
   `expect-invalid`.
3. For a negative control, add the input file and make its first line the
   `# expect-invalid:` header.
4. Run the suite. To run just the new scenario, add `--fx-filter <name>`.
