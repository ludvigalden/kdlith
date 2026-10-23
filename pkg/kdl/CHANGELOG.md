# Changelog

All notable changes to kdlith are recorded in this archive. The
`kdlith changelog` command prints it exactly as committed. Versions
follow the workspace version; entries are grouped by kind of change.

## 0.1.0 (unreleased)

The first development line of the workspace. Nothing has been
published yet.

### Added

- Cargo workspace with the library crates `kdlith`, `kdlith-parse`,
  `kdlith-model`, `kdlith-runtime`, `kdlith-diagnostic`,
  `kdlith-engine`, `kdlith-codegen`, `kdlith-superproc`, and
  `kdlith-cli`, plus the `kdlith` command-line tool.
- KDL v2 parsing through a pull-based event surface with source
  coordinates, verified against the official KDL v2 conformance suite.
- Canonical dialect definition files for types, bindings, documents,
  and context, with fixture entrypoints that generate the dialect.
- A dialect design record describing schemas, bindings, context,
  imports, and provenance.
- Runtime sessions with bounded binding frames and context effects.
- Engine schema compilation with type and binding catalogs, linking,
  import resolution, and a compiled processing plan.
- A typed binder producing bound item streams, with data imports and
  handler effects executed during a reading.
- A hand-maintained reference processor for schema definitions, used
  to compile definitions through its typed reader.
- Deterministic generation of Rust processor crates from compiled
  definitions.
- A verb-first command tree: `gen`, `vet`, `export`, `inspect`,
  `list`, `read`, `cache`, `changelog`, and `help`.
- `gen` with dry-run planning, drift checking, and refusal to
  overwrite files it does not own.
- `vet` and `inspect` for dialect definitions, schema definitions, and
  data documents, and `read` for one definition or indexed data item.
- `export` with entry and type selection, atomic or provisional
  delivery, provenance on every exported record, and an output policy
  for sensitive fields.
- `list` for declared types, nodes, profiles, and indexed data items.
- `cache` to inspect, refresh, and clean a versioned derived-state
  directory whose entries record what derived them.
- `changelog` to print this archive.
- A committed fixture suite and the `kdlith-fixtures` harness that
  drives the built command-line tool, including byte-exact goldens,
  isolated scratch directories, and a generate, compare, compile, and
  exercise oracle for generated crates.
- A contribution-level application scope: the canonical dialect names
  scope terms with the `ScopeName` vocabulary type, which changes the
  canonical dialect digest. A schema types a contribution's `scope`
  field with a reference type over its own scope declarations, so the
  link stage resolves it and an unknown or duplicated scope fails
  closed with the reference diagnostics.
- The shipped `layered` resolver partitions resolution by scope: layer
  precedence applies within each target-and-scope partition, and each
  `resolution` item carries its scope.
- Per-field merge for contributions: a contribution type's list field
  may declare `merge=union` with the `MergePolicy` vocabulary type,
  which changes the canonical dialect digest. Under a union-selecting
  policy the shipped `layered` resolver appends a higher layer's list
  items after the lower layers' items instead of overwriting the list,
  within each target-and-scope partition, skipping items whose
  declared shape is already held. A merge field under
  `selection=replace` or on a field that is not list-typed is rejected
  when the schema is compiled.
- Kind-directed dependent payloads: the `from-kind` binding codec,
  which changes the canonical dialect digest, defers a dependent
  payload until its reference resolves and selects its shape from the
  value the referenced declaration holds in the member field: `flag`
  binds a Bool, `number` a U64, `text` a String, and `choice` one of
  the declaration's bound choices. A payload outside the selected
  shape, a missing kind, and an unknown kind fail at linking.
- A dependent value binding whose target field is not a reference
  type, or whose member the referenced type does not declare, is
  rejected when the definition is compiled.
- `inspect data --view plan` lists every field typed with a declared
  reference type as a deferred reference-type entry, distinct from a
  plain `ref<T>`.
- `export` takes `--scope local|complete` (default `complete`): under
  `complete` the export's link stage fails the run on an unresolved
  reference before delivery — the stream is discarded atomically, or
  kept with a provisional-emission warning; under `local` the link
  report is printed and never decides the exit.
- Generated schema crates whose definition declares contribution-role
  nodes expose a typed `Contribution` enum and a `TypedItem::contribution`
  accessor that projects each raw contribution or resolution payload to
  the node's declared object type, failing closed with a named error
  when a payload fits no declared contribution type.
- Every generated type that backs a declaration-role node carries a
  `DOMAIN` constant naming the identity domain its declarations
  register in.
- Definitions whose only nodes are contribution-role now generate a
  processor where they were refused before; the no-items refusal names
  record, declaration, and contribution nodes.
- Generated schema-form types are exhaustive: every type declaration
  emits the exhaustive marker, so a `match` over a union, enum, or the
  `Item` enum needs no wildcard arm and objects construct with a
  struct literal. Dialect-form generation is unchanged.
