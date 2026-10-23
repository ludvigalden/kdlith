//! Self-description: each definition document is walked through its
//! describing dialect's compiled dispatch and scope rules, so a construct
//! the dialect does not declare is caught. A dialect describes itself.

use kdlith_diagnostic::ByteSpan;
use kdlith_parse::locate;

use crate::bindings::{ChildPlan, ScopeId};
use crate::definition::{Definition, DefinitionKind};
use crate::diag::{
    CARDINALITY, Diagnostics, MISSING_FIELD, Site, UNBOUND_ENTRY, UNKNOWN_NODE, UNRESOLVED_DIALECT,
    sibling_note,
};
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
    for (doc, document) in documents.iter().enumerate() {
        let walk = Walk {
            kind,
            dialect: describing,
            text: document.text(),
        };
        let mut found = Vec::new();
        walk.scope(
            &mut found,
            describing.bindings.root_scope(),
            document.nodes(),
            "the document root",
            0,
        );
        for finding in found {
            walk.report(diags, doc, &finding);
        }
    }
}

/// One construct the describing dialect does not admit, with the values
/// its binding code renders.
struct Finding {
    span: ByteSpan,
    code: &'static str,
    values: Vec<String>,
    /// A note at another span, with its message.
    note: Option<(ByteSpan, String)>,
}

impl Finding {
    fn new(span: ByteSpan, code: &'static str, values: &[&str]) -> Self {
        Self {
            span,
            code,
            values: values.iter().map(|v| (*v).to_owned()).collect(),
            note: None,
        }
    }

    /// One phrase for a dialect that does not describe its own construct.
    fn phrase(&self) -> String {
        match self.code {
            UNKNOWN_NODE => format!("{} under {}", self.values[0], self.values[1]),
            UNBOUND_ENTRY => format!("{} of {}", self.values[0], self.values[1]),
            _ => format!(
                "{} occurring {} times, expected {}",
                self.values[0], self.values[1], self.values[2]
            ),
        }
    }
}

struct Walk<'d> {
    kind: DefinitionKind,
    dialect: &'d Definition,
    /// The text of the document being walked, to compare columns.
    text: &'d str,
}

/// A child the scope admitted, kept as the sibling the next child is
/// compared against.
#[derive(Clone, Copy)]
struct Admitted<'n> {
    node: &'n SyntaxNode,
    scope: Option<ScopeId>,
}

