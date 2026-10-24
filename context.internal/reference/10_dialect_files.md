# Dialect files and fixture coverage

## Canonical dialect definition documents

Each file is a `dialect kdlith version="0"` document. Together they
are the only authority for the dialect:

| File                                    | Contributes                                                                                                                                                                                                                                                                                       |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pkg/kdl/schemas/dialect/types.kdl`     | Logical types, the type-expression grammar, built-in scalars and formats, field definitions, object constraints, numeric bounds, and field exclusivity, generic type parameters, pattern codecs, field sensitivity, per-field merge policy                                                        |
| `pkg/kdl/schemas/dialect/bindings.kdl`  | Roles, node declarations, field bindings and their sources, collision and duplicate-key policies, child cardinality, member-group expansion, dependent value binding, case dispatch, node groups, conditional occurrence bounds, node annotations and descriptions                                |
| `pkg/kdl/schemas/dialect/context.kdl`   | Typed context slots, effect groups and targets, effects, context policies, execution timing, source-context declarations, accumulating context, context snapshots                                                                                                                                 |
| `pkg/kdl/schemas/dialect/documents.kdl` | Dialect and schema roots, containers, structural document rules, source imports with their locations rules and namespaces, document profiles, descendant rules, ordered sequences, symbol catalogs and reference policies, contribution policies and their scope vocabulary, provider descriptors |

Conventions the canonical files follow: inside an `entries` envelope,
argument 0 of each entry node is the map key, so entry types
(`EnumMember`, `FieldDef`, `Variant`) carry no name field and their
own bindings start at argument 1; and every construct the canonical
files use is itself declared by them (self-description).

## Dialect-generation entrypoint

`pkg/kdl/dialect.kdl` selects the four canonical documents through
`import` nodes, in the order types, bindings, context, documents. It
declares nothing of its own and is never a second authority. It is
the input to dialect generation (`kdlith gen dialect --crate NAME
--out DIR pkg/kdl/dialect.kdl`).

## Fixture scenarios

Scenarios run under the `kdlith-fixtures` harness; the `{fx-suite}`
argument placeholder lets a scenario read a committed file in place
(see `tests/fixtures/README.md`). The dialect-coverage discipline:

- `tests/fixtures/dialect/canonical-read/*` — each canonical file
  and the entrypoint parse as KDL v2
- `tests/fixtures/dialect/canonical-vet/*` — each file passes local
  checking on its own; the merged run proves the four files link and
  describe themselves completely; the entrypoint run proves it yields
  the same dialect digest as the four files passed in the same order
- `tests/fixtures/dialect/canonical-bind/*` and
  `dialect/entrypoint-bind/*` — each definition document binds as
  data of the dialect through the same session and binder every
  processor uses

Every mechanism carries at least one positive bind scenario, and
every new diagnostic code lands with one negative control under
`tests/fixtures/invalid/` naming the code. The census of record is
`tests/fixtures/COVERAGE.md`.
