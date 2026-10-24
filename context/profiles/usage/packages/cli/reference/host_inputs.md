# Shared host inputs

Inputs that recur across the schema- and data-reading verbs.

## Schema and dialect

`--schema FILE` names the schema a data reading binds under. Without
it, the governing dialect reads the data. `--dialect FILE` on the
schema verbs names the dialect a schema is written in.

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
decides only the exit.

## Limits and resolvers

`--max-invocations N` bounds one reading's entries plus import
instantiations (default 1024, `KDLITH-SRC003`). `--resolve-from FILE`
registers the file-based value resolver on every data verb: a KDL
document of `value KEY VALUE` nodes supplying computed-default
values. Nothing else is ever read for values.

## Output policy

`export` takes `--emit roots|items...` selection, `--format ndjson`,
and the sensitivity policy: a `secret` field is emitted only when the
policy explicitly says emit, and is redacted otherwise.
`vet data --diagnostics json` writes each diagnostic as a `diagnostic`
item on standard output after every other item, so a host reads the
verdict without parsing stderr text.
