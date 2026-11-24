# Formatter contract

`kdlith fmt` writes a document in one canonical layout. The consumer
description of the layout is the usage module's
`authoring/reference/layout.md`; this file records what the formatter
guarantees, how it decides, and where each decision lives.

## Guarantees

- **Canonical.** The output depends only on the document's meaning
  (nodes, entries, values, slashdashed text), its comments, where its
  empty lines fell, and which strings were written bare. Spacing,
  wrapping, quoting, escaping, raw-ness, multi-line-ness, `;`, line
  continuations, and empty-line runs are all decided by the layout.
- **Value-preserving.** No value changes. A long string is broken only
  through whitespace escapes, which read back as nothing; the one
  exception is a value the schema declares prose, which a reading
  folds, so the folded value is what is preserved.
- **Idempotent.** Formatting formatted text changes nothing.
- **Fail-closed.** Every result is read back before it is returned:
  its syntax tree, under the same policy, must hold the same token
  stream (nodes, entries, values, slashdash flags, block structure,
  and the comment texts, each line trimmed) as the document the
  policy made; and when the policy rewrote nothing, the event
  parser, which shares no code with the tree, must read the same
  event stream from both texts. A failure is `KDLITH-FMT002` and
  nothing is written. A file is replaced by writing beside it and
  renaming over it.

## Where it lives

- `kdlith-parse::tree` reads a document into a `Document` of `Block`s,
  `Item`s (nodes and comments, each knowing whether an empty line
  precedes it and whether it shares the line before it), and `Node`s
  whose `parts` keep entries, comments, and children blocks
  (slashdashed ones included) in source order, each value with its
  `Spelling`. It drives the same `Scanner` productions as the pull
  parser, recording the trivia the pull parser skips, so it accepts
  exactly the same documents with the same errors; a comment written
  inside a node's head moves to `Node::leading` or to the part before
  its entry, never lost.
- `kdlith-format` holds the layout (`render.rs`), the string spellings
  (`strings.rs`), the schema policy and the rewrites it makes
  (`policy.rs`), and the read-back check (`verify.rs`). It knows no
  schema.
- `kdlith-cli`'s `fmt.rs` expands arguments, formats files on as many
  threads as the machine offers (each outcome in its file's place, so
  output order never depends on scheduling), and reports;
  `layout.rs` derives the `Policy` from a compiled `Definition`.

## Layout decisions

- A node header is laid out on one line when its entries and the
  block's ` {` fit within `min(width, indent + content-width)`.
  Otherwise its entries fill lines greedily; a continuation line ends
  the previous one with ` \` and starts two levels past the node, so
  it never reads as a child. A `//` comment inside a header keeps its
  place through `\ // comment`; one after the last entry and before a
  block moves to the block's `{` line.
- A string that cannot fit becomes a multi-line string when it is
  wrappable — it holds a breakable space and no `\` — and either it
  is the first entry on its line or it would not fit on a line of its
  own; otherwise it moves to the next continuation line. A multi-line
  string's body sits one level past the line it opens on.
- Within a multi-line string, a paragraph of one line breaks at the
  furthest space that leaves room for the `\`; a paragraph of several
  lines keeps its lines, since words cannot cross a line break without
  changing the value. Prose instead refills each paragraph at spaces,
  dropping the space at each break.
- A raw string is chosen only for a value holding a `\` or a `"` that
  a raw string can hold as written, never one starting with `"`
  (whose raw spelling would open a multi-line string), and never for
  a value holding a tab or a code point a quoted string escapes.

## The schema policy

`layout.rs` builds one `NodeRule` per node declaration, matched as the
binder dispatches: by node name and parent name. Children a field
binds through nested `child`/`children` sources with no declaration of
their own get a rule from those sources. A rule carries the `layout`
limits and order, the property ranks (field order; a compact child
counts as a property), the child ranks (field order, then children
rules), and a value rule per argument position and property: bare
for an enum or a case-format string, and the type's text layout.

Order is `authored` unless a `layout` rule says `declared`. Across
the repositories formatted while this was built, a declaration's field
order follows its type, not the order its documents are read in, so
reordering by default rewrote correct documents. Under `declared`, a
child that declares a role ranks nowhere: it is emitted, applied, or
imported in written order, so it stays in place and nothing moves
across it, and a node with a `sequence` keeps its children as written.

Prose folding is the one rule that reaches a reading: the engine's
decoder folds a value of a `layout prose` string type
(`types::fold_prose`), and `kdlith-format` folds with the same
algorithm, so a refilled value reads exactly as before.

## Verification on the attached repositories

The formatter was run over every KDL file of the repositories it was
built beside (about 7,700 files and a million lines): every file that
parses formats and reads back, a second pass changes nothing, and the
tree parser reports exactly the errors the pull parser reports on the
invalid ones and on the KDL v2 suite. The schema forms ran the same
way over each repository's schemas and data under their own schemas.
