# Layout

`kdlith fmt` writes every document in one canonical layout. The layout
depends only on what a document holds — its nodes, values, and
comments, where its empty lines fell, and which strings were written
bare — never on how it was otherwise spaced, wrapped, quoted, or
escaped. Formatting never changes a value, so a formatted document
reads exactly as it did.

## The canonical layout

- One node per line, four spaces per level; `;`-joined nodes take a
  line each.
- One space between entries; `key=value` with no spaces; a type
  annotation directly before what it annotates: `size=(u8)4`.
- A node name, property key, or type annotation is written bare when
  the bare spelling reads back as itself, quoted otherwise.
- A string value written bare stays bare. Any other string is written
  quoted, or raw when it holds a `\` or a `"` that a raw string spells
  without escapes (`#"^\d+$"#`). A quoted string escapes what it would
  otherwise hold invisibly: tabs, control code points, and spaces other
  than U+0020.
- Numbers keep their spelling (`0xFF`, `1_000`, `1.5e10`); keywords are
  `#true`, `#false`, `#null`, `#inf`, `#-inf`, `#nan`.
- An empty children block is `{}`; slashdashed nodes, entries, and
  blocks are written like any other, after `/-`.
- At most one empty line separates two items; none opens or closes a
  block or the document. The document ends in one newline.
- Comments stay where they were written: a line's trailing comment
  after one space, a comment on a block's `{` line on that line, a
  `/* */` comment before a node on the node's line. A multi-line block
  comment keeps its text, its lines moved as far as its first line
  moved. A comment written inside a node's head (after `/-`, inside a
  type annotation) moves to stand before that node or entry.

## Line limits

Two limits bound every line: the **width**, the column no line runs
past (default 100), and the **content width**, the most columns a
line holds after its indentation (default 80). A line at indentation
`i` ends by column `min(width, i + content-width)` wherever a break
can fall.

A node header that outgrows its line is filled onto continuation
lines, each ending in `\` and indented two levels past the node, so a
continuation never reads as a child:

```kdl
includes as-child component-elements-bodies component-marker-list \
        content-contract content-render-hooks
```

A string too long for any line is written as a multi-line string, one
level in. Its value is unchanged: a line breaks at a space through a
whitespace escape (`\` at the end of the line), which reads back as
nothing.

```kdl
detail """
    decide used to exit 1 both when its decisions held an error and when it could \
    not decide at all.
    """
```

A string holding a newline is always a multi-line string, and one that
holds none is always single-line. Trailing and whitespace-only text on
a line is escaped (`\s`, `\t`), so no editor trims it. A paragraph of
one line is broken to fit; a paragraph the author broke over several
lines keeps its lines, since no word can cross a line break without
changing the value. A value holding a `\`, and one with no space to
break at, is never broken.

## Schema layout rules

`kdlith fmt schema` and `kdlith fmt data --schema SCHEMA` follow the
schema's `layout` rules:

```kdl
schema shop version="1" {
    layout width=100 content-width=72
    types {
        string Note {
            layout prose
        }
        string Command {
            layout verbatim
        }
    }
    node item role=record type=Item {
        layout order=declared
        // ...
    }
}
```

- `layout width=N content-width=N order=declared|authored` on the
  schema root holds for every document the schema reads; on a node
  declaration, for that node and everything under it, until a rule
  further down says otherwise. Each property is optional, and a
  command line's `--width` or `--content-width` overrides every rule's.
- `order=declared`, the default, writes the node's arguments first,
  then its properties in its declaration's field order, and the
  children its fields bind in the same order; same-named children keep
  their order. Only an order no reading can observe changes: a child
  that only a `children` rule admits, or one that is a processing item
  of its own (it declares a role), stays where it is and nothing moves
  across it; nothing moves across an empty line, which closes a group
  the author made; and a node with a `sequence` keeps its children as
  written. `order=authored` keeps everything as written, for a node
  whose author's order reads better than its fields'. To change the
  declared order itself, reorder the declaration's `field` bindings.
- A string type's `layout prose` declares its values paragraphs:
  paragraphs are separated by empty lines, and within one every run of
  whitespace, line breaks included, reads as a single space. A reading
  folds a prose value so, and `fmt` refills it: one line when it fits,
  else a multi-line string whose lines break at spaces outright, with
  no escapes. A prose value reads the same before and after.
- A string type's `layout verbatim` declares values that are never
  broken over lines they do not already hold, such as commands.

By convention, and with no rule to write, a value a field of an enum
type or of a `kebab-case`, `camel-case`, `lower-snake`, or
`upper-snake` string type binds is written bare wherever the bare
spelling reads back as the same string.
