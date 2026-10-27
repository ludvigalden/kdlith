# Bindings

A binding maps KDL syntax into a logical field.

## Node declarations

```kdl
node card role=record type=Card {
    parents deck
    field title arg=0
    field labels { from { children label } }
    children label min-occurs=0 max-occurs=2
}
```

`node NAME` with optional `role=`, `type=`, and either `parent=` or a
`parents` child; a node with neither is a root node. `root=#true`
beside a parent admits the node at the root as well, so one
declaration covers a recursive container that may also start a
document. A declaration may carry `description=` documentation and
`annotation NAME VALUE` facts, which are metadata the processor never
enforces. `parents group=NAME` places the node under every member of
a declared group; `group NAME MEMBER...` under `schema` names the set
once.

## Multi-name declarations

```kdl
node name synonym definition description type=Designation parent=concept {
    field value arg=0
}
```

A declaration may spell several names: `node A B C ...` binds the one
body under every spelling, exactly as the same declaration repeated
once per name does. Each spelling is its own field-source node, so
bound items, the `bound` and `plan` views, and the generated `Item`
variants distinguish the spellings; a generated crate carries one
variant per spelling beside the one `ItemVariant<T>` matching them
all. Name admission is per spelling and fail closed: a spelling that
collides with another declaration under the same placement is
`KDLITH-SCH004` at compile, a non-kebab spelling `KDLITH-BND003` at
bind, and any failing spelling fails the whole declaration.
Placement (`parent=`, `parents`, `parents group=`, `root=#true`),
profiles, compact sources, and `include` splice treat the declaration
as one node with several admitted names; effects apply per spelling
as the separate declarations would.

## Field sources

Shorthands on a `field` binding: `arg=N`, `property=NAME`,
`child=NAME`, `context=SLOT`. A `from` block spells explicit
sources: `argument`, `args`, `property`, `child`, `children`,
`entries`, and `context`. Several sources may populate one field;
`collision reject` makes supplying more than one an error even with
equal values — there is no implicit precedence.

`args` collects the remaining positional arguments; `children`
collects repeated children in order. An `entries NODE
key="arg(0)" value=TYPE` envelope consumes the selected argument as
the map key exactly once, the rest of the node binding as the value.

A `children-group GROUP` or `child-group GROUP` source in a `from`
block admits every member of a declared node group as a source
spelling: at compile the group splices one `children MEMBER` or one
`child MEMBER` source per member at the reference's position, in
member order, exactly as the hand-written repeated form spells them.
A `child-group` field over several members is several alternative
sources, so it carries `collision reject` like its hand-written form;
naming an undeclared group fails compilation (`KDLITH-SCH006`), and a
member spelled both concretely and through the group in one field is
a compile-time duplicate (`KDLITH-SCH005`).

## Compact child spelling

A `child` or `children` source may carry `compact=#true`: the child
may then also be spelled as a property `NAME=VALUE` of the node the
source selects from, binding exactly as a `NAME` child carrying
VALUE as its argument, at the property's position. It qualifies only
when the child's one argument fills everything it binds; anything
richer keeps the block form (`KDLITH-DLC005` on the source,
`KDLITH-BND005` on a property spelling a non-compact child).
Properties cannot repeat a name — a repeated property is
`KDLITH-SYN003`, checked before the compact splice — so a compact
source admits at most one term per child name; spelling a name more
than once uses the block form.

## Cardinality and conditions

A child is permitted only when a field binding consumes it or a
`children NAME` rule names it; `min-occurs` / `max-occurs` bound
occurrences, minimums checked at scope completion (`KDLITH-BND002`).
A children rule may carry `when property=FIELD equals=V` branches
and `when-all` / `when-any` combinators, each branch root with its
own bounds; the rule's own bounds are the fallback when no branch
matches, and a rule with branches must spell at least one bound of
its own (`KDLITH-SCH019`). Branches must be provably disjoint
(`KDLITH-SCH017`); the gating property must be a scalar or enum field
(`KDLITH-SCH016`).

