//! The formatting policy a compiled definition implies. The schema's
//! `layout` rules set line limits and, with `order=declared`, write a
//! node's properties in its declaration's field order and its children
//! in the order its fields and children rules name them; otherwise
//! entries and children stay as written. By convention a value bound to
//! an enum or an identifier-format string is written bare, and a string
//! type's `layout prose` or `layout verbatim` decides how its long
//! values are laid out.
//!
//! Children whose order a reading can observe never move: a node with a
//! sequence rule keeps its children as written, and a child that is a
//! processing item of its own (it declares a role) stays in place with
//! nothing moved across it.

use std::collections::HashMap;

use kdlith_diagnostic::ExitStatus;
use kdlith_engine::{
    Definition, FieldBinding, FieldSource, LayoutDecl, LayoutOrder, NodeDecl, StringFormat, Sym,
    TextLayout, TypeExpr, TypeKind,
};
use kdlith_format::{NodeRule, Order, Policy, Text, ValueRule};

use crate::report::report_all;
use crate::vet::{DataOptions, definition_extensions, governing_meta_schema};
use crate::{FmtDataArgs, FmtSchemaArgs};

/// The policy of the governing meta-schema, for `fmt schema`.
pub(crate) fn meta_schema_policy(args: &FmtSchemaArgs) -> Result<Policy, ExitStatus> {
    let extensions = definition_extensions();
    let definition = governing_meta_schema(args.meta_schema.as_deref(), &extensions)
        .map_err(|records| report_all(&records))?;
    Ok(policy(&definition))
}

/// The policy of the schema `--schema` names, for `fmt data`.
pub(crate) fn schema_policy(args: &FmtDataArgs) -> Result<Policy, ExitStatus> {
    let options = args
        .locations
        .apply(DataOptions::new(None, Vec::new()))
        .with_meta_schema(args.meta_schema.clone());
    let definition = options
        .load_schema(&args.schema, &definition_extensions())
        .map_err(|records| report_all(&records))?;
    match report_all(&definition.notes) {
        ExitStatus::Success => Ok(policy(&definition)),
        failed => Err(failed),
    }
}

fn limit(value: Option<u64>) -> Option<usize> {
    value.map(|value| usize::try_from(value).unwrap_or(usize::MAX))
}

fn order(layout: Option<&LayoutDecl>) -> Option<Order> {
    layout
        .and_then(|layout| layout.order)
        .map(|order| match order {
            LayoutOrder::Authored => Order::Authored,
            _ => Order::Declared,
        })
}

/// The policy `definition` implies.
pub(crate) fn policy(definition: &Definition) -> Policy {
    let root = definition.layout.as_ref();
    let mut policy = Policy::new(
        limit(root.and_then(|layout| layout.width)),
        limit(root.and_then(|layout| layout.content_width)),
        order(root),
    );
    let decls: Vec<&NodeDecl> = definition.bindings.iter().map(|(_, decl)| decl).collect();
    let mut nested = Vec::new();
    for decl in &decls {
        let (rule, children) = Rules {
            definition,
            decls: &decls,
        }
        .rule(decl);
        policy.add(rule);
        nested.extend(children);
    }
    // A child a field binds through nested sources, with no declaration
    // of its own under that parent, follows the rule the sources imply.
    for rule in nested {
        let declared = decls.iter().any(|decl| {
            definition.names.resolve(decl.name) == rule.name
                && decl.placement.parents().iter().any(|parent| {
                    rule.parents
                        .iter()
                        .any(|name| name == definition.names.resolve(*parent))
                })
        });
        if !declared {
            policy.add(rule);
        }
    }
    policy
}

struct Rules<'d> {
    definition: &'d Definition,
    decls: &'d [&'d NodeDecl],
}

