//! A canonical rendering of what a definition defines, every name spelled
//! out and assembly details (import requests, paths) left out: two
//! definitions are the same definition exactly when summaries are equal.

use std::fmt::Write as _;

use crate::bindings::{
    Condition, ContextPolicy, Effect, EffectTarget, FieldBinding, NodeDecl, Placement, Role,
    SetFrom, Source,
};
use crate::definition::{Definition, MergeDuplicates, SlotDuplicates};
use crate::names::{Names, Sym};
use crate::types::{Constraint, TypeExpr, TypeKind};

impl Definition {
    /// The canonical summary; see the module documentation.
    pub fn summary(&self) -> String {
        let n = &self.names;
        let mut out = String::new();
        let version = self.version.as_deref().unwrap_or("-");
        let _ = writeln!(
            out,
            "{} {} version {version}",
            self.kind.root_name(),
            self.name
        );
        for (_, decl) in self.types.iter().filter(|(_, d)| d.site.is_some()) {
            let _ = writeln!(out, "type {} {}", n.resolve(decl.name), kind(n, &decl.kind));
        }
        for (_, node) in self.bindings.iter() {
            let _ = writeln!(out, "{}", node_line(n, node));
        }
        for slot in &self.slots {
            let namespace = slot.namespace.map_or("-", |ns| n.resolve(ns));
            let accumulate = slot
                .accumulate
                .map_or_else(|| "-".to_owned(), |op| op.name().to_owned());
            // A policy at its fail-closed default adds nothing, so every
            // digest of a definition declaring none is unchanged.
            let mut policy = String::new();
            if slot.merge_duplicates != MergeDuplicates::Reject {
                let _ = write!(policy, " merge-duplicates={}", slot.merge_duplicates.name());
            }
            if slot.duplicates != SlotDuplicates::Reject {
                let _ = write!(policy, " duplicates={}", slot.duplicates.name());
            }
            if !slot.layers.is_empty() {
                let _ = write!(policy, " layers={}", list(n, &slot.layers));
            }
            let _ = writeln!(
                out,
                "slot {} {} namespace={namespace} accumulate={accumulate}{policy}",
                n.resolve(slot.name),
                expr(n, &slot.ty)
            );
        }
        for profile in &self.profiles {
            let roots = profile
                .roots
                .as_ref()
                .map_or_else(|| "-".to_owned(), |roots| list(n, roots));
            let grammar = profile.grammar.map_or("-", |g| n.resolve(g));
            // The extended surface is written only when declared, so a
            // profile without it keeps its summary line and digest.
            let mut extended = String::new();
            if let Some(parent) = profile.extends {
                let _ = write!(extended, " extends={}", n.resolve(parent));
            }
            if profile.default {
                extended.push_str(" default");
            }
            if !profile.rules.is_empty() {
                let rules: Vec<String> = profile
                    .rules
                    .iter()
                    .map(|rule| format!("{} {:?}", rule.kind.name(), rule.text))
                    .collect();
                let _ = write!(extended, " match=[{}]", rules.join(","));
            }
            let _ = writeln!(
                out,
                "profile {} roots={roots} grammar={grammar}{extended}",
                n.resolve(profile.name)
            );
        }
        for grammar in &self.discoveries {
            let anchors: Vec<String> = grammar
                .anchors
                .iter()
                .map(|anchor| format!("{}={}", n.resolve(anchor.name), n.resolve(anchor.provider)))
                .collect();
            let patterns = crate::discovery::pattern_names(&grammar.patterns).join(",");
            let _ = writeln!(
                out,
                "discovery {} anchors [{}] patterns [{}] allow-empty={}",
                n.resolve(grammar.name),
                anchors.join(","),
                patterns,
                grammar.allow_empty
            );
        }
        for reference in &self.references {
            let codec = reference.codec.map_or("-", |c| n.resolve(c));
            let lookup = reference.lookup.map_or("-", |l| n.resolve(l));
            let qualifiers: Vec<String> = reference
                .qualifiers
                .iter()
                .map(|qualifier| {
                    format!(
                        "{}:{} required={}",
                        n.resolve(qualifier.name),
                        expr(n, &qualifier.ty),
                        qualifier.required
                    )
                })
                .collect();
            let _ = writeln!(
                out,
                "reference {} target={} codec={codec} lookup={lookup} qualification={} qualifiers [{}]",
                n.resolve(reference.name),
                n.resolve(reference.target),
                reference.qualification.name(),
                qualifiers.join(",")
            );
        }
        for annotation in &self.annotations {
            let _ = writeln!(
                out,
                "annotation {} {} required={}",
                n.resolve(annotation.name),
                expr(n, &annotation.ty),
                annotation.required
            );
        }
        if let Some(policy) = &self.resolution {
            let _ = writeln!(
                out,
                "resolution {} layers [{}] selection={} duplicates={} missing={}",
                n.resolve(policy.resolver),
                policy.layers.join(","),
                policy.selection.name(),
                policy.duplicates.name(),
                policy.missing.name()
            );
        }
        for provider in &self.providers {
            let ttl = provider
                .ttl
                .map_or_else(|| "-".to_owned(), |t| t.to_string());
            let _ = writeln!(
                out,
                "provider {} ttl={ttl} command [{}]",
                provider.id,
                provider.command.join(",")
            );
        }
        if let Some(source) = &self.source_context {
            let fields: Vec<String> = source
                .fields
                .iter()
                .map(|field| {
                    format!(
                        "{}:{} [{}]",
                        n.resolve(field.name),
                        expr(n, &field.ty),
                        field.sources.join(",")
                    )
                })
                .collect();
            let _ = writeln!(
                out,
                "source-context mode={} fields [{}]",
                source.mode.name(),
                fields.join("; ")
            );
        }
        if let Some(rule) = &self.document_rule {
            let first = rule
                .first
                .as_ref()
                .map_or_else(|| "-".to_owned(), |f| list(n, f));
            let _ = writeln!(
                out,
                "document first={first} following={}",
                list(n, &rule.following)
            );
        }
        out
    }

