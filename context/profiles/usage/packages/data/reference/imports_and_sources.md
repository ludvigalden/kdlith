# Imports and sources

## Sources

Every import path is a location resolved against a source, a named
tree of files:

| Spelling     | Resolves from                                         |
| ------------ | ----------------------------------------------------- |
| `rel/path`   | the importing document's directory, inside its source |
| `/path`      | the importing document's source root                  |
| `$name/path` | the root of the source called `name`                  |

The reading's own tree is the local source, rooted at the project
root (`--root DIR`, the current directory by default). An unprefixed
location may reach anything under the root and nothing outside it; a
leading `/` names the source's root, never the host filesystem's.

A host adds named sources: `--source NAME=DIR` on every schema- and
data-reading verb, or `--alias NAME=DIR` on the data verbs — the
alias form, shown as `$NAME/<path>` in output and confined there
(`KDLITH-IMP007`); a malformed or colliding alias is rejected
(`KDLITH-IMP008`) before any document is read. An embedded host
registers in-memory trees through the Rust API instead
(`SourceTree::embedded`). Each name maps to exactly one tree: there
is no search path and no per-file fallthrough.

## Sources manifests and the store

A host may name a sources manifest (`--sources FILE`,
`ReadOptions::with_sources_manifest`), a `kdlith-sources` document
(`pkg/kdl/schemas/sources/sources.kdl`) declaring sources by name:

```kdl
sources {
    source design dir="../design-tokens"
    source acme {
        origin git="https://github.com/acme/map-shared" rev="3f2c9a1" path="map"
        pinned "sha256:9c1e…"
    }
}
```

A `dir` source is a directory relative to the manifest. An `origin`
source names where its tree comes from; kdlith never fetches it. The
tree is read from the store (`--store DIR`, `with_store`): at
`sha256/<hex>` when pinned, at `origin/<key>` when not, the key being
the SHA-256 of the origin's properties in name order, each written
`NAME=VALUE` and a newline (an empty `path` is left out). A missing
tree is `KDLITH-LOC006`, its help naming what to fetch and where;
`kdlith sources plan` lists every origin with its store path. A pin is
optional. A pinned tree is checked against its canonical tree digest
the first time a reading enters it, a mismatch being `KDLITH-LOC007`.
Each unpinned source a reading uses is a `KDLITH-LOC010` note carrying
its digest, the value to pin it by; a note never fails a run. A
manifest may not declare a name the host registers (`KDLITH-LOC005`).

The canonical tree digest: one line per regular file under the tree,
`<SHA-256 of the file's bytes, lowercase hex>  <path>` and a newline,
the path portable and relative to the tree's root, the lines in
ascending byte order of their paths; the digest is the SHA-256 of those
lines, written `sha256:<hex>`. A tree holding a link or a special file
has none. `sha256sum` reproduces it:
`find . -type f -printf '%P\n' | LC_ALL=C sort` over the tree, each
file's `sha256sum` and path as a line, then `sha256sum` of the lines.
A file asset's digest is the SHA-256 of its bytes, a directory asset's
its canonical tree digest.

## Overrides

`override NAME dir=DIR` replaces a declared source's tree with a
directory. It may be written in the manifest, in an overrides file
(`--overrides FILE`, `with_overrides_file`, an `overrides` node holding
only overrides), or on the command line (`--override NAME=DIR`,
`with_override`). The most local layer wins: command line, then
overrides file, then manifest. Every override is a `KDLITH-LOC009`
warning naming its layer, the identity it displaced, its directory, and
the lower-layer overrides it shadowed, so `--warnings-as-errors`
rejects any override, a committed one included. An override naming no
declared source, carrying a pin, or repeated in one layer is
`KDLITH-LOC005`.

## Assets

A field typed by an `asset` type holds a location too. It resolves by
the same spellings — relative to the referring document's directory,
or to its source's root under `base source`; `/path`; `$name/path` —
and stays inside its source (`KDLITH-LOC002`). The asset must exist
(`KDLITH-LOC001`) and match its type's kind and extensions
(`KDLITH-LOC008`); a pattern is admitted only where the type enables
it and only as one item of a list (`KDLITH-LOC004`), expanding there
in portable-path-byte order. Its content is never read, except to
record a digest where the type asks for one. The bound value is the
`asset` value kind: `{"location", "source", "digest"?}`, the location
shown as below and the source's name empty for the local source.

## Shown paths

Every resolved location is shown, and recorded on the `import` item,
in its normalised form: `$name/<path>` in a named source and
project-root relative in the local source — never the directory
backing a named source.

## Faults

A location leaving its source is `KDLITH-LOC002`, naming no document
is `KDLITH-LOC001`, an unregistered `$name` is `KDLITH-LOC003`,
syntax the governing locations rule does not admit is
`KDLITH-LOC004`, and an invalid root, source, manifest, or override is
`KDLITH-LOC005` before any document is read. An origin the store does
not hold is `KDLITH-LOC006` and a pin its tree does not match
`KDLITH-LOC007`; an override warns (`KDLITH-LOC009`) and an unpinned
source notes (`KDLITH-LOC010`). An import cycle is `KDLITH-IMP002` and
a repeat under `repeated=reject` is `KDLITH-IMP003`.

## The invocation bound

One reading invokes at most 1024 sources by default: entry documents
and import instantiations count against the same bound
(`KDLITH-SRC003`, configurable with `--max-invocations N`). Reading
entries in separate batches also stays under the bound, but each
batch then links its references on its own, so a reference that
crosses batches does not resolve.
