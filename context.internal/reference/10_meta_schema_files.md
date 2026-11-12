# Meta-schema files and fixture coverage

## Canonical meta-schema documents

Each file is a `schema schema version="0"` document. Together they
are the only authority for the meta-schema:

| File                                   | Contributes                                                                                                                                                                                                                                                                              |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pkg/kdl/schemas/schema/types.kdl`     | Logical types, the type-expression grammar, built-in scalars and formats, field definitions, object constraints, numeric bounds, and field exclusivity, generic type parameters, pattern codecs, field sensitivity, per-field merge policy                                               |
| `pkg/kdl/schemas/schema/bindings.kdl`  | Roles, node declarations, field bindings and their sources, collision and duplicate-key policies, child cardinality, member-group expansion, dependent value binding, case dispatch, node groups, conditional occurrence bounds, node annotations and descriptions                       |
| `pkg/kdl/schemas/schema/context.kdl`   | Typed context slots, effect groups and targets, effects, context policies, execution timing, source-context declarations, accumulating context, context snapshots                                                                                                                        |
| `pkg/kdl/schemas/schema/documents.kdl` | The schema root, containers, structural document rules, source imports with their locations rules and namespaces, document profiles, descendant rules, ordered sequences, symbol catalogs and reference policies, contribution policies and their scope vocabulary, provider descriptors |

Conventions the canonical files follow: inside an `entries` envelope,
argument 0 of each entry node is the map key, so entry types
(`EnumMember`, `FieldDef`, `Variant`) carry no name field and their
own bindings start at argument 1; and every construct the canonical
files use is itself declared by them (self-description).

## Meta-schema entrypoint

`pkg/kdl/schemas/schema/schema.kdl` selects the four canonical
documents beside it through `import` nodes, in the order types,
bindings, context, documents. It declares nothing of its own, is
never a second authority, and is never embedded. It is the input to
vetting and generating the meta-schema (`kdlith gen schema --crate
NAME --out DIR pkg/kdl/schemas/schema/schema.kdl`).

## Fixture scenarios

Scenarios run under the `kdlith-fixtures` harness; the `{fx-suite}`
argument placeholder lets a scenario read a committed file in place
(see `tests/fixtures/README.md`). The meta-schema coverage discipline:

- `tests/fixtures/meta-schema/canonical-read/*` — each canonical file
  and the entrypoint parse as KDL v2
- `tests/fixtures/meta-schema/canonical-vet/*` — each file passes local
  checking on its own; the merged run proves the four files link and
  describe themselves completely; the entrypoint run proves it yields
  the same meta-schema digest as the four files passed in the same order
- `tests/fixtures/meta-schema/canonical-bind/*` and
  `meta-schema/entry-bind/*` — each definition document binds as
  data of the meta-schema through the same session and binder every
  processor uses

Every mechanism carries at least one positive bind scenario, and
every new diagnostic code lands with one negative control under
`tests/fixtures/invalid/` naming the code. The census of record is
`tests/fixtures/COVERAGE.md`.
