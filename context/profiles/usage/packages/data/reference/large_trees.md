# Large trees

A reading streams: each document is read, bound, and given to the
consumer item by item, and nothing of it is kept once it has
streamed. What a reading holds grows only with what it was asked to
keep.

## What a reading keeps

- A plain reading (`vet data --scope local`, a typed `read`) keeps
  neither texts nor items. A document invoked a second time under the
  same content, context, schema, and profiles is bound again and its
  result kept, so a third invocation replays it: only a document
  invoked repeatedly costs the memory of its items.
- Linking keeps one compact entry per declaration — its identity,
  namespace, site, and the few fields a reference's lookup reads —
  and one entry per reference a declared reference type looks up.
  Declarations are indexed by the hash of their composed name, so
  linking costs time in proportion to the reading: a lookup probes
  for the identity the reference determines and scans nothing.
  `vet data` keeps the failed links alone; a typed `collect` and the
  `references` view keep every link.
- `cache refresh` keeps every bound result and the context each
  record bound under, because it persists them.
- A typed `collect` keeps the whole reading by design; `read` with a
  sink keeps what the sink keeps.

## The invocation bound

One reading invokes at most 1024 sources unless `--max-invocations N`
(`ReadOptions::with_max_invocations`) raises the bound. A tree of
more documents than that names its own bound; the cost of an
invocation does not grow with the count.

## Reading less: demand

A consumer that wants a few record types does not need the rest
bound. `export --lazy` and the typed reader's `demanding` read on
demand: a node nothing demanded can come from is passed over where it
opens, unvalidated, and every demanded item is exactly the item a
full reading yields. See `reference/export.md` and the typed reader's
`reference/reader_usage.md`. Choosing the entry document chooses the
documents: an entry reads what it imports, so a tree whose parts each
have their own entry is read a part at a time by naming that entry.

## Reading faster: threads

`--threads N` (`ReadOptions::with_threads`) binds the documents one
import request fans out to ahead of their turn on other threads. No
import's effects reach its importer or its siblings, so each such
document binds alone; its items are then given in the order one
thread gives them, and the stream is byte for byte the same. A
document that itself imports binds in its turn.

The count defaults to one. Each item is made on a worker and released
on the thread that consumes it, which only an allocator that scales
across threads does cheaply: build the consuming program with
jemalloc or mimalloc before raising the count. Under an allocator
that guards each thread's memory with a lock, more threads make a
reading slower — unless a demand leaves few items to hand over.

## Documents

One document is read whole, and at most 16 MiB; a tree of many
moderate documents reads better than a few huge ones, and a document
of many short lines locates its values faster than one of few long
ones.
