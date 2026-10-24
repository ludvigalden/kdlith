---
description: >-
  Authoring and reading data documents: records, declarations,
  contributions, and imports; the item stream; import sources and
  confinement; reference linking and scopes; export; and the cache
  and snapshot state.
binds_to_depth: main...
---

# Data documents

A data document is a KDL document authored against a schema: domain
declarations, controls, imports, and records. Reading one yields a
stream of processing items.

```kdl
import "cards.kdl"

set-region "north"
catalog {
    deck "Launch" mode=dark { label "core"; label "beta" }
}
```

Read on demand:

- what a reading yields: item kinds, stream order, and parentage —
  `reference/item_stream.md`
- import sources, locations, aliases, and the invocation bound —
  `reference/imports_and_sources.md`
- reference linking under `--scope local|complete`, the link codes,
  and the JSON diagnostics stream — `reference/linking.md`
- export selection, the provenance envelope, and sensitivity —
  `reference/export.md`
- `cache refresh` state and `read data --state` snapshot restore —
  `reference/state_and_cache.md`