    /// A 64-bit FNV-1a digest of the summary, printed as 16 hex digits.
    pub fn digest(&self) -> String {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in self.summary().bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        format!("{hash:016x}")
    }
}

fn list(n: &Names, syms: &[Sym]) -> String {
    let spelled: Vec<&str> = syms.iter().map(|s| n.resolve(*s)).collect();
    format!("[{}]", spelled.join(","))
}

pub(crate) fn expr(n: &Names, ty: &TypeExpr) -> String {
    ty.render(n)
}

fn kind(n: &Names, kind: &TypeKind) -> String {
    match kind {
        TypeKind::Builtin(builtin) => format!("builtin {}", builtin.name()),
        TypeKind::String(string) => format!(
            "string format={:?} pattern={:?} validator={:?} min-length={:?}",
            string.format, string.pattern, string.validator, string.min_length
        ),
        TypeKind::Enum(enumeration) => {
            let members: Vec<String> = enumeration
                .members
                .iter()
                .map(|m| format!("{}={:?}", m.name, m.description))
                .collect();
            format!("enum [{}]", members.join(","))
        }
        TypeKind::Object(object) => {
            let fields: Vec<String> = object
                .fields
                .iter()
                .map(|f| {
                    format!(
                        "{}:{} {:?} default={:?} fallback={:?} sensitivity={:?}",
                        n.resolve(f.name),
                        expr(n, &f.ty),
                        f.description,
                        f.documented_default,
                        f.fallback,
                        f.sensitivity.map(|s| s.name())
                    )
                })
                .collect();
            let constraints: Vec<String> = object
                .constraints
                .iter()
                .map(|c| match c {
                    Constraint::AllOrNone(fields) => format!("all-or-none{}", list(n, fields)),
                    Constraint::Requires { field, required } => {
                        format!("requires {} {}", n.resolve(*field), n.resolve(*required))
                    }
                    Constraint::Bound { field, min, max } => {
                        let end = |key: &str, end: &Option<u64>| {
                            end.map(|end| format!(" {key} {end}")).unwrap_or_default()
                        };
                        let (min, max) = (end("min", min), end("max", max));
                        format!("bound {}{min}{max}", n.resolve(*field))
                    }
                    Constraint::AtMostOne(fields) => format!("at-most-one{}", list(n, fields)),
                    Constraint::ExactlyOneField(fields) => {
                        format!("exactly-one-field{}", list(n, fields))
                    }
                })
                .collect();
            format!(
                "object [{}] constraints [{}]",
                fields.join("; "),
                constraints.join("; ")
            )
        }
        TypeKind::Union(union) => {
            let variants: Vec<String> = union
                .variants
                .iter()
                .map(|v| match &v.prefix {
                    Some(prefix) => format!(
                        "{}:{} prefix={prefix:?}",
                        n.resolve(v.discriminator),
                        expr(n, &v.ty)
                    ),
                    None => format!("{}:{}", n.resolve(v.discriminator), expr(n, &v.ty)),
                })
                .collect();
            format!("union [{}]", variants.join(","))
        }
        TypeKind::Map(map) => format!("map {} {}", expr(n, &map.key), expr(n, &map.value)),
        TypeKind::Codec(codec) => {
            let captures: Vec<String> = codec
                .captures
                .iter()
                .map(|c| format!("{}:{}", c.name, expr(n, &c.ty)))
                .collect();
            format!(
                "codec output={} pattern={:?} captures [{}] namespace={:?}",
                expr(n, &codec.output),
                codec.pattern,
                captures.join(","),
                codec.namespace.map(|ns| n.resolve(ns))
            )
        }
    }
}

