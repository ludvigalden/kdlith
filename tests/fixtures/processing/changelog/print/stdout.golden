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
- Open-world reference arms, which change the canonical dialect
  digest: a reference type may declare `open-world
system-member=MEMBER values="V1 V2 ..."` (one whitespace-separated
  value set), and a reference of that type that resolves to no
  declaration then links with an `external` outcome — no diagnostic,
  rendered by `inspect data --view references` beside the resolved
  ones — when the declaration its outermost identity segment names
  (spelled as the qualifier of that field, as the first name segment
  when every segment is spelled, or filled from context when exactly
  the outermost one is missing) is indexed in the reading and holds
  one of the listed values in the named member. Every other
  unresolved reference still fails `KDLITH-REF001`, and a schema that
  declares no arm fails closed exactly as before; an externally
  resolved reference skips payload validation as an unresolved one
  does, replays and restores through the bound-result cache, and a
  typed reading's `resolve` answers `Resolved::External` at the site
  with no default value. An arm whose target composes no identity, or
  whose member or a listed value is statically wrong against the type
  the outermost field's declared qualifier names, is rejected as
  `KDLITH-SCH021` when the schema is compiled.
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
- A `children-group NAME` reference in a `from` block: one children
  source per group member splices into the field's sources at the
  reference's position when the schema is compiled, exactly the
  hand-written repeated-source form, which changes the canonical
  dialect digest. An undeclared group is `KDLITH-SCH006` under every
  checking scope: unlike a children rule, which only grants permission,
  a from source decides where a field's data comes from, so an
  unresolvable reference is never silently dropped.
- A `child-group NAME` reference in a `from` block beside the
  `children-group` form: one child source per group member splices into
  the field's sources at the reference's position, so a field sourced
  where a `child NAME` source is spelled today admits every member
  spelling through one reference, which changes the canonical dialect
  digest. The expansion is exactly the hand-written repeated form, so a
  group over several members carries `collision reject` as its twin
  must, and a member spelled both concretely and through the group in
  one field is a compile-time duplicate (`KDLITH-SCH005`), reported and
  skipped for both group forms.
- A `from-type-name` dependent-value codec beside `from-kind`: the
  member field the `against` target names holds a type name as a
  string, and the payload binds under the declared type that string
  names, which changes the canonical dialect digest. A bare name
  resolves to the declared type it spells and a `list<T>` or
  `optional<T>` spelling admits those forms; a name no declared type
  answers, or a payload that does not decode as the named type, fails
  closed at link (`KDLITH-REF005`), and a reference resolving external
  carries no declaration, so its payload stays pending.
- Two derived field sources in a `from` block, which change the
  canonical dialect digest. `ordinal` supplies a field, when nothing
  else does, with the node's 1-based ordinal among its same-named
  siblings under the same parent, spelled `<key>.<ordinal>` through the
  named key field's value or as the bare ordinal when no key is named;
  the ordinal is counted per key within the parent, so omitted mapping
  ids bind as `source.1`, `source.2`, and an explicit value wins over
  the derivation. `parent FIELD` supplies a field, when nothing else
  does, with the value the immediately enclosing node bound in the named
  field of its header, so a child whose code repeats its parent's binds
  once instead of per occurrence. The derived value composes the
  declaration's identity and reaches every reading path as a bound
  field; a field whose type does not admit only strings, a key or
  parent field nothing declares, or a parent field not bound from the
  parent's header fails closed at definition compile
  (`KDLITH-SCH005`/`KDLITH-SCH006`), and an occurrence that derives
  nothing reports through the required-field check (`KDLITH-BND004`).
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
- `--diagnostics json` on the data verbs (`vet data`, `inspect data`,
  `export`, and `read data`) writes every diagnostic the run prints to
  standard output as one compact JSON line per record, beside the
  unchanged standard error text and exit.
- A `child` or `children` source declaring `compact=#true` admits the
  child spelled as a property of the node it selects from:
  `select tag=go` binds exactly as `select { tag go }`, one child per
  property at the property's position, which changes the canonical
  dialect digest. Only a child whose every field binds from its
  argument 0 alone admits the spelling; any other admission is
  `KDLITH-DLC005` at schema compile. A property no compact admission
  names stays `KDLITH-BND005`, and a value its child's field rejects is
  `KDLITH-BND003`.
