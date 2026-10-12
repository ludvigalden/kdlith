# KDL v2 conformance cases

These scenarios, and their negative controls under `../../invalid/kdl-spec/`,
are derived from the official KDL test suite.

| Field    | Value                                                           |
| -------- | --------------------------------------------------------------- |
| Upstream | <https://github.com/kdl-org/kdl>, `tests/test_cases`            |
| Commit   | `89c1087d5e7f530de328f18b6a0fad54ca8ea227`                      |
| License  | CC BY-SA 4.0, <https://creativecommons.org/licenses/by-sa/4.0/> |

The upstream `input.kdl` files are copyright the KDL contributors and remain
under CC BY-SA 4.0. They are redistributed unmodified. The scenario manifests
and declared codes around them are kdlith's own.

## Derivation

Each upstream file `tests/test_cases/input/<case>.kdl` becomes one scenario
directory named `<case>`:

- `input.kdl` is the upstream file, byte for byte.
- A case without the `_fail` suffix is a positive control under
  `parse/kdl-spec/<case>/`: `kdlith read input.kdl` must exit `0`.
- A case with the `_fail` suffix is a negative control under
  `invalid/kdl-spec/<case>/`: the run must exit nonzero and report the code
  declared in `expect-invalid.txt`. The code is assigned from the registered
  triggers, not from kdlith's output: `KDLITH-SYN002` when the defect is a
  string, comment, or children block left open at the end of input
  (`unterminated_empty_node_fail` only), and `KDLITH-SYN001` otherwise.

At the pinned commit the suite holds 338 cases: 243 positive and 95 negative.
All of them are imported.

## Declared elisions

The scenarios prove accept-or-reject and the rejection code. They do not
cover:

- **`tests/test_cases/expected_kdl/`.** The upstream normalized output for
  each positive case is not imported. `kdlith read` emits no document, so the
  suite does not check that the parsed events match the expected content.
  Positive scenarios prove only that a valid document is accepted.
- **Error location.** Negative controls assert the code, not the reported
  span, line, or column.
- **`tests/benchmarks/`.** The two large upstream documents (about 35 MB in
  total) are performance material, which upstream keeps outside its test
  suite.
