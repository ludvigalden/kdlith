---
description: >-
  Driving a generated typed reader from Rust: generating the crate,
  the reader constructors and builders, items and provenance,
  linking and reference resolution, value resolution, and loading
  schemas held in memory.
binds_to_depth: main...
---

# The typed reader

`kdlith gen schema --crate NAME --out DIR schema.kdl` writes a Rust
crate whose typed reader runs the same data reading `vet data`
performs — imports, profiles, handlers, output policy, and the
bound-result cache are decided by the engine's one data reader. A
tool depends on the generated crate alone: it re-exports the
reading, linking, and diagnostic surface.

Read on demand:

- generating crates, ownership, drift checking, and packaging —
  `reference/generated_crates.md`
- the reader constructors, host-input builders, and in-memory
  sources — `reference/reader_usage.md`
- items, variants, `site` / `origin` provenance, and slot capture —
  `reference/provenance.md`
- `.link`, `collect`, `resolve`, and `settled` —
  `reference/linking_api.md`
- the value-resolution hook and `--resolve-from` —
  `reference/value_resolution.md`
