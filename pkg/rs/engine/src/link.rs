//! The link stage: identity indexing, reference binding, lookup policy,
//! and linking over the symbol catalog's declared surface and a
//! reading's items.
//!
//! A reading is linked from what its item stream leaves behind, never
//! from the items themselves: each declaration leaves one compact index
//! entry, each reference one bound reference, and nothing else of an
//! item is kept. The index answers a lookup through a hash of the
//! identity the reference determines, so linking costs time
//! proportional to the reading, not to its square.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use kdlith_diagnostic::{DiagnosticRecord, Label};
use kdlith_model::{
    BoundDeclaration, Field, Identity, Item, Location, NodeId, PendingBinding, QualifiedName,
    Reference, SourceId, Value,
};

use crate::bindings::{BindingCodec, NodeDecl};
use crate::decode::{Decoder, found};
use crate::definition::{Definition, OpenWorldDecl, Qualification, ReferenceDecl};
use crate::diag::record;
use crate::names::Sym;
use crate::summary;
use crate::types::{Builtin, ObjectType, TypeExpr, TypeKind};

const UNRESOLVED: &str = "KDLITH-REF001";
const DUPLICATE: &str = "KDLITH-REF002";
const AMBIGUOUS: &str = "KDLITH-REF003";
const UNMET_QUALIFIER: &str = "KDLITH-REF004";
const PAYLOAD_TYPE: &str = "KDLITH-REF005";
const SIBLING_DISAGREEMENT: &str = "KDLITH-REF006";
const ORIGIN: &str = "data-document";
/// The codec name a path-typed payload's item carries: never a
/// reference, never validated at link.
pub(crate) const PATH_VALUE_CODEC: &str = "value-from-path";
/// The member a `from-kind` payload reads its choice set from when its
/// binding names none.
const CHOICES: &str = "choices";
/// No declaration: the end of a chain of index positions.
const NONE: u32 = u32::MAX;

/// The linkable surface of one reading, gathered from its item stream
/// as it streams: the identity index of its declarations, every
/// reference a declared reference type looks up, its pending typed
/// bindings, the path of every document, and the namespace each
/// namespaced-imported document belongs to. Items are read, never kept.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LinkInputs<'d> {
    definition: &'d Definition,
    plan: Plan<'d>,
    index: IdentityIndex,
    references: Vec<BoundReference>,
    /// Each record or declaration holding a bound reference in a field.
    holders: Vec<Holder>,
    pending: Vec<PendingBinding>,
    /// Each document's path as the reading shows it.
    pub documents: BTreeMap<SourceId, String>,
    /// The namespace of each document a namespaced import loaded.
    pub namespaces: BTreeMap<SourceId, String>,
    imports: ImportNamespaces,
    /// The node and field names bound references are held under, each
    /// allocated once.
    spellings: HashMap<Box<str>, Arc<str>>,
}

/// What linking reads off a definition, derived once per reading.
#[derive(Clone, Debug)]
struct Plan<'d> {
    /// The domains linking covers.
    linked: BTreeSet<&'d str>,
    /// The declaration fields linking reads after the stream: a required
    /// qualifier's, an open-world arm's member, and a dependent
    /// binding's member and choice set.
    kept: HashSet<&'d str>,
    /// Each domain's identity compositions, in declaration order.
    compositions: HashMap<&'d str, Vec<Arc<[String]>>>,
    /// The labels of a declaration whose composition is not known.
    unlabeled: Arc<[String]>,
}

impl<'d> Plan<'d> {
    fn new(definition: &'d Definition) -> Self {
        let names = &definition.names;
        let mut kept: HashSet<&str> = HashSet::new();
        for decl in definition.symbols.references() {
            for rule in &decl.qualifiers {
                if rule.required && rule.from_field.is_none() {
                    kept.insert(names.resolve(rule.name));
                }
            }
            if let Some(arm) = &decl.open_world {
                kept.insert(names.resolve(arm.system_member));
            }
        }
        for (_, node) in definition.bindings.iter() {
            if let Some(dependent) = &node.value {
                kept.insert(names.resolve(dependent.member));
                kept.insert(dependent.choices.map_or(CHOICES, |set| names.resolve(set)));
            }
        }
        let mut compositions: HashMap<&str, Vec<Arc<[String]>>> = HashMap::new();
        for decl in definition.symbols.declarations() {
            let labels: Arc<[String]> = decl
                .identity_fields
                .iter()
                .map(|field| names.resolve(*field).to_owned())
                .collect();
            compositions
                .entry(names.resolve(decl.domain))
                .or_default()
                .push(labels);
        }
        Self {
            linked: linked_domains(definition),
            kept,
            compositions,
            unlabeled: Arc::from(Vec::new()),
        }
    }

    /// The identity field each segment of a declaration's name came
    /// from: the composition of the declaring node whose identity fields
    /// the declaration binds to exactly those segments.
    fn labels(&self, declaration: &BoundDeclaration) -> Arc<[String]> {
        let segments = &declaration.identity.name.0;
        self.compositions
            .get(declaration.identity.domain.as_str())
            .and_then(|compositions| {
                compositions.iter().find(|labels| {
                    labels.len() == segments.len()
                        && labels.iter().zip(segments).all(|(label, segment)| {
                            field_text(&declaration.fields, label)
                                .is_some_and(|text| text == segment)
                        })
                })
            })
            .unwrap_or(&self.unlabeled)
            .clone()
    }
}

/// One record or declaration holding bound references in its fields:
/// its type, and where its references sit among the bound ones.
#[derive(Clone, Copy, Debug)]
struct Holder {
    ty: Option<Sym>,
    first: u32,
    end: u32,
}

impl<'d> LinkInputs<'d> {
    /// The inputs of one reading under `definition`, before any item.
    pub fn new(definition: &'d Definition) -> Self {
        Self {
            definition,
            plan: Plan::new(definition),
            index: IdentityIndex::default(),
            references: Vec::new(),
            holders: Vec::new(),
            pending: Vec::new(),
            documents: BTreeMap::new(),
            namespaces: BTreeMap::new(),
            imports: ImportNamespaces::default(),
            spellings: HashMap::new(),
        }
    }

    /// The definition the reading binds under.
    pub fn definition(&self) -> &'d Definition {
        self.definition
    }

    /// The reading's declarations by identity, as indexed so far.
    pub fn index(&self) -> &IdentityIndex {
        &self.index
    }

    /// Every reference a declared reference type looks up, in stream
    /// order.
    pub fn references(&self) -> &[BoundReference] {
        &self.references
    }

    /// Take one item of the reading, in stream order.
    pub fn observe(&mut self, item: &Item) {
        let namespace = self.imports.observe(item);
        match item {
            Item::Document(document) => {
                if let Some(namespace) = namespace {
                    self.namespaces.insert(document.source, namespace);
                }
                self.documents
                    .insert(document.source, document.path.clone());
            }
            Item::Declaration(declaration) => {
                self.declare(declaration);
                self.bind(item);
            }
            Item::Record(_) | Item::Contribution(_) => self.bind(item),
            Item::PendingBinding(pending) => self.pending.push(pending.clone()),
            _ => {}
        }
    }

    /// Stage one, identity construction: index one declaration by its
    /// composed identity. Only domains an identity block, a declared
    /// reference type, or a dependent binding's target names are
    /// indexed. A declaration from a document a namespaced import loaded
    /// registers under that namespace, which gates every other
    /// document's view of it. Two declarations composing one identity in
    /// one domain and namespace are a duplicate; the first stays indexed.
    fn declare(&mut self, declaration: &BoundDeclaration) {
        let identity = &declaration.identity;
        if identity.name.0.is_empty() || !self.plan.linked.contains(identity.domain.as_str()) {
            return;
        }
        let kept = &self.plan.kept;
        self.index.insert(IndexedDeclaration {
            node: declaration.node,
            identity: identity.clone(),
            namespace: self.namespaces.get(&declaration.node.source).cloned(),
            labels: self.plan.labels(declaration),
            fields: declaration
                .fields
                .iter()
                .filter(|field| kept.contains(field.name.as_str()))
                .cloned()
                .collect(),
            site: declaration.site,
            ty: self.definition.names.get(&declaration.type_name),
        });
    }

    /// Stage two, reference binding: every reference value one record,
    /// declaration, or contribution holds, with the declared reference
    /// type looking it up. A reference whose domain no declared
    /// reference type looks up is left to its own mechanism.
    ///
    /// The reference type the holding field's declared type names binds
    /// the reference, so its qualification is the one the author chose.
    /// Where no field type names one (a contribution's target), the
    /// first reference type of the domain in declaration order whose
    /// qualifiers cover the reference's spelled qualifiers binds it.
    fn bind(&mut self, item: &Item) {
        let held = held_references(item);
        if held.is_empty() {
            return;
        }
        let definition = self.definition;
        let ty = item_type(definition, item);
        let object = ty.and_then(|ty| object_of(definition, ty));
        let first = self.references.len();
        let mut fielded = false;
        for held in held {
            let reference = held.reference;
            let named = held
                .field
                .and_then(|field| field_reference_type(definition, object?, field, held.key))
                .filter(|decl| domain_of(definition, decl) == reference.domain);
            let Some(decl) = named.or_else(|| domain_reference_type(definition, reference)) else {
                continue;
            };
            let Some(declared) = reference_position(definition, decl) else {
                continue;
            };
            fielded |= held.field.is_some();
            let node = held.node.map(|node| self.spelling(node));
            let field = held.field.map(|field| self.spelling(field));
            self.references.push(BoundReference {
                reference: reference.clone(),
                node,
                field,
                key: held.key,
                declared,
                holder: NONE,
            });
        }
        if fielded {
            let holder = position(self.holders.len());
            let end = self.references.len();
            for bound in &mut self.references[first..end] {
                if bound.field.is_some() {
                    bound.holder = holder;
                }
            }
            self.holders.push(Holder {
                ty,
                first: position(first),
                end: position(end),
            });
        }
    }

    /// One node or field name, allocated the first time it is seen.
    fn spelling(&mut self, name: &str) -> Arc<str> {
        if let Some(held) = self.spellings.get(name) {
            return held.clone();
        }
        let held: Arc<str> = Arc::from(name);
        self.spellings.insert(name.into(), held.clone());
        held
    }
}

