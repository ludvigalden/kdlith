# Per-field merge

Under `selection=union` contributed values merge deeply: objects
merge field by field and keyed collections key by key at every depth,
and a leaf both layers hold — a scalar, a list, a reference, or a
union value — takes the higher layer's value, with no diagnostic. A
key only the lower layer holds is kept, and a key only the higher
layer adds follows the held keys. A list-typed field may instead
declare `merge=union`, so a refinement adds to the lower layers' list
rather than replacing it; the declaration takes effect at the field's
own depth, on a contribution type's field and on a field of an
object a contribution nests alike. A map- or object-typed field may
declare `merge=union` too, stating the key-wise merge it already
receives. The canonical dialect spells the policy with the
`MergePolicy` vocabulary type, whose one member is `union`; a value
outside it is `KDLITH-BND008`.

```kdl
types {
    union Matcher { variant suffix Suffix; variant prefix Prefix }
    object Refiner {
        field target "ref<Option>"
        field match "optional<list<Matcher>>" merge=union
        field layer "optional<String>"
    }
}
resolution resolver=layered selection=union {
    layers user base
}
```

The shipped `layered` resolver applies the merge inside each
(target, scope) partition, after layer precedence and duplicate
treatment have settled the partition's records. Walking them from
the lowest rank to the highest, a merged field keeps the items it
already holds and appends each incoming item whose declared shape no
held item shares, in the order the incoming record lists them; a key
the lower records lack is taken as written, and every other field
still overwrites. Declared-shape equality compares strings and
booleans by value, numbers by their source spelling, references by
domain, name, qualifiers, and namespace context (never by written
spelling or site), lists item by item in order, and maps by their
entries in any order. A union value is bound as a one-entry map keyed
by its variant, so two variants with equal fields never collapse. The
layers a resolution reports are unchanged: every contributing layer
is listed, highest first.

With a base layer contributing `suffix src`, `suffix gen`, and
`suffix core` and a user layer contributing `suffix src`, `prefix
core`, and `prefix new` for one target, the resolved `match` list is
`suffix src`, `suffix gen`, `suffix core`, `prefix core`,
`prefix new`.

With a base layer contributing `changelog { section feat …; section
fix "Fixes" }` and a user layer contributing `changelog { section
perf …; section fix "Bug fixes" }` for one target, where `sections`
is a `map<String, Section>`, the resolved sections are `feat`, `fix`
titled `Bug fixes`, and `perf`.

The policy is checked when the schema is compiled, on every reading
path: a merge field on a schema whose resolution policy keeps
`selection=replace` (the default) is `KDLITH-DLC005`, since
replacement never combines values, and so is a merge on a field whose
type is not a list, a keyed collection, or an object (each optionally
absent). Records do not name their contribution type, so the shape a
value merges by is keyed by field name across the schema's
contribution types, the first contribution type in binding order
deciding a field name two of them declare; nested objects merge by
their own declared types.
