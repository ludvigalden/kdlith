# Contract for the engine and the reference processor

The engine (`pkg/rs/engine`) and runtime (`pkg/rs/runtime`)
implement the dialect for every processor, hand-written or generated:

1. **Compile once.** Compile dialect and schema definitions into
   catalogs and a processing plan; never reinterpret a definition's
   KDL per record.
2. **Merge contributions.** Combine documents that repeat one root
   name and version into one dialect or schema, and reject a
   declaration made twice.
3. **Resolve imports** as `documents.kdl` states: every path a
   location inside a source, written order, projection only,
   mutations never escaping, cycles diagnosed from the invocation
   stack, `repeated` defaulting to `reject`. Patterns expand only
   where a locations rule admits them, and no rule admits
   environment-variable interpolation. Namespaces preserve both
   source and namespace and gate visibility. Processing
   `pkg/kdl/dialect.kdl` yields the same dialect as reading the four
   canonical files directly, and that equivalence has its own fixture
   scenario (`dialect/entrypoint-vet/dialect`).
4. **Link.** Check that every type, node, slot, and parent a
   definition references is declared, including the canonical files'
   own self-description. Every field an `identity` block composes
   must bind a string or a reference of a declared reference type; a
   composed identity field holding any other value is rejected at
   binding (`KDLITH-BND003`). A reference contributes one segment:
   its explicit qualifier values, then its parsed name segments,
   joined by `.`; when no qualifier is spelled and its type admits
   context, the context value in effect leads instead, as it does a
   declaration's own context-sourced segment. The reference is still
   linked, so a misspelling fails `KDLITH-REF001` under complete
   scope. A declaration without an
   `identity` block takes its type as its domain and the value of its
   argument-0 field as its name, or is one of a kind in its scope
   when no field binds argument 0; a field type admitting any other
   value draws a note at the declaration (`KDLITH-SCH014`) and
   binding rejects a non-string (`KDLITH-BND003`).
5. **Enforce bindings.** Reject colliding sources, duplicate keys,
   overlapping bindings, unconsumed arguments, contradictory child
   ownership, and unpermitted children; check cardinality,
   exactly-one groups, and the first-node rule at the stated points.
6. **Dispatch deterministically** by profile, parent, node name, and
   explicit discriminators — node names, variant value prefixes, and
   property cases, each consulted as a compiled table with a single
   probe and no backtracking. The active profile is part of the
   position a node is admitted at, never something the engine guesses
   from content.
7. **Apply context** with the execution timing `context.kdl` states,
   and reject children effects that are not computable from the node
   header.
8. **Fail closed.** Reject a definition naming a handler, validator,
   resolver, or source-metadata provider that is not installed. The
   `type-expression` validator at version `0` is required.
9. **Decode codecs at bind time.** Apply a field's codec while the
   value arrives, keep the original spelling in provenance. Binding
   codecs follow a two-tier rule: a `codec NAME` no declaration
   defines fails closed, while a known codec the processor does not
   install still compiles and the binder emits its payloads as
   pending typed-binding items.
10. **Expand member groups** demand-driven: a compiled plan may carry
    an unresolved expansion reference, complete checking resolves
    every one, and a cycle in the expansion or
    generic-instantiation graph is `KDLITH-SCH009`.
11. **Bind dependent payloads** by looking up the referenced
    declaration's member type — immediately when the declaration is
    compiled, otherwise as a pending typed-binding item emitted right
    after the item holding its reference, the payload kept raw — and
    never promote a scalar to a one-element list. Linking validates a
    pending payload on demand: once its reference resolves to one
    declaration the payload must fit that member type — a list member
    zero or more, an optional member none or one, any other exactly
    one — or `KDLITH-REF005`. A `from-kind` payload always defers. An
    unresolved reference reports its link failure alone and leaves
    the payload unvalidated; nothing executes a payload.
12. **Enforce sensitivity.** Apply the caller's output policy (emit,
    omit, redact) in every serialization and diagnostic surface; a
    `secret` field is emitted only when the policy explicitly says
    emit.
13. **Select profiles from invocation context.** The active selection
    resolves once per entry document, before any node binds, through
    one shared total-order function; selection reads invocation
    context only, and dispatch consults it as part of position.
