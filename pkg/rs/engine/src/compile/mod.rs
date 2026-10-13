//! The schema compiler: contributing definition documents in, one merged
//! definition with its catalogs out. Compilation runs once per definition;
//! nothing downstream reads definition KDL again.

mod conform;
mod document;
mod link;
mod nodes;
mod reader;
mod scopes;
mod types;

use kdlith_diagnostic::DiagnosticRecord;

use crate::bindings::NodeDecl;
use crate::deferred::{Spelling, deferred_by};
use crate::definition::{
    CheckScope, Deferral, Definition, DefinitionKind, DocumentRule, ImportDecl, SlotDecl,
};
use crate::diag::{
    DEFERRED_MECHANISM, Diagnostics, MALFORMED, NOT_INSTALLED, ROOT_MISMATCH, Site,
    TYPE_EXPRESSION, UNKNOWN_CONSTRUCT,
};
use crate::extensions::{Extensions, TYPE_EXPRESSION_VALIDATOR, TYPE_EXPRESSION_VERSION};
use crate::names::Names;
use crate::plan;
use crate::symbols::SymbolCatalog;
use crate::syntax::{ParsedDocument, SyntaxNode};
use crate::type_expr::{self, ExprError};
use crate::types::{TypeCatalog, TypeExpr};

use reader::Construct;

/// The dialect version this engine compiles definitions against.
pub const DIALECT_VERSION: &str = "0";

/// A dialect and a schema definition compiled together.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Compiled {
    pub dialect: Definition,
    pub schema: Definition,
}

/// Compile a dialect definition from its contributing documents, with
/// complete checking.
pub fn compile_dialect(
    documents: &[ParsedDocument],
    extensions: &Extensions,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    compile_dialect_scoped(documents, extensions, CheckScope::Complete)
}

/// Compile a dialect definition with the given checking scope. Complete
/// checking links every reference and checks that the dialect describes
/// every construct its own documents use.
pub fn compile_dialect_scoped(
    documents: &[ParsedDocument],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    run(DefinitionKind::Dialect, documents, None, extensions, scope)
}

/// Compile a schema definition under its governing dialect, whose
/// deferrals decide which constructs fail closed, with complete checking.
pub fn compile_schema(
    dialect: &Definition,
    documents: &[ParsedDocument],
    extensions: &Extensions,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    compile_schema_scoped(dialect, documents, extensions, CheckScope::Complete)
}

/// Compile a schema definition with the given checking scope. Complete
/// checking links every reference and checks the documents against the
/// governing dialect's compiled bindings.
pub fn compile_schema_scoped(
    dialect: &Definition,
    documents: &[ParsedDocument],
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    run(
        DefinitionKind::Schema,
        documents,
        Some(dialect),
        extensions,
        scope,
    )
}

/// Compile a dialect, then a schema under it, with complete checking.
pub fn compile_pair(
    dialect: &[ParsedDocument],
    schema: &[ParsedDocument],
    extensions: &Extensions,
) -> Result<Compiled, Vec<DiagnosticRecord>> {
    let dialect = compile_dialect(dialect, extensions)?;
    let schema = compile_schema(&dialect, schema, extensions)?;
    Ok(Compiled { dialect, schema })
}

/// The state of one compilation.
pub(crate) struct Cx<'d> {
    pub(crate) kind: DefinitionKind,
    pub(crate) scope: CheckScope,
    pub(crate) diags: Diagnostics<'d>,
    pub(crate) names: Names,
    pub(crate) extensions: &'d Extensions,
    pub(crate) deferrals: Vec<Deferral>,
    pub(crate) types: TypeCatalog,
    pub(crate) nodes: Vec<NodeDecl>,
    pub(crate) slots: Vec<SlotDecl>,
    pub(crate) imports: Vec<ImportDecl>,
    pub(crate) document_rule: Option<DocumentRule>,
}

/// The identity every contributing document repeats.
struct Root<'d> {
    name: String,
    version: Option<String>,
    nodes: Vec<(usize, &'d SyntaxNode)>,
}

