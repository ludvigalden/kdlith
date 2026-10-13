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
        scope: "import requests, resolution, cycles, the processed import graph",
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
        scope: "export delivery: output destinations, atomic and provisional delivery",
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
        label: "unknown-stdout-format",
        severity: Severity::Error,
        exit_class: 2,
        trigger: "--stdout-format or export --format names an unsupported encoding",
        message_template: "unknown format <format>; expected one of <values>",
        correction: "Choose one of the listed formats",
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
        trigger: "a configured source, token, string, nesting, or record limit is exceeded before allocation",
        message_template: "<limit> limit <max> exceeded",
        correction: "Split the document or raise the configured limit",
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
        message_template: "import <target> does not resolve",
        correction: "Point the import at an existing document",
        retired: false,
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
    // DLC
    CodeDefinition {
        code: "KDLITH-DLC001",
        label: "unknown-dialect-construct",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a dialect definition uses a node the dialect does not define",
        message_template: "unknown dialect construct <node>",
        correction: "Use a construct the dialect defines",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC002",
        label: "duplicate-dialect-construct",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a dialect definition defines the same construct twice",
        message_template: "dialect construct <name> defined more than once",
        correction: "Keep one definition of the construct",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC003",
        label: "deferred-mechanism",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a definition relies on a mechanism the dialect declares deferred",
        message_template: "<construct> relies on deferred mechanism <mechanism>",
        correction: "Remove the construct; the dialect does not define this mechanism at this version",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC004",
        label: "definition-root-mismatch",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a definition document lacks a single dialect or schema root, or its root disagrees with the other contributing documents",
        message_template: "<path>: <reason>",
        correction: "Give every contributing document exactly one root with the same kind, name, and version",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC005",
        label: "malformed-construct",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a dialect construct has missing, extra, or ill-typed arguments, properties, or children",
        message_template: "malformed <construct>: <reason>",
        correction: "Spell the construct as the dialect declares it",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC006",
        label: "extension-not-installed",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a definition names a validator or context handler the processor does not install",
        message_template: "<kind> <name> is not installed",
        correction: "Install the validator or handler, or stop referencing it",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC007",
        label: "invalid-type-expression",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a type expression does not follow the type-expression grammar",
        message_template: "invalid type expression <expression>: <reason>",
        correction: "Write a declared type, a built-in scalar, or a constructor applied to its arguments",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-DLC008",
        label: "unresolved-dialect-reference",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a dialect definition references a type, node, field, slot, or namespace it does not declare, or uses a construct its own declarations do not describe",
        message_template: "<kind> <name> is not declared",
        correction: "Declare it in a contributing document, or fix the reference",
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
        trigger: "contributing schema documents make the same non-type declaration twice",
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
        trigger: "a schema definition references a node, field, slot, namespace, or reference target it does not declare",
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
    // BND
    CodeDefinition {
        code: "KDLITH-BND001",
        label: "unknown-node",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a data document contains a node no binding accepts in its position",
        message_template: "unknown node <node> under <parent>",
        correction: "Remove the node or use one the schema binds here",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND002",
        label: "cardinality-violation",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a field occurs fewer or more times than its binding allows",
        message_template: "<field> occurs <count> times; expected <expected>",
        correction: "Adjust the occurrences to the declared cardinality",
        retired: false,
    },
    CodeDefinition {
        code: "KDLITH-BND003",
        label: "value-type-mismatch",
        severity: Severity::Error,
        exit_class: 1,
        trigger: "a value fails to decode as its field's declared type",
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