14. **Apply source context, accumulation, and duplicate policies** at
    the stated timing, with `reject` the fail-closed default
    (`KDLITH-CTX006`, `KDLITH-BND006`).
15. **Preserve contributions and provider descriptors.** Yield
    contributions as preserved records and route them to the
    registered resolver only. Never execute a provider command, never
    compute a computed default, never reinterpret a provider's
    argument list as a shell command string.
16. **Persist and restore context snapshots.** Index each record with
    its identity, type, source digest and span, schema fingerprint,
    snapshot ID, invocation ID, and validation status; restore only
    when the fingerprint and lineage match (`KDLITH-CTX004`).
17. **Scale with the reading.** Derive what binding needs from a
    definition — the binding tables and the compiled patterns — once
    per definition, never per document. Keep nothing of a document
    once its items have streamed unless the reading returns it: a
    plain reading holds neither texts nor items, a result is kept for
    replay only from the second invocation of its semantics, and only
    the reading made for the snapshot store keeps the context each
    record bound under. Link from what the stream leaves behind — one
    compact entry per declaration, one per bound reference — with
    declarations indexed by the hash of their composed name, so a
    lookup probes for the identity its reference determines and no
    pass scans a domain. Check a value against its type without
    copying it, and resolve a line and column, or spell a label, only
    where something records it. Whatever the size of a reading, its
    stream and its diagnostics are byte for byte those of the
    straightforward implementation.
18. **Read on demand, exactly.** A reading with a demand
    (`ReadOptions::with_demand`, `export --lazy`) passes over a node
    whose item is of no demanded type and inside which nothing
    demanded can be written (`demand.rs`): the session numbers the
    nodes inside it (`Disposition::Pass`) and nothing in it is
    decoded, validated, or yielded. A source import, a node whose
    plan carries children or following effects, and every node a
    live node can be written inside stay bound, so each demanded
    item is the item a full reading yields, index and context
    included. Such a reading resolves no contributions; what it
    passes over is trusted, never checked.
19. **Bind ahead only where order cannot matter.** With threads
    (`ReadOptions::with_threads`), the documents one import request
    fans out to are read and bound on workers under a provisional
    source identity and their items given, in request order, under
    their invocation's (`remap`); admission, replay, asset
    resolution, and every consumer callback stay on the reading's
    thread, in order. A document holding a followed import binds in
    its turn. Handlers and the value resolver are the consuming
    program's code and run in document order, so a reading that
    installs either binds on one thread. The count never changes a
    stream.

## Self-hosted compilation

Loading a definition is reading it: `vet` and `inspect` hand their
entry documents to the reference processor's typed reader, and the
engine compiles what the reader gives it — one construct per
declaration, already validated by binding, with the span it was
written at. Each construct stays with the document that wrote it, so
a compile diagnostic names that document. In-memory definitions load
through `load_dialect_sources` / `load_schema_sources`
(`load_schema_sources_with` takes `ReadOptions`); an in-memory
definition belongs to no source, so an unprefixed import inside one
fails closed (`KDLITH-LOC001`). Data in memory reads through
`DataReader::read_sources` and the typed `read_sources`,
`read_sources_local`, and `collect_sources`.

One cycle cannot close: a reader needs a compiled dialect before it
can read one. `superproc::seed` breaks it the only way possible — the
engine's seed bootstrap (`compile_dialect`), the one remaining
hand-written reader of definition syntax, compiles the four canonical
documents embedded beside the reference processor's declarations. The
bootstrap compiles nothing else, and the equivalence it exists for is
fixture-pinned: reading the canonical files through the reader they
seed reproduces the seed's own digest.

## superproc

The reference processor (`pkg/rs/superproc`) reads schema definitions
in this dialect: it reads every canonical file and the entrypoint end
to end; it holds only the dialect's own type and binding declarations
plus glue, with every reusable decision in the engine or runtime; it
hosts the compiler entry points (`load_dialect`,
`load_dialect_sources`, `load_schema`, `load_schema_with`,
`load_schema_sources`, `load_schema_sources_with`, and `seed`); and
it is the code-generation oracle — generating from
`pkg/kdl/dialect.kdl` must reproduce its source, apart from the
defined generated-marker header. It is reviewed by hand and never
overwritten by the generator.
