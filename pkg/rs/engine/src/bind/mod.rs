//! The typed binder: a runtime processor that dispatches each node by
//! parent and name against a compiled definition, binds its entries and
//! children into typed fields, and emits bound items and diagnostics.

mod frame;
mod source;
mod tables;

use std::borrow::Cow;

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{ByteSpan, DiagnosticRecord};
use kdlith_model::{
    BoundContribution, BoundDeclaration, BoundItem, BoundRecord, Entry, Field, Identity,
    ImportRequest, Location, OriginId, QualifiedName, Reference, Repeated, ScopeCompletion,
    SourceId, ValidationState, Value,
};
use kdlith_parse::{Locator, Parser};
use kdlith_runtime::{
    Completion, Context, ContextEffect, Cx, Disposition, FollowingEffects, HeaderEffects,
    Processor, Session, SessionError, SlotValue,
};

use crate::bindings::{ChildPlan, ContextPolicy, NodeDeclId, Role, SetFrom};
use crate::decode::{self, Decoder};
use crate::definition::Definition;
use crate::names::Sym;
use crate::plan::EffectStep;
use crate::structure::{ScopeRules, ScopeState, Violation};
use crate::summary;
use crate::types::{TypeExpr, TypeKind, Variant};

pub use frame::BindFrame;
pub use source::ParseEvents;

use frame::{Kind, Opened, Supplied};
use tables::{Bind, Collect, Mode, Tables};

const UNKNOWN_NODE: &str = "KDLITH-BND001";
const VALUE_TYPE: &str = "KDLITH-BND003";
const MISSING_FIELD: &str = "KDLITH-BND004";
const UNBOUND_ENTRY: &str = "KDLITH-BND005";
const DUPLICATE_KEY: &str = "KDLITH-BND006";
const COLLISION: &str = "KDLITH-BND007";
const DUPLICATE_PROPERTY: &str = "KDLITH-SYN003";
const CONTEXT_UNAVAILABLE: &str = "KDLITH-CTX001";
const CONTEXT_DISAGREES: &str = "KDLITH-CTX002";

const DOCUMENT_ROOT: &str = "the document root";

/// The node-start payload the binder consumes: the name, borrowed from
/// the source where the parser could, and whether it is type-annotated.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct NodeStart<'s> {
    pub name: Cow<'s, str>,
    pub annotated: bool,
}

impl<'s> NodeStart<'s> {
    pub fn new(name: impl Into<Cow<'s, str>>, annotated: bool) -> Self {
        Self {
            name: name.into(),
            annotated,
        }
    }
}

/// An argument (no name) or a property, with its value.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EntryValue<'s> {
    pub name: Option<Cow<'s, str>>,
    pub annotated: bool,
    pub value: Value,
}

impl<'s> EntryValue<'s> {
    pub fn new(name: Option<Cow<'s, str>>, annotated: bool, value: Value) -> Self {
        Self {
            name,
            annotated,
            value,
        }
    }
}

/// A processor binding one data document against a compiled schema
/// definition. The definition's KDL is never read: every lookup goes
/// through its catalogs and plan, and through tables built from them
/// once in [`Binder::new`].
pub struct Binder<'d, 's> {
    def: &'d Definition,
    tables: Tables,
    decoder: Decoder,
    path: String,
    locator: Locator<'s>,
    source: SourceId,
    root: ScopeState,
    /// A root node broke the document's order rule, so the missing first
    /// node is not reported again at the end.
    order_broken: bool,
    errors: u64,
}

impl<'d, 's> Binder<'d, 's> {
    /// A binder for the document `text`, read from `path` as `source`.
    /// `text` locates diagnostics and import sites.
    pub fn new(
        def: &'d Definition,
        source: SourceId,
        path: impl Into<String>,
        text: &'s str,
    ) -> Self {
        let root = def
            .bindings
            .scope(def.bindings.root_scope())
            .map_or_else(|| ScopeRules::default().start(), |s| s.rules.start());
        Self {
            def,
            tables: tables::build(def),
            decoder: Decoder::new(&def.types).0,
            path: path.into(),
            locator: Locator::new(text),
            source,
            root,
            order_broken: false,
            errors: 0,
        }
    }

