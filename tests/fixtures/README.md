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
| `harness/`    | Harness self-checks: the scratch protections must fail closed                  |

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
`schema.kdl`, the import-mechanics scenarios share `imports.kdl` (whose
`use` node carries the repeated policy, the alias, and the projected
slots), the slot-duplicate-policy scenarios (`slot-*`, `merge-*`) share
`policies.kdl` (one slot per policy arm, a layered slot with its declared
layer order, and children scopes and a projecting import over it), the
profile-model scenarios (`profile-*` other than `profile-bind`) share
`profiles.kdl` (an `extends` chain, a default, and one match rule of each
kind, so each run's selected profile shows in which root binds), and so
do the `--profiles` set scenarios (`profiles-*`), while
`processing/bind/nullable/` and
`processing/bind/value-expr/` carry their own schemas for the `nullable<T>`
type constructor and the `ValueExpr` scalar, and the focused directories
`generic-types/`, `codecs/`, `union-dispatch/`, `sensitivity/`,
`expansion/`, `dependent-binding/`, and `case-dispatch/` (the seven
types/bindings mechanisms) plus `descendants/`, `sequence/`,
`contributions/`, `computed-default/`, `source-context/`,
`source-context-per-file/`, `accumulate/`, `import-alias/`, and
`identity-reference/` (the documents and context mechanisms) each carry a
schema proving one mechanism end to end. `invalid/data/` holds data
documents bound against the shared schema, each rejected with its declared
code; the mechanism-negative scenarios there carry the matching focused
schema from their positive counterpart. The source-area scenarios there
(`source-*`) instead name one document to `kdlith read file` and pin how
reading it fails: past a limit, missing, a directory, or not UTF-8. The
`anchor-undeclared*` scenarios (and `invalid/export/anchor-undeclared/`)
pass an `--anchor` no discovery grammar of the schema declares, over a
document that imports nothing, so the rejection comes from the anchor alone;
one scenario per data verb pins the shared rejection.

`schemas/vet/import-anchor/` holds a schema whose own import is a
grammar-admitted expression (`$root/*.kdl`) and the two documents it
matches under `parts/`. The definition-path anchor scenarios load it with
`--anchor root=...`: `vet schema` there, `list types`, `gen schema`, and
`vet data` (`processing/bind/schema-import-anchor/`) name it through the
suite placeholder, and `schemas/inspect/import-anchor/` carries its own
copies so the shown paths stay relative. `schemas/vet/import-anchor-literal/`
imports the same documents by literal path and reports the same digest,
the oracle for the expansion. The negatives load it without the anchor
(`invalid/schemas/*import-anchor-unsupplied/`), with an undeclared one
(`invalid/schemas/anchor-undeclared/`), or declare profiles so no grammar
is bound while loading (`invalid/schemas/import-anchor-profiled/`).

`dialect/canonical-inspect/`, `dialect/entrypoint-inspect/`, and
`schemas/inspect/` run `kdlith inspect`, which reads definitions through
the reference processor, prints each typed item as one JSON line, and then
checks the definition as `vet` does. Each scenario asserts ordered runs of
the expected declaration items and scope completions. The four single-file
`dialect/canonical-inspect/` scenarios show one canonical contributing
document's items and then exit 1, because a contributing document alone
does not link; `dialect/canonical-inspect/merged` reads all four and exits
0. `invalid/schemas/inspect-*` hold schema definitions the reference
processor rejects while binding them, each with its declared code; when
the reader rejects an input, the compile stage does not run.
`processing/inspect-rejects-*` each inspect, through `inspect schema`, the
same input a `vet schema` scenario rejects with a compiler-only code:
the items still stream to stdout, and the run then fails with that code.
`invalid/schemas/include-short-form/` and
`invalid/schemas/inspect-include-short-form/` pin the loud rejection of the
`include` short form: every CLI reading path goes through the typed reader,
where no role or field binding consumes an `include` child, so `vet schema`
and `inspect schema` both reject it with `KDLITH-BND001` instead of dropping
the included members. [COVERAGE.md](COVERAGE.md) carries the declared elision
row for the splice itself, with its reopen condition.

`processing/sch003-span-vet/` and `processing/sch003-span-inspect/` pin the
span of one diagnostic: an overlapping binding (`KDLITH-SCH003`) is reported
at the node declaration, at the same span on both reading paths, because the
reader-path binder check fires first in `vet`.

`processing/cli/` holds the CLI-surface contracts: `export` without its
required arguments, `--version`, the `help` verb, the exit-2 rejections of
a non-text `--stdout-format` and an out-of-band `--verbosity`, the bare
no-arguments invocation, and the argument-error stratum. Every argument
rejection exits `2` under its registered code: the `cli001-*` scenarios
pin `KDLITH-CLI001` across the argument forms, the `cli002-*` scenarios
pin `KDLITH-CLI002` on `gen`'s exclusive `--dry-run`/`--check` pair, and
`cli003-export-format` pins `KDLITH-CLI003`. Each of these holds its
streams byte for byte, so a change to clap's wording or to the command
grammar means regenerating their goldens from the binary's output.

The `*-dialect` twins under `processing/list/`, `processing/cache/`,
`processing/export/`, and `processing/read-data-found-dialect/` run a data
verb with `--dialect` naming the dialect-generation entrypoint and reuse
their bundled-default twin's goldens unchanged, and
`invalid/dialect/processor-mismatch-*` prove each verb consumes the
override.

`processing/export/` holds the `export` contract scenarios: `--emit
roots` and `--emit <TYPE>` selection over an entry document that imports
another, `--output`, the stream discipline (stdout carries only the data
lines), both delivery modes including a provisional run that fails
mid-stream, and the `--sensitive` policies. Each scenario carries its
own input copies, so the provenance envelope's source paths and revision
digests are stable, and pins its streams and written files byte for
byte with committed goldens. Editing an input changes its revision
digest, so regenerate the goldens with it. `invalid/export/` holds the
`KDLITH-EXP001`-`003` negative controls.

`processing/round-trip/` runs the full chain entrypoint to dialect to
schema to data over the committed catalog schema: `vet data` and
`inspect data` each bind a catalog-shaped data document end to end.

`processing/read-data-entry/` pins the `read data --entry` form: the entry
document's declarations compose with the read document, observed through
the bound record's source index. `processing/read-data-entry-import/` pins
the execution-backed composition: the entry's own import executes in the
same session, so a record reached through it is found under its own source
identity. The data verbs take host inputs the fixtures exercise:
`--anchor NAME=PATH` supplies a discovery anchor
(`processing/bind/import-anchor`), `--handler NAME` installs a built-in
context handler (`processing/bind/handler-effect`), and `--profile NAME`
names the invocation's profile (`processing/bind/profile-bind`).

`processing/resolve/` holds the `--resolve-from FILE` scenarios over
the shared `processing/resolve/schema.kdl`, whose computed defaults each
name one dependency, the key a committed `values.kdl` resolves them by:
non-secret values resolve unredacted while a resolved secret is redacted
unless `--sensitive emit` says otherwise, and a missing key fails with
`KDLITH-BND009`. The `processing/cache/resolve-*` scenarios pin the
resolved values' place in the cache identity: an unchanged value set
hits, a changed one misses and re-binds, and a document for which a
secret was resolved is never cached.

`processing/resolution/` holds the contribution-resolution scenarios.
`settings.kdl` declares an option declaration and a setting contribution
whose target is a `ref<Option>`, whose layer is bound from the `layer`
property, and whose `follows` field references another option; each
policy schema beside it (`replace.kdl`, `union.kdl`, `last-wins.kdl`,
`missing-allow.kdl`, `no-layers.kdl`) imports it and declares one
resolution policy naming the shipped `layered` resolver. Each arm
scenario runs `read data` on its own data document and pins the target's
`resolution` item byte for byte; `order` and `order-shuffled` run `vet
data` over the same contributions in different orders and pin the whole
stream, whose resolution items follow in sorted target order.
`schema-vet`, `schema-list-types`, and `schema-inspect` pin that the
schema verbs load a schema naming `layered`. The `invalid/data/resolution-*`
negatives pin the resolver's refusals (`KDLITH-DLC010`) and a policy
naming a resolver no processor installs (`KDLITH-DLC006`).

`schemas/vet/import-repeat-coalesce/` and
`schemas/vet/import-repeat-instantiate/` pin the two non-default
repeated-import policies on the schema path;
`dialect/entrypoint-vet/import-repeat-coalesce/` and
`dialect/entrypoint-vet/import-repeat-instantiate/` pin them on the
dialect path, where the conform walk now admits the `repeated=` property.
`invalid/schemas/deferred-mechanism/` and
`invalid/schemas/inspect-deferred-mechanism/` pin the retirement of the
deferral marker: its spelling rejects as an unbound node on both reading
paths. `schemas/vet/profile-declaration/`, `schemas/vet/discovery-local/`,
`schemas/vet/import-alias/`, `schemas/vet/identity-reference/`, and
`schemas/vet/secret-provider/` pin the declaration surface of the expanded
documents mechanisms.

`harness/scratch-fail-closed` proves the harness's own scratch
protections fail closed: after the run, the harness plants a symlink and
a foreign file inside the materialized copy and then classifies the copy
against the scenario source, requiring both plants to be reported as
foreign entries — the link recorded and never followed, neither file
silently deleted. It exercises the same declaration format as any
positive scenario plus the `scratch-check` block described below.

`parse/kdl-spec/` and `invalid/kdl-spec/` hold the official KDL v2 test suite,
one scenario per upstream case: valid documents must exit `0` under
`kdlith read input.kdl`, and invalid ones must be rejected with the declared
code. Each negative control keeps the upstream input byte-for-byte in
`input.kdl` and declares its codes in a separate `expect-invalid.txt`, so no
header line alters the document. The pinned upstream commit, the derivation,
and the declared elisions are in `parse/kdl-spec/PROVENANCE.md`.

`parse/indentation/` and its `invalid/data/` negatives pin a KDL v2 rule the
upstream suite leaves implicit: indentation carries no structure. A newline
ends a node and children exist only inside `{ }`, so an indented line after a
brace-less node is a sibling, not a child. Each input is a spec example with
only the construct under test removed.

## Codegen

`codegen/` holds the generation scenarios. `gen dialect` generates the
committed super-schema entrypoint's processor crate end to end into a
scratch `out/` directory inside the materialized copy — never the
reference crate — and `gen schema` does the same for a committed schema
definition. `--dry-run` prints the generator-owned plan while the
scratch check proves nothing was written; `--check` compares against a
destination holding an injected difference and classifies missing,
changed, and stale generator-owned files as `KDLITH-GEN002`; and a
writing run into a destination holding an unowned file at a
generator-owned path is refused `KDLITH-GEN001` before anything is
written. The reference-generation oracle is
`codegen/gen-dialect-oracle`: a multi-step codegen-oracle scenario that
generates the entrypoint's processor twice into scratch trees the
harness requires byte-identical, compares the generated
`src/**/*.rs` against `pkg/rs/superproc` excluding only the
generated-marker header line, compiles the generated crate in a scratch
workspace materialized from the scenario's committed template, and
exercises the compiled processor's schema reading — every step judged
by the harness and fail-closed on any difference. The schema-form
companion is `codegen/gen-schema-oracle`: the same step chain over a
committed schema definition (no reference comparison — the reference
processor is the dialect form's oracle), with the exercise reading a
committed data document through the generated typed reader and
requiring the `KDLITH-BND008` rejection a value outside the schema's
enum type produces. `codegen/gen-schema-ref-import` runs that chain over
the committed catalog schema itself, which imports a second document and
holds a `ref<T>` field: its exercise reads the schema by suite path
(`KDLITH_FX_SUITE`), never a copy, and resolves each reference against
the reading's linked declarations. `codegen/gen-schema-resolver` runs it
over a schema with computed defaults and a secret provider: its consumer
crate registers a resolver reading a committed values file through the
generated `reader_with_resolver`, and the exercise pins the resolved
records, the context each call receives, the provider's command list
arriving as unexecuted data, and the `KDLITH-BND009` rejection of a
value the file does not hold.

## Scenario format v0

`scenario.json` is a JSON object parsed strictly: an unknown key at any level
is a load error, and a load error fails the scenario rather than skipping it.

| Key              | Required | Type             | Meaning                                            |
| ---------------- | -------- | ---------------- | -------------------------------------------------- |
| `binary`         | yes      | string           | Must be `"kdlith"`, the only supported value at v0 |
| `description`    | no       | string           | Free-form description                              |
| `kind`           | no       | string           | `"codegen-oracle"` selects the multi-step kind below; absent means the single-invocation shape |
| `args`           | yes*     | array of strings | The arguments passed to the binary (see below)     |
| `expect`         | one of*  | object           | Positive control                                   |
| `expect-invalid` | one of*  | object           | Negative control                                   |

\* required for the single-invocation shape. A `codegen-oracle` manifest
instead requires `entrypoint`, `crate`, and `steps` (below) and carries
neither `args` nor `expect`/`expect-invalid`.

Exactly one of `expect` and `expect-invalid` must be present in the
single-invocation shape.

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

`{fx-suite}` is the only placeholder an `args` entry may carry (a
codegen-oracle manifest may also use `{fx-repo}`, below). Any other
`{fx-` token in `args` is a load error, so a misspelled placeholder
fails the scenario instead of reaching the binary as a literal path. The
expansion is plain text substitution: the harness interprets nothing
else in `args`.

### Multi-step kind: `codegen-oracle`

A `"kind": "codegen-oracle"` manifest declares an ordered multi-step run
over processor generation. Every judgement stays harness-side: the
processor under test generates, and the harness compares, compiles, and
exercises. The keys beyond `binary` and `description`:

| Key          | Required | Type             | Meaning                                            |
| ------------ | -------- | ---------------- | -------------------------------------------------- |
| `form`       | no       | string           | `"dialect"` (default) or `"schema"`: the `gen` form the generate step drives |
| `entrypoint` | yes      | string           | The definition entrypoint; both placeholders allowed |
| `crate`      | yes      | string           | The crate name generation publishes                |
| `steps`      | yes      | array            | The ordered steps; never empty — a zero-step run is a load error |

Each entry of `steps` is an object carrying exactly one step kind:

- `{ "generate": "out" }` — run `kdlith gen <form> <entrypoint> --crate
  <crate> --out <out>`; a nonzero exit fails the scenario.
- `{ "determinism": ["tree-a", "tree-b", ...] }` — require every named
  tree to hold exactly the same files with the same bytes; a difference
  names its path.
- `{ "compare": { "generated": "...", "reference": "..." } }` — require
  the generated tree's `**/*.rs` files to match the reference tree's
  byte for byte, excluding exactly the generated marker header line
  (the marker, its newline, and the blank separator); a missing, extra,
  or differing file fails the scenario naming the file.
- `{ "compile": { "workspace": "ws", "copy": {...}, "command": [...] } }`
  — copy the declared repository crates (`copy.crates`, names under
  `pkg/rs`) into `<ws>/rs/` and, when `copy.kdl` is true, `pkg/kdl`
  into `<ws>/kdl/`, then run the declared command; a nonzero exit fails
  the scenario with the command's streams in the report.
- `{ "exercise": { "command": [...], "stdout-contains": [...],
  "stderr-contains": [...] } }` — run the declared command and judge its
  exit and stream substrings, as a positive control does.

The `{fx-repo}` placeholder expands to the repository root above the
suite (also exported as `KDLITH_FX_REPO` to the run's subprocesses), so
a compare step can name the read-only reference tree. The workspace a
compile step builds exists to satisfy the generated crate's own pinned
layout — its `../<name>` path dependencies and its
`include_str!("../../../kdl/...")` seed includes — so the crate
directories land at `<ws>/rs/` and the schema tree at `<ws>/kdl/`, the
positions the generated manifest already assumes. The scenario's
committed template (its `ws/` directory) carries the workspace manifest
and any consumer crate the exercise step runs.

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
| `stdout-golden`   | absent  | A committed file the whole of stdout must equal byte for byte |
| `stderr-golden`   | absent  | A committed file the whole of stderr must equal byte for byte |
| `files`           | absent  | Object mapping a path the run leaves in the scenario copy to a committed golden it must equal byte for byte (a regular file, never a link) |
| `absent`          | absent  | Paths the run must not leave in the scenario copy |
| `scratch-check`   | absent  | Violations the harness must plant and then classify itself (see below) |

#### Scratch check: `scratch-check`

The scenario format cannot plant a violation (materialization refuses
symlinks and special files), so the `scratch-check` block is a
harness-internal hook: after the processor run, the harness plants the
declared entries inside the materialized copy and then classifies the
whole copy against the scenario's source directory with a no-follow
walk.

```json
"expect": {
  "exit": 0,
  "scratch-check": {
    "plant": [
      { "path": "planted-link", "kind": "symlink", "target": "/etc/passwd" },
      { "path": "planted-foreign.txt", "kind": "file" }
    ]
  }
}
```

Each plant names a `path` (relative, inside the scenario, not already
present in the scenario source — a planted violation must be foreign to
it) and a `kind`: `"symlink"` with a non-empty `target`, or `"file"`
(which takes no `target`). The check passes only when the classifier
reports every planted entry as a foreign entry of its planted kind,
reports nothing else, and the captured tree holds the planted link
target, the planted file content, and a fresh mtime — all observed
without following the link. If the classification is ever neutralized,
the expected foreign-entry reports go missing and the scenario fails,
which is what makes it a detection meta-fixture.

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

A codegen-oracle run also exports `KDLITH_FX_REPO`, the repository root
above the suite, to every subprocess its steps start.

The `fx-env` helper in `bin/` prints those resolved paths. It fails when run
outside a harness run.

A failure report lists every unmet assertion, followed by the captured stdout
and stderr. After the run, the harness removes the scratch directories
through a no-follow, descriptor-relative remover: a symlink is unlinked as a
link and never traversed, only the run's own directory is torn down (sibling
runs are untouched), and anything that cannot be removed — or any foreign
file left beside a kept failure — is printed on stderr as a leftover rather
than silently deleted. The only survivors are failed scenarios kept with
`--fx-keep-failed`.

## Adding a scenario

1. Choose the category, then create `<category>/<scenario-name>/`.
2. Write `scenario.json` with `binary`, `args`, and either `expect` or
   `expect-invalid`.
3. For a negative control, add the input file and make its first line the
   `# expect-invalid:` header.
4. Run the suite. To run just the new scenario, add `--fx-filter <name>`.