/// An index position, saturating at the end-of-chain mark.
fn position(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(NONE)
}

/// Which import request loaded each document of a reading, followed
/// through its item stream.
///
/// A document belongs to the most recent import request naming its path
/// among the request's targets. Targets are paths as the reading shows
/// them, so the same file under two spellings is two targets; matching
/// the most recent request, never a global path set, keeps the
/// attribution exact for nested and repeated imports.
#[derive(Clone, Debug, Default)]
pub(crate) struct ImportNamespaces {
    /// Import requests whose targets may still load, oldest first: each
    /// with how many times it still names each target.
    pending: Vec<(Option<String>, HashMap<String, u32>)>,
}

impl ImportNamespaces {
    /// Take one item of the reading, in stream order: for a document an
    /// namespaced import loaded, that import's namespace.
    pub(crate) fn observe(&mut self, item: &Item) -> Option<String> {
        match item {
            Item::Document(document) => {
                let at = self.pending.iter().rposition(|(_, targets)| {
                    targets.get(&document.path).is_some_and(|left| *left > 0)
                })?;
                let (namespace, targets) = &mut self.pending[at];
                let namespace = namespace.clone();
                match targets.get_mut(&document.path) {
                    Some(left) if *left > 1 => *left -= 1,
                    _ => {
                        targets.remove(&document.path);
                    }
                }
                // A request every target of which has loaded names
                // nothing any more.
                while self.pending.last().is_some_and(|(_, left)| left.is_empty()) {
                    self.pending.pop();
                }
                namespace
            }
            Item::Import(request) if !request.targets.is_empty() => {
                let mut targets: HashMap<String, u32> = HashMap::new();
                for target in &request.targets {
                    *targets.entry(target.clone()).or_default() += 1;
                }
                self.pending.push((request.namespace.clone(), targets));
                None
            }
            _ => None,
        }
    }
}

/// One declaration registered in the identity index: what linking reads
/// of it once its item has streamed past.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct IndexedDeclaration {
    pub node: NodeId,
    pub identity: Identity,
    /// The namespace the declaration registers under, when an
    /// namespaced import loaded its document: other documents reach it
    /// through the namespace and not by its unqualified name.
    pub namespace: Option<String>,
    /// The identity field each name segment came from, outermost first;
    /// empty when the composition is not known. Shared by every
    /// declaration of one composition.
    pub labels: Arc<[String]>,
    /// The fields linking reads: a required qualifier's, an open-world
    /// arm's member, and a dependent binding's member and choice set.
    /// Every other field of the declaration stays with its item.
    pub fields: Vec<Field>,
    pub site: Option<Location>,
    /// The declaration's type, when the definition names it.
    ty: Option<Sym>,
}

/// A declaration whose composed identity an earlier one in its domain
/// and namespace already claims.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DuplicateDeclaration {
    pub declaration: Arc<IndexedDeclaration>,
    pub first: Arc<IndexedDeclaration>,
}

/// The reading's declarations by identity domain, each domain hashed by
/// composed name.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct IdentityIndex {
    /// Every indexed declaration, in stream order.
    declarations: Vec<Arc<IndexedDeclaration>>,
    /// For each declaration, the earlier one of its domain whose name
    /// hashes alike, or [`NONE`].
    earlier: Vec<u32>,
    /// For each declaration, its shape within its domain.
    shape: Vec<u32>,
    domains: BTreeMap<String, Domain>,
    duplicates: Vec<DuplicateDeclaration>,
}

/// One identity domain of the index.
#[derive(Clone, Debug, Default)]
struct Domain {
    /// The domain's declarations, in stream order.
    members: Vec<u32>,
    /// The latest declaration under each name hash; earlier ones chain
    /// from it.
    named: HashMap<u64, u32>,
    shapes: Vec<Shape>,
    /// Whether any declaration registers under a namespace.
    namespaced: bool,
}

/// The declarations of one domain composing their identity alike: one
/// label list and one arity, so one reference derives one pattern for
/// all of them.
#[derive(Clone, Debug)]
struct Shape {
    labels: Arc<[String]>,
    arity: usize,
    members: Vec<u32>,
    /// Per identity position, the shape's declarations by the hash of
    /// the segment they hold there; built the first time a lookup pins
    /// that position while leaving another open.
    positions: Vec<OnceCell<HashMap<u64, Vec<u32>>>>,
}

impl IdentityIndex {
    /// How many declarations the index holds, duplicates left out.
    pub fn len(&self) -> usize {
        self.declarations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }

