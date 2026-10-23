//! Building compiler state from a reader's input: one construct per
//! declaration, already validated by binding, so only the rules the
//! dialect's declarations cannot state are checked here.

use kdlith_model::{FieldMerge, Value};
use kdlith_runtime::Repeated;

use crate::bindings::{
    BindingCodec, CaseRule, ChildRule, Collision, Condition, ContextPolicy, DependentBinding,
    DescendantRule, DuplicateKeys, Effect, EffectGroup, EffectTarget, FieldBinding, IdentityDecl,
    NodeAnnotation, NodeDecl, Placement, Role, SequenceDecl, SequenceStep, SetFrom, Source,
    WhenBranch,
};
use crate::compile::Cx;
use crate::compile::conditions;
use crate::compile::expansions::Pending;
use crate::compile::input::{
    AnnotationInput, ChildRuleInput, ConditionInput, ConstraintInput, Construct, DefaultInput,
    DefinitionInput, DependentInput, DiscoveryInput, DocumentRuleInput, EffectGroupInput,
    EffectInput, FieldBindingInput, FieldInput, ImportInput, Located, MatchRuleInput, MemberInput,
    NodeInput, ProfileInput, ProviderInput, ReferenceInput, ResolutionInput, SequenceInput,
    SlotInput, SourceContextInput, SourceInput, TypeInput,
};
use crate::compile::types::{declare, field_name, format_rule, validator_use};
use crate::decode::found;
use crate::definition::{
    AccumulateOp, AnchorDecl, AnnotationDecl, DiscoveryDecl, DocumentRule, DuplicateTreatment,
    ImportDecl, MatchKind, MatchRule, MergeDuplicates, MissingRequirement, Patterns, ProfileDecl,
    ProviderDecl, Qualification, QualifierDecl, ReferenceDecl, ResolutionPolicyDecl,
    ResolutionSelection, SlotDecl, SlotDuplicates, SourceContextDecl, SourceContextMode,
    SourceFieldDecl,
};
use crate::diag::{
    EMPTY_WHEN_COMPOSITION, FALLBACK_LESS_WHEN_RULE, INVALID_BINDING, NOT_INSTALLED,
    OVERLAPPING_WHEN_BRANCHES, SECOND_DEFAULT, Site, duplicate, duplicate_key,
};
use crate::discovery::{self, ExpansionError};
use crate::names::Sym;
use crate::syntax::parse_u64;
use crate::types::{
    CaptureDef, CodecDecl, Constraint, DefaultSpec, EnumMember, EnumType, FieldDef, MapType,
    ObjectType, Sensitivity, StringType, TypeExpr, TypeKind, UnionType, Variant,
};

/// Read every construct of every document: the grammar-affecting
/// declarations first (profiles, discovery grammars, reference policies),
/// so import expressions validate against the full grammar set, then
/// everything else in stream order. Each construct is sited in the
/// document that holds it.
pub(crate) fn all(cx: &mut Cx<'_>, input: &DefinitionInput) {
    let stream = stream(input);
    for (site, located) in &stream {
        let site = *site;
        match &located.item {
            Construct::Profile(input) => profile(cx, site, input),
            Construct::Discovery(input) => discovery_decl(cx, site, input),
            Construct::Reference(input) => reference(cx, site, input),
            _ => continue,
        }
    }
    for (site, located) in &stream {
        let site = *site;
        match &located.item {
            Construct::Type(ty) => type_decl(cx, site, ty),
            Construct::Node(input) => node(cx, site, input),
            Construct::Slot(input) => slot(cx, site, input),
            Construct::Rule(input) => rule(cx, site, input),
            Construct::Import(input) => import(cx, site, input),
            Construct::Include { owner, target, at } => include(cx, site, owner, target, *at),
            Construct::Resolution(input) => resolution(cx, site, input),
            Construct::Annotation(input) => annotation_decl(cx, site, input),
            Construct::Group(input) => {
                crate::compile::groups::declare(cx, site, &input.name, &input.nodes);
            }
            Construct::Provider(input) => provider(cx, site, input),
            Construct::SourceContext(input) => source_context(cx, site, input),
            // Collected by the grammar-affecting first pass above.
            Construct::Profile(_) | Construct::Discovery(_) | Construct::Reference(_) => {}
        }
    }
}

/// A field declaring merge must hold a list, which a refinement appends
/// to, or a keyed collection or object, which it merges key by key; and
/// the declared resolution policy must combine values by
/// `selection=union`, the only selection that merges.
/// Called from `finish` after the expansion pass, so fields an
/// `include` spliced in are checked with the declared ones and the
/// policy is known wherever in the stream it was declared.
pub(crate) fn merge_policy(cx: &mut Cx<'_>) {
    let selection = cx.resolution.as_ref().map(|policy| policy.selection);
    let mut reported: Vec<(Site, String, String)> = Vec::new();
    for (_, decl) in cx.types.iter() {
        let (TypeKind::Object(object), Some(site)) = (&decl.kind, decl.site) else {
            continue;
        };
        for field in object.fields.iter().filter(|field| field.merge.is_some()) {
            let label = format!("field {}", cx.names.resolve(field.name));
            if !mergeable(&cx.types, &field.ty) {
                let reason =
                    "declares merge, which only a list-, map-, or object-typed field can apply";
                reported.push((site, label.clone(), reason.to_owned()));
            }
            if let Some(selection) = selection
                && selection != ResolutionSelection::Union
            {
                let reason = format!(
                    "declares merge, which only selection=union applies, and the resolution policy is selection={}",
                    selection.name()
                );
                reported.push((site, label, reason));
            }
        }
    }
    for (at, (site, label, reason)) in reported.iter().enumerate() {
        // A generic declaration's instantiations repeat its fields.
        if !reported[..at].contains(&(*site, label.clone(), reason.clone())) {
            cx.malformed(*site, label, reason);
        }
    }
}

/// Whether a field's type holds a value a union merges: a list, a
/// keyed collection, or an object, optionally absent. A nullable list
/// may hold null, which has nothing to append.
fn mergeable(types: &crate::types::TypeCatalog, ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::List(_) | TypeExpr::Map(..) | TypeExpr::OrderedMap(..) => true,
        TypeExpr::Optional(inner) => mergeable(types, inner),
        // An instantiation survives only when monomorphization did not
        // run; its head declares the kind.
        TypeExpr::Named(name) | TypeExpr::Instantiation(name, _) => types
            .lookup(*name)
            .and_then(|id| types.get(id))
            .is_some_and(|decl| matches!(decl.kind, TypeKind::Object(_) | TypeKind::Map(_))),
        _ => false,
    }
}

/// Every construct with its site, in stream order. The sort is stable,
/// so constructs sharing a position keep document order.
fn stream(input: &DefinitionInput) -> Vec<(Site, &Located<Construct>)> {
    let mut stream: Vec<(Site, &Located<Construct>)> = input
        .documents
        .iter()
        .enumerate()
        .flat_map(|(doc, document)| {
            document
                .constructs
                .iter()
                .map(move |located| (Site::new(doc, located.span), located))
        })
        .collect();
    stream.sort_by_key(|(_, located)| located.position);
    stream
}

