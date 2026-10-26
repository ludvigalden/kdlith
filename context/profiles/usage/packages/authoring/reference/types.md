# Types

Types say what a field is; bindings say how it is spelled. A logical
field is never inherently "argument 0" or "a child named x".

## Declarations

Inside a `types` container: `string`, `enum`, `object`, `union`,
`map`, `codec`, and `asset` declarations. Type expressions are quoted strings
(`"optional<list<String>>"`) checked by a registered grammar, not
evaluated. Constructors: `optional`, `nullable`, `list`, `map`,
`ordered-map`, `union`, `ref`. Fields are required unless optional;
absence, null, and an empty list are distinct states.

Built-in scalars are `String`, `Bool`, `Integer`, `Decimal`, `U64`,
and `ValueExpr` (any single KDL scalar checked against the holding
field's type).

## Patterned strings and codecs

A `string` type may carry `pattern` (whole-string match),
`min-length`, a built-in `format` (`kebab-case`, `camel-case`,
`lower-snake`, `upper-snake`, `uuid`, `zero-padded` with `width`), or
a registered `validator` with a version.

A pattern codec decodes a matching string into a typed value:

```kdl
codec Stamp output=U64 {
    match #"(?<n>[0-9]+)"#
    capture n U64
}
```

The `match` must cover the whole string; `capture` children type each
named group and the capture set must cover the named groups exactly.
A codec name is usable wherever a type expression names a type; the
value decodes at bind time while provenance keeps the original
spelling. An optional `namespace NAME` records the reference
namespace a decoded identity belongs to.

## Assets

An asset type declares a field whose value is a location naming a
file or directory that is not a KDL document:

```kdl
asset Spec {
    kind file            // file (default) | dir
    extensions yaml yml  // without the dot; none admits any
    base source          // document (default) | source
    patterns #true       // a pattern expands as one item of a list
    digest #true         // record a content digest
}
```

The reading resolves the value as it resolves an import location and
checks that it exists, its kind, and its extension; it never reads
the content beyond the digest. See the `data` package's
`imports_and_sources.md`.

## Generic types

A declaration may declare type parameters (`parameter T`) and be
instantiated as `T<A, B>`. Parameters substitute at compile time, so
no generic survives into a compiled plan; arity is checked, and an
instantiation cycle is rejected (`KDLITH-SCH009`).

## Unions

A union variant is selected by the node name that spells it. A
variant may carry `prefix="…"`: for a union bound from a scalar
value, the variant is selected when the value's text starts with the
prefix, and a scalar no prefix selects binds as the first
un-prefixed variant, in declaration order, whose type accepts it.
A scalar spelling routed into a reference-typed variant binds as the
decoded reference, which a generated typed reader decodes into that
variant exactly as binding routed it.
Prefixes are checked pairwise at compile time (`KDLITH-SCH010`); once
a prefix selects a variant, a failure inside it is an error, never a
fallback.

## Constraints and sensitivity

An object type may declare explicit policies — `all-or-none`,
`requires`, `at-most-one`, `exactly-one-field` — enforced by the
binder (`KDLITH-BND011`, `KDLITH-BND013`); `bound F min=N max=M`
declares an inclusive numeric range (`KDLITH-BND012`, compared on the
exact value: `0x1388`, `5000`, and `5e3` all equal 5000). A field
holds a value once anything binds it — written, from context, a
literal fallback, or `#null`; only a field holding none is absent.

A logical field may carry `sensitivity=secret` or
`sensitivity=reference`: `secret` marks a resolved sensitive value,
`reference` an identifier that locates one. Every serialization and
diagnostic surface applies an explicit output policy — emit, omit, or
redact — and a `secret` field is never emitted unless the policy
explicitly says emit.

`documented-default` is metadata only; `fallback` supplies an absent
value. A computed fallback (`fallback computed=#true { dependency
"region" }`) is a descriptor the processor never computes — its value
arrives through the value-resolution hook (`--resolve-from` on the
CLI).
