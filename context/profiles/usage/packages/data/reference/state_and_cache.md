# Cache and snapshot state

## cache refresh

`kdlith cache refresh --state DIR --schema S entries...` writes a
state directory holding two owned classes: cached bound results, and
one `<key>.kdlith-snapshot` per error-free entry document. The
snapshot key is the entry as spelled, its revision, and its profile
selection; the metadata line carries the schema fingerprint and the
lineage (the entry chain as spelled), both checked at restore. The
snapshot indexes every record the entry's reading bound, each with
the context it bound under.

Both classes record the named sources the reading used. A cached
bound result is keyed under the source-set fingerprint as well as its
invocation identity, so one document read against two trees of one
source name keys two entries, and the refresh that follows a change
to a source removes the old one as stale. A snapshot's metadata line
lists each named source with the identity of the tree it read. A
reading that used no named source keys exactly as before.

A reading that consulted the value resolver writes no snapshot, so
no resolved secret reaches the store; a document for which a secret
was resolved gets no cache entry either.

## read data --state

`read data --state DIR --schema S --entry E record.kdl` restores the
requested record from the store instead of replaying: the snapshot's
context is restored and only the record's span binds, at the node
index the scan recorded. The `--entry` document still binds as
replay does — its imports number the read document's invocation —
and the output equals replay's.

- **Miss** — no `--state`, an absent directory, a changed read
  document, a store indexing no such record, or a snapshot recording
  a named source whose tree now has another identity (its content
  changed, or an override now replaces it): the reading replays.
- **Mismatch** — a snapshot captured under another schema or another
  entry chain is rejected before anything binds
  (`KDLITH-CTX004`, exit 1), never restored best-effort.

A read never writes the store. An owned entry the read cannot use is
`KDLITH-CAC002`.
