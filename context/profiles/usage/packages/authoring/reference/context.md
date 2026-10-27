# Context

The engine manages context; a schema defines what context-producing
nodes mean. Names like `package` or `defaults` gain meaning only
through the schema.

## Slots

`context { slot NAME TYPE }` declares a typed slot, with an optional
namespace and description. Values set into a slot are validated
against its type. A slot declares how a second value reaching it is
treated: `duplicates=reject` (the default, `KDLITH-CTX006`),
`duplicates=reject-different` (a byte-equal repeat is a no-op success
that keeps the held value; a differing one is rejected as reject
rejects it), `duplicates=last-wins`, or `duplicates=layered` with a
declared layer order. An accumulating slot instead declares a combining operation:
`accumulate append` for list-typed slots or `accumulate merge` for
map-typed slots, with `merge duplicates=reject` (default) or
`duplicates=last-wins` for a twice-contributed key.

## Effects

A node declaration carries effect groups:

```kdl
effects target=children { set region field=region }
effects target=following { clear place }
```

`effects target=following` and `effects target=children` are the two
targets; a node affecting both declares two groups. Effects are
`set` (from a bound field or a literal value, exactly one), `remove`
(the slot becomes unset), `clear` (every slot in a namespace), and
`handler NAME` for domain computation the processor installs. An
effect on an undeclared slot or namespace is rejected at its effect
point (`KDLITH-CTX005`).

A group may declare `replace=#true`: its `set` effects replace the
held value of the slots they name instead of failing the
duplicate-set check (`KDLITH-CTX006`), while slots the group does
not name keep their values — what distinguishes it from
clear-then-set.

`remove-from-args field=NAME` removes, per invocation, the slots the
named field's string values name — the names are data, so a document
spells which slots a control unsets:

```kdl
node clear-defaults role=context-control {
    field slots "optional<list<String>>" {
        from {
            args
        }
    }
    effects target=following {
        remove-from-args field=slots
    }
}
```

A non-string value names no slot and is skipped; a string naming no
declared slot fails closed at the effect point
(`KDLITH-CTX005`). A removed slot behaves as unset for every later
reader, so a context-bound field falls back to its declared
fallback. On a children target the field must come from the node's
header, not from a child (`KDLITH-SCH007`).

## Execution timing

A node is bound against its incoming context, then its effects are
computed. Children effects apply on entering the children and must be
computable from the node's header; following effects apply only after
the node completes successfully. Leaving a boundary restores the
caller's context; a late effect never changes nodes already bound.

## Context-bound fields and layers

A field may read a slot (`context=SLOT` or a `context` source) under
policy `fallback` (supplies the field only when no explicit value is
present) or `fixed` (an explicit value must agree).

A layered slot lists its layers highest precedence first
(`layers user base`); every `set` on it names one
(`set NAME field=F layer=L`). A higher layer's value replaces a
lower one's, and a second value in one layer is rejected, so the
result never depends on effect order.

## Source context

A `source-context` block declares fields derived from source
metadata — `source-context { field NAME TYPE { from { ... } } }` —
with sources such as `path-stem` naming registered providers. The
block declares its mode (`initial`, `fallback`, or `fixed`)
explicitly. Values apply before the document's first semantic node,
and every derived value records its origin in provenance.
