# Fixture suite coverage map

This file is the census of record for the committed fixture suite: which
dialect constructs, diagnostic codes, and CLI verb contracts the suite
exercises, and which gaps are declared elisions rather than silently
uncovered. The suite itself lives in this directory and runs under the
`kdlith-fixtures` harness (see `README.md`).

## Maintenance rule

The map is hand-maintained, and two invariants keep it honest:

1. Every scenario name written in this file must appear in the output of
   `kdlith-fixtures --fx-list` for the committed suite. A name that names
   no real scenario is a bug in this file.
2. Every non-parse scenario in `--fx-list` output must appear somewhere in
   the census tables below. `parse/kdl-spec` (243 scenarios, one per
   upstream KDL v2 positive case) is covered as a single row: the official
   KDL v2 suite, traced in `parse/kdl-spec/PROVENANCE.md`. The upstream
   negative cases live one-per-scenario under `invalid/kdl-spec/` and are
   named individually in the code census.

Adding or renaming a scenario means updating this file in the same commit.
The census is re-checkable by hand:

```sh
target/debug/kdlith-fixtures --fx-suite tests/fixtures --fx-kdlith target/debug/kdlith --fx-list
```

then diffing the listed names (all except `parse/kdl-spec/*`) against the
scenario names occurring in this file, in both directions.

Suite state at the time of writing: 550 scenarios — 243 `parse/kdl-spec`,
307 others — all green.

## Construct census

One row per in-scope mechanism row of the design record
(`docs/dialect.md`). "Defining file" is the canonical dialect document
that declares the mechanism.

### Logical types (`pkg/kdl/schemas/dialect/types.kdl`)

| Mechanism | Defining file | Covering scenarios | Notes |
| --- | --- | --- | --- |
| Logical types separate from bindings | types.kdl | schemas/inspect/constructs, schemas/inspect/shelf, schemas/vet/catalog | The separation is what every type-then-binding scenario exercises |
| Type declarations (`string`/`enum`/`object`/`union`/`map`) | types.kdl | schemas/inspect/constructs, schemas/inspect/shelf, schemas/vet/catalog, dialect/canonical-vet/merged | Includes contributing-document merge |
| Type expressions | types.kdl | schemas/inspect/constructs, invalid/schemas/bad-type-expression, invalid/schemas/inspect-bad-type-expression | Grammar negatives on both reading paths |
| Type constructors (`optional`/`nullable`/`list`/`map`/`ordered-map`/`union`/`ref`) | types.kdl | schemas/inspect/shelf, schemas/vet/catalog, processing/bind/nullable, processing/bind/value-expr, invalid/schemas/ref-without-declaration | `nullable<T>` and `ValueExpr` positives landed with the gap fill |
| Ordered keyed collections | types.kdl | schemas/inspect/shelf, schemas/vet/catalog | Unique keys, declaration order retained |
| Built-in scalars (incl. `U64`, `ValueExpr`) | types.kdl | schemas/inspect/shelf, processing/bind/value-expr | |
| Patterned strings (`pattern`/`min-length`/`format`/`validator`) | types.kdl | schemas/inspect/constructs, schemas/inspect/shelf, invalid/schemas/pattern-invalid, invalid/schemas/unknown-format, invalid/schemas/name-format, invalid/schemas/inspect-name-format, processing/inspect-accepts-malformed-pattern | |
| Enum member metadata | types.kdl | schemas/inspect/constructs | Descriptions survive compilation |
| Discriminated unions by node name | types.kdl | schemas/inspect/shelf, invalid/data/union-variant | |
| Maps (`key-type`/`value-type`) | types.kdl | schemas/vet/catalog, schemas/inspect/constructs | |
| Documented default versus fallback | types.kdl | schemas/inspect/constructs, schemas/inspect/shelf, processing/bind/defaults, invalid/schemas/documented-default-type, invalid/schemas/fallback-type, processing/inspect-accepts-schema-literal | Literal-decode negatives are SCH008 |
| Object constraints (`all-or-none`/`requires`) | types.kdl | schemas/inspect/constructs | |
| Generic type declarations (`parameter` children, `T<A, B>` instantiation) | types.kdl | processing/bind/generic-types, invalid/schemas/generic-arity-mismatch, invalid/schemas/instantiation-cycle | Monomorphization at compile time; the bind positive proves the instantiated declaration through case-selected retyping, the only CLI-reachable path for a monomorphized object type (the `TypeName` pattern blocks `node type=` spellings of the mangled name) |
| Pattern codecs (`codec`/`match`/`capture`/`namespace`) | types.kdl | processing/bind/codecs, invalid/schemas/codec-pattern-invalid, invalid/schemas/codec-capture-unknown-type, invalid/data/codec-match-failure | Whole-string match, typed captures decoded at bind time |
| Union dispatch by value prefix (`prefix=`) | types.kdl | processing/bind/union-dispatch, invalid/schemas/prefix-overlap, invalid/data/union-prefix-miss | Single probe, no fallback; overlapping prefixes are SCH010 |
| Field sensitivity (`sensitivity=secret`/`reference`) | types.kdl | processing/bind/sensitivity, invalid/schemas/sensitivity-inline | Redact-by-default for `secret`; inline field bindings cannot declare it |

### Bindings, roles, and cardinality (`pkg/kdl/schemas/dialect/bindings.kdl`)

