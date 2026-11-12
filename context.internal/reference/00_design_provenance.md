# Design provenance

Every mechanism in the meta-schema traces to the kdlith design drafts
that preceded the committed record. Those drafts are not part of the
repository; pointers name the draft round and section so the
reasoning can be found again:

| Pointer | Draft round                                                                                                 |
| ------- | ----------------------------------------------------------------------------------------------------------- |
| R1 §n   | Streaming framework: five components, bindings, context transitions, imports, pull parsing                  |
| R2 §n   | Schema expressions: type expressions, bindings, codecs, defaults, context expressions, plans                |
| R3 §n   | Roles and declarations: roles, profiles, ordered maps, references, providers, contributions, import sources |
| F §n    | Framework and repository layout: authoring levels, schema-kdl, fixtures                                     |

The committed meta-schema documents under
`pkg/kdl/schemas/schema/` are the only authority for the meta-schema.
This tree describes them; where the files and any description here
disagree, the files win and the description is out of date. The
committed census `tests/fixtures/COVERAGE.md` maps every mechanism
row to its covering scenarios.
