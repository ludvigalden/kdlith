# Documents, imports, and profiles

## Structural document rules

`document { first { exactly-one A B } }` requires the first active
semantic node to be exactly one of the listed nodes; an import may
neither precede nor supply it. `document { following ... }` names the
nodes permitted after the first. Inside a node declaration,
`exactly-one A B` requires exactly one of the named children at scope
completion. Descendant rules bound a named node anywhere in the
declaring node's subtree, never crossing an import boundary. An
ordered `sequence` of steps and alternatives compiles into the same
per-scope checks; repetition stays with occurrence bounds.

## Imports

`import PATH` with optional `as=NAME`. Every path is a location:
`rel/path` resolves from the importing document's directory, `/path`
from its source root, and `$name/path` from the root of the named
source. A location never leaves the source it resolved against
(`KDLITH-LOC002`); a location naming no document is
`KDLITH-LOC001`; an unregistered `$name` is `KDLITH-LOC003`.

An import may carry `repeated=reject` (the default), `coalesce`
(equal context reuses the first invocation), or `instantiate`. The
imported document receives only the `project` list of the caller's
slots; its context mutations never return to the caller.

An import carrying `as=NAME` preserves the namespace: the namespaced
document resolves its own declarations unqualified, while every other
document reaches them only through the namespace. A namespace must be
unique in its document, and a gated reference that cannot resolve is
`KDLITH-REF001` naming the namespace.

Pattern syntax (`*`, `**`, flat `{a,b}`) is admitted only where a
`locations { patterns star=#true doublestar=#true braces=#true }`
rule enables it, with `allow-empty=#true` for empty matches. No rule
admits environment-variable interpolation.

## Profiles

`profile NAME { roots { allow TYPE ... } }` names a selection of
roots and nodes, and a node declaration may carry `profile=NAME` so
one node name binds differently per profile. A profile may carry
`extends=NAME` (inheriting roots and match rules child over parent),
`default=#true` (one per schema, `KDLITH-SCH013`), and `match` rules:
`path-suffix`, `path-prefix`, and `file-name` compare the entry
document's root-relative source path. Selection is resolved once per
entry document, before any node binds, in one total order: an
explicitly named profile; else a listed profile set's matching or
default member; else the first matching declared rule; else the
schema's default; else `KDLITH-SCH011`. Selection reads invocation
context only, never document content.

## Annotations

`annotation NAME TYPE [required]` at the schema root declares
metadata vocabulary; node declarations carry facts as
`annotation NAME VALUE` children. The compiler checks that every
fact's name is declared, values decode as the declared type, and
every node carries each required annotation. Annotated nodes bind
exactly as unannotated ones; a generated schema crate exposes one
public constant per fact.

## Identity and references

A declaration may carry an `identity { domain NAME compose
qualified-name FIELD FIELD }` block composing its qualified symbol
from bound fields, and `reference NAME target=TYPE codec=…
qualification=explicit|explicit-or-context|imported-or-context`
declares how reference values spell and resolve. The identity block
pins the declaration's domain whatever a `cases` block selects: an
identity-bearing declaration registers under the block's domain even
when case dispatch retypes it, while a declaration without one
registers under the type the case selected. Data fields typed
with a declared reference type bind structured references that
linking resolves — see the `data` package's linking reference.
