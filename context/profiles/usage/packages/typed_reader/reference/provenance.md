# Provenance and capture

## site and origin

Every typed record and declaration carries where it was written:

- `site` — the location the binder recorded (source and span)
- `origin` — an `Origin { path, alias }`: the document's shown path
  as the reading displays it, plus the namespace of the import that
  loaded that document, or none for an entry document

Origins are stamped per source as the reading streams; a document
replayed from the cache carries its own path and sites. Each item
also carries `node_name`, the declared node that bound it, since
several nodes may bind one type.

## The sources a reading used

A typed reading ends with one `TypedItem::Sources(SourceSet)`, the
engine's `sources` item: `fingerprint` (the source-set fingerprint,
`None` when no named source was used) and `sources`, a `SourceUse`
per source — `name` (`None` for the local source), `kind`
(`SourceKind::{Local, Dir, Origin, Embedded}`), `identity`, `pinned`,
`origin`, `overridden`, and `replacement` (serialized as `override`).
An `Origin`'s path shown as `$name/...` joins to the entry named
`name`. A match over `TypedItem` that does not handle the item should
ignore it, as it ignores scope completions.

## Capturing context slots

`.capturing(&[slot, ...])` is opt-in: each `TypedItem::Document`
then carries the named slots as its first node binds under them
(`Option<SlotValue>`, unset kept as `None`) — the context its
importer projected, with the document's source-context values
applied. Every record and declaration then carries the same slots as
its `context`. A slot name the schema declares no slot for fails the
reading (`ReadError::UnknownSlot`) before any document is read.

`inspect data --view context` reports slot values at a different
point — each entry's context as the entry left it.

## Declaring profiles

`.provenance()` is opt-in: each `TypedItem::Document` then carries the
profiles its entry invocation selected — the one profile it named or
its source path match-selected, or each member of the selected set, in
listed order (`profiles`, empty when it was not asked for). A document
an import reaches carries the entry's selection, which is the selection
it binds under. A definition that declares no profiles, or a profile
the reading's options name that the definition does not declare, fails
the reading (`ReadError::NoProfiles`, `ReadError::UnknownProfile`)
before any document is read.

None of these fields is part of an item's serialized form, so
`vet data` output and exports are unchanged.