- Locations and sources: every import path is a location in a source.
  `rel/path` resolves from the importing document's directory and
  `/path` from its source's root, both inside the importer's source;
  `$name/path` resolves from the root of the named source wherever the
  importer lives. The local source is rooted at the project root, the
  current directory or `--root DIR` (`ReadOptions::with_root`); a host
  adds named sources backed by a directory (`--source NAME=DIR`,
  `ReadOptions::with_source` with `SourceTree::dir`) or by embedded
  files (`SourceTree::embedded`). A `locations` rule on the schema root
  or a profile admits `*`, `**`, and `{a,b}` patterns, expanded in
  named sources too. The new `LOC` diagnostic area reports a location
  naming no document (`KDLITH-LOC001`), leaving its source
  (`KDLITH-LOC002`), an unregistered `$name` (`KDLITH-LOC003`), syntax
  the governing rule does not admit (`KDLITH-LOC004`), and an invalid
  root or source (`KDLITH-LOC005`).
- Sources on every definition path: `load_schema_sources_with` (and
  `load_sources` and `load_sources_with` in generated schema crates)
  loads a schema held in memory whose `$name/` imports resolve in the
  sources its `ReadOptions` registers. Generated schema crates
  re-export `Source` and `SourceTree`, so a host registers an embedded
  library of schema documents without depending on the engine.
  `kdlith_cli::run_with_sources` runs a command line with host-held
  sources registered on every verb that reads a schema or data.
- The fixture harness's `source-embedded: ["NAME=DIR"]` scenario key
  runs a scenario twice, with DIR as a `--source` directory and as an
  embedded source, and fails unless both runs agree byte for byte; a
  codegen-oracle scenario's `sources` key passes `--source` to its
  generate steps.
- The `asset` type kind: `asset NAME { kind file|dir; extensions …;
base document|source; patterns #true; digest #true }` declares a
  field whose value is a location naming a file or directory that is
  not a KDL document. The reading resolves it as it resolves an import
  location (relative, `/root`, or `$name/`, from the referring
  document's directory or, under `base source`, its source's root),
  confines it to its source (`KDLITH-LOC002`), and checks that it
  exists (`KDLITH-LOC001`) and matches its kind and extensions
  (`KDLITH-LOC008`); it never reads the content beyond an optional
  digest. A pattern is admitted only where the type enables patterns
  and only as one item of a list (`KDLITH-LOC004`). Bound values carry
  the new `asset` value kind, `{"location","source","digest"?}`, in
  every output. Generated readers declare each asset type as a newtype
  over the re-exported `Asset`, and `Reading::open` reads a file
  asset's bytes through the reading's own sources; an asset another
  reading resolved is refused (`OpenError`).
- Sources manifests: `--sources FILE` (`ReadOptions::with_sources_manifest`)
  reads a `sources` document declaring sources by name, either
  `source NAME dir=DIR` or an origin, `source NAME { origin git=… rev=…
path=…; pinned "sha256:…" }`, whose pin is optional. kdlith never
  fetches: an origin resolves from the store (`--store DIR`,
  `with_store`) at `sha256/<hex>` when pinned and at `origin/<key>`
  when not; an absent tree is `KDLITH-LOC006` and its help names what
  to fetch and where. A pinned tree is checked against its canonical
  tree digest (SHA-256 over its files) on first use, a mismatch being
  `KDLITH-LOC007`. Every unpinned source a reading uses is noted once
  with its digest (`KDLITH-LOC010`, a note, which never fails a run).
  The manifest's shape is the `kdlith-sources` schema
  (`pkg/kdl/schemas/sources/sources.kdl`).
- Overrides: `override NAME dir=DIR` replaces a declared source, written
  in the sources manifest, in an `--overrides FILE` (`with_overrides_file`),
  or as `--override NAME=DIR` (`with_override`), the most local layer
  winning. Every override is a `KDLITH-LOC009` warning naming its layer,
  the identity it displaced, its directory, and the overrides it
  shadowed; `--warnings-as-errors`, on every verb that reads a schema or
  data, fails the run on it. An override naming no declared source,
  carrying a pin, or repeated in one layer is `KDLITH-LOC005`, as is a
  manifest source reusing a name the host registers.
