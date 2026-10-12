# Provenance: kdlite

This directory records where part of the `kdlith-parse` source came
from. It holds the upstream license texts; the upstream sources
themselves are no longer kept here.

| Field                  | Value                                                                                                                  |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Upstream repository    | <https://github.com/ferronweb/kdlite>                                                                                  |
| Upstream path          | `src`                                                                                                                  |
| Pinned commit          | `3addb2a1320dcbc06e67d842e9a9fcb26441afbd` (2025-12-28, "chore: correct Dorian Niemiec's email address")               |
| Upstream crate version | `kdlite` 0.1.1                                                                                                         |
| License                | `MIT OR Apache-2.0` (`Cargo.toml` `license` field; README "License" section; `SPDX-License-Identifier` in each source) |
| License texts          | `LICENSE.MIT`, `LICENSE.APACHE` (copied verbatim from the kdlite repository root)                                      |
| Copy date              | 2026-10-12                                                                                                             |

## History

1. The sources (`dom.rs`, `lib.rs`, `stream.rs`, and the upstream test module
   (`tests.rs`)) were vendored
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
