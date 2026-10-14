# The kdlith dialect, version 0

This is the design record for version 0 of kdlith's schema-definition
dialect: the language schema definitions are written in. It lists every
mechanism the dialect defines and every mechanism it deliberately defers,
maps the committed files that carry them, and states what the engine and
the reference processor must honor when they implement it.

The record describes the committed dialect definition documents. It does
not define anything on its own: where this record and the canonical files
under `pkg/kdl/schemas/dialect/` disagree, the files win and this record
is out of date.

## Vocabulary

kdlith has three authoring levels. All three are KDL documents, and their
role, not their file extension, tells them apart.

| Level | Term               | Defines                                                            |
| ----- | ------------------ | ------------------------------------------------------------------ |
| 1     | Dialect definition | The language used to author schemas (this record)                  |
| 2     | Schema definition  | Domain types, node bindings, context effects, and processing rules |
| 3     | Data document      | Domain declarations, controls, imports, and records                |

A dialect definition is written in the dialect it defines, so its
declarations are read exactly like a schema definition's. Several
documents may contribute to one dialect or schema. Each one repeats the
same root name and version, and a declaration made twice across them is
rejected, not merged.

Other terms used below: a _binding_ maps KDL syntax into a logical
field; a _processing item_ is a record, declaration, contribution, import
request, or other typed event a processing session yields; _superproc_ is
the hand-maintained reference processor for this dialect.

## Design provenance

Every mechanism below traces to the kdlith design drafts that preceded
this record. Those drafts are not part of the repository; pointers name
the draft round and section so the reasoning can be found again:

| Pointer | Draft round                                                                                            |
| ------- | ------------------------------------------------------------------------------------------------------ |
| R1 §n   | Streaming framework: five components, bindings, context transitions, imports, pull parsing             |
| R2 §n   | Schema expressions: type expressions, bindings, codecs, defaults, context expressions, plans           |
| R3 §n   | Roles and declarations: roles, profiles, ordered maps, references, providers, contributions, discovery |
| F §n    | Framework and repository layout: authoring levels, superproc, fixtures                                 |

## File map

### Canonical dialect definition documents

Each file is a `dialect kdlith version="0"` document. Together they are
the only authority for the dialect.

