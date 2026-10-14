//! Building compiler state from a reader's input: one construct per
//! declaration, already validated by binding, so only the rules the
//! dialect's declarations cannot state are checked here.

use kdlith_model::Value;
use kdlith_runtime::Repeated;

use crate::bindings::{
    BindingCodec, CaseRule, ChildRule, Collision, ContextPolicy, DependentBinding, DuplicateKeys,
    Effect, EffectGroup, EffectTarget, FieldBinding, NodeDecl, Placement, Role, SetFrom, Source,
};
use crate::compile::Cx;
use crate::compile::input::{
    ChildRuleInput, ConstraintInput, Construct, DeferralInput, DefinitionInput, DependentInput,
    DocumentRuleInput, EffectGroupInput, EffectInput, FieldBindingInput, FieldInput, ImportInput,
    Located, MemberInput, NodeInput, SlotInput, SourceInput, TypeInput,
};
use crate::compile::types::{declare, field_name, format_rule, validator_use};
use crate::decode::found;
use crate::definition::{Deferral, DocumentRule, ImportDecl, SlotDecl};
use crate::diag::{INVALID_BINDING, NOT_INSTALLED, Site, duplicate, duplicate_key};
use crate::names::Sym;
use crate::syntax::parse_u64;
use crate::types::{
    CaptureDef, CodecDecl, Constraint, EnumMember, EnumType, FieldDef, MapType, ObjectType,
    Sensitivity, StringType, TypeKind, UnionType, Variant,
};

/// Read every construct of every document, in stream order.
pub(crate) fn all(cx: &mut Cx<'_>, input: &DefinitionInput) {
    for (doc, document) in input.documents.iter().enumerate() {
        for located in &document.constructs {
            construct(cx, doc, located);
        }
    }
}

/// Collect a dialect's deferral markers before anything else is read,
/// so every construct is checked against the full deferral set.
pub(crate) fn deferrals(cx: &mut Cx<'_>, input: &DefinitionInput) {
    for (doc, document) in input.documents.iter().enumerate() {
        for located in &document.constructs {
            if let Construct::Deferral(input) = &located.item {
                deferral(cx, Site::new(doc, located.span), input);
            }
        }
    }
}

fn construct(cx: &mut Cx<'_>, doc: usize, located: &Located<Construct>) {
    let site = Site::new(doc, located.span);
    match &located.item {
        Construct::Type(ty) => type_decl(cx, site, ty),
        Construct::Node(input) => node(cx, site, input),
        Construct::Slot(input) => slot(cx, site, input),
        Construct::Rule(input) => rule(cx, site, input),
        Construct::Import(input) => import(cx, site, input),
        // Collected by `deferrals` before `all` runs.
        Construct::Deferral(_) => {}
    }
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
            for constraint in &constraints {
                let named: Vec<Sym> = match constraint {
                    Constraint::AllOrNone(list) => list.clone(),
                    Constraint::Requires { field, required } => vec![*field, *required],
                };
                for sym in named
                    .iter()
                    .filter(|sym| !built.iter().any(|f| f.name == **sym))
                {
                    let label = format!("object {name}");
                    let spelled = cx.names.resolve(*sym).to_owned();
                    cx.malformed(
                        site,
                        &label,
                        &format!("constraint names undeclared field {spelled}"),
                    );
                }
            }
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
    Some(FieldDef {
        name: cx.names.intern(&field.name),
        ty,
        description: field.description.clone(),
        documented_default: field.documented_default.clone(),
        fallback: field.fallback.clone(),
        sensitivity,
    })
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
    }
}

fn node(cx: &mut Cx<'_>, site: Site, input: &NodeInput) {
    let name = input.name.clone();
    let label = format!("node {name}");
    let role = input.role.as_deref().and_then(Role::from_name);
    let typed = input.ty.is_some();
    let ty = input.ty.as_deref().map(|ty| cx.names.intern(ty));

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
    cx.nodes.push(NodeDecl {
        name: cx.names.intern(&name),
        role,
        ty,
        placement: if under.is_empty() {
            Placement::Root
        } else {
            Placement::Under(under)
        },
        fields,
        children,
        effects,
        groups,
        value,
        cases,
        otherwise,
        site,
    });
}

/// A node declaration's dependent payload binding, with its codec
/// checked for installation.
fn dependent(cx: &mut Cx<'_>, site: Site, input: &DependentInput) -> Option<DependentBinding> {
    let codec = match BindingCodec::from_name(&input.codec) {
        Some(parsed) => parsed,
        None => {
            cx.not_member(site, "value.codec", "BindingCodec", &input.codec);
            return None;
        }
    };
    if !cx.extensions.has_binding_codec(codec.name()) {
        cx.diags
            .error(NOT_INSTALLED, site, &["binding codec", codec.name()]);
        return None;
    }
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
        SourceInput::Child { name, sources } => Some(Source::Child {
            name: cx.names.intern(name),
            sources: nested(cx, site, label, sources),
        }),
        SourceInput::Children { name, sources } => Some(Source::Children {
            name: cx.names.intern(name),
            sources: nested(cx, site, label, sources),
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
    if let (Some(min), Some(max)) = (input.min, input.max)
        && min > max
    {
        let reason = format!(
            "children {} has min-occurs {min} above max-occurs {max}",
            input.name
        );
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    if input.max == Some(0) {
        let reason = format!("children {} permits no occurrence", input.name);
        cx.diags.error(INVALID_BINDING, site, &[owner, &reason]);
        return None;
    }
    Some(ChildRule {
        name: cx.names.intern(&input.name),
        min: input.min,
        max: input.max,
        site,
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
        EffectInput::Set { slot, field, value } => {
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
    cx.slots.push(SlotDecl {
        name: sym,
        ty,
        namespace,
        description: input.description.clone(),
        site,
    });
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
    cx.imports.push(ImportDecl {
        path: input.path.clone(),
        project: input.project.iter().map(|n| cx.names.intern(n)).collect(),
        repeated,
        site,
    });
}

fn deferral(cx: &mut Cx<'_>, site: Site, input: &DeferralInput) {
    if let Some(first) = cx
        .deferrals
        .iter()
        .find(|d| d.mechanism == input.mechanism)
        .map(|d| d.site)
    {
        duplicate(
            &mut cx.diags,
            cx.kind,
            (site, first),
            "deferral",
            &input.mechanism,
        );
        return;
    }
    cx.deferrals.push(Deferral {
        mechanism: input.mechanism.clone(),
        note: input.note.clone(),
        site,
    });
}