- String-typed newtypes implement `Display`, `Borrow<str>`,
  `AsRef<str>`, and `PartialEq<str>`, so they serve as `HashMap` keys
  looked up by plain string literals and format with `{}`.
- Generated schema crates expose `Item::name()`, returning the single
  string identity field of a declaration and `None` for records,
  composed identities, and non-string identities.
- The engine root re-exports `Value`, and generated schema crates
  re-export `Value`, `TypedReader`, and `ReadError`, so a tool
  depending on the generated crate alone can name its reader and
  error types.
- Reference-typed newtypes dereference to their `Ref`, so `Ref`
  methods are reachable on the field and `Reading::resolve` takes the
  dereferenced or coerced borrow. `Reading::link_of` documents its
  gate list: the alias namespaces that hid a match, reported for each
  resolved reference. Enum-typed newtypes implement `Display` through
  their member name.
- Typed records and declarations carry the context slots they bound
  under, stamped per item when a reader is capturing (replayed
  documents included), and both carry the node name of the node that
  produced them.
- The engine's data reader accepts documents held in memory through
  `Source { shown, text }` and `read_sources`, with the typed
  reader's `read_sources` and `collect_sources` delegating over the
  same reading; an import inside an in-memory document fails closed.
- The engine root re-exports the field-binding value-source enum as
  `FieldSource`, leaving `Source` to name an in-memory document.
- A `bound` object constraint declares an inclusive `min`/`max` range
  over an `Integer`, `Decimal`, or `U64` field, compared exactly and
  per element of a list; a number outside it is `KDLITH-BND012`.
- `at-most-one FIELDS...` and `exactly-one-field FIELDS...` object
  constraints: at most one, or exactly one, of the listed fields holds a
  value, however each binds — argument, property, or child. A broken
  exclusivity list is rejected when the schema is compiled
  (`KDLITH-DLC005`), and a bound node violating the constraint is
  `KDLITH-BND013`, reported once per broken list. Both spellings change
  the canonical dialect digest.
- Node declaration metadata: a schema declares annotation vocabulary
  with `annotation NAME TYPE [required]` and node declarations carry
  typed `annotation NAME VALUE` facts plus `description=`
  documentation, which changes the canonical dialect digest. Every
  value is checked against its annotation's declared scalar type when
  the schema is compiled, a required annotation is total over every
  node declaration, and all five failure modes — a value outside its
  type, an undeclared name, a repeated annotation on one node, a node
  missing a required annotation, and a non-scalar annotation type — are
  `KDLITH-DLC005`. The metadata never changes binding: nothing in
  binding, linking, provenance, or export consumes it, `inspect schema`
  shows it (each declaration streams as an `AnnotationDef` item, the
  effective view lists it per node and per declaration), and generated
  schema processors expose one public constant per annotation fact,
  typed by the annotation's declared type.
- Node groups: a schema declares `group NAME MEMBER...` once, a node
  declaration admits every member with one `children-group NAME` rule,
  and `parents group=NAME` places a node under every member, which
  changes the canonical dialect digest. Groups expand to per-name
  children rules and placements when the schema is compiled, so
  dispatch stays static and a child outside every group is still
  `KDLITH-BND001`; a group rule's occurrence bounds apply to each
  member. An undeclared group is `KDLITH-SCH006`, a group without
  members is the new `KDLITH-SCH015`, and a member naming no declared
  node or a child spelled both directly and through a group is
  `KDLITH-SCH005`.
- `root=#true` on a node declaration that also gives `parent=` or
  `parents` admits the node at the document root as well, so a
  recursive container that may also start a document needs one
  declaration instead of two; this changes the canonical dialect
  digest. `list nodes` reports such a node as `parent=root,` followed by
  its parents. An unbound property whose name the node admits as a
  child now carries a note saying to write it as that child.
- The reference processor loads dialect and schema definitions held in
  memory through `load_dialect_sources` and `load_schema_sources`,
  reading `Source` documents through the same typed reader and
  compiler as the path loaders, and generated dialect processors carry
  the same loaders; an import inside an in-memory definition fails
  closed (`KDLITH-IMP001`). The typed reader gains
  `read_sources_local` for local-scope readings of in-memory documents.
