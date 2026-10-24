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

None of these fields is part of an item's serialized form, so
`vet data` output and exports are unchanged.