## Cases and dependent values

A `cases` block selects the logical type a node binds as by property
value: `case property=NAME equals=V type=TYPE` children evaluated
top-down, with an optional `otherwise type=TYPE`. A case-selected
object type binds closed: its required fields are checked as the
node's own (`KDLITH-BND004`) and a field the selected type does not
declare is refused (`KDLITH-BND014`).

A case body may also carry `field` and `children` binding
declarations, adding to the node's common ones exactly while that
case matches:

```kdl
node figure role=record type=Figure {
    field id arg=0
    field kind property=kind
    field title property=title
    cases {
        case property=kind equals=funnel type=Figure {
            field depth property=depth
            children section min-occurs=1
        }
        case property=kind equals=bar type=Figure {
            field note property=note
            children segment min-occurs=1
        }
        otherwise type=Figure
    }
}
```

The common bindings always bind, for every case. A matched case's
bindings add to them; under any other case — or `otherwise`, or no
match at all — they contribute nothing: another case's child is an
unknown node (`KDLITH-BND001`) and another case's property an unbound
entry (`KDLITH-BND005`). A case-level children rule enforces its
bounds only under its own case, composing with the rule's `when`
branches. A field one case binds alone should be optional in the
node's type, since it is absent wherever that case does not match.
A case-level binding naming what the node binds in common — a field
name, or a child name a common rule or field source takes — is
refused at compile (`KDLITH-SCH023`). A node whose cases declare
bindings settles its dispatch in the header: every case property
binds from a property and reads no context slot or type-declared
default (`KDLITH-DLC005`).

A `value` block binds a node's remaining payload against a
referenced declaration's member (`against target=FIELD member=FIELD`
plus a `codec` such as `argument-tail`, `fixture-value`, or
`from-kind`). When the target declaration is not yet compiled the
binder emits a pending typed-binding item carrying the payload raw;
linking validates it against the resolved declaration's member type
(`KDLITH-REF005`).

## Path-typed values

A `value-from-path field=PATH` block types a node's remaining payload
by the field the path names, resolved against the subject type the
node's own case dispatch selects:

```kdl
node set type=SetOperation parent=changes {
    field path arg=0
    field kind property=kind
    value-from-path field=path
    cases {
        case property=kind equals=module type=ModuleCard
        case property=kind equals=concept type=ConceptCard
    }
}
```

A scalar payload binds through the same decode paths a static binding
of the resolved field's type takes, and a dotted path
(`validity.from`) walks the subject type's object fields. An
object-typed path admits the child block form: the child's properties
compose the payload, each checked in the resolved object's own field
types. A path the subject type does not declare fails closed at bind
(`KDLITH-BND015`), as does a payload that fails the resolved field's
type (the same code a static binding produces). The dispatch's
selected type is the subject the path resolves against, not the type
the node binds as — its required fields and closed shape stay the
subject's. A node whose dispatch selects no subject type leaves its
payload a pending typed-binding item, preserved raw and never
validated: the bound view shows it, `read data` and export exclude
it. The named field must be one the node binds and admit only
strings, and no `value` block may sit beside it (`KDLITH-DLC005`); a
path-typed value is never a reference and never enters the link
stage.

## Include expansion

An `include NAME` child splices the entries of the named keyed member
of a declaration into the collection being bound, at the expansion
point, in declaration order (`KDLITH-SCH009` on a cycle, `BND006` on
a key collision). Provenance records both the declaration site and
the inclusion site.

## Duplicate keys and compile checks

`duplicate-keys reject` on keyed collections is the fail-closed
spelling (`KDLITH-BND006`). The compiler rejects overlapping bindings
(`KDLITH-SCH003`), unconsumed arguments, and contradictory ownership
of children. Bindings compile into a plan; a definition's KDL is
never reinterpreted per record.