    /// The declarations of one domain, in stream order.
    pub fn domain<'i>(
        &'i self,
        domain: &str,
    ) -> impl Iterator<Item = &'i Arc<IndexedDeclaration>> + 'i {
        self.domains
            .get(domain)
            .into_iter()
            .flat_map(|domain| &domain.members)
            .map(|at| &self.declarations[*at as usize])
    }

    /// The indexed domains, in name order.
    pub fn domains(&self) -> impl Iterator<Item = &str> {
        self.domains.keys().map(String::as_str)
    }

    /// The declarations that repeat an identity, in stream order.
    pub fn duplicates(&self) -> &[DuplicateDeclaration] {
        &self.duplicates
    }

    fn insert(&mut self, entry: IndexedDeclaration) {
        let name = &entry.identity.name.0;
        let hash = name_hash(name.iter().map(String::as_str));
        if !self.domains.contains_key(&entry.identity.domain) {
            self.domains
                .insert(entry.identity.domain.clone(), Domain::default());
        }
        let Some(domain) = self.domains.get_mut(&entry.identity.domain) else {
            return;
        };
        let mut at = domain.named.get(&hash).copied().unwrap_or(NONE);
        while at != NONE {
            let held = &self.declarations[at as usize];
            if held.namespace == entry.namespace && held.identity.name == entry.identity.name {
                self.duplicates.push(DuplicateDeclaration {
                    first: held.clone(),
                    declaration: Arc::new(entry),
                });
                return;
            }
            at = self.earlier[at as usize];
        }
        let here = position(self.declarations.len());
        self.earlier
            .push(domain.named.insert(hash, here).unwrap_or(NONE));
        domain.members.push(here);
        domain.namespaced |= entry.namespace.is_some();
        let arity = name.len();
        let shape = domain
            .shapes
            .iter()
            .position(|shape| shape.arity == arity && shape.labels == entry.labels)
            .unwrap_or_else(|| {
                domain.shapes.push(Shape {
                    labels: entry.labels.clone(),
                    arity,
                    members: Vec::new(),
                    positions: vec![OnceCell::new(); arity],
                });
                domain.shapes.len() - 1
            });
        domain.shapes[shape].members.push(here);
        self.shape.push(position(shape));
        self.declarations.push(Arc::new(entry));
    }

    /// The declarations of `domain` a reference's `segments` match, in
    /// stream order; under `namespace`, only those registered there.
    fn matching(
        &self,
        domain: &Domain,
        segments: &[String],
        bound: (&Reference, Qualification, Option<&SiblingFill>),
        namespace: Option<&str>,
    ) -> Vec<u32> {
        let (reference, policy, sibling) = bound;
        let mut found = Vec::new();
        for (at, shape) in domain.shapes.iter().enumerate() {
            let Some(slots) = pattern(
                &shape.labels,
                shape.arity,
                segments,
                reference,
                policy,
                sibling,
            ) else {
                continue;
            };
            let fits = |entry: u32| {
                let held = &self.declarations[entry as usize];
                self.shape[entry as usize] as usize == at
                    && namespace.is_none_or(|name| held.namespace.as_deref() == Some(name))
                    && agrees(&slots, &held.identity.name.0)
            };
            if slots.iter().all(Option::is_some) {
                // Every segment is determined: the name hashes to its
                // declarations.
                let hash = name_hash(slots.iter().flatten().copied());
                let mut entry = domain.named.get(&hash).copied().unwrap_or(NONE);
                while entry != NONE {
                    if fits(entry) {
                        found.push(entry);
                    }
                    entry = self.earlier[entry as usize];
                }
            } else if let Some(pinned) = slots.iter().rposition(Option::is_some) {
                // A segment stays open: the innermost determined one
                // narrows the shape to the declarations holding it.
                let held = shape.positions[pinned].get_or_init(|| {
                    let mut held: HashMap<u64, Vec<u32>> = HashMap::new();
                    for member in &shape.members {
                        let name = &self.declarations[*member as usize].identity.name.0;
                        held.entry(name_hash([name[pinned].as_str()]))
                            .or_default()
                            .push(*member);
                    }
                    held
                });
                let hash = name_hash(slots[pinned].iter().copied());
                let narrowed = held.get(&hash).map_or(&[][..], Vec::as_slice);
                found.extend(narrowed.iter().copied().filter(|entry| fits(*entry)));
            } else {
                found.extend(shape.members.iter().copied().filter(|entry| fits(*entry)));
            }
        }
        found.sort_unstable();
        found
    }

    /// Stage three, lookup policy: derive one reference's candidates
    /// under its reference type's qualification and, for a
    /// sibling-qualified reference, the contributions its resolved
    /// sibling fields supply.
    ///
    /// A spelled qualifier named after an identity field pins that
    /// field's segment; the written name segments fill the innermost
    /// remaining segments. `explicit` requires every segment spelled.
    /// The context policies fill the innermost still-missing segment
    /// from the namespace context captured where the reference bound,
    /// and leave any further missing segment open, so a partial name may
    /// match several declarations. `imported-or-context` also admits a
    /// namespace as the name's first segment. A declaration under a
    /// namespace is reachable by its unqualified name from its own
    /// document only; every other document reaches it through the
    /// namespace or not at all. A sibling-qualified reference fills its
    /// leading open segments from its resolved sibling fields'
    /// identities and leaves nothing open, so a segment nothing covers
    /// matches no declaration.
    fn lookup(
        &self,
        reference: &Reference,
        policy: Qualification,
        sibling: Option<&SiblingFill>,
    ) -> Found {
        let Some(domain) = self.domains.get(&reference.domain) else {
            return Found::default();
        };
        let bound = (reference, policy, sibling);
        let plain = self.matching(domain, &reference.name.0, bound, None);
        let prefixed = match reference.name.0.split_first() {
            Some((first, rest)) if domain.namespaced && !rest.is_empty() => {
                self.matching(domain, rest, bound, Some(first))
            }
            _ => Vec::new(),
        };
        let mut found = Found::default();
        let mut gated = BTreeSet::new();
        let (mut plains, mut prefixes) = (plain.iter().peekable(), prefixed.iter().peekable());
        loop {
            let (entry, plain, prefixed) = match (plains.peek(), prefixes.peek()) {
                (None, None) => break,
                (Some(a), Some(b)) if a == b => (**a, true, true),
                (Some(a), Some(b)) if a < b => (**a, true, false),
                (Some(a), None) => (**a, true, false),
                (_, Some(b)) => (**b, false, true),
            };
            if plain {
                plains.next();
            }
            if prefixed {
                prefixes.next();
            }
            let held = &self.declarations[entry as usize];
            match &held.namespace {
                None => found.candidates.push(entry),
                Some(_) if plain && own_document(held, reference) => {
                    found.candidates.push(entry);
                }
                Some(_) if prefixed && policy == Qualification::ImportedOrContext => {
                    found.candidates.push(entry);
                }
                Some(namespace) => {
                    gated.insert(namespace.clone());
                }
            }
        }
        found.gated = gated.into_iter().collect();
        found
    }

    /// The declarations of any domain composing exactly the one-segment
    /// name `value`.
    fn single<'i>(&'i self, value: &'i str) -> impl Iterator<Item = &'i IndexedDeclaration> + 'i {
        let hash = name_hash([value]);
        self.domains.values().flat_map(move |domain| {
            let first = domain.named.get(&hash).copied().unwrap_or(NONE);
            std::iter::successors((first != NONE).then_some(first), |at| {
                let earlier = self.earlier[*at as usize];
                (earlier != NONE).then_some(earlier)
            })
            .map(|at| &*self.declarations[at as usize])
            .filter(move |held| held.identity.name.0.as_slice() == [value])
        })
    }

    /// The one indexed declaration an unlinked pending reference names,
    /// by exact name and under the index's namespace carve-out. A name
    /// unseen, or held by more than one declaration, resolves to
    /// nothing.
    fn target(&self, reference: &Reference) -> Option<&IndexedDeclaration> {
        let domain = self.domains.get(&reference.domain)?;
        let hash = name_hash(reference.name.0.iter().map(String::as_str));
        let first = domain.named.get(&hash).copied().unwrap_or(NONE);
        let mut fits = std::iter::successors((first != NONE).then_some(first), |at| {
            let earlier = self.earlier[*at as usize];
            (earlier != NONE).then_some(earlier)
        })
        .map(|at| &*self.declarations[at as usize])
        .filter(|entry| {
            entry.identity.name == reference.name
                && (entry.namespace.is_none() || own_document(entry, reference))
        });
        let target = fits.next()?;
        fits.next().is_none().then_some(target)
    }
}

/// The hash a composed name is indexed under: FNV-1a over its segments,
/// each closed by a byte no segment's UTF-8 holds.
fn name_hash<'a>(segments: impl IntoIterator<Item = &'a str>) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut step = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for segment in segments {
        segment.bytes().for_each(&mut step);
        step(0xFF);
    }
    hash
}

/// Whether a declared name holds every segment a pattern determines.
fn agrees(slots: &[Option<&str>], declared: &[String]) -> bool {
    slots.len() == declared.len()
        && slots
            .iter()
            .zip(declared)
            .all(|(slot, segment)| slot.is_none_or(|supplied| supplied == segment))
}

/// The candidates one lookup derived, as index positions in stream
/// order, and the namespaces holding a declaration the reference would
/// match if namespaced visibility did not gate it, in name order.
#[derive(Clone, Debug, Default)]
struct Found {
    candidates: Vec<u32>,
    gated: Vec<String>,
}

/// A reference the catalog declares a reference type for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BoundReference {
    pub reference: Reference,
    /// The record or declaration node holding the reference; `None`
    /// for a contribution's target.
    pub node: Option<Arc<str>>,
    /// The record or declaration field holding the reference; `None`
    /// for a contribution's target.
    pub field: Option<Arc<str>>,
    /// The reference is the key of an entry of `field`'s map.
    pub key: bool,
    /// The declared reference type whose lookup domain the reference
    /// names, by its position among the definition's reference types.
    pub declared: usize,
    /// The record or declaration holding it in a field, or [`NONE`].
    holder: u32,
}

