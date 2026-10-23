//! Contribution resolution: the preserved records, the declared policy,
//! and the routing surface that hands them to the registered resolver.
//! The engine never resolves contributions itself.

use std::collections::BTreeMap;
use std::sync::Arc;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::ResolvedContribution;
use kdlith_runtime::{
    ContributionPolicy, ContributionRecord, ContributionResolver, MergeShape, ObjectShape,
};

use crate::bindings::Role;
use crate::definition::{Definition, ResolutionPolicyDecl};
use crate::diag::record;
use crate::types::{FieldDef, TypeCatalog, TypeExpr, TypeId, TypeKind};

/// A registered contribution resolver, shared cheaply across readings.
pub type SharedContributionResolver = Arc<dyn ContributionResolver>;

/// Why a resolution run could not proceed. Every failure is loud: an
/// uninstalled resolver is rejected at compile time already, and a
/// resolver that refuses its records reports its own reason.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolutionError {
    /// The schema declares no resolution policy, so there is nothing to
    /// route the records to.
    NoPolicy,
    /// The registered resolver is not the one the policy names.
    ResolverMismatch { declared: String, installed: String },
    /// The resolver failed; its reason, verbatim.
    Failed(String),
}

impl ResolutionError {
    /// The registered diagnostic code the failure reports under.
    pub fn code(&self) -> &'static str {
        match self {
            ResolutionError::NoPolicy | ResolutionError::ResolverMismatch { .. } => "KDLITH-DLC006",
            ResolutionError::Failed(_) => "KDLITH-DLC010",
        }
    }

    pub fn message(&self) -> String {
        match self {
            ResolutionError::NoPolicy => "the schema declares no resolution policy".to_owned(),
            ResolutionError::ResolverMismatch {
                declared,
                installed,
            } => format!("the policy names resolver {declared} but {installed} is registered"),
            ResolutionError::Failed(reason) => reason.clone(),
        }
    }
}

/// Route `records` to `resolver` under the definition's declared
/// policy. Fails closed when the schema declares no policy or the
/// registered resolver is not the one the policy names.
pub fn resolve(
    definition: &Definition,
    resolver: &dyn ContributionResolver,
    records: &[ContributionRecord],
) -> Result<Vec<ResolvedContribution>, ResolutionError> {
    let policy = definition
        .resolution
        .as_ref()
        .ok_or(ResolutionError::NoPolicy)?;
    let declared = definition.names.resolve(policy.resolver).to_owned();
    let installed = resolver.name().to_owned();
    if declared != installed {
        return Err(ResolutionError::ResolverMismatch {
            declared,
            installed,
        });
    }
    resolver
        .resolve(&policy_of(definition, policy), records)
        .map_err(ResolutionError::Failed)
}

/// The descriptors a resolver receives for a compiled policy. Records
/// carry no contribution type, so the root shape is keyed by field name
/// across every contribution type, the first declaring a name decides.
fn policy_of(definition: &Definition, policy: &ResolutionPolicyDecl) -> ContributionPolicy {
    let mut merges = BTreeMap::new();
    let mut shapes = Shapes {
        definition,
        objects: vec![ObjectShape::default()],
        index: BTreeMap::new(),
    };
    let mut root: BTreeMap<String, MergeShape> = BTreeMap::new();
    for (_, decl) in definition.bindings.iter() {
        if decl.role != Some(Role::Contribution) {
            continue;
        }
        let Some(ty) = decl.ty.and_then(|name| definition.types.lookup(name)) else {
            continue;
        };
        let Some(TypeKind::Object(object)) = definition.types.get(ty).map(|ty| &ty.kind) else {
            continue;
        };
        for field in &object.fields {
            let name = definition.names.resolve(field.name).to_owned();
            if let Some(merge) = field.merge {
                merges.entry(name.clone()).or_insert(merge);
            }
            if let std::collections::btree_map::Entry::Vacant(slot) = root.entry(name) {
                slot.insert(shapes.field(field));
            }
        }
    }
    let mut objects = shapes.objects;
    objects[0] = ObjectShape::new(root);
    ContributionPolicy::new(
        policy.layers.clone(),
        policy.selection,
        policy.duplicates,
        policy.missing,
        merges,
    )
    .with_shapes(objects)
}

/// The merge shapes of the object types a contributed value reaches,
/// each object type described once, so a recursive type terminates.
struct Shapes<'d> {
    definition: &'d Definition,
    objects: Vec<ObjectShape>,
    index: BTreeMap<TypeId, usize>,
}

impl Shapes<'_> {
    /// A field's shape: a list declaring `merge=union` appends, and every
    /// other field merges by its declared type.
    fn field(&mut self, field: &FieldDef) -> MergeShape {
        if field.merge.is_some() && holds_list(&field.ty) {
            return MergeShape::Append;
        }
        self.expr(&field.ty)
    }

    /// Objects merge field by field and keyed collections key by key;
    /// scalars, lists, references, and union values are replaced whole,
    /// since their parts carry no key to merge by.
    fn expr(&mut self, ty: &TypeExpr) -> MergeShape {
        match ty {
            TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) => self.expr(inner),
            TypeExpr::Map(_, value) | TypeExpr::OrderedMap(_, value) => {
                MergeShape::Keys(Box::new(self.expr(value)))
            }
            TypeExpr::Named(name) => self.named(*name),
            _ => MergeShape::Replace,
        }
    }

    fn named(&mut self, name: crate::names::Sym) -> MergeShape {
        let types: &TypeCatalog = &self.definition.types;
        let Some(id) = types.lookup(name) else {
            return MergeShape::Replace;
        };
        if let Some(at) = self.index.get(&id) {
            return MergeShape::Fields(*at);
        }
        let Some(decl) = types.get(id) else {
            return MergeShape::Replace;
        };
        match &decl.kind {
            TypeKind::Object(object) => {
                let at = self.objects.len();
                self.objects.push(ObjectShape::default());
                self.index.insert(id, at);
                let mut fields = BTreeMap::new();
                for field in &object.fields {
                    let key = self.definition.names.resolve(field.name).to_owned();
                    let shape = self.field(field);
                    fields.insert(key, shape);
                }
                self.objects[at] = ObjectShape::new(fields);
                MergeShape::Fields(at)
            }
            TypeKind::Map(map) => {
                let value = map.value.clone();
                MergeShape::Keys(Box::new(self.expr(&value)))
            }
            _ => MergeShape::Replace,
        }
    }
}

/// Whether a type holds a list, through `optional` and `nullable`.
fn holds_list(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::List(_) => true,
        TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) => holds_list(inner),
        _ => false,
    }
}

/// The diagnostic a resolution run that could not complete reports.
pub(crate) fn failure_record(error: &ResolutionError, origin: &str) -> DiagnosticRecord {
    match error {
        ResolutionError::Failed(reason) => record(error.code(), &[reason], origin),
        ResolutionError::ResolverMismatch { declared, .. } => {
            record(error.code(), &["resolver", declared], origin)
        }
        _ => record(
            error.code(),
            &["resolution policy", "of the schema"],
            origin,
        )
        .with_message(error.message()),
    }
}
