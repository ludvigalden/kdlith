# Linking

`vet data`, `inspect data`, and `export` take `--scope local|complete`
(default `complete`). Under complete scope the reading is linked
after it streams: every reference held by a field typed with a
declared reference type — including each key of a map whose key type
is one — is looked up among the reading's declarations under that
reference type's qualification policy.

## Link codes

| Code            | Failure                                                                     |
| --------------- | --------------------------------------------------------------------------- |
| `KDLITH-REF001` | no declaration matches; names the alias namespace when gating hid the match |
| `KDLITH-REF002` | two declarations compose one identity in one domain                         |
| `KDLITH-REF003` | the policy derives more than one candidate (ambiguous)                      |
| `KDLITH-REF004` | the one candidate fails a required qualifier                                |
| `KDLITH-REF005` | a pending payload does not fit the declaration its reference resolved to    |

Aliased imports gate visibility: a document imported `as=NAME`
resolves its own declarations unqualified, while every other document
reaches them only under the alias.

## Scope behavior

The scope gates the exit only — every view renders first. Linking
runs after the whole reading has streamed, so every item, and on
`inspect data` every view, prints before the link diagnostics decide
the exit on standard error. Under local scope the `references` view
still links and prints the same report, but link diagnostics do not
decide the exit.

`export` links whatever the scope: under complete scope a link
diagnostic fails the run (atomic delivery publishes nothing;
provisional delivery keeps what it wrote with an `KDLITH-EXP002`
warning), and under local scope every selected item exports with its
references as written.

## JSON diagnostics

`vet data --diagnostics json` writes each link diagnostic to
standard output as a `diagnostic` item, after every other item and in
the order standard error lists them, so a host reads the verdict from
the stream without parsing text. Binding diagnostics already stream
in place.