- `kdlith sources plan|verify|list`: the fetch plan, a re-hash of every
  stored origin tree against its pin, and every source with the digest
  of the tree a reading reads, one JSON line per source.
- Diagnostic records carry `facts`, named values a consumer reads
  without parsing the message, serialized only when present.
- Every data reading ends with one `sources` item listing the sources
  it used: the local source first, then each named source by name,
  with its kind, the identity of the tree it read (a pin, an embedded
  build digest, or a digest computed from its content), whether that
  tree is pinned, its origin, and any override replacing it. The item
  carries the source-set fingerprint, the SHA-256 over the named
  sources used with their identities.
- Cache entries and snapshots record the source-set fingerprint: a
  bound result is keyed under it, and a snapshot restores only when
  every named source it recorded still reads the same tree, so one
  document read against two different trees of a source never shares
  a cache entry or a restore; a changed source is a miss, never a
  stale hit. A reading that uses no named source keys as before.
- `gen schema` and `gen dialect` take `--kdlith-workspace`: the
  generated manifest names each kdlith crate `{ workspace = true }` and
  inherits the workspace lints, so the consuming workspace's root
  manifest names the kdlith source. It excludes `--kdlith-path` and
  `--kdlith-version`.
- `kdlith gen workspace --sources FILE` checks a consuming workspace
  against one named source (default `kdlith`), an origin of the kdlith
  repository root at a full commit: the root `[workspace.dependencies]`
  must spell each crate a schema reader needs as
  `{ git = "<git>", rev = "<rev>" }`. A line that is missing, different,
  or local is `KDLITH-GEN006` with the expected line; the root manifest
  is never written. An overridden source writes the owned
  `.cargo/config.toml`, a `[patch]` of the kdlith git source covering
  every kdlith crate a schema reader resolves, pointed at the override's
  checkout; without an override the file holds the kdlith marker
  alone. An unowned file is refused (`KDLITH-GEN001`), `--check`
  reports drift as `KDLITH-GEN002`, and a source that cannot pin the
  crates is `KDLITH-GEN007`. A writing run prints the one-time cargo
  cache seed for an unreachable origin as a note.
- `read data --scope` also admits a kebab-form scope name beside
  `local` and `complete`: the lookup then consults only the resolution
  partitions settled under that scope, a record is found under any
  scope, and a miss names the restriction (`under --scope NAME`). A
  malformed name stays `KDLITH-CLI001`; `vet` and `inspect` keep
  `local|complete`.

### Changed

- Breaking: an asset `digest` is the SHA-256 of a file's bytes, or a
  directory's canonical tree digest, written `sha256:<hex>`, in place
  of the interim `fnv1a64:` digest.
- A schema load's warnings and notes, such as an overridden source,
  are reported by every verb that loads a schema, and decide the exit
  under `--warnings-as-errors`.

- Breaking: a schema definition's own imports are confined as a data
  document's are: an unprefixed import stays under the project root and
  a `$name/` import inside its source (`KDLITH-LOC002`). Dialect
  definitions keep resolving from their own directory unconfined. A
  location naming a missing file in a directory source now reads "the
  source holds no such file", as it does in an embedded source.

- Breaking: the locations model replaces discovery grammars, anchors,
  and host import aliases, with no compatibility path. Removed from the
  dialect: `discovery`, `anchor`, `provider`, and the `grammar=`
  property on `schema` and `profile`. Removed from the command line:
  `--anchor` and `--alias`. Removed from the library:
  `ReadOptions::with_anchor`, `ReadOptions::with_alias`, `AliasRoot`,
  the typed reader's `with_anchors` and `with_alias`, and
  `load_schema_anchored`, replaced by `load_schema_with` (and by
  `load_with` in generated schema crates) taking `ReadOptions`.
  Unprefixed data imports may now reach anything under the project
  root instead of only the entry document's directory, and resolved
  locations are shown normalised. `KDLITH-IMP001` and
  `KDLITH-IMP004` through `KDLITH-IMP008` are retired in favour of the
  `LOC` codes. The canonical dialect digest changes.
