//! The typed binder: a runtime processor that dispatches each node by
//! parent and name against a compiled definition, binds its entries and
//! children into typed fields, and emits bound items and diagnostics.

mod frame;
mod source;
mod tables;

use std::borrow::Cow;
use std::collections::BTreeMap;

use kdlith_diagnostic::registry::{lookup, render};
use kdlith_diagnostic::{ByteSpan, DiagnosticRecord, Label};
use kdlith_model::{
    BoundContribution, BoundDeclaration, BoundItem, BoundRecord, Derivation, Entry,
    ExpansionRequest, Field, Identity, ImportRequest, Location, Origin, OriginId, PendingBinding,
    QualifiedName, Reference, Repeated, ScopeCompletion, SourceId, Trace, ValidationState, Value,
};
use kdlith_parse::{Event, Locator, Parser};
use kdlith_runtime::{
    Completion, Context, ContextEffect, ContextTarget, Cx, Disposition, FollowingEffects,
    HandlerCall, HandlerEffect, Handlers, HeaderEffects, Processor, ResolvedStream, Session,
    SessionError, SlotId, SlotValue,
};

use crate::bindings::{
    BindingCodec, ChildPlan, ContextPolicy, DependentBinding, NodeDeclId, Role, ScopeId, SetFrom,
};
use crate::decode::{self, Decoder, Mismatch};
use crate::definition::{
    AccumulateOp, Definition, DefinitionKind, MergeDuplicates, ProfileSelection, ProviderDecl,
    SlotDuplicates,
};
use crate::diag::{
    CONFLICTING_BINDING, DUPLICATE_KEY, MALFORMED, MISSING_FIELD, NOT_INSTALLED, TYPE_EXPRESSION,
    UNBOUND_ENTRY, UNKNOWN_MEMBER, UNKNOWN_NODE, VALUE_TYPE, sibling_note,
};
use crate::names::Sym;
use crate::overlap;
use crate::plan::EffectStep;
use crate::structure::{ScopeRules, ScopeState, Violation};
use crate::summary;
use crate::types::{
    Constraint, ObjectType, OutputPolicy, Sensitivity, TypeExpr, TypeKind, Variant,
};
use crate::value_resolver::{ResolutionCall, SharedResolver};

pub use frame::BindFrame;
pub use source::ParseEvents;

use frame::{Kind, Opened, Sibling, Supplied};
use tables::{Bind, Collect, FieldInfo, IdentityBind, Mode, SlotInfo, Tables, optional};

const COLLISION: &str = "KDLITH-BND007";
const DUPLICATE_PROPERTY: &str = "KDLITH-SYN003";
const CONTEXT_UNAVAILABLE: &str = "KDLITH-CTX001";
const CONTEXT_DISAGREES: &str = "KDLITH-CTX002";
const SLOT_DUPLICATE: &str = "KDLITH-CTX006";
const HANDLER_UNDECLARED: &str = "KDLITH-CTX005";
const PROFILE_NOT_SELECTED: &str = "KDLITH-SCH011";
const UNRESOLVED_VALUE: &str = "KDLITH-BND009";
const UNSUPPORTED_KEY: &str = "KDLITH-BND010";
const CONSTRAINT_VIOLATED: &str = "KDLITH-BND011";

const DOCUMENT_ROOT: &str = "the document root";

/// What a diagnostic shows in place of a value the output policy
/// withholds.
const CONCEALED: &str = "a withheld sensitive value";

/// The outcome of a node's case dispatch.
enum CaseSelection {
    /// The node declares no cases.
    None,
    /// A case (or `otherwise`) selected; the type name, when the
    /// selected type names one, and the property of the case rule that
    /// matched (`None` for `otherwise`).
    Selected {
        typed: Option<Sym>,
        property: Option<Sym>,
    },
    /// No case matched and there is no `otherwise`.
    Unmatched,
}

/// The dialect construct a schema definition's root node is written as.
const SCHEMA_ROOT: &str = "schema";

/// The include short form's declared type, target field, and the owner
/// field its splice point counts from.
const INCLUDE_TYPE: &str = "IncludeRef";
const INCLUDE_TARGET: &str = "target";
const INCLUDE_NAME: &str = "name";

/// The fields of the node an effect step runs for, as its timing knows
/// them: the header alone for children effects, the node's fully bound
/// values for following effects.
#[derive(Clone, Copy)]
enum FieldView<'a> {
    Header,
    Bound(&'a [Option<Value>]),
}

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

