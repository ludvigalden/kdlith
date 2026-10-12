# Provenance: just-kdl

This directory is a vendored copy of the `just-kdl` crate sources.

| Field                  | Value                                                                                                                      |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Upstream repository    | https://github.com/1e1001/rsutil                                                                                           |
| Upstream path          | `just-kdl/src`                                                                                                             |
| Pinned commit          | `96d90ef64a72e07bbcceade010b9fc3a8d2d9a2e` (2026-09-04, "comment fix")                                                     |
| Upstream crate version | `just-kdl` 0.3.1                                                                                                           |
| License                | `MIT OR Apache-2.0` (rsutil workspace `Cargo.toml` `license` field; `SPDX-License-Identifier` headers in each source file) |
| License texts          | `LICENSE-MIT-upstream`, `LICENSE-APACHE-upstream` (copied verbatim from the rsutil repository root)                        |
| Copy date              | 2026-10-12                                                                                                                 |

## Byte-faithful scope

Every file under `just-kdl/src` at the pinned commit is copied with its
original name and relative path. The copy is byte-identical except for
the deviations listed below.

## Deviations

1. Crate-root paths: the sources are mounted as the module
   `kdlith_parse::justkdl` rather than as a crate root, so every
   `crate::` path in the non-test sources is rewritten to
   `crate::justkdl::` (`dom.rs`, `dom/iter.rs`, `dom/number.rs`,
   `lexer.rs`, `reader.rs`, `validator.rs`, `writer.rs`). No other
   source text changes.
2. Test modules (`tests.rs`, `tests/spec.rs`, `tests/gen-tests.rs`,
   `lexer/tests.rs`) are copied unmodified. They are slated for removal;
   kdlith verifies behavior through its fixture suite instead.

## Wiring outside this directory

- `pkg/rs/parse/src/lib.rs` declares `extern crate alloc;` at the crate
  root and mounts `justkdl/lib.rs` through `#[path]`, marked
  `#[rustfmt::skip]` so the upstream formatting is preserved.
- `pkg/rs/parse/Cargo.toml` adds the upstream dependencies `displaydoc`
  and `smol_str` (exact pins, `default-features = false`, as upstream),
  the upstream `std` feature (on by default), and `rust-version = "1.93"`
  because the sources use standard-library items stabilized up to 1.93.