/// The type an item binds as, when the definition names it.
fn item_type(definition: &Definition, item: &Item) -> Option<Sym> {
    let type_name = match item {
        Item::Record(record) => &record.type_name,
        Item::Declaration(declaration) => &declaration.type_name,
        _ => return None,
    };
    definition.names.get(type_name)
}

/// The object type a name declares.
fn object_of(definition: &Definition, ty: Sym) -> Option<&ObjectType> {
    match &definition.types.get(definition.types.lookup(ty)?)?.kind {
        TypeKind::Object(object) => Some(object),
        _ => None,
    }
}

/// The reference type an object's field is declared with, through any
/// optional, nullable, or list wrapper; for a key, the key type of the
/// map the field holds.
fn field_reference_type<'d>(
    definition: &'d Definition,
    object: &'d ObjectType,
    field: &str,
    key: bool,
) -> Option<&'d ReferenceDecl> {
    let field = definition.names.get(field)?;
    let mut ty = &object.fields.iter().find(|def| def.name == field)?.ty;
    loop {
        match ty {
            TypeExpr::Named(name) => return definition.symbols.reference(*name),
            TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) | TypeExpr::List(inner) => {
                ty = inner;
            }
            TypeExpr::Map(inner, _) | TypeExpr::OrderedMap(inner, _) if key => ty = inner,
            _ => return None,
        }
    }
}

/// The reference type of a reference's domain when no field type names
/// one: the first in declaration order whose qualifiers cover the
/// spelled ones, else the first.
fn domain_reference_type<'d>(
    definition: &'d Definition,
    reference: &Reference,
) -> Option<&'d ReferenceDecl> {
    let mut types = definition
        .symbols
        .references()
        .iter()
        .filter(|decl| domain_of(definition, decl) == reference.domain);
    let first = types.clone().next();
    let covering = types.find(|decl| {
        reference.qualifiers.iter().all(|(name, _)| {
            decl.qualifiers
                .iter()
                .any(|q| definition.names.resolve(q.name) == name)
        })
    });
    covering.or(first)
}

/// Where a reference type's links are reported: the first declared
/// reference type of its name.
fn reference_position(definition: &Definition, decl: &ReferenceDecl) -> Option<usize> {
    definition
        .symbols
        .references()
        .iter()
        .position(|held| held.name == decl.name)
}

/// What a sibling-qualified reference derived from its record's sibling
/// fields: each contributing qualifier's identity-segment window, in
/// declaration order, and the qualified spelling the reference
/// attempted when the contributions and its spelling together
/// determined every segment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SiblingFill {
    /// One contributing qualifier's window, in declaration order.
    pub windows: Vec<SiblingWindow>,
    /// The qualified spelling the reference attempted, segments joined
    /// by `.`; `None` when the contributions left a segment open.
    pub attempt: Option<String>,
}

/// The leading identity segments one sibling field contributed, and
/// where they fill.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SiblingWindow {
    /// The identity position the contribution fills from.
    pub start: usize,
    /// The record field the contribution came from.
    pub field: String,
    /// The resolved identity of the reference the field holds.
    pub segments: Vec<String>,
}

/// A required qualifier the resolved declaration does not meet.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnmetQualifier {
    pub qualifier: String,
    /// The value the reference supplies, when it supplies one.
    pub spelled: Option<String>,
    /// The value the target declares, when it declares one.
    pub declared: Option<String>,
}

/// How one reference linked.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkOutcome {
    /// Exactly one candidate, meeting every required qualifier.
    Resolved,
    /// No candidate, but the reference type's open-world arm admits it:
    /// the declaration its outermost segment names is defined upstream,
    /// so the reference cannot resolve in any reading of this tree.
    /// Not a diagnostic.
    External,
    /// No candidate (`KDLITH-REF001`).
    Unresolved,
    /// More than one candidate (`KDLITH-REF003`).
    Ambiguous,
    /// One candidate failing a required qualifier (`KDLITH-REF004`).
    UnmetQualifier(Vec<UnmetQualifier>),
}

impl LinkOutcome {
    /// The registered code a failed outcome reports.
    pub fn code(&self) -> Option<&'static str> {
        match self {
            LinkOutcome::Resolved | LinkOutcome::External => None,
            LinkOutcome::Unresolved => Some(UNRESOLVED),
            LinkOutcome::Ambiguous => Some(AMBIGUOUS),
            LinkOutcome::UnmetQualifier(_) => Some(UNMET_QUALIFIER),
        }
    }
}

/// One reference, its candidates, and how it linked.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Link {
    pub reference: Reference,
    /// The record or declaration node holding the reference; `None`
    /// for a contribution's target.
    pub node: Option<Arc<str>>,
    /// The record or declaration field holding the reference; `None`
    /// for a contribution's target.
    pub field: Option<Arc<str>>,
    /// The reference is the key of an entry of `field`'s map.
    pub key: bool,
    /// The qualified spelling a sibling-qualified reference attempted,
    /// when its resolved sibling fields and its spelling together
    /// determined every segment; an unqualified failure names it.
    pub attempt: Option<String>,
    /// The declarations the lookup derived, in stream order, each
    /// shared with the index.
    pub candidates: Vec<Arc<IndexedDeclaration>>,
    pub gated: Vec<String>,
    pub outcome: LinkOutcome,
}

impl Link {
    /// The fully qualified identity the reference resolved to — the
    /// one declaration's composed name segments, outermost qualifier
    /// first — when it resolved. `None` for every other outcome: an
    /// external one resolved to no declaration in the reading, and a
    /// failed one has no identity to name. A sibling-qualified
    /// reference that resolved under segments its sibling fields
    /// contributed returns that qualified identity, not the spelling
    /// it was written with.
    pub fn resolved_identity(&self) -> Option<&[String]> {
        match (&self.outcome, self.candidates.as_slice()) {
            (LinkOutcome::Resolved, [target]) => Some(target.identity.name.0.as_slice()),
            _ => None,
        }
    }
}

/// Every reference one declared reference type looks up.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReferenceLinks {
    pub name: String,
    pub domain: String,
    pub qualification: Qualification,
    /// In stream order.
    pub links: Vec<Link>,
}

/// The result of linking one reading.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LinkReport {
    /// One entry per declared reference type, in declaration order. A
    /// report made by [`check_reading`] holds the failed links only.
    pub types: Vec<ReferenceLinks>,
    pub duplicates: Vec<DuplicateDeclaration>,
    /// The duplicates' diagnostics, then each failed link's, in stream
    /// order, then each pending payload's that fails its member type.
    pub diagnostics: Vec<DiagnosticRecord>,
}

impl LinkReport {
    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

/// Whether an open-world arm admits one unresolved reference: the
/// declaration its outermost identity segment names (spelled, first
/// segment, or from context) is indexed and holds a listed value in
/// the member the arm names.
fn external(
    definition: &Definition,
    index: &IdentityIndex,
    reference: &Reference,
    arm: &OpenWorldDecl,
) -> Option<LinkOutcome> {
    let names = &definition.names;
    let field = names.resolve(arm.system_field);
    let value = reference
        .qualifiers
        .iter()
        .find(|(name, _)| name == field)
        .map(|(_, value)| value.as_str())
        .or_else(|| match reference.name.0.len() {
            len if len == arm.identity_len => reference.name.0.first().map(String::as_str),
            len if len + 1 == arm.identity_len => reference.context.as_deref(),
            _ => None,
        })?;
    let member = names.resolve(arm.system_member);
    index
        .single(value)
        .any(|entry| {
            field_text(&entry.fields, member)
                .is_some_and(|held| arm.values.iter().any(|value| value == held))
        })
        .then_some(LinkOutcome::External)
}

/// The type a resolved declaration's member is declared with: the field
/// of that name on the declaration's object type.
fn member_type<'d>(
    definition: &'d Definition,
    target: &IndexedDeclaration,
    member: &str,
) -> Option<&'d TypeExpr> {
    let object = object_of(definition, target.ty?)?;
    let member = definition.names.get(member)?;
    Some(&object.fields.iter().find(|def| def.name == member)?.ty)
}

