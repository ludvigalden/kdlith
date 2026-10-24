# Snapshot restore

`cache refresh` writes, beside the bound results it caches, one
snapshot entry per error-free entry document into the same state
directory: `<key>.kdlith-snapshot`, headed `kdlith-snapshot format=2
canonicalization=2`, whose metadata line carries the entry as
spelled, its revision, the schema fingerprint, the profile selection,
and the lineage, followed by one index line per record the entry's
reading bound, with the context it bound under. The key hashes the
entry as spelled, its revision, and its profile selection only. The
lineage is the invocation's entry documents as spelled, up to and
including the snapshot's own, because the entries before a document
number its invocations. A reading that consulted the value resolver
writes no snapshot, so no resolved secret reaches the store.

`read data --state DIR` resolves the record it is asked for from
that store instead of replaying the read document:

- **Hit.** The snapshot keyed by the read document (as spelled, at
  its current revision, under its profile selection) indexes the
  record. Its context is restored, and only the record's span binds
  under it, at the node index the scan recorded. Nothing else of the
  read document binds, and no context crosses between documents. The
  `--entry` document still binds as replay does, its diagnostics
  reported and its items not printed, because its imports number the
  read document's invocation and so the record's source id. The
  output equals replay's.
- **Fallback.** No `--state`, an absent directory, a changed read
  document (its revision keys no snapshot), or a store indexing no
  such record replays the documents as `read data` always does. A
  record an import reached is not restored and replays.
- **Mismatch.** A snapshot captured under another schema, or under
  another entry chain (the read document read without the `--entry`
  it was scanned after, for example), is rejected before anything
  binds (`KDLITH-CTX004`, exit 1), never restored best-effort.

A read never writes the store. An owned entry the read cannot use is
`KDLITH-CAC002`.
