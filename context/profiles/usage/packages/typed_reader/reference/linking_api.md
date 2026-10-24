# Linking from the typed reader

## .link(CheckScope)

`.link(CheckScope::Complete)` runs the same link composition
`vet data` runs — the engine's one link stage over the reading's
declarations and references. Under complete scope any link
diagnostic fails the reading; under local scope the same diagnostics
are reported and the reading does not fail.

## collect, resolve, settled

`collect` keeps a whole reading: its items in stream order, pending
typed bindings among them, the link report, and the reading's
diagnostics (binding diagnostics followed by linking ones). On the
resulting `Reading<T>`:

- `reading.resolve(&reference)` returns the declaration a reference
  resolved to, read as its target type
- `reading.settled::<T, _>(&reference)` reads it as `T`

A reference that did not resolve, resolved ambiguously, or failed a
required qualifier is a `ResolveError::Link` carrying the link
outcome and the namespaces that hid a match — never a default value.

A generated reference type holds its reference as `.0`, so
`reading.resolve(&record.target.0)` resolves a record's field.
