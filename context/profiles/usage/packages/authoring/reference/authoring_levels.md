# Authoring levels and roots

## Roots and containers

A dialect definition has exactly one `dialect NAME version="V"` root;
a schema definition has exactly one `schema NAME version="V"` root.
The root containers hold `types`, `node`, `context`, `document`,
`import`, `annotation`, `reference`, and `resolution` declarations as
the sections below describe. Every definition root states its
`version` property; a root without one is rejected
(`KDLITH-BND004`).

## Contributing documents

Several documents may contribute to one dialect or schema. Every
contributor repeats the same root name and version, and a declaration
made twice across them is rejected rather than merged, so one
definition never depends on merge order. The committed kdlith
dialect is itself four contributing documents — `types.kdl`,
`bindings.kdl`, `context.kdl`, and `documents.kdl` under
`pkg/kdl/schemas/dialect/` — and `pkg/kdl/dialect.kdl` is the thin
entrypoint that imports the four, in that order. It declares nothing
of its own.

## Roles

A node declaration names a processing role beside its binding shape:

- `record` — validated data with no identity
- `declaration` — registers an identity other documents may reference
- `contribution` — a layered value preserved unresolved for the
  contribution resolver
- `context-control` — carries context effects only
- `source-import` — an import request
- `container` — structure without its own item

A node without `role=` binds as part of its parent (a field value or
an envelope entry) and is not a processing item.

## Dispatch

A node is matched by its parent, its name, the active profile, and
explicit discriminators (union value prefixes, `cases` property
values) — never by trying declarations in turn. The same node name
may declare different nodes under different parents.
