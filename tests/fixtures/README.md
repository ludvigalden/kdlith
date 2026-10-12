# kdlith fixture suite

This directory is the committed fixture suite for kdlith. The
`kdlith-fixtures` harness (`tests/tools/kdlith-fixtures`) runs every scenario
in it against a built `kdlith` binary. The harness, not kdlith, judges each
run: kdlith's output is evidence and never interprets an expectation.

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
| `dialect/`    | Dialect definitions: loading, validating, and linking dialect constructs       |
| `schemas/`    | Schema definitions: types, bindings, context effects, and processing rules     |
| `processing/` | Processing data documents end to end, plus CLI smoke scenarios                 |
| `codegen/`    | Processor generation: generated output, ownership, and drift checks            |
| `invalid/`    | Negative controls: inputs that must be rejected with declared diagnostic codes |

`processing/smoke-help` asserts that `kdlith --help` exits `0` and prints its
usage text. It keeps the suite non-empty and exercises the harness end to end.

## Scenario format v0

`scenario.json` is a JSON object parsed strictly: an unknown key at any level
is a load error, and a load error fails the scenario rather than skipping it.

| Key              | Required | Type             | Meaning                                            |
| ---------------- | -------- | ---------------- | -------------------------------------------------- |
| `binary`         | yes      | string           | Must be `"kdlith"`, the only supported value at v0 |
| `description`    | no       | string           | Free-form description                              |
| `args`           | yes      | array of strings | The arguments passed to the binary                 |
| `expect`         | one of   | object           | Positive control                                   |
| `expect-invalid` | one of   | object           | Negative control                                   |

Exactly one of `expect` and `expect-invalid` must be present.

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
