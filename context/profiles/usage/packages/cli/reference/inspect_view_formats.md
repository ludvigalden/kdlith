# Inspect view formats

Key tables for decoding each view's JSON lines. Sample lines for
every view are pinned in the committed fixture goldens under
`tests/fixtures/processing/inspect-view-*`.

## context

One line per entry document: `entry` (the path as given) and `slots`
listing every declared slot with `value` (the tagged encoding item
fields use; `null` when unset) and `layer` (`null` when the slot is
not layered). Only entry documents report; an imported document runs
under its import's projection. An entry replayed from the cache still
reports the context its cached reading left.

## effective on a definition target

One line per entry of the compiled definition. `section` is `type`,
`node`, `slot`, `import`, `profile`, or `annotation`, and `entry`
carries that section's keys:

| Section      | Entry keys                                                                                                                                   |
| ------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `type`       | `name`, `parameters`, `kind`; plus `expanded` only when the type's includes spliced members (one `{target, declared, included}` per include) |
| `node`       | `name`, `parents`, `role`, `type`, `profile`, plus `root`, `description`, and `annotations` when the declaration carries them                |
| `slot`       | `name`, `type` (as written), `namespace`, `accumulate`, `duplicates`, `layers`                                                               |
| `import`     | `path`, `project`, `repeated` (kebab-case), `namespace`                                                                                      |
| `profile`    | `name`, `roots`, `locations` (the profile's own rule, or null)                                                                               |
| `annotation` | `name`, `type` (as written), `required`                                                                                                      |

On data, `effective` writes the stream's `declaration` and
`resolution` items unchanged: the declarations as imported, then the
values the contribution resolver settled (each carrying the target,
the resolved value, the layers that reached it, and its scope).

## plan

One `plan-context` line (slots with their types, namespaces,
accumulation; namespaces in declaration order), then one `plan-node`
line per node declaration carrying `name`, `role`, `scope`,
`children-effects` and `following-effects`, `context-reads`
(`field`, `slot`, `policy`), `deferred` (one entry per reference a
field's type holds, at any depth), `dependent` (`target`, `member`,
`codec`), `declaration`, and `requires`. An effect is `set` (with
`field` or `value`, and `layer`), `remove`, `clear`, or `handler`.
`requires` follows a fixed order:

| Flag                         | Present when                                                                                                            |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `record-buffering`           | the node defers a contribution, or an effect sets a slot that accumulates                                               |
| `scope-completion`           | the scope runs checks when it closes (minimums, `when` branches, exactly-one, first child, sequence, descendant bounds) |
| `declaration-lookup`         | the node registers a declaration                                                                                        |
| `imported-schema-metadata`   | the definition imports documents (marks every node)                                                                     |
| `deferred-binding`           | the node defers a reference                                                                                             |
| `global-validation`          | the definition declares a document rule (marks every node)                                                              |
| `registered-domain-handlers` | one of the node's effects is a registered handler                                                                       |

## references

One line per declared reference type, in declaration order, with
`name`, `domain`, `qualification` (`explicit`,
`explicit-or-context`, `imported-or-context`), and `links`. Each
link carries `written`, `domain`, `name` (segments, outermost
first), `qualifiers`, `context`, `field` (or null for a
contribution's target), `key` (true only for a map key), `site`
(source path and span), `candidates` (each with identity, namespace,
labels, site), `gated` (the namespaces that hid a candidate), and
`outcome`: `resolved`, `unresolved`, `ambiguous`, or
`unmet-qualifier` (which adds `reasons` with `qualifier`,
`spelled`, `declared`). A duplicate declaration is reported only on
standard error.
