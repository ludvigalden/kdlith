# Export

`kdlith export` writes typed records as NDJSON. Selection
(`--emit`) picks which bound items export; `export` selects bound
items only, so a pending typed binding is excluded.

## Lazy export

`--lazy` makes the export read on demand: the reading binds only the
nodes the emitted type can come from. A node whose item is of another
type, and inside which no node of the emitted type can be written, is
passed over where it opens — its text is scanned for its end, and
nothing in it is decoded, validated, or yielded. Imports are still
followed, a node whose effects reach its siblings still binds, and so
does every node on the way down to an emitted one, so each exported
record is byte for byte the record the same export without `--lazy`
writes, its node index included.

A lazy export trusts the tree for what it passes over: an error
inside a skipped node is not reported, and a skipped declaration is
not there to link against. It therefore links nothing and requires
`--emit TYPE` and `--scope local` (`KDLITH-CLI001` otherwise). Vet a
tree with `vet data`; export from a vetted tree lazily.

## Provenance envelope

Each exported record carries a provenance envelope beside its data:
the item's node (source and span), the source revisions its
locations name, a shared origin table whose entries are `literal`,
`derived` (with an operation and its inputs), or `resolved` (from the
value-resolution hook), and each field's index into that table. The
`origins` view writes the same envelope for every record,
declaration, and contribution.

## Asset values

An asset field exports as the `asset` value kind,
`{"kind":"asset","value":{"location":…,"source":…}}`, with a
`digest` where its type records one; its origin is the literal
spelling's site.

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