    /// The number of error diagnostics emitted so far.
    pub fn errors(&self) -> u64 {
        self.errors
    }

    fn name(&self, sym: Sym) -> &'d str {
        self.def.names.resolve(sym)
    }

    fn location(&mut self, span: ByteSpan) -> Location {
        Location::new(self.source, self.locator.locate(span))
    }

    fn diagnose(&mut self, cx: &mut Cx<'_>, code: &'static str, span: ByteSpan, values: &[&str]) {
        let mut record = DiagnosticRecord::from_code(code)
            .expect("binder diagnostic codes are registered and never retired");
        if let Some(definition) = lookup(code) {
            record = record.with_message(render(definition.message_template, values));
        }
        let at = self.locator.locate(span).start;
        let record = record
            .with_source(self.path.clone())
            .with_span(span, Some(at.line), Some(at.column))
            .with_origin("data-document");
        self.errors += 1;
        cx.emit(BoundItem::Diagnostic(Box::new(record)));
    }

    fn violation(&mut self, cx: &mut Cx<'_>, span: ByteSpan, owner: &str, violation: &Violation) {
        let code = violation.code();
        let (what, count, expected) = match violation {
            Violation::NotPermitted { child }
            | Violation::FirstNotAllowed { child }
            | Violation::NotFollowing { child } => {
                let child = self.name(*child);
                let place = match violation {
                    Violation::FirstNotAllowed { .. } => format!("{owner} as its first node"),
                    Violation::NotFollowing { .. } => format!("{owner} after its first node"),
                    _ => owner.to_owned(),
                };
                self.diagnose(cx, code, span, &[child, &place]);
                return;
            }
            Violation::TooMany { child, max } => (
                format!("{} under {owner}", self.name(*child)),
                max.saturating_add(1).to_string(),
                format!("at most {max}"),
            ),
            Violation::TooFew { child, count, min } => (
                format!("{} under {owner}", self.name(*child)),
                count.to_string(),
                format!("at least {min}"),
            ),
            Violation::GroupRepeated { child, .. } => (
                format!(
                    "the exactly-one group of {} under {owner}",
                    self.name(*child)
                ),
                "2".to_owned(),
                "exactly 1".to_owned(),
            ),
            Violation::GroupMissing { group } => (
                format!("exactly-one group {} under {owner}", group + 1),
                "0".to_owned(),
                "exactly 1".to_owned(),
            ),
            Violation::FirstMissing => (
                format!("the first node of {owner}"),
                "0".to_owned(),
                "exactly 1".to_owned(),
            ),
        };
        self.diagnose(cx, code, span, &[&what, &count, &expected]);
    }

    /// The type one supplied item decodes as: the field's type, or its
    /// element type when the field collects.
    fn item_type(&self, owner: NodeDeclId, bind: Bind) -> Option<&'_ TypeExpr> {
        let mut ty = self.tables.decls[owner.index()].fields[bind.field]
            .ty
            .as_ref()?;
        while let TypeExpr::Optional(inner) = ty {
            ty = inner;
        }
        Some(match (bind.mode, ty) {
            (Mode::Append, TypeExpr::List(inner)) => inner,
            (Mode::Keyed, TypeExpr::Map(_, value) | TypeExpr::OrderedMap(_, value)) => value,
            (_, ty) => ty,
        })
    }

    /// The variants of the union a supplied item belongs to, when its
    /// type names a union.
    fn union_of(&self, owner: NodeDeclId, bind: Bind) -> Option<&'d [Variant]> {
        let def = self.def;
        let TypeExpr::Named(name) = self.item_type(owner, bind)? else {
            return None;
        };
        match &def.types.get(def.types.lookup(*name)?)?.kind {
            TypeKind::Union(union) => Some(&union.variants),
            _ => None,
        }
    }

    /// The key type of a keyed field.
    fn key_type(&self, owner: NodeDeclId, bind: Bind) -> Option<&'_ TypeExpr> {
        let mut ty = self.tables.decls[owner.index()].fields[bind.field]
            .ty
            .as_ref()?;
        while let TypeExpr::Optional(inner) = ty {
            ty = inner;
        }
        match ty {
            TypeExpr::Map(key, _) | TypeExpr::OrderedMap(key, _) => Some(key),
            _ => None,
        }
    }

    fn field_label(&self, owner: NodeDeclId, field: usize) -> String {
        let node = self
            .def
            .bindings
            .get(owner)
            .map_or("", |d| self.name(d.name));
        let field = self.tables.decls[owner.index()].fields[field].name;
        format!("{node}.{}", self.name(field))
    }

    /// Check a scalar against `ty`, reporting a mismatch.
    fn check(
        &mut self,
        cx: &mut Cx<'_>,
        ty: Option<TypeExpr>,
        what: &str,
        value: Value,
        span: ByteSpan,
    ) -> Option<Value> {
        let Some(ty) = ty else {
            return Some(value);
        };
        let site = self.location(span);
        let decoded = self
            .decoder
            .decode(&self.def.types, &self.def.names, &ty, &value, site);
        if decoded.is_none() {
            let expected = summary::expr(&self.def.names, &ty);
            let found = decode::found(&value);
            self.diagnose(cx, VALUE_TYPE, span, &[what, &expected, &found]);
        }
        decoded
    }

    /// Decode one scalar of a node's syntax and hand it to its field.
    fn take(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &mut BindFrame,
        bind: Bind,
        value: Value,
        span: ByteSpan,
    ) {
        let owner = frame.owner;
        let ty = self.item_type(owner, bind).cloned();
        let what = self.field_label(owner, bind.field);
        match self.check(cx, ty, &what, value, span) {
            Some(value) => self.route(cx, frame, bind, value, span),
            None => frame.rejected.push(bind.field),
        }
    }

    /// Land a value in the frame owning its field, or carry it up when
    /// `target` is a nested or scalar part of that owner.
    fn route(
        &mut self,
        cx: &mut Cx<'_>,
        target: &mut BindFrame,
        bind: Bind,
        value: Value,
        span: ByteSpan,
    ) {
        if !target.declared() {
            target.carried.push((bind, value, span));
            return;
        }
        let label = self.field_label(target.owner, bind.field);
        let node = self.name(target.name);
        let slot = &mut target.fields[bind.field];
        match bind.mode {
            Mode::Single => match slot {
                None => *slot = Some(Supplied::new(value, span)),
                Some(_) => self.diagnose(cx, COLLISION, span, &[&label, node]),
            },
            Mode::Append => match slot {
                None => *slot = Some(Supplied::new(Value::List(vec![value]), span)),
                Some(Supplied {
                    value: Value::List(items),
                    ..
                }) => items.push(value),
                Some(_) => self.diagnose(cx, COLLISION, span, &[&label, node]),
            },
            Mode::Keyed => {
                let Value::Map(mut pair) = value else {
                    return;
                };
                let Some(entry) = pair.pop() else {
                    return;
                };
                match slot {
                    None => *slot = Some(Supplied::new(Value::Map(vec![entry]), span)),
                    Some(Supplied {
                        value: Value::Map(entries),
                        ..
                    }) => {
                        if entries.iter().any(|held| held.key == entry.key) {
                            let key = entry.key;
                            self.diagnose(cx, DUPLICATE_KEY, span, &[&key, &label]);
                        } else {
                            entries.push(entry);
                        }
                    }
                    Some(_) => self.diagnose(cx, COLLISION, span, &[&label, node]),
                }
            }
        }
    }

    /// A field's value as the header knows it: explicit, then context,
    /// then the field's fallback.
    fn header_value(&self, cx: &Cx<'_>, frame: &BindFrame, field: Sym) -> Option<Value> {
        let infos = &self.tables.decls[frame.owner.index()].fields;
        let at = infos.iter().position(|info| info.name == field)?;
        if let Some(supplied) = &frame.fields[at] {
            return Some(supplied.value.clone());
        }
        let info = &infos[at];
        info.context
            .and_then(|context| cx.context().get(context.slot))
            .map(|held| held.value.clone())
            .or_else(|| info.fallback.clone())
    }

    /// Resolve every field of a closing declared node: explicit values,
    /// context under its policy, fallbacks, empty collections, and the
    /// required check. Returns the values by field position.
    fn finish_fields(&mut self, cx: &mut Cx<'_>, frame: &mut BindFrame) -> Vec<Option<Value>> {
        let owner = frame.owner;
        let count = self.tables.decls[owner.index()].fields.len();
        let mut values = Vec::with_capacity(count);
        for at in 0..count {
            let info = self.tables.decls[owner.index()].fields[at].clone();
            let explicit = frame.fields[at].take();
            let held = info
                .context
                .and_then(|context| cx.context().get(context.slot))
                .map(|held| held.value.clone());
            let value = match (explicit, info.context) {
                (Some(supplied), Some(context)) => {
                    if context.policy == Some(ContextPolicy::Fixed)
                        && let Some(held) = &held
                        && *held != supplied.value
                    {
                        let field = self.field_label(owner, at);
                        let ours = decode::found(&supplied.value);
                        let theirs = decode::found(held);
                        let slot = self.name(context.name);
                        let values = [field.as_str(), &ours, slot, &theirs];
                        self.diagnose(cx, CONTEXT_DISAGREES, supplied.span, &values);
                    }
                    Some(supplied.value)
                }
                (Some(supplied), None) => Some(supplied.value),
                (None, Some(context)) => {
                    if held.is_none()
                        && context.policy.is_none()
                        && info.required
                        && info.fallback.is_none()
                    {
                        let slot = self.name(context.name);
                        self.diagnose(cx, CONTEXT_UNAVAILABLE, frame.span, &[slot]);
                        values.push(None);
                        continue;
                    }
                    held
                }
                (None, None) => None,
            };
            let value = value.or(info.fallback).or(match info.collect {
                Collect::List => Some(Value::List(Vec::new())),
                Collect::Map => Some(Value::Map(Vec::new())),
                Collect::No => None,
            });
            if value.is_none() && info.required && !frame.rejected.contains(&at) {
                let node = self.name(frame.name);
                let field = self.name(info.name);
                self.diagnose(cx, MISSING_FIELD, frame.span, &[node, field]);
            }
            values.push(value);
        }
        values
    }

    /// Resolve effect steps into runtime effects, reading fields by name.
    fn effects(
        &self,
        steps: &[EffectStep],
        origin: OriginId,
        value_of: impl Fn(Sym) -> Option<Value>,
    ) -> Vec<ContextEffect> {
        let mut out = Vec::new();
        for step in steps {
            match step {
                EffectStep::Set { slot, from } => {
                    let value = match from {
                        SetFrom::Value(value) => Some(value.clone()),
                        SetFrom::Field(field) => value_of(*field),
                    };
                    // A set from an absent optional field leaves the slot
                    // as it was rather than inventing a removal.
                    if let Some(value) = value {
                        out.push(ContextEffect::Set {
                            slot: *slot,
                            value: SlotValue::new(value, origin),
                        });
                    }
                }
                EffectStep::Remove { slot } => out.push(ContextEffect::Remove { slot: *slot }),
                EffectStep::Clear { namespace } => out.push(ContextEffect::Clear {
                    namespace: *namespace,
                }),
                // Compilation rejects a handler the processor does not
                // install, and the binder installs none.
                _ => {}
            }
        }
        out
    }

    /// Keep the effects whose set value decodes as its slot's type,
    /// reporting the others: values set into a slot are validated
    /// against the slot, not only against the field they came from.
    fn typed(
        &mut self,
        cx: &mut Cx<'_>,
        effects: Vec<ContextEffect>,
        span: ByteSpan,
    ) -> Vec<ContextEffect> {
        let mut kept = Vec::with_capacity(effects.len());
        for effect in effects {
            if let ContextEffect::Set { slot, value } = &effect
                && let Some((name, ty)) = self.tables.slots.get(slot.index())
            {
                let what = format!("context {}", self.name(*name));
                let ty = Some(ty.clone());
                if self
                    .check(cx, ty, &what, value.value.clone(), span)
                    .is_none()
                {
                    continue;
                }
            }
            kept.push(effect);
        }
        kept
    }

    /// Emit the processing item a declared node's role calls for.
    fn emit(&mut self, cx: &mut Cx<'_>, frame: &BindFrame, role: Role, values: Vec<Option<Value>>) {
        let owner = frame.owner;
        let node = cx.node();
        let origin = OriginId(node.index);
        let info = &self.tables.decls[owner.index()];
        let validation = if info.provisional {
            ValidationState::Provisional
        } else {
            ValidationState::Valid
        };
        let type_name = info.type_name.clone();
        let text_of = |at: Option<usize>| match at.and_then(|at| values.get(at)?.as_ref()) {
            Some(Value::String(text)) => Some(text.clone()),
            _ => None,
        };
        let identity = text_of(info.identity);
        let path = text_of(info.path);
        let repeated = text_of(info.repeated);
        let project: Vec<String> = match info.project.and_then(|at| values.get(at)?.as_ref()) {
            Some(Value::List(names)) => names
                .iter()
                .filter_map(|name| match name {
                    Value::String(name) => Some(name.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let reference = info.references.iter().find_map(|at| match values.get(*at) {
            Some(Some(Value::Reference(reference))) => Some(reference.clone()),
            _ => None,
        });
        let fields: Vec<Field> = info
            .fields
            .iter()
            .zip(values)
            .filter_map(|(field, value)| Some(Field::new(self.name(field.name), value?, origin)))
            .collect();
        let item = match role {
            Role::Record => BoundItem::Record(BoundRecord::new(
                node, type_name, fields, origin, validation,
            )),
            Role::Declaration => {
                let name = QualifiedName(identity.into_iter().collect());
                let identity = Identity::new(type_name.clone(), name);
                BoundItem::Declaration(BoundDeclaration::new(
                    node, identity, type_name, fields, origin, validation,
                ))
            }
            Role::Contribution => {
                let site = self.location(frame.span);
                let target = reference.unwrap_or_else(|| {
                    let written = self.name(frame.name);
                    Reference::new(type_name, QualifiedName(Vec::new()), written, site)
                });
                let value = Value::Map(
                    fields
                        .into_iter()
                        .map(|f| Entry::new(f.name, f.value))
                        .collect(),
                );
                BoundItem::Contribution(Box::new(BoundContribution::new(
                    node, target, value, origin, validation,
                )))
            }
            Role::SourceImport => {
                let site = self.location(frame.span);
                let expression = path.unwrap_or_default();
                let policy = match repeated.as_deref().map(Repeated::from_name) {
                    None => Repeated::default(),
                    Some(Some(policy)) => policy,
                    Some(None) => {
                        let found = format!("{:?}", repeated.unwrap_or_default());
                        let values = [
                            "import repeated",
                            "reject, coalesce, or instantiate",
                            &found,
                        ];
                        self.diagnose(cx, VALUE_TYPE, frame.span, &values);
                        return;
                    }
                };
                let request = ImportRequest::new(node, expression, site, origin);
                BoundItem::Import(request.with_policy(project, policy))
            }
            Role::Container => {
                BoundItem::ScopeComplete(ScopeCompletion::new(self.source, Some(node)))
            }
            // A context control's result is its effects, not an item.
            _ => return,
        };
        cx.emit(item);
    }

    fn close_declared(
        &mut self,
        cx: &mut Cx<'_>,
        parent: Option<&mut BindFrame>,
        frame: &mut BindFrame,
        key: Option<u64>,
        following: &mut FollowingEffects<'_>,
    ) {
        let owner = frame.owner;
        if key.is_some() && frame.key.is_none() {
            let node = self.name(frame.name);
            self.diagnose(cx, MISSING_FIELD, frame.span, &[node, "key argument"]);
        }
        let values = self.finish_fields(cx, frame);
        if self.errors - frame.mark > frame.excluded {
            // A part that failed is reported already; its owner's field
            // is not also missing.
            let bind = parent
                .as_deref()
                .and_then(|p| p.table(&self.tables))
                .and_then(|t| t.child(frame.name));
            if let (Some(parent), Some(bind)) = (parent, bind) {
                parent.rejected.push(bind.field);
            }
            return;
        }
        let def = self.def;
        let role = def.bindings.get(owner).and_then(|decl| decl.role);
        if let Some(role) = role {
            if let Some(plan) = def.plan.node(owner) {
                let infos = &self.tables.decls[owner.index()].fields;
                let value_of = |field: Sym| {
                    let at = infos.iter().position(|info| info.name == field)?;
                    values.get(at).cloned().flatten()
                };
                let origin = OriginId(cx.node().index);
                let effects = self.effects(&plan.following_effects, origin, value_of);
                for effect in self.typed(cx, effects, frame.span) {
                    following.push(effect);
                }
            }
            self.emit(cx, frame, role, values);
            return;
        }
        let Some(parent) = parent else {
            return;
        };
        let Some(bind) = parent.table(&self.tables).and_then(|t| t.child(frame.name)) else {
            return;
        };
        let infos = &self.tables.decls[owner.index()].fields;
        let entries = infos
            .iter()
            .zip(values)
            .filter_map(|(info, value)| Some(Entry::new(self.name(info.name), value?)))
            .collect();
        let mut value = Value::Map(entries);
        if let Some(variants) = self.union_of(parent.owner, bind) {
            // A union value is spelled by the node naming its variant, so
            // the variant is kept as the value's single key.
            if !variants.iter().any(|v| v.discriminator == frame.name) {
                let what = self.field_label(parent.owner, bind.field);
                let ty = self.item_type(parent.owner, bind).cloned();
                let expected =
                    ty.map_or_else(String::new, |ty| summary::expr(&self.def.names, &ty));
                let found = format!("node {}", self.name(frame.name));
                self.diagnose(cx, VALUE_TYPE, frame.span, &[&what, &expected, &found]);
                parent.rejected.push(bind.field);
                return;
            }
            value = Value::Map(vec![Entry::new(self.name(frame.name), value)]);
        }
        if bind.mode == Mode::Keyed {
            let Some((key, span)) = frame.key.take() else {
                return;
            };
            let ty = self.key_type(parent.owner, bind).cloned();
            let what = format!("the key of {}", self.name(frame.name));
            let Some(Value::String(key)) = self.check(cx, ty, &what, Value::String(key), span)
            else {
                return;
            };
            value = Value::Map(vec![Entry::new(key, value)]);
        }
        let span = frame.span;
        self.route(cx, parent, bind, value, span);
    }

    fn close_scalar(
        &mut self,
        cx: &mut Cx<'_>,
        parent: Option<&mut BindFrame>,
        frame: &mut BindFrame,
    ) {
        let Some(parent) = parent else {
            return;
        };
        let Some(bind) = parent.table(&self.tables).and_then(|t| t.child(frame.name)) else {
            return;
        };
        match frame.scalar.take() {
            Some((value, span)) => self.take(cx, parent, bind, value, span),
            None => {
                let node = self.name(parent.name);
                let what = format!("value of child {}", self.name(frame.name));
                self.diagnose(cx, MISSING_FIELD, frame.span, &[node, &what]);
            }
        }
    }
}

impl<'s> Processor for Binder<'_, 's> {
    type Node = NodeStart<'s>;
    type Entry = EntryValue<'s>;
    type Frame = BindFrame;

    fn open(
        &mut self,
        cx: &mut Cx<'_>,
        mut parent: Option<&mut BindFrame>,
        node: NodeStart<'s>,
    ) -> Disposition<BindFrame> {
        let span = cx.span();
        let mark = self.errors;
        let def = self.def;
        let owner_label = parent.as_ref().map_or(DOCUMENT_ROOT, |p| self.name(p.name));
        let scope = match parent.as_ref() {
            Some(frame) => frame.scope,
            None => Some(def.bindings.root_scope()),
        };
        let (Some(sym), Some(scope_id)) = (def.names.get(&node.name), scope) else {
            self.diagnose(cx, UNKNOWN_NODE, span, &[&node.name, owner_label]);
            return Disposition::Skip;
        };
        let Some(scope) = def.bindings.scope(scope_id) else {
            return Disposition::Skip;
        };
        let admitted = match parent.as_deref_mut() {
            Some(frame) => match frame.state.as_mut() {
                Some(state) => scope.rules.admit(state, sym),
                None => Ok(()),
            },
            None => scope.rules.admit(&mut self.root, sym),
        };
        if let Err(violation) = admitted {
            if parent.is_none() {
                self.order_broken |= matches!(
                    violation,
                    Violation::FirstNotAllowed { .. } | Violation::NotFollowing { .. }
                );
            }
            self.violation(cx, span, owner_label, &violation);
            return Disposition::Skip;
        }
        let parent_owner = parent.as_ref().map(|p| p.owner);
        let (kind, owner, inner) = match (scope.child(sym), parent_owner) {
            (Some(ChildPlan::Declared { decl, key }), _) => (
                Kind::Declared { decl, key },
                decl,
                def.bindings.decl_scope(decl),
            ),
            (Some(ChildPlan::Nested(inner)), Some(owner)) => (Kind::Nested, owner, Some(inner)),
            (Some(ChildPlan::Scalar), Some(owner)) => (Kind::Scalar, owner, None),
            _ => {
                self.diagnose(cx, UNKNOWN_NODE, span, &[&node.name, owner_label]);
                return Disposition::Skip;
            }
        };
        if node.annotated {
            self.diagnose(
                cx,
                UNBOUND_ENTRY,
                span,
                &["the type annotation", &node.name],
            );
        }
        let width = match kind {
            Kind::Declared { decl, .. } => self.tables.decls[decl.index()].fields.len(),
            Kind::Nested | Kind::Scalar => 0,
        };
        let state = inner
            .and_then(|id| def.bindings.scope(id))
            .map(|scope| scope.rules.start());
        Disposition::Bind(BindFrame::new(Opened {
            name: sym,
            owner,
            kind,
            scope: inner,
            state,
            span,
            width,
            mark,
        }))
    }

    fn entry(&mut self, cx: &mut Cx<'_>, frame: &mut BindFrame, entry: EntryValue<'s>) {
        let span = cx.span();
        let node = self.name(frame.name);
        if entry.annotated {
            let what = match &entry.name {
                Some(name) => format!("the type annotation of property {name}"),
                None => format!("the type annotation of argument {}", frame.args),
            };
            self.diagnose(cx, UNBOUND_ENTRY, span, &[&what, node]);
        }
        let Some(name) = entry.name else {
            let at = frame.args;
            frame.args = frame.args.saturating_add(1);
            if frame.kind == Kind::Scalar && at == 0 {
                frame.scalar = Some((entry.value, span));
                return;
            }
            if let Kind::Declared { key: Some(key), .. } = frame.kind
                && at == key
            {
                match entry.value {
                    Value::String(text) => frame.key = Some((text, span)),
                    other => {
                        let what = format!("the key of {node}");
                        let found = decode::found(&other);
                        self.diagnose(cx, VALUE_TYPE, span, &[&what, "String", &found]);
                    }
                }
                return;
            }
            match frame.table(&self.tables).and_then(|table| table.arg(at)) {
                Some(bind) => self.take(cx, frame, bind, entry.value, span),
                None => {
                    let what = format!("argument {at}");
                    self.diagnose(cx, UNBOUND_ENTRY, span, &[&what, node]);
                }
            }
            return;
        };
        let sym = self.def.names.get(&name);
        if let Some(sym) = sym
            && frame.props.contains(&sym)
        {
            self.diagnose(cx, DUPLICATE_PROPERTY, span, &[&name]);
            return;
        }
        let bind = sym.and_then(|sym| frame.table(&self.tables)?.prop(sym));
        match (sym, bind) {
            (Some(sym), Some(bind)) => {
                frame.props.push(sym);
                self.take(cx, frame, bind, entry.value, span);
            }
            _ => {
                let what = format!("property {name}");
                self.diagnose(cx, UNBOUND_ENTRY, span, &[&what, node]);
            }
        }
    }

    fn header(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &mut BindFrame,
        has_children: bool,
        children: &mut HeaderEffects<'_>,
    ) {
        let Kind::Declared { decl, .. } = frame.kind else {
            return;
        };
        let def = self.def;
        let Some(plan) = def.plan.node(decl).filter(|_| has_children) else {
            return;
        };
        let origin = OriginId(cx.node().index);
        let value_of = |field| self.header_value(cx, frame, field);
        let effects = self.effects(&plan.children_effects, origin, value_of);
        for effect in self.typed(cx, effects, frame.span) {
            children.push(effect);
        }
    }

    fn close(
        &mut self,
        cx: &mut Cx<'_>,
        mut parent: Option<&mut BindFrame>,
        mut frame: BindFrame,
        following: &mut FollowingEffects<'_>,
    ) -> Completion {
        let def = self.def;
        if let (Some(scope), Some(state)) = (frame.scope, frame.state.as_ref())
            && let Some(compiled) = def.bindings.scope(scope)
        {
            let owner = self.name(frame.name);
            for violation in compiled.rules.complete(state) {
                self.violation(cx, frame.span, owner, &violation);
            }
        }
        let item = match frame.kind {
            Kind::Declared { decl, key } => {
                self.close_declared(cx, parent.as_deref_mut(), &mut frame, key, following);
                def.bindings.get(decl).is_some_and(|d| d.role.is_some())
            }
            Kind::Nested => {
                if let Some(parent) = parent.as_deref_mut() {
                    for (bind, value, span) in std::mem::take(&mut frame.carried) {
                        self.route(cx, parent, bind, value, span);
                    }
                    parent.rejected.append(&mut frame.rejected);
                }
                false
            }
            Kind::Scalar => {
                self.close_scalar(cx, parent.as_deref_mut(), &mut frame);
                false
            }
        };
        let total = self.errors - frame.mark;
        if let Some(parent) = parent {
            // An item answers for its own errors; a part's errors are
            // its owner's.
            parent.excluded += if item { total } else { frame.excluded };
        }
        if total > frame.excluded {
            Completion::Failed
        } else {
            Completion::Complete
        }
    }

    fn finish(&mut self, cx: &mut Cx<'_>) {
        let def = self.def;
        if let Some(scope) = def.bindings.scope(def.bindings.root_scope()) {
            let span = cx.span();
            for violation in scope.rules.complete(&self.root) {
                if self.order_broken && violation == Violation::FirstMissing {
                    continue;
                }
                self.violation(cx, span, DOCUMENT_ROOT, &violation);
            }
        }
    }
}

/// A binding session over one data document: parse events, structural
/// match, typed binding, and a `BoundItem` stream. A parse failure or a
/// session limit ends the stream with its diagnostic as the last item.
pub struct BindingSession<'d, 's> {
    session: Session<ParseEvents<'s>, Binder<'d, 's>>,
    ended: bool,
    stopped: bool,
}

impl<'d, 's> BindingSession<'d, 's> {
    /// Bind `text`, read from `path`, against `def` as source `source`.
    pub fn new(def: &'d Definition, source: SourceId, path: &str, text: &'s str) -> Self {
        let context = Context::new(def.plan.context.layout.clone());
        Self::with_context(def, source, path, text, context)
    }

    /// Bind starting from `context`, such as an import's projection of
    /// its caller's context.
    pub fn with_context(
        def: &'d Definition,
        source: SourceId,
        path: &str,
        text: &'s str,
        context: Context,
    ) -> Self {
        let events = ParseEvents::new(Parser::new(text), text, path);
        let binder = Binder::new(def, source, path, text);
        Self {
            session: Session::with_context(source, events, binder, context),
            ended: false,
            stopped: false,
        }
    }

    /// The context as the latest item left it: at an import request, the
    /// context of the import site.
    pub fn context(&self) -> &Context {
        self.session.context()
    }

    /// The next bound item. `Ok(None)` ends the stream; an `Err` is a
    /// broken event-source or context contract, never a document error.
    pub fn next_item(&mut self) -> Result<Option<BoundItem>, SessionError> {
        if self.ended {
            return Ok(None);
        }
        loop {
            match self.session.next_item() {
                Ok(Some(item)) => {
                    // The binder emits only bound items.
                    if let Ok(item) = BoundItem::try_from(item) {
                        return Ok(Some(item));
                    }
                }
                Ok(None) => {
                    self.ended = true;
                    return Ok(None);
                }
                Err(error) => {
                    let Some(record) = error.diagnostic() else {
                        return Err(error);
                    };
                    self.ended = true;
                    self.stopped = true;
                    return Ok(Some(BoundItem::Diagnostic(Box::new(record.clone()))));
                }
            }
        }
    }

    /// Whether the document failed: an error diagnostic was emitted, or
    /// the stream stopped early.
    pub fn failed(&self) -> bool {
        self.stopped || self.session.processor().errors() > 0
    }
}
