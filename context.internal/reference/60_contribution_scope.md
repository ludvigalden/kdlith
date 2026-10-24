# Contribution scope

A contribution carries an application scope beside its layer. As
with `layer`, a schema establishes it by binding a contribution
node's field named `scope` (`field scope property=scope`, written
`scope=dev`); a contribution without one is scope-less. A schema
declares its own scope vocabulary with existing constructs: a
declaration node over a scope type, a reference type targeting it,
and the contribution's `scope` field typed with that reference type.

```kdl
types {
    object ScopeDef { field name String }
    object Setting {
        field target "ref<Option>"
        field value String
        field scope "optional<ScopeTerm>"
        field layer "optional<String>"
    }
    codec ScopeSpelling output=String {
        match #"(?<name>[a-z][a-z0-9-]*)"#
        capture name String
    }
}
node scope role=declaration type=ScopeDef { field name arg=0 }
reference ScopeTerm target=ScopeDef codec=ScopeSpelling qualification=explicit
node setting role=contribution type=Setting {
    field target arg=0
    field value arg=1
    field scope property=scope
    field layer property=layer
}
```

Data documents declare the terms (`scope dev`) and spell them on
contributions. The bound contribution keeps the scope reference in
its value map and carries its written spelling as the record's
`scope`; the contribution's target is always taken from its other
reference fields, never from the scope, whichever field is declared
first. A scope field typed `optional<String>` still binds its text
unlinked.

A typed scope links like any field typed with a declared reference
type: an unmatched spelling is `KDLITH-REF001`, a duplicate identity
`KDLITH-REF002`, and `KDLITH-REF003` / `KDLITH-REF004` apply under
the reference type's qualification policy. Under the default
complete scope these fail the reading; the resolver never refuses a
record for its scope.

The shipped `layered` resolver partitions resolution by scope within
layer precedence: records first group by target and scope (the
written spelling, scope-less records forming their own partition),
and inside each partition layer precedence, duplicate treatment, and
selection apply exactly as for an unscoped target. Layer never
crosses a partition, and scope never overrides layer inside one. A
target contributed under two scopes resolves twice, one `resolution`
item per scope, and two records for one target in one layer under
different scopes are not duplicates. `missing reject` checks a
reference-valued field against the targets contributed under any
scope. With `layers user base` under `replace`, the data document

```kdl
scope dev
scope staging
option font
option mode
setting mode dark scope=staging layer=user
setting mode light scope=dev layer=base
setting mode contrast scope=dev layer=user
setting mode plain scope=staging layer=base
setting font serif layer=base
setting font sans layer=user
```

settles three partitions, in this order: `font` without a scope
resolves to `sans`, `mode` under `dev` to `contrast`, and `mode`
under `staging` to `dark`, each reporting `layers` `["user"]`
(`processing/resolution/scope-partition`).
