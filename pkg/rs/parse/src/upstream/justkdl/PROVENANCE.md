# Provenance: just-kdl

This directory records where part of the `kdlith-parse` source came
from. It holds the upstream license texts; the upstream sources
themselves are no longer kept here.

| Field                  | Value                                                                                                                      |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Upstream repository    | <https://github.com/1e1001/rsutil>                                                                                         |
| Upstream path          | `just-kdl/src`                                                                                                             |
| Pinned commit          | `96d90ef64a72e07bbcceade010b9fc3a8d2d9a2e` (2026-09-04, "comment fix")                                                     |
| Upstream crate version | `just-kdl` 0.3.1                                                                                                           |
| License                | `MIT OR Apache-2.0` (rsutil workspace `Cargo.toml` `license` field; `SPDX-License-Identifier` headers in each source file) |
| License texts          | `LICENSE.MIT`, `LICENSE.APACHE` (copied verbatim from the rsutil repository root)                                          |
| Copy date              | 2026-10-12                                                                                                                 |

## History

1. The sources (`dom.rs`, `dom/iter.rs`, `dom/number.rs`, `lexer.rs`, `reader.rs`,
   `ssb2.rs`, `validator.rs`, `writer.rs`, `lib.rs`, and the upstream test
   modules (`tests.rs`, `tests/`, `lexer/tests.rs`)) were vendored
   byte-faithfully at the pinned commit, apart from `crate::` path
   rewrites needed to mount them as a submodule.
2. They were then reorganized into the `kdlith-parse` module tree and
   are no longer byte-faithful: the upstream test code was removed, the
   just-kdl and kdlite parsers were merged into one parser, and the
   code was rewritten to kdlith conventions (borrowed pull events,
   registered diagnostic codes, fail-closed resource limits). Which
   modules derive from this upstream is listed in `../../NOTICE.md`.

The pinned commit above identifies the exact upstream revision the
derived code started from.
