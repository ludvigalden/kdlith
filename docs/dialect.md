# The kdlith dialect, version 0

This is the design record for version 0 of kdlith's schema-definition
dialect: the language schema definitions are written in. It lists every
mechanism the dialect defines, maps the committed files that carry them,
and states what the engine and the reference processor must honor when
they implement it. No mechanism is deferred at this version; the
[deferral disposition](#deferral-disposition) states how the nineteen
deferrals version 0 carried were retired.

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

| File                                    | Contributes                                                                                                                                                                                                                                                            |
| --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pkg/kdl/schemas/dialect/types.kdl`     | Logical types, the type-expression grammar, built-in scalars and formats, field definitions, object constraints, generic type parameters, pattern codecs, field sensitivity                                                                                            |
| `pkg/kdl/schemas/dialect/bindings.kdl`  | Roles, node declarations, field bindings and their sources, collision and duplicate-key policies, child cardinality, member-group expansion, dependent value binding, case dispatch                                                                                    |
| `pkg/kdl/schemas/dialect/context.kdl`   | Typed context slots, effect groups and targets, effects, context policies, execution timing, source-context declarations, accumulating context, context snapshots                                                                                                      |
| `pkg/kdl/schemas/dialect/documents.kdl` | Dialect and schema roots, containers, structural document rules, source imports with their discovery grammars and aliases, document profiles, descendant rules, ordered sequences, symbol catalogs and reference policies, contribution policies, provider descriptors |

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
dialect's canonical summary. The digest covers the dialect documents in
the order the command receives them, so both invocations pass the four
canonical files in the same order: `types.kdl`, `bindings.kdl`,
`context.kdl`, `documents.kdl`. The `-bind` scenarios run `kdlith vet data`
with the dialect as the schema: each definition document is bound as a
data document of the dialect it is written in, through the same session
and binder every processor uses. Negative controls under
`tests/fixtures/invalid/dialect/`, `tests/fixtures/invalid/schemas/`, and
`tests/fixtures/invalid/data/` each name the diagnostic code of one
rejected construct. The mechanisms this expansion defines carry the same
coverage discipline: at least one positive bind scenario per mechanism
and one negative control per new code (`KDLITH-SCH011`, `KDLITH-SCH012`,
`KDLITH-SCH013`, `KDLITH-IMP004`,
`KDLITH-IMP005`, `KDLITH-CTX006`, `KDLITH-CTX004`, `KDLITH-REF005`,
`KDLITH-BND010`, `KDLITH-BND011`; the retired
`KDLITH-CTX003` is subsumed by `KDLITH-CTX006`, whose negative control is
the `accumulate-no-op` scenario), landing with the
canonical declarations that define them.

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
| Object constraints                   | `all-or-none` and `requires`: enforceable policies an author states explicitly, never rules extracted from descriptions. The binder enforces them on every type a node binds as (`KDLITH-BND011`); `#null` and an empty collection are supplied values, only a field holding none is absent                                                                                                                                                                                                                                                                                                                                        | R2 §10       |

### Bindings, roles, and cardinality (`bindings.kdl`)

| Mechanism                        | What the dialect declares                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | Draft         |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------- |
| Roles vocabulary                 | `record`, `declaration`, `contribution`, `context-control`, `source-import`, `container`. Structure (bindings) and role (processing) are separate. A declaration registers an identity others may reference; a record is validated data with no identity, such as the `document` rule.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | R3 §2         |
| Node declarations                | `node NAME` with optional `role=`, `type=`, and either `parent=` or a `parents` child. A node with neither is a root node. A node without `role=` binds as part of its parent (a field value or an envelope entry) and is not a processing item.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | R3 §2, R3 §3  |
| Parent-sensitive dispatch        | Dispatch is by parent, node name, and explicit discriminators, never by trying every schema until one accepts. The same name may declare different nodes under different parents (for example `children` under `node` versus under `from`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | R2 §7, R3 §3  |
| Binding shorthands               | `arg=N`, `property=NAME`, `child=NAME`, `context=SLOT` on a `field` binding                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | R2 §3, R3 §5  |
| Explicit sources                 | A `from` block listing `argument`, `args`, `property`, `child`, `children`, `entries`, and `context` sources; `child` and `children` sources may nest further sources                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | R2 §3         |
| Collection bindings              | `args` collects the remaining positional arguments; `children` collects repeated children in order                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | R2 §3         |
| Entries envelope                 | `entries NODE key="arg(0)" value=TYPE`: the selected argument supplies the map key and is consumed exactly once; the rest of the node binds as the value type                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | R2 §3         |
| Declaration-group expansion      | An `expand include target=TYPE member=NAME` source inside a `from` block splices the entries of the named keyed member of the named declaration into the collection being bound, at the expansion point, in declaration order; the authored short form is an `include NAME` child among like members. Expansion is demand-driven and cycle-checked (`KDLITH-SCH009`); each expanded entry's provenance records both the original declaration site and the inclusion site, and an expanded key colliding with a locally declared one is a duplicate-key rejection. Every reading path splices the short form: the typed reader streams each `include` child as an `expansion` item at its position, carrying its owner and how many like members precede it, and the compiler's one expansion pass splices there; the `effective` view shows each spliced group's two sites. The declaration-group splice is the whole built `include` mechanism: source discovery, which shares the spelling in the drafts, remains the anchors and profile grammar's mechanism and is not built as an `include`                                                                                                                                                                            | R3 §4         |
| Multiple spellings and collision | Several sources may populate one field; `collision reject` makes supplying more than one an error, even with equal values. There is no implicit precedence.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | R2 §3         |
| Dependent value binding          | A `value` block on a node declaration binds the node's remaining payload against a declaration's type: `against target=FIELD member=FIELD` names a field of the same node holding a reference and the member of the referenced declaration that carries the expected type, and `codec NAME` — a registered binding codec such as `argument-tail` (the remaining arguments, typed per expected type, with no scalar-to-list promotion) or `fixture-value` (the type-directed recursive form) — names how the payload binds to it. When the target declaration is compiled the payload binds immediately; otherwise the binder emits a pending typed-binding item right after the item holding its reference, carrying the payload as a raw value — the payload's one value when there is exactly one, otherwise a list of the values in order — under the node and field the binding names. A pending binding is not a validated value and nothing executes it: under complete scope linking resolves its reference and validates the raw payload against the resolved declaration's member type, failing with `KDLITH-REF005`; the bound view shows it, export and `read data` exclude it, and typed item readers carry it through as a `pending-binding` item of their own | R2 §8, R3 §6  |
| Case dispatch                    | A `cases` block inside a node declaration selects the logical type a node binds as by property value: `case property=NAME equals=V type=TYPE` children, evaluated top-down, first match wins, and an optional `otherwise type=TYPE` is the fallback; a node whose properties match no case and that has no `otherwise` is rejected as unbound. Cases select among types of one node spelling; unions discriminate among node spellings and value prefixes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | R3 §8         |
| Duplicate keys                   | `duplicate-keys reject` on keyed collections                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | R1 §2, R2 §3  |
| Compile-time binding checks      | The compiler rejects overlapping bindings (`KDLITH-SCH003`, also reported when a definition is read as data of the dialect, so every processor of definitions rejects it), unconsumed arguments, and contradictory ownership of children                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | R2 §3         |
| Child permission and cardinality | A child is permitted only when a field binding consumes it or a `children NAME` rule names it. `min-occurs` and `max-occurs` bound occurrences; minimums are checked at scope completion. These are schema occurrence rules, never reinterpretations of authored data that uses a property named `min-occurs` or `max-count`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | R1 §3, R3 §7  |
| Compiled plans                   | Bindings compile into a plan; the schema's own KDL is not reinterpreted per record                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | R1 §2, R2 §12 |

### Context (`context.kdl`)

| Mechanism                              | What the dialect declares                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | Draft         |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------- |
| Engine-managed, schema-defined context | The engine manages context but never hardcodes what a context-producing node means; names like `package` or `defaults` gain meaning only through a schema definition                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | R1 §4         |
| Typed slots                            | `context { slot NAME TYPE }` with an optional namespace and description; values set into a slot are validated against its type                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | R2 §11        |
| Effect targets                         | `effects target=following` and `effects target=children`; a node affecting both declares two groups                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | R1 §4, R2 §11 |
| Generic effects                        | `set` (from a bound field or a literal value, exactly one), `remove` (the slot becomes unset, not null), `clear` (every slot in a namespace)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | R1 §4         |
| Registered handlers                    | `handler NAME` names domain computation implemented by the processor that installs it; a schema naming a handler that is not installed fails clearly. A handler's effects are checked when it runs: one that sets or removes a slot, or clears a namespace, the definition does not declare is rejected at the effect point (`KDLITH-CTX005`) — a run-time fault of the installed handler, distinct from the malformed-construct code a definition fault carries                                                                                                                                                                                                                                                                                                                                                         | R1 §4         |
| Context-bound fields and policies      | A field may read a slot (`context=` shorthand or a `context` source) under policy `fallback` (supplies the field only when no explicit value is present) or `fixed` (an explicit value must agree)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | R1 §4, R2 §10 |
| Execution timing                       | A node is bound against its incoming context, then its effects are computed. Children effects apply on entering the children and must be computable from the node's header; one that depends on a child field is rejected as incompatible with streaming. Following effects apply only after the node completes successfully. Leaving a boundary restores the caller's context. A late effect never changes nodes already bound.                                                                                                                                                                                                                                                                                                                                                                                         | R1 §4, R2 §11 |
| Source context                         | A `source-context` block in a schema definition declares fields derived from source metadata — `source-context { field NAME TYPE { from { ... } } }` — where sources such as `path-stem` name registered source-metadata providers the processor installs (an unregistered source fails closed). The block declares its mode — `initial`, `fallback`, or `fixed` — explicitly; there is no default, so an undeclared mode is a missing required property (`KDLITH-BND004`), never a silent fallback. Values apply before the document's first semantic node, and every derived value records its origin in provenance, so filename inference never becomes an invisible source of precedence.                                                                                                                            | R1 §5, R2 §11 |
| Accumulating context                   | A slot may declare an accumulation policy naming its combining operation: `append` for list-typed slots (the incoming list concatenates in effect order) and `merge` for map-typed slots (key-wise). `merge` declares how a key contributed twice is treated: `accumulate merge duplicates=reject`, the default, rejects it as a duplicate key (`KDLITH-BND006`), never a silent overwrite; `duplicates=last-wins` replaces the held entry's value and keeps the key's first position. `remove` still unsets and `clear` still clears; accumulation is a third policy, not a replacement for either.                                                                                                                                                                                                                     | R1 §4         |
| Slot duplicate policies                | A slot that does not accumulate declares how a second value reaching it is treated: `slot NAME TYPE duplicates=reject`, the fail-closed default, rejects it (`KDLITH-CTX006`); `duplicates=last-wins` replaces the held value; `duplicates=layered` orders values by declared layer. The held value is the slot's incoming one, inherited by a children scope, projected into an import, or set earlier in the scope; an equal value is still a second value. `remove` and `clear` unset, so a later set is a first one.                                                                                                                                                                                                                                                                                                 | R1 §4, R3 §9  |
| Layered slots                          | A layered slot lists its layers highest precedence first (`layers override base`), and every `set` on it names one (`set NAME field=F layer=L`). A higher layer's value replaces a lower one's, a lower layer's leaves the held value in place, and a second value in one layer, or meeting a held value without a layer, is rejected (`KDLITH-CTX006`), so the result does not depend on effect order. A layer on another slot's set, a layered set with none, or layers without `layered` is malformed.                                                                                                                                                                                                                                                                                                                | R3 §9         |
| Context snapshots                      | The engine persists context as shared immutable snapshots or checkpoint-plus-delta chains and records, per indexed record, its identity and type, source digest and span, schema fingerprint, context snapshot ID, invocation ID, and validation status. On-demand resolution restores the snapshot and binds the recorded span instead of replaying every preceding control. A restore whose schema fingerprint or invocation lineage does not match the requesting context is rejected (`KDLITH-CTX004`), and a file without a prior scan or a trustworthy checkpoint cannot be entered at an arbitrary node: forward context makes that impossible by construction, not by policy. `cache refresh` writes these index entries and `read data --state` restores from them (see [Snapshot restore](#snapshot-restore)). | R1 §7         |

### Documents, structure, and imports (`documents.kdl`)

| Mechanism                        | What the dialect declares                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | Draft                 |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------- |
| Dialect and schema roots         | `dialect NAME version=V` and `schema NAME version=V` containers holding `types`, `node`, `context`, `document`, and `import`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | F §1                  |
| Contributing documents           | Several documents contribute to one dialect or schema by repeating its root name and version; a repeated declaration is rejected                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | F §1                  |
| First-node rule                  | `document { first { exactly-one A B } }`: the first active semantic node (after comments and slashdash-suppressed syntax) must be exactly one of the listed nodes; an import may neither precede nor supply it                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | R1 §3                 |
| Following rule                   | `document { following ... }` names the nodes permitted after the first                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | R1 §3                 |
| Exactly-one groups               | `exactly-one A B` inside a node declaration: exactly one of the named children must appear, checked at scope completion                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | R1 §3                 |
| Source imports                   | `import PATH` with role `source-import`, optionally `as=NAME` (aliases below). The path is a literal or a grammar-admitted expression under the bound discovery grammar (discovery below), resolved relative to the importing document and never computed beyond that restricted expansion. The imported document receives only the `project` list of the caller's slots, and its context mutations never return to the caller. Cycles are diagnosed from the active invocation stack; import order is the order of the import nodes.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | R1 §5, R2 §11, R3 §10 |
| Repeated imports                 | `repeated=reject` (the fallback), `coalesce` (equal context reuses the first invocation), or `instantiate` (each import is a separate invocation)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | R1 §5                 |
| Document profiles                | `profile NAME { roots { allow TYPE ... } }` declarations name selections of roots and nodes, and a node declaration may carry `profile=NAME` so one node name binds differently per profile (the draft's `program` as a definition under `program-definition` and as an overlay under `device-config`). Dispatch is by profile, parent binding, node name, and explicit discriminators — never by trying profiles until one accepts. A profile may also carry `extends=NAME`, `default=#true`, and a `match { ... }` child of rules. **Inheritance.** A profile's effective declaration composes along its `extends` chain, child over parent: its effective roots are the union of its own and its parent's effective roots, and are unrestricted when either declares no `roots`; its effective match rules are its own in written order followed by its parent's effective rules; its own `grammar` overrides the inherited one and an absent one is inherited; `default` is never inherited. A node declaration carrying `profile=P` is admitted under `P` and under every profile whose `extends` chain reaches `P`. An `extends` naming an undeclared profile is an undeclared reference (`KDLITH-SCH006`), and an `extends` chain that returns to a profile already on it, including a profile extending itself, is an inheritance cycle (`KDLITH-SCH012`). **Default.** `default=#true` marks the schema's default profile (`#false` is the same as absent); a second profile marked default is rejected (`KDLITH-SCH013`). **Match rules.** `path-suffix TEXT`, `path-prefix TEXT`, and `file-name TEXT` compare the processed document's source path as the invocation spells it — the same path the `path-stem` source-context provider reads — split into components on `/` and `\`: `path-suffix` matches when the path's trailing components equal TEXT's components, `path-prefix` when its leading components do, and `file-name` when its final component, extension included, equals TEXT. Comparison is exact and case-sensitive; an empty TEXT fails its type (`KDLITH-BND003`), and a `match` block holding no rule is malformed (`KDLITH-DLC005`). **Selection.** Each entry document's profile is resolved once, before any of its nodes binds, in one total order: an explicitly named profile wins; otherwise the first rule that matches the entry's source path selects its profile, taking profiles in declaration order (contribution order across a schema's documents) and each profile's effective rules in order; otherwise the schema's default profile; otherwise selection fails (`KDLITH-SCH011`). An explicit name the schema does not declare is rejected (`KDLITH-SCH011`) and never falls through to inference or the default. **Profile sets.** An invocation may name a profile set instead of one profile (`--profiles NAME[,NAME...]` on every data verb; naming both, or listing an empty or non-kebab name, is an argument-form fault, `KDLITH-CLI001`). Every listed name must be declared: an undeclared member is rejected (`KDLITH-SCH011`) and never falls through to the remaining members. The set then resolves through the same shared selection, in order: every listed profile whose effective rules match the entry's source path is active, in listed order; otherwise the listed profile carrying the default mark; otherwise the first listed. A set never consults the schema's default or inference over profiles it does not list. Under several active members a root is admitted when any member's effective roots admit it, a declaration carrying `profile=P` is admitted when `P` is on any member's `extends` chain, and the first active member's grammar governs imports. With neither an explicit name nor a set, selection is the inference-then-default order above. The order is total, so two rules matching one path never conflict and no ambiguity code exists. Inference reads invocation context — the source path and its components — and never document content, so the dispatch rule above still holds: documents of identical content under one path always select the same profile. A document an import reaches binds under the profile its importing invocation selected. A schema that declares no profiles admits every declared root as before. A node the active profile does not admit is rejected as unbound (`KDLITH-BND001`). A declaration's `profile=` must name a profile the schema declares; an undeclared name is an undeclared reference (`KDLITH-SCH006`), since the declaration could bind under no invocation. | R3 §3                 |
| Descendant rules                 | Structural rules may range over descendants rather than immediate children: a node declaration bounds occurrences of a named node anywhere in its subtree, checked when the declaring scope completes. Descendant rules stop at the subtree of their declaring node and never cross an import boundary — an imported document is a separate invocation with its own scopes. A violation reports `KDLITH-BND002` like any occurrence rule.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | R1 §3                 |
| Ordered sequences                | A node declaration may require its immediate children to match an ordered sequence with alternatives (`sequence` steps naming nodes, or alternatives of nodes), compiled into the same per-scope state machine as the other structural rules; a child arriving where the compiled sequence does not admit it is rejected (`KDLITH-BND002`). Repetition stays with `min-occurs` and `max-occurs` on a step; there are no unbounded-repetition groups at this version.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | R1 §3                 |
| Discovery grammars               | `discovery NAME { anchor NAME provider=... }` declarations with a `patterns` child (`star`, `doublestar`, `braces`) and an `order` policy define the grammar import expressions are evaluated under; the active profile (or the schema root, when no profiles are declared) binds the grammar, and a schema binding none evaluates literal paths only. Expansion is restricted path expansion: anchors are host-supplied typed paths named by the grammar, and environment-variable interpolation is admitted by no grammar (`KDLITH-IMP004`). An anchor the grammar does not declare, or declares but the host does not supply, is rejected (`KDLITH-IMP005`). Matched order follows the declared ordering policy (`portable-path-bytes` is the only defined policy at this version, and the default); each result retains the original expression, resolved anchor values, ordered matched sources, import-site provenance, and source revisions; an expression matching zero sources does not resolve (`KDLITH-IMP001`) unless the grammar declares an explicit empty-match allowance; and the resulting inventory is itself a dependency — adding a matching file changes the result even when previously matched files are unchanged.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | R3 §10                |
| Import aliases                   | An import may carry `as=NAME`: the import request preserves both the source and its alias, and a resolved request records the shown paths of the targets it loaded. A document imported under an alias resolves its own declarations by their unqualified names; every other document reaches them only through the alias namespace. An alias must be unique in its document (a repeat is a duplicate-key rejection, `KDLITH-BND006`), and a reference through an alias that linking cannot resolve is an unresolved reference (`KDLITH-REF001`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | R3 §5                 |
| Symbol catalogs                  | Declarations carry identity: an `identity { domain NAME compose qualified-name FIELD FIELD }` block on a declaration composes its qualified symbol from bound fields (a `namespace` slot plus a local name, for example), and `reference NAME target=TYPE` declarations state a reference's registered syntax codec, lookup domain, and qualification policy (`explicit`, `explicit-or-context`, `imported-or-context`) with declared required qualifiers. A data field whose type expression names a declared reference type binds a value of that reference: the codec capture whose name matches a declared qualifier becomes that qualifier, the remaining captures become the name segments in declaration order, and the `namespace` slot in effect at the field's position is captured only when the policy admits context. Identity construction, reference binding, lookup policy, and linking are separate stages, and the compiled symbol catalog holds them all: identity domains, declaration descriptors, and reference policies. A codec's `namespace` metadata is consumed here — a decoded identity belongs to that reference namespace. Two declarations composing the same qualified identity in one domain are a duplicate declaration (`KDLITH-REF002`), and a reference its qualification and lookup policy resolves to no candidate is unresolved (`KDLITH-REF001`), to several is ambiguous (`KDLITH-REF003`), and to one failing a required qualifier is `KDLITH-REF004`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | R3 §5, R3 §11         |
| Contribution resolution          | Contributions stay preserved records — target, value, layer, application scope, source origin — and resolution across layers is performed by a configuration resolver the processor registers, never by lexical context replacement and never by the engine itself. The schema declares the policy the resolver applies (layer precedence, per-target selection, union versus replacement, duplicate treatment, missing-value requirements, computed defaults, secret-reference handling) as descriptors; a schema naming a resolver that is not installed fails closed (`KDLITH-DLC006`). kdlith ships one resolver, registered as `layered`, installed by the data verbs and admitted alike by every verb that loads a schema, so a schema naming it passes `vet schema`, `list`, and `gen schema` while any other resolver name still fails closed. A generated crate installs it statically, with no plugin loader: its consumer admits the name on the `Extensions` it loads under and registers `Layered` through the generated `reader_with_contribution_resolver`, whose typed reading yields the same resolution items, while the plain `reader` leaves contributions as records (it is distinct from a context slot's `duplicates=layered` policy, which governs slot effects during binding rather than preserved records after it). A schema establishes a layer by binding a contribution node's field to `layer` (for example `field layer property=layer`, written `layer=user` in the document); a contribution without one ranks below every declared layer, and a contribution in a layer `layers` does not list is refused rather than ranked. A contribution's application scope binds the same way, from a contribution field named `scope`; a schema types it with a declared reference type over its own scope declarations, so the link stage resolves it (see [Contribution scope](#contribution-scope)). Without `layers` the policy declares no precedence: every record shares one rank and the duplicates treatment decides repeats. Per target and scope, in sorted (target, scope) order with the scope-less partition first: `layers` orders precedence highest first; records of one target in one rank are duplicates, which `reject` refuses and `last-wins` settles by document order; `replace` selects the highest-ranked value, and `union` merges value maps from lowest rank to highest so a higher layer's keys overwrite a lower one's, resolving non-map values by replacement; `missing reject` refuses a contribution whose reference-valued field names a target no record resolves (a reference into a domain no record contributes to is a link, not a target), and `allow` leaves that target unresolved. Each refusal fails the reading as `KDLITH-DLC010`. Once the documents are read, each settled target follows them as one `resolution` item per scope carrying the target, the resolved value, the layers that reached it, highest first, and its scope (null for scope-less records); `read data` finds it by the target as written, and `export` selects bound items only. Resolved output is derived from bound records, so it adds nothing to the cache identity. Lexical context, contribution resolution, declaration expansion, and provider execution remain four distinct mechanisms with distinct ordering, caching, and provenance requirements.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | R3 §9                 |
| Computed defaults                | A default may be computed: bound through a union of literal and computed alternatives discriminated by property (`case property=computed equals=#true type=ComputedDefault; otherwise type=LiteralDefault`), a computed default is a descriptor carrying its dependencies as typed references. The processor never computes it — computation belongs to the owning domain process. A required field whose only fallback is computed is therefore missing-required (`KDLITH-BND004`) when no value binds and no registered value resolver supplies one; an optional one stays absent (see [Value-resolution hook](#value-resolution-hook)).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | R3 §8                 |
| Secret providers                 | A secret provider is descriptor data: an identifier, an optional TTL, and a command as a list of strings that preserves argument boundaries. It is never executed, never reinterpreted as a shell command string, and the CLI never executes provider commands to validate or export declarations. Provider references and resolved secret values stay separate fields (`sensitivity=reference` versus `sensitivity=secret`), and every serialization and diagnostic surface applies the explicit output policy: a secret is emitted only when the policy explicitly says emit.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | R3 §8                 |
| This dialect's own document rule | A dialect definition has exactly one `dialect` root; a schema definition has exactly one `schema` root                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | F §1                  |
| Document rules are records       | `document` has role `record`: a document rule is a validated statement about the documents a definition reads, with no identity of its own, so it yields a record item rather than a declaration. It occurs at most once per root; its rules are listed in the rows above.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | R3 §2                 |

### Conventions the canonical files follow

- **Envelope keys.** Inside an `entries` envelope, argument 0 of each
  entry node is the map key, so entry types (`EnumMember`, `FieldDef`,
  `Variant`) carry no name field and their own bindings start at
  argument 1.
- **Self-description.** Every construct the canonical files use is itself
  declared by them.

## Deferral disposition

No mechanism is deferred at this version. The nineteen deferrals this
record previously carried — `generic-types`, `codecs`, `union-dispatch`,
`sensitivity`, `expansion`, `dependent-binding`, `case-dispatch`,
`source-context`, `accumulating-context`, `context-snapshots`,
`profiles`, `descendant-rules`, `ordered-sequences`,
`discovery-grammars`, `import-aliases`, `symbol-catalogs`,
`contribution-resolution`, `computed-defaults`, `secret-providers` — are
all defined in the in-scope tables above, so the `deferred` node and the
diagnostic code that rejected its spellings (`KDLITH-DLC003`) are retired
from the surface: `documents.kdl` no longer declares the marker, and a
spelling no binding accepts is rejected as unbound (`KDLITH-BND001`) on
every reading path, which is the same loud failure the marker existed to
guarantee. This is a retirement, not a reservation: the marker earns its
place only while something is deferred, and a future version that defers
a mechanism again re-introduces it as an explicit, versioned change to
this record and the canonical files together.

The restricted-semantics boundaries that motivated several deferrals are
now defined boundaries of the mechanisms themselves: providers are never
executed (secret-provider commands and computed defaults are descriptor
data), discovery is restricted path expansion (host-supplied typed-path
anchors, never environment-variable interpolation), and sensitivity is
policy-gated (an explicit output policy on every serialization and
diagnostic surface).

Several in-scope mechanisms stop short of extensions this version does
not define. Profiles are selected by the invocation — named
explicitly, chosen from a listed profile set, inferred from declared
match rules over the source path, or taken from the schema's default —
and the engine never infers a profile
from document content, since that would be trying profiles until one
accepts; match rules compare path components only, with no glob or
pattern forms. Discovery defines one ordering policy
(`portable-path-bytes`, the default) and admits no interpolation.
Descendant rules stop at the subtree of their declaring node. Ordered
sequences carry no unbounded-repetition groups; repetition stays with
occurrence bounds. Symbol catalogs define the three named qualification
policies and `qualified-name` composition; other composition forms and
lookup policies are not defined. Contribution resolution happens only
through registered resolvers — the engine never resolves contributions
and never substitutes lexical context for them. Computed defaults and
secret providers are descriptors the processor never executes. Because a
computed default never materializes a value, it never satisfies a
required field on its own: a non-optional field whose only fallback is
computed is missing-required (`KDLITH-BND004`) when no value binds and
no registered value resolver supplies one, and an optional one stays
absent. That is the defined behavior, not a gap; a field the
owning domain process computes is either optional here or supplied
explicitly by the document; a provider
command's argument boundaries are preserved, and dependent binding's
`against context=` form — looking a key up in a provider-held schema
table — stays stopped short, because the providers this version defines
hold no tables: a schema-provider table is a processor-installed
extension the dialect only names. Generic types monomorphize at compile
time; parameter constraints, defaults, and variadic parameters are not
defined.

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

An import expression is evaluated under the discovery grammar the
active profile (or the schema root, when no profiles are declared)
binds; a schema binding no grammar evaluates literal paths only. An
expression whose syntax the active grammar does not admit — including
any environment-variable interpolation, which no grammar admits — fails
with `KDLITH-IMP004`, and an anchor the grammar does not declare, or
declares but the host does not supply, with `KDLITH-IMP005`. The mirror
arm is rejected too: an anchor the host supplies that no discovery
grammar of the definition declares is `KDLITH-IMP005`, never silently
ignored. An expression matching zero sources does not resolve
(`KDLITH-IMP001`) unless the grammar declares an explicit empty-match
allowance, and each match retains the original expression, resolved
anchor values, ordered matched sources, import-site provenance, and
source revisions.

A schema's own imports expand the same way when the schema is loaded:
the host passes its anchors to the definition reader (`--anchor` on
`vet schema`, `inspect schema`, `gen schema`, `list`, and every data
verb, where one set of anchors serves both the schema and its data
documents), and the reader evaluates each expression under the grammar
the schema root binds, as declared by the documents read so far. A
schema declaring profiles binds no grammar while it is loaded — no
invocation profile is named then — so its own imports are literal paths
only, and an expression among them fails closed (`KDLITH-IMP004` or
`KDLITH-IMP005`) whatever anchors the host supplies. A
dialect root binds no discovery grammar at all, so a dialect's imports
are always literal (an expression is `KDLITH-DLC005`) and the dialect
verbs (`vet dialect`, `inspect dialect`, `gen dialect`) take no
`--anchor`.

An import carrying `as=NAME` preserves both the source and its alias in
the import request. The aliased document resolves its own declarations
by their unqualified names, exactly as if no import named it; every
other document reaches those declarations only through the alias
namespace. An alias must be unique in its document (a repeat is
`KDLITH-BND006`), and a reference through an alias that linking cannot
resolve is `KDLITH-REF001`.

Why visibility narrows under an alias instead of widening: the design
of the language left open whether `import "..." as=NAME` may affect
symbol visibility differently from ordinary source inclusion, and never
decided the question. It is decided here in favor of the authoring
document. An alias describes how one document reaches another, not a
second home for the other document's declarations; the declarations
still live once, in the document that wrote them, and that document
keeps resolving them the way it always did. Admitting them unqualified
everywhere the import reaches would let a single import line reach
across every document boundary in the reading, so the alias namespaces
every view except the declaring document's own. An import without an
alias changes nothing about visibility: both documents share one
namespace, as before.

A request that resolves streams an `import` item recording what it
loaded: the shown paths of its resolved targets, in resolution order,
as the item's `targets`. Each imported document that follows names one
of those paths, so attribution of an alias — or of any import — joins
the document's path against the request that precedes it. The key is
absent when nothing resolved, so a failed request keeps its shape.

## Contract for the engine and the reference processor

### Engine and runtime

The engine (`pkg/rs/engine`) and runtime (`pkg/rs/runtime`) implement the
dialect for every processor, hand-written or generated:

1. **Compile once.** Compile dialect and schema definitions into catalogs
   and a processing plan; never reinterpret a definition's KDL per record.
2. **Merge contributions.** Combine documents that repeat one root name
   and version into one dialect or schema, and reject a declaration made
   twice.
3. **Resolve imports** as `documents.kdl` states: literal or
   grammar-admitted paths relative to the importing document, written
   order, projection only, mutations never escaping, cycles diagnosed
   from the invocation stack, and `repeated` defaulting to `reject`.
   Discovery is restricted path expansion under the declared grammar —
   anchors are host-supplied typed paths, ordering follows the declared
   policy, the inventory is a dependency, and no grammar admits
   environment-variable interpolation. Aliases preserve both source and
   alias and gate visibility through the alias namespace: the aliased
   document resolves its own declarations unqualified, and every other
   document reaches them under the alias. Processing
   `contracts/super-schemas/dialect.kdl` yields the same dialect as
   reading the four canonical files directly, and that equivalence has
   its own fixture scenario (`dialect/entrypoint-vet/dialect`).
4. **Link.** Check that every type, node, slot, and parent a definition
   references is declared, including the canonical files' own
   self-description. The compiled symbol catalog holds identity domains,
   declaration descriptors, and reference policies: a declaration's
   qualified identity is composed as its `identity` block declares, two
   declarations composing one identity in a domain are a duplicate
   (`KDLITH-REF002`), and a reference its qualification and lookup
   policy resolves to no candidate is unresolved (`KDLITH-REF001`), to
   several is ambiguous (`KDLITH-REF003`), and to one that fails a
   required qualifier is `KDLITH-REF004`. Data linking runs these
   checks over a reading's declarations and references (see
   [Linking data](#linking-data)).
5. **Enforce bindings.** Reject colliding sources, duplicate keys,
   overlapping bindings, unconsumed arguments, contradictory child
   ownership, and unpermitted children; check cardinality, exactly-one
   groups, and the first-node rule at the points stated above.
6. **Dispatch deterministically** by profile, parent, node name, and
   explicit discriminators; the discriminators are node names, variant
   value prefixes, and property cases, each consulted as a compiled
   table with a single probe and no backtracking. The active profile is
   part of the position a node is admitted at, never something the
   engine guesses from content.
7. **Apply context** with the execution timing in `context.kdl`, and
   reject children effects that are not computable from the node header.
8. **Fail closed.** Reject a definition that names a handler, validator,
   resolver, or source-metadata provider that is not installed. The
   `type-expression` validator at version `0` is required.
9. **Decode codecs at bind time.** Apply a field's codec while the value
   arrives, keep the original spelling in provenance, and reject a
   string that fails its match or a capture that fails its declared type
   as a decode failure. Type-directed binding codecs (`fixture-value`,
   `argument-tail`) follow a two-tier rule: a `codec NAME` no
   declaration defines fails closed, while a known codec the processor
   does not install still compiles and the binder emits its payloads as
   pending typed-binding items.
10. **Expand member groups** demand-driven: a compiled plan may carry an
    unresolved expansion reference, complete checking resolves every
    one, and a cycle in the expansion or generic-instantiation graph is
    `KDLITH-SCH009`.
11. **Bind dependent payloads** by looking up the referenced
    declaration's member type — immediately when the declaration is
    compiled, otherwise as a pending typed-binding item emitted right
    after the item holding its reference, with the unbound payload kept
    raw (its one value, or a list of the values when there are more or
    fewer than one) — and never promote a scalar to a one-element list.
    Linking validates a pending payload on demand: its reference links
    through the item that holds it, and once that reference resolves to
    one declaration the payload must fit that declaration's member type
    — a list member zero or more values of its element type, an optional
    member none or one, any other member exactly one — or the binding
    fails with `KDLITH-REF005`. An unresolved reference reports its link
    failure and leaves the payload unvalidated; nothing executes a
    payload.
12. **Enforce sensitivity.** Apply the caller's output policy (emit,
    omit, redact) to sensitive fields in every serialization and
    diagnostic surface; a `secret` field is emitted only when the policy
    explicitly says emit.
13. **Select profiles from invocation context.** A schema that declares
    profiles has its active selection resolved once per entry document,
    before any node binds, by one shared function in a total order: the
    explicitly named profile; else, when the invocation lists a profile
    set, the listed profiles whose rules the source path satisfies, else
    the listed default, else the first listed (a set never falls through
    to inference or the schema default); else the first declared match
    rule the document's source path satisfies; else the schema's default
    profile; else `KDLITH-SCH011`. Selection reads invocation context
    only — never document content — and dispatch consults the active
    selection as part of position and never tries profiles in turn.
14. **Apply source context, accumulation, and duplicate policies.**
    Source-context values apply under their declared mode before the
    document's first semantic node, with their origin recorded. An
    accumulating slot combines through its declared operation, with
    `append` concatenating lists in effect order and `merge` treating a
    twice-contributed key under its declared duplicate treatment
    (`reject` by default, `KDLITH-BND006`). Every other set reaching an
    occupied slot follows the slot's declared duplicate policy: `reject`
    by default (`KDLITH-CTX006`), `last-wins`, or `layered` under the
    slot's declared layer order.
15. **Preserve contributions and provider descriptors.** Yield
    contributions as preserved records — target, value, layer,
    application scope, source origin — and route them to the registered
    resolver only. Never execute a provider command, never compute a
    computed default, and never reinterpret a provider's argument list
    as a shell command string.
16. **Persist and restore context snapshots.** Store context as shared
    immutable snapshots or checkpoint-plus-delta chains; index each
    record with its identity, type, source digest and span, schema
    fingerprint, snapshot ID, invocation ID, and validation status; and
    restore a snapshot only when the schema fingerprint and invocation
    lineage match (`KDLITH-CTX004` otherwise). The command-line tool
    writes the index with `cache refresh` and restores with `read data
--state` ([Snapshot restore](#snapshot-restore)).

### Self-hosted compilation

Loading a definition is reading it: `vet` and `inspect` both hand their
entry documents to the reference processor's typed reader, and the
engine compiles what the reader gives it — one construct per
declaration, already validated by binding, with the span it was written
at. Each construct stays with the document that wrote it, so a compile
diagnostic names that document — imported or importing — while the
compiler still takes constructs in reading order, an imported
document's between its import and the importer's next construct. The
reader binds dialect documents against the canonical dialect (the
definition language every dialect is written in) and schema documents
against their governing dialect, so a construct the governing dialect
does not admit is rejected with the binding code any processor binding
it reports.

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

- **Dispatch.** Each node is matched by its parent's compiled scope,
  the active profile, and its name, never by trying declarations in
  turn. The scope's structural rules admit it as it arrives; minimum
  counts, exactly-one groups, ordered sequences, descendant rules, and
  the document's first-node rule are checked when the scope completes.
- **Fields.** Arguments, properties, and children land in the fields
  their bindings name and are decoded against the field's type as they
  arrive. An entries envelope consumes its key argument once; a repeated
  key under `duplicate-keys reject` and a second source under
  `collision reject` are rejected. A union value is kept under the name
  of the node that spelled its variant. A value whose type is a codec
  decodes through it as it arrives. An entries envelope's key decodes
  against its map's key type like any value: a key the type decodes to
  a string keys the entry by that string, and a key whose type is a
  declared reference type keys the entry by the reference it spells,
  kept beside its written form for linking to resolve. A key the type
  decodes to anything else is rejected (`KDLITH-BND010`, quoting the
  key, the map field, the key type, and the decoded value) and the node
  is not emitted; no key is ever dropped. A value whose type names a declared
  reference type binds as a structured reference: the reference type's
  codec parses the whole spelling, captures whose names match the
  reference type's declared qualifiers become those qualifiers, the
  remaining captures become the name segments in declaration order, the
  `namespace` context slot in effect at the bind position is captured
  when the type's qualification policy admits context, and the domain
  is the type's `lookup` (else its `target`). A node with a `cases`
  block binds under the type its first matching case selects, and that
  type's required fields are checked like the node's own: one the node
  holds no value for is missing (`KDLITH-BND004`, naming it as
  `Type.field`).
- **Object constraints.** At scope completion, after case selection,
  the binder enforces the constraints of the node's declared type and of
  its case-selected type (`KDLITH-BND011`): `requires F G` fails when F
  holds a value and G none, and `all-or-none` fails when some but not
  all of its fields hold one, once per absent field. A field holds a
  value once anything binds it — written, from context, a literal
  fallback, the value resolver, or a collection default — so `#null`
  and an empty list are
  supplied; only a field left with no value is absent. A node that
  breaks a constraint or misses a selected type's required field is not
  emitted.
- **Context.** A context-bound field takes the slot's value only when no
  explicit value is present (`fallback`), and an explicit value must
  agree with the slot (`fixed`). A field read only from context fails
  where no control provides the slot. Source-context values apply under
  their declared mode before the first semantic node; an accumulating
  slot combines only through its declared operation, and any other set
  reaching an occupied slot follows the slot's declared duplicate
  policy, rejecting it by default. Effects follow the
  execution timing above, and a node that fails applies no following
  effects.
- **Items.** A bound node yields the item its role names: a record, a
  declaration with its identity, a contribution preserved unresolved, an
  import request — which, once it resolves, records the shown paths of
  the targets it loaded, each imported document following the request
  that names it — a pending typed binding awaiting its declaration,
  emitted immediately after the item holding its reference, or a
  container's scope completion. Context controls
  yield their effects only. Fields holding references stay provisional
  until linking resolves them. Every diagnostic is an item in the same
  stream.

Literals a definition writes (fallbacks, documented defaults, and `set`
values) are decoded against their types when the definition compiles,
through the same decoder.

### Value-resolution hook

A computed default and a secret provider are descriptors the processor
never computes or executes. The values they describe — secret values
and non-secret context variables alike — reach a bound record only
through one static hook the consuming program implements: kdlith picks
no source, and nothing is loaded at run time.

- **One trait.** The engine's public surface defines a single
  `ValueResolver` trait for secrets and context variables. They are
  requested through the same construct (a computed fallback) at the
  same point, and they differ only in how the result is shown, which
  is the output policy's concern rather than the resolver's — so one
  resolver serves both, told apart by the call's declared sensitivity
  and provider descriptors. A program registers its implementation on
  the reader (`with_resolver`, or a generated crate's
  `reader_with_resolver`) at compile time — the counterpart of a
  contribution resolver, registered through `with_contribution_resolver`
  or `reader_with_contribution_resolver`; a reader without one
  keeps the descriptor-only behavior above, so an optional field stays
  absent and a required one is missing (`KDLITH-BND004`).
- **When it is asked.** When a bound node completes, each field that
  nothing supplied — no explicit value, no context value, no literal
  fallback — and whose declared fallback is computed is put to the
  resolver once. Resolution happens at node completion, so a
  children-effect computed from the node's header never sees a
  resolved value.
- **What the call carries.** A `ResolutionCall` holds the context
  information every hook receives — the source document as the reading
  shows it, the node's site (source and span), the declared node name,
  and the field it fills (its slot identity) with the field's declared
  type and sensitivity — plus the declared descriptors as data: the
  computed default's dependency list in declared order, the declared
  secret providers those dependencies name (identifier, TTL, and
  command list with its argument boundaries preserved), and the node's
  incoming context slots by name. Nothing in the engine executes a
  provider command, joins its arguments into a shell string, or
  computes a dependency.
- **Loud failure.** A resolver that declines returns a reason, and the
  node fails with `KDLITH-BND009` naming the field, the node, and that
  reason; the reason must not quote a secret. A resolved value that
  fails the field's type is a decode failure (`KDLITH-BND003`) whose
  message never names a concealed value. Neither is ever replaced by
  a silent fallback, and a field that failed resolution is not also
  reported missing.
- **Provenance.** A resolved value records the origin kind `resolved`:
  the node's location and the identifiers of the declared providers
  the call carried (omitted when none). A `secret` field passes the
  output policy like any other secret field — redacted unless the
  policy explicitly says emit — whatever its origin.
- **Caching.** The values a resolver supplies are part of a bound
  result's invocation identity, so a changed value misses the cache and
  re-binds rather than replaying a stale result. The identity's sixth
  component, `resolved`, is a domain-separated fingerprint of the
  length-prefixed stream of each non-secret resolution — the node
  site's byte span, the field name, and the value's canonical
  encoding — in resolution order; a result the resolver supplied
  nothing to carries the empty stream's fingerprint. A result for
  which any `secret` field was resolved is never cached, in the
  session or on disk: even a fingerprint of a low-entropy secret on
  disk is an offline guessing oracle, so no key, fingerprint, or clear
  text of a resolved secret is ever written. Cache entries carry the
  component and are format 3; an entry of another format is reported
  as such (`KDLITH-CAC002`), never misread.

### Resolving values from a file (`--resolve-from`)

The command-line tool is itself a consuming program of the hook. Every
data verb (`vet data`, `inspect data`, `read data`, `list items`,
`cache refresh`, and `export`) takes `--resolve-from FILE`, which
registers one resolver whose values come from the named file and from
nothing else: no environment variable, dotfile, or default location is
ever read, and without the option no resolver is registered.

| Aspect                | Behavior                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| File format           | A KDL document of `value KEY VALUE` nodes: `KEY` a string, `VALUE` one string, number, or boolean, with no properties, type annotations, or children. A key given twice, any other node, or any other shape is an invalid option value (`KDLITH-CLI001`, exit 2), reported before any definition loads; an unreadable, non-UTF-8, or oversized file is `KDLITH-SRC001`/`SRC002`/`SRC003`, and a syntax fault is its `KDLITH-SYN` code. Comments are KDL comments. |
| Lookup                | A computed default names exactly one dependency, and that dependency is the key: `fallback computed=#true { dependency "region" }` resolves from `value "region" ...`. A dependency the file does not hold, or a computed default naming any other number of dependencies, is declined: `KDLITH-BND009` at the node. Declared providers reach the resolver as data, and the tool never executes their commands.                                                   |
| Types                 | The value is decoded against the field's declared type like any other value; a mismatch is `KDLITH-BND003`.                                                                                                                                                                                                                                                                                                                                                       |
| Secrets and variables | One file serves both. A non-secret field's resolved value is output as it is; a `secret` field's resolved value passes `--sensitive` like any other secret: redacted unless the policy says `emit`. Provenance records the origin kind `resolved`.                                                                                                                                                                                                                |
| Caching               | `cache refresh` keys each entry on the non-secret resolved values (see Caching above); a document for which a secret was resolved gets no entry, and an entry document whose reading consulted the resolver at all writes no snapshot entry ([Snapshot restore](#snapshot-restore)).                                                                                                                                                                              |

Example: with the file

```kdl
value "region" "eu-north"
value "replicas" 3
value "vault" "token-from-a-vault"
```

`kdlith export --schema deploy.kdl --resolve-from values.kdl --emit roots
--format ndjson data.kdl` fills every absent computed-default field
named by those keys, and writes the `vault` value as `[redacted]` when
the field it fills is declared `sensitivity=secret`.

### Snapshot restore

`cache refresh` writes, beside the bound results it caches, one
snapshot entry per error-free entry document into the same state
directory: `<key>.kdlith-snapshot`, headed `kdlith-snapshot format=2
canonicalization=2`, whose metadata line carries the entry as spelled,
its revision, the schema fingerprint, the profile selection, and the
lineage, followed by one index line per record the entry's reading
bound, with the context it bound under. The key hashes the entry as
spelled, its revision, and its profile selection only. The lineage is
the invocation's entry documents as spelled, up to and including the
snapshot's own, because the entries before a document number its
invocations. A reading that consulted the value resolver writes no
snapshot, so no resolved secret reaches the store.

`read data --state DIR` resolves the record it is asked for from that
store instead of replaying the read document:

- **Hit.** The snapshot keyed by the read document (as spelled, at its
  current revision, under its profile selection) indexes the record.
  Its context is restored, and only the record's span binds under it,
  at the node index the scan recorded. Nothing else of the read
  document binds, and no context crosses between documents. The
  `--entry` document still binds as it does in replay, its
  diagnostics reported and its items not printed, because its imports
  number the read document's invocation and so the record's source id.
  The output equals replay's.
- **Fallback.** No `--state`, an absent directory, a changed read
  document (its revision keys no snapshot), or a store that indexes
  no such record replays the documents as `read data` always does. A
  record an import reached is not restored and replays.
- **Mismatch.** A snapshot captured under another schema, or under
  another entry chain (the read document read without the `--entry`
  it was scanned after, for example), is rejected before anything
  binds (`KDLITH-CTX004`, exit 1), never restored best-effort.

A read never writes the store. An owned entry the read cannot use is
`KDLITH-CAC002`.

### One code per rejected construct

A definition document is a data document of its dialect. A construct
the dialect's declarations reject therefore gets one diagnostic code on
every reading path: when a processor such as superproc binds it (the
item stream `inspect` shows) and when the engine compiles it (`vet`, and
`inspect` after its item stream).

| Rejected construct                                                                                  | Code            |
| --------------------------------------------------------------------------------------------------- | --------------- |
| A node no binding accepts in its position, including a missing or extra root                        | `KDLITH-BND001` |
| A child that occurs more or fewer times than its rule allows, including the first-node rule         | `KDLITH-BND002` |
| A value that does not decode as its declared type, such as a name with the wrong format             | `KDLITH-BND003` |
| A required field or argument that is absent, such as a definition root without `version=`           | `KDLITH-BND004` |
| An argument, property, or type annotation that no binding consumes                                  | `KDLITH-BND005` |
| A repeated key of a keyed collection, such as two fields of one object or two bindings of a field   | `KDLITH-BND006` |
| A value that names no member of its enum, such as an unknown `role=` or string `format`             | `KDLITH-BND008` |
| A map key its key type decodes to a value no map key holds, such as a number                        | `KDLITH-BND010` |
| A node breaking a `requires` or `all-or-none` constraint of a type it binds as                      | `KDLITH-BND011` |
| A type expression that breaks the type-expression grammar                                           | `KDLITH-DLC007` |
| Two fields bound to the same argument, property, or rest arguments                                  | `KDLITH-SCH003` |
| An import that projects a slot the dialect does not declare                                         | `KDLITH-SCH006` |
| An expansion or generic-instantiation cycle in a definition                                         | `KDLITH-SCH009` |
| Variant prefixes of one union where one is a prefix of another                                      | `KDLITH-SCH010` |
| A schema that declares profiles while its invocation selects none, or names or lists an unknown one | `KDLITH-SCH011` |
| A profile whose `extends` chain returns to a profile already on it                                  | `KDLITH-SCH012` |
| A second profile of one schema marked `default`                                                     | `KDLITH-SCH013` |

A bad type expression (`KDLITH-DLC007`) and an unknown enum value
(`KDLITH-BND008`) are distinct from a plain value-type mismatch
(`KDLITH-BND003`). A codec string that fails its match, a capture that
fails its declared type, and a value-bound union that matches no variant
prefix are decode failures (`KDLITH-BND003`); a node whose properties
match no case of its `cases` block and that has no `otherwise` is
unbound (`KDLITH-BND001`). Every definition root — dialect or schema —
states its `version` property; a root without one is a missing required field,
rejected with `KDLITH-BND004` on both reading paths.

An unbound node (`KDLITH-BND001`) carries a note when it looks like a
child written without braces: its immediately preceding sibling in the
same scope was bound, that sibling's binding admits the unbound node's
name as a child, and the unbound node starts at a strictly greater
column. KDL indentation is not structure, so such a line is a sibling,
and the note, placed at the sibling's line, asks whether it was meant
to nest there. Both reading paths attach it, and it renders as an
indented `note:` line ending in the sibling's `(path:line:column)`.

The expanded surface adds `KDLITH-SCH011` to this table and retires
`KDLITH-DLC003` together with the deferral marker
([Deferral disposition](#deferral-disposition)). Its other new codes
live with their mechanisms: the discovery failures `KDLITH-IMP004` and
`KDLITH-IMP005` extend the import codes in
[Import resolution](#import-resolution), and the context failures
`KDLITH-CTX004` and `KDLITH-CTX005` are stated in the context rows
above (the expansion's `KDLITH-CTX003` is retired, as below);
`KDLITH-CTX004` is reached through `read data --state`
([Snapshot restore](#snapshot-restore)).

The profile model adds `KDLITH-SCH012` and `KDLITH-SCH013`, each
minted once per offending declaration when the schema compiles: an
`extends` chain that returns to a profile already on it is an
inheritance cycle (`KDLITH-SCH012`), and a second profile marked
`default` is rejected (`KDLITH-SCH013`), the first in contribution
order keeping the mark. An `extends` naming an undeclared profile is an
undeclared reference (`KDLITH-SCH006`), the node `profile=` precedent.
`KDLITH-SCH011` keeps one construct across the extended selection: no
explicit name, no matching rule, and no default selects nothing, and an
explicit name the schema does not declare selects nothing either.

The slot duplicate policies add `KDLITH-CTX006`, minted only by the
`reject` arm of a slot's declared duplicate policy: a set reaching an
occupied slot that does not accumulate, under `reject` (the default),
or under `layered` within one layer or against a held value that carries
no layer. It subsumes `KDLITH-CTX003`, which is retired: a second value
on a list- or map-typed slot without an accumulation operation is a
duplicate under that slot's policy like any other, so one construct
keeps one code. A twice-contributed `merge` key under its `reject`
default keeps its duplicate-key code, `KDLITH-BND006`. A `layer=` on a
set whose slot is not layered, a set on a layered slot without one,
`layers` on a slot that is not layered, a non-`reject` duplicate
policy on an accumulating slot, and a non-`reject` duplicate treatment
on `append` are malformed constructs
(`KDLITH-DLC005`); a layer name the slot does not list is an unresolved
reference (`KDLITH-SCH006`, or `KDLITH-DLC008` in a dialect
definition).

The value-resolution hook adds `KDLITH-BND009`, a binding code minted
only when the registered value resolver declines to resolve an absent
computed-default field ([Value-resolution hook](#value-resolution-hook));
it is never minted without a registered resolver, which leaves the
field absent instead.

Reference-keyed maps add `KDLITH-BND010`, minted when an entries
envelope's key decodes under its map's key type to a value that is
neither a string nor a reference. A key whose type is a declared
reference type binds the entry with its reference, and a key that
reference cannot resolve is a link failure (`KDLITH-REF001`) whose
message names the map field: `reference NAME in DOMAIN as a key of
FIELD does not resolve`.

Object constraints add `KDLITH-BND011`, minted at the node once per
absent field of each broken `requires` or `all-or-none` constraint,
quoting the constraint, its type, the field that triggered it, and the
absent field. A required field of a case-selected type keeps the
missing-field code, `KDLITH-BND004`, as the node's own required fields
do: one condition, one code. On the definition path this reaches the
dialect's own dispatch: a computed `fallback` selects `ComputedDefault`,
whose `dependencies` is required, so a computed fallback without a
`dependency` child is `KDLITH-BND004`.

The shipped contribution resolver adds `KDLITH-DLC010`, minted when a
registered resolver refuses the records it was handed (a duplicate
under `reject`, a missing target under `reject`, or a record in a layer
the policy does not declare); `KDLITH-DLC006` keeps only the
not-installed and no-policy cases.

The remaining definition codes (`KDLITH-SCH001`, `KDLITH-SCH002`,
`KDLITH-SCH004`, `KDLITH-SCH005`, `KDLITH-SCH007`, `KDLITH-SCH008`,
`KDLITH-DLC005`, and `KDLITH-DLC006`) come from linking, plan building,
and extension checks that only the compiler runs. `inspect dialect` and
`inspect schema` run that compiler too: they first stream every item the
reader yields to standard output, then hand the same documents to the
compiler exactly as `vet dialect` and `vet schema` do, under complete
checking, and render the reader's diagnostics followed by the
compiler's on standard error. Any error exits nonzero after the items
have been shown; warnings alone exit 0. Show, then fail: the items
explain what the documents declare as written, and the diagnostics say
why the definition does not compile. A resolution policy naming a
resolver nothing installs is therefore `KDLITH-DLC006` under `inspect
schema` as under `vet schema`, `list`, and `gen schema`, and a single
contributing document of a multi-document dialect, inspected alone,
streams its items and then fails linking (`KDLITH-DLC008`, or
`KDLITH-SCH005` for an unmet document rule) because the types it refers
to are declared in its siblings. When the reader itself reports an
error, the compile stage does not run, since the compiler would re-read
the same documents and stop on the same errors; such a run reports only
the reader's diagnostics. `inspect data` binds exactly as `vet data`
does, compiling the schema first, and links as it does (see
[Linking data](#linking-data)). A node-level check such as an
overlapping binding sits at the same span on both reading paths: the
reader-path binder check reports it at the node declaration, and it
fires first, so the compiler's field-span report of the same fault is
not reached.

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

### Generated crates

`gen` renders every name a definition declares as a Rust identifier:
types and enum members or union variants in Pascal case, field keys in
snake case (a Rust keyword as a raw identifier, `type` as `ty`), and each
contributing document as a module named after its file stem. A field's
type imports every engine item it spells at any depth — an `ordered-map`
inside `optional<…>` or `list<…>`, in a declared type or a typeless
node's local type alike. The dialect admits names Rust cannot spell, so
generation checks them before writing anything and fails closed, naming
the name and the reason, when a member, variant, or field key is no Rust
identifier, two of one declaration's spell the same identifier, a
document's module name is no identifier, or a declared, reference, or
node-local type is named `Self`. A schema crate spells every prelude
and engine name it uses by path (`::core::option::Option`,
`::std::vec::Vec`, `::kdlith_engine::Ref`), so a schema may declare a
type named `Option`, `Vec`, or `Ref` beside fields that use them. A
dialect crate keeps the reference processor's unqualified spellings,
so on the dialect form a declared type taking one of those names
(`Option`, `Item`, `Ref`, and the engine items its modules import)
fails closed instead. A dialect declares no reference types, and a
schema's reference type names pass the dialect's type-name grammar at
load, so on the schema form the only reference-type name generation
refuses is `Self`. A generic
declaration (one naming `parameter`s) is never spelled itself; each
instance of it (`Pair<String, Mode>`) generates as its own type under
a mangled Rust name, the instantiation's words joined, each first
letter capitalized and the punctuation dropped (`PairStringMode`),
while its shape and logical expression still spell the instantiation
(`Pair<String, Mode>`, and `optional<Pair<String, Mode>>` under
`optional`) and its fields carry the substituted body. Two instances
of one declaration with different arguments are two types. Because
the mangle drops punctuation, two instantiations can spell one name
(`Pair<A, BC>` and `Pair<AB, C>` both spell `PairABC`), and a mangled
name can take a declared or node-local type's name; generation then
fails closed the same way — under
`--dry-run` and `--check` too — naming the instance and its mangled
name, never inventing a suffix. Such a definition still vets: the
limit is the generated crate's, not the dialect's.

Each reading node's items stream as a variant of the crate's `Item`
enum, named after the node's type. When any type the crate declares —
declared, reference, node-local, generic-instance, or inline-union —
is named `Item`, the enum is named `ItemKind` instead, at every site
the crate spells it; when `ItemKind` is taken as well, generation fails
closed rather than inventing a third name. A reading node reads as
exactly one object type, so a node whose cases and `otherwise` do not
select exactly one object type fails generation closed naming the node
and the types it dispatches to; the schema still vets. When
differently named nodes of a
schema bind one type, each of those nodes reads as its own variant,
named after the node instead (`widget`, `gadget`, and `part` binding
`Widget` read as `Item::Widget`, `Item::Gadget`, and `Item::Part`,
each holding a `Widget`), so a consumer tells them apart by variant
while the type-level projection still matches all three. Nodes of one
name under different parents share one variant. Generation fails
closed, before anything is written, when two nodes would spell one
variant for different items, and the same way as other unspellable
names when a node keys a variant that is no Rust identifier (a node
named `self`). Every refusal of this section prints one line,
`kdlith: gen <form>: <reason>`, and exits 1.

A declared reference type generates as a newtype of its own name over
the engine's symbolic reference to its target (`ConfigVarRef` holding a
reference to `ConfigVarDef`), carrying the lookup domain and its
qualifier names, in declaration order, as associated constants. A field
typed by it reads the reference its codec decoded into that lookup
domain, with the reference's name segments, qualifiers in the order the
spelling carried them, written spelling, and site; reading never
resolves it. A pattern codec generates as a newtype of its own name
over its output type (`Stamp` holding a `u64`), and a field typed by
the codec holds the decoded output value, never the spelling. Both
kinds join the generated crate's type shapes, so the conformance check
compares them against the compiled schema like any other declared type.
A reference type's name passes the same spelling checks as a type's.

A generated crate's `reader` constructors return the engine's typed
reader, which types the items of the same data reading `vet data`
performs: imports expand under the grammar of the profile each entry
document selects, and a document's diagnostics, handler effects, and
output policy are the data path's own. Host inputs are builders on the
returned reader: `with_anchors` supplies discovery anchors,
`with_profile` names the profile every entry document binds under,
`with_options` takes a whole `ReadOptions` (profile or profile set,
anchors, output policy) in place of every option set before, and
`with_handlers` installs the context handlers handler effects run
through. A document no rule or default selects a profile for fails
with `KDLITH-SCH011` until a profile is named, and an anchor no grammar
declares fails with `KDLITH-IMP005` before any document is read.
`gen schema --handler NAME` admits a built-in handler a schema names,
as the data verbs do; generation never runs it, so the consuming
program installs the implementation on the reader with
`with_handlers` and admits its name at load with
`Extensions::with_installed_handlers`. Without the flag such a schema
fails generation with `KDLITH-DLC006`.

A tool needs only the generated schema crate: it re-exports the reading,
linking, and diagnostic types a reader returns (`CheckScope`,
`Extensions`, `ReadOptions`, `TypedItem`, `ItemVariant`, `Ref`,
`Reading`, `LinkReport`, `LinkOutcome`, `UnmetQualifier`,
`ResolveError`, `Origin`, `Location`, and `DiagnosticRecord`). The reader's
`link(CheckScope)` builder links each reading the way `vet data` does
(see Linking data), and `collect` keeps a whole reading: its items in
stream order, pending typed bindings among them, the link report, and
the reading's diagnostics, the binding diagnostics followed by the
linking ones. `reading.resolve(&reference)` returns the declaration a
reference resolved to, read as its target type, and
`reading.settled::<T, _>(&reference)` reads it as `T`. A reference that
did not resolve, resolved ambiguously, or failed a required qualifier
is an error carrying the link outcome and the alias namespaces that hid
a match, never a default. Under complete scope any link diagnostic fails
the reading; under local scope the same diagnostics are reported and the
reading does not fail. A generated reference type holds its reference
as `.0`, so `reading.resolve(&record.target.0)` resolves a record's
field.

The generated crate's `Cargo.toml` is standalone — no workspace
inheritance — and by default names its kdlith dependencies as sibling
paths (`kdlith-engine = { path = "../engine" }`, and so on), the layout
of a generated crate placed beside the kdlith crates in this
repository. `gen schema --kdlith-path PATH` roots those path
dependencies at `PATH` instead (`{ path = "PATH/engine" }`), and
`gen schema --kdlith-version VER` writes them as registry dependencies
(`kdlith-engine = "VER"`). Either value is written verbatim — no
canonicalization, no trailing-slash trimming, no existence check, no
version grammar; Cargo judges the path or requirement when the crate
builds — and only the kdlith dependency lines change: `[package]` keeps
the generator's version. The two flags are mutually exclusive (both
together is `KDLITH-CLI002`), neither repeats, and an empty value
fails closed (`KDLITH-CLI001`), as does a value that is not UTF-8 and
so cannot be written into the manifest; each exits 2 with nothing
written. Without either flag the manifest keeps the sibling paths.
`gen dialect` takes neither flag: the dialect form generates the
reference processor, which lives in the sibling layout.

Every typed record and declaration carries where it was written: its
`site`, the location the binder recorded (absent only where the binder
recorded none), and its `origin`, the document it was read from — the
document's path as the reading shows it, the same spelling `vet data`
prints on its document item (the entry as given, an imported document
as its import spelled it) — with the alias of the import that loaded
that document, or none for an entry document or an unaliased import. A
document's items keep its origin after a nested import completes, and
a document replayed from the cache carries its own path and sites. The
reader's `capturing(&[slot, …])` builder names context slots every
document item then carries, in that order, each with its value as the
document's first node binds under it — the context its importer
projected, with the document's source-context values applied — or
none where the slot is unset. Capture is opt-in; a name the schema
declares no slot for fails the reading before any document is read.
`inspect data --view context` reports the same slot values at a
different point, each entry's context as the entry left it: a slot an
entry sets after its first node reads unset in the entry's capture and
set in the view. None of these fields is part of
an item's serialized form, so `vet data` output and exports are
unchanged.

## Contribution scope

A contribution carries an application scope beside its layer. As with
`layer`, a schema establishes it by binding a contribution node's field
named `scope` (`field scope property=scope`, written `scope=dev` in the
document); a contribution without one is scope-less. The canonical
dialect names scope terms with the `ScopeName` vocabulary type
(kebab-case, beside `AnchorName` and `ProviderName`), and a schema
declares its own scope vocabulary with existing constructs: a
declaration node over a scope type, a reference type targeting it, and
the contribution's `scope` field typed with that reference type.

```kdl
types {
    object ScopeDef {
        field name String
    }
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
node scope role=declaration type=ScopeDef {
    field name arg=0
}
reference ScopeTerm target=ScopeDef codec=ScopeSpelling qualification=explicit
node setting role=contribution type=Setting {
    field target arg=0
    field value arg=1
    field scope property=scope
    field layer property=layer
}
```

Data documents then declare the terms (`scope dev`) and spell them on
contributions. The bound contribution keeps the scope reference in its
value map and carries its written spelling as the record's `scope`;
the contribution's target is always taken from its other reference
fields, never from the scope, whichever field is declared first. A
scope field typed `optional<String>` still binds its text unlinked.

A typed scope links like any field typed with a declared reference
type ([Linking data](#linking-data)): a spelling no scope declaration
matches is `KDLITH-REF001`, two scope declarations composing one
identity are `KDLITH-REF002`, and `KDLITH-REF003` and `KDLITH-REF004`
apply under the reference type's qualification policy. Under the
default complete scope these fail the reading; the resolver never
refuses a record for its scope.

The shipped `layered` resolver partitions resolution by scope, within
layer precedence: records first group by target and scope (the written
spelling, with scope-less records forming their own partition), and
inside each partition layer precedence, duplicate treatment, and
selection apply exactly as for an unscoped target. Layer never crosses
a partition, and scope never overrides layer inside one. A target
contributed under two scopes therefore resolves twice, one `resolution`
item per scope, and two records for one target in one layer under
different scopes are not duplicates. `missing reject` checks a
reference-valued field against the targets contributed under any
scope.

## Linking data

`vet data` and `inspect data` take `--scope local|complete`, defaulting
to `complete`. Local scope binds the documents and stops, except under
the `references` view of `inspect data`, whose report is the link
stage's output. Complete scope then links the reading: every reference
held by a field typed with a declared reference type, including each key
of a map whose key type is one, is looked up among
the reading's declarations under that reference type's qualification
policy. Plain `ref<T>` values
are not linked here; they keep their own mechanisms.

| Code            | Linking failure                                                                                                    |
| --------------- | ------------------------------------------------------------------------------------------------------------------ |
| `KDLITH-REF001` | No declaration matches; when only a declaration under an import alias would, the message names the alias namespace |
| `KDLITH-REF002` | Two declarations compose one identity in one domain; reported at the second, labelled with the first               |
| `KDLITH-REF003` | The policy derives more than one candidate                                                                         |
| `KDLITH-REF004` | The one candidate fails a required qualifier                                                                       |
| `KDLITH-REF005` | A pending payload does not fit the member type of the declaration its reference resolved to                        |

Linking runs after the whole reading has streamed, so every item, and on
`inspect data` every view, prints first; the link diagnostics go to
standard error and decide the exit with the binding diagnostics. The
scope never changes standard output, only the exit: under local scope
the `references` view still links and prints the same report, and its
link diagnostics still reach standard error, but they do not decide
the exit.

## Inspect views

`inspect dialect`, `inspect schema`, and `inspect data` take `--view`,
which picks what standard output carries. Every view writes JSON lines
in the tagged envelope the item stream already uses,
`{"item":"<kind>","data":{…}}`, so one decoder reads all of them.
Diagnostics always go to standard error. The default view, `source`,
is the item stream described under
[One code per rejected construct](#one-code-per-rejected-construct).
The view names follow the design drafts' inspection command (F §8).

### Which views apply to which target

| View         | `inspect dialect` | `inspect schema` | `inspect data`               |
| ------------ | ----------------- | ---------------- | ---------------------------- |
| `source`     | item stream       | item stream      | item stream                  |
| `bound`      | `KDLITH-CLI004`   | `KDLITH-CLI004`  | data items                   |
| `context`    | `KDLITH-CLI004`   | `KDLITH-CLI004`  | entry contexts               |
| `effective`  | compiled entries  | compiled entries | declarations and resolutions |
| `origins`    | `KDLITH-CLI004`   | `KDLITH-CLI004`  | provenance envelopes         |
| `plan`       | processing plan   | processing plan  | the bound definition's plan  |
| `references` | `KDLITH-CLI004`   | `KDLITH-CLI004`  | linked references            |

`bound`, `context`, `origins`, and `references` describe how data binds
and links, and a definition target binds no data. Asking for one of
them on `inspect dialect` or `inspect schema` is an argument fault,
reported before any document is read and exiting 2:

```text
error[KDLITH-CLI004]: the bound view does not apply to a dialect target
```

A `--view` value that names no view is
a usage error from the argument parser (`KDLITH-CLI001`, exit 2), which
lists the seven names.

### Line formats

Each sample below is one line copied from the committed fixture
goldens under `tests/fixtures/processing/inspect-view-*`.

**`bound`** (`inspect data`) writes the stream's `record`,
`declaration`, `contribution`, and `pending-binding` items unchanged; a
pending typed binding follows the contribution or record holding its
reference. Document, import, scope, resolution, and diagnostic items
are dropped.

```text
{"item":"declaration","data":{"node":{"source":1,"index":1},"identity":{"domain":"Group","name":["imported"]},"type_name":"Group","fields":[{"name":"name","value":{"kind":"string","value":"imported"},"origin":1},{"name":"labels","value":{"kind":"map","value":[{"key":"i","value":{"kind":"map","value":[{"key":"text","value":{"kind":"string","value":"Imported"}}]}}]},"origin":1}],"origin":1,"validation":"valid"}}
```

**`origins`** (`inspect data`) writes one `origins` line for each record,
declaration, and contribution, and none for a pending typed binding,
which carries no provenance of its own. Its `data` is exactly the provenance
envelope `export` writes beside that item: `node` (the item's
location), `sources` (the source revisions its locations name),
`origins` (the shared origin table, whose entries are `literal`,
`derived` with an `operation` and its `inputs`, or `resolved`), and
`fields` (each field's index into that table, in field order).

```text
{"item":"origins","data":{"node":{"source":0,"span":{"start":{"offset":265,"line":15,"column":1},"end":{"offset":272,"line":15,"column":8}}},"sources":[{"source":0,"path":"input.kdl","revision":"fnv1a64:c8ffe414bb5839bc"}],"origins":[{"kind":"literal","location":{"source":0,"span":{"start":{"offset":273,"line":15,"column":9},"end":{"offset":279,"line":15,"column":15}}}},{"kind":"literal","location":{"source":0,"span":{"start":{"offset":279,"line":15,"column":15},"end":{"offset":283,"line":15,"column":19}}}}],"fields":[{"name":"target","origin":0},{"name":"value","origin":1}]}}
```

**`context`** (`inspect data`) writes one `context` line per entry
document after the read. `entry` is the path as given, and `slots`
lists every declared slot with the `value` and `layer` the entry's
root level left. A value uses the same tagged encoding as item fields;
an unset slot has `"value":null`, and a slot that is not layered has
`"layer":null`.

```text
{"item":"context","data":{"entry":"first.kdl","slots":[{"name":"region","value":{"kind":"string","value":"north"},"layer":null},{"name":"locale","value":{"kind":"string","value":"sv-se"},"layer":"user"},{"name":"zone","value":null,"layer":null}]}}
```

Only entry documents report. An imported document runs under its
import's projection, and its effects stay in its own invocation, so it
writes no line of its own; the provenance of the items it emits covers
it. An entry replayed from the bound-result cache still reports the
context its cached reading left. The snapshot is the entry's state at
the end of its document, while the root level is still live; it is not
the context restored after the document closes.

**`effective`** on `inspect dialect` and `inspect schema` writes one
`effective` line per entry of the compiled definition. `section` is
`type`, `node`, `slot`, `import`, or `profile`, and `entry` carries
that section's keys:

| Section   | Entry keys                                                                                         |
| --------- | -------------------------------------------------------------------------------------------------- |
| `type`    | `name`, `parameters`, `kind` (as `list types` reports them)                                        |
| `node`    | `name`, `parents`, `role`, `type`, `profile` (as `list nodes` reports them)                        |
| `slot`    | `name`, `type` (the type expression as written), `namespace`, `accumulate`, `duplicates`, `layers` |
| `import`  | `path`, `project`, `repeated` (kebab-case, as on the import item), `alias`                         |
| `profile` | `name`, `roots`, `grammar` (as `list profiles` reports them)                                       |

A `type` entry carries one more key, `expanded`, only when the type's
includes spliced members: one `{target, declared, included}` object per
include, in written order, where `declared` is the target's declaration
site and `included` the include's site, each `{source, span}`.

```text
{"item":"effective","data":{"section":"slot","entry":{"name":"locale","type":"String","namespace":null,"accumulate":null,"duplicates":"layered","layers":["user","base"]}}}
```

On `inspect data`, `effective` writes the stream's `declaration` and
`resolution` items unchanged: the declarations as imported, then the
values the shipped `layered` contribution resolver settled.

```text
{"item":"resolution","data":{"target":{"domain":"Option","name":["size"],"qualifiers":[],"written":"size","site":{"source":1,"span":{"start":{"offset":20,"line":2,"column":9},"end":{"offset":25,"line":2,"column":14}}}},"value":{"kind":"map","value":[{"key":"target","value":{"kind":"reference","value":{"domain":"Option","name":["size"],"qualifiers":[],"written":"size","site":{"source":1,"span":{"start":{"offset":20,"line":2,"column":9},"end":{"offset":25,"line":2,"column":14}}}}}},{"key":"value","value":{"kind":"string","value":"large"}},{"key":"layer","value":{"kind":"string","value":"base"}}]},"layers":["base"],"scope":null}}
```

The two sides answer different questions. On a definition target the
view is context-free: compiling merged the imported documents, so
their types and nodes appear in their sections and each import is
listed as written. On data, what is effective depends on both imports
and context (R1 §5): an imported document's contribution can win a
layer over the entry's own, and the resolution lines show the outcome.

**`plan`** writes one `plan-context` line, then one `plan-node` line per
node declaration. On `inspect data` it renders the plan of the
definition the data bound under: the `--schema` schema, or the
governing dialect without one. No data item is written, but the data
is still read, and the read decides the exit status.

```text
{"item":"plan-context","data":{"slots":[{"name":"tags","type":"list<String>","namespace":null,"accumulate":"append"},{"name":"locale","type":"String","namespace":"look","accumulate":null},{"name":"region","type":"String","namespace":"place","accumulate":null}],"namespaces":["look","place"]}}
{"item":"plan-node","data":{"name":"fill","role":"contribution","scope":7,"children-effects":[],"following-effects":[],"context-reads":[],"deferred":[{"deferred":"contribution"},{"deferred":"reference","field":"target","target":"Shelf"}],"dependent":{"target":"target","member":"sizes","codec":"argument-tail"},"declaration":false,"requires":["record-buffering","imported-schema-metadata","deferred-binding","global-validation"]}}
```

A `plan-node` carries `name`, `role` (or null), `scope` (the scope
index, or null), `children-effects` and `following-effects`,
`context-reads` (`field`, `slot`, and `policy`: `fallback`, `fixed`, or
null), `deferred` (`{"deferred":"reference","field","target"}` or
`{"deferred":"contribution"}`), `dependent` (`target`, `member`, and
`codec`, or null), `declaration`, and `requires`. An effect is
`{"effect":"set","slot",…,"layer"}` (with `field` when the value comes
from a field, or `value` when it is written in the schema),
`{"effect":"remove","slot"}`, `{"effect":"clear","namespace"}`, or
`{"effect":"handler","name"}`. Slots and namespaces are named, never
numbered.

`requires` lists what processing the node needs, derived from the
plan, always in this order:

| Flag                         | Present when                                                                                                                                           |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `record-buffering`           | the node defers a contribution, or one of its effects sets a slot that accumulates                                                                     |
| `scope-completion`           | the node's scope runs checks when it closes: a minimum count, an exactly-one group, a required first child, an ordered sequence, or a descendant bound |
| `declaration-lookup`         | the node registers a declaration                                                                                                                       |
| `imported-schema-metadata`   | the definition imports documents (marks every node)                                                                                                    |
| `deferred-binding`           | the node defers a reference                                                                                                                            |
| `global-validation`          | the definition declares a document rule (marks every node)                                                                                             |
| `registered-domain-handlers` | one of the node's effects is a registered handler                                                                                                      |

A node with no flag is processed within its own header and record. The
accumulation need belongs to the node that sets the slot, not to the
nodes that read it. A scope that only caps counts or permits children
runs no check when it closes, so it does not need scope completion.

This vocabulary follows the design drafts' list for the `plan` view
(F §8). The drafts' earlier table of plan requirements (R2 §12) names
other categories: its scope-completion and global-validation
requirements are the same two flags here; its header-local and
record-local requirements are what a node with no flag needs; its
indexed lookup is split into `declaration-lookup` (registering an
identity) and `deferred-binding` (referring to one); and its staged
schema compilation has no flag. `record-buffering`,
`imported-schema-metadata`, and `registered-domain-handlers` appear only
in the `plan` view's list.

**`references`** (`inspect data`) writes the reading's link report
once every item has bound: one `references` line per declared reference
type, in declaration order, with `name`, `domain`, `qualification`
(`explicit`, `explicit-or-context`, or `imported-or-context`), and
`links`. A type no reference names still writes its line, with
`"links":[]`. Each link, in stream order, carries `written`, `domain`,
`name` (the segments, outermost first), `qualifiers` (`name` and
`value`, as written), `context` (the namespace context it bound under,
or null), `field` (the record or declaration field holding it, or null
for a contribution's target), `key` (present and `true` only when the
reference is a key of the map that field holds), `site` (`source`, the
document's path as the reading shows it, and `span`), `candidates`
(each with its `identity`, the `alias` namespace it registers under or
null, the identity field behind each segment as `labels`, and its
`site`),
`gated` (the alias namespaces that hid a candidate), and `outcome`:
`resolved`, `unresolved`, `ambiguous`, or `unmet-qualifier`, which adds
`reasons` (`qualifier`, `spelled`, `declared`, null where a value is
absent). A duplicate declaration is reported only on standard error.

```text
{"item":"references","data":{"name":"ToolRef","domain":"tools","qualification":"explicit","links":[{"written":"stock/query@v1","domain":"tools","name":["stock","query"],"qualifiers":[{"name":"contract","value":"v1"}],"context":null,"field":"target","site":{"source":"data.kdl","span":{"start":{"offset":188,"line":10,"column":9},"end":{"offset":204,"line":10,"column":25}}},"candidates":[{"identity":{"domain":"tools","name":["stock","query"]},"alias":null,"labels":["program","tool"],"site":{"source":"data.kdl","span":{"start":{"offset":125,"line":8,"column":1},"end":{"offset":129,"line":8,"column":5}}}}],"gated":[],"outcome":"unmet-qualifier","reasons":[{"qualifier":"contract","spelled":"v1","declared":"v2"}]}]}}
```

### Ordering

Every view's output order is chosen, never the iteration order of a
hash map:

1. **Stream order** (`bound`, `origins`, `effective` on data): the order
   the reader yields items, never re-sorted. An imported document's
   items follow the import that brings them in.
2. **Entry order** (`context`): the order the entry documents are given
   on the command line.
3. **Section order** (`effective` on a definition target): types,
   nodes, slots, imports, profiles. Within a section, entries sort by
   name as `list` sorts them; imports sort by path, then alias.
4. **Slot order** (`context` slots, `plan-context` slots): the order the
   slots are declared, which is the order of their slot ids, not name
   order. `plan-context` lists namespaces in namespace-id order.
5. **Plan order** (`plan-node` lines): declaration order, with the nodes
   of an imported document where its import merged them.
6. **`requires` order**: the fixed order of the table above.
7. **Reference-type order** (`references` lines): the order the
   reference types are declared; within a line, links in stream order.

### When reading or compiling fails

Every view keeps the rule the source view follows: show, then fail.
What there is to show depends on the view.

- Stream views (`source`, `bound`, `origins`, and `effective` on data)
  write every item the reader yields, then exit 1 if any error was
  reported.
- `context` writes the context of every entry that was read, then exits
  1 if any error was reported. An entry that reads an unset slot still
  gets its line, showing that slot as null.
- Views rendered from the compiled definition (`effective` and `plan`
  on a definition target, and `plan` on data) have nothing to show when
  the definition did not compile. Standard output stays empty, the load
  diagnostics are reported, and one more line says why the view is
  missing, then the run exits 1:

  ```text
  error[KDLITH-SCH001]: unknown type Missing
    --> input.kdl:3:9
  kdlith: the plan view needs the compiled definition, which did not compile
  ```

- `plan` on data whose schema compiled but whose read failed still
  writes the whole plan; the read's exit 1 governs.
- `references` writes the report of a reading that completed, even one
  that reported binding errors, then exits 1 on those errors or, under
  complete scope, on its link diagnostics. A syntax fault, a missing
  document, or a crossed limit still completes the reading as
  diagnostics, so the report prints with the links bound before the
  fault. Only a reading the reader abandons as broken, which no input
  document provokes, writes no report.
- When the `--schema` schema of `inspect data` does not compile, there
  is no stream either: the stream views, `context`, and `references`
  write nothing and exit 1.

## Changing the dialect

Change the canonical files first, then this record. A mechanism moves
between files, or between defined and deferred, only as a deliberate
change to both: the declarations move, fixture coverage follows, and
this record's tables move with them. Retiring the `deferred` marker
followed the same rule in reverse — the marker's removal from
`documents.kdl` and this record's deferral disposition landed together,
and a future deferral re-introduces the marker the same way.