| Mechanism | Defining file | Covering scenarios | Notes |
| --- | --- | --- | --- |
| Roles vocabulary (all six roles) | bindings.kdl | schemas/vet/catalog, schemas/inspect/shelf, invalid/schemas/unknown-role, invalid/schemas/inspect-unknown-role | record, declaration, contribution, context-control, source-import, container |
| Node declarations | bindings.kdl | dialect/canonical-vet/merged, schemas/vet/catalog, invalid/schemas/unknown-node | `role=`, `type=`, `parent=`/`parents` |
| Parent-sensitive dispatch | bindings.kdl | schemas/inspect/shelf, schemas/vet/catalog, invalid/schemas/unknown-parent | Same name under different parents |
| Binding shorthands (`arg=`/`property=`/`child=`/`context=`) | bindings.kdl | schemas/inspect/shelf | |
| Explicit `from` sources | bindings.kdl | schemas/inspect/shelf | `argument`/`args`/`property`/`child`/`children`/`entries`/`context` |
| Collection bindings (`args`/`children`) | bindings.kdl | schemas/inspect/shelf | |
| Entries envelope | bindings.kdl | schemas/inspect/shelf, invalid/data/entries-key | Key consumed exactly once |
| Multiple spellings and collision reject | bindings.kdl | invalid/data/collision | |
| Duplicate keys reject | bindings.kdl | invalid/data/duplicate-key, invalid/schemas/duplicate-binding | |
| Compile-time binding checks (overlaps, unconsumed arguments) | bindings.kdl | invalid/schemas/duplicate-argument, invalid/schemas/rest-arguments-overlap, invalid/schemas/inspect-duplicate-argument, invalid/schemas/inspect-duplicate-nested-argument, invalid/schemas/inspect-duplicate-property, invalid/schemas/inspect-rest-arguments-overlap, processing/sch003-span-vet, processing/sch003-span-inspect | Span pinned: SCH003 reports the node declaration on both reading paths |
| Child permission and cardinality | bindings.kdl | invalid/schemas/cardinality, invalid/schemas/inspect-cardinality, invalid/data/min-occurs, invalid/data/max-occurs | Minimums at scope completion |
| Compiled plans | bindings.kdl | processing/bind/stream, processing/bind/context | Item streams come from the compiled plan, not re-reading the schema KDL |
| Declaration-group expansion (`expand` sources, `include` short form) | bindings.kdl | processing/bind/expansion, invalid/data/expansion-key-collision, schemas/inspect/include-short-form-dropped | The `expand` source splices on every CLI path; the `include` short form is compile-path-only and dropped by the typed reader (declared elision below) |
| Dependent value binding (`value`/`against`, `argument-tail`) | bindings.kdl | processing/bind/dependent-binding, invalid/data/dependent-payload-type | Payload validated inline; no pending typed-binding item exists yet |
| Case dispatch (`cases`/`case`/`otherwise`) | bindings.kdl | processing/bind/case-dispatch, invalid/data/case-no-match | Top-down first-match retyping; no match without `otherwise` is BND001 |

### Context (`pkg/kdl/schemas/dialect/context.kdl`)

