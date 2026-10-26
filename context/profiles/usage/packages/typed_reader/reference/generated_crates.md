# Generated crates

## Generating

`gen dialect` and `gen schema` both require `--crate NAME` (the
published package name) and `--out DIR`, before the definition
documents. A writing run records every path it owns in
`DIR/kdlith-manifest` and refuses to overwrite an existing file the
manifest does not own (`KDLITH-GEN001`, nothing written);
generation never deletes anything.

- `--dry-run` prints the plan and each owned path, writing nothing
- every generated Rust file is already rustfmt-formatted (style
  edition 2024, default widths), so rustfmt leaves a generated crate
  unchanged and `--check` compares exactly the bytes a write produces
- `--check` compares the generated tree against `DIR`: absent is
  missing, differing bytes are changed, and a manifest-owned path
  the tree no longer generates is stale — each one `KDLITH-GEN002`
  record, any finding exits 1
- the two modes exclude each other (`KDLITH-CLI002`)

`gen schema` also takes `--root DIR` and `--source NAME=DIR` for the
schema's own import locations and `--handler NAME` to admit a built-in
context handler the schema names. Both forms take the packaging
flags: `--kdlith-path PATH` roots the crate's kdlith path
dependencies at PATH, `--kdlith-version VER` writes them as registry
dependencies, and `--kdlith-workspace` writes each as
`{ workspace = true }` and replaces the crate's own `[lints.clippy]`
table with `[lints] workspace = true`. Values are written verbatim;
the three flags are mutually exclusive. Without any, the manifest
keeps sibling paths.

Every generated manifest carries registry metadata — the package
name, a version, and a `description` naming the definition the crate
was generated from — which is what a crates.io-style publication
requires.

A dialect crate embeds its seed as a versioned snapshot: the
dialect's contributing documents are written under `src/dialect/`,
`compile.rs` includes each through a crate-relative `include_str!`
beside the `CANONICAL_VERSION` constant naming the version the
documents declare, and the crate reads no file outside itself, so a
generated dialect crate is publishable and self-contained wherever it
is built.

## Workspace dependencies

Under `--kdlith-workspace` the consuming workspace's root manifest
names the kdlith source, and `kdlith gen workspace` keeps it in line
with a sources manifest. The source (`--kdlith-source NAME`, default
`kdlith`) must be an origin of the kdlith repository root with a full
40-hex `rev`; anything else is `KDLITH-GEN007`, and an undeclared name
`KDLITH-LOC003`.

```kdl
sources {
    source kdlith {
        origin git="https://github.com/ludvigalden/kdlith" rev="<40-hex commit>"
    }
}
```

- The root `Cargo.toml` is checked, never written. Each crate a schema
  reader needs (`diagnostic`, `engine`, `model`, `superproc`) must be
  spelled `kdlith-<crate> = { git = "<git>", rev = "<rev>" }` under
  `[workspace.dependencies]`; otherwise `KDLITH-GEN006` names the crate
  as missing, different, or local (a path dependency, or a `[patch]`
  entry in that manifest) with the expected line.
- An override of the source (in the manifest, an `--overrides FILE`, or
  `--override NAME=DIR`) names a local kdlith checkout D. The run then
  writes `<root>/.cargo/config.toml`, first line the kdlith marker, with
  a `[patch."<git>"]` entry `{ path = "D/pkg/rs/<crate>" }` for every
  crate a schema reader resolves (`diagnostic`, `engine`, `model`,
  `parse`, `runtime`, `superproc`), and warns `KDLITH-LOC009`. Without
  an override the file holds the marker alone. An existing file whose
  first line is not the marker is refused (`KDLITH-GEN001`) and left as
  it is. Keep the override and the patch file out of version control.
- `--check` writes nothing: root dependency findings are
  `KDLITH-GEN006`, and a patch file that is missing, unowned, changed,
  or stale (a patch with no override) is `KDLITH-GEN002`.

The patch applies only once cargo has fetched the original git source.
With an empty cargo git cache and an origin cargo cannot reach, seed
the cache once from the local checkout; a writing run prints this
recipe as a note:

```sh
GIT_CONFIG_COUNT=1 \
GIT_CONFIG_KEY_0=url.<checkout>.insteadOf \
GIT_CONFIG_VALUE_0=https://github.com/ludvigalden/kdlith \
CARGO_NET_GIT_FETCH_WITH_CLI=true \
cargo generate-lockfile
```

The variables apply to that one command, and the checkout must hold
the pinned commit. Once a lock file resolved with the patch exists, the
origin is not needed again.

## Naming limits

Generation renders every declared name as a Rust identifier
(PascalCase types, snake_case fields). It fails closed, naming the
name and the reason, when a name is no Rust identifier, two of one
declaration spell the same identifier, or a type is named `Self`.
Generic instances generate under mangled names
(`Pair<String, Mode>` becomes `PairStringMode`), and two
instantiations spelling one mangled name fail closed the same way.
Such a definition still vets: the limit is the generated crate's.

A type may take the name of a trait the generated code derives
(`Hash`, `Clone`, `Debug`, `Default`, `PartialEq`, `Eq`): every derive,
macro, and trait is spelled by its full path. Each record or
declaration reads as an `Item` variant named after its type. Where a
record and a declaration of different types would spell one variant,
each takes its node's name suffixed with its role, so a schema may
declare a record `workspace` beside a declaration `workspace`:

```rust
pub enum Item {
    Workspace => WorkspaceRecord,
    Kind => WorkspaceDeclaration | Component,
}
```

## Two forms

`gen dialect` regenerates a reference-processor-shaped crate;
`gen schema` generates a domain crate. The dialect form takes none
of the schema-only flags (`--root`, `--source`, `--handler`); the
packaging flags and `--kdlith-workspace` are shared.
