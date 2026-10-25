//! The schema compiler: reader input in, one merged definition out.
//! The seed bootstrap (`compile_dialect`) reads syntax directly and
//! compiles only the canonical dialect; see context.internal/reference/40_engine_contract.md

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

use crate::bindings::NodeDecl;
use crate::compile::reader::Construct;
use crate::definition::{
    CheckScope, Definition, DefinitionKind, DocumentRule, ImportDecl, SlotDecl,
};
use crate::diag::{
    CARDINALITY, Diagnostics, MALFORMED, MISSING_FIELD, NOT_INSTALLED, ROOT_MISMATCH, Site,
    TYPE_EXPRESSION, UNKNOWN_MEMBER, UNKNOWN_NODE, VALUE_TYPE,
};
use crate::extensions::{Extensions, TYPE_EXPRESSION_VALIDATOR, TYPE_EXPRESSION_VERSION};
use crate::names::Names;
use crate::plan;
use crate::symbols::SymbolCatalog;
use crate::syntax::{ParsedDocument, SyntaxNode};
use crate::type_expr::{self, ExprError};
use crate::types::{TypeCatalog, TypeExpr};

use input::DefinitionInput;

/// The dialect version this engine compiles definitions against.
pub const DIALECT_VERSION: &str = "0";

/// Compile a dialect definition from a reader's input, with the given
/// checking scope. This is the path every definition takes: the
/// reference processor's typed reader produced the input, and its
/// binding diagnostics were reported before compilation began.
pub fn compile_dialect_input(
    input: &DefinitionInput,
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    compile_input(DefinitionKind::Dialect, input, None, extensions, scope)
}

/// Compile a schema definition from a reader's input, under its
/// governing dialect, with the given checking scope.
pub fn compile_schema_input(
    dialect: &Definition,
    input: &DefinitionInput,
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    compile_input(
        DefinitionKind::Schema,
        input,
        Some(dialect),
        extensions,
        scope,
    )
}

/// Compile a dialect definition straight from its documents, with
/// complete checking. This is the seed bootstrap: it exists only to
/// compile the canonical dialect a typed reader is constructed over,
/// because reading that dialect's own documents through a reader
/// already needs a compiled dialect.
pub fn compile_dialect(
    documents: &[ParsedDocument],
    extensions: &Extensions,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let mut cx = start(
        DefinitionKind::Dialect,
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
    if let Some(dialect) = governing {
        if dialect.kind != DefinitionKind::Dialect
            || dialect.version.as_deref() != Some(DIALECT_VERSION)
        {
            let reason =
                format!("the governing definition must be a dialect at version {DIALECT_VERSION}");
            cx.diags.unsited(ROOT_MISMATCH, &[&dialect.name, &reason]);
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
    let symbols = SymbolCatalog::build(&bindings, &cx.references);
    source_context_check(&mut cx);
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
        if !matches!(node.name.as_str(), "dialect" | "schema") {
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
        if node.name != expected {
            let reason = format!("expected a {expected} root, found {}", node.name);
            cx.diags.error(
                ROOT_MISMATCH,
                Site::new(doc, node.span),
                &[document.path(), &reason],
            );
            ok = false;
            continue;
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
    /// A rule of a dialect construct its dialect's declarations cannot
    /// state. A rule they state is reported with its binding code, the
    /// one every processor binding the document reports.
    pub(crate) fn malformed(&mut self, site: Site, construct: &str, reason: &str) {
        self.diags.error(MALFORMED, site, &[construct, reason]);
    }

    /// A required field of a construct is absent.
    pub(crate) fn missing(&mut self, site: Site, construct: &str, field: &str) {
        self.diags.error(MISSING_FIELD, site, &[construct, field]);
    }

    /// A value does not decode as the type the dialect declares for it.
    pub(crate) fn mistyped(&mut self, site: Site, what: &str, expected: &str, found: &str) {
        self.diags.error(VALUE_TYPE, site, &[what, expected, found]);
    }

    /// A value names no member of the dialect enum `ty`.
    pub(crate) fn not_member(&mut self, site: Site, what: &str, ty: &str, value: &str) {
        let found = format!("string {value:?}");
        self.diags.error(UNKNOWN_MEMBER, site, &[what, ty, &found]);
    }

    /// A child occurs more or fewer times than its dialect rule allows.
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
