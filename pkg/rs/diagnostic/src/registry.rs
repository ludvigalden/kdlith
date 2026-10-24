//! The diagnostic-code registry: areas, codes, and the
//! `KDLITH-<AREA><NNN>` numbering scheme. A code is the stable identity
//! of one condition: never renumbered or reused; a behavior change takes
//! a new code.

use crate::record::Severity;

/// The fixed product prefix of every registered code, so kdlith codes
/// never collide with codes from a host application or a generated
/// processor's own registry.
pub const CODE_PREFIX: &str = "KDLITH-";

/// The highest number an area may allocate. `<NNN>` is three zero-padded
/// decimal digits: `001`-`199` are allocated by the owning area in
/// ascending order, `200`-`999` stay unallocated, `000` is never a code.
pub const MAX_ALLOCATED_NUMBER: u16 = 199;

/// One diagnostic-area namespace.
#[derive(Debug)]
#[non_exhaustive]
pub struct AreaDefinition {
    pub name: &'static str,
    /// The crate or design surface that owns the area's numbers.
    pub owner: &'static str,
    pub scope: &'static str,
}

/// One registered diagnostic code. The table carries only publicly
/// resolvable columns.
#[derive(Debug)]
#[non_exhaustive]
pub struct CodeDefinition {
    pub code: &'static str,
    /// Stable kebab-case name of the condition: short and
    /// presentation-neutral.
    pub label: &'static str,
    /// Fixed for the life of the code; never changes once registered.
    pub severity: Severity,
    /// Exit status when this code alone decides the outcome and
    /// warnings are not promoted to errors: `2` for invocation errors,
    /// `1` for every other error, `0` for warnings and notes.
    pub exit_class: u8,
    /// The condition that mints this code.
    pub trigger: &'static str,
    /// Human-readable message with `<placeholder>` tokens filled in
    /// token order by [`render`].
    pub message_template: &'static str,
    /// How to fix the condition.
    pub correction: &'static str,
    /// A retired code never mints a diagnostic again; it stays listed
    /// with `retired: true` so its number is visibly spent.
    pub retired: bool,
}

/// A code split into its area and number; see [`parse_code`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CodeParts<'a> {
    pub area: &'a str,
    pub number: u16,
}

/// The diagnostic areas, in pipeline order. `<AREA>` is one of these
/// three-letter uppercase names; each area is owned by one design
/// surface, and a code's home area never moves.
pub const AREAS: &[AreaDefinition] = &[
    AreaDefinition {
        name: "CLI",
        owner: "cli",
        scope: "command-line arguments, option combinations, output selection",
    },
    AreaDefinition {
        name: "SRC",
        owner: "parse",
        scope: "source loading, encoding, size and nesting limits, source coordinates",
    },
    AreaDefinition {
        name: "SYN",
        owner: "parse",
        scope: "KDL syntax: tokens, strings, numbers, nodes, slashdash, annotations",
    },
    AreaDefinition {
        name: "IMP",
        owner: "runtime",
        scope: "import requests: cycles, repeats, the processed import graph",
    },
    AreaDefinition {
        name: "LOC",
        owner: "engine",
        scope: "locations and sources: resolving import locations inside named source trees",
    },
    AreaDefinition {
        name: "DLC",
        owner: "engine",
        scope: "dialect definitions: dialect constructs, their validation and linking",
    },
    AreaDefinition {
        name: "SCH",
        owner: "engine",
        scope: "schema definitions: types, node bindings, context effects, processing rules, compiled plans",
    },
    AreaDefinition {
        name: "BND",
        owner: "runtime",
        scope: "binding data documents: node and field bindings, cardinality, value decoding",
    },
    AreaDefinition {
        name: "CTX",
        owner: "runtime",
        scope: "context operations: context-producing controls, scopes, scope completion",
    },
    AreaDefinition {
        name: "REF",
        owner: "runtime",
        scope: "declarations and references: symbol identity, lookup, deferred binding",
    },
    AreaDefinition {
        name: "GEN",
        owner: "codegen",
        scope: "processor generation: output manifest, generator-owned paths, check drift",
    },
    AreaDefinition {
        name: "EXP",
        owner: "cli",
        scope: "export: item selection, output destinations, atomic and provisional delivery",
    },
    AreaDefinition {
        name: "CAC",
        owner: "runtime",
        scope: "derived processing state: the explicit state directory, its owned entries, foreign entries",
    },
];

