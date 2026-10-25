# Generated crate contract

`gen` renders every name a definition declares as a Rust identifier:
types and enum members or union variants in Pascal case, field keys
in snake case (a Rust keyword as a raw identifier, `type` as `ty`),
and each contributing document as a module named after its file stem.
A field's type imports every engine item it spells at any depth. The
dialect admits names Rust cannot spell, so generation checks them
before writing anything and fails closed, naming the name and the
reason, when: a member, variant, or field key is no Rust identifier;
two of one declaration's spellings collide; a document's module name
is no identifier; or a declared, reference, or node-local type is
named `Self`.

A schema crate spells every prelude and engine name by path
(`::core::option::Option`, `::std::vec::Vec`,
`::kdlith_engine::Ref`), so a schema may declare types named
`Option`, `Vec`, or `Ref`. The declaration macros spell every derive,
macro, and trait by path (`::core::hash::Hash`, `::core::clone::Clone`)
on both forms, so a declared type a sibling module imports by name —
`Hash`, `Clone`, `Debug`, `Default`, `PartialEq`, `Eq` — never shadows
one. A dialect crate keeps the reference
processor's unqualified spellings, so the dialect form fails closed
on those names instead. A dialect declares no reference types, so on
the schema form the only reference-type name generation refuses is
`Self`.

Generic declarations are never spelled themselves: each instantiation
(`Pair<String, Mode>`) generates as its own type under a mangled name
(`PairStringMode`), with substituted fields; two instantiations
spelling one mangled name fail closed, under `--dry-run` and
`--check` too, never inventing a suffix. Such a definition still
vets: the limit is the generated crate's.

## The Item enum

Each reading node's items stream as a variant of the crate's `Item`
enum, named after the node's type. When any declared type is named
`Item`, the enum is named `ItemKind` instead; when both are taken,
generation fails closed. A reading node reads as exactly one object
type, so a node whose cases and `otherwise` do not select exactly one
object type fails generation closed, naming the node and its types;
the schema still vets. When differently named nodes bind one type,
each reads as its own variant named after the node, so a consumer
tells them apart by variant while the type-level projection matches
all of them; nodes of one name under different parents share one
variant. When a record and a declaration of different types would
spell one variant on the schema form — a record node `workspace` of
type `Workspace` beside a declaration node `workspace` whose type backs
several nodes — each such item keys its own variant by node, suffixed
with its role: `WorkspaceRecord` and `WorkspaceDeclaration`. The suffix
depends on the item alone, never on declaration order, and each keeps
its own node handler. Generation fails closed when two nodes would spell one
variant for different items (`KDLITH-GEN004`) or a node keys a
variant that is no Rust identifier. Every generation refusal prints
`kdlith: gen <form>: <reason>` and exits 1.

## References, codecs, unions

A declared reference type generates as a newtype over the engine's
symbolic reference to its target, carrying the lookup domain and
qualifier names as associated constants; reading never resolves it. A
pattern codec generates as a newtype over its output type; a field
typed by it holds the decoded value, never the spelling. Both join
the crate's type shapes for the conformance check. A generated union
reads a bare scalar exactly as the binder decodes it; the engine's
`variant_value` takes the variant table, and the `union_type!` macro
builds it from `prefix "…"` annotations.

## Manifest and packaging

A writing run records every owned path in `DIR/kdlith-manifest` and
refuses unowned overwrites (`KDLITH-GEN001`, nothing written);
generation never deletes anything. `--dry-run` prints the plan;
`--check` reports missing, changed, and stale paths, each
`KDLITH-GEN002`, exiting 1 on any finding; the modes exclude each
other. The generated `Cargo.toml` is standalone: by default the
kdlith dependencies are sibling paths, `--kdlith-path PATH` re-roots
them, and `--kdlith-version VER` writes registry dependencies —
values verbatim, flags mutually exclusive, empty or non-UTF-8 values
failing closed. `gen dialect` takes neither flag. `--kdlith-workspace`
(both forms, exclusive with the other two) writes
`{ workspace = true }` dependencies and `[lints] workspace = true`, so
the bytes stay a pure function of the inputs while the source moves to
the consumer's root manifest.

`gen workspace` is the workspace-level half. It reads one named source
of a sources manifest (an origin at the repository root with a 40-hex
`rev`, else `KDLITH-GEN007`), checks the root
`[workspace.dependencies]` line by line (`KDLITH-GEN006` missing,
different, or local; the root manifest is never written, and kdlith
links no TOML library), and owns `.cargo/config.toml` by its first-line
marker: a `[patch."<git>"]` over the schema reader's transitive kdlith
closure when the source is overridden, the marker alone otherwise. The
closure is a codegen constant (`SCHEMA_CLOSURE`); the
`codegen/gen-workspace-oracle` fixture compares it with the
repository's own `cargo metadata` resolve graph, since a partial patch
resolves crates twice and an extra entry is an unused patch.
