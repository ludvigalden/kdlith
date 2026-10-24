# The item stream

## Item kinds

A bound node yields the item its role names: a `record`, a
`declaration` with its identity, a `contribution` preserved
unresolved, an `import` request — which, once resolved, records the
shown paths of the targets it loaded — a `pending-binding` awaiting
its declaration, an `expansion` for each `include` short form, a
`resolution` per settled contribution target and scope, or a
container's scope completion. A reading ends with one `sources` item. Context controls yield their effects
only. Every diagnostic is an item in the same stream, and every
view and export reads these items.

## Stream order

A node's item is emitted when the node completes, after everything
nested in it, so the items of

```kdl
section "a" { card "a1"; section "b" { card "b1" } }
```

arrive as `a1`, `b1`, `b`, `a`. Each record and declaration carries,
as `parent`, the nearest enclosing node whose role yields an item,
or none at the document's root level — including the root of an
imported document. Readers may rely on this order and linkage.

## Provenance

Every record and declaration carries where it was written: its
`site` (source and span) and its `origin` — the document's shown
path plus the namespace of the import that loaded it. A document's
items keep its origin after a nested import completes, and a
document replayed from the cache carries its own path and sites.
The `origins` view writes the full provenance envelope for each
item.

## The sources item

After its last document, and after any resolution, a reading emits
exactly one `sources` item listing every source it used: the local
source first (`name` null, `kind` `local`, its identity the canonical
digest of the documents it read from that source, by shown path),
then each named source a location entered, in name order. Each named
entry carries `kind` (`dir`, `origin`, or `embedded`, as declared),
`identity` (the digest of the tree the reading read: an origin's pin,
an embedded source's build digest, or the computed canonical tree
digest), `pinned` (`false` for an unpinned or overridden source),
`origin` when one is declared, and `overridden`, with
`override: {layer, directory}` when an override replaced it.

The item's `fingerprint` is the source-set fingerprint: the SHA-256
of one `NAME IDENTITY` line per named source used, in name order. It
is absent when the reading used no named source. Every `$name/`
location an import, document, or asset shows joins to the entry
named `name`.