/// The registered codes, grouped by area in [`AREAS`] order and
/// ascending by number within an area; each code is defined in exactly
/// one area.
pub const CODES: &[CodeDefinition] = &[
    // CLI
    CodeDefinition {
        code: "KDLITH-CLI001",
        label: "invalid-argument",
        severity: Severity::Error,
        exit_class: 2,
        trigger: "an argument or option value fails its declared form",
        message_template: "invalid value <value> for <option>: <reason>",
        correction: "Pass a value of the documented form; see the command's --help",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-CLI002",
        label: "conflicting-options",
        severity: Severity::Error,
        exit_class: 2,
        trigger: "two options that exclude each other are both given",
        message_template: "<option> conflicts with <other>",
        correction: "Drop one of the conflicting options",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-CLI003",
        label: "unknown-format",
        severity: Severity::Error,
        exit_class: 2,
        trigger: "export --format names an unsupported encoding",
        message_template: "unknown format <format>; expected one of <values>",
        correction: "Choose one of the listed formats",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-CLI004",
        label: "inapplicable-view",
        severity: Severity::Error,
        exit_class: 2,
        trigger: "inspect --view names a view that does not apply to the inspected target",
        message_template: "the <view> view does not apply to a <target> target",
        correction: "Choose a view the target supports; see the command's --help",
        retired: false,
    },
    // SRC
    CodeDefinition {
        code: "KDLITH-SRC001",
        label: "source-unreadable",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a named or imported document cannot be read",
        message_template: "cannot read <path>: <reason>",
        correction: "Check that the path exists and is readable",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SRC002",
        label: "source-not-utf8",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a document is not valid UTF-8",
        message_template: "<path> is not valid UTF-8 at byte <offset>",
        correction: "Re-encode the document as UTF-8",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SRC003",
        label: "source-limit-exceeded",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a configured source, token, string, nesting, record, or invocation limit is exceeded before allocation",
        message_template: "<limit> limit <max> exceeded",
        correction: "Read the documents in smaller batches or raise the invocation limit (--max-invocations); make a document past a size or nesting limit smaller",
        retired: false,
    },
    // SYN
    CodeDefinition {
        code: "KDLITH-SYN001",
        label: "invalid-syntax",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "the document is not valid KDL",
        message_template: "invalid KDL: <reason>",
        correction: "Fix the syntax at the reported span",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SYN002",
        label: "unterminated-construct",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a string, comment, or children block reaches end of input unterminated",
        message_template: "unterminated <construct>",
        correction: "Close the construct opened at the reported span",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SYN003",
        label: "duplicate-property",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a node carries the same decoded property name twice where the binding forbids it",
        message_template: "property <name> given more than once",
        correction: "Keep one occurrence of the property",
        retired: false,
    },
    // IMP
    CodeDefinition {
        code: "KDLITH-IMP001",
        label: "import-unresolved",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "an import request names no resolvable document",
        message_template: "import <target> does not resolve: <reason>",
        correction: "Point the import at an existing document",
        retired: true,
    },
    CodeDefinition {
        code: "KDLITH-IMP002",
        label: "import-cycle",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "the import graph contains a cycle",
        message_template: "import cycle: <cycle>",
        correction: "Break the cycle by removing one of the imports",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-IMP003",
        label: "import-repeated",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a source already invoked in the session is imported again by a request whose repeated policy is reject",
        message_template: "<target> is already imported in this session",
        correction: "Import the source once, or declare repeated=coalesce or repeated=instantiate on the request",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-IMP004",
        label: "import-expression-not-admitted",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "an import expression uses syntax the active discovery grammar does not admit, including any environment-variable interpolation, which no grammar admits",
        message_template: "import <expression> is not admitted: <reason>",
        correction: "Use only the pattern syntax and anchors the grammar declares, or import a literal path",
        retired: true,
    },
    CodeDefinition {
        code: "KDLITH-IMP005",
        label: "import-anchor-unavailable",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "an import expression names an anchor the active discovery grammar does not declare, or declares but the host does not supply; or the host supplies an anchor no discovery grammar of the definition declares",
        message_template: "anchor <anchor> is unavailable: <reason>",
        correction: "Name an anchor the grammar declares and the host supplies, or import a literal path",
        retired: true,
    },
    CodeDefinition {
        code: "KDLITH-IMP006",
        label: "import-outside-entry",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a data document's import resolves, after symbolic links, to a path outside the directory of the entry document the reading began from; it is refused before the target is read",
        message_template: "import <expression> (<shown>) resolves outside the directory of entry document <entry>",
        correction: "Keep every document the data imports inside the entry document's directory",
        retired: true,
    },
    CodeDefinition {
        code: "KDLITH-IMP007",
        label: "import-outside-alias",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "an import addressed through a host-registered import alias, or made by a document such an alias loaded, resolves outside that alias's root, lexically or after symbolic links; it is refused before the target is read",
        message_template: "import <expression> resolves outside the root of import alias <alias>",
        correction: "Keep every document an import alias reaches inside the alias's root",
        retired: true,
    },
    CodeDefinition {
        code: "KDLITH-IMP008",
        label: "import-alias-invalid",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a host registers an import alias whose name is malformed, registered twice, or also an anchor the host supplies or a discovery grammar of the definition declares, or whose root does not resolve to a directory or holds a document under a key that is not a normalized relative path; it is refused before any document is read",
        message_template: "import alias <alias> is invalid: <reason>",
        correction: "Register each import alias once, under a kebab name no anchor uses, with a root that exists",
        retired: true,
    },
    // LOC
    CodeDefinition {
        code: "KDLITH-LOC001",
        label: "location-unresolved",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "an import location names no document: no such file, not a file, a named source holding no such file, a location naming a source's root, or a pattern matching nothing where the governing locations rule does not allow an empty match",
        message_template: "location <location> does not resolve: <reason>",
        correction: "Point the location at an existing document, or allow an empty match in the locations rule",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-LOC002",
        label: "location-outside-source",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "an import location resolves outside the source it was resolved against, lexically or after symbolic links: the project root (or an out-of-root entry document's own directory) for an unprefixed location in the local source, the named source's root for $name/ and for every location a document of that source writes; it is refused before the target is read",
        message_template: "location <location> resolves outside <source>",
        correction: "Keep every location inside its source; reach another tree through a named source ($name/PATH)",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-LOC003",
        label: "source-unknown",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a location's $name/ prefix names no source the reading registered",
        message_template: "location <location> names unknown source <source>: <reason>",
        correction: "Register the source (--source NAME=DIR, or a source the host provides), or correct its name",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-LOC004",
        label: "location-syntax-not-admitted",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a location uses syntax its governing locations rule does not admit: a pattern kind the rule does not enable, environment-variable interpolation (which no rule admits), a malformed $name, a $name with no path, a backslash, or a .. after a pattern segment",
        message_template: "location <location> is not admitted: <reason>",
        correction: "Write a literal location, or enable the pattern in the schema's or profile's locations rule",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-LOC005",
        label: "source-declaration-invalid",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "the project root or a source the host registers is invalid: a name that is malformed or registered twice, a directory that does not resolve or is not a directory, or an embedded file key that is not a normalized relative path; it is refused before any document is read",
        message_template: "source <source> is invalid: <reason>",
        correction: "Register each source once under a kebab name, backed by a directory that exists or by normalized embedded keys",
        retired: false,
    },
    // DLC
    CodeDefinition {
        code: "KDLITH-DLC001",
        label: "unknown-dialect-construct",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "retired: a node no dialect binding accepts in its position is KDLITH-BND001 on every reading path",
        message_template: "unknown dialect construct <node>",
        correction: "Use a construct the dialect defines",
        retired: true,
    },
    CodeDefinition {
        code: "KDLITH-DLC002",
        label: "duplicate-dialect-construct",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a dialect definition declares the same type, node, or slot twice; a repeated key of a keyed collection such as an object's fields is KDLITH-BND006",
        message_template: "dialect construct <name> defined more than once",
        correction: "Keep one definition of the construct",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC003",
        label: "deferred-mechanism",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "retired: no mechanism is deferred at dialect version 0, so a spelling no binding accepts is KDLITH-BND001 on every reading path",
        message_template: "<construct> relies on deferred mechanism <mechanism>",
        correction: "Remove the construct; the dialect does not define this mechanism at this version",
        retired: true,
    },
    CodeDefinition {
        code: "KDLITH-DLC004",
        label: "definition-root-mismatch",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a definition document's root is the other kind of definition, or disagrees with the other contributing documents or the governing dialect; a missing, extra, or unknown root node is KDLITH-BND001 or KDLITH-BND002",
        message_template: "<path>: <reason>",
        correction: "Give every contributing document exactly one root with the same kind, name, and version",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC005",
        label: "malformed-construct",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a dialect construct breaks a rule its dialect's declarations cannot state: an uncompilable pattern, a shadowed built-in, a contradictory option, or a list that repeats a name; what the declarations state is checked with the binding codes",
        message_template: "malformed <construct>: <reason>",
        correction: "Spell the construct as the dialect declares it",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC006",
        label: "extension-not-installed",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a definition names a validator, context handler, or contribution resolver the processor does not install, or contribution records are routed to a resolver the schema's policy does not name",
        message_template: "<kind> <name> is not installed",
        correction: "Install the validator, handler, or resolver, or stop referencing it",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC007",
        label: "invalid-type-expression",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a type expression, or any value the type-expression validator checks, does not follow the type-expression grammar",
        message_template: "invalid type expression <expression>: <reason>",
        correction: "Write a declared type, a built-in scalar, or a constructor applied to its arguments",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC008",
        label: "unresolved-dialect-reference",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a dialect definition references a type, node, field, slot, namespace, or profile it does not declare, or uses a construct its own declarations do not describe",
        message_template: "<kind> <name> is not declared",
        correction: "Declare it in a contributing document, or fix the reference",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC009",
        label: "dialect-literal-type-mismatch",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a literal in a dialect definition (a fallback, a documented default, or a set value) does not decode as its declared type",
        message_template: "<owner>: literal <literal> does not decode as <type>",
        correction: "Write a literal of the declared type, or change the type",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC010",
        label: "resolution-failed",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "the registered contribution resolver refuses the records a reading preserved under the schema's declared policy: a target contributed twice in one layer under duplicates reject, a referenced target no record contributes under missing reject, or a record in a layer the policy does not declare",
        message_template: "contribution resolution failed: <reason>",
        correction: "Contribute each target once per layer, contribute every referenced target, and use only declared layers; or relax the policy",
        retired: false,
    },
    // SCH
    CodeDefinition {
        code: "KDLITH-SCH001",
        label: "unknown-type",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a schema definition references a type no linked schema declares",
        message_template: "unknown type <type>",
        correction: "Declare the type or import the schema that declares it",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH002",
        label: "duplicate-type",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "two schema definitions declare the same type identity",
        message_template: "type <type> declared more than once",
        correction: "Rename or remove one declaration",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH003",
        label: "conflicting-binding",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "two bindings map the same KDL syntax to different logical fields",
        message_template: "<syntax> is bound to both <field> and <other>",
        correction: "Bind the syntax to one field",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH004",
        label: "duplicate-declaration",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "contributing schema documents declare the same node, slot, or document rule twice; a repeated key of a keyed collection such as an object's fields is KDLITH-BND006",
        message_template: "<declaration> declared more than once",
        correction: "Keep one declaration; contributions are rejected, not merged",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH005",
        label: "invalid-node-binding",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a node declaration's bindings, child rules, or groups contradict each other or leave an argument unconsumed",
        message_template: "node <node>: <reason>",
        correction: "Bind every argument, property, and child exactly once and consistently",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH006",
        label: "unresolved-schema-reference",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a schema definition references a node, node group, field, slot, namespace, profile, or reference target it does not declare",
        message_template: "<kind> <name> is not declared",
        correction: "Declare it in a contributing document, or fix the reference",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH007",
        label: "children-effect-not-streamable",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a children effect reads a field bound from a child, so it cannot be computed from the node header",
        message_template: "node <node>: children effect on <slot> reads field <field>, which is bound from a child",
        correction: "Bind the field from an argument, property, or context, or apply the effect to following siblings",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH008",
        label: "schema-literal-type-mismatch",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a literal in a schema definition (a fallback, a documented default, or a set value) does not decode as its declared type",
        message_template: "<owner>: literal <literal> does not decode as <type>",
        correction: "Write a literal of the declared type, or change the type",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH009",
        label: "expansion-or-instantiation-cycle",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a cycle in a definition's declaration-expansion or generic-instantiation graph, so a member group or a monomorphized type would include or instantiate itself",
        message_template: "cycle through <cycle>: expansion and instantiation must terminate",
        correction: "Remove one include or type argument so the cycle breaks",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH010",
        label: "ambiguous-dispatch-prefix",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "variant prefixes of one union where one is a prefix of another, so value-prefix dispatch cannot select a variant deterministically",
        message_template: "prefix <prefix> of variant <variant> is extended by prefix <other> of variant <earlier>",
        correction: "Give every variant a prefix no other variant's prefix extends",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH011",
        label: "profile-not-selected",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a schema that declares profiles is invoked with no profile named, no declared match rule matching the source path, and no default profile; with a profile set no member's rules match and no member is the default; or with a name none of its profiles declares; selection reads invocation context, never document content",
        message_template: "invocation profile <profile> is not one of the profiles schema <schema> declares",
        correction: "Name one of the schema's declared profiles at invocation, or declare a match rule or a default profile that selects one",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH012",
        label: "profile-inheritance-cycle",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a profile's extends chain returns to a profile already on it, including a profile extending itself",
        message_template: "profile inheritance cycle through <cycle>",
        correction: "Remove one extends so every chain ends at a profile that extends none",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH013",
        label: "second-default-profile",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a second profile of one schema is marked default; the first in contribution order keeps the mark",
        message_template: "<profile> is marked default, but <earlier> already is",
        correction: "Mark at most one profile of the schema default=#true",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH014",
        label: "fallback-identity-not-string",
        severity: Severity::Note,
        exit_class: 0,
        trigger: "a declaration without an identity block takes its identity from argument 0's field, and the field's type admits values that are not strings, which binding rejects as KDLITH-BND003",
        message_template: "declaration <node> takes its identity from argument 0's field <field>, whose type <type> admits values that are not strings",
        correction: "Type the field as a string, or give the declaration an identity block",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH015",
        label: "empty-node-group",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a node group is declared with zero members, so no children-group rule or parents group reference expands to a name",
        message_template: "node group <name> declares no members",
        correction: "Name at least one declared node as a member",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH016",
        label: "invalid-condition-property",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a when condition gates a children rule on a property whose declared type is not a scalar or an enum",
        message_template: "node <node>: condition property <field> holds type <type>, which no condition gates on",
        correction: "Gate on a scalar or enum property",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH017",
        label: "overlapping-when-branches",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "two when branches of one children rule cannot be proven never to match the same node, so their bounds could both apply",
        message_template: "node <node>: children rule for <child> has overlapping when branches",
        correction: "Make the branches provably disjoint: give each a required leaf on one shared property with distinct values, or compose them into one branch",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH018",
        label: "empty-when-composition",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a when-all or when-any combinator declares no member conditions",
        message_template: "node <node>: <combinator> composes no conditions",
        correction: "Name at least one when or nested combinator",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-SCH019",
        label: "fallback-less-when-rule",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a children rule carries when branches but spells no base min-occurs/max-occurs, so no bounds are declared for the instances no branch matches",
        message_template: "node <node>: children rule for <child> carries when branches without base bounds",
        correction: "Spell the rule's base min-occurs/max-occurs; they are the bounds when no branch matches",
        retired: false,
    },
    // BND
    CodeDefinition {
        code: "KDLITH-BND001",
        label: "unknown-node",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a data document, or a definition document read as data of its dialect, contains a node no binding accepts in its position",
        message_template: "unknown node <node> under <parent>",
        correction: "Remove the node or use one the schema binds here",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND002",
        label: "cardinality-violation",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a field or child occurs fewer or more times than its binding allows",
        message_template: "<field> occurs <count> times; expected <expected>",
        correction: "Adjust the occurrences to the declared cardinality",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND003",
        label: "value-type-mismatch",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a value fails to decode as its field's declared type; a value outside an enum is KDLITH-BND008, and one the type-expression validator rejects is KDLITH-DLC007",
        message_template: "<field> expects <type>, found <found>",
        correction: "Supply a value of the declared type",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND004",
        label: "missing-required-field",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a required field is absent from a bound node",
        message_template: "<node> is missing required <field>",
        correction: "Add the required field",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND005",
        label: "unbound-entry",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a node carries an argument, property, or type annotation no binding consumes",
        message_template: "<entry> of <node> is not bound",
        correction: "Remove it, or bind it in the schema",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND006",
        label: "duplicate-key",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a keyed envelope repeats a map key under the duplicate-key policy reject, such as two fields of one object or two members of one enum",
        message_template: "key <key> of <field> given more than once",
        correction: "Keep one entry per key",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND007",
        label: "source-collision",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "more than one binding source supplies a field whose collision policy is reject",
        message_template: "<field> of <node> is supplied more than once",
        correction: "Supply the field through one source",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND008",
        label: "unknown-enum-member",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a value names no member of its field's enum type",
        message_template: "<field> expects a member of <type>, found <found>",
        correction: "Use one of the enum's members",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND009",
        label: "unresolved-value",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a bound node's field is absent, its declared fallback is computed, and the value resolver the consuming program registered declines to resolve it; a resolved value that fails the field's type is KDLITH-BND003",
        message_template: "<field> of <node> could not be resolved: <reason>",
        correction: "Supply the value through the registered resolver's source, or write it in the document",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND010",
        label: "unsupported-map-key",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "an entries envelope's key decodes, under its map's declared key type, to a value that is neither a string nor a reference, so no map entry can carry it; a key the type rejects is KDLITH-BND003",
        message_template: "key <key> of <field> decodes under <type> to <found>, which no map key holds",
        correction: "Declare the map's key type as a string type, a codec yielding a string, or a reference type",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND011",
        label: "object-constraint-violated",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a bound node breaks an object constraint of a type it binds as: requires F G with F holding a value and G none, or all-or-none with some but not all of its fields holding one; #null and an empty collection are values, only a field holding none is absent",
        message_template: "<node> violates <constraint>: <field> is supplied but <missing> is absent",
        correction: "Supply the absent field, or remove the field that requires it",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND012",
        label: "bound-violated",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a bound node's numeric field holds a number outside the inclusive range a bound constraint of a type it binds as declares; a list field is checked per element, and an absent field is not checked",
        message_template: "<node> violates <constraint>: <field> holds <found>, outside the bound",
        correction: "Write a value within the bound, or widen the bound",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND013",
        label: "exclusivity-violated",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a bound node holds values for more than one field an at-most-one or exactly-one-field constraint of a type it binds as lists, or for none of an exactly-one-field list; presence is judged after defaults, context, and resolved values fill the node, and #null is a value",
        message_template: "<node> violates <constraint>: fields holding a value: <found>",
        correction: "Supply exactly the permitted number of the listed fields",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND014",
        label: "off-type-field",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a node that a cases dispatch binds as an object type holds a value for a field that type does not declare, however the field came to hold it; the case selector itself is always admitted",
        message_template: "<node> binds <field>, which <type> does not declare",
        correction: "Remove the field, or spell the case the type belongs under",
        retired: false,
    },
    // CTX
    CodeDefinition {
        code: "KDLITH-CTX001",
        label: "context-unavailable",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a binding reads a context value no enclosing control provides",
        message_template: "context <name> is not available here",
        correction: "Nest the node under a control that provides the context",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-CTX002",
        label: "context-disagreement",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "an explicit value disagrees with the context slot a field reads under policy fixed",
        message_template: "<field> is <value> but context <slot> holds <held>",
        correction: "Remove the explicit value, or make it agree with the context",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-CTX003",
        label: "accumulation-without-operation",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "retired: a second value reaching any occupied slot that does not accumulate is decided by the slot's declared duplicate policy, and its reject arm is KDLITH-CTX006",
        message_template: "context <slot> receives a second value with no declared operation to combine or replace it",
        correction: "Declare accumulate append or merge on the slot, or remove the earlier value before setting it again",
        retired: true,
    },
    CodeDefinition {
        code: "KDLITH-CTX004",
        label: "snapshot-lineage-mismatch",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a context snapshot restore whose schema fingerprint or invocation lineage does not match the requesting context",
        message_template: "snapshot <snapshot> cannot restore here: <reason>",
        correction: "Restore within the invocation the snapshot was taken in, or re-scan the document under the current schema",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-CTX005",
        label: "handler-effect-undeclared",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "an installed context handler, run at its effect point, sets or removes a context slot or clears a context namespace the definition does not declare; a declarative effect naming one is an undeclared reference at compile time",
        message_template: "handler <handler> <effect> <name>, which the definition does not declare",
        correction: "Declare the slot or namespace in the schema's context block, or install a handler that addresses only declared ones",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-CTX006",
        label: "slot-duplicate-rejected",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a set reaches an occupied slot that does not accumulate and the slot's declared duplicate policy rejects it: reject, the default, rejects every second value; layered rejects a second value within one layer, and a set meeting a held value without a layer or carrying none",
        message_template: "context <slot> under duplicates=<policy> receives a second value from <incoming> while it holds one from <held>",
        correction: "Remove the held value before setting the slot again, or declare duplicates=last-wins or duplicates=layered on the slot",
        retired: false,
    },
    // REF
    CodeDefinition {
        code: "KDLITH-REF001",
        label: "unresolved-reference",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a reference names no declaration in the processed graph",
        message_template: "reference <name> does not resolve",
        correction: "Declare the target or import the document that declares it",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-REF002",
        label: "duplicate-declaration",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "two declarations claim the same symbol identity",
        message_template: "<name> declared more than once",
        correction: "Rename or remove one declaration",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-REF003",
        label: "ambiguous-reference",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a reference's qualification and lookup policy derive more than one candidate declaration",
        message_template: "reference <name> is ambiguous between <candidates>",
        correction: "Qualify the reference so its lookup policy derives exactly one declaration",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-REF004",
        label: "unmet-qualifier",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a reference resolves to one declaration that fails a required qualifier its reference type declares",
        message_template: "reference <name> does not meet the required qualifier <qualifier>: <reason>",
        correction: "Spell the required qualifier with the value its target declares, or reference a declaration that carries it",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-REF005",
        label: "pending-payload-type",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a pending typed binding whose payload fails its resolved declaration's member type at complete scope",
        message_template: "pending payload of <node> under codec <codec> does not match <expected>: found <found>",
        correction: "Write the payload as the referenced declaration's member type declares it: a list member takes zero or more values, any other member exactly one, and an optional member none or one",
        retired: false,
    },
    // GEN
    CodeDefinition {
        code: "KDLITH-GEN001",
        label: "unowned-output-path",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "generation would overwrite a file the output manifest does not own",
        message_template: "refusing to overwrite unowned <path>",
        correction: "Move the file aside or choose another --out directory",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-GEN002",
        label: "generated-tree-drift",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "--check finds missing, changed, or stale generator-owned files",
        message_template: "<path> is <state>",
        correction: "Re-run generation without --check",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-GEN003",
        label: "processor-definition-mismatch",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a typed processor's declared types, nodes, slots, or handlers disagree with the compiled definition it reads",
        message_template: "processor <processor> disagrees with its definition: <subject>: <reason>",
        correction: "Regenerate the processor from its definition, or bring its declarations in line",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-GEN004",
        label: "item-variant-collision",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "two reading nodes of a definition spell the same generated item variant for different items",
        message_template: "the nodes <first> and <second> both read as the item variant <variant>",
        correction: "Rename one of the nodes, or bind the nodes that share a type under names that spell distinct variants",
        retired: false,
    },
    // EXP
    CodeDefinition {
        code: "KDLITH-EXP001",
        label: "output-unwritable",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "the export destination cannot be written",
        message_template: "cannot write <path>: <reason>",
        correction: "Check the destination path and its permissions",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-EXP002",
        label: "provisional-output-incomplete",
        severity: Severity::Warning,
        exit_class: 0,
        trigger: "provisional delivery emitted items before processing failed",
        message_template: "<count> items were emitted provisionally; the stream is incomplete",
        correction: "Discard the partial stream, or use --delivery atomic",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-EXP003",
        label: "emit-type-unselectable",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "an export selects a type the schema does not declare, or one that is not a record type",
        message_template: "cannot emit <type>: <reason>",
        correction: "Name a record type the schema declares, or select roots",
        retired: false,
    },
    // CAC
    CodeDefinition {
        code: "KDLITH-CAC001",
        label: "foreign-state-entry",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a state directory holds an entry the cache does not own",
        message_template: "<path> is not a cache-owned entry; it was left in place",
        correction: "Move the entry out of the state directory, or name a directory only the cache writes",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-CAC002",
        label: "corrupt-state-entry",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a cache-owned entry fails its envelope check: another format version, an interrupted write, or damage",
        message_template: "cache entry <path> is unusable: <reason>",
        correction: "Run cache clean to remove it, then cache refresh to derive it again",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-CAC003",
        label: "state-inaccessible",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "the state directory or one of its entries cannot be read, created, written, or removed",
        message_template: "cannot <action> <path>: <reason>",
        correction: "Check the state path and its permissions",
        retired: false,
    },
];

