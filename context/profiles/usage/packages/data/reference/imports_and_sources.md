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

## Shown paths

Every resolved location is shown, and recorded on the `import` item,
in its normalised form: `$name/<path>` in a named source and
project-root relative in the local source — never the directory
backing a named source.

## Faults

A location leaving its source is `KDLITH-LOC002`, naming no document
is `KDLITH-LOC001`, an unregistered `$name` is `KDLITH-LOC003`,
syntax the governing locations rule does not admit is
`KDLITH-LOC004`, and an invalid root or source is `KDLITH-LOC005`
before any document is read. An import cycle is `KDLITH-IMP002` and
a repeat under `repeated=reject` is `KDLITH-IMP003`.

## The invocation bound

One reading invokes at most 1024 sources by default: entry documents
and import instantiations count against the same bound
(`KDLITH-SRC003`, configurable with `--max-invocations N`). Reading
entries in separate batches also stays under the bound, but each
batch then links its references on its own, so a reference that
crosses batches does not resolve.
