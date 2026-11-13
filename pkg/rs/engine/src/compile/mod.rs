//! The schema compiler: reader input in, one merged definition out.
//! The seed bootstrap (`compile_meta_schema`) reads syntax directly and
//! compiles only the canonical meta-schema; see context.internal/reference/40_engine_contract.md

mod annotations;
mod collect;
mod conditions;
mod conform;
mod document;
mod expansions;
mod feed;
mod groups;
pub mod input;
mod link;
mod literals;
mod mono;
mod nodes;
mod reader;
mod scopes;
mod types;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::Value;

use crate::bindings::NodeDecl;
use crate::compile::reader::Construct;
use crate::definition::{
    CheckScope, Definition, DefinitionKind, DocumentRule, ImportDecl, META_SCHEMA_NAME, SlotDecl,
};
use crate::diag::{
    CARDINALITY, Diagnostics, MALFORMED, MISSING_FIELD, NOT_INSTALLED, RESERVED_NAME,
    ROOT_MISMATCH, Site, TYPE_EXPRESSION, UNKNOWN_MEMBER, UNKNOWN_NODE, VALUE_TYPE,
};
use crate::extensions::{Extensions, TYPE_EXPRESSION_VALIDATOR, TYPE_EXPRESSION_VERSION};
use crate::names::Names;
use crate::plan;
use crate::symbols::SymbolCatalog;
use crate::syntax::{ParsedDocument, SyntaxNode};
use crate::type_expr::{self, ExprError};
use crate::types::{TypeCatalog, TypeExpr};

use input::DefinitionInput;

/// The meta-schema version this engine compiles definitions against.
pub const META_SCHEMA_VERSION: &str = "0";

/// Compile a meta-schema definition from a reader's input, with the given
/// checking scope. This is the path every definition takes: the
/// reference processor's typed reader produced the input, and its
/// binding diagnostics were reported before compilation began.
pub fn compile_meta_schema_input(
    input: &DefinitionInput,
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    compile_input(DefinitionKind::MetaSchema, input, None, extensions, scope)
}

/// Compile a schema definition from a reader's input, under its
/// governing meta-schema, with the given checking scope.
pub fn compile_schema_input(
    meta_schema: &Definition,
    input: &DefinitionInput,
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    compile_input(
        DefinitionKind::Schema,
        input,
        Some(meta_schema),
        extensions,
        scope,
    )
}

/// Compile a meta-schema definition straight from its documents, with
/// complete checking. This is the seed bootstrap: it exists only to
/// compile the canonical meta-schema a typed reader is constructed over,
/// because reading that meta-schema's own documents through a reader
/// already needs a compiled meta-schema.
pub fn compile_meta_schema(
    documents: &[ParsedDocument],
    extensions: &Extensions,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let mut cx = start(
        DefinitionKind::MetaSchema,
        CheckScope::Complete,
        documents,
        extensions,
        None,
    )?;
    let Some(root) = roots(&mut cx, documents) else {
        return Err(cx.diags.into_records());
    };
    for &(doc, node) in &root.nodes {
        root_children(&mut cx, doc, node);
    }
    finish(cx, documents, None)
}

fn compile_input(
    kind: DefinitionKind,
    input: &DefinitionInput,
    governing: Option<&Definition>,
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let mut parsed = Vec::with_capacity(input.documents.len());
    for document in &input.documents {
        match ParsedDocument::parse(document.path.clone(), &document.text) {
            Ok(document) => parsed.push(document),
            Err(record) => return Err(vec![*record]),
        }
    }
    let documents = &parsed[..];
    let mut cx = start(kind, scope, documents, extensions, governing)?;
    if roots(&mut cx, documents).is_none() {
        return Err(cx.diags.into_records());
    }
    collect::all(&mut cx, input);
    let mut definition = finish(cx, documents, governing)?;
    definition.notes.splice(0..0, input.notes.iter().cloned());
    Ok(definition)
}