/// Look up a registered code, retired or not.
pub fn lookup(code: &str) -> Option<&'static CodeDefinition> {
    CODES.iter().find(|definition| definition.code == code)
}

/// Look up an area by name.
pub fn area(name: &str) -> Option<&'static AreaDefinition> {
    AREAS.iter().find(|definition| definition.name == name)
}

/// Split a code into its area and number when it follows the numbering
/// scheme: the prefix, a registered area, and a number in
/// `1..=MAX_ALLOCATED_NUMBER`. Conformance to the scheme says nothing
/// about registration; use [`lookup`] for that.
pub fn parse_code(code: &str) -> Option<CodeParts<'_>> {
    let rest = code.strip_prefix(CODE_PREFIX)?;
    if rest.len() != 6 || !rest.is_ascii() {
        return None;
    }
    let (area_name, digits) = rest.split_at(3);
    area(area_name)?;
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number: u16 = digits.parse().ok()?;
    if number == 0 || number > MAX_ALLOCATED_NUMBER {
        return None;
    }
    Some(CodeParts {
        area: area_name,
        number,
    })
}

/// Exit class for a code: the registered class when registered; for an
/// unregistered code, an error fails with class 1 and lesser severities
/// never fail.
pub fn exit_class(code: &str, severity: Severity) -> u8 {
    match lookup(code) {
        Some(definition) => definition.exit_class,
        None => match severity {
            Severity::Error => 1,
            Severity::Warning | Severity::Note => 0,
        },
    }
}

/// Fill a message template's `<placeholder>` tokens in token order from
/// `values`. A template with more tokens than values keeps the leftover
/// tokens verbatim rather than inventing content.
pub fn render(template: &str, values: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    let mut next = values.iter();
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        let end = start + end + 1;
        out.push_str(&rest[..start]);
        match next.next() {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[start..end]),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}