impl Walk<'_> {
    /// A schema definition the dialect does not admit gets the binding
    /// code any processor binding it reports; a dialect that does not
    /// describe its own constructs is not self-describing.
    fn report(&self, diags: &mut Diagnostics<'_>, doc: usize, finding: &Finding) {
        let site = Site::new(doc, finding.span);
        match self.kind {
            DefinitionKind::Dialect => {
                diags.error(UNRESOLVED_DIALECT, site, &["construct", &finding.phrase()]);
            }
            DefinitionKind::Schema => {
                let values: Vec<&str> = finding.values.iter().map(String::as_str).collect();
                diags.noted(finding.code, site, &values, finding.note.clone());
            }
        }
    }

    /// Admit every child of one scope, descend into each, then run the
    /// scope's completion checks.
    fn scope(
        &self,
        found: &mut Vec<Finding>,
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
        let mut previous: Option<Admitted<'_>> = None;
        for child in children {
            let above = previous.take();
            let unknown = || {
                let mut finding = Finding::new(child.span, UNKNOWN_NODE, &[&child.name, owner]);
                finding.note = above.and_then(|sibling| self.sibling(sibling, child));
                finding
            };
            let Some(sym) = names.get(&child.name) else {
                found.push(unknown());
                continue;
            };
            if let Err(violation) = compiled.rules.admit(&mut state, sym) {
                let finding = match violation {
                    Violation::NotPermitted { .. } => unknown(),
                    _ => self.violation(child.span, owner, &violation),
                };
                found.push(finding);
                continue;
            }
            if depth >= MAX_DEPTH {
                let what = format!("{} under {owner}, nested", child.name);
                found.push(Finding::new(
                    child.span,
                    CARDINALITY,
                    &[
                        &what,
                        &depth.to_string(),
                        &format!("at most {MAX_DEPTH} deep"),
                    ],
                ));
                continue;
            }
            let inner = match compiled.child(sym) {
                Some(ChildPlan::Declared { decl, .. }) => {
                    let inner = self.dialect.bindings.decl_scope(decl);
                    if let Some(inner) = inner {
                        self.node(found, inner, child, depth);
                    }
                    inner
                }
                Some(ChildPlan::Nested(inner)) => {
                    self.node(found, inner, child, depth);
                    Some(inner)
                }
                Some(ChildPlan::Scalar) => {
                    self.scalar(found, child);
                    None
                }
                None => {
                    found.push(unknown());
                    continue;
                }
            };
            previous = Some(Admitted {
                node: child,
                scope: inner,
            });
        }
        let span = children.last().map_or(ByteSpan::new(0, 0), |c| c.span);
        for close in compiled.rules.complete(&state) {
            found.push(self.violation(span, owner, &close));
        }
    }

    /// The note for an unknown child whose admitted sibling above it
    /// admits it as a child, when the child is indented past it.
    fn sibling(&self, sibling: Admitted<'_>, child: &SyntaxNode) -> Option<(ByteSpan, String)> {
        let sym = self.dialect.names.get(&child.name)?;
        self.dialect.bindings.child(sibling.scope?, sym)?;
        let at = locate(self.text, child.span).start;
        let above = locate(self.text, sibling.node.span).start;
        (at.column > above.column).then(|| {
            let message = sibling_note(&sibling.node.name, &child.name);
            (sibling.node.span, message)
        })
    }

    /// A child bound as exactly one positional value of its owner's field.
    fn scalar(&self, found: &mut Vec<Finding>, child: &SyntaxNode) {
        for (at, scalar) in child.args.iter().enumerate().skip(1) {
            let what = format!("argument {at}");
            found.push(Finding::new(
                scalar.span,
                UNBOUND_ENTRY,
                &[&what, &child.name],
            ));
        }
        for (key, scalar) in &child.props {
            let what = format!("property {key}");
            found.push(Finding::new(
                scalar.span,
                UNBOUND_ENTRY,
                &[&what, &child.name],
            ));
        }
        for inner in &child.children {
            found.push(Finding::new(
                inner.span,
                UNKNOWN_NODE,
                &[&inner.name, &child.name],
            ));
        }
        if child.args.is_empty() {
            let what = format!("value of child {}", child.name);
            found.push(Finding::new(
                child.span,
                MISSING_FIELD,
                &[&child.name, &what],
            ));
        }
    }

    /// Check a node's entries against its scope, then walk its children.
    fn node(&self, found: &mut Vec<Finding>, scope: ScopeId, node: &SyntaxNode, depth: usize) {
        let Some(compiled) = self.dialect.bindings.scope(scope) else {
            return;
        };
        let names = &self.dialect.names;
        let arity = compiled.arity;
        if !arity.rest {
            let skip = usize::try_from(arity.indexed).unwrap_or(usize::MAX);
            for (at, scalar) in node.args.iter().enumerate().skip(skip) {
                let what = format!("argument {at}");
                found.push(Finding::new(
                    scalar.span,
                    UNBOUND_ENTRY,
                    &[&what, &node.name],
                ));
            }
        }
        for (key, scalar) in &node.props {
            let bound = names
                .get(key)
                .is_some_and(|sym| compiled.properties.contains(&sym));
            if !bound {
                let what = format!("property {key}");
                found.push(Finding::new(
                    scalar.span,
                    UNBOUND_ENTRY,
                    &[&what, &node.name],
                ));
            }
        }
        self.scope(found, scope, &node.children, &node.name, depth + 1);
    }

    /// A scope rule the children broke, as the binder reports it.
    fn violation(&self, span: ByteSpan, owner: &str, violation: &Violation) -> Finding {
        let names = &self.dialect.names;
        let under = |child| format!("{} under {owner}", names.resolve(child));
        match violation {
            Violation::NotPermitted { child } => {
                Finding::new(span, UNKNOWN_NODE, &[names.resolve(*child), owner])
            }
            Violation::FirstNotAllowed { child } => {
                let place = format!("{owner} as its first node");
                Finding::new(span, UNKNOWN_NODE, &[names.resolve(*child), &place])
            }
            Violation::NotFollowing { child } => {
                let place = format!("{owner} after its first node");
                Finding::new(span, UNKNOWN_NODE, &[names.resolve(*child), &place])
            }
            Violation::TooMany { child, count, max } => {
                let count = count.to_string();
                let what = under(*child);
                Finding::new(
                    span,
                    CARDINALITY,
                    &[&what, &count, &format!("at most {max}")],
                )
            }
            Violation::TooFew { child, count, min } => {
                let what = under(*child);
                let expected = format!("at least {min}");
                Finding::new(span, CARDINALITY, &[&what, &count.to_string(), &expected])
            }
            Violation::GroupRepeated { child, .. } => {
                let what = format!("the exactly-one group of {}", under(*child));
                Finding::new(span, CARDINALITY, &[&what, "2", "exactly 1"])
            }
            Violation::GroupMissing { group } => {
                let what = format!("exactly-one group {} under {owner}", group + 1);
                Finding::new(span, CARDINALITY, &[&what, "0", "exactly 1"])
            }
            Violation::FirstMissing => {
                let what = format!("the first node of {owner}");
                Finding::new(span, CARDINALITY, &[&what, "0", "exactly 1"])
            }
            Violation::SequenceRejected { child, step } => {
                let what = format!("{} under {owner}", names.resolve(*child));
                Finding::new(
                    span,
                    CARDINALITY,
                    &[
                        &what,
                        &format!("step {}", step + 1),
                        "a position the ordered sequence admits",
                    ],
                )
            }
            Violation::SequenceIncomplete { step } => {
                let what = format!("the ordered sequence under {owner}");
                Finding::new(
                    span,
                    CARDINALITY,
                    &[
                        &what,
                        &format!("stopped at step {}", step + 1),
                        "every step within its bounds",
                    ],
                )
            }
        }
    }
}