impl Rules<'_> {
    fn name(&self, sym: Sym) -> String {
        self.definition.names.resolve(sym).to_owned()
    }

    /// The rule of one declaration, and the rules of the children its
    /// fields bind through nested sources.
    fn rule(&self, decl: &NodeDecl) -> (NodeRule, Vec<NodeRule>) {
        let mut rule = NodeRule::new(self.name(decl.name));
        rule.root = decl.placement.at_root();
        rule.parents = decl
            .placement
            .parents()
            .iter()
            .map(|sym| self.name(*sym))
            .collect();
        if let Some(layout) = &decl.layout {
            rule.width = limit(layout.width);
            rule.content_width = limit(layout.content_width);
            rule.order = order(Some(layout));
        }
        self.ranks(decl, &mut rule);
        let mut nested = Vec::new();
        let owner = rule.name.clone();
        for field in fields(decl) {
            let ty = self.field_type(decl, field);
            self.values(&owner, &field.sources, ty, &mut rule, &mut nested);
        }
        (rule, nested)
    }

    fn ranks(&self, decl: &NodeDecl, rule: &mut NodeRule) {
        let mut children: HashMap<String, usize> = HashMap::new();
        let mut rank = 0;
        for field in fields(decl) {
            for source in &field.sources {
                match source {
                    FieldSource::Property(key) => {
                        rule.property_ranks.entry(self.name(*key)).or_insert(rank);
                    }
                    FieldSource::Child {
                        name: child,
                        compact,
                        ..
                    }
                    | FieldSource::Children {
                        name: child,
                        compact,
                        ..
                    } => {
                        if *compact {
                            rule.property_ranks.entry(self.name(*child)).or_insert(rank);
                        }
                        children.entry(self.name(*child)).or_insert(rank);
                    }
                    FieldSource::Entries { node, .. } => {
                        children.entry(self.name(*node)).or_insert(rank);
                    }
                    _ => {}
                }
            }
            rank += 1;
        }
        let rules = decl
            .children
            .iter()
            .chain(decl.cases.iter().flat_map(|case| case.children.iter()));
        for child in rules {
            children.entry(self.name(child.name)).or_insert(rank);
            rank += 1;
        }
        // A child that is a processing item of its own is emitted,
        // applied, or imported in the order written: it ranks nowhere.
        children.retain(|child, _| {
            !self.decls.iter().any(|other| {
                other.role.is_some()
                    && self.definition.names.resolve(other.name) == child
                    && other.placement.parents().contains(&decl.name)
            })
        });
        rule.child_ranks = decl.sequence.is_none().then_some(children);
    }

    /// Record the value rules `sources` imply for `rule`, and a rule for
    /// each child they bind through nested sources.
    fn values(
        &self,
        owner: &str,
        sources: &[FieldSource],
        ty: Option<&TypeExpr>,
        rule: &mut NodeRule,
        nested: &mut Vec<NodeRule>,
    ) {
        for source in sources {
            match source {
                FieldSource::Argument(index) => {
                    let Ok(index) = usize::try_from(*index) else {
                        continue;
                    };
                    if index >= 64 {
                        continue;
                    }
                    if rule.args.len() <= index {
                        rule.args.resize(index + 1, ValueRule::default());
                    }
                    rule.args[index] = self.value_rule(ty);
                }
                FieldSource::Args => {
                    rule.rest = self.value_rule(ty.and_then(list_element));
                }
                FieldSource::Property(key) => {
                    rule.properties.insert(self.name(*key), self.value_rule(ty));
                }
                FieldSource::Child { name, sources, .. }
                | FieldSource::Children { name, sources, .. } => {
                    let element = match source {
                        FieldSource::Children { .. } => ty.and_then(list_element),
                        _ => ty,
                    };
                    let mut child = NodeRule::new(self.name(*name));
                    child.parents = vec![owner.to_owned()];
                    if sources.is_empty() {
                        // A bare child carries the value as its one
                        // argument.
                        child.args = vec![self.value_rule(element)];
                    } else {
                        let owner = child.name.clone();
                        self.values(&owner, sources, element, &mut child, nested);
                    }
                    nested.push(child);
                }
                _ => {}
            }
        }
    }

    fn value_rule(&self, ty: Option<&TypeExpr>) -> ValueRule {
        let Some(kind) = ty.and_then(|ty| self.named(ty)) else {
            return ValueRule::default();
        };
        match kind {
            TypeKind::Enum(_) => ValueRule::new(true, Text::Exact),
            TypeKind::String(string) => {
                let bare = string.format.is_some_and(|rule| {
                    matches!(
                        rule.format,
                        StringFormat::KebabCase
                            | StringFormat::CamelCase
                            | StringFormat::LowerSnake
                            | StringFormat::UpperSnake
                    )
                });
                let text = match string.layout {
                    Some(TextLayout::Prose) => Text::Prose,
                    Some(TextLayout::Verbatim) => Text::Verbatim,
                    _ => Text::Exact,
                };
                ValueRule::new(bare, text)
            }
            _ => ValueRule::default(),
        }
    }

    /// The declared type a scalar type expression names, through
    /// `optional` and `nullable`.
    fn named(&self, ty: &TypeExpr) -> Option<&TypeKind> {
        match ty {
            TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) => self.named(inner),
            TypeExpr::Named(sym) => {
                let types = &self.definition.types;
                types
                    .lookup(*sym)
                    .and_then(|id| types.get(id))
                    .map(|decl| &decl.kind)
            }
            _ => None,
        }
    }

    /// The type a field binding holds: its inline type, else the type of
    /// the field of that name in the declaration's object type.
    fn field_type<'a>(
        &'a self,
        decl: &'a NodeDecl,
        field: &'a FieldBinding,
    ) -> Option<&'a TypeExpr> {
        if let Some(ty) = &field.ty {
            return Some(ty);
        }
        let types = &self.definition.types;
        let object = types.get(types.lookup(decl.ty?)?)?;
        let TypeKind::Object(object) = &object.kind else {
            return None;
        };
        object
            .fields
            .iter()
            .find(|def| def.name == field.name)
            .map(|def| &def.ty)
    }
}

/// Every field binding of a declaration, its cases' after its own.
fn fields(decl: &NodeDecl) -> impl Iterator<Item = &FieldBinding> {
    decl.fields
        .iter()
        .chain(decl.cases.iter().flat_map(|case| case.fields.iter()))
}

fn list_element(ty: &TypeExpr) -> Option<&TypeExpr> {
    match ty {
        TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) => list_element(inner),
        TypeExpr::List(inner) => Some(inner),
        _ => None,
    }
}