/// Refuse a definition that takes the name reserved for the meta-schema
/// without being one. It stays admitted when one of its documents is a
/// document of `own`, the reading processor's embedded meta-schema, or
/// imports one by a literal path (an entrypoint read without following
/// its imports), or when it declares the node `schema` its own root is
/// written with, as every self-describing meta-schema does. Otherwise it
/// is a schema under the reserved name, refused before either compiler
/// reads it.
pub fn check_reserved_name(
    input: &DefinitionInput,
    own: &[&str],
) -> Result<(), Vec<DiagnosticRecord>> {
    let Some((document, span)) = input
        .documents
        .iter()
        .find_map(|document| reserved_root(&document.text).map(|span| (document, span)))
    else {
        return Ok(());
    };
    let owned = input
        .documents
        .iter()
        .any(|document| own.contains(&document.text.as_str()) || imports_own(document, own));
    let describes_root = input
        .documents
        .iter()
        .flat_map(|document| &document.constructs)
        .any(|located| match &located.item {
            input::Construct::Node(node) => {
                node.name == META_SCHEMA_NAME
                    || node.aliases.iter().any(|alias| alias == META_SCHEMA_NAME)
            }
            _ => false,
        });
    if owned || describes_root {
        return Ok(());
    }
    let at = kdlith_parse::locate(&document.text, span).start;
    let record = crate::diag::record(
        RESERVED_NAME,
        &[&document.path, META_SCHEMA_NAME],
        DefinitionKind::Schema.origin(),
    )
    .with_source(document.path.clone())
    .with_span(span, Some(at.line), Some(at.column));
    Err(vec![record])
}

/// Whether `document` imports a document of `own` by a literal path,
/// resolved beside the document as its imports resolve.
fn imports_own(document: &input::DocumentInput, own: &[&str]) -> bool {
    let dir = std::path::Path::new(&document.path).parent();
    document.constructs.iter().any(|located| {
        let input::Construct::Import(import) = &located.item else {
            return false;
        };
        if !crate::locations::is_literal(&import.path) {
            return false;
        }
        let target = dir.map_or_else(
            || std::path::PathBuf::from(&import.path),
            |dir| dir.join(&import.path),
        );
        let small = std::fs::metadata(&target)
            .is_ok_and(|meta| meta.len() <= crate::documents::MAX_DOCUMENT_BYTES);
        small && std::fs::read_to_string(&target).is_ok_and(|text| own.contains(&text.as_str()))
    })
}

/// The span of a document's first root node when it is the schema named
/// schema; only the root's opening is read.
fn reserved_root(text: &str) -> Option<kdlith_diagnostic::ByteSpan> {
    use kdlith_parse::{Event, Parser, Value as Written};
    let mut parser = Parser::new(text);
    let opened = parser.next_event().ok()??;
    match opened.item {
        Event::Node { name, .. } if name == DefinitionKind::Schema.root_name() => {}
        _ => return None,
    }
    loop {
        match parser.next_event().ok()??.item {
            Event::Entry(entry) if entry.name.is_none() => {
                return matches!(entry.value, Written::String(name) if name == META_SCHEMA_NAME)
                    .then_some(opened.span);
            }
            Event::Entry(_) => {}
            _ => return None,
        }
    }
}

/// The state of one compilation.
pub(crate) struct Cx<'d> {
    pub(crate) kind: DefinitionKind,
    pub(crate) scope: CheckScope,
    pub(crate) diags: Diagnostics<'d>,
    pub(crate) names: Names,
    pub(crate) extensions: &'d Extensions,
    pub(crate) types: TypeCatalog,
    pub(crate) nodes: Vec<NodeDecl>,
    pub(crate) slots: Vec<SlotDecl>,
    pub(crate) imports: Vec<ImportDecl>,
    pub(crate) profiles: Vec<crate::definition::ProfileDecl>,
    /// The schema root's locations rule, when it declares one.
    pub(crate) locations: Option<crate::definition::LocationsDecl>,
    pub(crate) references: Vec<crate::definition::ReferenceDecl>,
    pub(crate) annotations: Vec<crate::definition::AnnotationDecl>,
    /// The node groups the schema declares, expanded into children
    /// rules and placements before scopes are built.
    pub(crate) groups: Vec<groups::GroupDecl>,
    pub(crate) resolution: Option<crate::definition::ResolutionPolicyDecl>,
    pub(crate) providers: Vec<crate::definition::ProviderDecl>,
    pub(crate) source_context: Option<crate::definition::SourceContextDecl>,
    pub(crate) document_rule: Option<DocumentRule>,
    /// The `include` markers read in type declarations, spliced by the
    /// complete-checking expansion pass.
    pub(crate) expansions: Vec<expansions::Pending>,
    /// The identity every contributing document repeats, read by `roots`.
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
}

/// The identity every contributing document repeats.
struct Root<'d> {
    name: String,
    version: Option<String>,
    nodes: Vec<(usize, &'d SyntaxNode)>,
}

