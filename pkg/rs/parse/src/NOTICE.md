# Attribution

`kdlith-parse` derives from two upstream KDL parsers. Both are licensed
`MIT OR Apache-2.0`. kdlith redistributes the derived code under the
MIT option and keeps both upstream license texts beside each provenance
record.

| Upstream                                                            | Repository                                          | Pinned commit                              | Record              |
| ------------------------------------------------------------------- | --------------------------------------------------- | ------------------------------------------ | ------------------- |
| just-kdl 0.3.1 (Copyright (c) 2026 1e1001)                          | <https://github.com/1e1001/rsutil> (`just-kdl/src`) | `96d90ef64a72e07bbcceade010b9fc3a8d2d9a2e` | `upstream/justkdl/` |
| kdlite 0.1.1 (Copyright (c) 2025 1e1001, Copyright (c) 2025 Ferron) | <https://github.com/ferronweb/kdlite> (`src`)       | `3addb2a1320dcbc06e67d842e9a9fcb26441afbd` | `upstream/kdlite/`  |

Both sources were first vendored byte-faithfully at the pinned commits.
They were then merged and reorganized into this module tree, and are no
longer byte-faithful.

| Module                                         | Derived from                                                                                                                     |
| ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `chars.rs`                                     | kdlite `stream.rs` code-point classes; just-kdl `lexer.rs` UTF-8 classes                                                         |
| `number.rs`                                    | just-kdl `dom/number.rs` number state machine                                                                                    |
| `scan.rs`                                      | kdlite `stream.rs` grammar productions (whitespace, comments, escapes, strings); just-kdl `lexer.rs` multi-line string dedenting |
| `parse.rs`                                     | kdlite `stream.rs` parser state machine; just-kdl `reader.rs` slashdash and children-block handling                              |
| `event.rs`                                     | just-kdl `dom.rs` event and value shapes; kdlite `stream.rs` borrowed events                                                     |
| `coords.rs`, `error.rs`, `limits.rs`, `lib.rs` | kdlith                                                                                                                           |