| Mechanism | Defining file | Covering scenarios | Notes |
| --- | --- | --- | --- |
| Engine-managed, schema-defined context | context.kdl | schemas/vet/catalog, schemas/inspect/shelf, processing/bind/context | |
| Typed slots | context.kdl | schemas/vet/catalog, invalid/data/slot-type, invalid/schemas/unknown-slot | Slot-typed values validated |
| Effect targets (`following`/`children`) | context.kdl | schemas/inspect/shelf, invalid/schemas/effects-without-context-control, invalid/schemas/children-effect-from-child | |
| Generic effects (`set`/`remove`/`clear`) | context.kdl | schemas/inspect/shelf, invalid/data/failed-control | Both targets |
| Registered handlers | context.kdl | processing/bind/handler-effect, invalid/data/handler-not-installed, invalid/data/handler-not-installed-inspect, invalid/schemas/handler-not-installed, invalid/schemas/validator-not-installed, processing/inspect-accepts-handler-not-installed | An installed handler executes through `--handler copy-fields` (the CLI's built-in catalog); an uninstalled name fails closed on the vet, inspect, and schema-load paths |
| Context-bound fields and policies (`fallback`/`fixed`) | context.kdl | processing/bind/context, invalid/data/context-fixed, invalid/data/context-unavailable | |
| Execution timing | context.kdl | invalid/schemas/children-effect-from-child, invalid/data/failed-control | Children effects from node header only; failed node applies no following effects |
| Source context (`source-context`, `path-stem`, modes) | context.kdl | processing/bind/source-context, processing/bind/source-context-per-file, invalid/schemas/source-context-missing-mode | Values derived from source metadata before the first semantic node; no default mode |
| Accumulating context (`accumulate append`/`merge`) | context.kdl | processing/bind/accumulate, invalid/data/accumulate-no-op | Second value on a slot with no declared operation is CTX003 |
| Context snapshots (index-entry data model) | context.kdl | dialect/canonical-inspect/context | The declared data model reads as typed declarations; no CLI surface restores a snapshot yet (declared elision below) |

### Documents, structure, and imports (`pkg/kdl/schemas/dialect/documents.kdl`)

| Mechanism | Defining file | Covering scenarios | Notes |
| --- | --- | --- | --- |
| Dialect and schema roots | documents.kdl | dialect/canonical-vet/merged, dialect/entrypoint-vet/dialect, invalid/schemas/root-not-schema | |
| Contributing documents | documents.kdl | dialect/canonical-vet/merged, invalid/schemas/duplicate-type, processing/inspect-accepts-duplicate-type | Repeated declaration rejected, not merged |
| First-node rule | documents.kdl | invalid/data/first-node, schemas/inspect/shelf | |
| Following rule | documents.kdl | invalid/data/not-following | |
| Exactly-one groups | documents.kdl | invalid/data/exactly-one-missing, invalid/data/exactly-one-repeated | Checked at scope completion |
| Source imports | documents.kdl | processing/bind/import-reject, processing/bind/import-projection, dialect/entrypoint-vet/dialect, invalid/dialect/import-missing, invalid/dialect/import-cycle, invalid/schemas/import-missing, invalid/data/import-cycle | Projection only, mutations never escape; the data path executes imports as invocations with per-document source identity |
| Repeated imports (`reject`/`coalesce`/`instantiate`) | documents.kdl | processing/bind/import-reject, processing/bind/import-coalesce, processing/bind/import-instantiate, invalid/data/import-repeated, schemas/vet/import-repeat-coalesce, schemas/vet/import-repeat-instantiate, dialect/entrypoint-vet/import-repeat-coalesce, dialect/entrypoint-vet/import-repeat-instantiate | All three policies pinned on the schema path, the dialect path, and the data path |
| Document profiles (`profile`, `roots allow`) | documents.kdl | schemas/vet/profile-declaration, processing/bind/profile-bind, invalid/data/profile-none-named, invalid/data/profile-unknown | Declaration surface; both SCH011 arms (none named, unknown name) plus the named-bind positive through `--profile` |
| Descendant rules (`descendants`) | documents.kdl | processing/bind/descendants, invalid/data/descendants-too-many | Subtree occurrence bounds checked at the declaring scope's completion |
| Ordered sequences (`sequence`/`step`/`alternatives`) | documents.kdl | processing/bind/sequence, invalid/data/sequence-out-of-order | Compiled into the per-scope state machine; violations are BND002 |
| Discovery grammars (`discovery`/`anchor`/`patterns`) | documents.kdl | schemas/vet/discovery-local, processing/bind/import-anchor, invalid/data/import-anchor-unsupplied, invalid/schemas/import-expression-env, invalid/schemas/import-expression-anchor, invalid/schemas/import-expression-unexpanded | Grammar validation pinned; the data path expands expressions against host-supplied anchors (`--anchor`), ordered by portable path bytes; the definition paths (vet/inspect of schemas and dialects) still evaluate literal paths only (declared elision below) |
| Import aliases (`as=NAME`) | documents.kdl | processing/bind/import-alias, processing/bind/import-alias-data, invalid/data/import-alias-duplicate, schemas/vet/import-alias, invalid/schemas/import-alias-not-kebab | The aliased contributing document merges; the alias value round-trips into the data path's import item; a duplicate alias in one document rejects BND006; the alias namespace's visibility gating stays linking-internal (declared elision below) |
| Symbol catalogs (`identity`/`reference`/`qualifier`) | documents.kdl | schemas/vet/identity-reference, processing/bind/identity-reference, invalid/schemas/reference-unknown-qualification | Declaration surface and composed identity at bind; linking codes stay elided |
| Contribution resolution (`resolution`, layer/scope records) | documents.kdl | processing/bind/contributions, invalid/schemas/resolver-not-installed | Contributions stay preserved records; an unregistered resolver fails closed DLC006 |
| Computed defaults (`fallback computed=`) | documents.kdl | processing/bind/computed-default, invalid/schemas/fallback-computed-non-bool | The computed default never materializes a value; the field stays absent |
| Secret providers (`provider`) | documents.kdl | schemas/vet/secret-provider, invalid/schemas/provider-empty-command | Descriptor data only; the command list is never executed |
| Deferral marker (retired) | documents.kdl | invalid/schemas/deferred-mechanism, invalid/schemas/inspect-deferred-mechanism | The marker retired with the twelve mechanisms it tracked; its spelling rejects as BND001 unbound on every reading path, the same loud failure it existed to guarantee |
| This dialect's own document rule | documents.kdl | invalid/dialect/not-self-describing, invalid/schemas/root-not-schema | |
| Document rules are records | documents.kdl | schemas/inspect/shelf, processing/round-trip/data-through-schema | |
| Self-hosted reader | documents.kdl | dialect/canonical-inspect/merged, dialect/entrypoint-inspect/dialect | Reading the canonical files through the reader they seed |

## Code census

The diagnostic registry (`pkg/rs/diagnostic/src/registry.rs`) is the
census authority: every registered code appears below with an explicit
status — `covered`, `UNCOVERED`, or `elided-with-reason` — even when the
code is absent from the design record's table.

### Definition-surface codes

| Code | Exercising scenarios | Status |
| --- | --- | --- |
| KDLITH-BND001 | invalid/data/case-no-match, invalid/data/first-node, invalid/data/not-following, invalid/data/union-variant, invalid/data/unknown-node, invalid/schemas/unknown-node, invalid/schemas/inspect-unknown-node | covered |
| KDLITH-BND002 | invalid/data/descendants-too-many, invalid/data/exactly-one-missing, invalid/data/exactly-one-repeated, invalid/data/max-occurs, invalid/data/min-occurs, invalid/data/sequence-out-of-order, invalid/schemas/cardinality, invalid/schemas/inspect-cardinality | covered |
| KDLITH-BND003 | invalid/data/codec-match-failure, invalid/data/dependent-payload-type, invalid/data/failed-control, invalid/data/slot-type, invalid/data/union-prefix-miss, invalid/data/value-type, invalid/schemas/fallback-computed-non-bool, invalid/schemas/import-alias-not-kebab, invalid/schemas/name-format, invalid/schemas/inspect-name-format | covered |
| KDLITH-BND004 | invalid/data/entries-key, invalid/data/failed-control, invalid/data/missing-field, invalid/schemas/missing-argument, invalid/schemas/root-without-version, invalid/schemas/source-context-missing-mode, invalid/schemas/inspect-missing-argument, invalid/schemas/inspect-root-without-version | covered |
| KDLITH-BND005 | invalid/data/unbound-entry, invalid/schemas/sensitivity-inline, invalid/schemas/unbound-property, invalid/schemas/inspect-unbound-property | covered |
| KDLITH-BND006 | invalid/data/duplicate-key, invalid/data/expansion-key-collision, invalid/data/import-alias-duplicate, invalid/schemas/duplicate-binding, invalid/schemas/inspect-duplicate-binding | covered — the duplicate-binding and duplicate-key arms plus the import-alias arm on the data path |
| KDLITH-BND007 | invalid/data/collision | covered |
| KDLITH-BND008 | invalid/data/enum-member, invalid/schemas/reference-unknown-qualification, invalid/schemas/unknown-format, invalid/schemas/unknown-role, invalid/schemas/inspect-unknown-format, invalid/schemas/inspect-unknown-role | covered |
| KDLITH-DLC002 | (none) | UNCOVERED — reachable (a dialect document declaring one construct twice is rejected with it), no fixture yet |
| KDLITH-DLC003 | invalid/schemas/deferred-mechanism, invalid/schemas/inspect-deferred-mechanism | retired — the code is never minted; the two scenarios pin that the retired marker's spelling now rejects as KDLITH-BND001 on both reading paths |
| KDLITH-DLC004 | invalid/schemas/root-not-schema | covered |
| KDLITH-DLC005 | invalid/schemas/codec-pattern-invalid, invalid/schemas/generic-arity-mismatch, invalid/schemas/pattern-invalid, invalid/schemas/provider-empty-command, processing/inspect-accepts-malformed-pattern | covered |
| KDLITH-DLC006 | invalid/data/handler-not-installed, invalid/data/handler-not-installed-inspect, invalid/schemas/handler-not-installed, invalid/schemas/resolver-not-installed, invalid/schemas/validator-not-installed, processing/bind/handler-effect, processing/inspect-accepts-handler-not-installed | covered — the uninstalled arm on the vet data, inspect data, and schema-load paths, plus the installed arm through `--handler copy-fields` |
| KDLITH-DLC007 | invalid/schemas/bad-type-expression, invalid/schemas/inspect-bad-type-expression | covered |
| KDLITH-DLC008 | invalid/dialect/not-self-describing, invalid/dialect/unknown-type | covered |
| KDLITH-SCH001 | invalid/schemas/codec-capture-unknown-type, invalid/schemas/unknown-type, processing/inspect-accepts-unknown-type | covered |
| KDLITH-SCH002 | invalid/schemas/duplicate-type, processing/inspect-accepts-duplicate-type | covered |
| KDLITH-SCH003 | invalid/schemas/duplicate-argument, invalid/schemas/inspect-duplicate-argument, invalid/schemas/inspect-duplicate-nested-argument, invalid/schemas/inspect-duplicate-property, invalid/schemas/inspect-rest-arguments-overlap, invalid/schemas/rest-arguments-overlap, processing/sch003-span-inspect, processing/sch003-span-vet | covered |
| KDLITH-SCH004 | invalid/schemas/duplicate-declaration | covered |
| KDLITH-SCH005 | invalid/schemas/effects-without-context-control | covered |
| KDLITH-SCH006 | invalid/schemas/import-projects-unknown-slot, invalid/schemas/inspect-import-projects-unknown-slot, invalid/schemas/ref-without-declaration, invalid/schemas/unknown-field, invalid/schemas/unknown-parent, invalid/schemas/unknown-slot | covered |
| KDLITH-SCH007 | invalid/schemas/children-effect-from-child, processing/inspect-accepts-not-streamable | covered |
| KDLITH-SCH008 | invalid/schemas/documented-default-type, invalid/schemas/fallback-type, invalid/schemas/set-value-type, processing/inspect-accepts-schema-literal | covered |
| KDLITH-SCH009 | invalid/schemas/instantiation-cycle | covered — the include-cycle arm of the code is unreachable through the CLI (see the include short-form elision); the instantiation-cycle arm is pinned |
| KDLITH-SCH010 | invalid/schemas/prefix-overlap | covered |
| KDLITH-SCH011 | invalid/data/profile-none-named, invalid/data/profile-unknown, processing/bind/profile-bind | covered — both arms (none named, unknown name through `--profile`) plus the named-bind positive |
| KDLITH-IMP001 | invalid/dialect/import-missing, invalid/schemas/import-expression-unexpanded, invalid/schemas/import-missing | covered |
| KDLITH-IMP002 | invalid/data/import-cycle, invalid/dialect/import-cycle | covered — the cycle chain from the active invocation stack on both paths |
| KDLITH-IMP003 | invalid/data/entry-as-import-target, invalid/data/import-repeated, invalid/dialect/import-repeated | covered |
| KDLITH-IMP004 | invalid/schemas/import-expression-env | covered |
| KDLITH-IMP005 | invalid/data/import-anchor-unsupplied, invalid/schemas/import-expression-anchor | covered — the unsupplied-anchor arm on the data path and the schema path |
| KDLITH-CTX001 | invalid/data/context-unavailable, invalid/data/slot-type | covered |
| KDLITH-CTX002 | invalid/data/context-fixed | covered |
| KDLITH-CTX003 | invalid/data/accumulate-no-op | covered |
| KDLITH-CTX004 | (none) | elided-with-reason — the code exists only on the engine's snapshot-restore API, which no CLI verb reaches; no fixture can honestly mint it yet |
| KDLITH-GEN003 | invalid/dialect/processor-mismatch | covered |
| KDLITH-SYN001 | the 94 kdl-spec negative controls listed below | covered |
| KDLITH-SYN002 | invalid/kdl-spec/unterminated_empty_node_fail, invalid/data/syntax | covered |
| KDLITH-SYN003 | invalid/data/duplicate-property | covered |

The KDLITH-SYN001 kdl-spec negatives (one scenario per upstream invalid
case; every `invalid/kdl-spec/` scenario except the SYN002 one above):

```
invalid/kdl-spec/bare_ident_numeric_dot_fail, invalid/kdl-spec/bare_ident_numeric_fail,
invalid/kdl-spec/bare_ident_numeric_sign_fail, invalid/kdl-spec/bom_later_fail,
invalid/kdl-spec/dot_but_no_fraction_before_exponent_fail, invalid/kdl-spec/dot_but_no_fraction_fail,
invalid/kdl-spec/dot_in_exponent_fail, invalid/kdl-spec/dot_zero_fail,
invalid/kdl-spec/empty_arg_type_fail, invalid/kdl-spec/empty_node_type_fail,
invalid/kdl-spec/empty_prop_type_fail, invalid/kdl-spec/err_backslash_in_bare_id_fail,
invalid/kdl-spec/false_prop_key_fail, invalid/kdl-spec/floating_point_keyword_identifier_strings_fail,
invalid/kdl-spec/hash_in_id_fail, invalid/kdl-spec/illegal_char_in_binary_fail,
invalid/kdl-spec/illegal_char_in_hex_fail, invalid/kdl-spec/illegal_char_in_octal_fail,
invalid/kdl-spec/just_space_in_arg_type_fail, invalid/kdl-spec/just_space_in_node_type_fail,
invalid/kdl-spec/just_space_in_prop_type_fail, invalid/kdl-spec/just_type_no_arg_fail,
invalid/kdl-spec/just_type_no_node_id_fail, invalid/kdl-spec/just_type_no_prop_fail,
invalid/kdl-spec/legacy_raw_string_fail, invalid/kdl-spec/legacy_raw_string_hash_fail,
invalid/kdl-spec/multiline_raw_string_non_matching_prefix_character_error_fail,
invalid/kdl-spec/multiline_raw_string_non_matching_prefix_count_error_fail,
invalid/kdl-spec/multiline_raw_string_single_line_err_fail, invalid/kdl-spec/multiline_raw_string_single_quote_err_fail,
invalid/kdl-spec/multiline_string_escape_newline_at_end_fail, invalid/kdl-spec/multiline_string_final_whitespace_escape_fail,
invalid/kdl-spec/multiline_string_non_literal_prefix_fail,
invalid/kdl-spec/multiline_string_non_matching_prefix_character_error_fail,
invalid/kdl-spec/multiline_string_non_matching_prefix_count_error_fail,
invalid/kdl-spec/multiline_string_single_line_err_fail, invalid/kdl-spec/multiline_string_single_quote_err_fail,
invalid/kdl-spec/multiple_dots_in_float_before_exponent_fail, invalid/kdl-spec/multiple_dots_in_float_fail,
invalid/kdl-spec/multiple_es_in_float_fail, invalid/kdl-spec/multiple_x_in_hex_fail,
invalid/kdl-spec/no_digits_in_hex_fail, invalid/kdl-spec/no_integer_digit_fail,
invalid/kdl-spec/no_solidus_escape_fail, invalid/kdl-spec/null_prop_key_fail,
invalid/kdl-spec/parens_in_bare_id_fail, invalid/kdl-spec/quote_in_bare_id_fail,
invalid/kdl-spec/raw_string_just_quote_fail, invalid/kdl-spec/semicolon_missing_after_children_fail,
invalid/kdl-spec/slashdash_after_arg_type_fail, invalid/kdl-spec/slashdash_after_node_type_fail,
invalid/kdl-spec/slashdash_after_prop_key_fail, invalid/kdl-spec/slashdash_after_prop_val_type_fail,
invalid/kdl-spec/slashdash_after_type_fail, invalid/kdl-spec/slashdash_before_children_end_fail,
invalid/kdl-spec/slashdash_before_eof_fail, invalid/kdl-spec/slashdash_before_prop_value_fail,
invalid/kdl-spec/slashdash_before_semicolon_fail, invalid/kdl-spec/slashdash_between_child_blocks_fail,
invalid/kdl-spec/slashdash_child_block_before_entry_err_fail, invalid/kdl-spec/slashdash_inside_arg_type_fail,
invalid/kdl-spec/slashdash_inside_node_type_fail, invalid/kdl-spec/slash_in_bare_id_fail,
invalid/kdl-spec/square_bracket_in_bare_id_fail, invalid/kdl-spec/true_prop_key_fail,
invalid/kdl-spec/type_before_prop_key_fail, invalid/kdl-spec/unbalanced_raw_hashes_fail,
invalid/kdl-spec/underscore_at_start_of_fraction_fail, invalid/kdl-spec/underscore_at_start_of_hex_fail,
invalid/kdl-spec/unicode_delete_fail, invalid/kdl-spec/unicode_escaped_above_max_fail,
invalid/kdl-spec/unicode_escaped_h1_fail, invalid/kdl-spec/unicode_escaped_h2_fail,
invalid/kdl-spec/unicode_escaped_h3_fail, invalid/kdl-spec/unicode_escaped_h4_fail,
invalid/kdl-spec/unicode_escaped_l1_fail, invalid/kdl-spec/unicode_escaped_l2_fail,
invalid/kdl-spec/unicode_escaped_l3_fail, invalid/kdl-spec/unicode_escaped_too_long_lead0_fail,
invalid/kdl-spec/unicode_fsi_fail, invalid/kdl-spec/unicode_lre_fail, invalid/kdl-spec/unicode_lri_fail,
invalid/kdl-spec/unicode_lrm_fail, invalid/kdl-spec/unicode_lro_fail, invalid/kdl-spec/unicode_pdf_fail,
invalid/kdl-spec/unicode_pdi_fail, invalid/kdl-spec/unicode_rle_fail, invalid/kdl-spec/unicode_rli_fail,
invalid/kdl-spec/unicode_rlm_fail, invalid/kdl-spec/unicode_rlo_fail, invalid/kdl-spec/unicode_under_0x20_fail,
invalid/kdl-spec/zero_space_before_first_arg_fail, invalid/kdl-spec/zero_space_before_prop_fail,
invalid/kdl-spec/zero_space_before_second_arg_fail
```

### Codes outside the definition surface

| Code | Exercising scenarios | Status |
| --- | --- | --- |
| KDLITH-CLI001 | (none) | elided-with-reason — no emitting path at v0; argument-form rejections are clap usage errors (exit 2) without a registered code |
| KDLITH-CLI002 | (none) | elided-with-reason — no emitting path at v0; no conflicting-option pair exists yet |
| KDLITH-CLI003 | (none) | elided-with-reason — `--stdout-format` rejection is a clap value error (processing/cli/stdout-format-invalid pins exit 2 and the message, not the code) |
| KDLITH-SRC001 | (none) | UNCOVERED — reachable (an unreadable named document reports it), no fixture yet |
| KDLITH-SRC002 | (none) | UNCOVERED — reachable (a non-UTF-8 document reports it), no fixture yet; needs a committed non-UTF-8 input |
| KDLITH-SRC003 | invalid/data/source-limit | covered — 600-deep nesting input rejected at the limit |
| KDLITH-DLC009 | (none) | UNCOVERED — reachable (a dialect-definition literal that does not decode as its type reports it), no fixture yet |
| KDLITH-REF001 | (none) | elided-with-reason — no emitting path at v0; references bind provisionally and linking never fails a run |
| KDLITH-REF002 | (none) | elided-with-reason — no emitting path at v0; symbol-identity clashes surface as DLC002/SCH002/SCH004 first |
| KDLITH-GEN001 | codegen/gen-refuses-unowned-overwrite | covered |
| KDLITH-GEN002 | codegen/gen-dialect-check-drift | covered — the missing, changed, and stale arms in one destination |
| KDLITH-EXP001 | (none) | elided-with-reason — `export` is a stub |
| KDLITH-EXP002 | (none) | elided-with-reason — `export` is a stub |
| KDLITH-DLC001 | (none) | elided-with-reason — retired; a node no dialect binding accepts is BND001 on every reading path |

## Harness-protection census

What the suite proves about the harness itself, beyond the processor
under test.

| Protection | Exercised by | State |
| --- | --- | --- |
| Fail-closed scratch classification of a planted violation (symlink and foreign file reported as foreign entries by the harness, link never followed, neither silently deleted) | harness/scratch-fail-closed | covered — doubles as the detection meta-fixture: neutralizing the classification fails the scenario |

## Verb-contract census

| Verb contract | Exercised by | State |
| --- | --- | --- |
| `vet dialect` local scope, one file each | dialect/canonical-vet/bindings-local, dialect/canonical-vet/context-local, dialect/canonical-vet/documents-local, dialect/canonical-vet/types-local | covered |
| `vet dialect` complete, digest pin | dialect/canonical-vet/merged, dialect/entrypoint-vet/dialect, dialect/entrypoint-vet/import-repeat-coalesce, dialect/entrypoint-vet/import-repeat-instantiate | covered — same digest `d1f2c23d303b8b9d` on every path, including under the dialect-path repeated policies |
| `vet schema` bundled default | processing/vet-schema-default, schemas/vet/catalog | covered — catalog digest pinned |
| `vet schema` `--dialect` override | processing/vet-schema-override | covered — same digest as the default |
| `vet schema` multi-document and import policies | schemas/vet/import-alias, schemas/vet/import-repeat-coalesce, schemas/vet/import-repeat-instantiate, invalid/schemas/duplicate-type | covered |
| `vet data` item stream | processing/bind/accumulate, processing/bind/case-dispatch, processing/bind/codecs, processing/bind/computed-default, processing/bind/context, processing/bind/contributions, processing/bind/defaults, processing/bind/dependent-binding, processing/bind/descendants, processing/bind/expansion, processing/bind/generic-types, processing/bind/identity-reference, processing/bind/import-alias, processing/bind/import-alias-data, processing/bind/import-anchor, processing/bind/import-coalesce, processing/bind/import-instantiate, processing/bind/import-projection, processing/bind/import-reject, processing/bind/nullable, processing/bind/sensitivity, processing/bind/sequence, processing/bind/source-context, processing/bind/source-context-per-file, processing/bind/stream, processing/bind/union-dispatch, processing/bind/value-expr, dialect/canonical-bind/bindings, dialect/canonical-bind/context, dialect/canonical-bind/documents, dialect/canonical-bind/types, dialect/entrypoint-bind/dialect | covered |
| `vet data` negatives | every invalid/data/ scenario (see code census) | covered |
| data-verb host inputs (`--anchor`, `--profile`, `--handler` on vet/inspect/read data) | processing/bind/import-anchor, processing/bind/handler-effect, processing/bind/profile-bind, invalid/data/import-anchor-unsupplied, invalid/data/handler-not-installed, invalid/data/profile-unknown | covered — anchor resolution, installed-handler execution, and profile selection through the CLI surface; a malformed `--anchor` or unknown `--handler` is a clap usage error (exit 2), consistent with the other argument-form rejections |
| `inspect dialect` | dialect/canonical-inspect/bindings, dialect/canonical-inspect/context, dialect/canonical-inspect/documents, dialect/canonical-inspect/merged, dialect/canonical-inspect/types, dialect/entrypoint-inspect/dialect | covered |
| `inspect schema` | schemas/inspect/catalog, schemas/inspect/constructs, schemas/inspect/include-short-form-dropped, schemas/inspect/shelf | covered — include-short-form-dropped pins the reader-path include drop, not a splice |
| `inspect data` | processing/inspect-data, processing/round-trip/inspect-data-catalog, invalid/data/handler-not-installed-inspect | covered — the same bound item stream, and the same fail-closed load an uninstalled handler rejects |
| `inspect --view` fail-closed | processing/inspect-view-bound, processing/inspect-view-context, processing/inspect-view-effective, processing/inspect-view-origins, processing/inspect-view-plan, processing/inspect-view-references | covered — all six unimplemented views pinned as fail-closed |
| `read` bare form | dialect/canonical-read/bindings, dialect/canonical-read/context, dialect/canonical-read/documents, dialect/canonical-read/types, dialect/entrypoint-read/dialect | covered |
| `read file` | processing/read-file-form | covered |
| `read data` found / missing | processing/read-data-found, processing/read-data-missing | covered |
| `read data --entry` | processing/read-data-entry, processing/read-data-entry-import, invalid/data/entry-as-import-target | covered — composition pinned via the source index; the entry's own imports execute in the same session (a record reached through the entry's import is found, source-indexed), and a read document naming its own entry rejects as a repeat |
| round trip dialect to schema to data | processing/round-trip/data-through-schema, processing/round-trip/inspect-data-catalog | covered — full chain over the committed catalog schema |
| `--help` | processing/smoke-help | covered |
| `--version` | processing/cli/version | covered |
| `help` verb | processing/cli/help-verb, processing/cli/help-vet | covered |
| stub verbs fail closed | processing/cli/stub-export, processing/cli/stub-list, processing/cli/stub-cache, processing/cli/stub-changelog | covered — exit 1 plus "not implemented yet"; `gen` is implemented, its scenarios are the gen rows below |
| `--stdout-format` non-text | processing/cli/stdout-format-invalid | covered — exit 2 |
| `--verbosity` out of band | processing/cli/verbosity-out-of-band | covered — exit 2 |
| bare `kdlith` no args | processing/cli/no-args | covered — exit 2, usage on stderr |
| `gen dialect <entrypoint>` | codegen/gen-dialect, codegen/gen-dialect-entrypoint-read, codegen/gen-dialect-entrypoint-vet | covered — generation runs end to end into scratch (never the reference crate) with the summary pinned, over the same entrypoint whose parse and vet preconditions the entrypoint scenarios pin |
| `gen schema <schema>` | codegen/gen-schema | covered — a committed schema generates its data-reader crate into scratch |
| `gen --dry-run` plan, write nothing | codegen/gen-dialect-dry-run | covered — the plan lists the generator-owned paths, and the scratch check proves nothing but the planted file appears in the materialized copy |
| `gen --check` drift report | codegen/gen-dialect-check-drift | covered — missing, changed, and stale generator-owned files each reported KDLITH-GEN002 with exit 1; the destination's unowned extension file is present and not flagged |
| `gen` refuses an unowned overwrite | codegen/gen-refuses-unowned-overwrite | covered — KDLITH-GEN001, exit 1, and the scratch check proves nothing was written |