/// Whether a pending payload (one value, else a list of all values)
/// fits its expected type, or what was found. A list member takes
/// element-typed values; any other takes one, or none if optional.
/// No scalar/list promotion either way.
fn payload_fits(
    decoder: &Decoder,
    definition: &Definition,
    expected: &TypeExpr,
    pending: &PendingBinding,
) -> Result<(), String> {
    let values: Vec<&Value> = match &pending.raw {
        Value::List(values) => values.iter().collect(),
        value => vec![value],
    };
    let decodes = |ty: &TypeExpr, value: &Value| {
        let types = &definition.types;
        let names = &definition.names;
        match decoder.decode(types, names, ty, value, pending.against.site) {
            Some(_) => Ok(()),
            None => Err(found(value)),
        }
    };
    let (optional, ty) = match expected {
        TypeExpr::Optional(inner) => (true, inner.as_ref()),
        ty => (false, ty),
    };
    match (ty, values.as_slice()) {
        (TypeExpr::List(inner), values) => {
            values.iter().try_for_each(|value| decodes(inner, value))
        }
        (_, []) if optional => Ok(()),
        (_, []) => Err("no value".to_owned()),
        (ty, [value]) => decodes(ty, value),
        (_, values) => Err(format!("{} values", values.len())),
    }
}

/// Whether a `from-kind` payload fits the shape its declaration's
/// member value selects: flag Bool, number U64, text String, choice one
/// of the set held in the field the binding names, or the declaration's
/// `choices` when it names none. Any other or missing kind fits no
/// shape.
fn kind_fits(
    decoder: &Decoder,
    definition: &Definition,
    target: &IndexedDeclaration,
    pending: &PendingBinding,
) -> Result<(), (String, String)> {
    let bound = |name: &str| target.fields.iter().find(|field| field.name == name);
    let member = pending.member.as_str();
    let kind = match bound(member).map(|field| &field.value) {
        Some(Value::String(kind)) => kind.clone(),
        Some(Value::Null) | None => {
            return Err((member.to_owned(), format!("no value for member {member}")));
        }
        Some(other) => found(other),
    };
    let builtin = match kind.as_str() {
        "flag" => Builtin::Bool,
        "number" => Builtin::U64,
        "text" => Builtin::String,
        "choice" => {
            let set = pending.choices.as_deref().unwrap_or(CHOICES);
            return choice_fits(bound(set).map(|field| &field.value), pending);
        }
        _ => {
            let found = format!("kind {kind} is not a kind the codec binds");
            return Err((member.to_owned(), found));
        }
    };
    let expected = definition
        .names
        .get(builtin.name())
        .map(TypeExpr::Named)
        .ok_or_else(|| (builtin.name().to_owned(), "no such type".to_owned()))?;
    payload_fits(decoder, definition, &expected, pending)
        .map_err(|found| (builtin.name().to_owned(), found))
}

/// Whether a `from-type-name` payload fits the declared type its
/// declaration names. A name no declared type answers, or a member
/// holding no string, fits nothing.
fn type_name_fits(
    decoder: &Decoder,
    definition: &Definition,
    target: &IndexedDeclaration,
    pending: &PendingBinding,
) -> Result<(), (String, String)> {
    let member = pending.member.as_str();
    let name = match target
        .fields
        .iter()
        .find(|field| field.name == member)
        .map(|field| &field.value)
    {
        Some(Value::String(name)) => name.clone(),
        Some(Value::Null) | None => {
            return Err((member.to_owned(), format!("no value for member {member}")));
        }
        Some(other) => found(other),
    };
    let expected =
        named_type(definition, &name).ok_or_else(|| (name.clone(), "no such type".to_owned()))?;
    payload_fits(decoder, definition, &expected, pending)
        .map_err(|found| (summary::expr(&definition.names, &expected), found))
}

/// The type a `from-type-name` member names: a bare name resolves to
/// the declared type it spells, a compound expression when every type
/// name it mentions is declared. Anything else resolves to nothing.
fn named_type(definition: &Definition, text: &str) -> Option<TypeExpr> {
    if let Some(sym) = definition.names.get(text) {
        return definition.types.lookup(sym).map(|_| TypeExpr::Named(sym));
    }
    let mut names = definition.names.clone();
    let expr = crate::type_expr::parse(text, &mut names).ok()?;
    names_declared(definition, &expr).then_some(expr)
}

/// Whether every type name an expression mentions is a declared type.
fn names_declared(definition: &Definition, ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Named(sym) => definition.types.lookup(*sym).is_some(),
        TypeExpr::Optional(inner)
        | TypeExpr::Nullable(inner)
        | TypeExpr::List(inner)
        | TypeExpr::Ref(inner) => names_declared(definition, inner),
        TypeExpr::Map(key, value) | TypeExpr::OrderedMap(key, value) => {
            names_declared(definition, key) && names_declared(definition, value)
        }
        TypeExpr::Union(members) => members.iter().all(|m| names_declared(definition, m)),
        TypeExpr::Instantiation(_, _) => false,
    }
}

/// Whether a `choice` payload is exactly one scalar equal, by spelling,
/// to one of the declaration's bound choices.
fn choice_fits(choices: Option<&Value>, pending: &PendingBinding) -> Result<(), (String, String)> {
    let choices: Vec<&Value> = match choices {
        Some(Value::List(choices)) => choices.iter().collect(),
        _ => Vec::new(),
    };
    let spelled: Vec<String> = choices.iter().map(|choice| found(choice)).collect();
    let expected = if spelled.is_empty() {
        "one of no choices".to_owned()
    } else {
        format!("one of {}", spelled.join(", "))
    };
    let value = match &pending.raw {
        Value::List(values) => match values.as_slice() {
            [] => return Err((expected, "no value".to_owned())),
            [value] => value,
            values => return Err((expected, format!("{} values", values.len()))),
        },
        value => value,
    };
    let scalar = !matches!(value, Value::List(_) | Value::Map(_));
    if scalar && choices.contains(&value) {
        Ok(())
    } else {
        Err((expected, found(value)))
    }
}

/// Link one reading: derive each sibling-qualified reference's
/// contributions after the sibling fields holding them resolve, look up
/// and link every reference against the identity index, then validate
/// the pending payloads. Every consumer that links a reading runs this
/// one composition, so the command line and a typed reading decide each
/// outcome alike. The report holds every link.
pub fn link_reading(inputs: &LinkInputs<'_>) -> LinkReport {
    Linking::new(inputs, true).run()
}

/// Link one reading as [`link_reading`] does and report its verdict
/// alone: the same duplicates and diagnostics, with only the failed
/// links kept. What a reading of any size needs to decide whether it
/// links.
pub fn check_reading(inputs: &LinkInputs<'_>) -> LinkReport {
    Linking::new(inputs, false).run()
}

/// One link pass over a reading's inputs.
struct Linking<'a, 'd> {
    inputs: &'a LinkInputs<'d>,
    definition: &'d Definition,
    /// Whether every link is reported, or the failed ones alone.
    full: bool,
    found: Vec<Option<Found>>,
    fills: Vec<Option<SiblingFill>>,
    active: Vec<bool>,
    disagreements: Vec<DiagnosticRecord>,
}

impl<'a, 'd> Linking<'a, 'd> {
    fn new(inputs: &'a LinkInputs<'d>, full: bool) -> Self {
        let count = inputs.references.len();
        Self {
            inputs,
            definition: inputs.definition,
            full,
            found: vec![None; count],
            fills: vec![None; count],
            active: vec![false; count],
            disagreements: Vec::new(),
        }
    }