| File                                    | Contributes                                                                                                                                                                         |
| --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pkg/kdl/schemas/dialect/types.kdl`     | Logical types, the type-expression grammar, built-in scalars and formats, field definitions, object constraints, generic type parameters, pattern codecs, field sensitivity         |
| `pkg/kdl/schemas/dialect/bindings.kdl`  | Roles, node declarations, field bindings and their sources, collision and duplicate-key policies, child cardinality, member-group expansion, dependent value binding, case dispatch |
| `pkg/kdl/schemas/dialect/context.kdl`   | Typed context slots, effect groups and targets, effects, context policies, execution timing                                                                                         |
| `pkg/kdl/schemas/dialect/documents.kdl` | Dialect and schema roots, containers, structural document rules, source imports, the deferral marker                                                                                |

### Dialect-generation entrypoint

| File                                  | Role                                                                                                                                                                                                                                                                             |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `contracts/super-schemas/dialect.kdl` | Selects the four canonical documents through `import` nodes, in the order types, bindings, context, documents. It declares nothing of its own and is never a second authority. It is the input to dialect generation (`kdlith gen dialect contracts/super-schemas/dialect.kdl`). |

### Fixture scenarios

Scenarios run under the `kdlith-fixtures` harness. The `{fx-suite}`
argument placeholder lets a scenario read a committed file in place
instead of a copy that could drift (see `tests/fixtures/README.md`).

| Scenario                                                        | Proves                                                   |
| --------------------------------------------------------------- | -------------------------------------------------------- |
| `tests/fixtures/dialect/canonical-read/types/scenario.json`     | `types.kdl` parses as KDL v2                             |
| `tests/fixtures/dialect/canonical-read/bindings/scenario.json`  | `bindings.kdl` parses as KDL v2                          |
| `tests/fixtures/dialect/canonical-read/context/scenario.json`   | `context.kdl` parses as KDL v2                           |
| `tests/fixtures/dialect/canonical-read/documents/scenario.json` | `documents.kdl` parses as KDL v2                         |
| `tests/fixtures/dialect/entrypoint-read/dialect/scenario.json`  | `contracts/super-schemas/dialect.kdl` parses as KDL v2   |
| `tests/fixtures/dialect/canonical-vet/*-local/scenario.json`    | Each canonical file passes local checking on its own     |
| `tests/fixtures/dialect/canonical-vet/merged/scenario.json`     | The four files link and describe themselves completely   |
| `tests/fixtures/dialect/entrypoint-vet/dialect/scenario.json`   | The entrypoint yields the same dialect as the four files |
| `tests/fixtures/dialect/canonical-bind/*/scenario.json`         | Each canonical file binds as data of the dialect         |
| `tests/fixtures/dialect/entrypoint-bind/dialect/scenario.json`  | The entrypoint binds to its four import requests         |
| `tests/fixtures/processing/bind/*/scenario.json`                | A small schema binds data documents into item streams    |

The `-read` scenarios prove syntax. The `-vet` scenarios run
`kdlith vet dialect`, which resolves imports, links every reference, and
walks each document through the dialect's own compiled bindings. The
entrypoint and the merged files must print the same digest of the
dialect's canonical summary. The `-bind` scenarios run `kdlith vet data`
with the dialect as the schema: each definition document is bound as a
data document of the dialect it is written in, through the same session
and binder every processor uses. Negative controls under
`tests/fixtures/invalid/dialect/`, `tests/fixtures/invalid/schemas/`, and
`tests/fixtures/invalid/data/` each name the diagnostic code of one
rejected construct.

## In-scope mechanisms

### Logical types (`types.kdl`)

| Mechanism                            | What the dialect declares                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | Draft        |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ |
| Logical types separate from bindings | Types say what a field is; bindings (below) say how it is spelled. A logical field is never inherently "argument 0" or "a child named x".                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | R1 §2, R2 §3 |
| Type declarations                    | `string`, `enum`, `object`, `union`, and `map` declarations (the `TypeDef` union), each inside a `types` container                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | R1 §2, R2 §2 |
| Type expressions                     | `TypeExpression`: a type-description grammar inside a quoted string, checked by the registered validator `type-expression` version `0`. It is not an expression evaluator.                                                                                                                                                                                                                                                                                                                                                                                                                                                         | R2 §2        |
| Type constructors                    | `optional`, `nullable`, `list`, `map`, `ordered-map`, `union`, `ref`. Fields are required unless optional; absence, null, and an empty list are distinct states.                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | R2 §2, R3 §4 |
| Generic type declarations            | A type declaration or codec may declare type parameters as `parameter T` children (uppercase identifiers, at least one, in order). A type expression instantiates a generic with `T<A, B>`; the compiler substitutes arguments for parameters when the definition compiles, so no generic survives into a compiled plan. Arity is checked at compile time, and an instantiation cycle — other than through `ref<T>`, which stays symbolic — is rejected with `KDLITH-SCH009`                                                                                                                                                       | R2 §2, R2 §5 |
| Ordered keyed collections            | `ordered-map<K, V>`: unique keys, declaration order retained                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | R3 §4        |
| Built-in scalars                     | `String`, `Bool`, `Integer`, `Decimal`, `U64`, and `ValueExpr` (any single KDL scalar, checked against the type of the field holding it)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | R2 §2, R2 §7 |
| Patterned strings                    | `string` types with `pattern` (whole-string match), `min-length`, a built-in `format` (`kebab-case`, `camel-case`, `lower-snake`, `upper-snake`, `uuid`, `zero-padded` with `width`), or a registered `validator` with a version                                                                                                                                                                                                                                                                                                                                                                                                   | R2 §2        |
| Pattern codecs                       | `codec NAME output=TYPE` inside `types`: a `match` child carries one regular expression that must match the whole string (no automatic trimming), `capture NAME TYPE` children type each named group, and the capture set must cover the match's named groups exactly. An optional `namespace NAME` records, as metadata, the reference namespace a decoded identity belongs to. A codec name is usable wherever a type expression names a declared type; the decoded value's logical type is `output=`, and at bind time a matching string decodes through the codec in the stream while provenance retains the original spelling | R2 §4, R2 §9 |
| Enum member metadata                 | Member descriptions remain metadata after compilation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | R2 §7        |
| Discriminated unions                 | A union variant is selected by the node name that spells it; no speculative parsing or backtracking                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | R1 §3        |
| Union dispatch by value prefix       | A variant may carry `prefix="…"`: for a union bound from a scalar value (an argument, a property, or a single-value child rather than a child node), the variant is selected when the value's text starts with the prefix. The prefixes of one union are checked pairwise at compile time (`KDLITH-SCH010`); once a prefix selects a variant, a failure inside it is an error, never a fallback to another variant                                                                                                                                                                                                                 | R2 §4        |
| Maps                                 | `map` declarations with `key-type` and `value-type` children                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | R1 §2        |
| Documented default versus fallback   | `documented-default` is metadata only and never materializes an absent value; `fallback` supplies an absent value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | R2 §10       |
| Field sensitivity                    | A logical field may carry `sensitivity=secret` or `sensitivity=reference`: `secret` marks a resolved sensitive value, `reference` an identifier that locates one, and a type modeling both keeps them as separate fields. Metadata is not enforcement: every serialization and diagnostic surface applies an explicit output policy — emit, omit, or redact — and a `secret` field is never emitted unless the policy explicitly says emit                                                                                                                                                                                         | R3 §8        |
| Object constraints                   | `all-or-none` and `requires`: enforceable policies an author states explicitly, never rules extracted from descriptions                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | R2 §10       |

### Bindings, roles, and cardinality (`bindings.kdl`)

| Mechanism                        | What the dialect declares                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | Draft         |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------- |
| Roles vocabulary                 | `record`, `declaration`, `contribution`, `context-control`, `source-import`, `container`. Structure (bindings) and role (processing) are separate. A declaration registers an identity others may reference; a record is validated data with no identity, such as the `document` rule.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | R3 §2         |
| Node declarations                | `node NAME` with optional `role=`, `type=`, and either `parent=` or a `parents` child. A node with neither is a root node. A node without `role=` binds as part of its parent (a field value or an envelope entry) and is not a processing item.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | R3 §2, R3 §3  |
| Parent-sensitive dispatch        | Dispatch is by parent, node name, and explicit discriminators, never by trying every schema until one accepts. The same name may declare different nodes under different parents (for example `children` under `node` versus under `from`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | R2 §7, R3 §3  |
| Binding shorthands               | `arg=N`, `property=NAME`, `child=NAME`, `context=SLOT` on a `field` binding                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | R2 §3, R3 §5  |
| Explicit sources                 | A `from` block listing `argument`, `args`, `property`, `child`, `children`, `entries`, and `context` sources; `child` and `children` sources may nest further sources                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | R2 §3         |
| Collection bindings              | `args` collects the remaining positional arguments; `children` collects repeated children in order                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | R2 §3         |
| Entries envelope                 | `entries NODE key="arg(0)" value=TYPE`: the selected argument supplies the map key and is consumed exactly once; the rest of the node binds as the value type                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | R2 §3         |
| Declaration-group expansion      | An `expand include target=TYPE member=NAME` source inside a `from` block splices the entries of the named keyed member of the named declaration into the collection being bound, at the expansion point, in declaration order; the authored short form is an `include NAME` child among like members. Expansion is demand-driven and cycle-checked (`KDLITH-SCH009`); each expanded entry's provenance records both the original declaration site and the inclusion site, and an expanded key colliding with a locally declared one is a duplicate-key rejection                                                                                                                                                                                                                                  | R3 §4         |
| Multiple spellings and collision | Several sources may populate one field; `collision reject` makes supplying more than one an error, even with equal values. There is no implicit precedence.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | R2 §3         |
| Dependent value binding          | A `value` block on a node declaration binds the node's remaining payload against a declaration's type: `against target=FIELD member=FIELD` names a field of the same node holding a reference and the member of the referenced declaration that carries the expected type, and `codec NAME` — a registered binding codec such as `argument-tail` (the remaining arguments, typed per expected type, with no scalar-to-list promotion) or `fixture-value` (the type-directed recursive form) — names how the payload binds to it. When the target declaration is compiled the payload binds immediately; otherwise the binder preserves the payload syntax and source locator and emits a pending typed-binding item that linking validates on demand — a pending binding is not a validated value | R2 §8, R3 §6  |
| Case dispatch                    | A `cases` block inside a node declaration selects the logical type a node binds as by property value: `case property=NAME equals=V type=TYPE` children, evaluated top-down, first match wins, and an optional `otherwise type=TYPE` is the fallback; a node whose properties match no case and that has no `otherwise` is rejected as unbound. Cases select among types of one node spelling; unions discriminate among node spellings and value prefixes                                                                                                                                                                                                                                                                                                                                         | R3 §8         |
| Duplicate keys                   | `duplicate-keys reject` on keyed collections                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | R1 §2, R2 §3  |
| Compile-time binding checks      | The compiler rejects overlapping bindings (`KDLITH-SCH003`, also reported when a definition is read as data of the dialect, so every processor of definitions rejects it), unconsumed arguments, and contradictory ownership of children                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | R2 §3         |
| Child permission and cardinality | A child is permitted only when a field binding consumes it or a `children NAME` rule names it. `min-occurs` and `max-occurs` bound occurrences; minimums are checked at scope completion. These are schema occurrence rules, never reinterpretations of authored data that uses a property named `min-occurs` or `max-count`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | R1 §3, R3 §7  |
| Compiled plans                   | Bindings compile into a plan; the schema's own KDL is not reinterpreted per record                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | R1 §2, R2 §12 |

### Context (`context.kdl`)

| Mechanism                              | What the dialect declares                                                                                                                                                                                                                                                                                                                                                                                                        | Draft         |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------- |
| Engine-managed, schema-defined context | The engine manages context but never hardcodes what a context-producing node means; names like `package` or `defaults` gain meaning only through a schema definition                                                                                                                                                                                                                                                             | R1 §4         |
| Typed slots                            | `context { slot NAME TYPE }` with an optional namespace and description; values set into a slot are validated against its type                                                                                                                                                                                                                                                                                                   | R2 §11        |
| Effect targets                         | `effects target=following` and `effects target=children`; a node affecting both declares two groups                                                                                                                                                                                                                                                                                                                              | R1 §4, R2 §11 |
| Generic effects                        | `set` (from a bound field or a literal value, exactly one), `remove` (the slot becomes unset, not null), `clear` (every slot in a namespace)                                                                                                                                                                                                                                                                                     | R1 §4         |
| Registered handlers                    | `handler NAME` names domain computation implemented by the processor that installs it; a schema naming a handler that is not installed fails clearly                                                                                                                                                                                                                                                                             | R1 §4         |
| Context-bound fields and policies      | A field may read a slot (`context=` shorthand or a `context` source) under policy `fallback` (supplies the field only when no explicit value is present) or `fixed` (an explicit value must agree)                                                                                                                                                                                                                               | R1 §4, R2 §10 |
| Execution timing                       | A node is bound against its incoming context, then its effects are computed. Children effects apply on entering the children and must be computable from the node's header; one that depends on a child field is rejected as incompatible with streaming. Following effects apply only after the node completes successfully. Leaving a boundary restores the caller's context. A late effect never changes nodes already bound. | R1 §4, R2 §11 |

### Documents, structure, and imports (`documents.kdl`)

| Mechanism                        | What the dialect declares                                                                                                                                                                                                                                                                                                                                               | Draft                                 |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| Dialect and schema roots         | `dialect NAME version=V` and `schema NAME version=V` containers holding `types`, `node`, `context`, `document`, and `import` (plus `deferred` under `dialect`)                                                                                                                                                                                                          | F §1                                  |
| Contributing documents           | Several documents contribute to one dialect or schema by repeating its root name and version; a repeated declaration is rejected                                                                                                                                                                                                                                        | F §1                                  |
| First-node rule                  | `document { first { exactly-one A B } }`: the first active semantic node (after comments and slashdash-suppressed syntax) must be exactly one of the listed nodes; an import may neither precede nor supply it                                                                                                                                                          | R1 §3                                 |
| Following rule                   | `document { following ... }` names the nodes permitted after the first                                                                                                                                                                                                                                                                                                  | R1 §3                                 |
| Exactly-one groups               | `exactly-one A B` inside a node declaration: exactly one of the named children must appear, checked at scope completion                                                                                                                                                                                                                                                 | R1 §3                                 |
| Source imports                   | `import PATH` with role `source-import`. The path is literal and resolved relative to the importing document, never computed. The imported document receives only the `project` list of the caller's slots, and its context mutations never return to the caller. Cycles are diagnosed from the active invocation stack; import order is the order of the import nodes. | R1 §5, R2 §11                         |
| Repeated imports                 | `repeated=reject` (the fallback), `coalesce` (equal context reuses the first invocation), or `instantiate` (each import is a separate invocation)                                                                                                                                                                                                                       | R1 §5                                 |
| Deferral marker                  | `deferred NAME note="..."` names a mechanism the dialect does not define at this version, so its absence is explicit; a schema definition relying on a deferred mechanism is rejected, never silently ignored                                                                                                                                                           | R1 "Recommended first implementation" |
| This dialect's own document rule | A dialect definition has exactly one `dialect` root; a schema definition has exactly one `schema` root                                                                                                                                                                                                                                                                  | F §1                                  |
| Document rules are records       | `document` has role `record`: a document rule is a validated statement about the documents a definition reads, with no identity of its own, so it yields a record item rather than a declaration. It occurs at most once per root; its rules are listed in the rows above.                                                                                              | R3 §2                                 |

### Conventions the canonical files follow

- **Envelope keys.** Inside an `entries` envelope, argument 0 of each
  entry node is the map key, so entry types (`EnumMember`, `FieldDef`,
  `Variant`) carry no name field and their own bindings start at
  argument 1.
- **Self-description.** Every construct the canonical files use is itself
  declared by them.

## Deferred mechanisms

Each deferral appears in the canonical files as a `deferred` node, so
the boundary is explicit rather than implied. All 12 are listed here.

| Mechanism                 | Declared in     | Note in the file                                                                                 | Draft         |
| ------------------------- | --------------- | ------------------------------------------------------------------------------------------------ | ------------- |
| `source-context`          | `context.kdl`   | Context values inferred from source metadata such as the file path                               | R1 §5, R2 §11 |
| `accumulating-context`    | `context.kdl`   | Accumulating context values combined by a declared operation                                     | R1 §4         |
| `context-snapshots`       | `context.kdl`   | Persisted context snapshots for on-demand resolution of indexed records                          | R1 §7         |
| `profiles`                | `documents.kdl` | Document profiles: named root and node selections chosen per document                            | R3 §3         |
| `descendant-rules`        | `documents.kdl` | Structural rules over descendants rather than immediate children                                 | R1 §3         |
| `ordered-sequences`       | `documents.kdl` | Ordered child sequences and alternatives                                                         | R1 §3         |
| `discovery-grammars`      | `documents.kdl` | Glob patterns, brace alternatives, and path anchors in imports                                   | R3 §10        |
| `import-aliases`          | `documents.kdl` | Import aliases that affect symbol visibility                                                     | R3 §5         |
| `symbol-catalogs`         | `documents.kdl` | Identity domains, qualified identities, and reference lookup policies beyond declaration by name | R3 §5, R3 §11 |
| `contribution-resolution` | `documents.kdl` | Resolution of contributions across layers                                                        | R3 §9         |
| `computed-defaults`       | `documents.kdl` | Defaults computed by a provider at run time; descriptors only                                    | R3 §8         |
| `secret-providers`        | `documents.kdl` | Secret-provider descriptors and their sensitivity policy; never executed                         | R3 §8         |

Several in-scope mechanisms stop short of their deferred extensions.
`ref<T>` is a symbolic reference to a declared entity found by name;
identity domains, qualification, and lookup policies are
`symbol-catalogs`, and a codec's `namespace` metadata is recorded now
and consumed only when that mechanism lands. The `contribution` role
preserves an assignment for later resolution; dependent binding defines
how such an assignment's payload binds against its target's declared
type, while resolving contributions across layers remains
`contribution-resolution`. Expansion includes a member group by naming
its declaring type; which declaration a name denotes across imports and
layers is still `symbol-catalogs`. Dependent binding's `against` looks
up a target bound in the same node; looking a key up in a provider-held
schema table (the `context=` form) stops short until providers are
defined. Generic types monomorphize at compile time; parameter
constraints, defaults, and variadic parameters are not defined.

## Import resolution

The engine resolves `import` nodes when it loads a definition with
complete checking. Paths are resolved relative to the importing document
and followed in written order, and every document reached contributes to
the one definition. An import that does not resolve fails with
`KDLITH-IMP001`, a cycle on the active invocation stack with
`KDLITH-IMP002`, and a repeat under `repeated=reject` with
`KDLITH-IMP003`. Each imported document gets a context of its own holding
only the projected slots, so its mutations never reach the caller. Local
checking (`--scope local`) reads the named documents alone and follows
no imports.

## Contract for the engine and the reference processor

### Engine and runtime

The engine (`pkg/rs/engine`) and runtime (`pkg/rs/runtime`) implement the
dialect for every processor, hand-written or generated:

1. **Compile once.** Compile dialect and schema definitions into catalogs
   and a processing plan; never reinterpret a definition's KDL per record.
2. **Merge contributions.** Combine documents that repeat one root name
   and version into one dialect or schema, and reject a declaration made
   twice.
3. **Resolve imports** as `documents.kdl` states: literal paths relative
   to the importing document, written order, projection only, mutations
   never escaping, cycles diagnosed from the invocation stack, and
   `repeated` defaulting to `reject`. Once this lands, processing
   `contracts/super-schemas/dialect.kdl` must yield the same dialect as
   reading the four canonical files directly, and that equivalence gets
   its own fixture scenario.
4. **Link.** Check that every type, node, slot, and parent a definition
   references is declared, including the canonical files' own
   self-description.
5. **Enforce bindings.** Reject colliding sources, duplicate keys,
   overlapping bindings, unconsumed arguments, contradictory child
   ownership, and unpermitted children; check cardinality, exactly-one
   groups, and the first-node rule at the points stated above.
6. **Dispatch deterministically** by parent, node name, and explicit
   discriminators; the discriminators are node names, variant value
   prefixes, and property cases, each consulted as a compiled table with
   a single probe and no backtracking.
7. **Apply context** with the execution timing in `context.kdl`, and
   reject children effects that are not computable from the node header.
8. **Fail closed.** Reject a definition that relies on a deferred
   mechanism or names a handler or validator that is not installed. The
   `type-expression` validator at version `0` is required.
9. **Decode codecs at bind time.** Apply a field's codec while the value
   arrives, keep the original spelling in provenance, and reject a
   string that fails its match or a capture that fails its declared type
   as a decode failure. Type-directed binding codecs (`fixture-value`,
   `argument-tail`) are registered by the processor like validators and
   handlers, so naming one that is not installed fails closed.
10. **Expand member groups** demand-driven: a compiled plan may carry an
    unresolved expansion reference, complete checking resolves every
    one, and a cycle in the expansion or generic-instantiation graph is
    `KDLITH-SCH009`.
11. **Bind dependent payloads** by looking up the referenced
    declaration's member type — immediately when the declaration is
    compiled, otherwise as a pending typed-binding item that linking
    validates on demand — and never promote a scalar to a one-element
    list.
12. **Enforce sensitivity.** Apply the caller's output policy (emit,
    omit, redact) to sensitive fields in every serialization and
    diagnostic surface; a `secret` field is emitted only when the policy
    explicitly says emit.

### Self-hosted compilation

Loading a definition is reading it: `vet` and `inspect` both hand their
entry documents to the reference processor's typed reader, and the
engine compiles what the reader gives it — one construct per
declaration, already validated by binding, with the span it was written
at. The reader binds dialect documents against the canonical dialect
(the definition language every dialect is written in) and schema
documents against their governing dialect, so a construct the governing
dialect does not admit is rejected with the binding code any processor
binding it reports.

One cycle cannot close: a reader needs a compiled dialect before it can
read one, so the canonical dialect's own compilation cannot flow through
a reader built on it. `superproc::seed` breaks it the only way possible —
the engine's seed bootstrap (`compile_dialect`), the one remaining
hand-written reader of definition syntax, compiles the four canonical
documents embedded beside the reference processor's declarations. The
bootstrap compiles nothing else, and the equivalence it exists for is
fixture-pinned: reading the canonical files through the reader they seed
reproduces the seed's own digest.

### Binding data documents

The engine's binder (`pkg/rs/engine/src/bind/`) is the runtime processor
that turns a data document into processing items. It is built from a
compiled definition once and never reads the definition's KDL:

- **Dispatch.** Each node is matched by its parent's compiled scope and
  its name, never by trying declarations in turn. The scope's structural
  rules admit it as it arrives; minimum counts, exactly-one groups, and
  the document's first-node rule are checked when the scope completes.
- **Fields.** Arguments, properties, and children land in the fields
  their bindings name and are decoded against the field's type as they
  arrive. An entries envelope consumes its key argument once; a repeated
  key under `duplicate-keys reject` and a second source under
  `collision reject` are rejected. A union value is kept under the name
  of the node that spelled its variant. A value whose type is a codec
  decodes through it as it arrives, and a node with a `cases` block
  binds under the type its first matching case selects.
- **Context.** A context-bound field takes the slot's value only when no
  explicit value is present (`fallback`), and an explicit value must
  agree with the slot (`fixed`). A field read only from context fails
  where no control provides the slot. Effects follow the execution
  timing above, and a node that fails applies no following effects.
- **Items.** A bound node yields the item its role names: a record, a
  declaration with its identity, a contribution preserved unresolved, an
  import request, a pending typed binding awaiting its declaration, or a
  container's scope completion. Context controls
  yield their effects only. Fields holding references stay provisional
  until linking resolves them. Every diagnostic is an item in the same
  stream.

Literals a definition writes (fallbacks, documented defaults, and `set`
values) are decoded against their types when the definition compiles,
through the same decoder.

### One code per rejected construct

A definition document is a data document of its dialect. A construct
the dialect's declarations reject therefore gets one diagnostic code on
every reading path: when the engine compiles the definition (`vet`) and
when a processor such as superproc binds it (`inspect`).

| Rejected construct                                                                                | Code            |
| ------------------------------------------------------------------------------------------------- | --------------- |
| A node no binding accepts in its position, including a missing or extra root                      | `KDLITH-BND001` |
| A child that occurs more or fewer times than its rule allows, including the first-node rule       | `KDLITH-BND002` |
| A value that does not decode as its declared type, such as a name with the wrong format           | `KDLITH-BND003` |
| A required field or argument that is absent, such as a definition root without `version=`         | `KDLITH-BND004` |
| An argument, property, or type annotation that no binding consumes                                | `KDLITH-BND005` |
| A repeated key of a keyed collection, such as two fields of one object or two bindings of a field | `KDLITH-BND006` |
| A value that names no member of its enum, such as an unknown `role=` or string `format`           | `KDLITH-BND008` |
| A type expression that breaks the type-expression grammar                                         | `KDLITH-DLC007` |
| A spelling of a deferred mechanism (a node, a property, or a policy value)                        | `KDLITH-DLC003` |
| Two fields bound to the same argument, property, or rest arguments                                | `KDLITH-SCH003` |
| An import that projects a slot the dialect does not declare                                       | `KDLITH-SCH006` |
| An expansion or generic-instantiation cycle in a definition                                       | `KDLITH-SCH009` |
| Variant prefixes of one union where one is a prefix of another                                    | `KDLITH-SCH010` |

A bad type expression (`KDLITH-DLC007`) and an unknown enum value
(`KDLITH-BND008`) are distinct from a plain value-type mismatch
(`KDLITH-BND003`). A codec string that fails its match, a capture that
fails its declared type, and a value-bound union that matches no variant
prefix are decode failures (`KDLITH-BND003`); a node whose properties
match no case of its `cases` block and that has no `otherwise` is
unbound (`KDLITH-BND001`). Every definition root — dialect or schema —
states its `version` property; a root without one is a missing required field,
rejected with `KDLITH-BND004` on both reading paths.

The remaining definition codes (`KDLITH-SCH001`, `KDLITH-SCH002`,
`KDLITH-SCH004`, `KDLITH-SCH005`, `KDLITH-SCH007`, `KDLITH-SCH008`,
`KDLITH-DLC005`, and `KDLITH-DLC006`) come from linking, plan building,
and extension checks that only the compiler runs. That split is a
deliberate product decision, not an accident: `inspect` runs the reader
path and performs only the checks the dialect's declarations own, so it
accepts an input whose only faults are compiler-only checks, while
`vet` hands the same reading to the compiler and adds them. Fixture
scenarios pin that acceptance, so it is committed behavior rather than
an accident. A node-level check such as an overlapping binding sits at the same
span on both reading paths: the reader-path binder check reports it at
the node declaration, and it fires first in `vet`, so the compiler's
field-span report of the same fault is not reached.

### superproc

The reference processor (`pkg/rs/superproc`) reads schema definitions in
this dialect:

1. It reads every canonical file and the entrypoint end to end and yields
   the expected declaration and record items.
2. It holds only the dialect's own type and binding declarations plus
   glue. Every reusable decision lives in the engine or runtime.
3. It hosts the compiler entry point (`load_dialect`, `load_schema`,
   `seed`): the engine compiles the item stream this processor reads,
   as [Self-hosted compilation](#self-hosted-compilation) records.
4. Conformance fixtures bind positive cases to expected items, and
   negative controls each name the diagnostic codes of one rejected
   construct.
5. It is the code-generation oracle: generating from
   `contracts/super-schemas/dialect.kdl` must reproduce its source, apart
   from the defined generated-marker header. It is reviewed by hand and
   never overwritten by the generator.

## Changing the dialect

Change the canonical files first, then this record. Moving a mechanism
from deferred to in scope means removing its `deferred` node, declaring
the mechanism in the same file, adding fixture coverage, and moving its
row from [Deferred mechanisms](#deferred-mechanisms) to the matching
in-scope table.
