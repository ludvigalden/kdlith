# Provenance: kdlite

This directory is a vendored copy of the `kdlite` crate sources.

| Field                  | Value                                                                                                                  |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Upstream repository    | <https://github.com/ferronweb/kdlite>                                                                                  |
| Upstream path          | `src`                                                                                                                  |
| Pinned commit          | `3addb2a1320dcbc06e67d842e9a9fcb26441afbd` (2025-12-28, "chore: correct Dorian Niemiec's email address")               |
| Upstream crate version | `kdlite` 0.1.1                                                                                                         |
| License                | `MIT OR Apache-2.0` (`Cargo.toml` `license` field; README "License" section; `SPDX-License-Identifier` in each source) |
| License texts          | `LICENSE-MIT-upstream`, `LICENSE-APACHE-upstream` (copied verbatim from the kdlite repository root)                    |
| Copy date              | 2026-10-12                                                                                                             |

## Byte-faithful scope

Every file under `src` at the pinned commit (`dom.rs`, `lib.rs`,
`stream.rs`, `tests.rs`) is copied with its original name and relative
path. The copy is byte-identical except for the deviations listed below.

## Deviations

1. Crate-root paths: the sources are mounted as the module
   `kdlith_parse::kdlite` rather than as a crate root, so every
   `crate::` path in the non-test sources is rewritten to
   `crate::kdlite::` (`dom.rs` lines 12-13, `stream.rs` lines 18-19 and
   the doc link on line 1002). No other source text changes.
2. The test module (`tests.rs`, declared `#[cfg(test)] mod tests;` in
   `lib.rs`) is copied unmodified. It is slated for removal; kdlith
   verifies behavior through its fixture suite instead.

## Vendored-as-is lint findings

Because the copy is byte-faithful, repository lint findings on it are
expected until the sources are reorganized: upstream formatting (two-space
indentation under the upstream `rustfmt.toml`), the upstream module and
type doc comments that exceed the repository comment-length limits, the
upstream test module, and the indentation of the verbatim Apache license
text.

## Wiring outside this directory

- `pkg/rs/parse/src/lib.rs` mounts `kdlite/lib.rs` through `#[path]`,
  marked `#[rustfmt::skip]` so the upstream formatting is preserved.
- No dependencies are added: kdlite has no runtime dependencies. Its
  dev-dependencies (`kdl`, `shiba`) are not carried because upstream
  tests are not built here.