fn node_line(n: &Names, node: &NodeDecl) -> String {
    let role = node.role.map_or("-", Role::name);
    let ty = node.ty.map_or("-", |ty| n.resolve(ty));
    let placement = match &node.placement {
        Placement::Root => "root".to_owned(),
        Placement::Under(parents) => list(n, parents),
        Placement::RootAndUnder(parents) => format!("root+{}", list(n, parents)),
    };
    let fields: Vec<String> = node.fields.iter().map(|f| field(n, f)).collect();
    let children: Vec<String> = node
        .children
        .iter()
        .map(|c| {
            let mut rule = format!("{}:{:?}..{:?}", n.resolve(c.name), c.min, c.max);
            // Branches are written only when declared, so every rule
            // carrying none keeps its summary line and digest.
            for branch in &c.branches {
                let _ = write!(
                    rule,
                    " when {} {:?}..{:?}",
                    condition(n, &branch.condition),
                    branch.min,
                    branch.max
                );
            }
            rule
        })
        .collect();
    let effects: Vec<String> = node
        .effects
        .iter()
        .map(|group| {
            let target = match group.target {
                EffectTarget::Following => "following",
                EffectTarget::Children => "children",
            };
            let steps: Vec<String> = group.effects.iter().map(|e| effect(n, e)).collect();
            format!("{target}[{}]", steps.join(","))
        })
        .collect();
    let groups: Vec<String> = node.groups.iter().map(|g| list(n, g)).collect();
    let value = node.value.as_ref().map_or_else(
        || "-".to_owned(),
        |value| {
            format!(
                "against {} {} codec {}",
                n.resolve(value.target),
                n.resolve(value.member),
                value.codec.name()
            )
        },
    );
    let cases: Vec<String> = node
        .cases
        .iter()
        .map(|case| {
            format!(
                "{}={:?} {}",
                n.resolve(case.property),
                case.equals,
                expr(n, &case.ty)
            )
        })
        .collect();
    let otherwise = node
        .otherwise
        .as_ref()
        .map_or_else(|| "-".to_owned(), |ty| expr(n, ty));
    let profile = node.profile.map_or("-", |p| n.resolve(p));
    let descendants: Vec<String> = node
        .descendants
        .iter()
        .map(|rule| format!("{}:{:?}..{:?}", n.resolve(rule.name), rule.min, rule.max))
        .collect();
    let sequence = node.sequence.as_ref().map_or_else(
        || "-".to_owned(),
        |sequence| {
            let steps: Vec<String> = sequence
                .steps
                .iter()
                .map(|step| {
                    let alternatives: Vec<&str> =
                        step.alternatives.iter().map(|a| n.resolve(*a)).collect();
                    format!("{}..{} {}", step.min, step.max, alternatives.join("|"))
                })
                .collect();
            steps.join(";")
        },
    );
    let identity = node.identity.as_ref().map_or_else(
        || "-".to_owned(),
        |identity| {
            let fields: Vec<&str> = identity.fields.iter().map(|f| n.resolve(*f)).collect();
            format!(
                "{} qualified-name [{}]",
                n.resolve(identity.domain),
                fields.join(",")
            )
        },
    );
    // Metadata is written only when declared, so every node carrying
    // none keeps its summary line and digest.
    let mut metadata = String::new();
    if let Some(description) = &node.description {
        let _ = write!(metadata, " description={description:?}");
    }
    if !node.annotations.is_empty() {
        let facts: Vec<String> = node
            .annotations
            .iter()
            .map(|fact| format!("{}={:?}", n.resolve(fact.name), fact.value))
            .collect();
        let _ = write!(metadata, " annotations [{}]", facts.join(","));
    }
    format!(
        "node {} role={role} type={ty} under={placement} profile={profile} fields [{}] children [{}] effects [{}] groups [{}] descendants [{}] sequence {sequence} value {value} cases [{}] otherwise {otherwise} identity {identity}{metadata}",
        n.resolve(node.name),
        fields.join("; "),
        children.join(","),
        effects.join(","),
        groups.join(","),
        descendants.join(","),
        cases.join(";")
    )
}

