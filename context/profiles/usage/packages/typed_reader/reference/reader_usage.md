# Reader usage

## Constructors

A generated schema crate exposes two reader constructors:

- `reader()` returns the plain typed reader; contributions stay
  preserved records.
- `reader_with_contribution_resolver()` installs the shipped
  `layered` contribution resolver statically, so a reading yields
  the same resolution items the data verbs produce.

The engine's data verbs install the `layered` resolver themselves,
so the CLI and the typed reader resolve alike.

## Host-input builders

Host inputs are builders on the returned reader:

- `with_source(NAME, tree)` registers a named source
- `with_profile(NAME)` names the profile every entry document binds
  under
- `with_options(ReadOptions)` takes a whole options set (profile or
  profile set, root and sources, output policy, max invocations,
  demand, threads) in place of every option set before
- `demanding(&["Widget", "Gadget"])` reads on demand: only the nodes
  an item of a named record type can come from bind (see Reading on
  demand below)
- `with_handlers` installs the context handlers that handler effects
  run through

A document no rule or default selects a profile for fails with
`KDLITH-SCH011` until a profile is named; an invalid source fails
with `KDLITH-LOC005` before any document is read.

## Reading

`reader.read(entries)` streams `TypedItem`s in the engine's stream
order. Each reading node's items stream as a variant of the crate's
`Item` enum, named after the node's type (`Item::Widget`); when
differently named nodes bind one type, each node keys its own
variant (`Item::Widget`, `Item::Gadget`, `Item::Part`), and
`ItemVariant<Widget>` matches all of them.
`TypedItem::contribution::<C>()` decodes a preserved contribution's
value — or a settled resolution's value once the reading's resolver
has answered — as the type named at the call, and is `None` on every
other item. The schema form also
exposes `load`, `load_with(entries, extensions, scope,
ReadOptions)`, and `load_sources` / `load_sources_with` for a schema
held in memory.

## Reading on demand

A tool that wants a few record types out of a large tree names them:
`reader.demanding(&["Widget"])`, or `ReadOptions::with_demand`. A
node whose item is of no demanded type, and inside which nothing
demanded can be written, is passed over where it opens: nothing in it
is decoded, validated, or yielded. Imports are still followed, a node
whose effects reach its siblings still binds, and so does every node
on the way down to a demanded one, so a demanded item is exactly the
item a full reading yields — the same fields, context, site, and node
index. Items of other types the reading had to bind on the way still
stream; match the variants you asked for and ignore the rest.

A demand trusts the documents for everything it passes over: errors
inside a skipped node are not reported. Such a reading resolves no
contributions, and `.link` sees only the declarations the reading
bound, so demand every type its references target. A name the schema
declares no record type for fails the reading before any document is
read (`ReadError::UnknownType`).

## Threads

`ReadOptions::with_threads(n)` binds the documents an import fans out
to ahead of their turn on other threads and gives their items in the
order one thread gives them: the stream is the same whatever the
count, which defaults to one. A reading with context handlers or a
value resolver installed binds on one thread, so they are called in
document order. Each item is made on one thread and released on
another, so threads pay off only when the program's allocator scales
across threads (jemalloc or mimalloc, for instance); under an
allocator that guards each thread's memory with a lock they cost more
than they bind, except where a demand leaves few items to hand over.

## In-memory sources

A `Source` is a document's shown path and its text; data held in
memory reads through `read_sources`, `read_sources_local`, and
`collect_sources` in place of entry paths. An in-memory document
belongs to no source, so its unprefixed imports fail closed
(`KDLITH-LOC001`), while `$name/` locations resolve in the sources
the options register.

## Assets

An asset type generates a newtype over `Asset` (its `location`,
`source`, and optional `digest`). `Reading::open(&asset)` reads a file
asset's bytes through the reading's own sources, so an embedded
source and a directory serve it alike and it stays inside its source;
a directory asset, and an asset another reading resolved, are refused
with an `OpenError`.

## Re-exports

The generated crate re-exports everything a tool needs
(`Asset`, `CheckScope`, `Extensions`, `FromValue`, `ItemVariant`,
`LinkOutcome`, `LinkReport`, `Mismatch`, `OpenError`, `Origin`,
`ReadError`, `ReadOptions`, `Reading`, `Ref`, `Regex`, `ResolveError`,
`Source`, `SourceTree`, `TypedItem`, `TypedReader`, `UnmetQualifier`,
`Value`, `matches_line`, plus `Location` and `DiagnosticRecord`), so
a host embeds its own schema library without depending on the engine
crates directly.
