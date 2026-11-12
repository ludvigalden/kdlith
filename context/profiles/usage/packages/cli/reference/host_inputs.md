# Shared host inputs

Inputs that recur across the schema- and data-reading verbs.

## Schema and meta-schema

`--schema FILE` names the schema a data reading binds under. Without
it, the governing meta-schema reads the data. `--meta-schema FILE`
replaces the bundled meta-schema a schema is written in; its root
must be `schema schema`.

## Root and sources

`--root DIR` (also spelled `--match-root`) names the project root on
every verb that reads a schema or data: the current directory by
default. Unprefixed import paths resolve inside the root and never
leave it. `--source NAME=DIR` registers a named source tree so
`$name/path` imports resolve inside it. `--alias NAME=DIR` (data
verbs) is the host import alias form of a named source: a duplicate,
anchor-colliding, malformed, or unresolvable alias is rejected
(`KDLITH-IMP008`) before any document is read.

`--sources FILE` names a sources manifest, `--store DIR` the store its
origin sources are read from (kdlith never fetches), `--overrides FILE`
an overrides file, and `--override NAME=DIR` (repeatable) replaces a
declared source on the command line; each override is a
`KDLITH-LOC009` warning. `--warnings-as-errors` fails the run on any
warning; a note, such as an unpinned source's `KDLITH-LOC010`, never
fails it.

## Profiles and scope

`--profiles NAME[,NAME...]` names a profile set; `--profile NAME`
names one profile explicitly. `--scope local|complete` (default
`complete`) on `vet data` and `inspect data` gates whether link
diagnostics fail the run: every view renders first, and the scope
decides only the exit. On `read data` the same flag instead selects the
partition the lookup consults and also admits a scope name (see the
verbs reference).

## Limits, threads, and resolvers

`--max-invocations N` bounds one reading's entries plus import
instantiations (default 1024, `KDLITH-SRC003`). `--threads N` binds a
data reading on N threads in place of one: the documents an import
fans out to are bound ahead of their turn and their items given in
the same order, so the output never depends on the count (see the
data package's `reference/large_trees.md` for when it pays off).
`--resolve-from FILE`
registers the file-based value resolver on every data verb: a KDL
document of `value KEY VALUE` nodes supplying computed-default
values. Nothing else is ever read for values.

## Output policy

`export` takes `--emit roots|items...` selection, `--format ndjson`,
and the sensitivity policy: a `secret` field is emitted only when the
policy explicitly says emit, and is redacted otherwise. `--lazy`
binds only the nodes the emitted type can come from (see the data
package's `reference/export.md`).
`vet data --diagnostics json` writes each diagnostic as a `diagnostic`
item on standard output after every other item, so a host reads the
verdict without parsing stderr text.
