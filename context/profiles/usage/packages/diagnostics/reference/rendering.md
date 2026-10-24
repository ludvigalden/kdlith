# Rendering

## Standard error

Text on standard error is the diagnostic presentation the CLI
offers. A diagnostic rendered from a record is one
`error[CODE]: message` line, the location (`--> path:line:column`),
optional indented `note:` lines carrying labels, and a `help:` line
carrying the code's registered correction:

```text
error[KDLITH-BND001]: unknown node crad under deck
 --> input.kdl:2:5
 note: did you mean card? (input.kdl:2:5)
 help: Remove the node or use one the schema binds here
```

Notes name the nearest admitted names when one lies within an edit
distance of 2 (deterministic: distance, then name), point at a child
meant to nest under its sibling, or name the property/child spelling
a required field accepts. Argument faults (`KDLITH-CLI001` through
`CLI004`) print before any record exists and carry no help line.

## JSON form

The machine-readable form is the data path's JSON item stream on
standard output, whose `diagnostic` items carry the whole record:
code, severity, source path, span, line and column, labels with
their positions, related records, the correction, and, when the code
documents them, `facts`: named values a consumer reads without parsing
the message (`KDLITH-LOC009` carries `source`, `layer`, `declared`,
`directory`, and `shadowed`; `KDLITH-LOC010` carries `source`, `kind`,
and `digest`). `vet data --diagnostics json` also writes link
diagnostics there after every other item.

## Exit strata

Warnings and notes alone exit 0; under `--warnings-as-errors` a
warning exits 1, a note never does. Errors exit 1. Argument faults exit 2. Views
keep show-then-fail: everything printable is written first.