/// One `include TARGET` among a declaring node's keyed members, recorded
/// for the expansion pass to splice at the include's position.
fn include(cx: &mut Cx<'_>, site: Site, owner: &str, target: &str, at: usize) {
    if !cx.type_name(site, &format!("include of {owner}"), target) {
        return;
    }
    cx.expansions.push(Pending {
        owner: cx.names.intern(owner),
        target: cx.names.intern(target),
        site,
        at,
    });
}

fn type_decl(cx: &mut Cx<'_>, site: Site, ty: &TypeInput) {
    let (name, parameters, kind) = match ty {
        TypeInput::String {
            name,
            parameters,
            format,
            pattern,
            validator,
            min_length,
        } => {
            let label = format!("string {name}");
            let mut string = StringType::default();
            if let Some(rule) = format {
                string.format = format_rule(cx, site, &label, &rule.format, rule.width);
            }
            string.pattern = pattern.clone();
            if let Some(use_) = validator {
                string.validator =
                    validator_use(cx, site, &label, use_.name.clone(), use_.version.clone());
            }
            string.min_length = *min_length;
            (
                name.clone(),
                parameter_list(cx, site, &label, parameters),
                TypeKind::String(string),
            )
        }
        TypeInput::Enum {
            name,
            parameters,
            members,
        } => (
            name.clone(),
            parameter_list(cx, site, &format!("enum {name}"), parameters),
            TypeKind::Enum(EnumType {
                members: members.iter().map(member).collect(),
            }),
        ),
        TypeInput::Object {
            name,
            parameters,
            fields,
            constraints,
        } => {
            let mut built = Vec::new();
            for field in fields {
                if let Some(field) = field_def(cx, site, field) {
                    built.push(field);
                }
            }
            let constraints = constraints
                .iter()
                .map(|c| constraint(cx, c))
                .collect::<Vec<_>>();
            (
                name.clone(),
                parameter_list(cx, site, &format!("object {name}"), parameters),
                TypeKind::Object(ObjectType {
                    fields: built,
                    constraints,
                }),
            )
        }
        TypeInput::Union {
            name,
            parameters,
            variants,
        } => (
            name.clone(),
            parameter_list(cx, site, &format!("union {name}"), parameters),
            TypeKind::Union(UnionType {
                variants: variants
                    .iter()
                    .filter_map(|variant| {
                        let ty = cx.type_expr(site, &variant.ty)?;
                        Some(Variant {
                            discriminator: cx.names.intern(&variant.discriminator),
                            ty,
                            prefix: variant.prefix.clone(),
                        })
                    })
                    .collect(),
            }),
        ),
        TypeInput::Map {
            name,
            parameters,
            key,
            value,
        } => {
            let (Some(key), Some(value)) = (cx.type_expr(site, key), cx.type_expr(site, value))
            else {
                return;
            };
            (
                name.clone(),
                parameter_list(cx, site, &format!("map {name}"), parameters),
                TypeKind::Map(MapType { key, value }),
            )
        }
        TypeInput::Codec {
            name,
            parameters,
            output,
            pattern,
            captures,
            namespace,
        } => {
            let label = format!("codec {name}");
            let Some(output) = cx.type_expr(site, output) else {
                return;
            };
            let mut built: Vec<CaptureDef> = Vec::new();
            for capture in captures {
                let Some(ty) = cx.type_expr(site, &capture.ty) else {
                    continue;
                };
                built.push(CaptureDef {
                    name: capture.name.clone(),
                    ty,
                });
            }
            let namespace = namespace.as_ref().map(|n| cx.names.intern(n));
            (
                name.clone(),
                parameter_list(cx, site, &label, parameters),
                TypeKind::Codec(CodecDecl {
                    output,
                    pattern: pattern.clone(),
                    captures: built,
                    namespace,
                }),
            )
        }
    };
    declare(cx, site, name, parameters, kind);
}

/// The parameter names a declaration carries, in order.
fn parameter_list(cx: &mut Cx<'_>, site: Site, label: &str, parameters: &[String]) -> Vec<Sym> {
    let mut out = Vec::new();
    for name in parameters {
        if !crate::type_expr::is_type_name(name) {
            let spelled = found(&Value::String(name.clone()));
            cx.mistyped(site, "the name of parameter", "TypeName", &spelled);
            continue;
        }
        let sym = cx.names.intern(name);
        if out.contains(&sym) {
            duplicate_key(&mut cx.diags, (site, site), name, label);
            continue;
        }
        out.push(sym);
    }
    out
}

fn member(member: &MemberInput) -> EnumMember {
    EnumMember {
        name: member.name.clone(),
        description: member.description.clone(),
    }
}

fn field_def(cx: &mut Cx<'_>, site: Site, field: &FieldInput) -> Option<FieldDef> {
    if !field_name(&field.name) {
        let spelled = found(&Value::String(field.name.clone()));
        cx.mistyped(site, "the key of field", "FieldName", &spelled);
        return None;
    }
    let ty = cx.type_expr(site, &field.ty)?;
    let sensitivity =
        field
            .sensitivity
            .as_deref()
            .and_then(|value| match Sensitivity::from_name(value) {
                Some(parsed) => Some(parsed),
                None => {
                    cx.not_member(site, "field.sensitivity", "Sensitivity", value);
                    None
                }
            });
    let merge = field
        .merge
        .as_deref()
        .and_then(|value| match FieldMerge::from_name(value) {
            Some(parsed) => Some(parsed),
            None => {
                cx.not_member(site, "field.merge", "MergePolicy", value);
                None
            }
        });
    let fallback = field
        .default
        .as_ref()
        .and_then(|input| default(cx, site, input));
    Some(FieldDef {
        name: cx.names.intern(&field.name),
        ty,
        description: field.description.clone(),
        documented_default: field.documented_default.clone(),
        fallback,
        sensitivity,
        merge,
    })
}

/// A field's declared default: a literal value or a computed descriptor.
/// A default that is neither, or both, is rejected loudly — the engine
/// never guesses which reading was meant.
fn default(cx: &mut Cx<'_>, site: Site, input: &DefaultInput) -> Option<DefaultSpec> {
    let label = "fallback";
    match (input.computed, &input.value) {
        (true, Some(_)) => {
            let reason = "a computed default carries no literal value";
            cx.malformed(site, label, reason);
            None
        }
        (true, None) => Some(DefaultSpec::Computed {
            dependencies: input.dependencies.clone(),
        }),
        (false, Some(value)) => Some(DefaultSpec::Literal(value.clone())),
        (false, None) => {
            let reason = "a literal default carries no value";
            cx.malformed(site, label, reason);
            None
        }
    }
}