- Bound and typed records and declarations carry `parent`, the node of
  the nearest enclosing item (none at a document's root level), outside
  their serialized form. The design record documents the stream order:
  node indexes count in document order as nodes open, and an item
  follows the items nested in it.
- The bound on how many sources one reading invokes (default 1024,
  entry documents and import instantiations together) is configurable
  through `--max-invocations N` on every data verb and
  `ReadOptions::with_max_invocations`, and documented. Its
  `KDLITH-SRC003` message now names the invocation count of entries
  plus imports instead of an import count, and the correction points at
  reading smaller batches or raising the limit.
- Children rules carry `when` branches: `when property=P equals=V`
  leaves compose through `when-all` and `when-any`, and a branch's
  `min-occurs`/`max-occurs` replace the rule's own bounds for the
  instances it matches, which changes the canonical dialect digest.
  The schema compile refuses what it cannot decide: an undeclared
  condition property is `KDLITH-SCH006`, a property that is not a
  scalar or enum field is the new `KDLITH-SCH016`, an `equals` value
  that does not decode as the property's type is `KDLITH-DLC005`, two
  branches not provably disjoint by a required leaf on one shared
  property are the new `KDLITH-SCH017`, a combinator with no members
  is the new `KDLITH-SCH018`, and a rule with branches but no base
  bounds is the new `KDLITH-SCH019`.
- Import aliases: a host registers a named root — a directory, or
  documents held in memory such as ones embedded in a binary — through
  `ReadOptions::with_alias` (`AliasRoot::Fs`, `AliasRoot::Embedded`),
  the typed reader's `with_alias`, or `--alias NAME=DIR` on every data
  verb. An import written `$NAME/PATH` resolves inside that root
  wherever the importing document lives, and a document an alias loaded
  resolves its own literal imports relative to itself under the root.
  What an alias reaches is confined to its root instead of the entry
  document's directory: leaving it is the new `KDLITH-IMP007`. A
  malformed or repeated alias name, a name an anchor also uses, or a
  root that does not resolve is the new `KDLITH-IMP008`, before any
  document is read; with aliases registered, an unregistered leading
  `$name` is `KDLITH-IMP005` naming them.
- A map-typed or object-typed contribution field may declare
  `merge=union` under a union-selecting resolution policy, stating the
  key-wise merge it receives; any other non-list field declaring it is
  still `KDLITH-DLC005`.
- `--match-root DIR` on every data verb, and
  `ReadOptions::with_match_root`, name the directory profile match
  rules read entry paths relative to; without one it is the current
  directory.

### Changed

- Profile match rules compare an entry's path normalized and relative
  to the match root instead of as the invocation spells it, so an
  absolute, `./`-prefixed, or parent-relative spelling of one document
  selects the profile its root-relative spelling does.
- Under `--profiles`, an entry that satisfies no listed member's rules
  and finds no listed default now fails with `KDLITH-SCH011` instead of
  binding under the first listed profile.

### Fixed

- A data document's imports are confined to the entry document's
  directory: a target that resolves outside it — including through a
  symbolic link — is refused with `KDLITH-IMP006` before the target is
  read, so an import can no longer leak the contents of a file outside
  the entry tree. Definition documents loading through their governing
  dialect are exempt, and `--scope local` refuses as well.
- A schema declaring one node name under different parents, each
  declaration carrying the same annotation, no longer fails generation
  with an unspellable-constant error: the first declaration in
  contribution order keeps `<NODE>_<ANNOTATION>` and each later fact of
  the same node and annotation names takes an ordinal suffix from `_2`;
  the generated doc comment names the declaration's parent so the
  constants stay tellable apart.
- A fallback identity composed from a non-string field no longer
  composes an empty identity silently: the compiler notes the field at
  its declaration and the binder refuses the binding.
- Under `missing=reject`, the contribution resolver's missing-target
  check also refuses a reference held by a reference-typed map key,
  matching the link stage.
- Schema roots without a version are rejected on every reading path.
- Overlapping argument bindings are reported on every definition
  path, with one diagnostic code per rejected construct.
- A value-bound union accepts a number, a bool, or an un-prefixed
  string that an un-prefixed variant's type accepts: prefix selection
  runs first, then the first such variant in declaration order binds
  the value, so a union can hold a literal number beside a prefixed
  template variant.
- A node whose `cases` dispatch selects an object type no longer binds
  a field that type does not declare: such a field is refused with
  `KDLITH-BND014`, naming the node, the field, and the selected type,
  and the node is not emitted. The case selector itself stays
  admitted.
- An entry document named on the command line that cannot be read —
  missing, or a directory — is reported as `KDLITH-SRC001` with its
  path and the OS reason on `vet`, `inspect`, `list`, `export`, and
  the other data verbs, instead of as an unresolved import. A genuine
  import that does not resolve stays `KDLITH-IMP001`, and its message
  now names the reason; a directory a discovery expansion cannot list
  is `KDLITH-SRC001` rather than an anchor fault.
- Every diagnostic on stderr ends with a `help:` line carrying its
  code's correction, which until now only the JSON item stream showed.
  An unknown node, an unbound property, and an unknown enum value also
  carry a `did you mean …?` note naming the nearest admitted names
  within edit distance 2, which the JSON stream carries as a positioned
  label.
- Union resolution no longer drops nested keys: under
  `selection=union` the shipped `layered` resolver merges contributed
  values deeply, an object field by field and a keyed collection key by
  key at every depth, so a key only a lower layer contributes survives
  and a leaf both layers hold takes the higher layer's value. Before,
  every map-valued field was replaced whole by the highest layer. A
  list declaring `merge=union` inside a nested block now appends at its
  own depth instead of being silently replaced.