    fn declared(&self, at: usize) -> &'d ReferenceDecl {
        &self.definition.symbols.references()[self.inputs.references[at].declared]
    }

    fn run(mut self) -> LinkReport {
        let inputs = self.inputs;
        let definition = self.definition;
        let index = &inputs.index;
        let documents = &inputs.documents;
        let names = &definition.names;
        let mut types: Vec<ReferenceLinks> = definition
            .symbols
            .references()
            .iter()
            .map(|decl| ReferenceLinks {
                name: names.resolve(decl.name).to_owned(),
                domain: domain_of(definition, decl).to_owned(),
                qualification: decl.qualification,
                links: Vec::new(),
            })
            .collect();
        let mut diagnostics: Vec<DiagnosticRecord> = index
            .duplicates()
            .iter()
            .map(|duplicate| duplicate_record(duplicate, documents))
            .collect();
        // Each pending payload's reference, by its site: the first link
        // made for it decides what the payload is validated against.
        let mut awaited: HashMap<Location, Vec<usize>> = HashMap::new();
        for (at, pending) in inputs.pending.iter().enumerate() {
            if pending.codec != PATH_VALUE_CODEC {
                awaited.entry(pending.against.site).or_default().push(at);
            }
        }
        let mut settled: Vec<Option<(usize, Option<u32>)>> = vec![None; inputs.pending.len()];
        for at in 0..inputs.references.len() {
            self.resolve(at);
            let bound = &inputs.references[at];
            let declared = self.declared(at);
            let Some(found) = self.found[at].take() else {
                continue;
            };
            let sibling = self.fills[at].as_ref();
            let outcome = match found.candidates.as_slice() {
                [] => declared
                    .open_world
                    .as_ref()
                    .and_then(|arm| external(definition, index, &bound.reference, arm))
                    .unwrap_or(LinkOutcome::Unresolved),
                [target] => {
                    let target = &index.declarations[*target as usize];
                    let unmet = unmet_qualifiers(definition, bound, declared, target, sibling);
                    if unmet.is_empty() {
                        LinkOutcome::Resolved
                    } else {
                        LinkOutcome::UnmetQualifier(unmet)
                    }
                }
                _ => LinkOutcome::Ambiguous,
            };
            if let Some(waiting) = awaited.get(&bound.reference.site) {
                let target = match (&outcome, found.candidates.as_slice()) {
                    (LinkOutcome::Resolved, [target]) => Some(*target),
                    _ => None,
                };
                for pending in waiting {
                    let first = settled[*pending].is_none_or(|(kind, _)| bound.declared < kind);
                    if first && inputs.pending[*pending].against == bound.reference {
                        settled[*pending] = Some((bound.declared, target));
                    }
                }
            }
            // The lookup stays for a sibling this reference qualifies.
            let keep = bound.holder != NONE;
            if self.full || outcome.code().is_some() {
                let link = Link {
                    reference: bound.reference.clone(),
                    node: bound.node.clone(),
                    field: bound.field.clone(),
                    key: bound.key,
                    candidates: found
                        .candidates
                        .iter()
                        .map(|at| index.declarations[*at as usize].clone())
                        .collect(),
                    gated: found.gated.clone(),
                    attempt: sibling.and_then(|fill| fill.attempt.clone()),
                    outcome,
                };
                diagnostics.extend(link_records(&link, documents));
                types[bound.declared].links.push(link);
            }
            if keep {
                self.found[at] = Some(found);
            }
        }
        diagnostics.append(&mut self.disagreements);
        self.validate_pending(&settled, &mut diagnostics);
        LinkReport {
            duplicates: index.duplicates().to_vec(),
            types,
            diagnostics,
        }
    }

    /// Resolve one bound reference's lookup, its sibling contributions
    /// first; a reference still being resolved contributes nothing, so
    /// a cycle degrades instead of looping.
    fn resolve(&mut self, at: usize) {
        if self.found[at].is_some() || self.active[at] {
            return;
        }
        self.active[at] = true;
        let fill = self.fill_for(at).filter(|fill| !fill.windows.is_empty());
        if let Some(fill) = &fill {
            self.disagree(at, fill);
        }
        let reference = &self.inputs.references[at].reference;
        let policy = self.declared(at).qualification;
        let found = self.inputs.index.lookup(reference, policy, fill.as_ref());
        self.fills[at] = fill;
        self.found[at] = Some(found);
        self.active[at] = false;
    }

    /// The bound reference held last in one field of a holder.
    fn held_in(&self, holder: Holder, field: &str) -> Option<usize> {
        (holder.first as usize..holder.end as usize)
            .rev()
            .find(|at| self.inputs.references[*at].field.as_deref() == Some(field))
    }

    /// One dependent's sibling contributions, in qualifier declaration
    /// order, each resolved sibling filling the leading identity
    /// segments its target composes.
    fn fill_for(&mut self, at: usize) -> Option<SiblingFill> {
        let declared = self.declared(at);
        if declared.qualification != Qualification::ExplicitOrSibling {
            return None;
        }
        let definition = self.definition;
        let names = &definition.names;
        let qualifiers: Vec<Sym> = declared
            .qualifiers
            .iter()
            .filter_map(|rule| rule.from_field)
            .collect();
        if qualifiers.is_empty() {
            return None;
        }
        let labels = target_labels(definition, declared);
        let bound = &self.inputs.references[at];
        let holder = *self.inputs.holders.get(bound.holder as usize)?;
        let mut windows = Vec::new();
        let mut start = 0usize;
        let mut contributed = 0usize;
        for field in &qualifiers {
            let field = names.resolve(*field);
            let sibling = self.held_in(holder, field);
            let resolved = sibling
                .filter(|sibling| *sibling != at)
                .inspect(|sibling| self.resolve(*sibling))
                .and_then(|sibling| self.resolved_identity(sibling));
            match resolved {
                Some(segments) => {
                    start += segments.len();
                    windows.push(SiblingWindow {
                        start: start - segments.len(),
                        field: field.to_owned(),
                        segments,
                    });
                    contributed += 1;
                }
                None => {
                    // An absent sibling contributes nothing, but its
                    // window still advances by the arity its target
                    // composes, so a later qualifier fills its own
                    // leading segments.
                    let target = sibling
                        .map(|sibling| self.declared(sibling).target)
                        .or_else(|| {
                            let object = object_of(definition, holder.ty?)?;
                            field_reference_type(definition, object, field, false)
                                .map(|decl| decl.target)
                        });
                    let Some(target) = target else {
                        break;
                    };
                    match composition_len(definition, target) {
                        Some(len) => start += len,
                        None => break,
                    }
                }
            }
        }
        let name = &bound.reference.name.0;
        let attempt = (contributed == qualifiers.len()
            && !labels.is_empty()
            && start + name.len() == labels.len())
        .then(|| {
            windows
                .iter()
                .flat_map(|window| window.segments.iter())
                .chain(name.iter())
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(".")
        });
        Some(SiblingFill { windows, attempt })
    }

    /// The identity one bound reference resolved to: exactly one
    /// candidate, meeting every required qualifier.
    fn resolved_identity(&self, at: usize) -> Option<Vec<String>> {
        let found = self.found.get(at)?.as_ref()?;
        let [target] = found.candidates.as_slice() else {
            return None;
        };
        let target = &self.inputs.index.declarations[*target as usize];
        let sibling = self.fills.get(at)?.as_ref();
        let bound = &self.inputs.references[at];
        let unmet = unmet_qualifiers(self.definition, bound, self.declared(at), target, sibling);
        unmet.is_empty().then(|| target.identity.name.0.clone())
    }

    /// The REF006 disagreements of one dependent's contributions
    /// against the values its spelling pins.
    fn disagree(&mut self, at: usize, fill: &SiblingFill) {
        let labels = target_labels(self.definition, self.declared(at));
        if labels.is_empty() {
            return;
        }
        let reference = &self.inputs.references[at].reference;
        let mut spelled: Vec<Option<&str>> = vec![None; labels.len()];
        for (name, value) in &reference.qualifiers {
            if let Some(at) = labels.iter().position(|label| label == name) {
                spelled[at] = Some(value);
            }
        }
        let open: Vec<usize> = (0..labels.len())
            .filter(|at| spelled[*at].is_none())
            .collect();
        let Some(missing) = open.len().checked_sub(reference.name.0.len()) else {
            return;
        };
        for (at, segment) in open[missing..].iter().zip(&reference.name.0) {
            spelled[*at] = Some(segment);
        }
        for window in &fill.windows {
            let Some(found) = window
                .segments
                .iter()
                .enumerate()
                .find_map(|(offset, segment)| {
                    let held = spelled.get(window.start + offset).copied().flatten()?;
                    (held != segment.as_str()).then_some(held)
                })
            else {
                continue;
            };
            let sibling = window.segments.join(".");
            let values = [
                reference.written.as_str(),
                reference.domain.as_str(),
                found,
                window.field.as_str(),
                sibling.as_str(),
            ];
            let at = Some(reference.site);
            self.disagreements.push(located(
                record(SIBLING_DISAGREEMENT, &values, ORIGIN),
                at,
                &self.inputs.documents,
            ));
        }
    }

    /// Validate each pending typed binding, in stream order, against
    /// the one declaration its reference names — its member type, under
    /// `from-kind` the shape its member value selects, or under
    /// `from-type-name` the declared type its member names — appending
    /// a `KDLITH-REF005` for each payload that does not fit. Linking
    /// resolved the reference first; a reference with a link that did
    /// not resolve already has its link's diagnostic and is not
    /// validated. Only a reference no declared reference type covers
    /// falls back to the identity index, which resolves it by name
    /// under the namespace carve-out. Nothing executes a payload.
    fn validate_pending(
        &self,
        settled: &[Option<(usize, Option<u32>)>],
        diagnostics: &mut Vec<DiagnosticRecord>,
    ) {
        let definition = self.definition;
        let index = &self.inputs.index;
        let decoder = &definition.runtime.get(definition).decoder;
        for (pending, settled) in self.inputs.pending.iter().zip(settled) {
            // A path-typed value is not a reference: its item carries the
            // binding the binder resolved at bind time, and the link stage
            // validates nothing of it.
            if pending.codec == PATH_VALUE_CODEC {
                continue;
            }
            let target = match settled {
                Some((_, target)) => target.map(|at| &*index.declarations[at as usize]),
                None => index.target(&pending.against),
            };
            let Some(target) = target else {
                continue;
            };
            let outcome = if pending.codec == BindingCodec::FromKind.name() {
                kind_fits(decoder, definition, target, pending)
            } else if pending.codec == BindingCodec::FromTypeName.name() {
                type_name_fits(decoder, definition, target, pending)
            } else {
                let Some(expected) = member_type(definition, target, &pending.member) else {
                    continue;
                };
                payload_fits(decoder, definition, expected, pending)
                    .map_err(|found| (summary::expr(&definition.names, expected), found))
            };
            if let Err((expected, found)) = outcome {
                let values = [
                    pending.node_name.as_str(),
                    pending.codec.as_str(),
                    expected.as_str(),
                    found.as_str(),
                ];
                let at = Some(pending.against.site);
                diagnostics.push(located(
                    record(PAYLOAD_TYPE, &values, ORIGIN),
                    at,
                    &self.inputs.documents,
                ));
            }
        }
    }
}