fn constraint(cx: &mut Cx<'_>, constraint: &ConstraintInput) -> Constraint {
    match constraint {
        ConstraintInput::AllOrNone { fields } => {
            Constraint::AllOrNone(fields.iter().map(|f| cx.names.intern(f)).collect())
        }
        ConstraintInput::Requires { field, required } => Constraint::Requires {
            field: cx.names.intern(field),
            required: cx.names.intern(required),
        },
        ConstraintInput::Bound { field, min, max } => Constraint::Bound {
            field: cx.names.intern(field),
            min: *min,
            max: *max,
        },
        ConstraintInput::AtMostOne { fields } => {
            Constraint::AtMostOne(fields.iter().map(|f| cx.names.intern(f)).collect())
        }
        ConstraintInput::ExactlyOneField { fields } => {
            Constraint::ExactlyOneField(fields.iter().map(|f| cx.names.intern(f)).collect())
        }
    }
}

fn node(cx: &mut Cx<'_>, site: Site, input: &NodeInput) {
    let name = input.name.clone();
    let label = format!("node {name}");
    let role = input.role.as_deref().and_then(Role::from_name);
    let typed = input.ty.is_some();
    let ty = input.ty.as_deref().map(|ty| cx.names.intern(ty));
    let profile = input.profile.as_deref().and_then(|profile| {
        cx.kebab(site, &label, profile, "profile name")
            .then(|| cx.names.intern(profile))
    });

    let mut fields = Vec::new();
    for field in &input.fields {
        if let Some(field) = field_binding(cx, site, field, &name, typed) {
            fields.push(field);
        }
    }
    let mut children = Vec::new();
    for rule in &input.children {
        if let Some(rule) = child_rule(cx, site, rule, &name) {
            children.push(rule);
        }
    }
    let mut children_groups = Vec::new();
    for rule in &input.children_groups {
        if let Some(rule) = child_rule(cx, site, rule, &name) {
            children_groups.push(rule);
        }
    }
    let mut effects = Vec::new();
    for group in &input.effects {
        if let Some(group) = effect_group(cx, site, group) {
            if effects
                .iter()
                .any(|g: &EffectGroup| g.target == group.target)
            {
                cx.diags.error(
                    INVALID_BINDING,
                    site,
                    &[&name, "two effect groups share one target"],
                );
                continue;
            }
            effects.push(group);
        }
    }
    if !effects.is_empty() && role != Some(Role::ContextControl) {
        cx.diags.error(
            INVALID_BINDING,
            site,
            &[&name, "only a context-control node declares effects"],
        );
    }
    let mut groups = Vec::new();
    for group in &input.groups {
        if group.len() < 2 {
            cx.malformed(site, &label, "exactly-one names at least two children");
            continue;
        }
        let members: Vec<Sym> = group.iter().map(|m| cx.names.intern(m)).collect();
        if (1..members.len()).any(|at| members[..at].contains(&members[at])) {
            cx.malformed(site, &label, "exactly-one names a child twice");
            continue;
        }
        groups.push(members);
    }
    let descendants = input
        .descendants
        .iter()
        .filter_map(|rule| descendant_rule(cx, site, rule, &name))
        .collect();
    let sequence = input
        .sequence
        .as_ref()
        .and_then(|seq| sequence_decl(cx, site, seq, &name));
    let identity = input.identity.as_ref().and_then(|identity| {
        let fields: Vec<Sym> = identity
            .fields
            .iter()
            .map(|field| cx.names.intern(field))
            .collect();
        if identity.form != "qualified-name" {
            cx.not_member(site, "compose.form", "ComposeForm", &identity.form);
            return None;
        }
        if fields.is_empty() {
            cx.malformed(site, &label, "identity composes no field");
            return None;
        }
        if !cx.kebab(site, &label, &identity.domain, "identity domain") {
            return None;
        }
        let domain = cx.names.intern(&identity.domain);
        Some(IdentityDecl {
            domain,
            fields,
            site,
        })
    });
    let value = input
        .value
        .as_ref()
        .and_then(|value| dependent(cx, site, value));
    let mut cases = Vec::new();
    for case in &input.cases {
        if !field_name(&case.property) {
            let spelled = found(&Value::String(case.property.clone()));
            cx.mistyped(site, "the property of case", "FieldName", &spelled);
            continue;
        }
        let Some(ty) = cx.type_expr(site, &case.ty) else {
            continue;
        };
        cases.push(CaseRule {
            property: cx.names.intern(&case.property),
            equals: case.equals.clone(),
            ty,
            site,
        });
    }
    let otherwise = input
        .otherwise
        .as_deref()
        .and_then(|ty| cx.type_expr(site, ty));
    let annotations = input
        .annotations
        .iter()
        .map(|fact| NodeAnnotation {
            name: cx.names.intern(&fact.name),
            value: fact.value.clone(),
        })
        .collect();
    let description = input.description.clone();
    let placement = match (&input.parent, &input.parents) {
        (Some(_), Some(_)) => {
            cx.diags.error(
                INVALID_BINDING,
                site,
                &[&name, "gives both parent= and parents"],
            );
            return;
        }
        (Some(parent), None) => vec![parent.clone()],
        (None, Some(parents)) if parents.is_empty() => {
            cx.malformed(site, &label, "parents names no parent");
            return;
        }
        (None, Some(parents)) => parents.clone(),
        (None, None) => Vec::new(),
    };
    let mut under = Vec::new();
    for parent in placement {
        let sym = cx.names.intern(&parent);
        if under.contains(&sym) {
            cx.malformed(site, &label, &format!("parent {parent} listed twice"));
            return;
        }
        under.push(sym);
    }
    if typed && ty.is_none() {
        // The rejected type= is reported; its fields are not also
        // reported as untyped.
        return;
    }
    let parent_group = input.parent_group.as_deref().map(|g| cx.names.intern(g));
    // root=#true on a node placed only through a group stays marked until
    // the group expands; on a node with no parents at all it is a no-op.
    let placement = match (input.root, under.is_empty()) {
        (Some(true), true) if parent_group.is_some() => Placement::RootAndUnder(under),
        (_, true) => Placement::Root,
        (Some(true), false) => Placement::RootAndUnder(under),
        (_, false) => Placement::Under(under),
    };
    cx.nodes.push(NodeDecl {
        name: cx.names.intern(&name),
        role,
        ty,
        placement,
        parent_group,
        profile,
        fields,
        children,
        children_groups,
        effects,
        groups,
        descendants,
        sequence,
        value,
        cases,
        otherwise,
        identity,
        description,
        annotations,
        site,
    });
}

/// A schema-level `annotation NAME TYPE [required]` declaration: the
/// name node declarations may attach, the type every value is checked
/// against when the definition compiles, and whether the annotation is
/// total. A name declared twice is a duplicate declaration.
fn annotation_decl(cx: &mut Cx<'_>, site: Site, input: &AnnotationInput) {
    let name = cx.names.intern(&input.name);
    if let Some(first) = cx
        .annotations
        .iter()
        .find(|decl| decl.name == name)
        .map(|decl| decl.site)
    {
        duplicate(
            &mut cx.diags,
            cx.kind,
            (site, first),
            "annotation",
            &input.name,
        );
        return;
    }
    let Some(ty) = cx.type_expr(site, &input.ty) else {
        return;
    };
    cx.annotations.push(AnnotationDecl {
        name,
        ty,
        required: input.required,
        site,
    });
}

