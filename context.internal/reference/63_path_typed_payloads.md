# Path-typed payloads

A `value-from-path field=PATH` binding types a node's remaining
payload by the field the path its named field holds names. The
subject is identified by the node's own case dispatch — the same
`select_case` that selects required fields, never a second selection
— so one amendment-style node can type its value against a different
record type per instance.

```kdl
node set type=SetOperation parent=changes {
    field path arg=0
    field kind property=kind
    value-from-path field=path
    cases {
        case property=kind equals=module type=ModuleCard
        case property=kind equals=concept type=ConceptCard
    }
}
```

## Resolution

At bind time, once the node's fields are bound:

1. The subject type is the type the node's case dispatch selected.
   The selected type is the subject the path resolves against, not
   the type the node binds as: its required fields, closed shape,
   and constraints stay the subject's own, checked where the subject
   binds, so the checks a case selection normally brings keep off
   the value-from-path node.
2. The path string — a bare field name, or dotted segments naming
   nested object fields — walks the subject type's object fields.
   Optional wrappers unwrap for walking; an intermediate segment
   naming no object type is unresolvable.
3. The resolved field's type expression types the payload, through
   the same decode paths a static binding of that type takes: a
   scalar arm takes exactly one value and never promotes it to a
   list, a list member takes zero or more in its element type.

An object-typed path admits the child block form: the payload rides
the node's children, whose properties compose the payload map — each
checked against the resolved object's own field type, a required
field the payload omits reported missing (`KDLITH-BND004`), a name
the object does not declare unresolvable (`KDLITH-BND015`), and a
repeated name a duplicate key (`KDLITH-BND006`).

## Fail-closed and pending arms

| What fails                         | Diagnostic                                                                  |
| ---------------------------------- | --------------------------------------------------------------------------- |
| dispatch selects no subject type   | the payload stays a pending item                                            |
| path names no field of the subject | `KDLITH-BND015`                                                             |
| payload fails the resolved type    | the static-binding code for that type (`KDLITH-BND003`, `KDLITH-BND008`, …) |

A node whose dispatch selects no subject type — including one that
declares no dispatch at all — resolves nothing: its payload stays a
pending typed-binding item at the node's stream position, preserved
raw and never validated. Every value-from-path node leaves such an
item (carrying the decoded payload when it resolved, the raw one
when it did not): the bound view admits it, `read data` and export
exclude it, and typed readers carry it through.

A path-typed value is not a reference. Its item carries a reference
naming the subject type and path as its locator, but it is never
resolved, never linked, and never REF005-validated — the link
stage's pending-payload pass skips the `value-from-path` codec
entirely.

## Compile-time shape

The named field must be one the node binds (`KDLITH-SCH006`), its
type must admit only strings (`KDLITH-DLC005`), and no `value` block
may sit beside it, since nothing binds a second payload
(`KDLITH-DLC005`). Unlike a `value` block, the node may be roleless:
the payload binds as the node's typed value wherever the node binds
as a part.