/// The identity-field labels of a reference declaration's target
/// composition, when a declaration node of the target type composes
/// one.
fn target_labels(definition: &Definition, declared: &ReferenceDecl) -> Vec<String> {
    let names = &definition.names;
    definition
        .bindings
        .iter()
        .find(|(_, node)| {
            node.ty == Some(declared.target)
                && node
                    .identity
                    .as_ref()
                    .is_some_and(|identity| !identity.fields.is_empty())
        })
        .and_then(|(_, node)| node.identity.as_ref())
        .map(|identity| {
            identity
                .fields
                .iter()
                .map(|field| names.resolve(*field).to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// How many identity segments one target type composes.
fn composition_len(definition: &Definition, target: Sym) -> Option<usize> {
    definition
        .bindings
        .iter()
        .find(|(_, node)| {
            node.ty == Some(target)
                && node
                    .identity
                    .as_ref()
                    .is_some_and(|identity| !identity.fields.is_empty())
        })
        .and_then(|(_, node)| node.identity.as_ref())
        .map(|identity| identity.fields.len())
}

/// The domains linking covers: every identity block's, every declared
/// reference type's lookup domain, and every dependent binding's
/// `ref<T>` target domain.
fn linked_domains(definition: &Definition) -> BTreeSet<&str> {
    let names = &definition.names;
    let blocks = definition
        .symbols
        .declarations()
        .iter()
        .filter(|decl| decl.identity.is_some())
        .map(|decl| names.resolve(decl.domain));
    let looked_up = definition
        .symbols
        .references()
        .iter()
        .map(|decl| domain_of(definition, decl));
    let dependent = definition.bindings.iter().filter_map(|(_, node)| {
        let dependent = node.value.as_ref()?;
        dependent_domain(definition, node, dependent.target)
    });
    blocks.chain(looked_up).chain(dependent).collect()
}

/// The domain a dependent binding's target field names: the type
/// argument of its `ref<T>`, from the node's inline field type or its
/// object type's field of that name (`ref<T>` is forced at compile,
/// `KDLITH-DLC005`).
fn dependent_domain<'d>(
    definition: &'d Definition,
    node: &NodeDecl,
    target: Sym,
) -> Option<&'d str> {
    let inline = || {
        node.fields
            .iter()
            .find(|field| field.name == target)
            .and_then(|field| field.ty.as_ref())
    };
    let from_type = || {
        let ty = definition.types.lookup(node.ty?)?;
        let TypeKind::Object(object) = &definition.types.get(ty)?.kind else {
            return None;
        };
        Some(&object.fields.iter().find(|def| def.name == target)?.ty)
    };
    let TypeExpr::Ref(inner) = inline().or_else(from_type)? else {
        return None;
    };
    match inner.as_ref() {
        TypeExpr::Named(name) => Some(definition.names.resolve(*name)),
        _ => None,
    }
}

/// The domain a reference type looks up: its lookup domain, else its
/// target's.
fn domain_of<'d>(definition: &'d Definition, decl: &ReferenceDecl) -> &'d str {
    definition.names.resolve(decl.lookup.unwrap_or(decl.target))
}

/// Whether a declaration's identity matches a reference's spelled
/// segments and qualifiers under a qualification policy.
fn matches(
    entry: &IndexedDeclaration,
    segments: &[String],
    reference: &Reference,
    policy: Qualification,
    sibling: Option<&SiblingFill>,
) -> bool {
    derive(entry, segments, reference, policy, sibling).is_some()
}

/// Whether a reference is written in the document that holds a
/// declaration: a namespaced import namespaces other documents' view of
/// its declarations, never their own.
fn own_document(entry: &IndexedDeclaration, reference: &Reference) -> bool {
    entry.node.source == reference.site.source
}

/// The name segments after a reference's first, when that first segment
/// is the declaration's namespace and the rest match it.
fn through_namespace<'r>(
    entry: &IndexedDeclaration,
    reference: &'r Reference,
    policy: Qualification,
    sibling: Option<&SiblingFill>,
) -> Option<&'r [String]> {
    let (first, rest) = reference.name.0.split_first()?;
    (entry.namespace.as_ref() == Some(first)
        && !rest.is_empty()
        && matches(entry, rest, reference, policy, sibling))
    .then_some(rest)
}

/// The segments a reference supplies for an identity composed of
/// `arity` segments under `labels`, by position; see
/// [`IdentityIndex::lookup`] for the filling order. An open position is
/// supplied by nothing; `None` when the reference cannot name such an
/// identity at all.
fn pattern<'r>(
    labels: &[String],
    arity: usize,
    segments: &'r [String],
    reference: &'r Reference,
    policy: Qualification,
    sibling: Option<&'r SiblingFill>,
) -> Option<Vec<Option<&'r str>>> {
    let mut slots: Vec<Option<&str>> = vec![None; arity];
    for (name, value) in &reference.qualifiers {
        if let Some(at) = labels.iter().position(|label| label == name) {
            if slots[at].is_some_and(|held| held != value) {
                return None;
            }
            slots[at] = Some(value);
        }
    }
    let open: Vec<usize> = (0..slots.len()).filter(|at| slots[*at].is_none()).collect();
    let missing = open.len().checked_sub(segments.len())?;
    for (at, segment) in open[missing..].iter().zip(segments) {
        slots[*at] = Some(segment);
    }
    if let Some(fill) = sibling {
        // Sibling mode: the windows fill the leading open slots, the
        // spelling has filled the innermost, and every slot must be
        // filled — nothing is inferred and no segment stays open. A
        // slot the spelling already pins keeps its spelled value; the
        // disagreement is a link diagnostic, never a silent override.
        for window in &fill.windows {
            for (offset, segment) in window.segments.iter().enumerate() {
                let slot = slots.get_mut(window.start + offset)?;
                if slot.is_none() {
                    *slot = Some(segment.as_str());
                }
            }
        }
        return slots.iter().all(Option::is_some).then_some(slots);
    }
    if missing > 0 {
        // Sibling mode reaches here only when no sibling contributed:
        // the bare name then matches nothing, exactly as `explicit`.
        if matches!(
            policy,
            Qualification::Explicit | Qualification::ExplicitOrSibling
        ) {
            return None;
        }
        if let Some(context) = reference.context.as_deref() {
            slots[open[missing - 1]] = Some(context);
        }
    }
    Some(slots)
}

