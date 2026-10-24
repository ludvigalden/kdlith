---
description: >-
  Authoring kdlith definitions: the three authoring levels, schema
  and dialect documents, logical types, node bindings, context slots
  and effects, document rules, imports, and profiles.
binds_to_depth: main...
---

# Authoring definitions

kdlith has three authoring levels. All three are KDL documents, and
their role, not their file extension, tells them apart:

| Level | Term               | Defines                                                            |
| ----- | ------------------ | ------------------------------------------------------------------ |
| 1     | Dialect definition | The language used to author schemas                                |
| 2     | Schema definition  | Domain types, node bindings, context effects, and processing rules |
| 3     | Data document      | Domain declarations, controls, imports, and records                |

A dialect definition is written in the dialect it defines. Several
documents may contribute to one dialect or schema: each repeats the
same root name and version, and a declaration made twice across them
is rejected, never merged.

Most tools use the committed kdlith dialect (`dialect kdlith
version="0"`, whose canonical documents live under
`pkg/kdl/schemas/dialect/`) and author a schema definition for their
domain; data documents are authored against that schema.

Read on demand:

- the authoring levels, roots, and vocabulary —
  `reference/authoring_levels.md`
- type declarations, expressions, generics, codecs, and constraints —
  `reference/types.md`
- node declarations, field sources, cardinality, cases, and the
  compact child spelling — `reference/bindings.md`
- context slots, effects, policies, and layers — `reference/context.md`
- document rules, imports, profiles, and annotations —
  `reference/documents.md`

A minimal schema is in `examples/shop_schema.kdl`; the data that
reads against it is in the `data` package's `examples/`.