fn condition(n: &Names, held: &Condition) -> String {
    let members = |members: &[Condition]| {
        let members: Vec<String> = members.iter().map(|m| condition(n, m)).collect();
        members.join(",")
    };
    match held {
        Condition::Equals { property, equals } => format!("{}={equals:?}", n.resolve(*property)),
        Condition::All(all) => format!("all({})", members(all)),
        Condition::Any(any) => format!("any({})", members(any)),
    }
}

fn field(n: &Names, field: &FieldBinding) -> String {
    let ty = field
        .ty
        .as_ref()
        .map_or_else(|| "-".to_owned(), |t| expr(n, t));
    let sources: Vec<String> = field.sources.iter().map(|s| source(n, s)).collect();
    format!(
        "{}:{ty} [{}] collision={:?} duplicate-keys={:?}",
        n.resolve(field.name),
        sources.join(","),
        field.collision,
        field.duplicate_keys
    )
}

fn source(n: &Names, source: &Source) -> String {
    let nested = |sources: &[Source]| {
        let inner: Vec<String> = sources.iter().map(|s| self::source(n, s)).collect();
        inner.join(",")
    };
    match source {
        Source::Argument(index) => format!("argument {index}"),
        Source::Args => "args".to_owned(),
        Source::Property(name) => format!("property {}", n.resolve(*name)),
        // A source without the compact spelling summarizes exactly as
        // before the spelling existed, so no such digest moves.
        Source::Child {
            name,
            sources,
            compact,
        } => {
            let compact = if *compact { " compact" } else { "" };
            format!("child {}{compact}{{{}}}", n.resolve(*name), nested(sources))
        }
        Source::Children {
            name,
            sources,
            compact,
        } => {
            let compact = if *compact { " compact" } else { "" };
            format!(
                "children {}{compact}{{{}}}",
                n.resolve(*name),
                nested(sources)
            )
        }
        Source::Entries { node, key, value } => format!(
            "entries {} key={key} value={}",
            n.resolve(*node),
            n.resolve(*value)
        ),
        Source::Expand { target, member } => {
            format!(
                "expand {} member={}",
                n.resolve(*target),
                n.resolve(*member)
            )
        }
        Source::Context { slot, policy } => {
            let policy = match policy {
                None => "-",
                Some(ContextPolicy::Fallback) => "fallback",
                Some(ContextPolicy::Fixed) => "fixed",
            };
            format!("context {} policy={policy}", n.resolve(*slot))
        }
    }
}

fn effect(n: &Names, effect: &Effect) -> String {
    match effect {
        Effect::Set { slot, from, layer } => {
            let set = match from {
                SetFrom::Field(field) => {
                    format!("set {} field={}", n.resolve(*slot), n.resolve(*field))
                }
                SetFrom::Value(value) => format!("set {} value={value:?}", n.resolve(*slot)),
            };
            // An unlayered set summarizes exactly as before its policy
            // existed, so every digest without a layer is unchanged.
            match layer {
                Some(layer) => format!("{set} layer={}", n.resolve(*layer)),
                None => set,
            }
        }
        Effect::Remove { slot } => format!("remove {}", n.resolve(*slot)),
        Effect::Clear { namespace } => format!("clear {}", n.resolve(*namespace)),
        Effect::Handler { name } => format!("handler {name}"),
    }
}