fn start<'d>(
    kind: DefinitionKind,
    scope: CheckScope,
    documents: &'d [ParsedDocument],
    extensions: &'d Extensions,
    governing: Option<&Definition>,
) -> Result<Cx<'d>, Vec<DiagnosticRecord>> {
    let mut names = Names::default();
    let types = TypeCatalog::with_builtins(&mut names);
    let mut cx = Cx {
        kind,
        scope,
        diags: Diagnostics::new(documents, kind.origin()),
        names,
        extensions,
        types,
        nodes: Vec::new(),
        slots: Vec::new(),
        imports: Vec::new(),
        profiles: Vec::new(),
        locations: None,
        references: Vec::new(),
        annotations: Vec::new(),
        groups: Vec::new(),
        resolution: None,
        providers: Vec::new(),
        source_context: None,
        document_rule: None,
        expansions: Vec::new(),
        name: None,
        version: None,
    };
    if !extensions.has_validator(TYPE_EXPRESSION_VALIDATOR, Some(TYPE_EXPRESSION_VERSION)) {
        let wanted = format!("{TYPE_EXPRESSION_VALIDATOR} version {TYPE_EXPRESSION_VERSION}");
        cx.diags.unsited(NOT_INSTALLED, &["validator", &wanted]);
        return Err(cx.diags.into_records());
    }
    if let Some(meta_schema) = governing {
        if meta_schema.kind != DefinitionKind::MetaSchema
            || meta_schema.version.as_deref() != Some(META_SCHEMA_VERSION)
        {
            let reason = format!(
                "the governing definition must be a meta-schema at version {META_SCHEMA_VERSION}"
            );
            cx.diags
                .unsited(ROOT_MISMATCH, &[&meta_schema.name, &reason]);
            return Err(cx.diags.into_records());
        }
    }
    Ok(cx)
}

/// Complete a compilation: expansion resolution, monomorphization,
/// scopes, linking, literal checks, the plan, and the self-description
/// walk, then the merged definition.
fn finish<'d>(
    mut cx: Cx<'d>,
    documents: &'d [ParsedDocument],
    governing: Option<&Definition>,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let complete = cx.scope == CheckScope::Complete && !cx.diags.has_errors();
    if complete {
        expansions::run(&mut cx);
    }
    types::constraints(&mut cx);
    if complete && !cx.diags.has_errors() {
        mono::run(&mut cx);
    }
    collect::merge_policy(&mut cx);
    groups::expand(&mut cx);
    let bindings = scopes::build(&mut cx);
    // Before the symbol catalog snapshots them: the pass anchors every
    // open-world arm the references declare, so the catalog and the
    // definition agree on the arm the link stage reads.
    collect::open_world_arms(&mut cx, &bindings);
    let symbols = SymbolCatalog::build(&bindings, &cx.references);
    source_context_check(&mut cx);
    collect::qualify_from_slots(&mut cx, &bindings, &symbols);
    collect::sibling_qualifiers(&mut cx, &bindings, &symbols);
    collect::derived_sources(&mut cx, &bindings, &symbols);
    if cx.scope == CheckScope::Complete && !cx.diags.has_errors() {
        link::run(&mut cx, &bindings, &symbols);
        if !cx.diags.has_errors() {
            literals::run(&mut cx, &bindings);
            annotations::run(&mut cx, &bindings);
            conditions::run(&mut cx, &bindings);
            feed::run(&mut cx, &bindings);
        }
    }
    if cx.diags.has_errors() {
        return Err(cx.diags.into_records());
    }
    let plan = plan::build(&cx.slots, &cx.types, &bindings, &symbols);
    let definition = Definition {
        kind: cx.kind,
        name: cx.name.clone().unwrap_or_default(),
        version: cx.version.clone(),
        documents: documents.iter().map(|d| d.path().to_owned()).collect(),
        texts: documents.iter().map(|d| d.text().to_owned()).collect(),
        names: cx.names,
        types: cx.types,
        bindings,
        symbols,
        plan,
        slots: cx.slots,
        imports: cx.imports,
        profiles: cx.profiles,
        locations: cx.locations,
        references: cx.references,
        annotations: cx.annotations,
        resolution: cx.resolution,
        providers: cx.providers,
        source_context: cx.source_context,
        document_rule: cx.document_rule,
        scope: cx.scope,
        notes: cx.diags.take_notes(),
        runtime: crate::bind::RuntimeCell::default(),
    };
    if cx.scope == CheckScope::Complete {
        let describing = governing.unwrap_or(&definition);
        conform::run(&mut cx.diags, cx.kind, describing, documents);
        if cx.diags.has_errors() {
            return Err(cx.diags.into_records());
        }
    }
    Ok(definition)
}

