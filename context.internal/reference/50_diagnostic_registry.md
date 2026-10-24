# One code per rejected construct

A definition document is a data document of its dialect. A construct
the dialect's declarations reject therefore gets one diagnostic code
on every reading path: when a processor binds it (the item stream
`inspect` shows) and when the engine compiles it (`vet`, and
`inspect` after its item stream).

| Rejected construct                                                                                  | Code            |
| --------------------------------------------------------------------------------------------------- | --------------- |
| A node no binding accepts in its position, including a missing or extra root                        | `KDLITH-BND001` |
| A child that occurs more or fewer times than its rule allows, including the first-node rule         | `KDLITH-BND002` |
| A value that does not decode as its declared type                                                   | `KDLITH-BND003` |
| A required field or argument that is absent                                                         | `KDLITH-BND004` |
| An argument, property, or type annotation that no binding consumes                                  | `KDLITH-BND005` |
| A repeated key of a keyed collection                                                                | `KDLITH-BND006` |
| A value that names no member of its enum                                                            | `KDLITH-BND008` |
| A value-resolution decline at an absent computed-default field                                      | `KDLITH-BND009` |
| A map key its key type decodes to a value no map key holds                                          | `KDLITH-BND010` |
| A node breaking a `requires` or `all-or-none` constraint of a type it binds as                      | `KDLITH-BND011` |
| A number outside the inclusive range of a `bound`                                                   | `KDLITH-BND012` |
| A node breaking an `at-most-one` or `exactly-one-field` list                                        | `KDLITH-BND013` |
| A node holding a field the object type its case selects does not declare                            | `KDLITH-BND014` |
| A type expression that breaks the type-expression grammar                                           | `KDLITH-DLC007` |
| Two fields bound to the same argument, property, or rest arguments                                  | `KDLITH-SCH003` |
| An import that projects a slot the dialect does not declare, or any undeclared reference            | `KDLITH-SCH006` |
| An expansion or generic-instantiation cycle                                                         | `KDLITH-SCH009` |
| Variant prefixes of one union where one is a prefix of another                                      | `KDLITH-SCH010` |
| A schema that declares profiles while its invocation selects none, or names or lists an unknown one | `KDLITH-SCH011` |
| A profile whose `extends` chain returns to a profile already on it                                  | `KDLITH-SCH012` |
| A second profile of one schema marked `default`                                                     | `KDLITH-SCH013` |
| A node group declared with zero members                                                             | `KDLITH-SCH015` |
| A `when` condition gating on a property that is not a scalar or an enum                             | `KDLITH-SCH016` |
| Two `when` branches of one children rule not provably disjoint                                      | `KDLITH-SCH017` |
| A `when-all` or `when-any` combinator with no member conditions                                     | `KDLITH-SCH018` |
| A children rule carrying `when` branches without bounds of its own                                  | `KDLITH-SCH019` |

Distinctness rules: a bad type expression (`KDLITH-DLC007`) and an
unknown enum value (`KDLITH-BND008`) are distinct from a plain
value-type mismatch (`KDLITH-BND003`). A codec string failing its
match, a capture failing its declared type, and a value-bound union
matching no variant prefix are decode failures (`KDLITH-BND003`); a
node matching no case with no `otherwise` is unbound
(`KDLITH-BND001`). Every definition root states `version`; a root
without one is `KDLITH-BND004` on both reading paths. The remaining
definition codes (`SCH001`, `SCH002`, `SCH004`, `SCH005`, `SCH007`,
`SCH008`, `DLC005`, `DLC006`) come from linking, plan building, and
extension checks only the compiler runs.

## Rendering detail

`KDLITH-BND004` renders from one template, `<node> is missing
required <field>`, with child and compact spellings named in the
message (`expected child title VALUE`, `expected property title=VALUE
or child title VALUE`); a valueless child supplying nothing yields
both records, the child's first. An unbound node (`BND001`) carries a
note when it looks like a child written without braces: its
immediately preceding sibling was bound, that sibling's binding
admits the name as a child, and the node starts at a strictly greater
column; KDL indentation is not structure. Unknown nodes
(`BND001`), unbound properties (`BND005`), and unknown enum values
(`BND008`) carry a `did you mean` note for candidates within an edit
distance of 2 (Unicode scalars; distance, then name — deterministic),
and an unbound property whose name the node admits as a child first
carries a note saying to write it as that child. Every record-rendered
diagnostic ends with a `help:` line carrying its code's registered
correction; argument faults (`CLI001`–`CLI004`) precede any record
and carry none. Text on stderr is the only text presentation; the
machine-readable form is the JSON `diagnostic` item.

## Show, then fail

`inspect dialect` and `inspect schema` stream every item the reader
yields, then hand the same documents to the compiler exactly as `vet`
does, under complete checking, rendering the reader's diagnostics
followed by the compiler's; any error exits nonzero after the items
are shown, warnings alone exit 0. When the reader itself reports an
error the compile stage does not run — it would stop on the same
errors. A node-level check such as an overlapping binding sits at the
same span on both paths; the reader-path binder check fires first.

## Registry history

`KDLITH-SCH011` joined with the profile model; `KDLITH-DLC003` was
retired with the deferral marker. The location area (`LOC001`–
`LOC005`) retired `IMP001`, `IMP004`–`IMP008` (spent numbers in the
registry); `IMP002` (cycle) and `IMP003` (repeat) keep import-graph
semantics. `LOC008` joins with the `asset` type kind. `KDLITH-CTX006` subsumed the retired `CTX003`; the slot
malformed constructs stay `DLC005` (a layer name the slot does not
list is `SCH006`, or `DLC008` in a dialect definition). `BND009`
joins with the value-resolution hook; `BND010` with reference-keyed
maps (its link failure names the map field); `BND011`–`BND014` with
constraints, bounds, exclusivity, and case shapes; `SCH016`–`SCH019`
with conditional bounds (minted once per offending rule at compile).
The shipped resolver adds `DLC010` (a resolver refusal), leaving
`DLC006` the not-installed and no-policy cases.