/// One descendant rule of a node declaration.
fn descendant_rule(
    cx: &mut Cx<'_>,
    site: Site,
    input: &crate::compile::input::DescendantRuleInput,
    owner: &str,
) -> Option<DescendantRule> {
    if let (Some(min), Some(max)) = (input.min, input.max)
        && min > max
    {
        let reason = format!(
            "descendants {} has min-occurs {min} above max-occurs {max}",
            input.name
        );
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    if input.max == Some(0) {
        let reason = format!("descendants {} permits no occurrence", input.name);
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    Some(DescendantRule {
        name: cx.names.intern(&input.name),
        min: input.min,
        max: input.max,
        site,
    })
}

/// An ordered sequence a node declaration's children must match, with
/// absent bounds meaning exactly once.
fn sequence_decl(
    cx: &mut Cx<'_>,
    site: Site,
    input: &SequenceInput,
    owner: &str,
) -> Option<SequenceDecl> {
    let mut steps = Vec::with_capacity(input.steps.len());
    for step in &input.steps {
        let mut alternatives: Vec<Sym> = step
            .alternatives
            .iter()
            .map(|name| cx.names.intern(name))
            .collect();
        if let Some(name) = &step.name {
            alternatives.push(cx.names.intern(name));
        }
        if alternatives.is_empty() {
            let reason = "a sequence step names no node and no alternatives";
            cx.diags.error(INVALID_BINDING, site, &[owner, reason]);
            continue;
        }
        let min = step.min.unwrap_or(1).max(1);
        let max = step.max.unwrap_or_else(|| min.max(1));
        if min > max {
            let reason = format!("a sequence step has min-occurs {min} above max-occurs {max}");
            cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
            continue;
        }
        if (1..alternatives.len()).any(|at| alternatives[..at].contains(&alternatives[at])) {
            let reason = "a sequence step names a node twice";
            cx.diags.error(INVALID_BINDING, site, &[owner, reason]);
            continue;
        }
        steps.push(SequenceStep {
            alternatives,
            min,
            max,
            site,
        });
    }
    if steps.is_empty() {
        let reason = "sequence names no step";
        cx.diags.error(INVALID_BINDING, site, &[owner, reason]);
        return None;
    }
    Some(SequenceDecl { steps, site })
}

/// A node declaration's dependent payload binding. An unknown codec
/// fails closed; a known one the reading processor does not install
/// compiles, and the binder defers its payload as a pending binding.
fn dependent(cx: &mut Cx<'_>, site: Site, input: &DependentInput) -> Option<DependentBinding> {
    let codec = match BindingCodec::from_name(&input.codec) {
        Some(parsed) => parsed,
        None => {
            cx.not_member(site, "value.codec", "BindingCodec", &input.codec);
            return None;
        }
    };
    for (spelled, what) in [
        (&input.target, "target field"),
        (&input.member, "member field"),
    ] {
        if !field_name(spelled) {
            let found = format!("string {spelled:?}");
            cx.mistyped(site, what, "FieldName", &found);
            return None;
        }
    }
    Some(DependentBinding {
        target: cx.names.intern(&input.target),
        member: cx.names.intern(&input.member),
        codec,
        site,
    })
}

fn field_binding(
    cx: &mut Cx<'_>,
    site: Site,
    input: &FieldBindingInput,
    owner: &str,
    typed_owner: bool,
) -> Option<FieldBinding> {
    let name = input.name.clone();
    let label = format!("field {name} of {owner}");
    let ty = match &input.ty {
        Some(text) if typed_owner => {
            let reason = format!("field {name} gives a type although the node's type= declares it");
            cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
            return None;
        }
        Some(text) => Some(cx.type_expr(site, text)?),
        None if typed_owner => None,
        None => {
            let reason = format!("field {name} needs a type because the node has no type=");
            cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
            return None;
        }
    };
    let mut sources = Vec::new();
    for input in &input.sources {
        if let Some(source) = source(cx, site, &label, input) {
            sources.push(source);
        }
    }
    if sources.is_empty() {
        let reason = format!("field {name} has no source");
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    let collision = input
        .collision
        .as_deref()
        .and_then(|spelled| match spelled {
            "reject" => Some(Collision::Reject),
            _ => {
                cx.not_member(site, "field.collision", "Collision", spelled);
                None
            }
        });
    let duplicate_keys = input
        .duplicate_keys
        .as_deref()
        .and_then(|spelled| match spelled {
            "reject" => Some(DuplicateKeys::Reject),
            _ => {
                cx.not_member(site, "field.duplicate-keys", "DuplicateKeys", spelled);
                None
            }
        });
    let spellings = sources
        .iter()
        .filter(|s| !matches!(s, Source::Context { .. } | Source::Expand { .. }))
        .count();
    let alternatives = sources
        .iter()
        .filter(|s| {
            !matches!(
                s,
                Source::Context { .. } | Source::Children { .. } | Source::Expand { .. }
            )
        })
        .count();
    if alternatives > 1 && collision.is_none() {
        let reason = format!(
            "field {name} has {alternatives} alternative sources and no collision policy; no implicit precedence exists"
        );
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    if duplicate_keys.is_some() && !sources.iter().any(|s| matches!(s, Source::Entries { .. })) {
        let reason = format!("field {name} sets duplicate-keys without a keyed entries source");
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    let explicit = spellings > 0;
    let unpoliced = sources
        .iter()
        .any(|s| matches!(s, Source::Context { policy: None, .. }));
    if explicit && unpoliced {
        let reason = format!(
            "field {name} combines an explicit source with a context source that names no policy"
        );
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    Some(FieldBinding {
        name: cx.names.intern(&name),
        ty,
        sources,
        collision,
        duplicate_keys,
        site,
    })
}

fn source(cx: &mut Cx<'_>, site: Site, label: &str, input: &SourceInput) -> Option<Source> {
    match input {
        SourceInput::Argument(index) => Some(Source::Argument(*index)),
        SourceInput::Args => Some(Source::Args),
        SourceInput::Property(name) => Some(Source::Property(cx.names.intern(name))),
        SourceInput::Child {
            name,
            sources,
            compact,
        } => Some(Source::Child {
            name: cx.names.intern(name),
            sources: nested(cx, site, label, sources),
            compact: *compact,
        }),
        SourceInput::Children {
            name,
            sources,
            compact,
        } => Some(Source::Children {
            name: cx.names.intern(name),
            sources: nested(cx, site, label, sources),
            compact: *compact,
        }),
        SourceInput::Entries { node, key, value } => entries(cx, site, label, node, key, value),
        SourceInput::Expand { target, member } => {
            if !cx.type_name(site, label, target) {
                return None;
            }
            if member.is_empty() {
                let found = format!("string {member:?}");
                cx.mistyped(site, "the member of expand", "MemberName", &found);
                return None;
            }
            Some(Source::Expand {
                target: cx.names.intern(target),
                member: cx.names.intern(member),
            })
        }
        SourceInput::Context { slot, policy } => {
            let policy = match policy.as_deref() {
                None => None,
                Some("fallback") => Some(ContextPolicy::Fallback),
                Some("fixed") => Some(ContextPolicy::Fixed),
                Some(other) => {
                    cx.not_member(site, "context.policy", "ContextPolicy", other);
                    return None;
                }
            };
            Some(Source::Context {
                slot: cx.names.intern(slot),
                policy,
            })
        }
    }
}

fn nested(cx: &mut Cx<'_>, site: Site, label: &str, sources: &[SourceInput]) -> Vec<Source> {
    sources
        .iter()
        .filter_map(|input| source(cx, site, label, input))
        .collect()
}

fn entries(
    cx: &mut Cx<'_>,
    site: Site,
    label: &str,
    node: &str,
    key: &str,
    value: &str,
) -> Option<Source> {
    let index = key
        .strip_prefix("arg(")
        .and_then(|rest| rest.strip_suffix(')'))
        .and_then(parse_u64);
    let Some(index) = index else {
        cx.malformed(site, label, &format!("key {key:?} is not arg(N)"));
        return None;
    };
    Some(Source::Entries {
        node: cx.names.intern(node),
        key: index,
        value: cx.names.intern(value),
    })
}

fn child_rule(
    cx: &mut Cx<'_>,
    site: Site,
    input: &ChildRuleInput,
    owner: &str,
) -> Option<ChildRule> {
    let label = format!("children {}", input.name);
    // A rule with branches may refuse its fallback instances outright.
    let refusable = !input.branches.is_empty();
    if !bounds(cx, site, owner, &label, (input.min, input.max), refusable) {
        return None;
    }
    let mut branches = Vec::with_capacity(input.branches.len());
    let mut ok = true;
    for branch in &input.branches {
        let (min, max) = match branch {
            ConditionInput::When { min, max, .. }
            | ConditionInput::All { min, max, .. }
            | ConditionInput::Any { min, max, .. } => (*min, *max),
        };
        let what = format!("a when branch of {label}");
        ok &= bounds_ordered(cx, site, owner, &what, min, max);
        match condition(cx, site, owner, branch) {
            Some(condition) => branches.push(WhenBranch {
                condition,
                min,
                max,
            }),
            None => ok = false,
        }
    }
    if !ok {
        return None;
    }
    if !branches.is_empty() && input.min.is_none() && input.max.is_none() {
        cx.diags
            .error(FALLBACK_LESS_WHEN_RULE, site, &[owner, &input.name]);
        return None;
    }
    if !conditions::disjoint(&branches) {
        cx.diags
            .error(OVERLAPPING_WHEN_BRANCHES, site, &[owner, &input.name]);
        return None;
    }
    Some(ChildRule {
        name: cx.names.intern(&input.name),
        min: input.min,
        max: input.max,
        branches,
        site,
    })
}

/// A rule's base bounds: ordered, and admitting at least one occurrence
/// unless `refusable`. A branch may always refuse its instances outright
/// with `max-occurs=0`.
fn bounds(
    cx: &mut Cx<'_>,
    site: Site,
    owner: &str,
    label: &str,
    (min, max): (Option<u64>, Option<u64>),
    refusable: bool,
) -> bool {
    if !bounds_ordered(cx, site, owner, label, min, max) {
        return false;
    }
    if max == Some(0) && !refusable {
        let reason = format!("{label} permits no occurrence");
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return false;
    }
    true
}

fn bounds_ordered(
    cx: &mut Cx<'_>,
    site: Site,
    owner: &str,
    label: &str,
    min: Option<u64>,
    max: Option<u64>,
) -> bool {
    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        let reason = format!("{label} has min-occurs {min} above max-occurs {max}");
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return false;
    }
    true
}

/// One condition, its combinators each composing at least one member.
fn condition(
    cx: &mut Cx<'_>,
    site: Site,
    owner: &str,
    input: &ConditionInput,
) -> Option<Condition> {
    let (members, combinator, any) = match input {
        ConditionInput::When {
            property, equals, ..
        } => {
            if !field_name(property) {
                let spelled = found(&Value::String(property.clone()));
                cx.mistyped(site, "the property of when", "FieldName", &spelled);
                return None;
            }
            return Some(Condition::Equals {
                property: cx.names.intern(property),
                equals: equals.clone(),
            });
        }
        ConditionInput::All { members, .. } => (members, "when-all", false),
        ConditionInput::Any { members, .. } => (members, "when-any", true),
    };
    if members.is_empty() {
        cx.diags
            .error(EMPTY_WHEN_COMPOSITION, site, &[owner, combinator]);
        return None;
    }
    let mut composed = Vec::with_capacity(members.len());
    for member in members {
        composed.push(condition(cx, site, owner, member)?);
    }
    Some(if any {
        Condition::Any(composed)
    } else {
        Condition::All(composed)
    })
}

fn effect_group(cx: &mut Cx<'_>, site: Site, input: &EffectGroupInput) -> Option<EffectGroup> {
    let target = match input.target.as_str() {
        "following" => EffectTarget::Following,
        "children" => EffectTarget::Children,
        other => {
            cx.not_member(site, "effects.target", "EffectTarget", other);
            return None;
        }
    };
    let mut effects = Vec::new();
    for input in &input.effects {
        if let Some(effect) = effect(cx, site, input) {
            effects.push(effect);
        }
    }
    Some(EffectGroup {
        target,
        effects,
        site,
    })
}

fn effect(cx: &mut Cx<'_>, site: Site, input: &EffectInput) -> Option<Effect> {
    match input {
        EffectInput::Set {
            slot,
            field,
            value,
            layer,
        } => {
            let from = match (field, value) {
                (Some(field), None) => Some(SetFrom::Field(cx.names.intern(field))),
                (None, Some(value)) => Some(SetFrom::Value(value.clone())),
                _ => {
                    cx.malformed(site, "set", "set takes exactly one of field= and value=");
                    None
                }
            };
            from.map(|from| Effect::Set {
                slot: cx.names.intern(slot),
                from,
                layer: layer.as_deref().map(|layer| cx.names.intern(layer)),
            })
        }
        EffectInput::Remove { slot } => Some(Effect::Remove {
            slot: cx.names.intern(slot),
        }),
        EffectInput::Clear { namespace } => Some(Effect::Clear {
            namespace: cx.names.intern(namespace),
        }),
        EffectInput::Handler { name } => {
            if !cx.extensions.has_handler(name) {
                cx.diags.error(NOT_INSTALLED, site, &["handler", name]);
                return None;
            }
            Some(Effect::Handler { name: name.clone() })
        }
    }
}

fn slot(cx: &mut Cx<'_>, site: Site, input: &SlotInput) {
    let Some(ty) = cx.type_expr(site, &input.ty) else {
        return;
    };
    let sym = cx.names.intern(&input.name);
    if let Some(first) = cx
        .slots
        .iter()
        .find(|slot| slot.name == sym)
        .map(|slot| slot.site)
    {
        duplicate(
            &mut cx.diags,
            cx.kind,
            (site, first),
            "context slot",
            &input.name,
        );
        return;
    }
    let namespace = input
        .namespace
        .clone()
        .map(|namespace| cx.names.intern(&namespace));
    let accumulate = input.accumulate.as_deref().and_then(|op| {
        let parsed = AccumulateOp::from_name(op).or_else(|| {
            cx.not_member(site, "slot.accumulate.op", "AccumulateOp", op);
            None
        })?;
        let shape = accumulate_shape(&ty);
        let ok = match parsed {
            AccumulateOp::Append => shape == Some(AccumulateShape::List),
            AccumulateOp::Merge => shape == Some(AccumulateShape::Map),
        };
        if !ok {
            let reason = format!(
                "slot {} declares accumulate {} but its type is not {}-typed",
                &input.name,
                parsed.name(),
                match parsed {
                    AccumulateOp::Append => "list",
                    AccumulateOp::Merge => "map",
                }
            );
            cx.diags
                .error(INVALID_BINDING, site, &[&input.name, &reason]);
            return None;
        }
        Some(parsed)
    });
    let (duplicates, merge_duplicates, layers) = slot_policy(cx, site, input, accumulate);
    cx.slots.push(SlotDecl {
        name: sym,
        ty,
        namespace,
        description: input.description.clone(),
        accumulate,
        merge_duplicates,
        duplicates,
        layers,
        site,
    });
}

/// A slot's duplicate policy, merge treatment, and layer order, checked
/// against each other and its accumulation (`KDLITH-DLC005`). Absent or
/// broken, the slot keeps the fail-closed reject defaults.
fn slot_policy(
    cx: &mut Cx<'_>,
    site: Site,
    input: &SlotInput,
    accumulate: Option<AccumulateOp>,
) -> (SlotDuplicates, MergeDuplicates, Vec<Sym>) {
    const CLOSED: (SlotDuplicates, MergeDuplicates, Vec<Sym>) =
        (SlotDuplicates::Reject, MergeDuplicates::Reject, Vec::new());
    let label = format!("slot {}", input.name);
    let duplicates = match input.duplicates.as_deref() {
        None => SlotDuplicates::Reject,
        Some(name) => {
            let Some(parsed) = SlotDuplicates::from_name(name) else {
                cx.not_member(site, "slot.duplicates", "SlotDuplicates", name);
                return CLOSED;
            };
            parsed
        }
    };
    let merge_duplicates = match input.accumulate_duplicates.as_deref() {
        None => MergeDuplicates::Reject,
        Some(name) => {
            let Some(parsed) = MergeDuplicates::from_name(name) else {
                cx.not_member(site, "slot.accumulate.duplicates", "MergeDuplicates", name);
                return CLOSED;
            };
            parsed
        }
    };
    let mut ok = true;
    if input.accumulate.is_some() && duplicates != SlotDuplicates::Reject {
        // Accumulation already defines how a second value combines; a
        // slot-level policy would contradict it.
        let reason = format!(
            "an accumulating slot combines through its operation, so duplicates={} contradicts it",
            duplicates.name()
        );
        cx.malformed(site, &label, &reason);
        ok = false;
    }
    // The dialect's fallback spells `reject` on every accumulation, so
    // only a declared non-default treatment can contradict its operation.
    if merge_duplicates != MergeDuplicates::Reject {
        let reason = match (input.accumulate.as_deref(), accumulate) {
            (None, _) => Some("an accumulation duplicate treatment needs an accumulate operation"),
            (Some(_), Some(AccumulateOp::Append)) => Some(
                "an accumulation duplicate treatment is declared for merge only; append has no keys",
            ),
            // Merge, or an operation already reported as no member.
            _ => None,
        };
        if let Some(reason) = reason {
            cx.malformed(site, &label, reason);
            ok = false;
        }
    }
    let mut layers: Vec<Sym> = Vec::new();
    match (duplicates, input.layers.as_deref()) {
        (SlotDuplicates::Layered, None | Some([])) => {
            cx.malformed(
                site,
                &label,
                "a layered slot lists its layer order in layers",
            );
            ok = false;
        }
        (SlotDuplicates::Layered, Some(names)) => {
            for name in names {
                if !cx.kebab(site, &label, name, "layer name") {
                    ok = false;
                    continue;
                }
                let sym = cx.names.intern(name);
                if layers.contains(&sym) {
                    let reason = format!("layers lists {name} more than once");
                    cx.malformed(site, &label, &reason);
                    ok = false;
                    continue;
                }
                layers.push(sym);
            }
        }
        (_, Some(_)) => {
            let reason = format!(
                "layers is declared only by a layered slot, and this slot is duplicates={}",
                duplicates.name()
            );
            cx.malformed(site, &label, &reason);
            ok = false;
        }
        (_, None) => {}
    }
    if ok {
        (duplicates, merge_duplicates, layers)
    } else {
        CLOSED
    }
}

/// The collection shape a slot's type has, once optional and nullable
/// wrappers are stripped: accumulation is defined for list and map
/// slots only.
fn accumulate_shape(mut ty: &TypeExpr) -> Option<AccumulateShape> {
    loop {
        match ty {
            TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) => ty = inner,
            TypeExpr::List(_) => return Some(AccumulateShape::List),
            TypeExpr::Map(_, _) | TypeExpr::OrderedMap(_, _) => return Some(AccumulateShape::Map),
            _ => return None,
        }
    }
}

#[derive(PartialEq, Eq)]
enum AccumulateShape {
    List,
    Map,
}

fn rule(cx: &mut Cx<'_>, site: Site, input: &DocumentRuleInput) {
    if let Some(earlier) = cx.document_rule.as_ref().map(|rule| rule.site) {
        duplicate(&mut cx.diags, cx.kind, (site, earlier), "document", "rule");
        return;
    }
    let first = input.first.as_ref().map(|names| name_list(cx, site, names));
    if first.as_ref().is_some_and(Vec::is_empty) {
        cx.malformed(site, "document", "first names no node");
    }
    let following = name_list(cx, site, &input.following);
    cx.document_rule = Some(DocumentRule {
        first,
        following,
        site,
    });
}

fn name_list(cx: &mut Cx<'_>, site: Site, names: &[String]) -> Vec<Sym> {
    let mut out: Vec<Sym> = Vec::new();
    for name in names {
        let sym = cx.names.intern(name);
        if out.contains(&sym) {
            cx.malformed(site, "document", &format!("names {name} twice"));
            continue;
        }
        out.push(sym);
    }
    out
}

fn import(cx: &mut Cx<'_>, site: Site, input: &ImportInput) {
    let Some(repeated) = Repeated::from_name(&input.repeated) else {
        cx.not_member(site, "import.repeated", "RepeatedImport", &input.repeated);
        return;
    };
    let alias = match &input.alias {
        Some(alias) if cx.kebab(site, "import", alias, "alias") => Some(cx.names.intern(alias)),
        Some(_) => return,
        None => None,
    };
    if let Some(alias) = alias
        && let Some(first) = cx
            .imports
            .iter()
            .find(|import| import.alias == Some(alias))
            .map(|import| import.site)
    {
        duplicate_key(
            &mut cx.diags,
            (site, first),
            "the alias of import",
            "document.imports",
        );
        return;
    }
    if discovery::is_expression(&input.path) {
        // An expression is evaluated under the grammar the active
        // profile binds, or the schema root when no profiles are
        // declared. Compile-time validation covers the root-bound case;
        // a profile-bound grammar validates at invocation.
        let grammar = if cx.profiles.is_empty() {
            cx.root_grammar.as_deref().and_then(|name| {
                cx.discoveries
                    .iter()
                    .find(|d| cx.names.resolve(d.name) == name)
            })
        } else {
            None
        };
        match grammar {
            Some(grammar) => {
                if let Err(error) = discovery::parse(&input.path, Some(grammar), &cx.names) {
                    report_expansion(cx, site, &error);
                    return;
                }
            }
            None => {
                if cx.profiles.is_empty() {
                    let construct = format!("import {:?}", input.path);
                    cx.malformed(site, &construct, "import paths are literal");
                    return;
                }
                // A profile-bound grammar validates the expression when
                // the invocation names its profile.
            }
        }
    }
    cx.imports.push(ImportDecl {
        path: input.path.clone(),
        project: input.project.iter().map(|n| cx.names.intern(n)).collect(),
        repeated,
        alias,
        site,
    });
}

/// Report an import-expression failure under its registered code.
fn report_expansion(cx: &mut Cx<'_>, site: Site, error: &ExpansionError) {
    cx.diags.error(error.code(), site, &error.values());
}

/// A document profile: a named selection of root types.
fn profile(cx: &mut Cx<'_>, site: Site, input: &ProfileInput) {
    let label = format!("profile {}", input.name);
    if !cx.kebab(site, "profile", &input.name, "profile name") {
        return;
    }
    let name = cx.names.intern(&input.name);
    if let Some(first) = cx
        .profiles
        .iter()
        .find(|profile| profile.name == name)
        .map(|profile| profile.site)
    {
        duplicate(
            &mut cx.diags,
            cx.kind,
            (site, first),
            "profile",
            &input.name,
        );
        return;
    }
    let roots = input.roots.as_ref().map(|roots| {
        let mut out = Vec::new();
        for root in roots {
            if !cx.type_name(site, &label, root) {
                continue;
            }
            let sym = cx.names.intern(root);
            if out.contains(&sym) {
                cx.malformed(site, &label, &format!("roots allows {root} twice"));
                continue;
            }
            out.push(sym);
        }
        out
    });
    if roots.as_ref().is_some_and(Vec::is_empty) {
        cx.malformed(site, &label, "roots allows no root");
        return;
    }
    let grammar = input.grammar.as_deref().and_then(|grammar| {
        cx.kebab(site, &label, grammar, "grammar name")
            .then(|| cx.names.intern(grammar))
    });
    let extends = input.extends.as_deref().and_then(|parent| {
        cx.kebab(site, &label, parent, "extended profile name")
            .then(|| cx.names.intern(parent))
    });
    let rules = match profile_rules(cx, site, &label, input.rules.as_deref()) {
        Some(rules) => rules,
        None => return,
    };
    // At most one profile of a schema is the default: the first in
    // contribution order keeps the mark, and every later one is
    // rejected at its own site.
    let mut default = input.default == Some(true);
    if default
        && let Some(first) = cx
            .profiles
            .iter()
            .find(|profile| profile.default)
            .map(|profile| (profile.site, cx.names.resolve(profile.name).to_owned()))
    {
        let shown = format!("profile {}", input.name);
        let earlier = format!("profile {}", first.1);
        cx.diags.labelled(
            SECOND_DEFAULT,
            site,
            first.0,
            &[&shown, &earlier],
            "first marked default here",
        );
        default = false;
    }
    cx.profiles.push(ProfileDecl {
        name,
        extends,
        default,
        roots,
        grammar,
        rules,
        site,
    });
}

/// A profile's declared match rules, in written order; `None` when the
/// block is malformed. A `match` block holding no rule would select
/// nothing, so it is malformed rather than silently inert.
fn profile_rules(
    cx: &mut Cx<'_>,
    site: Site,
    label: &str,
    rules: Option<&[MatchRuleInput]>,
) -> Option<Vec<MatchRule>> {
    let Some(rules) = rules else {
        return Some(Vec::new());
    };
    if rules.is_empty() {
        cx.malformed(site, label, "match holds no rule");
        return None;
    }
    let mut out = Vec::with_capacity(rules.len());
    for rule in rules {
        let Some(kind) = MatchKind::from_name(&rule.kind) else {
            cx.malformed(
                site,
                label,
                &format!("match rule {} is not a rule", rule.kind),
            );
            return None;
        };
        out.push(MatchRule::new(kind, rule.text.clone()));
    }
    Some(out)
}

/// A discovery grammar declaration.
fn discovery_decl(cx: &mut Cx<'_>, site: Site, input: &DiscoveryInput) {
    if !cx.kebab(site, "discovery", &input.name, "grammar name") {
        return;
    }
    let name = cx.names.intern(&input.name);
    if let Some(first) = cx
        .discoveries
        .iter()
        .find(|grammar| grammar.name == name)
        .map(|grammar| grammar.site)
    {
        duplicate(
            &mut cx.diags,
            cx.kind,
            (site, first),
            "discovery",
            &input.name,
        );
        return;
    }
    let mut anchors = Vec::new();
    for anchor in &input.anchors {
        let label = format!("anchor {}", anchor.name);
        if !cx.kebab(site, &label, &anchor.name, "anchor name") {
            continue;
        }
        if !cx.kebab(site, &label, &anchor.provider, "provider name") {
            continue;
        }
        let name = cx.names.intern(&anchor.name);
        if anchors
            .iter()
            .any(|declared: &AnchorDecl| declared.name == name)
        {
            cx.malformed(site, &label, "declares the anchor twice");
            continue;
        }
        anchors.push(AnchorDecl {
            name,
            provider: cx.names.intern(&anchor.provider),
            site,
        });
    }
    cx.discoveries.push(DiscoveryDecl {
        name,
        anchors,
        patterns: Patterns {
            star: input.star,
            doublestar: input.doublestar,
            braces: input.braces,
        },
        allow_empty: input.allow_empty,
        site,
    });
}

/// A named reference type.
fn reference(cx: &mut Cx<'_>, site: Site, input: &ReferenceInput) {
    let label = format!("reference {}", input.name);
    if !cx.type_name(site, "reference", &input.name) {
        return;
    }
    let name = cx.names.intern(&input.name);
    if let Some(first) = cx
        .references
        .iter()
        .find(|reference| reference.name == name)
        .map(|reference| reference.site)
    {
        duplicate(
            &mut cx.diags,
            cx.kind,
            (site, first),
            "reference",
            &input.name,
        );
        return;
    }
    if !cx.type_name(site, &label, &input.target) {
        return;
    }
    let target = cx.names.intern(&input.target);
    let codec = input.codec.as_deref().and_then(|codec| {
        cx.type_name(site, &label, codec)
            .then(|| cx.names.intern(codec))
    });
    // A codec's namespace metadata is consumed here: a decoded identity
    // belongs to that reference namespace.
    let namespace = codec.and_then(|codec| {
        cx.types
            .lookup(codec)
            .and_then(|id| cx.types.get(id))
            .and_then(|decl| match &decl.kind {
                TypeKind::Codec(codec) => codec.namespace,
                _ => None,
            })
    });
    let lookup = input.lookup.as_deref().and_then(|lookup| {
        cx.kebab(site, &label, lookup, "lookup domain")
            .then(|| cx.names.intern(lookup))
    });
    let qualification = match Qualification::from_name(&input.qualification) {
        Some(qualification) => qualification,
        None => {
            cx.not_member(
                site,
                "reference.qualification",
                "Qualification",
                &input.qualification,
            );
            return;
        }
    };
    let mut qualifiers = Vec::new();
    for qualifier in &input.qualifiers {
        if !field_name(&qualifier.name) {
            let found = found(&Value::String(qualifier.name.clone()));
            cx.mistyped(site, "the name of qualifier", "FieldName", &found);
            continue;
        }
        let Some(ty) = cx.type_expr(site, &qualifier.ty) else {
            continue;
        };
        qualifiers.push(QualifierDecl {
            name: cx.names.intern(&qualifier.name),
            ty,
            required: qualifier.required,
            site,
        });
    }
    cx.references.push(ReferenceDecl {
        name,
        target,
        codec,
        namespace,
        lookup,
        qualification,
        qualifiers,
        site,
    });
}

/// The declared contribution-resolution policy. The resolver must be
/// installed; resolution itself belongs to the registered resolver.
fn resolution(cx: &mut Cx<'_>, site: Site, input: &ResolutionInput) {
    let label = format!("resolution {}", input.resolver);
    if !cx.kebab(site, "resolution", &input.resolver, "resolver name") {
        return;
    }
    if cx.resolution.is_some() {
        cx.malformed(site, "resolution", "declared twice in one schema");
        return;
    }
    if !cx.extensions.has_resolver(&input.resolver) {
        cx.diags
            .error(NOT_INSTALLED, site, &["resolver", &input.resolver]);
        return;
    }
    let selection = match ResolutionSelection::from_name(&input.selection) {
        Some(selection) => selection,
        None => {
            cx.not_member(
                site,
                "resolution.selection",
                "ResolutionSelection",
                &input.selection,
            );
            return;
        }
    };
    let duplicates = match DuplicateTreatment::from_name(&input.duplicates) {
        Some(duplicates) => duplicates,
        None => {
            cx.not_member(
                site,
                "resolution.duplicates",
                "DuplicateTreatment",
                &input.duplicates,
            );
            return;
        }
    };
    let missing = match MissingRequirement::from_name(&input.missing) {
        Some(missing) => missing,
        None => {
            cx.not_member(
                site,
                "resolution.missing",
                "MissingRequirement",
                &input.missing,
            );
            return;
        }
    };
    let mut layers = Vec::new();
    for layer in &input.layers {
        if layer.is_empty() {
            cx.malformed(site, &label, "names an empty layer");
            return;
        }
        if layers.contains(layer) {
            cx.malformed(site, &label, &format!("names layer {layer} twice"));
            return;
        }
        layers.push(layer.clone());
    }
    cx.resolution = Some(ResolutionPolicyDecl {
        resolver: cx.names.intern(&input.resolver),
        layers,
        selection,
        duplicates,
        missing,
        site,
    });
}

/// A secret-provider descriptor: identifier, optional TTL, and a command
/// as a list of strings that preserves argument boundaries. It is
/// descriptor data only — nothing ever executes it.
fn provider(cx: &mut Cx<'_>, site: Site, input: &ProviderInput) {
    let label = format!("provider {}", input.id);
    if input.id.is_empty() {
        cx.mistyped(
            site,
            "the id of provider",
            "SecretId",
            &found(&Value::String(String::new())),
        );
        return;
    }
    if cx.providers.iter().any(|p| p.id == input.id) {
        duplicate(&mut cx.diags, cx.kind, (site, site), "provider", &input.id);
        return;
    }
    if input.command.is_empty() {
        cx.malformed(site, &label, "declares no command");
        return;
    }
    if input.command.iter().any(String::is_empty) {
        cx.malformed(site, &label, "declares an empty command argument");
        return;
    }
    cx.providers.push(ProviderDecl {
        id: input.id.clone(),
        ttl: input.ttl,
        command: input.command.clone(),
        site,
    });
}

/// The declared source-context block. The mode is required — there is no
/// default, so an undeclared mode is a missing required property, never
/// a silent fallback — and every source names a registered
/// source-metadata provider.
fn source_context(cx: &mut Cx<'_>, site: Site, input: &SourceContextInput) {
    if cx.source_context.is_some() {
        cx.malformed(site, "source-context", "declared twice in one schema");
        return;
    }
    let Some(mode) = SourceContextMode::from_name(&input.mode) else {
        // The mode is required with no default: absent is the binding's
        // missing-required rejection; present but unknown is an unknown
        // member.
        if input.mode.is_empty() {
            cx.missing(site, "source-context", "mode");
        } else {
            cx.not_member(
                site,
                "source-context.mode",
                "SourceContextMode",
                &input.mode,
            );
        }
        return;
    };
    if input.fields.is_empty() {
        cx.malformed(site, "source-context", "declares no field");
        return;
    }
    let mut fields = Vec::new();
    for field in &input.fields {
        if !field_name(&field.name) {
            let spelled = found(&Value::String(field.name.clone()));
            cx.mistyped(
                site,
                "the key of source-context field",
                "FieldName",
                &spelled,
            );
            continue;
        }
        let Some(ty) = cx.type_expr(site, &field.ty) else {
            continue;
        };
        if field.sources.is_empty() {
            cx.malformed(
                site,
                &format!("source-context field {}", field.name),
                "derives from no source",
            );
            continue;
        }
        let mut sources = Vec::new();
        let mut broken = false;
        for source in &field.sources {
            if !cx.kebab(site, "source-context", source, "source name") {
                broken = true;
                continue;
            }
            if !cx.extensions.has_source_provider(source) {
                cx.diags.error(NOT_INSTALLED, site, &["source", source]);
                broken = true;
                continue;
            }
            if sources.contains(source) {
                cx.malformed(
                    site,
                    &format!("source-context field {}", field.name),
                    &format!("derives from {source} twice"),
                );
                broken = true;
                continue;
            }
            sources.push(source.clone());
        }
        if broken {
            continue;
        }
        let name = cx.names.intern(&field.name);
        if fields
            .iter()
            .any(|declared: &SourceFieldDecl| declared.name == name)
        {
            duplicate_key(
                &mut cx.diags,
                (site, site),
                &field.name,
                "source-context.fields",
            );
            continue;
        }
        fields.push(SourceFieldDecl {
            name,
            ty,
            sources,
            site,
        });
    }
    if fields.is_empty() {
        return;
    }
    cx.source_context = Some(SourceContextDecl { mode, fields, site });
}