## Declared elisions

Each row names a gap the suite deliberately does not exercise, why, and
the condition that reopens it. No scenario anywhere reports an elided
behavior as passed.

| Gap | Why it is not exercised | Reopen condition |
| --- | --- | --- |
| Generation oracle comparison (generate from `contracts/super-schemas/dialect.kdl` into scratch, compare against `pkg/rs/superproc` excluding the generated marker, compile, exercise) | The scenario format expresses exactly one invocation with stream assertions; the oracle comparison needs a multi-step run (generate, compare, compile, exercise) and file comparison, which the harness does not offer yet — the gen scenarios here pin CLI-level outcomes only | The harness grows a multi-step scenario kind and file comparison — add the oracle scenarios under `codegen/` |
| Stub verbs beyond fail-closed (`export`, `list`, `cache`, `changelog`) | The verbs are stubs; only their exit-1 "not implemented yet" contract exists | Each verb is implemented — replace its processing/cli stub pin with the real contract scenarios |
| Unimplemented `inspect --view` views beyond the six fail-closed pins | The views do not exist; the pins prove the fail-closed rejection, not view content | A view is implemented — flip its inspect-view pin to the real output |
| Compiler-only codes accepted on the inspect path | Deliberate product decision: `inspect` runs only the checks the dialect's declarations own, so it accepts inputs whose only faults are compiler-only checks (pinned by the six inspect-accepts scenarios) | The product decision changes — the inspect-accepts pins and the design record change together |
| Import-alias namespace visibility gating | The alias value round-trips into the executed import item and a duplicate alias rejects BND006 (pinned by `processing/bind/import-alias-data` and `invalid/data/import-alias-duplicate`), but the exclusivity itself — imported declarations reachable only through the alias namespace — is a linking-stage behavior: references bind provisionally at v0 and linking never fails a run, and type-merging ignores the alias (an aliased duplicate type still rejects SCH002), so no CLI surface observes the gating yet | Linking gains a resolution surface — add the qualification negatives and the alias-gated visibility pins |
| Definition-path import-expression expansion (anchors on the schema and dialect loading paths) | The data verbs expand grammar-admitted expressions against `--anchor` values (pinned by `processing/bind/import-anchor`), but the definition reader that loads schemas and dialects still evaluates import paths literally, so a schema or dialect whose own imports use discovery syntax fails complete checking with KDLITH-IMP001 regardless of `--anchor` (honestly pinned by `invalid/schemas/import-expression-unexpanded`); the definition paths take no `--anchor` flag, so nothing is silently ignored | The definition reader consumes host anchors — add the flag to the definition verbs and rewrite the pin into expansion positives |
| Snapshot restore surface (KDLITH-CTX004) | The engine's snapshot checkpoint/delta/restore API is not reachable from any verb; `read data` entry composition uses the source index, and no CLI path restores a context, so the fingerprint/lineage rejection cannot be honestly minted | A verb restores indexed records from snapshots — add the mismatch negative |
| Required field with a computed-only fallback | The design record does not define the interplay: a non-optional field whose fallback is computed reports missing-required at bind (fail-closed) because the computed default never materializes; `processing/bind/computed-default` pins the optional-field arm only | A record ruling lands — pin whichever behavior it selects |
| Reference resolution failures (`KDLITH-REF001`/`REF002`) | References bind provisionally at v0; linking never fails a run, so `read data --entry` composition is pinned via the source index rather than resolution | Linking gains a failure surface — add resolution negatives and tighten `processing/read-data-entry` |
| Registered CLI/export error codes (`KDLITH-CLI001`-`003`, `EXP001`-`002`) | No emitting path exists at v0: usage errors are clap's exit-2 stratum, and `export` is a stub (the generation codes `GEN001`-`002` emit through `gen`, their controls live in the code census above) | The emitting paths land — add one negative control per code |
| Reader-path `include` short form silently dropped | Every CLI reading path goes through the typed reader, which admits `include` children and binds them through `node include`/`IncludeRef` but has no field consuming them, so the typed value drops them and the reader-path compile records no pending expansion — a schema using `include` silently loses the included members (fail-open, recorded as a superproc-side deviation; `schemas/inspect/include-short-form-dropped` pins the drop). Consequences also elided: the include splice is unprovable through the CLI, and SCH009's include-cycle arm is unreachable (the instantiation-cycle arm is pinned by `invalid/schemas/instantiation-cycle`) | The engine grows an expansion-request channel the typed reader can feed (`kdlith_model::ExpansionRequest` exists unproduced) or the binder rejects reader-path includes loudly — then rewrite the pin into the splice or the rejection and un-elide the SCH009 include-cycle arm (candidate for a later dialect-expansion story) |
| Pending typed-binding item emission | The model's `PendingBinding` has no producer: the engine binder validates dependent `argument-tail` payloads inline (BND003/BND002) and `BoundItem`/`TypedItem` carry no pending variant, so a dependent payload's validated value never surfaces as its own item (a superproc-side deviation; `processing/bind/dependent-binding` pins the inline validation instead) | An engine surface grows the pending item — extend the dependent-binding positive to pin the emitted pending item |
| Committed run commands name the `target/debug` build path | The committed commands are the contract; on hosts where that build path is unavailable a host-local substitution runs the same binaries | None — host note only |

## Story checklist

The acceptance criteria this suite answers to, with their gate commands:

- [ ] `cargo build --workspace` exits 0 (run unpiped; record the literal exit code)
- [ ] `repoci check` exits 0 (run unpiped; record the literal exit code)
- [ ] `/usr/bin/rg --no-ignore --hidden -n 'cfg\(test\)|#\[test\]' pkg tools tests --glob '*.rs'` returns zero matches
- [ ] `target/debug/kdlith-fixtures --fx-suite tests/fixtures --fx-kdlith target/debug/kdlith` exits 0 unpiped, with all counts recorded
- [ ] Each commit subject is conventional, lowercase, at most 72 characters, with no body (`git log --format=%s%n%b`)
- [ ] The harness output names the codegen elision explicitly; no scenario reports the oracle comparison as passed
- [ ] Every `tests/fixtures/invalid/` scenario carries an `expect-invalid` code line