/// The segments a reference supplies for a declaration's identity, by
/// position, when they agree with it.
fn derive<'r>(
    entry: &IndexedDeclaration,
    segments: &'r [String],
    reference: &'r Reference,
    policy: Qualification,
    sibling: Option<&'r SiblingFill>,
) -> Option<Vec<Option<&'r str>>> {
    let declared = &entry.identity.name.0;
    pattern(
        &entry.labels,
        declared.len(),
        segments,
        reference,
        policy,
        sibling,
    )
    .filter(|slots| agrees(slots, declared))
}

/// The required qualifiers its resolved target fails: the reference
/// supplies each, spelled or derived into the identity field of that
/// name, and the target declares the same value. A `from-field`
/// qualifier is satisfied by its sibling's own resolution, checked as a
/// disagreement when both spell, so it never fails here.
fn unmet_qualifiers(
    definition: &Definition,
    bound: &BoundReference,
    declared: &ReferenceDecl,
    target: &IndexedDeclaration,
    sibling: Option<&SiblingFill>,
) -> Vec<UnmetQualifier> {
    let reference = &bound.reference;
    let policy = declared.qualification;
    let plain = reference.name.0.as_slice();
    let unqualified =
        own_document(target, reference) && matches(target, plain, reference, policy, sibling);
    let segments = match &target.namespace {
        Some(_) if !unqualified => {
            through_namespace(target, reference, policy, sibling).unwrap_or_default()
        }
        _ => plain,
    };
    let derived = derive(target, segments, reference, policy, sibling).unwrap_or_default();
    declared
        .qualifiers
        .iter()
        .filter(|rule| rule.required)
        .filter(|rule| rule.from_field.is_none())
        .filter_map(|rule| {
            let name = definition.names.resolve(rule.name);
            let spelled = reference
                .qualifiers
                .iter()
                .find(|(qualifier, _)| qualifier == name)
                .map(|(_, value)| value.as_str())
                .or_else(|| {
                    let at = target.labels.iter().position(|label| label == name)?;
                    derived.get(at).copied().flatten()
                })
                .map(str::to_owned);
            let declared = field_text(&target.fields, name).map(str::to_owned);
            let met = spelled.is_some() && spelled == declared;
            (!met).then(|| UnmetQualifier {
                qualifier: name.to_owned(),
                spelled,
                declared,
            })
        })
        .collect()
}

/// A field's text, when the field holds a string or number.
fn field_text<'f>(fields: &'f [Field], name: &str) -> Option<&'f str> {
    match &fields.iter().find(|field| field.name == name)?.value {
        Value::String(text) | Value::Number(text) => Some(text),
        _ => None,
    }
}

/// One reference an item holds.
struct Held<'i> {
    /// The record or declaration node holding it.
    node: Option<&'i str>,
    /// The record or declaration field holding it.
    field: Option<&'i str>,
    /// It is the key of an entry of the field's map.
    key: bool,
    reference: &'i Reference,
}

/// Every reference an item holds, in field order, each map entry's key
/// before its value.
fn held_references(item: &Item) -> Vec<Held<'_>> {
    let mut held = Vec::new();
    let (node, fields) = match item {
        Item::Record(record) => (
            (!record.node_name.is_empty()).then_some(record.node_name.as_str()),
            record.fields.as_slice(),
        ),
        Item::Declaration(declaration) => (
            (!declaration.node_name.is_empty()).then_some(declaration.node_name.as_str()),
            declaration.fields.as_slice(),
        ),
        Item::Contribution(contribution) => {
            collect(None, None, &contribution.value, false, &mut held);
            return held;
        }
        _ => return held,
    };
    for field in fields {
        collect(
            node,
            Some(field.name.as_str()),
            &field.value,
            false,
            &mut held,
        );
    }
    held
}

fn collect<'v>(
    node: Option<&'v str>,
    field: Option<&'v str>,
    value: &'v Value,
    key: bool,
    held: &mut Vec<Held<'v>>,
) {
    match value {
        Value::Reference(reference) => held.push(Held {
            node,
            field,
            key,
            reference,
        }),
        Value::List(values) => values
            .iter()
            .for_each(|value| collect(node, field, value, key, held)),
        Value::Map(entries) => entries.iter().for_each(|entry| {
            if let Some(reference) = &entry.reference {
                held.push(Held {
                    node,
                    field,
                    key: true,
                    reference,
                });
            }
            collect(node, field, &entry.value, key, held);
        }),
        _ => {}
    }
}

/// A name as its segments read, outermost first, under a namespace
/// namespace when it has one.
fn spelled(namespace: Option<&str>, name: &QualifiedName) -> String {
    namespace
        .into_iter()
        .chain(name.0.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(".")
}

fn duplicate_record(
    duplicate: &DuplicateDeclaration,
    documents: &BTreeMap<SourceId, String>,
) -> DiagnosticRecord {
    let declaration = &duplicate.declaration;
    let name = format!(
        "{} {}",
        declaration.identity.domain,
        spelled(declaration.namespace.as_deref(), &declaration.identity.name)
    );
    let record = located(
        record(DUPLICATE, &[&name], ORIGIN),
        declaration.site,
        documents,
    );
    match duplicate.first.site {
        Some(first) => {
            let start = first.span.start;
            let label = Label::new(
                documents.get(&first.source).cloned(),
                first.span.byte_span(),
                "first declared here",
            )
            .with_position(start.line, start.column);
            record.with_label(label)
        }
        None => record,
    }
}

fn link_records(link: &Link, documents: &BTreeMap<SourceId, String>) -> Vec<DiagnosticRecord> {
    let reference = &link.reference;
    let at = Some(reference.site);
    // The site every failed outcome names the reference by: a map-key
    // reference names the map's field; REF001 below additionally names
    // the holding node and field, as a type mismatch does.
    let site = match (&link.field, link.key) {
        (Some(field), true) => format!(" as a key of {field}"),
        _ => String::new(),
    };
    let name = format!("{} in {}{}", reference.written, reference.domain, site);
    match &link.outcome {
        LinkOutcome::Resolved | LinkOutcome::External => Vec::new(),
        LinkOutcome::Unresolved => {
            // A sibling-qualified reference that derived contributions
            // names the qualified spelling it attempted, never the
            // bare one it wrote.
            let written = link.attempt.as_deref().unwrap_or(&reference.written);
            let held = match (&link.node, &link.field, link.key) {
                (Some(node), Some(field), true) => format!(" as a key of {node}.{field}"),
                (Some(node), Some(field), false) => format!(" held by {node}.{field}"),
                _ => site,
            };
            let name = format!("{} in {}{}", written, reference.domain, held);
            let name = match link.gated.as_slice() {
                [] => name,
                gated => format!(
                    "{name} (declared only under the namespace {})",
                    gated.join(", ")
                ),
            };
            vec![located(record(UNRESOLVED, &[&name], ORIGIN), at, documents)]
        }
        LinkOutcome::Ambiguous => {
            let candidates = link
                .candidates
                .iter()
                .map(|candidate| spelled(candidate.namespace.as_deref(), &candidate.identity.name))
                .collect::<Vec<_>>()
                .join(", ");
            let values = [name.as_str(), candidates.as_str()];
            vec![located(record(AMBIGUOUS, &values, ORIGIN), at, documents)]
        }
        LinkOutcome::UnmetQualifier(unmet) => unmet
            .iter()
            .map(|unmet| {
                let reason = match (&unmet.spelled, &unmet.declared) {
                    (None, _) => "the reference does not supply it".to_owned(),
                    (Some(_), None) => "the target declares no value for it".to_owned(),
                    (Some(spelled), Some(declared)) => {
                        format!("the reference spells {spelled}, the target declares {declared}")
                    }
                };
                let values = [name.as_str(), unmet.qualifier.as_str(), reason.as_str()];
                located(record(UNMET_QUALIFIER, &values, ORIGIN), at, documents)
            })
            .collect(),
    }
}

/// A record placed at a location of a reading's document.
fn located(
    record: DiagnosticRecord,
    at: Option<Location>,
    documents: &BTreeMap<SourceId, String>,
) -> DiagnosticRecord {
    let Some(at) = at else {
        return record;
    };
    let start = at.span.start;
    let record = record.with_span(at.span.byte_span(), Some(start.line), Some(start.column));
    match documents.get(&at.source) {
        Some(path) => record.with_source(path.clone()),
        None => record,
    }
}
