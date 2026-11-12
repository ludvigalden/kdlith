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

`reading.link_of(&reference)` returns the link itself: its outcome,
its `candidates`, and the namespaces that gated a match. Each
candidate is the link stage's index entry for one declaration — its
`node`, `identity`, `namespace`, the identity `labels` its name
segments came from, and its `site` — shared with every other link
that reached it, so holding links costs no copy of a declaration.

A generated reference type holds its reference as `.0`, so
`reading.resolve(&record.target.0)` resolves a record's field.

A reference carries what it was bound with: `written()` (the spelling),
`name()` and `qualifiers()` (the codec's segments and captured
qualifiers), `site()`, and `context()` — for a reference type whose
qualification may come from context, the qualifier value in effect
where it was bound (the system a bare code resolves under), `None`
otherwise. A reference a context slot or a literal fallback fed is
bound at the node it fed, so several can share one site; `resolve`
and `link_of` tell them apart by site together with domain, name, and
qualifiers.
