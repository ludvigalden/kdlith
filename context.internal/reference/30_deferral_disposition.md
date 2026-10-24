# Deferral disposition

No mechanism is deferred at this version. The nineteen deferrals the
record previously carried — `generic-types`, `codecs`,
`union-dispatch`, `sensitivity`, `expansion`, `dependent-binding`,
`case-dispatch`, `source-context`, `accumulating-context`,
`context-snapshots`, `profiles`, `descendant-rules`,
`ordered-sequences`, the import-source and namespace deferrals,
`symbol-catalogs`, `contribution-resolution`, `computed-defaults`,
`secret-providers` — are all defined in the mechanism tables of this
tree. The `deferred` node and the diagnostic code that rejected its
spellings (`KDLITH-DLC003`) are retired from the surface:
`documents.kdl` no longer declares the marker, and a spelling no
binding accepts is rejected as unbound (`KDLITH-BND001`) on every
reading path — the same loud failure the marker existed to guarantee.
This is a retirement, not a reservation: the marker earns its place
only while something is deferred, and a future version that defers a
mechanism again re-introduces it as an explicit, versioned change to
the canonical files and this record together.

The restricted-semantics boundaries that motivated several deferrals
are now defined boundaries of the mechanisms themselves: providers
are never executed (secret-provider commands and computed defaults
are descriptor data), import locations are restricted path expansion
inside named sources (never environment-variable interpolation), and
sensitivity is policy-gated (an explicit output policy on every
serialization and diagnostic surface).

Several in-scope mechanisms stop short of extensions this version
does not define: the engine never infers a profile from document
content, and match rules compare path components only; locations
define one ordering (portable path bytes) and admit no interpolation;
descendant rules stop at the declaring node's subtree; ordered
sequences carry no unbounded-repetition groups; symbol catalogs
define the three named qualification policies and `qualified-name`
composition; contribution resolution happens only through registered
resolvers; computed defaults and secret providers are descriptors
never executed; dependent binding's `against context=` form stays
stopped short because the providers this version defines hold no
tables; and generic types monomorphize at compile time with no
parameter constraints, defaults, or variadic parameters. Each stop is
defined behavior, not a gap.
