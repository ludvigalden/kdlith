# Export

`kdlith export` writes typed records as NDJSON. Selection
(`--emit`) picks which bound items export; `export` selects bound
items only, so a pending typed binding is excluded.

## Provenance envelope

Each exported record carries a provenance envelope beside its data:
the item's node (source and span), the source revisions its
locations name, a shared origin table whose entries are `literal`,
`derived` (with an operation and its inputs), or `resolved` (from the
value-resolution hook), and each field's index into that table. The
`origins` view writes the same envelope for every record,
declaration, and contribution.

## Sensitivity

An explicit output policy decides how sensitive fields are emitted —
emit, omit, or redact. A `secret` field is emitted only when the
policy explicitly says emit; otherwise it is redacted
(`[redacted]`), whatever its origin, including a value the
`--resolve-from` file supplied. Diagnostic messages never name a
concealed value.

## Delivery

Under complete scope the reading is linked before delivery, and a
link diagnostic fails the run: atomic delivery publishes nothing.
Provisional delivery keeps the lines it already wrote with an
`KDLITH-EXP002` warning naming their count. Under local scope every
selected item exports with its references as written.