fn run(
    kind: DefinitionKind,
    documents: &[ParsedDocument],
    governing: Option<&Definition>,
    extensions: &Extensions,
    scope: CheckScope,
) -> Result<Definition, Vec<DiagnosticRecord>> {
    let mut names = Names::default();
    let types = TypeCatalog::with_builtins(&mut names);
    let mut cx = Cx {
        kind,
        scope,
        diags: Diagnostics::new(documents, kind.origin()),
        names,
        extensions,
        deferrals: Vec::new(),
        types,
        nodes: Vec::new(),
        slots: Vec::new(),
        imports: Vec::new(),
        document_rule: None,
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
        cx.deferrals.clone_from(&dialect.deferrals);
    }
    let Some(root) = roots(&mut cx, documents) else {
        return Err(cx.diags.into_records());
    };
    if kind == DefinitionKind::Dialect {
        for &(doc, node) in &root.nodes {
            document::deferrals(&mut cx, doc, node);
        }
    }
    for &(doc, node) in &root.nodes {
        root_children(&mut cx, doc, node);
    }
    let bindings = scopes::build(&mut cx);
    let symbols = SymbolCatalog::build(&bindings);
    if scope == CheckScope::Complete && !cx.diags.has_errors() {
        link::run(&mut cx, &bindings, &symbols);
    }
    if cx.diags.has_errors() {
        return Err(cx.diags.into_records());
    }
    let plan = plan::build(&cx.slots, &cx.types, &bindings, &symbols);
    let definition = Definition {
        kind,
        name: root.name,
        version: root.version,
        documents: documents.iter().map(|d| d.path().to_owned()).collect(),
        names: cx.names,
        types: cx.types,
        bindings,
        symbols,
        plan,
        slots: cx.slots,
        imports: cx.imports,
        deferrals: cx.deferrals,
        document_rule: cx.document_rule,
        scope,
    };
    if scope == CheckScope::Complete {
        let describing = governing.unwrap_or(&definition);
        conform::run(&mut cx.diags, kind, describing, documents);
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
            let reason = format!("expected one {expected} root, found an empty document");
            cx.diags.unsited(ROOT_MISMATCH, &[document.path(), &reason]);
            ok = false;
            continue;
        };
        if let Some(extra) = nodes.get(1) {
            let reason = format!(
                "expected one {expected} root, found {} root nodes",
                nodes.len()
            );
            cx.diags.error(
                ROOT_MISMATCH,
                Site::new(doc, extra.span),
                &[document.path(), &reason],
            );
            ok = false;
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
        if cx.kind == DefinitionKind::Dialect && version.is_none() {
            cx.malformed(
                Site::new(doc, node.span),
                &format!("dialect {name}"),
                "missing version",
            );
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
            "deferred" if cx.kind == DefinitionKind::Dialect => {}
            _ => cx.unknown(doc, child, &root.name),
        }
    }
}

impl Cx<'_> {
    pub(crate) fn malformed(&mut self, site: Site, construct: &str, reason: &str) {
        self.diags.error(MALFORMED, site, &[construct, reason]);
    }

    /// A child node no construct reading admits: a deferred mechanism's
    /// spelling when the dialect defers it, an unknown construct otherwise.
    pub(crate) fn unknown(&mut self, doc: usize, node: &SyntaxNode, parent: &str) {
        let site = Site::new(doc, node.span);
        let construct = format!("{} under {parent}", node.name);
        match deferred_by(&self.deferrals, Spelling::Node(&node.name)) {
            Some(mechanism) => self
                .diags
                .error(DEFERRED_MECHANISM, site, &[&construct, mechanism]),
            None => self.diags.error(UNKNOWN_CONSTRUCT, site, &[&construct]),
        }
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
            self.malformed(site, owner, &format!("more than one {child}"));
            return false;
        }
        *seen = Some(site);
        true
    }

    /// Compile a type expression with the `type-expression` validator.
    pub(crate) fn type_expr(&mut self, site: Site, text: &str) -> Option<TypeExpr> {
        match type_expr::parse(text, &mut self.names) {
            Ok(expr) => Some(expr),
            Err(ExprError::Generic(name)) => {
                let construct = format!("type expression {text:?}");
                if crate::deferred::defers(&self.deferrals, "generic-types") {
                    self.diags
                        .error(DEFERRED_MECHANISM, site, &[&construct, "generic-types"]);
                } else {
                    let reason = format!("{name} is instantiated with type arguments");
                    self.diags.error(TYPE_EXPRESSION, site, &[text, &reason]);
                }
                None
            }
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
        self.malformed(
            site,
            construct,
            &format!("{what} {name:?} is not kebab-case"),
        );
        false
    }

    /// Check a type name: an uppercase ASCII letter, then letters and digits.
    pub(crate) fn type_name(&mut self, site: Site, construct: &str, name: &str) -> bool {
        if type_expr::is_type_name(name) {
            return true;
        }
        self.malformed(
            site,
            construct,
            &format!("type name {name:?} must start uppercase and hold only letters and digits"),
        );
        false
    }
}
