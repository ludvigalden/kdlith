//! Expansion: resolving declaration-group includes. Complete checking
//! splices each one's entries at the recorded position; a cycle is
//! `KDLITH-SCH009`.

use crate::compile::Cx;
use crate::diag::{EXPANSION_CYCLE, Site, duplicate_key};
use crate::names::Sym;
use crate::types::{
    CaptureDef, EnumMember, ExpandedGroup, FieldDef, TypeDecl, TypeId, TypeKind, Variant,
};

/// One `include TARGET` read inside a type declaration, awaiting the
/// complete-checking pass that splices its entries.
#[derive(Clone, Debug)]
pub(crate) struct Pending {
    /// The type declaring the group the include splices into.
    pub owner: Sym,
    /// The declaration whose member group is spliced in.
    pub target: Sym,
    /// Where the include was written.
    pub site: Site,
    /// The position in the owner's member list the entries splice at.
    pub at: usize,
}

/// The entries of one keyed member group, keyed for collision checks.
struct Group {
    keys: Vec<String>,
    fields: Vec<FieldDef>,
    members: Vec<EnumMember>,
    variants: Vec<Variant>,
    captures: Vec<CaptureDef>,
    /// Which of the four lists holds the entries.
    of: Of,
}

impl Group {
    /// How many entries the group splices in.
    fn len(&self) -> usize {
        match self.of {
            Of::Fields => self.fields.len(),
            Of::Members => self.members.len(),
            Of::Variants => self.variants.len(),
            Of::Captures => self.captures.len(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Of {
    Fields,
    Members,
    Variants,
    Captures,
}

/// Resolve every pending include; the `expand` sources are validated by
/// linking, which sees the compiled bindings.
pub(crate) fn run(cx: &mut Cx<'_>) {
    let mut pending = std::mem::take(&mut cx.expansions);
    let total = pending.len();
    let mut applied = vec![false; total];
    for _ in 0..total {
        let mut progressed = false;
        for at in 0..total {
            if applied[at] || cx.types.lookup(pending[at].target).is_none() {
                continue;
            }
            // A target with its own unresolved include is not ready: its
            // member group is still changing. It either becomes ready in
            // a later round or the pass reports the cycle.
            let blocked = pending
                .iter()
                .enumerate()
                .any(|(other, p)| !applied[other] && p.owner == pending[at].target);
            if blocked {
                continue;
            }
            match group_of(cx, &pending[at]) {
                Some(group) => {
                    let count = group.len();
                    splice(cx, &pending[at], group);
                    // A splice shifts every later include of the same
                    // owner, whose recorded position still counts the
                    // members as written before it.
                    shift_later(&mut pending, &applied, at, count);
                    applied[at] = true;
                    progressed = true;
                }
                // The include was reported as never satisfiable.
                None => applied[at] = true,
            }
        }
        if !progressed {
            break;
        }
    }
    for (at, done) in applied.iter().enumerate() {
        if *done {
            continue;
        }
        report_stuck(cx, &pending, &applied, at);
    }
}

/// Move every unapplied include of the spliced one's owner that is
/// written after it (a later position, or the same position and a later
/// include) past the entries the splice inserted.
fn shift_later(pending: &mut [Pending], applied: &[bool], spliced: usize, count: usize) {
    let (owner, at) = (pending[spliced].owner, pending[spliced].at);
    for (other, item) in pending.iter_mut().enumerate() {
        if applied[other] || item.owner != owner || (item.at, other) <= (at, spliced) {
            continue;
        }
        item.at += count;
    }
}

/// The named member group of the pending target, when the target is
/// declared and ready; `None` reports why it never will be.
fn group_of(cx: &mut Cx<'_>, pending: &Pending) -> Option<Group> {
    let target = cx.names.resolve(pending.target).to_owned();
    let id = cx.types.lookup(pending.target)?;
    // A target with its own unresolved include is not ready: its member
    // group is still changing. It either becomes ready in a later round
    // or the pass reports the cycle.
    let decl = cx.types.get(id)?;
    let group = match &decl.kind {
        TypeKind::Object(object) => Group {
            keys: object
                .fields
                .iter()
                .map(|f| cx.names.resolve(f.name).to_owned())
                .collect(),
            fields: object.fields.clone(),
            members: Vec::new(),
            variants: Vec::new(),
            captures: Vec::new(),
            of: Of::Fields,
        },
        TypeKind::Enum(enumeration) => Group {
            keys: enumeration.members.iter().map(|m| m.name.clone()).collect(),
            fields: Vec::new(),
            members: enumeration.members.clone(),
            variants: Vec::new(),
            captures: Vec::new(),
            of: Of::Members,
        },
        TypeKind::Union(union) => Group {
            keys: union
                .variants
                .iter()
                .map(|v| cx.names.resolve(v.discriminator).to_owned())
                .collect(),
            fields: Vec::new(),
            members: Vec::new(),
            variants: union.variants.clone(),
            captures: Vec::new(),
            of: Of::Variants,
        },
        TypeKind::Codec(codec) => Group {
            keys: codec.captures.iter().map(|c| c.name.clone()).collect(),
            fields: Vec::new(),
            members: Vec::new(),
            variants: Vec::new(),
            captures: codec.captures.clone(),
            of: Of::Captures,
        },
        _ => {
            let label = format!("include {target}");
            let reason = "builtin types and map types declare no keyed member to include";
            cx.malformed(pending.site, &label, reason);
            return None;
        }
    };
    Some(group)
}

/// Splice one group's entries into their owner at the recorded position,
/// rejecting a key that collides with a locally declared one.
fn splice(cx: &mut Cx<'_>, pending: &Pending, group: Group) {
    let owner_id = match cx.types.lookup(pending.owner) {
        Some(id) => id,
        None => return,
    };
    let owner_name = cx.names.resolve(pending.owner).to_owned();
    let owner_site = cx.types.get(owner_id).and_then(|decl| decl.site);
    let owner_of = match cx.types.get(owner_id).map(|decl| &decl.kind) {
        Some(TypeKind::Object(_)) => Of::Fields,
        Some(TypeKind::Enum(_)) => Of::Members,
        Some(TypeKind::Union(_)) => Of::Variants,
        Some(TypeKind::Codec(_)) => Of::Captures,
        _ => {
            let label = format!("include of {owner_name}");
            let reason = "the declaring type has no keyed member to expand into";
            cx.malformed(pending.site, &label, reason);
            return;
        }
    };
    if owner_of != group.of {
        let label = format!("include of {owner_name}");
        let reason = "the included member group is of a different kind";
        cx.malformed(pending.site, &label, reason);
        return;
    }
    let existing = existing_keys(cx, owner_id);
    for key in &group.keys {
        if existing.contains(key) {
            let of = format!("the expanded {owner_name}");
            let earlier = owner_site.unwrap_or(pending.site);
            duplicate_key(&mut cx.diags, (pending.site, earlier), key, &of);
        }
    }
    let Some(mut decl) = cx.types.take_decl(owner_id) else {
        return;
    };
    let at = pending.at.min(member_len(&decl));
    match &mut decl.kind {
        TypeKind::Object(object) => {
            object.fields.splice(at..at, group.fields.clone());
        }
        TypeKind::Enum(enumeration) => {
            enumeration.members.splice(at..at, group.members.clone());
        }
        TypeKind::Union(union) => {
            union.variants.splice(at..at, group.variants.clone());
        }
        TypeKind::Codec(codec) => {
            codec.captures.splice(at..at, group.captures.clone());
        }
        _ => {}
    }
    cx.types.put_decl(owner_id, decl);
    let declared = cx
        .types
        .lookup(pending.target)
        .and_then(|id| cx.types.get(id))
        .and_then(|decl| decl.site);
    let group = ExpandedGroup {
        target: pending.target,
        declared,
        included: pending.site,
    };
    cx.types.record_expansion(owner_id, group);
}

fn existing_keys(cx: &Cx<'_>, id: TypeId) -> Vec<String> {
    let Some(decl) = cx.types.get(id) else {
        return Vec::new();
    };
    match &decl.kind {
        TypeKind::Object(object) => object
            .fields
            .iter()
            .map(|f| cx.names.resolve(f.name).to_owned())
            .collect(),
        TypeKind::Enum(enumeration) => enumeration.members.iter().map(|m| m.name.clone()).collect(),
        TypeKind::Union(union) => union
            .variants
            .iter()
            .map(|v| cx.names.resolve(v.discriminator).to_owned())
            .collect(),
        TypeKind::Codec(codec) => codec.captures.iter().map(|c| c.name.clone()).collect(),
        _ => Vec::new(),
    }
}

fn member_len(decl: &TypeDecl) -> usize {
    match &decl.kind {
        TypeKind::Object(object) => object.fields.len(),
        TypeKind::Enum(enumeration) => enumeration.members.len(),
        TypeKind::Union(union) => union.variants.len(),
        TypeKind::Codec(codec) => codec.captures.len(),
        _ => 0,
    }
}

/// A pending include that never resolved: its target is undeclared, or
/// every remaining include waits on another that waits on it.
fn report_stuck(cx: &mut Cx<'_>, pending: &[Pending], applied: &[bool], at: usize) {
    let stuck = &pending[at];
    let target = cx.names.resolve(stuck.target).to_owned();
    if cx.types.lookup(stuck.target).is_none() {
        let owner = cx.names.resolve(stuck.owner).to_owned();
        let spelled = format!("include target {target} of type {owner}");
        crate::compile::link::unresolved(cx, stuck.site, "type", &spelled);
        return;
    }
    let mut chain = vec![cx.names.resolve(stuck.owner).to_owned(), target.clone()];
    let mut cursor = stuck.target;
    for _ in 0..pending.len() {
        let next = pending
            .iter()
            .enumerate()
            .find(|(other, p)| !applied[*other] && p.owner == cursor)
            .map(|(_, p)| p.target);
        match next {
            Some(next) => {
                chain.push(cx.names.resolve(next).to_owned());
                if next == stuck.owner {
                    break;
                }
                cursor = next;
            }
            None => break,
        }
    }
    let cycle = chain.join(" -> ");
    cx.diags.error(EXPANSION_CYCLE, stuck.site, &[&cycle]);
}