/// Read each document's single root and check every document repeats the
/// first one's kind, name, and version.
fn roots<'d>(cx: &mut Cx<'d>, documents: &'d [ParsedDocument]) -> Option<Root<'d>> {
    let expected = cx.kind.root_name();
    if documents.is_empty() {
        cx.diags.unsited(
            ROOT_MISMATCH,
            &["(no documents)", "a definition needs at least one document"],
        );
        return None;
    }
    let mut root: Option<Root<'d>> = None;
    let mut ok = true;
    for (doc, document) in documents.iter().enumerate() {
        let nodes = document.nodes();
        let Some(node) = nodes.first() else {
            let site = Site::new(doc, kdlith_diagnostic::ByteSpan::new(0, 0));
            cx.cardinality(
                site,
                "the first node of the document root",
                "0",
                "exactly 1",
            );
            ok = false;
            continue;
        };
        for extra in &nodes[1..] {
            let site = Site::new(doc, extra.span);
            let place = "the document root after its first node";
            cx.diags.error(UNKNOWN_NODE, site, &[&extra.name, place]);
            ok = false;
        }
        if node.name != expected {
            let site = Site::new(doc, node.span);
            cx.unknown(doc, node, "the document root");
            cx.cardinality(
                site,
                "the first node of the document root",
                "0",
                "exactly 1",
            );
            ok = false;
            continue;
        }
        let declared = match node.args.first().map(|arg| &arg.value) {
            Some(Value::String(name)) => Some(DefinitionKind::of_name(name)),
            _ => None,
        };
        if let Some(declared) = declared
            && declared != cx.kind
        {
            let reason = format!(
                "expected {}, found {}",
                kind_phrase(cx.kind),
                kind_phrase(declared)
            );
            cx.diags.error(
                ROOT_MISMATCH,
                Site::new(doc, node.span),
                &[document.path(), &reason],
            );
            ok = false;
            continue;
        }
        if cx.kind == DefinitionKind::MetaSchema {
            ok &= meta_children(cx, doc, node);
        }
        let mut construct = Construct::new(cx, doc, node);
        let name = construct.string_arg(cx, 0, "name");
        let version = construct.string_prop(cx, "version");
        construct.finish(cx);
        let Some(name) = name else {
            ok = false;
            continue;
        };
        if version.is_none() {
            cx.missing(Site::new(doc, node.span), expected, "version");
            ok = false;
        }
        cx.kebab(
            Site::new(doc, node.span),
            &format!("{expected} {name}"),
            &name,
            "name",
        );
        match &mut root {
            None => {
                cx.name = Some(name.clone());
                cx.version = version.clone();
                root = Some(Root {
                    name,
                    version,
                    nodes: vec![(doc, node)],
                });
            }
            Some(first) => {
                if first.name != name || first.version != version {
                    let reason = format!(
                        "root {expected} {name} version {} differs from {} version {}",
                        version.as_deref().unwrap_or("(none)"),
                        first.name,
                        first.version.as_deref().unwrap_or("(none)"),
                    );
                    cx.diags.error(
                        ROOT_MISMATCH,
                        Site::new(doc, node.span),
                        &[document.path(), &reason],
                    );
                    ok = false;
                }
                first.nodes.push((doc, node));
            }
        }
    }
    if ok { root } else { None }
}

/// How a root mismatch names a definition kind.
fn kind_phrase(kind: DefinitionKind) -> &'static str {
    match kind {
        DefinitionKind::MetaSchema => "the meta-schema root schema schema",
        DefinitionKind::Schema => "a schema root",
    }
}

/// The meta-schema root admits only the constructs the meta-schema
/// describes itself with; a schema-only construct under it is an unknown
/// node, as binding reports one. Whether every child was admitted.
fn meta_children(cx: &mut Cx<'_>, doc: usize, root: &SyntaxNode) -> bool {
    let mut ok = true;
    for child in &root.children {
        if !META_CHILDREN.contains(&child.name.as_str()) {
            cx.unknown(doc, child, &format!("{} {}", root.name, META_SCHEMA_NAME));
            ok = false;
        }
    }
    ok
}

/// The children a meta-schema root may hold.
const META_CHILDREN: [&str; 5] = ["types", "node", "context", "document", "import"];