- `as=NAME` on an import is now called a namespace: the import item's
  and the typed origin's `alias` field is `namespace`, the dialect type
  `AliasName` is folded into `NamespaceName`, and diagnostics name the
  namespace. Its behaviour is unchanged.
- `--match-root` is now spelled `--root`, which also roots import
  locations; `--match-root` still names the same root.

- Profile match rules compare an entry's path normalized and relative
  to the match root instead of as the invocation spells it, so an
  absolute, `./`-prefixed, or parent-relative spelling of one document
  selects the profile its root-relative spelling does.
- Under `--profiles`, an entry that satisfies no listed member's rules
  and finds no listed default now fails with `KDLITH-SCH011` instead of
  binding under the first listed profile.
- The engine's `variant_value` takes the union's variant table, each
  variant name with its optional prefix, and a per-variant acceptance
  test instead of a bare name list, and `union_type!` variants may
  carry `prefix "…"`. Generated typed readers now read a bare scalar
  union value exactly as the binder decodes it: a matching prefix
  selects its variant, otherwise the first un-prefixed variant whose
  type accepts the value.
- A child bound as one field value and written with no argument is
  reported as `KDLITH-BND004` naming the field and the spellings that
  supply it instead of naming only the valueless child:
  `card is missing required title (expected child title VALUE)`, with
  `property title=VALUE or` added when the source declares
  `compact=#true`.
- A composed identity may draw a segment from a field typed with a
  declared reference type, so a checked name can address a
  declaration. The segment is the reference's explicit qualifier
  values then its name segments, joined by `.`; when no qualifier is
  spelled and the type admits context, the context value leads. The
  reference still links, so a misspelling fails `KDLITH-REF001` under
  complete scope. Before, such a field was `KDLITH-BND003` and the
  declaration registered no identity.
- `from { ordinal key=F }` admits a key field typed with a declared
  reference type or `ref<T>`. The key text is the segment the
  reference composes into an identity, so a bare and a qualified
  spelling of one target share a counter, and one bare name in two
  namespaces counts twice. The reference still links. Before, such a
  key was `KDLITH-SCH005`.
- A `case` body admits `children-group NAME` rules. For that case
  alone each expands to one `children` rule per group member, carrying
  the rule's bounds and `when` branches, as if the members were spelled
  one by one. A member the case already rules is `KDLITH-SCH005`, and a
  member the node rules in common is `KDLITH-SCH023`. Before, the rule
  under a case was `KDLITH-BND001`.
- A case field's `from` block admits `children-group NAME` and
  `child-group NAME` sources. For that case alone each splices one
  `children` or `child` source per group member at the reference's
  position, exactly the hand-written repeated form, with the same
  duplicate (`KDLITH-SCH005`) and collision-policy checks as at node
  level. Before, either reference under a case was `KDLITH-SCH005`.
- `gen dialect` and `gen schema` write `license = "MIT"` into the
  generated manifest's `[package]` after its `description`, so a
  generated crate carries the metadata a registry publication needs.

### Fixed

- A schema whose record and declaration of different types would read
  as one item variant no longer fails generation with `KDLITH-GEN004`:
  each such item keys its own variant by node, suffixed with its role
  (`WorkspaceRecord`, `WorkspaceDeclaration`).
- A generated crate compiles when a schema declares a type named after
  a derived trait (`Hash`, `Clone`, `Debug`, `PartialEq`, …): the
  declaration macros spell every derive and trait by its full path.
- Generated Rust files are rustfmt-clean: rustfmt leaves a generated
  crate unchanged, and `gen --check` compares the same formatted bytes
  a write produces.
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
- A dependent value whose `against` target is typed by a declared
  reference type now binds its payload as pending when the referenced
  declaration composes its identity from an `identity` block, so
  linking validates it and a payload that does not fit fails
  `KDLITH-REF005`. Before, only a declaration keeping the fallback
  identity was reached; a composed one dropped the payload unchecked
  and the reading passed.
- A child name that two cases of one node each bind now binds under
  whichever case the node's dispatch selects, with that case's own
  field and bounds. Before, only the first case consumed it and the
  other case's child was `KDLITH-BND001`. A case that did not match
  still admits none of its children.
