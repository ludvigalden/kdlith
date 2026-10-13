//! Self-description: each definition document is walked through its
//! describing dialect's compiled dispatch and scope rules, so a construct
//! the dialect does not declare is caught. A dialect describes itself.

use kdlith_diagnostic::ByteSpan;

use crate::bindings::{ChildPlan, ScopeId};
use crate::definition::{Definition, DefinitionKind};
use crate::diag::{Diagnostics, Site, UNKNOWN_CONSTRUCT, UNRESOLVED_DIALECT};
use crate::structure::Violation;
use crate::syntax::{ParsedDocument, SyntaxNode};

/// Construct trees nest at most this deep; the readers bound the same
/// trees, so this only guards the walk itself.
const MAX_DEPTH: usize = 64;

pub(crate) fn run(
    diags: &mut Diagnostics<'_>,
    kind: DefinitionKind,
    describing: &Definition,
    documents: &[ParsedDocument],
) {
    let walk = Walk {
        kind,
        dialect: describing,
    };
    for (doc, document) in documents.iter().enumerate() {
        let mut found = Vec::new();
        walk.scope(
            &mut found,
            describing.bindings.root_scope(),
            document.nodes(),
            "the document root",
            0,
        );
        for (span, what) in found {
            walk.report(diags, Site::new(doc, span), &what);
        }
    }
}

struct Walk<'d> {
    kind: DefinitionKind,
    dialect: &'d Definition,
}

impl Walk<'_> {
    fn report(&self, diags: &mut Diagnostics<'_>, site: Site, what: &str) {
        match self.kind {
            DefinitionKind::Dialect => diags.error(UNRESOLVED_DIALECT, site, &["construct", what]),
            DefinitionKind::Schema => diags.error(UNKNOWN_CONSTRUCT, site, &[what]),
        }
    }

    /// Admit every child of one scope, descend into each, then run the
    /// scope's completion checks.
    fn scope(
        &self,
        found: &mut Vec<(ByteSpan, String)>,
        scope: ScopeId,
        children: &[SyntaxNode],
        owner: &str,
        depth: usize,
    ) {
        let Some(compiled) = self.dialect.bindings.scope(scope) else {
            return;
        };
        let names = &self.dialect.names;
        let mut state = compiled.rules.start();
        for child in children {
            let label = format!("{} under {owner}", child.name);
            let Some(sym) = names.get(&child.name) else {
                found.push((child.span, label));
                continue;
            };
            if let Err(violation) = compiled.rules.admit(&mut state, sym) {
                found.push((child.span, self.violation(&label, &violation)));
                continue;
            }
            if depth >= MAX_DEPTH {
                found.push((child.span, format!("{label} nested too deeply")));
                continue;
            }
            match compiled.child(sym) {
                Some(ChildPlan::Declared { decl, .. }) => {
                    if let Some(inner) = self.dialect.bindings.decl_scope(decl) {
                        self.node(found, inner, child, depth);
                    }
                }
                Some(ChildPlan::Nested(inner)) => self.node(found, inner, child, depth),
                Some(ChildPlan::Scalar) => {
                    let scalar = child.args.len() == 1
                        && child.props.is_empty()
                        && child.children.is_empty();
                    if !scalar {
                        found.push((child.span, format!("{label} with other than one value")));
                    }
                }
                None => found.push((child.span, label)),
            }
        }
        if let Some(close) = compiled.rules.complete(&state).first() {
            let span = children.last().map_or(ByteSpan::new(0, 0), |c| c.span);
            found.push((span, self.violation(owner, close)));
        }
    }

    /// Check a node's entries against its scope, then walk its children.
    fn node(
        &self,
        found: &mut Vec<(ByteSpan, String)>,
        scope: ScopeId,
        node: &SyntaxNode,
        depth: usize,
    ) {
        let Some(compiled) = self.dialect.bindings.scope(scope) else {
            return;
        };
        let names = &self.dialect.names;
        let arity = compiled.arity;
        let extra = node.args.len() as u64 > arity.indexed && !arity.rest;
        if extra {
            let at = arity.indexed;
            found.push((node.span, format!("argument {at} of {}", node.name)));
        }
        for (key, scalar) in &node.props {
            let bound = names
                .get(key)
                .is_some_and(|sym| compiled.properties.contains(&sym));
            if !bound {
                found.push((scalar.span, format!("property {key} of {}", node.name)));
            }
        }
        self.scope(found, scope, &node.children, &node.name, depth + 1);
    }

    fn violation(&self, what: &str, violation: &Violation) -> String {
        let names = &self.dialect.names;
        match violation {
            Violation::NotPermitted { .. }
            | Violation::FirstNotAllowed { .. }
            | Violation::NotFollowing { .. } => what.to_owned(),
            Violation::TooMany { max, .. } => format!("{what} beyond max-occurs {max}"),
            Violation::TooFew { child, min, .. } => {
                format!(
                    "{} under {what} below min-occurs {min}",
                    names.resolve(*child)
                )
            }
            Violation::GroupRepeated { .. } => format!("{what} repeating an exactly-one group"),
            Violation::GroupMissing { .. } => format!("{what} missing its exactly-one group"),
            Violation::FirstMissing => format!("{what} without its first node"),
        }
    }
}
