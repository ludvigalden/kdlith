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
  profile set, root and sources, output policy, max invocations) in
  place of every option set before
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
`ItemVariant<Widget>` matches all of them. The schema form also
exposes `load`, `load_with(entries, extensions, scope,
ReadOptions)`, and `load_sources` / `load_sources_with` for a schema
held in memory.

## In-memory sources

A `Source` is a document's shown path and its text; data held in
memory reads through `read_sources`, `read_sources_local`, and
`collect_sources` in place of entry paths. An in-memory document
belongs to no source, so its unprefixed imports fail closed
(`KDLITH-LOC001`), while `$name/` locations resolve in the sources
the options register.

## Re-exports

The generated crate re-exports everything a tool needs
(`CheckScope`, `Extensions`, `ReadOptions`, `ReadError`,
`TypedReader`, `TypedItem`, `ItemVariant`, `Ref`, `Value`,
`Reading`, `LinkReport`, `LinkOutcome`, `UnmetQualifier`,
`ResolveError`, `Origin`, `Location`, `DiagnosticRecord`, `Source`,
`SourceTree`), so a host embeds its own schema library without
depending on the engine crates directly.
