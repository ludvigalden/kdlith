# Value-resolution hook

A computed default and a secret provider are descriptors the
processor never computes or executes. The values they describe reach
a bound record only through one static hook the consuming program
implements: kdlith picks no source, and nothing is loaded at run
time.

- **One trait.** The engine's public surface defines a single
  `ValueResolver` trait for secrets and context variables: they are
  requested through the same construct (a computed fallback) at the
  same point, differing only in how the result is shown, which is the
  output policy's concern. A program registers its implementation on
  the reader (`with_resolver`, or a generated crate's
  `reader_with_resolver`) — the counterpart of a contribution
  resolver; a reader without one keeps descriptor-only behavior, so
  an optional field stays absent and a required one is missing
  (`KDLITH-BND004`).
- **When it is asked.** When a bound node completes, each field that
  nothing supplied and whose declared fallback is computed is put to
  the resolver once. Resolution happens at node completion, so a
  children-effect computed from the node's header never sees a
  resolved value.
- **What the call carries.** A `ResolutionCall` holds the source
  document as the reading shows it, the node's site, the declared
  node name, the field with its declared type and sensitivity, the
  declared descriptors as data (the dependency list, the named
  providers' identifier, TTL, and command list with argument
  boundaries preserved), and the node's incoming context slots by
  name. Nothing in the engine executes a provider command, joins its
  arguments into a shell string, or computes a dependency.
- **Loud failure.** A resolver that declines returns a reason, and
  the node fails with `KDLITH-BND009` naming the field, the node, and
  that reason; the reason must not quote a secret. A resolved value
  failing the field's type is a decode failure (`KDLITH-BND003`)
  whose message never names a concealed value. Neither is ever
  replaced by a silent fallback, and a field that failed resolution
  is not also reported missing.
- **Provenance.** A resolved value records the origin kind
  `resolved`: the node's location and the identifiers of the declared
  providers the call carried. A `secret` field passes the output
  policy like any other secret field, whatever its origin.
- **Caching.** The values a resolver supplies are part of a bound
  result's invocation identity, so a changed value misses the cache
  and re-binds. The identity's sixth component, `resolved`, is a
  domain-separated fingerprint of the length-prefixed stream of each
  non-secret resolution — node site span, field name, and the
  value's canonical encoding, in resolution order; a result the
  resolver supplied nothing to carries the empty stream's
  fingerprint. A result for which any `secret` field was resolved is
  never cached, in the session or on disk: even a fingerprint of a
  low-entropy secret on disk is an offline guessing oracle. Cache
  entries carrying the component are format 3; an entry of another
  format is reported as such (`KDLITH-CAC002`), never misread.

## --resolve-from

The command-line tool is itself a consuming program of the hook:
every data verb takes `--resolve-from FILE`, a KDL document of
`value KEY VALUE` nodes (a string key, one scalar value, no
properties, annotations, or children; any other shape is
`KDLITH-CLI001` before any definition loads). A computed default
names exactly one dependency and that dependency is the key; a
dependency the file does not hold, or a default naming any other
number of dependencies, is declined (`KDLITH-BND009`). Declared
providers reach the resolver as data and their commands are never
executed. `cache refresh` keys each entry on the non-secret resolved
values; a document for which a secret was resolved gets no entry,
and a reading that consulted the resolver at all writes no snapshot
entry.