/// Every source-context field names a declared slot: the derived value
/// lands in that slot, validated against both the field's and the slot's
/// declared types. A field naming no declared slot is rejected loudly,
/// never silently dropped.
fn source_context_check(cx: &mut Cx<'_>) {
    let Some(decl) = &cx.source_context else {
        return;
    };
    let site = decl.site;
    for field in &decl.fields {
        if !cx.slots.iter().any(|slot| slot.name == field.name) {
            let spelled = cx.names.resolve(field.name).to_owned();
            let reason = format!("source-context field {spelled} names no declared slot");
            cx.diags
                .error(MALFORMED, site, &["source-context", &reason]);
        }
    }
}

fn root_children(cx: &mut Cx<'_>, doc: usize, root: &SyntaxNode) {
    let mut context = None;
    let mut document = None;
    for child in &root.children {
        let site = Site::new(doc, child.span);
        match child.name.as_str() {
            "types" => types::container(cx, doc, child),
            "node" => nodes::declaration(cx, doc, child),
            "context" => {
                if cx.only_once(&mut context, site, &root.name, "context") {
                    document::context(cx, doc, child);
                }
            }
            "document" => {
                if cx.only_once(&mut document, site, &root.name, "document") {
                    document::rule(cx, doc, child);
                }
            }
            "import" => document::import(cx, doc, child),
            _ => cx.unknown(doc, child, &root.name),
        }
    }
}

impl Cx<'_> {
    /// A rule of a meta-schema construct its meta-schema's declarations cannot
    /// state. A rule they state is reported with its binding code, the
    /// one every processor binding the document reports.
    pub(crate) fn malformed(&mut self, site: Site, construct: &str, reason: &str) {
        self.diags.error(MALFORMED, site, &[construct, reason]);
    }

    /// A required field of a construct is absent.
    pub(crate) fn missing(&mut self, site: Site, construct: &str, field: &str) {
        self.diags.error(MISSING_FIELD, site, &[construct, field]);
    }

    /// A value does not decode as the type the meta-schema declares for it.
    pub(crate) fn mistyped(&mut self, site: Site, what: &str, expected: &str, found: &str) {
        self.diags.error(VALUE_TYPE, site, &[what, expected, found]);
    }

    /// A value names no member of the meta-schema enum `ty`.
    pub(crate) fn not_member(&mut self, site: Site, what: &str, ty: &str, value: &str) {
        let found = format!("string {value:?}");
        self.diags.error(UNKNOWN_MEMBER, site, &[what, ty, &found]);
    }

    /// A child occurs more or fewer times than its meta-schema rule allows.
    pub(crate) fn cardinality(&mut self, site: Site, what: &str, count: &str, expected: &str) {
        self.diags
            .error(CARDINALITY, site, &[what, count, expected]);
    }

    /// A child node no construct reading admits.
    pub(crate) fn unknown(&mut self, doc: usize, node: &SyntaxNode, parent: &str) {
        let site = Site::new(doc, node.span);
        self.diags.error(UNKNOWN_NODE, site, &[&node.name, parent]);
    }

    /// Track a child that may appear once; the second is malformed.
    pub(crate) fn only_once(
        &mut self,
        seen: &mut Option<Site>,
        site: Site,
        owner: &str,
        child: &str,
    ) -> bool {
        if seen.is_some() {
            let what = format!("{child} under {owner}");
            self.cardinality(site, &what, "2", "at most 1");
            return false;
        }
        *seen = Some(site);
        true
    }

    /// Compile a type expression with the `type-expression` validator.
    pub(crate) fn type_expr(&mut self, site: Site, text: &str) -> Option<TypeExpr> {
        match type_expr::parse(text, &mut self.names) {
            Ok(expr) => Some(expr),
            Err(ExprError::Syntax(reason)) => {
                self.diags.error(TYPE_EXPRESSION, site, &[text, &reason]);
                None
            }
        }
    }

    /// Check a kebab-case name (node, slot, namespace, and root names).
    pub(crate) fn kebab(&mut self, site: Site, construct: &str, name: &str, what: &str) -> bool {
        if crate::types::StringFormat::KebabCase.matches(name, None) {
            return true;
        }
        let what = format!("{what} of {construct}");
        self.mistyped(
            site,
            &what,
            "a kebab-case name",
            &format!("string {name:?}"),
        );
        false
    }

    /// Check a type name: an uppercase ASCII letter, then letters and
    /// digits.
    pub(crate) fn type_name(&mut self, site: Site, construct: &str, name: &str) -> bool {
        if type_expr::is_type_name(name) {
            return true;
        }
        let what = format!("type name of {construct}");
        self.mistyped(site, &what, "TypeName", &format!("string {name:?}"));
        false
    }
}