/// Positional arguments a dependent payload may hold at most; more
/// fails closed rather than growing without bound.
const MAX_PAYLOAD: usize = 32;

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
    /// The explicit output policy for sensitive fields; `None` applies
    /// each sensitivity's fail-closed default.
    output_policy: Option<OutputPolicy>,
    /// The profiles whose node declarations the active selection
    /// admits: each active profile and every profile it extends, in
    /// selection then chain order; empty when none is selected.
    profile_chain: Vec<Sym>,
    /// The root types the active selection admits: the union of each
    /// active profile's roots composed along its chain; `None` admits
    /// every declared root.
    profile_roots: Option<Vec<Sym>>,
    /// The explicit profile name as the invocation spelled it, when it
    /// selected nothing, so a rejected name reports itself rather than
    /// a missing one.
    profile_named: Option<String>,
    /// Whether selection needs no report: a profile was selected, or the
    /// definition declares none and none was named. Otherwise the first
    /// root node fails closed (`KDLITH-SCH011`).
    profile_reported: bool,
    /// A root node broke the document's order rule, so the missing first
    /// node is not reported again at the end.
    order_broken: bool,
    /// The name of the document's first root node, once it opened.
    root_name: Option<String>,
    /// The root node bound last, while no later root node has opened.
    root_last: Option<Sibling>,
    /// The `as=` aliases this document's imports bound, with the span of
    /// the first import that bound each; an alias is unique in its
    /// document.
    aliases: Vec<(String, ByteSpan)>,
    /// The context handlers the reading processor installed: a handler
    /// effect executes through this registry at its declared timing,
    /// and one it lacks fails at its site.
    handlers: Handlers,
    /// The value resolver the consuming program registered: a field
    /// whose declared fallback is computed and that nothing else
    /// supplies is resolved through it. Without one, such a field stays
    /// absent, as the descriptor-only contract has it.
    resolver: Option<SharedResolver>,
    /// Whether any field was put to the resolver, resolved or not.
    consulted: bool,
    /// The non-secret values the resolver supplied, folded in
    /// resolution order: part of the result's cache identity.
    resolved: ResolvedStream,
    /// Whether the resolver supplied a value for a `secret` field; such
    /// a result is never cached.
    resolved_secret: bool,
    /// The context the document's root level left, captured as the
    /// stream ends and before its scope restores the entry context.
    left: Option<Vec<Option<SlotValue>>>,
    /// When one node is bound alone, the scope its parent dispatches
    /// it in; the document's root rules then do not apply to it.
    window: Option<ScopeId>,
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
        Self::with_output_policy(def, source, path, text, None)
    }

    /// A binder applying one explicit output policy to every sensitive
    /// field; `None` redacts a secret and emits a reference.
    pub fn with_output_policy(
        def: &'d Definition,
        source: SourceId,
        path: impl Into<String>,
        text: &'s str,
        output_policy: Option<OutputPolicy>,
    ) -> Self {
        Self::with_profile_and_policy(def, source, path, text, None, output_policy)
    }

    /// A binder applying the profile `profile` the invocation named.
    /// `None` against a definition that declares profiles fails closed.
    pub fn with_profile(
        def: &'d Definition,
        source: SourceId,
        path: impl Into<String>,
        text: &'s str,
        profile: Option<&str>,
    ) -> Self {
        Self::with_profile_and_policy(def, source, path, text, profile, None)
    }

    /// A binder applying the named profile and one explicit output
    /// policy. The profile is selected for `path` as an entry document
    /// by [`Definition::select_profile`]: the named one, else the first
    /// matching declared rule, else the default. An unknown name, or no
    /// selection at all, fails closed.
    pub fn with_profile_and_policy(
        def: &'d Definition,
        source: SourceId,
        path: impl Into<String>,
        text: &'s str,
        profile: Option<&str>,
        output_policy: Option<OutputPolicy>,
    ) -> Self {
        let path = path.into();
        let selection = def.select_profile(profile, &[], &path);
        Self::with_selection(def, source, path, text, &selection, output_policy)
    }

    /// A binder applying a profile selection already resolved for the
    /// invocation, so a document an import reaches binds under its
    /// importing entry's selection rather than one inferred from its
    /// own path.
    pub fn with_selection(
        def: &'d Definition,
        source: SourceId,
        path: impl Into<String>,
        text: &'s str,
        selection: &ProfileSelection,
        output_policy: Option<OutputPolicy>,
    ) -> Self {
        let root = def
            .bindings
            .scope(def.bindings.root_scope())
            .map_or_else(|| ScopeRules::default().start(), |s| s.rules.start());
        let (profile_chain, profile_roots) = active_profiles(def, selection.members());
        let (profile_named, profile_reported) = match selection {
            ProfileSelection::Unselected { named } => (named.clone(), false),
            _ => (None, true),
        };
        Self {
            def,
            tables: tables::build(def),
            decoder: Decoder::new(&def.types).0.with_references(
                &def.types,
                &def.names,
                def.symbols.references(),
            ),
            path: path.into(),
            locator: Locator::new(text),
            source,
            root,
            output_policy,
            profile_chain,
            profile_roots,
            profile_named,
            profile_reported,
            order_broken: false,
            root_name: None,
            root_last: None,
            aliases: Vec::new(),
            handlers: Handlers::new(),
            resolver: None,
            consulted: false,
            resolved: ResolvedStream::new(),
            left: None,
            window: None,
            resolved_secret: false,
            errors: 0,
        }
    }

    /// The number of error diagnostics emitted so far.
    pub fn errors(&self) -> u64 {
        self.errors
    }

    /// The name of the document's first root node, once it has opened.
    pub fn root_name(&self) -> Option<&str> {
        self.root_name.as_deref()
    }

    /// A node no binding accepts here, with a note when the sibling
    /// bound just before it admits it as a child and the node is
    /// indented past that sibling.
    fn unknown(
        &mut self,
        cx: &mut Cx<'_>,
        span: ByteSpan,
        node: &str,
        owner: &str,
        previous: Option<Sibling>,
    ) {
        let note = previous.and_then(|sibling| self.sibling_label(span, node, sibling));
        self.diagnose_noted(cx, UNKNOWN_NODE, span, &[node, owner], note);
    }

    fn sibling_label(&mut self, span: ByteSpan, node: &str, sibling: Sibling) -> Option<Label> {
        let def = self.def;
        let sym = def.names.get(node)?;
        def.bindings.child(sibling.scope?, sym)?;
        let at = self.locator.locate(span).start;
        let above = self.locator.locate(sibling.span).start;
        if at.column <= above.column {
            return None;
        }
        let message = sibling_note(self.name(sibling.name), node);
        let label = Label::new(Some(self.path.clone()), sibling.span, message)
            .with_position(above.line, above.column);
        Some(label)
    }

    /// Report a value that did not decode as `ty`, under the code that
    /// names why: see [`decode::mismatch`].
    fn mismatched(
        &mut self,
        cx: &mut Cx<'_>,
        ty: &TypeExpr,
        what: &str,
        value: &Value,
        span: ByteSpan,
    ) {
        self.mismatched_as(cx, ty, what, value, span, false);
    }

    /// Report a value that fails its type; a concealed value never
    /// reaches the diagnostic, only the fact that it failed.
    fn mismatched_as(
        &mut self,
        cx: &mut Cx<'_>,
        ty: &TypeExpr,
        what: &str,
        value: &Value,
        span: ByteSpan,
        conceal: bool,
    ) {
        let def = self.def;
        let found = if conceal {
            CONCEALED.to_owned()
        } else {
            decode::found(value)
        };
        match decode::mismatch(&def.types, &def.names, ty, value) {
            Mismatch::Value => {
                let expected = summary::expr(&def.names, ty);
                self.diagnose(cx, VALUE_TYPE, span, &[what, &expected, &found]);
            }
            Mismatch::Member { ty } => {
                self.diagnose(cx, UNKNOWN_MEMBER, span, &[what, &ty, &found]);
            }
            Mismatch::TypeExpression { reason } => {
                let text = match value {
                    _ if conceal => CONCEALED,
                    Value::String(text) => text.as_str(),
                    _ => "",
                };
                self.diagnose(cx, TYPE_EXPRESSION, span, &[text, &reason]);
            }
        }
    }

    fn name(&self, sym: Sym) -> &'d str {
        self.def.names.resolve(sym)
    }

    fn location(&mut self, span: ByteSpan) -> Location {
        Location::new(self.source, self.locator.locate(span))
    }

    fn diagnose(&mut self, cx: &mut Cx<'_>, code: &'static str, span: ByteSpan, values: &[&str]) {
        self.diagnose_noted(cx, code, span, values, None);
    }

    fn diagnose_noted(
        &mut self,
        cx: &mut Cx<'_>,
        code: &'static str,
        span: ByteSpan,
        values: &[&str],
        note: Option<Label>,
    ) {
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
        let record = match note {
            Some(label) => record.with_label(label),
            None => record,
        };
        self.errors += 1;
        cx.emit(BoundItem::Diagnostic(Box::new(record)));
    }

    fn violation(
        &mut self,
        cx: &mut Cx<'_>,
        span: ByteSpan,
        owner: &str,
        violation: &Violation,
        previous: Option<Sibling>,
    ) {
        let code = violation.code();
        let (what, count, expected) = match violation {
            Violation::NotPermitted { child } => {
                let child = self.name(*child);
                self.unknown(cx, span, child, owner, previous);
                return;
            }
            Violation::FirstNotAllowed { child } | Violation::NotFollowing { child } => {
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
            Violation::SequenceRejected { child, step } => (
                format!("{} under {owner}", self.name(*child)),
                format!("step {}", step + 1),
                "a position the ordered sequence admits".to_owned(),
            ),
            Violation::SequenceIncomplete { step } => (
                format!("the ordered sequence under {owner}"),
                format!("stopped at step {}", step + 1),
                "every step within its bounds".to_owned(),
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
        self.check_as(cx, ty, what, value, span, false)
    }

    /// Check a scalar against `ty`; a concealed value's mismatch never
    /// names the value.
    fn check_as(
        &mut self,
        cx: &mut Cx<'_>,
        ty: Option<TypeExpr>,
        what: &str,
        value: Value,
        span: ByteSpan,
        conceal: bool,
    ) -> Option<Value> {
        let Some(ty) = ty else {
            return Some(value);
        };
        let site = self.location(span);
        // The namespace in effect at this position, never a later one.
        let namespace = self
            .tables
            .namespace
            .and_then(|slot| cx.context().get(slot))
            .and_then(|held| match &held.value {
                Value::String(text) => Some(text.clone()),
                _ => None,
            });
        let at = (site, namespace.as_deref());
        let decoded = self
            .decoder
            .decode_at(&self.def.types, &self.def.names, &ty, &value, at);
        if decoded.is_none() {
            self.mismatched_as(cx, &ty, what, &value, span, conceal);
        }
        decoded
    }

    /// The policy a sensitive field's value passes on every output
    /// surface: the explicit one, or the sensitivity's fail-closed
    /// default.
    fn policy_for(&self, sensitivity: Sensitivity) -> OutputPolicy {
        self.output_policy
            .unwrap_or_else(|| OutputPolicy::default_for(sensitivity))
    }

    /// Whether a field's value is withheld from output, and so from
    /// every diagnostic that would otherwise quote it.
    fn conceals(&self, owner: NodeDeclId, field: usize) -> bool {
        self.tables.decls[owner.index()].fields[field]
            .sensitivity
            .is_some_and(|sensitivity| self.policy_for(sensitivity) != OutputPolicy::Emit)
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
        let conceal = self.conceals(owner, bind.field);
        match self.check_as(cx, ty, &what, value, span, conceal) {
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
                    more,
                    ..
                }) => {
                    items.push(value);
                    more.push(span);
                }
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
                        more,
                        ..
                    }) => {
                        if entries.iter().any(|held| held.key == entry.key) {
                            let key = entry.key;
                            self.diagnose(cx, DUPLICATE_KEY, span, &[&key, &label]);
                        } else {
                            entries.push(entry);
                            more.push(span);
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
        let mut traces = Vec::with_capacity(count);
        let site = self.location(frame.span);
        for at in 0..count {
            let info = self.tables.decls[owner.index()].fields[at].clone();
            let explicit = frame.fields[at].take();
            let held = info
                .context
                .and_then(|context| cx.context().get(context.slot))
                .cloned();
            let explicit_trace = explicit
                .as_ref()
                .map(|supplied| self.supplied_trace(supplied));
            let (value, mut trace) = match (explicit, info.context) {
                (Some(supplied), Some(context)) => {
                    if context.policy == Some(ContextPolicy::Fixed)
                        && let Some(held) = &held
                        && held.value != supplied.value
                    {
                        let field = self.field_label(owner, at);
                        let (ours, theirs) = if self.conceals(owner, at) {
                            (CONCEALED.to_owned(), CONCEALED.to_owned())
                        } else {
                            (decode::found(&supplied.value), decode::found(&held.value))
                        };
                        let slot = self.name(context.name);
                        let values = [field.as_str(), &ours, slot, &theirs];
                        self.diagnose(cx, CONTEXT_DISAGREES, supplied.span, &values);
                    }
                    (Some(supplied.value), explicit_trace.unwrap_or_default())
                }
                (Some(supplied), None) => {
                    (Some(supplied.value), explicit_trace.unwrap_or_default())
                }
                (None, Some(context)) => {
                    if held.is_none()
                        && context.policy.is_none()
                        && info.required
                        && info.fallback.is_none()
                    {
                        let slot = self.name(context.name);
                        self.diagnose(cx, CONTEXT_UNAVAILABLE, frame.span, &[slot]);
                        values.push(None);
                        traces.push(Trace::default());
                        continue;
                    }
                    match held {
                        Some(held) => (
                            Some(held.value),
                            Trace::derived(Derivation::Context, site, vec![held.trace]),
                        ),
                        None => (None, Trace::default()),
                    }
                }
                (None, None) => (None, Trace::default()),
            };
            let defaulted = value.is_none();
            let mut resolved = None;
            let value = match value.or(info.fallback.clone()) {
                Some(value) => Some(value),
                None => match self.resolve_value(cx, frame, at, &info) {
                    Resolution::Resolved(value, origin) => {
                        resolved = Some(origin);
                        Some(value)
                    }
                    Resolution::Failed => {
                        frame.rejected.push(at);
                        None
                    }
                    Resolution::NotAsked => None,
                },
            };
            let value = value.or(match info.collect {
                Collect::List => Some(Value::List(Vec::new())),
                Collect::Map => Some(Value::Map(Vec::new())),
                Collect::No => None,
            });
            if defaulted && value.is_some() {
                trace = resolved
                    .unwrap_or_else(|| Trace::derived(Derivation::Default, site, Vec::new()));
            }
            let expands = !self.tables.decls[owner.index()].fields[at]
                .expand
                .is_empty();
            let value = match value {
                Some(Value::Map(entries)) if expands => {
                    let mut entries = entries;
                    let before = entries.len();
                    self.splice_expansions(cx, owner, at, frame.span, &mut entries);
                    if entries.len() > before {
                        trace = Trace::derived(Derivation::Expansion, site, vec![trace]);
                    }
                    Some(Value::Map(entries))
                }
                None if expands => {
                    let mut entries = Vec::new();
                    self.splice_expansions(cx, owner, at, frame.span, &mut entries);
                    if !entries.is_empty() {
                        trace = Trace::derived(Derivation::Expansion, site, Vec::new());
                    }
                    (!entries.is_empty()).then_some(Value::Map(entries))
                }
                other => other,
            };
            traces.push(if value.is_some() {
                trace
            } else {
                Trace::default()
            });
            if value.is_none() && info.required && !frame.rejected.contains(&at) {
                let node = self.name(frame.name);
                let field = self.name(info.name);
                self.diagnose(cx, MISSING_FIELD, frame.span, &[node, field]);
            }
            values.push(value);
        }
        frame.traces = traces;
        values
    }

    /// Resolve an absent computed-default field through the registered
    /// value resolver, descriptors passed as data only. A decline is
    /// `KDLITH-BND009`, a mistyped value `KDLITH-BND003`.
    fn resolve_value(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &BindFrame,
        at: usize,
        info: &FieldInfo,
    ) -> Resolution {
        let (Some(dependencies), Some(resolver)) = (&info.computed, self.resolver.clone()) else {
            return Resolution::NotAsked;
        };
        self.consulted = true;
        let def = self.def;
        let providers: Vec<&ProviderDecl> = dependencies
            .iter()
            .filter_map(|dep| def.providers.iter().find(|provider| provider.id == *dep))
            .collect();
        let mut context = BTreeMap::new();
        for (slot, slot_info) in cx.context().layout().slots().zip(self.tables.slots.iter()) {
            if let Some(held) = cx.context().get(slot) {
                context.insert(self.name(slot_info.name).to_owned(), held.value.clone());
            }
        }
        let ty = info
            .ty
            .as_ref()
            .map(|ty| summary::expr(&def.names, ty))
            .unwrap_or_default();
        let site = self.location(frame.span);
        let outcome = {
            let call = ResolutionCall {
                source: &self.path,
                site,
                node: self.name(frame.name),
                field: self.name(info.name),
                ty: &ty,
                sensitivity: info.sensitivity,
                dependencies,
                providers: &providers,
                context: &context,
            };
            resolver.resolve(&call)
        };
        let label = self.field_label(frame.owner, at);
        match outcome {
            Ok(value) => {
                let conceal = self.conceals(frame.owner, at);
                let checked =
                    self.check_as(cx, info.ty.clone(), &label, value, frame.span, conceal);
                match checked {
                    Some(value) => {
                        if info.sensitivity == Some(Sensitivity::Secret) {
                            self.resolved_secret = true;
                        } else {
                            let span = (frame.span.start, frame.span.end);
                            let field = self.name(info.name);
                            self.resolved.push(span, field, &value);
                        }
                        let named = providers.iter().map(|p| p.id.clone()).collect();
                        Resolution::Resolved(value, Trace::resolved(site, named))
                    }
                    None => Resolution::Failed,
                }
            }
            Err(unresolvable) => {
                let node = self.name(frame.name);
                let values = [label.as_str(), node, unresolvable.reason()];
                self.diagnose(cx, UNRESOLVED_VALUE, frame.span, &values);
                Resolution::Failed
            }
        }
    }

    /// How an explicitly supplied value came to be: written literally
    /// at its span, or — for a collection gathered from several
    /// supplies — merged at the node from each literal supply.
    fn supplied_trace(&mut self, supplied: &Supplied) -> Trace {
        let first = Trace::literal(self.location(supplied.span));
        if supplied.more.is_empty() {
            return first;
        }
        let mut inputs = vec![first];
        for span in &supplied.more {
            inputs.push(Trace::literal(self.location(*span)));
        }
        let start = supplied.span.start;
        let end = supplied
            .more
            .iter()
            .map(|span| span.end)
            .max()
            .unwrap_or(start);
        let gathered = self.location(ByteSpan::new(start, end.max(supplied.span.end)));
        Trace::derived(Derivation::Merge, gathered, inputs)
    }

    /// Splice the entries of every expansion a field's sources declare
    /// into its collection, rejecting a key that collides with one
    /// already held.
    fn splice_expansions(
        &mut self,
        cx: &mut Cx<'_>,
        owner: NodeDeclId,
        at: usize,
        span: ByteSpan,
        entries: &mut Vec<Entry>,
    ) {
        let expands = self.tables.decls[owner.index()].fields[at].expand.clone();
        for (target, member) in expands {
            let Some(spliced) = self.expanded_entries(target, member) else {
                continue;
            };
            for (key, value) in spliced {
                if entries.iter().any(|held| held.key == key) {
                    let field = self.field_label(owner, at);
                    self.diagnose(cx, DUPLICATE_KEY, span, &[&key, &field]);
                } else {
                    entries.push(Entry::new(key, value));
                }
            }
        }
    }

    /// The rendered entries of one declaration's keyed member, as bound
    /// children of that member's kind would carry them.
    fn expanded_entries(&self, target: Sym, member: Sym) -> Option<Vec<(String, Value)>> {
        let names = &self.def.names;
        let member = names.resolve(member);
        let decl = self
            .def
            .types
            .lookup(target)
            .and_then(|id| self.def.types.get(id))?;
        let expr = |ty: &TypeExpr| Value::String(summary::expr(names, ty));
        match &decl.kind {
            TypeKind::Object(object) if member == "fields" => Some(
                object
                    .fields
                    .iter()
                    .map(|field| {
                        let mut entries = vec![Entry::new("type".to_owned(), expr(&field.ty))];
                        if let Some(description) = &field.description {
                            entries.push(Entry::new(
                                "description".to_owned(),
                                Value::String(description.clone()),
                            ));
                        }
                        if let Some(default) = &field.documented_default {
                            entries
                                .push(Entry::new("documented-default".to_owned(), default.clone()));
                        }
                        if let Some(fallback) = &field.fallback {
                            entries.push(Entry::new("fallback".to_owned(), fallback.to_bound()));
                        }
                        if let Some(sensitivity) = field.sensitivity {
                            entries.push(Entry::new(
                                "sensitivity".to_owned(),
                                Value::String(sensitivity.name().to_owned()),
                            ));
                        }
                        (names.resolve(field.name).to_owned(), Value::Map(entries))
                    })
                    .collect(),
            ),
            TypeKind::Enum(enumeration) if member == "members" => Some(
                enumeration
                    .members
                    .iter()
                    .map(|member| {
                        let entries = member.description.clone().map(|description| {
                            vec![Entry::new(
                                "description".to_owned(),
                                Value::String(description),
                            )]
                        });
                        (member.name.clone(), Value::Map(entries.unwrap_or_default()))
                    })
                    .collect(),
            ),
            TypeKind::Union(union) if member == "variants" => Some(
                union
                    .variants
                    .iter()
                    .map(|variant| {
                        let mut entries = vec![Entry::new("type".to_owned(), expr(&variant.ty))];
                        if let Some(prefix) = &variant.prefix {
                            entries.push(Entry::new(
                                "prefix".to_owned(),
                                Value::String(prefix.clone()),
                            ));
                        }
                        (
                            names.resolve(variant.discriminator).to_owned(),
                            Value::Map(entries),
                        )
                    })
                    .collect(),
            ),
            TypeKind::Codec(codec) if member == "captures" => Some(
                codec
                    .captures
                    .iter()
                    .map(|capture| {
                        (
                            capture.name.clone(),
                            Value::Map(vec![Entry::new("type".to_owned(), expr(&capture.ty))]),
                        )
                    })
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Evaluate a node's case dispatch, top-down, first match winning:
    /// a property present and scalar-equal selects the type the node
    /// binds as; present but unequal falls through; no match and no
    /// `otherwise` rejects the node as unbound.
    fn select_case(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &BindFrame,
        values: &[Option<Value>],
    ) -> CaseSelection {
        let info = &self.tables.decls[frame.owner.index()];
        if info.cases.is_empty() {
            return CaseSelection::None;
        }
        for case in &info.cases {
            let held = info
                .fields
                .iter()
                .position(|field| field.name == case.property)
                .and_then(|at| values.get(at))
                .is_some_and(|value| value.as_ref() == Some(&case.equals));
            if held {
                return match &case.ty {
                    TypeExpr::Named(name) => CaseSelection::Selected {
                        typed: Some(*name),
                        property: Some(case.property),
                    },
                    _ => CaseSelection::Selected {
                        typed: None,
                        property: Some(case.property),
                    },
                };
            }
        }
        match &info.otherwise {
            Some(TypeExpr::Named(name)) => CaseSelection::Selected {
                typed: Some(*name),
                property: None,
            },
            Some(_) => CaseSelection::Selected {
                typed: None,
                property: None,
            },
            None => {
                let node = self.name(frame.name);
                let place = "no case of its cases block matches and it has no otherwise";
                self.diagnose(cx, UNKNOWN_NODE, frame.span, &[node, place]);
                CaseSelection::Unmatched
            }
        }
    }

    /// The object type `name` declares, when it declares one.
    fn object_type(&self, name: Sym) -> Option<&'d ObjectType> {
        let def = self.def;
        let id = def.types.lookup(name)?;
        match &def.types.get(id)?.kind {
            TypeKind::Object(object) => Some(object),
            _ => None,
        }
    }

    /// A case-selected type's required fields, checked as the node's own
    /// are: one the node holds no value for is missing (`KDLITH-BND004`).
    /// The record carries the node's fields, so the selected type's
    /// fallbacks supply nothing.
    fn selected_required(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &BindFrame,
        values: &[Option<Value>],
        typed_as: Option<Sym>,
    ) {
        let own = self.def.bindings.get(frame.owner).and_then(|decl| decl.ty);
        let Some(selected) = typed_as.filter(|selected| Some(*selected) != own) else {
            return;
        };
        let Some(object) = self.object_type(selected) else {
            return;
        };
        for field in object.fields.iter().filter(|f| !optional(&f.ty)) {
            if self.held(frame, values, field.name) {
                continue;
            }
            let node = self.name(frame.name);
            let missing = format!("{}.{}", self.name(selected), self.name(field.name));
            self.diagnose(cx, MISSING_FIELD, frame.span, &[node, &missing]);
        }
    }

    /// Whether the node holds a value for the field named `name`. `#null`
    /// and an empty collection are values; only a field holding none is
    /// absent.
    fn held(&self, frame: &BindFrame, values: &[Option<Value>], name: Sym) -> bool {
        self.tables.decls[frame.owner.index()]
            .fields
            .iter()
            .position(|info| info.name == name)
            .and_then(|at| values.get(at))
            .is_some_and(Option::is_some)
    }

    /// Enforce the object constraints of every type the node binds as —
    /// its own declared type and the type a case selected
    /// (`KDLITH-BND011`): `requires F G` fails when F holds a value and G
    /// none, and `all-or-none` fails for each listed field left absent
    /// while another holds a value.
    fn constraints(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &BindFrame,
        values: &[Option<Value>],
        typed_as: Option<Sym>,
    ) {
        let own = self.def.bindings.get(frame.owner).and_then(|decl| decl.ty);
        let mut types = own.into_iter().collect::<Vec<_>>();
        if let Some(selected) = typed_as.filter(|selected| Some(*selected) != own) {
            types.push(selected);
        }
        for ty in types {
            let Some(object) = self.object_type(ty) else {
                continue;
            };
            for constraint in &object.constraints {
                let (spelled, unmet) = match constraint {
                    Constraint::Requires { field, required } => {
                        let spelled =
                            format!("requires {} {}", self.name(*field), self.name(*required));
                        let unmet = (self.held(frame, values, *field)
                            && !self.held(frame, values, *required))
                        .then(|| (*field, vec![*required]));
                        (spelled, unmet)
                    }
                    Constraint::AllOrNone(fields) => {
                        let names: Vec<&str> = fields.iter().map(|f| self.name(*f)).collect();
                        let spelled = format!("all-or-none {}", names.join(" "));
                        let (held, absent): (Vec<Sym>, Vec<Sym>) = fields
                            .iter()
                            .partition(|field| self.held(frame, values, **field));
                        let unmet = held
                            .first()
                            .filter(|_| !absent.is_empty())
                            .map(|first| (*first, absent));
                        (spelled, unmet)
                    }
                };
                let Some((trigger, absent)) = unmet else {
                    continue;
                };
                let node = self.name(frame.name);
                let constraint = format!("{spelled} of {}", self.name(ty));
                let trigger = self.name(trigger);
                for missing in absent {
                    let missing = self.name(missing);
                    let values = [node, constraint.as_str(), trigger, missing];
                    self.diagnose(cx, CONSTRAINT_VIOLATED, frame.span, &values);
                }
            }
        }
    }

    /// Validate a node's dependent payload against its `against` target's
    /// member type, never promoting a scalar to a list. A payload whose
    /// codec or target type this reading lacks stays a pending binding;
    /// an absent target field binds nothing.
    fn dependent_payload(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &mut BindFrame,
        values: &[Option<Value>],
    ) -> Option<PendingBinding> {
        let dep = self.tables.decls[frame.owner.index()].dependent.clone()?;
        let infos = &self.tables.decls[frame.owner.index()];
        let at = infos
            .fields
            .iter()
            .position(|field| field.name == dep.target)?;
        let Some(Some(Value::Reference(reference))) = values.get(at) else {
            return None;
        };
        let Some(domain) = self.def.names.get(&reference.domain) else {
            return Some(self.pending(cx, frame, &dep, reference));
        };
        let Some(TypeKind::Object(object)) = self
            .def
            .types
            .lookup(domain)
            .and_then(|id| self.def.types.get(id))
            .map(|decl| &decl.kind)
        else {
            return None;
        };
        let field = object
            .fields
            .iter()
            .find(|field| field.name == dep.member)?;
        let expected = field.ty.clone();
        if dep.codec != BindingCodec::ArgumentTail {
            return Some(self.pending(cx, frame, &dep, reference));
        }
        let node = self.name(frame.name);
        let payload = std::mem::take(&mut frame.payload);
        let what = format!("the payload of {node}");
        match &expected {
            TypeExpr::List(inner) => {
                let inner = (**inner).clone();
                for (value, span) in payload {
                    self.check(cx, Some(inner.clone()), &what, value, span);
                }
            }
            _ => match payload.len() {
                0 => {}
                1 => {
                    let (value, span) = payload.into_iter().next().expect("one payload value");
                    self.check(cx, Some(expected), &what, value, span);
                }
                count => {
                    let expected = summary::expr(&self.def.names, &expected);
                    self.diagnose(
                        cx,
                        crate::diag::CARDINALITY,
                        frame.span,
                        &[
                            &what,
                            &count.to_string(),
                            &format!("at most 1 for {expected}"),
                        ],
                    );
                }
            },
        }
        None
    }

    /// The node's payload kept raw against its bound reference: the one
    /// value it holds, or a list of every value, with the node and its
    /// origin as the source locator.
    fn pending(
        &self,
        cx: &Cx<'_>,
        frame: &mut BindFrame,
        dep: &DependentBinding,
        against: &Reference,
    ) -> PendingBinding {
        let mut payload: Vec<Value> = std::mem::take(&mut frame.payload)
            .into_iter()
            .map(|(value, _)| value)
            .collect();
        let raw = match payload.len() {
            1 => payload.remove(0),
            _ => Value::List(payload),
        };
        let node = cx.node();
        let field = self.name(dep.target);
        PendingBinding::new(node, field, against.clone(), raw, OriginId(node.index)).with_dependent(
            self.name(frame.name),
            self.name(dep.member),
            dep.codec.name(),
        )
    }

    /// Resolve effect steps into runtime effects, in declared order:
    /// declarative steps read the fields they name, and a handler step
    /// runs its installed implementation with its results spliced in at
    /// the handler's position.
    fn effects(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &BindFrame,
        steps: &[EffectStep],
        target: ContextTarget,
        view: FieldView<'_>,
    ) -> Vec<ContextEffect> {
        let mut out = Vec::new();
        for step in steps {
            match step {
                EffectStep::Set { slot, from, layer } => {
                    let site = self.location(frame.span);
                    let (value, trace) = match from {
                        // The schema's own constant, set at this node.
                        SetFrom::Value(value) => (
                            Some(value.clone()),
                            Trace::derived(Derivation::Context, site, Vec::new()),
                        ),
                        SetFrom::Field(field) => (
                            self.field_value(cx, frame, view, *field),
                            self.field_trace(frame, view, *field)
                                .unwrap_or_else(|| Trace::literal(site)),
                        ),
                    };
                    // A set from an absent optional field leaves the slot
                    // as it was rather than inventing a removal.
                    if let Some(value) = value {
                        out.push(ContextEffect::Set {
                            slot: *slot,
                            value: SlotValue::new(value, OriginId(cx.node().index))
                                .with_trace(trace)
                                .with_layer(layer.map(|layer| self.name(layer).to_owned())),
                        });
                    }
                }
                EffectStep::Remove { slot } => out.push(ContextEffect::Remove { slot: *slot }),
                EffectStep::Clear { namespace } => out.push(ContextEffect::Clear {
                    namespace: *namespace,
                }),
                EffectStep::Handler { name } => {
                    self.run_handler(cx, frame, name, target, view, &mut out)
                }
            }
        }
        out
    }

    /// How one field of the node an effect runs for came to be, when
    /// its timing knows: a bound field's resolved trace.
    fn field_trace(&self, frame: &BindFrame, view: FieldView<'_>, field: Sym) -> Option<Trace> {
        match view {
            FieldView::Header => None,
            FieldView::Bound(_) => {
                let infos = &self.tables.decls[frame.owner.index()].fields;
                let at = infos.iter().position(|info| info.name == field)?;
                frame
                    .traces
                    .get(at)
                    .filter(|trace| !trace.is_empty())
                    .cloned()
            }
        }
    }

    /// One field of the node an effect runs for, as its timing knows it.
    fn field_value(
        &self,
        cx: &Cx<'_>,
        frame: &BindFrame,
        view: FieldView<'_>,
        field: Sym,
    ) -> Option<Value> {
        match view {
            FieldView::Header => self.header_value(cx, frame, field),
            FieldView::Bound(values) => {
                let infos = &self.tables.decls[frame.owner.index()].fields;
                let at = infos.iter().position(|info| info.name == field)?;
                values.get(at).cloned().flatten()
            }
        }
    }

    /// Execute one handler step through its installed implementation,
    /// splicing its effects into the running list. A missing handler
    /// or an undeclared slot fails at the site, never silently.
    fn run_handler(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &BindFrame,
        name: &str,
        target: ContextTarget,
        view: FieldView<'_>,
        out: &mut Vec<ContextEffect>,
    ) {
        let Some(handler) = self.handlers.get(name) else {
            self.diagnose(cx, NOT_INSTALLED, frame.span, &["handler", name]);
            return;
        };
        let produced = {
            let mut fields = BTreeMap::new();
            for info in &self.tables.decls[frame.owner.index()].fields {
                if let Some(value) = self.field_value(cx, frame, view, info.name) {
                    fields.insert(self.name(info.name).to_owned(), value);
                }
            }
            let mut context = BTreeMap::new();
            for (slot, info) in cx.context().layout().slots().zip(self.tables.slots.iter()) {
                if let Some(held) = cx.context().get(slot) {
                    context.insert(self.name(info.name).to_owned(), held.value.clone());
                }
            }
            let call = HandlerCall {
                handler: name,
                node: self.name(frame.name),
                target,
                fields: &fields,
                context: &context,
            };
            handler(&call)
        };
        let origin = OriginId(cx.node().index);
        let provider = Trace::provided(format!("handler {name}"));
        for effect in produced {
            let resolved = match effect {
                HandlerEffect::Set { slot, value } => self
                    .def
                    .names
                    .get(&slot)
                    .and_then(|sym| self.def.plan.context.slot(sym))
                    .map(|slot| ContextEffect::Set {
                        slot,
                        value: SlotValue::new(value, origin).with_trace(provider.clone()),
                    })
                    .ok_or(("sets context slot", slot)),
                HandlerEffect::Remove { slot } => self
                    .def
                    .names
                    .get(&slot)
                    .and_then(|sym| self.def.plan.context.slot(sym))
                    .map(|slot| ContextEffect::Remove { slot })
                    .ok_or(("removes context slot", slot)),
                HandlerEffect::Clear { namespace } => self
                    .def
                    .names
                    .get(&namespace)
                    .and_then(|sym| self.def.plan.context.namespace(sym))
                    .map(|namespace| ContextEffect::Clear { namespace })
                    .ok_or(("clears context namespace", namespace)),
            };
            match resolved {
                Ok(effect) => out.push(effect),
                Err((effect, target)) => {
                    let values = [name, effect, target.as_str()];
                    self.diagnose(cx, HANDLER_UNDECLARED, frame.span, &values);
                }
            }
        }
    }

    /// Keep the sets that decode as their slot's type and pass its
    /// duplicate policy, reporting the others. Each set meets the held
    /// value as the effects before it in the list leave it.
    fn typed(
        &mut self,
        cx: &mut Cx<'_>,
        effects: Vec<ContextEffect>,
        span: ByteSpan,
    ) -> Vec<ContextEffect> {
        let mut kept = Vec::with_capacity(effects.len());
        // The slots the effects kept so far have written, as they leave
        // them: `None` is unset by a remove or a clear.
        let mut written: BTreeMap<SlotId, Option<SlotValue>> = BTreeMap::new();
        for effect in effects {
            let (slot, value) = match effect {
                ContextEffect::Set { slot, value } => (slot, value),
                ContextEffect::Remove { slot } => {
                    written.insert(slot, None);
                    kept.push(ContextEffect::Remove { slot });
                    continue;
                }
                ContextEffect::Clear { namespace } => {
                    for &member in cx.context().layout().members(namespace) {
                        written.insert(member, None);
                    }
                    kept.push(ContextEffect::Clear { namespace });
                    continue;
                }
                other => {
                    kept.push(other);
                    continue;
                }
            };
            let Some(info) = self.tables.slots.get(slot.index()).cloned() else {
                written.insert(slot, Some(value.clone()));
                kept.push(ContextEffect::Set { slot, value });
                continue;
            };
            let what = format!("context {}", self.name(info.name));
            let Some(decoded) =
                self.check(cx, Some(info.ty.clone()), &what, value.value.clone(), span)
            else {
                continue;
            };
            let held = match written.get(&slot) {
                Some(held) => held.clone(),
                None => cx.context().get(slot).cloned(),
            };
            let incoming = SlotValue::new(decoded, value.origin)
                .with_trace(value.trace)
                .with_layer(value.layer);
            let Some(placed) = self.combine(cx, &info, held.as_ref(), incoming, span) else {
                continue;
            };
            written.insert(slot, Some(placed.clone()));
            kept.push(ContextEffect::Set {
                slot,
                value: placed,
            });
        }
        kept
    }

    /// Place a set under the slot's accumulation or duplicate policy.
    /// `Some` is the slot's value after it; `None` leaves the slot as it
    /// was: rejected and reported, or outranked by a held layer.
    fn combine(
        &mut self,
        cx: &mut Cx<'_>,
        info: &SlotInfo,
        held: Option<&SlotValue>,
        incoming: SlotValue,
        span: ByteSpan,
    ) -> Option<SlotValue> {
        let Some(held) = held else {
            return self.first_value(cx, info, incoming, span);
        };
        match info.accumulate {
            Some(op) => {
                let combined = self.accumulate(cx, info, op, &held.value, incoming.value, span)?;
                let site = self.location(span);
                let trace = Trace::derived(
                    Derivation::Merge,
                    site,
                    vec![held.trace.clone(), incoming.trace],
                );
                Some(SlotValue::new(combined, incoming.origin).with_trace(trace))
            }
            None => match info.duplicates {
                SlotDuplicates::Reject => {
                    self.duplicate_set(cx, info, held, &incoming, span);
                    None
                }
                SlotDuplicates::LastWins => Some(incoming),
                SlotDuplicates::Layered => self.layered(cx, info, held, incoming, span),
            },
        }
    }

    /// A set reaching an unset slot: the value it carries. A layered
    /// slot first checks the set's layer, as every layered set does.
    fn first_value(
        &mut self,
        cx: &mut Cx<'_>,
        info: &SlotInfo,
        incoming: SlotValue,
        span: ByteSpan,
    ) -> Option<SlotValue> {
        if info.duplicates == SlotDuplicates::Layered
            && !self.layer_declared(cx, info, &incoming, span)
        {
            return None;
        }
        Some(incoming)
    }

    /// Whether a set on a layered slot names a layer the slot declares.
    /// The compiler rejects every declarative set that does not; a
    /// handler's set cannot name one, so it fails closed here
    /// (`KDLITH-DLC005`) rather than landing outside the layer order.
    fn layer_declared(
        &mut self,
        cx: &mut Cx<'_>,
        info: &SlotInfo,
        incoming: &SlotValue,
        span: ByteSpan,
    ) -> bool {
        let slot = self.name(info.name);
        let reason = match &incoming.layer {
            Some(layer) if info.layers.contains(layer) => return true,
            Some(layer) => format!("layer {layer} is not declared by layered slot {slot}"),
            None => format!("a set on layered slot {slot} names no layer"),
        };
        self.diagnose(cx, MALFORMED, span, &["set", &reason]);
        false
    }

    /// The layered arm, by declared order, highest first: a higher layer
    /// replaces, a lower one leaves the held value, so order never
    /// matters. Same layer, or a held value without one, rejects.
    fn layered(
        &mut self,
        cx: &mut Cx<'_>,
        info: &SlotInfo,
        held: &SlotValue,
        incoming: SlotValue,
        span: ByteSpan,
    ) -> Option<SlotValue> {
        let rank = |layer: &Option<String>| {
            layer
                .as_ref()
                .and_then(|layer| info.layers.iter().position(|declared| declared == layer))
        };
        if !self.layer_declared(cx, info, &incoming, span) {
            return None;
        }
        match (rank(&incoming.layer), rank(&held.layer)) {
            (Some(ours), Some(theirs)) if ours < theirs => Some(incoming),
            (Some(ours), Some(theirs)) if ours > theirs => None,
            _ => {
                self.duplicate_set(cx, info, held, &incoming, span);
                None
            }
        }
    }

    /// Combine an incoming value with the held one under the slot's
    /// declared accumulation operation.
    fn accumulate(
        &mut self,
        cx: &mut Cx<'_>,
        info: &SlotInfo,
        op: AccumulateOp,
        held: &Value,
        incoming: Value,
        span: ByteSpan,
    ) -> Option<Value> {
        match (op, held.clone(), incoming) {
            (AccumulateOp::Append, Value::List(mut held), Value::List(mut items)) => {
                held.append(&mut items);
                Some(Value::List(held))
            }
            (AccumulateOp::Merge, Value::Map(mut held), Value::Map(entries)) => {
                let label = format!("context {}", self.name(info.name));
                for entry in entries {
                    match held.iter_mut().find(|held| held.key == entry.key) {
                        // The declared treatment decides a key the held
                        // map already carries: reject keeps the
                        // duplicate-key code; last-wins replaces the
                        // value in the key's first position.
                        Some(existing) => match info.merge_duplicates {
                            MergeDuplicates::LastWins => existing.value = entry.value,
                            _ => {
                                self.diagnose(cx, DUPLICATE_KEY, span, &[&entry.key, &label]);
                                return None;
                            }
                        },
                        None => held.push(entry),
                    }
                }
                Some(Value::Map(held))
            }
            (_, held, _) => {
                let what = format!("context {}", self.name(info.name));
                self.mismatched(cx, &info.ty, &what, &held, span);
                None
            }
        }
    }

    /// The reject arm (`KDLITH-CTX006`): slot, policy, and both
    /// contributions by origin. Values stay out, since a slot may carry
    /// a sensitive field's value.
    fn duplicate_set(
        &mut self,
        cx: &mut Cx<'_>,
        info: &SlotInfo,
        held: &SlotValue,
        incoming: &SlotValue,
        span: ByteSpan,
    ) {
        let slot = self.name(info.name);
        let held = self.contribution(held);
        let incoming = self.contribution(incoming);
        let values = [slot, info.duplicates.name(), &incoming, &held];
        self.diagnose(cx, SLOT_DUPLICATE, span, &values);
    }

    /// One contribution to a slot, as a diagnostic names it: its layer,
    /// when it has one, and where its value was set.
    fn contribution(&self, value: &SlotValue) -> String {
        let at = if value.origin == OriginId::SOURCE_METADATA {
            "source metadata".to_owned()
        } else {
            match value.trace.origins().first() {
                Some(origin) => match origin.location() {
                    Some(location) => {
                        let start = location.span.start;
                        let place = format!("line {} column {}", start.line, start.column);
                        if location.source == self.source {
                            place
                        } else {
                            format!("source {} {place}", location.source.0)
                        }
                    }
                    None => match origin {
                        Origin::Provided { provider } => provider.clone(),
                        _ => "an untraced origin".to_owned(),
                    },
                },
                None => "an untraced origin".to_owned(),
            }
        };
        match &value.layer {
            Some(layer) => format!("layer {layer} at {at}"),
            None => at,
        }
    }

    /// Whether a node declaration of a definition bound as data of the
    /// dialect binds two fields to the same syntax, reported with the
    /// code the definition compiler uses for the same construct.
    fn overlapping(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &BindFrame,
        values: &[Option<Value>],
    ) -> bool {
        if self.def.kind != DefinitionKind::Dialect
            || self.name(frame.name) != overlap::NODE_CONSTRUCT
        {
            return false;
        }
        let infos = &self.tables.decls[frame.owner.index()].fields;
        let fields: Vec<(&str, &Value)> = infos
            .iter()
            .zip(values)
            .filter_map(|(info, value)| Some((self.def.names.resolve(info.name), value.as_ref()?)))
            .collect();
        let found = overlap::in_declaration(&fields);
        for overlap in &found {
            self.diagnose(cx, CONFLICTING_BINDING, frame.span, &overlap.values());
        }
        !found.is_empty()
    }

    /// Whether a schema root of a definition read as data of the dialect
    /// states its `version`, with the code the compiler reports for the
    /// same construct; dialect roots declare theirs required already.
    fn root_without_version(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &BindFrame,
        values: &[Option<Value>],
        root: bool,
    ) -> bool {
        if !root || self.def.kind != DefinitionKind::Dialect || self.name(frame.name) != SCHEMA_ROOT
        {
            return false;
        }
        let infos = &self.tables.decls[frame.owner.index()].fields;
        let Some((_, version)) = infos
            .iter()
            .zip(values)
            .find(|(info, _)| self.def.names.resolve(info.name) == "version")
        else {
            return false;
        };
        if version.is_some() {
            return false;
        }
        self.diagnose(cx, MISSING_FIELD, frame.span, &[SCHEMA_ROOT, "version"]);
        true
    }

    /// Emit the processing item a declared node's role calls for. The
    /// type a case dispatch selected overrides the declaration's own,
    /// and every sensitive field's value passes the output policy.
    fn emit(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &BindFrame,
        role: Role,
        values: Vec<Option<Value>>,
        typed_as: Option<Sym>,
    ) {
        let owner = frame.owner;
        let node = cx.node();
        let origin = OriginId(node.index);
        let info = &self.tables.decls[owner.index()];
        let validation = if info.provisional {
            ValidationState::Provisional
        } else {
            ValidationState::Valid
        };
        let type_name = match typed_as {
            Some(typed) => self.name(typed).to_owned(),
            None => info.type_name.clone(),
        };
        let text_of = |at: Option<usize>| match at.and_then(|at| values.get(at)?.as_ref()) {
            Some(Value::String(text)) => Some(text.clone()),
            _ => None,
        };
        let identity = match role {
            Role::Declaration => {
                let Some(identity) = self.compose_identity(cx, frame, &values, &type_name) else {
                    return;
                };
                Some(identity)
            }
            _ => None,
        };
        let info = &self.tables.decls[owner.index()];
        let path = text_of(info.path);
        let repeated = text_of(info.repeated);
        let alias = text_of(info.alias);
        let layer = text_of(info.layer);
        // A scope field typed as a declared reference hoists its written
        // spelling, matching the untyped string form.
        let scope = match info.scope.and_then(|at| values.get(at)?.as_ref()) {
            Some(Value::String(text)) => Some(text.clone()),
            Some(Value::Reference(reference)) => Some(reference.written.clone()),
            _ => None,
        };
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
        // The scope reference is the contribution's metadata, never its
        // target.
        let reference = info
            .references
            .iter()
            .filter(|at| Some(**at) != info.scope)
            .find_map(|at| match values.get(*at) {
                Some(Some(Value::Reference(reference))) => Some(reference.clone()),
                _ => None,
            });
        let fields: Vec<Field> = info
            .fields
            .iter()
            .zip(values)
            .zip(frame.traces.iter())
            .filter_map(|((field, value), trace)| {
                let value = value?;
                let (value, redacted) = match field.sensitivity {
                    Some(sensitivity) => {
                        let policy = self.policy_for(sensitivity);
                        (policy.apply(value)?, policy == OutputPolicy::Redact)
                    }
                    None => (value, false),
                };
                Some(
                    Field::new(self.name(field.name), value, origin)
                        .with_provenance(trace.clone(), redacted),
                )
            })
            .collect();
        let node_site = self.location(frame.span);
        let node_name = self.name(frame.name);
        let item = match role {
            Role::Record => BoundItem::Record(
                BoundRecord::new(
                    node, node_name, type_name, fields, origin, validation, frame.span,
                )
                .with_site(node_site),
            ),
            Role::Declaration => {
                let identity = identity.expect("a declaration composes its identity above");
                BoundItem::Declaration(
                    BoundDeclaration::from_record(
                        BoundRecord::new(
                            node, node_name, type_name, fields, origin, validation, frame.span,
                        ),
                        identity,
                    )
                    .with_site(node_site),
                )
            }
            Role::Contribution => {
                let site = self.location(frame.span);
                let target = reference.unwrap_or_else(|| {
                    let written = self.name(frame.name);
                    Reference::new(type_name, QualifiedName(Vec::new()), written, site)
                });
                let value = Value::Map(
                    fields
                        .iter()
                        .map(|f| Entry::new(f.name.clone(), f.value.clone()))
                        .collect(),
                );
                let mut contribution =
                    BoundContribution::new(node, target, value, origin, validation, frame.span)
                        .with_provenance(node_site, fields);
                // The preserved record carries its layer and application
                // scope as metadata for the registered resolver.
                contribution.layer = layer;
                contribution.scope = scope;
                BoundItem::Contribution(Box::new(contribution))
            }
            Role::SourceImport => {
                let site = self.location(frame.span);
                let expression = path.unwrap_or_default();
                if !self.literal_import(cx, frame.span, &expression) {
                    return;
                }
                let policy = match repeated.as_deref().map(Repeated::from_name) {
                    None => Repeated::default(),
                    Some(Some(policy)) => policy,
                    Some(None) => {
                        let found = format!("string {:?}", repeated.unwrap_or_default());
                        let values = ["import.repeated", "RepeatedImport", &found];
                        self.diagnose(cx, UNKNOWN_MEMBER, frame.span, &values);
                        return;
                    }
                };
                if let Some(alias) = &alias
                    && self.aliases.iter().any(|(held, _)| held == alias)
                {
                    // The request preserves `as=`; an alias is unique in
                    // its document, on every reading path.
                    let values = [alias.as_str(), "the alias of import"];
                    self.diagnose(cx, DUPLICATE_KEY, frame.span, &values);
                    return;
                }
                if let Some(alias) = &alias {
                    self.aliases.push((alias.clone(), frame.span));
                }
                let mut request = ImportRequest::new(node, expression, site, origin);
                request.alias = alias;
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

    /// A declaration's identity. An identity field holding a value that
    /// is not a string, or a composed field left unbound, is reported
    /// and yields no identity, since a shortened name could collide
    /// with another declaration's.
    fn compose_identity(
        &mut self,
        cx: &mut Cx<'_>,
        frame: &BindFrame,
        values: &[Option<Value>],
        type_name: &str,
    ) -> Option<Identity> {
        let owner = frame.owner;
        let text = |at: Option<usize>| at.and_then(|at| values.get(at)?.as_ref());
        match &self.tables.decls[owner.index()].identity {
            IdentityBind::Fallback(at) => {
                let written = at.and_then(|at| Some((at, values.get(at)?.as_ref()?)));
                let name = match written {
                    Some((_, Value::String(segment))) => vec![segment.clone()],
                    Some((at, other)) => {
                        let field = self.tables.decls[owner.index()]
                            .fields
                            .get(at)
                            .map_or("", |field| self.name(field.name));
                        let what = format!("{}.{field}", self.name(frame.name));
                        let found = decode::found(other);
                        self.diagnose(cx, VALUE_TYPE, frame.span, &[&what, "String", &found]);
                        return None;
                    }
                    // No argument-0 field, or none written: one of a kind
                    // in its scope.
                    None => Vec::new(),
                };
                Some(Identity::new(type_name.to_owned(), QualifiedName(name)))
            }
            IdentityBind::Composed { domain, fields } => {
                let (domain, fields) = (domain.clone(), fields.clone());
                let mut name = Vec::with_capacity(fields.len());
                let mut complete = true;
                for (field, at) in fields {
                    match text(at) {
                        Some(Value::String(segment)) => name.push(segment.clone()),
                        Some(other) => {
                            complete = false;
                            let what = format!("{}.{}", self.name(frame.name), self.name(field));
                            let found = decode::found(other);
                            self.diagnose(cx, VALUE_TYPE, frame.span, &[&what, "String", &found]);
                        }
                        None => {
                            complete = false;
                            let node = self.name(frame.name).to_owned();
                            let field = self.name(field).to_owned();
                            self.diagnose(cx, MISSING_FIELD, frame.span, &[&node, &field]);
                        }
                    }
                }
                complete.then(|| Identity::new(domain, QualifiedName(name)))
            }
        }
    }

    /// Whether a dialect document's import path is literal, reported
    /// as the compiler reports it when empty or in discovery syntax.
    /// A schema document's paths are left to its own compile.
    fn literal_import(&mut self, cx: &mut Cx<'_>, span: ByteSpan, path: &str) -> bool {
        if self.root_name.as_deref() != Some("dialect") {
            return true;
        }
        if path.is_empty() {
            self.diagnose(cx, MALFORMED, span, &["import", "empty path"]);
            return false;
        }
        if !crate::discovery::is_expression(path) {
            return true;
        }
        let construct = format!("import {path:?}");
        self.diagnose(
            cx,
            MALFORMED,
            span,
            &[&construct, "import paths are literal"],
        );
        false
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
        if key.is_some() && frame.key.is_none() && !frame.key_rejected {
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
        if self.overlapping(cx, frame, &values) {
            return;
        }
        let root = parent.is_none() && self.window.is_none();
        if self.root_without_version(cx, frame, &values, root) {
            return;
        }
        // A case dispatch selects the type a node binds as on every
        // reading path: an item's type, or the union member a part
        // spells. No match and no otherwise rejects the node outright.
        let selection = self.select_case(cx, frame, &values);
        if matches!(selection, CaseSelection::Unmatched) {
            return;
        }
        let (typed_as, selected_on) = match selection {
            CaseSelection::Selected { typed, property } => (typed, property),
            CaseSelection::None => (None, None),
            CaseSelection::Unmatched => unreachable!("an unmatched case returned above"),
        };
        let errors = self.errors;
        self.selected_required(cx, frame, &values, typed_as);
        self.constraints(cx, frame, &values, typed_as);
        if self.errors > errors {
            let bind = parent
                .as_deref()
                .and_then(|p| p.table(&self.tables))
                .and_then(|t| t.child(frame.name));
            if let (Some(parent), Some(bind)) = (parent, bind) {
                parent.rejected.push(bind.field);
            }
            return;
        }
        let role = def.bindings.get(owner).and_then(|decl| decl.role);
        if let Some(role) = role {
            let pending = self.dependent_payload(cx, frame, &values);
            if let Some(plan) = def.plan.node(owner) {
                let effects = self.effects(
                    cx,
                    frame,
                    &plan.following_effects,
                    ContextTarget::FollowingSiblings,
                    FieldView::Bound(&values),
                );
                for effect in self.typed(cx, effects, frame.span) {
                    following.push(effect);
                }
            }
            let errors = self.errors;
            self.emit(cx, frame, role, values, typed_as);
            // The pending binding follows the item holding its reference;
            // a node rejected while emitting leaves nothing to defer.
            if let Some(pending) = pending.filter(|_| self.errors == errors) {
                cx.emit(BoundItem::PendingBinding(pending));
            }
            return;
        }
        let Some(parent) = parent else {
            return;
        };
        let Some(bind) = parent.table(&self.tables).and_then(|t| t.child(frame.name)) else {
            if !self.expansion_point(cx, parent, frame, &values) {
                self.reject_unconsumed(cx, parent, frame);
            }
            return;
        };
        let infos = &self.tables.decls[owner.index()].fields;
        // A part's case selector is dispatch input, not payload: the
        // variant it lands under says it. An item keeps the property
        // among its fields, per its selected type's contract.
        let entries = infos
            .iter()
            .zip(values)
            .filter(|(info, _)| Some(info.name) != selected_on)
            .filter_map(|(info, value)| Some(Entry::new(self.name(info.name), value?)))
            .collect();
        let mut value = Value::Map(entries);
        if let Some(variants) = self.union_of(parent.owner, bind) {
            // A union value is spelled by the node naming its variant, or
            // by the case dispatch that selected a variant's type.
            let spelled = variants.iter().find_map(|variant| {
                if variant.discriminator == frame.name {
                    return Some(variant.discriminator);
                }
                match typed_as {
                    Some(selected) => {
                        matches!(variant.ty, TypeExpr::Named(name) if name == selected)
                            .then_some(variant.discriminator)
                    }
                    None => None,
                }
            });
            let Some(spelled) = spelled else {
                let what = self.field_label(parent.owner, bind.field);
                let ty = self.item_type(parent.owner, bind).cloned();
                let expected =
                    ty.map_or_else(String::new, |ty| summary::expr(&self.def.names, &ty));
                let found = format!("node {}", self.name(frame.name));
                self.diagnose(cx, VALUE_TYPE, frame.span, &[&what, &expected, &found]);
                parent.rejected.push(bind.field);
                return;
            };
            value = Value::Map(vec![Entry::new(self.name(spelled), value)]);
        }
        if bind.mode == Mode::Keyed {
            let Some((key, span)) = frame.key.take() else {
                return;
            };
            let ty = self.key_type(parent.owner, bind).cloned();
            let what = format!("the key of {}", self.name(frame.name));
            let entry = match self.check(cx, ty.clone(), &what, Value::String(key.clone()), span) {
                None => return,
                Some(Value::String(key)) => Entry::new(key, value),
                // The link stage resolves a referenced key like any
                // reference a field holds.
                Some(Value::Reference(reference)) => Entry::referenced(reference, value),
                Some(other) => {
                    let field = self.field_label(parent.owner, bind.field);
                    let expected =
                        ty.map_or_else(String::new, |ty| summary::expr(&self.def.names, &ty));
                    let found = decode::found(&other);
                    let values = [key.as_str(), field.as_str(), &expected, &found];
                    self.diagnose(cx, UNSUPPORTED_KEY, span, &values);
                    return;
                }
            };
            value = Value::Map(vec![entry]);
        }
        let span = frame.span;
        self.route(cx, parent, bind, value, span);
    }

    /// Emit an `include` child's expansion request at its position among
    /// the owner's like members; whether the node was consumed as one.
    fn expansion_point(
        &mut self,
        cx: &mut Cx<'_>,
        parent: &BindFrame,
        frame: &BindFrame,
        values: &[Option<Value>],
    ) -> bool {
        let info = &self.tables.decls[frame.owner.index()];
        if self.def.kind != DefinitionKind::Dialect || info.type_name != INCLUDE_TYPE {
            return false;
        }
        // The keyed members bound before the include: its splice point
        // among the owner's members. Every declaring container admits
        // exactly one keyed member group.
        let members = parent
            .table(&self.tables)
            .map(|table| {
                table
                    .children
                    .iter()
                    .filter(|(_, bind)| bind.mode == Mode::Keyed)
                    .map(|(_, bind)| {
                        parent
                            .fields
                            .get(bind.field)
                            .and_then(|held| held.as_ref())
                            .map_or(0, |held| match &held.value {
                                Value::Map(entries) => entries.len(),
                                _ => 0,
                            })
                    })
                    .sum::<usize>()
            })
            .unwrap_or(0);
        let infos = &self.tables.decls[frame.owner.index()].fields;
        let target = infos
            .iter()
            .zip(values)
            .find(|(info, _)| self.name(info.name) == INCLUDE_TARGET)
            .and_then(|(_, value)| value.as_ref())
            .and_then(|value| match value {
                Value::String(text) => Some(text.clone()),
                _ => None,
            });
        // An include with no usable target or no named owner is rejected
        // loudly as unconsumed, never dropped.
        let Some(target) = target else {
            return false;
        };
        let Some(owner) = self.held_value(parent, INCLUDE_NAME) else {
            return false;
        };
        let Some(Value::String(owner)) = owner else {
            return false;
        };
        let site = self.location(frame.span);
        let domain = self.tables.decls[parent.owner.index()].type_name.clone();
        let reference = Reference::new(domain, QualifiedName(vec![target.clone()]), target, site);
        let node = cx.node();
        let request = ExpansionRequest::new(node, reference, site, OriginId(node.index))
            .among(owner, members);
        cx.emit(BoundItem::Expansion(request));
        true
    }

    /// The value `frame` holds so far for its owner's field `field`:
    /// `None` when the owner declares no such field.
    fn held_value(&self, frame: &BindFrame, field: &str) -> Option<Option<Value>> {
        let infos = &self.tables.decls[frame.owner.index()].fields;
        let at = infos
            .iter()
            .position(|info| self.name(info.name) == field)?;
        Some(
            frame
                .fields
                .get(at)
                .and_then(|held| held.as_ref())
                .map(|held| held.value.clone()),
        )
    }

    /// A child rule admitted the node, but neither a role nor a field of
    /// its owner consumes it: a node binding fields is rejected rather
    /// than dropped with its values.
    fn reject_unconsumed(&mut self, cx: &mut Cx<'_>, parent: &BindFrame, frame: &BindFrame) {
        // A field-less marker carries nothing; its structural rules
        // answer for it.
        if self.tables.decls[frame.owner.index()].fields.is_empty() {
            return;
        }
        let node = self.name(frame.name);
        let place = format!(
            "{}, where no role or field binding consumes its value",
            self.name(parent.name)
        );
        self.diagnose(cx, UNKNOWN_NODE, frame.span, &[node, &place]);
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
        // Only a node that binds becomes the sibling the next one is
        // compared against.
        let previous = match parent.as_deref_mut() {
            Some(frame) => frame.last_child.take(),
            None => self.root_last.take(),
        };
        let scope = match parent.as_ref() {
            Some(frame) => frame.scope,
            None => Some(self.window.unwrap_or(def.bindings.root_scope())),
        };
        if parent.is_none() && self.root_name.is_none() {
            self.root_name = Some(node.name.to_string());
        }
        if parent.is_none() && !self.profile_reported {
            // A schema that declares profiles must have exactly one
            // selected from invocation context — never from document
            // content. A name that selected nothing reports itself; no
            // selection at all reports the missing one.
            self.profile_reported = true;
            let named = self.profile_named.as_deref().unwrap_or("(none)");
            let shown = format!("profile {named}");
            let values = [shown.as_str(), &def.name];
            self.diagnose(cx, PROFILE_NOT_SELECTED, span, &values);
            return Disposition::Skip;
        }
        let (Some(sym), Some(scope_id)) = (def.names.get(&node.name), scope) else {
            self.unknown(cx, span, &node.name, owner_label, previous);
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
            None if self.window.is_some() => Ok(()),
            None => scope.rules.admit(&mut self.root, sym),
        };
        if let Err(violation) = admitted {
            if parent.is_none() {
                self.order_broken |= matches!(
                    violation,
                    Violation::FirstNotAllowed { .. } | Violation::NotFollowing { .. }
                );
            }
            self.violation(cx, span, owner_label, &violation, previous);
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
                self.unknown(cx, span, &node.name, owner_label, previous);
                return Disposition::Skip;
            }
        };
        // Dispatch consults the active profile as part of position: a
        // declaration no profile on the active chain owns, or a root the
        // profile's effective roots do not allow, is rejected as unbound.
        if let Kind::Declared { decl, .. } = kind {
            let declared = def.bindings.get(decl);
            if declared.is_some_and(|d| d.profile.is_some_and(|p| !self.profile_chain.contains(&p)))
                || (parent.is_none()
                    && self.window.is_none()
                    && self.profile_roots.as_ref().is_some_and(|roots| {
                        declared
                            .and_then(|d| d.ty)
                            .is_none_or(|ty| !roots.contains(&ty))
                    }))
            {
                self.unknown(cx, span, &node.name, owner_label, previous);
                return Disposition::Skip;
            }
        }
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
        let bound = Some(Sibling {
            name: sym,
            span,
            scope: inner,
        });
        match parent {
            Some(frame) => frame.last_child = bound,
            None => self.root_last = bound,
        }
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
                        frame.key_rejected = true;
                    }
                }
                return;
            }
            match frame.table(&self.tables).and_then(|table| table.arg(at)) {
                Some(bind) => self.take(cx, frame, bind, entry.value, span),
                None => {
                    // A node with a dependent value binding keeps its
                    // remaining arguments as its payload.
                    if frame.declared()
                        && frame.payload.len() < MAX_PAYLOAD
                        && self.tables.decls[frame.owner.index()].dependent.is_some()
                    {
                        frame.payload.push((entry.value, span));
                        return;
                    }
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
        let effects = self.effects(
            cx,
            frame,
            &plan.children_effects,
            ContextTarget::Children,
            FieldView::Header,
        );
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
        let subtree = frame
            .state
            .take()
            .map(|state| {
                let compiled = frame
                    .scope
                    .and_then(|scope| def.bindings.scope(scope))
                    .map(|scope| scope.rules.complete(&state));
                if let Some(violations) = compiled {
                    let owner = self.name(frame.name);
                    for violation in violations {
                        self.violation(cx, frame.span, owner, &violation, None);
                    }
                }
                ScopeRules::subtree(&state).clone()
            })
            .unwrap_or_default();
        // Occurrences anywhere below count toward every enclosing scope's
        // descendant rules; an import is a separate invocation with its
        // own scopes, so the counts never cross that boundary.
        match parent.as_deref_mut() {
            Some(parent) => {
                if let Some(state) = parent.state.as_mut() {
                    ScopeRules::bubble(state, &subtree);
                }
            }
            None => ScopeRules::bubble(&mut self.root, &subtree),
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
        self.left = Some(cx.context().snapshot_values());
        if self.window.is_some() {
            return;
        }
        let def = self.def;
        if let Some(scope) = def.bindings.scope(def.bindings.root_scope()) {
            let span = cx.span();
            for violation in scope.rules.complete(&self.root) {
                if self.order_broken && violation == Violation::FirstMissing {
                    continue;
                }
                self.violation(cx, span, DOCUMENT_ROOT, &violation, None);
            }
        }
    }
}

/// The scope the `node`th node of `text`, in document order, is
/// dispatched in: its parent's children scope, or the document's root
/// scope. `None` when the text holds fewer nodes, or the node sits where
/// nothing binds.
pub(crate) fn dispatch_scope(def: &Definition, text: &str, node: u32) -> Option<ScopeId> {
    let mut parser = Parser::new(text);
    let mut open: Vec<Option<ScopeId>> = Vec::new();
    let mut seen: u32 = 0;
    while let Ok(Some(event)) = parser.next_event() {
        match event.item {
            Event::Node { name, .. } => {
                let parent = match open.last() {
                    Some(scope) => *scope,
                    None => Some(def.bindings.root_scope()),
                };
                if seen == node {
                    return parent;
                }
                seen = seen.saturating_add(1);
                let inner = parent.zip(def.names.get(&name)).and_then(|(scope, sym)| {
                    match def.bindings.child(scope, sym)? {
                        ChildPlan::Declared { decl, .. } => def.bindings.decl_scope(decl),
                        ChildPlan::Nested(inner) => Some(inner),
                        ChildPlan::Scalar => None,
                    }
                });
                open.push(inner);
            }
            Event::End => {
                open.pop();
            }
            _ => {}
        }
    }
    None
}

/// The admission an active selection composes: every active profile
/// with every profile it extends, and the union of their effective
/// roots — `None`, every declared root, when any active profile admits
/// every root or none is active.
fn active_profiles(def: &Definition, members: &[Sym]) -> (Vec<Sym>, Option<Vec<Sym>>) {
    let mut chain: Vec<Sym> = Vec::new();
    let mut roots: Option<Vec<Sym>> = (!members.is_empty()).then(Vec::new);
    for member in members {
        for profile in def.profile_chain(*member) {
            if !chain.contains(&profile.name) {
                chain.push(profile.name);
            }
        }
        roots = match (roots, def.effective_roots(*member)) {
            (Some(mut union), Some(own)) => {
                for root in own {
                    if !union.contains(&root) {
                        union.push(root);
                    }
                }
                Some(union)
            }
            _ => None,
        };
    }
    (chain, roots)
}

/// What asking the value resolver for one absent field came to.
enum Resolution {
    /// No resolver is registered, or the field's fallback is not
    /// computed: the field stays absent.
    NotAsked,
    /// The resolver supplied a value that decoded as the field's type,
    /// with the origin it records.
    Resolved(Value, Trace),
    /// The resolver declined, or its value failed the field's type; the
    /// failure is reported already.
    Failed,
}

/// The knobs a binding session applies beyond its definition: the
/// profile the invocation named, one explicit output policy, and the
/// installed context handlers.
struct Knobs<'a> {
    profile: Option<&'a str>,
    /// A selection the reading already resolved; when absent, the
    /// session selects for its own path as an entry document.
    selection: Option<ProfileSelection>,
    output_policy: Option<OutputPolicy>,
    handlers: Handlers,
}

impl Knobs<'_> {
    fn plain() -> Self {
        Self {
            profile: None,
            selection: None,
            output_policy: None,
            handlers: Handlers::new(),
        }
    }
}

/// A binding session over one data document: parse events, structural
/// match, typed binding, and a `BoundItem` stream. A parse failure or a
/// session limit ends the stream with its diagnostic as the last item.
pub struct BindingSession<'d, 's> {
    session: Session<ParseEvents<'s>, Binder<'d, 's>>,
    /// Diagnostics the source-context application produced before the
    /// stream began, yielded ahead of every bound item.
    pending: Vec<BoundItem>,
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
        Self::with_context_and_policy(def, source, path, text, context, None)
    }

    /// Bind starting from `context` and applying one explicit output
    /// policy to every sensitive field.
    pub fn with_context_and_policy(
        def: &'d Definition,
        source: SourceId,
        path: &str,
        text: &'s str,
        context: Context,
        output_policy: Option<OutputPolicy>,
    ) -> Self {
        Self::with_all(
            def,
            source,
            path,
            text,
            context,
            Knobs {
                output_policy,
                ..Knobs::plain()
            },
        )
    }

    /// Bind starting from `context` and applying the profile the
    /// invocation named. `None` against a definition that declares
    /// profiles fails closed (`KDLITH-SCH011`).
    pub fn with_context_and_profile(
        def: &'d Definition,
        source: SourceId,
        path: &str,
        text: &'s str,
        context: Context,
        profile: Option<&str>,
    ) -> Self {
        Self::with_all(
            def,
            source,
            path,
            text,
            context,
            Knobs {
                profile,
                ..Knobs::plain()
            },
        )
    }

    /// Bind applying one explicit output policy to every sensitive
    /// field; `None` redacts a secret and emits a reference.
    pub fn with_policy(
        def: &'d Definition,
        source: SourceId,
        path: &str,
        text: &'s str,
        output_policy: OutputPolicy,
    ) -> Self {
        Self::with_context_and_policy(
            def,
            source,
            path,
            text,
            Context::new(def.plan.context.layout.clone()),
            Some(output_policy),
        )
    }

    /// Bind starting from `context` under a profile selection the
    /// reading resolved once for the document's entry, with the context
    /// handlers the reading processor installed: a handler effect
    /// executes at its declared timing through `handlers`, and one the
    /// registry lacks fails at its site.
    pub fn with_handlers(
        def: &'d Definition,
        source: SourceId,
        path: &str,
        text: &'s str,
        context: Context,
        selection: ProfileSelection,
        handlers: Handlers,
    ) -> Self {
        Self::with_all(
            def,
            source,
            path,
            text,
            context,
            Knobs {
                selection: Some(selection),
                handlers,
                ..Knobs::plain()
            },
        )
    }

    /// Bind starting from `context` under a profile selection the
    /// reading resolved once for the document's entry.
    pub fn with_selection(
        def: &'d Definition,
        source: SourceId,
        path: &str,
        text: &'s str,
        context: Context,
        selection: ProfileSelection,
    ) -> Self {
        Self::with_all(
            def,
            source,
            path,
            text,
            context,
            Knobs {
                selection: Some(selection),
                ..Knobs::plain()
            },
        )
    }

    /// Every binding-session knob at once.
    fn with_all(
        def: &'d Definition,
        source: SourceId,
        path: &str,
        text: &'s str,
        mut context: Context,
        knobs: Knobs<'_>,
    ) -> Self {
        let events = ParseEvents::new(Parser::new(text), text, path);
        let selection = knobs
            .selection
            .unwrap_or_else(|| def.select_profile(knobs.profile, &[], path));
        let mut binder =
            Binder::with_selection(def, source, path, text, &selection, knobs.output_policy);
        binder.handlers = knobs.handlers;
        // Source-context values apply before the document's first
        // semantic node — before the stream begins — under their declared
        // mode, with failures surfacing as the stream's first items.
        let (_, records) = crate::source_context::apply(def, &mut context, path);
        let pending = records
            .into_iter()
            .map(|record| BoundItem::Diagnostic(Box::new(record)))
            .collect();
        Self {
            session: Session::with_context(source, events, binder, context),
            pending,
            ended: false,
            stopped: false,
        }
    }

    /// Bind only the `node`th node of `text` in document order, with its
    /// subtree, starting from a restored `context` and dispatched in
    /// `scope`, its parent's scope. Source-context values are not
    /// applied again: the restored context already holds them. The node
    /// keeps the source id and node index it had in its whole document.
    pub(crate) fn restored(
        def: &'d Definition,
        (source, path, text): (SourceId, &str, &'s str),
        context: Context,
        selection: &ProfileSelection,
        handlers: Handlers,
        (node, scope): (u32, ScopeId),
    ) -> Self {
        let events = ParseEvents::new(Parser::new(text), text, path).windowed(node);
        let mut binder = Binder::with_selection(def, source, path, text, selection, None);
        binder.handlers = handlers;
        binder.window = Some(scope);
        Self {
            session: Session::with_context(source, events, binder, context).starting_at(node),
            pending: Vec::new(),
            ended: false,
            stopped: false,
        }
    }

    /// The session applying one explicit output policy to every
    /// sensitive field; `None` keeps each sensitivity's fail-closed
    /// default. Set before the first item is taken.
    #[must_use]
    pub fn under_policy(mut self, output_policy: Option<OutputPolicy>) -> Self {
        self.session.processor_mut().output_policy = output_policy;
        self
    }

    /// The session resolving absent computed-default fields through the
    /// value resolver the consuming program registered; `None` leaves
    /// them absent. Set before the first item is taken.
    #[must_use]
    pub fn with_resolver(mut self, resolver: Option<SharedResolver>) -> Self {
        self.session.processor_mut().resolver = resolver;
        self
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
        if let Some(item) = self.pending.first().cloned() {
            self.pending.remove(0);
            return Ok(Some(item));
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

    /// The name of the document's first root node, once it has opened.
    pub fn root_name(&self) -> Option<&str> {
        self.session.processor().root_name()
    }

    /// Whether binding put any field to the registered value resolver,
    /// whether or not it resolved: such a result depends on values
    /// outside the document, its context, and its schema.
    pub fn consulted_resolver(&self) -> bool {
        self.session.processor().consulted
    }

    /// The fingerprint of the non-secret values the resolver supplied,
    /// in resolution order, or `None` when it supplied a value for a
    /// `secret` field: a result carrying a resolved secret has no
    /// cacheable identity.
    pub fn resolved_fingerprint(&self) -> Option<u64> {
        let binder = self.session.processor();
        (!binder.resolved_secret).then(|| binder.resolved.fingerprint())
    }

    /// Whether the document failed: an error diagnostic was emitted, or
    /// the stream stopped early.
    pub fn failed(&self) -> bool {
        self.stopped || !self.pending.is_empty() || self.session.processor().errors() > 0
    }

    /// The context the document's root level left once its stream ended;
    /// for a stream that stopped early, the context as it stopped.
    pub(crate) fn left_context(&self) -> Vec<Option<SlotValue>> {
        self.session
            .processor()
            .left
            .clone()
            .unwrap_or_else(|| self.context().snapshot_values())
    }
}
