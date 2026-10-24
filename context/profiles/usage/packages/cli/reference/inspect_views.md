# Inspect views

`inspect dialect`, `inspect schema`, and `inspect data` take
`--view`, which picks what standard output carries. Every view writes
JSON lines in the tagged envelope the item stream already uses,
`{"item":"<kind>","data":{…}}`, so one decoder reads all of them.
Diagnostics always go to standard error.

## Applicability

| View         | dialect          | schema           | data                         |
| ------------ | ---------------- | ---------------- | ---------------------------- |
| `source`     | item stream      | item stream      | item stream                  |
| `bound`      | `KDLITH-CLI004`  | `KDLITH-CLI004`  | data items                   |
| `context`    | `KDLITH-CLI004`  | `KDLITH-CLI004`  | entry contexts               |
| `effective`  | compiled entries | compiled entries | declarations and resolutions |
| `origins`    | `KDLITH-CLI004`  | `KDLITH-CLI004`  | provenance envelopes         |
| `plan`       | processing plan  | processing plan  | the bound definition's plan  |
| `references` | `KDLITH-CLI004`  | `KDLITH-CLI004`  | linked references            |

`bound`, `context`, `origins`, and `references` describe how data
binds and links; a definition target binds no data, so asking for one
of them there is an argument fault (exit 2, `KDLITH-CLI004`).

## What each view writes

- **`bound`** writes the stream's `record`, `declaration`,
  `contribution`, and `pending-binding` items unchanged; document,
  import, scope, resolution, and diagnostic items are dropped.
- **`context`** writes one `context` line per entry document after
  the read, listing every declared slot with the value and layer the
  entry's root level left (`"value":null` for an unset slot).
- **`effective`** on a definition target writes one line per entry of
  the compiled definition, sectioned `type`, `node`, `slot`,
  `import`, `profile`, and `annotation`. On data it writes the
  stream's `declaration` and `resolution` items: the declarations as
  imported, then the values the contribution resolver settled.
- **`origins`** writes one `origins` line per record, declaration,
  and contribution, carrying the same provenance envelope `export`
  writes beside that item: the item's node, the source revisions, the
  shared origin table (`literal`, `derived`, or `resolved`), and each
  field's index into it.
- **`plan`** writes one `plan-context` line, then one `plan-node`
  line per node declaration — its effects, context reads, deferred
  references, dependent bindings, and the `requires` flags stating
  what processing the node needs. On data it renders the bound
  definition's plan; the data is still read and decides the exit.
- **`references`** writes the reading's link report: one line per
  declared reference type with its qualification policy, and each
  link's written spelling, site, candidates, gated namespaces, and
  outcome (`resolved`, `unresolved`, `ambiguous`, `unmet-qualifier`).

Sample lines for every view are pinned in the committed fixture
goldens under `tests/fixtures/processing/inspect-view-*`.

## Ordering

Output order is chosen, never hash-map iteration: stream views keep
the reader's order; `context` follows entry-document order;
`effective` on a definition sorts by section, then name; slots appear
in declaration order; `plan-node` lines follow declaration order;
`requires` flags follow a fixed order; reference-type lines follow
declaration order with links in stream order.

## When reading fails

Every view keeps show-then-fail: stream views write every item the
reader yields, then exit 1 on error. Views rendered from the compiled
definition write nothing when the definition did not compile, report
the load diagnostics, and add one line saying why the view is missing.
`references` prints the report of any reading that completed, even
one with binding errors, then exits on those errors.
