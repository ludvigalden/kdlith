//! The link stage: identity construction, reference binding, lookup
//! policy, and linking as four separate pure passes over the symbol
//! catalog's declared surface and a reading's items.

use std::collections::{BTreeMap, BTreeSet};

use kdlith_diagnostic::{DiagnosticRecord, Label};
use kdlith_model::{
    BoundDeclaration, Field, Identity, Item, Location, NodeId, PendingBinding, QualifiedName,
    Reference, SourceId, Value,
};

use crate::decode::{Decoder, found};
use crate::definition::{Definition, Qualification, ReferenceDecl};
use crate::diag::record;
use crate::summary;
use crate::types::{TypeExpr, TypeKind};

const UNRESOLVED: &str = "KDLITH-REF001";
const DUPLICATE: &str = "KDLITH-REF002";
const AMBIGUOUS: &str = "KDLITH-REF003";
const UNMET_QUALIFIER: &str = "KDLITH-REF004";
const PAYLOAD_TYPE: &str = "KDLITH-REF005";
const ORIGIN: &str = "data-document";

/// The linkable surface of one reading, gathered from its item stream:
/// the items that carry declarations or references, the path of every
/// document, and the alias namespace each aliased-imported document
/// belongs to.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct LinkInputs {
    /// Declarations, pending typed bindings, and the records and
    /// contributions that hold a reference, in stream order.
    pub items: Vec<Item>,
    /// Each document's path as the reading shows it.
    pub documents: BTreeMap<SourceId, String>,
    /// The alias namespace of each document an aliased import loaded.
    pub aliases: BTreeMap<SourceId, String>,
    imports: ImportAliases,
}

impl LinkInputs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take one item of the reading, in stream order.
    pub fn observe(&mut self, item: &Item) {
        let alias = self.imports.observe(item);
        match item {
            Item::Document(document) => {
                if let Some(alias) = alias {
                    self.aliases.insert(document.source, alias);
                }
                self.documents
                    .insert(document.source, document.path.clone());
            }
            Item::Declaration(_) | Item::PendingBinding(_) => self.items.push(item.clone()),
            Item::Record(_) | Item::Contribution(_) if !references_of(item).is_empty() => {
                self.items.push(item.clone());
            }
            _ => {}
        }
    }
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
pub(crate) struct ImportAliases {
    /// Import requests whose targets may still load, oldest first.
    pending: Vec<(Option<String>, Vec<String>)>,
}

impl ImportAliases {
    /// Take one item of the reading, in stream order: for a document an
    /// aliased import loaded, that import's alias.
    pub(crate) fn observe(&mut self, item: &Item) -> Option<String> {
        match item {
            Item::Document(document) => self
                .pending
                .iter_mut()
                .rev()
                .find_map(|(alias, targets)| {
                    let at = targets.iter().position(|t| *t == document.path)?;
                    targets.remove(at);
                    Some(alias.clone())
                })
                .flatten(),
            Item::Import(request) if !request.targets.is_empty() => {
                self.pending
                    .push((request.alias.clone(), request.targets.clone()));
                None
            }
            _ => None,
        }
    }
}

/// One declaration registered in the identity index.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct IndexedDeclaration {
    pub node: NodeId,
    pub identity: Identity,
    /// The alias namespace the declaration registers under, when an
    /// aliased import loaded its document: it is reachable through the
    /// alias and not by its unqualified name.
    pub alias: Option<String>,
    /// The identity field each name segment came from, outermost first;
    /// empty when the composition is not known.
    pub labels: Vec<String>,
    pub fields: Vec<Field>,
    pub site: Option<Location>,
}

/// A declaration whose composed identity an earlier one in its domain
/// and namespace already claims.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DuplicateDeclaration {
    pub declaration: IndexedDeclaration,
    pub first: IndexedDeclaration,
}

/// Stage one: the reading's declarations by identity domain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct IdentityIndex {
    domains: BTreeMap<String, Vec<IndexedDeclaration>>,
    duplicates: Vec<DuplicateDeclaration>,
}

impl IdentityIndex {
    /// The declarations of one domain, in stream order.
    pub fn domain(&self, domain: &str) -> &[IndexedDeclaration] {
        self.domains.get(domain).map_or(&[], Vec::as_slice)
    }

    /// The indexed domains, in name order.
    pub fn domains(&self) -> impl Iterator<Item = (&str, &[IndexedDeclaration])> {
        self.domains
            .iter()
            .map(|(name, declarations)| (name.as_str(), declarations.as_slice()))
    }

    /// The declarations that repeat an identity, in stream order.
    pub fn duplicates(&self) -> &[DuplicateDeclaration] {
        &self.duplicates
    }
}

/// Stage one, identity construction: index the reading's declarations by
/// their composed identities. Only domains an identity block or a
/// declared reference type names are indexed. A declaration from a
/// document `aliases` attributes to an alias namespace registers under
/// that namespace only. Two declarations composing one identity in one
/// domain and namespace are a duplicate; the first stays indexed.
pub fn index_identities(
    definition: &Definition,
    items: &[Item],
    aliases: &BTreeMap<SourceId, String>,
) -> IdentityIndex {
    let linked = linked_domains(definition);
    let mut index = IdentityIndex::default();
    for item in items {
        let Item::Declaration(declaration) = item else {
            continue;
        };
        let identity = &declaration.identity;
        if identity.name.0.is_empty() || !linked.contains(identity.domain.as_str()) {
            continue;
        }
        let entry = IndexedDeclaration {
            node: declaration.node,
            identity: identity.clone(),
            alias: aliases.get(&declaration.node.source).cloned(),
            labels: labels(definition, declaration),
            fields: declaration.fields.clone(),
            site: declaration.site,
        };
        let domain = index.domains.entry(identity.domain.clone()).or_default();
        match domain
            .iter()
            .find(|held| held.alias == entry.alias && held.identity.name == identity.name)
        {
            Some(first) => index.duplicates.push(DuplicateDeclaration {
                first: first.clone(),
                declaration: entry,
            }),
            None => domain.push(entry),
        }
    }
    index
}

/// A reference the catalog declares a reference type for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BoundReference {
    pub reference: Reference,
    /// The record or declaration field holding the reference; `None`
    /// for a contribution's target.
    pub field: Option<String>,
    /// The reference is the key of an entry of `field`'s map.
    pub key: bool,
    /// The declared reference type whose lookup domain the reference
    /// names.
    pub declared: ReferenceDecl,
}

/// Stage two, reference binding: every reference value in the reading's
/// records, declarations, and contributions, in stream order, with the
/// declared reference type looking it up. A reference whose domain no
/// declared reference type looks up is left to its own mechanism.
///
/// The reference type the holding field's declared type names binds the
/// reference, so its qualification is the one the author chose. Where no
/// field type names one (a contribution's target), the first reference
/// type of the domain in declaration order whose qualifiers cover the
/// reference's spelled qualifiers binds it.
pub fn bind_references(definition: &Definition, items: &[Item]) -> Vec<BoundReference> {
    let mut bound = Vec::new();
    for item in items {
        for held in held_references(item) {
            let reference = held.reference;
            let named = held
                .field
                .and_then(|field| field_reference_type(definition, item, field, held.key))
                .filter(|decl| domain_of(definition, decl) == reference.domain);
            if let Some(decl) = named.or_else(|| domain_reference_type(definition, reference)) {
                bound.push(BoundReference {
                    reference: reference.clone(),
                    field: held.field.map(str::to_owned),
                    key: held.key,
                    declared: decl.clone(),
                });
            }
        }
    }
    bound
}

/// The reference type a record's or declaration's field is declared
/// with, through any optional, nullable, or list wrapper; for a key, the
/// key type of the map the field holds.
fn field_reference_type<'d>(
    definition: &'d Definition,
    item: &Item,
    field: &str,
    key: bool,
) -> Option<&'d ReferenceDecl> {
    let type_name = match item {
        Item::Record(record) => &record.type_name,
        Item::Declaration(declaration) => &declaration.type_name,
        _ => return None,
    };
    let names = &definition.names;
    let ty = definition.types.lookup(names.get(type_name)?)?;
    let TypeKind::Object(object) = &definition.types.get(ty)?.kind else {
        return None;
    };
    let field = names.get(field)?;
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

/// The candidates one reference's lookup policy derives.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Lookup {
    pub bound: BoundReference,
    pub candidates: Vec<IndexedDeclaration>,
    /// The alias namespaces holding a declaration the reference would
    /// match if aliased visibility did not gate it, in name order.
    pub gated: Vec<String>,
}

/// Stage three, lookup policy: derive each reference's candidates under
/// its reference type's qualification.
///
/// A spelled qualifier named after an identity field pins that field's
/// segment; the written name segments fill the innermost remaining
/// segments. `explicit` requires every segment spelled. The context
/// policies fill the innermost still-missing segment from the namespace
/// context captured where the reference bound, and leave any further
/// missing segment open, so a partial name may match several
/// declarations. `imported-or-context` also admits an alias namespace as
/// the name's first segment; declarations under an alias are reachable
/// in no other way.
pub fn derive_candidates(index: &IdentityIndex, bound: &[BoundReference]) -> Vec<Lookup> {
    bound
        .iter()
        .map(|bound| {
            let reference = &bound.reference;
            let policy = bound.declared.qualification;
            let segments = &reference.name.0;
            let mut candidates = Vec::new();
            let mut gated = BTreeSet::new();
            for entry in index.domain(&reference.domain) {
                match &entry.alias {
                    None => {
                        if matches(entry, segments, reference, policy) {
                            candidates.push(entry.clone());
                        }
                    }
                    Some(alias) => {
                        let prefixed = segments.split_first().filter(|(first, rest)| {
                            *first == alias
                                && !rest.is_empty()
                                && matches(entry, rest, reference, policy)
                        });
                        if prefixed.is_some() && policy == Qualification::ImportedOrContext {
                            candidates.push(entry.clone());
                        } else if prefixed.is_some() || matches(entry, segments, reference, policy)
                        {
                            gated.insert(alias.clone());
                        }
                    }
                }
            }
            Lookup {
                bound: bound.clone(),
                candidates,
                gated: gated.into_iter().collect(),
            }
        })
        .collect()
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
            LinkOutcome::Resolved => None,
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
    /// The record or declaration field holding the reference; `None`
    /// for a contribution's target.
    pub field: Option<String>,
    /// The reference is the key of an entry of `field`'s map.
    pub key: bool,
    pub candidates: Vec<IndexedDeclaration>,
    pub gated: Vec<String>,
    pub outcome: LinkOutcome,
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
    /// One entry per declared reference type, in declaration order.
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

    /// Validate each pending typed binding among `items`, in stream
    /// order, against the member type of the one declaration its
    /// reference resolved to, appending a `KDLITH-REF005` for each
    /// payload that does not fit. A pending binding whose reference did
    /// not resolve already has its link's diagnostic and is not
    /// validated. Nothing executes a payload.
    pub fn validate_pending(
        &mut self,
        definition: &Definition,
        items: &[Item],
        documents: &BTreeMap<SourceId, String>,
    ) {
        let decoder = Decoder::new(&definition.types).0.with_references(
            &definition.types,
            &definition.names,
            definition.symbols.references(),
        );
        for item in items {
            let Item::PendingBinding(pending) = item else {
                continue;
            };
            let Some(target) = self.resolved_target(&pending.against) else {
                continue;
            };
            let Some(expected) = member_type(definition, items, target, &pending.member) else {
                continue;
            };
            if let Err(found) = payload_fits(&decoder, definition, expected, pending) {
                let expected = summary::expr(&definition.names, expected);
                let values = [
                    pending.node_name.as_str(),
                    pending.codec.as_str(),
                    expected.as_str(),
                    found.as_str(),
                ];
                let at = Some(pending.against.site);
                self.diagnostics.push(located(
                    record(PAYLOAD_TYPE, &values, ORIGIN),
                    at,
                    documents,
                ));
            }
        }
    }

    /// The one declaration a reference resolved to.
    fn resolved_target(&self, reference: &Reference) -> Option<&IndexedDeclaration> {
        let link = self
            .types
            .iter()
            .flat_map(|entry| &entry.links)
            .find(|link| link.reference == *reference)?;
        match (&link.outcome, link.candidates.as_slice()) {
            (LinkOutcome::Resolved, [target]) => Some(target),
            _ => None,
        }
    }
}

/// The type a resolved declaration's member is declared with: the field
/// of that name on the declaration's object type.
fn member_type<'d>(
    definition: &'d Definition,
    items: &[Item],
    target: &IndexedDeclaration,
    member: &str,
) -> Option<&'d TypeExpr> {
    let type_name = items.iter().find_map(|item| match item {
        Item::Declaration(declaration) if declaration.node == target.node => {
            Some(&declaration.type_name)
        }
        _ => None,
    })?;
    let names = &definition.names;
    let ty = definition.types.lookup(names.get(type_name)?)?;
    let TypeKind::Object(object) = &definition.types.get(ty)?.kind else {
        return None;
    };
    let member = names.get(member)?;
    Some(&object.fields.iter().find(|def| def.name == member)?.ty)
}

/// Whether a pending payload fits its expected type, or what was found
/// instead. The raw payload is the argument tail: one value alone, or a
/// list of every value when there are more or fewer than one. A list
/// member takes zero or more values, each decoding as the element type;
/// any other member takes exactly one value, or none when optional. A
/// scalar is never read as a one-element list, nor a list as a scalar.
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

/// Stage four, linking: decide each lookup's outcome — one candidate
/// meeting every required qualifier resolves — and derive the report
/// with its diagnostics, located through `documents`.
pub fn link(
    definition: &Definition,
    index: &IdentityIndex,
    lookups: Vec<Lookup>,
    documents: &BTreeMap<SourceId, String>,
) -> LinkReport {
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
    for lookup in lookups {
        let Lookup {
            bound,
            candidates,
            gated,
        } = lookup;
        let outcome = match candidates.as_slice() {
            [] => LinkOutcome::Unresolved,
            [target] => {
                let unmet = unmet_qualifiers(definition, &bound, target);
                if unmet.is_empty() {
                    LinkOutcome::Resolved
                } else {
                    LinkOutcome::UnmetQualifier(unmet)
                }
            }
            _ => LinkOutcome::Ambiguous,
        };
        let link = Link {
            reference: bound.reference,
            field: bound.field,
            key: bound.key,
            candidates,
            gated,
            outcome,
        };
        diagnostics.extend(link_records(&link, documents));
        let name = names.resolve(bound.declared.name);
        if let Some(entry) = types.iter_mut().find(|entry| entry.name == name) {
            entry.links.push(link);
        }
    }
    LinkReport {
        duplicates: index.duplicates().to_vec(),
        types,
        diagnostics,
    }
}

/// Link one reading: index its identities, bind and look up its
/// references, link them, then validate its pending payloads. Every
/// consumer that links a reading runs this one composition, so the
/// command line and a typed reading decide each outcome alike.
pub fn link_reading(definition: &Definition, inputs: &LinkInputs) -> LinkReport {
    let index = index_identities(definition, &inputs.items, &inputs.aliases);
    let bound = bind_references(definition, &inputs.items);
    let lookups = derive_candidates(&index, &bound);
    let mut report = link(definition, &index, lookups, &inputs.documents);
    report.validate_pending(definition, &inputs.items, &inputs.documents);
    report
}

/// The domains linking covers: every identity block's, and every
/// declared reference type's lookup domain.
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
    blocks.chain(looked_up).collect()
}

/// The domain a reference type looks up: its lookup domain, else its
/// target's.
fn domain_of<'d>(definition: &'d Definition, decl: &ReferenceDecl) -> &'d str {
    definition.names.resolve(decl.lookup.unwrap_or(decl.target))
}

/// The identity field each segment of a declaration's name came from:
/// the composition of the declaring node whose identity fields the
/// declaration binds to exactly those segments.
fn labels(definition: &Definition, declaration: &BoundDeclaration) -> Vec<String> {
    let names = &definition.names;
    let segments = &declaration.identity.name.0;
    definition
        .symbols
        .declarations()
        .iter()
        .filter(|decl| names.resolve(decl.domain) == declaration.identity.domain)
        .map(|decl| {
            decl.identity_fields
                .iter()
                .map(|field| names.resolve(*field).to_owned())
                .collect::<Vec<_>>()
        })
        .find(|labels| {
            labels.len() == segments.len()
                && labels.iter().zip(segments).all(|(label, segment)| {
                    field_text(&declaration.fields, label).is_some_and(|text| text == segment)
                })
        })
        .unwrap_or_default()
}

/// Whether a declaration's identity matches a reference's spelled
/// segments and qualifiers under a qualification policy.
fn matches(
    entry: &IndexedDeclaration,
    segments: &[String],
    reference: &Reference,
    policy: Qualification,
) -> bool {
    derive(entry, segments, reference, policy).is_some()
}

/// The segments a reference supplies for a declaration's identity, by
/// position, when they agree with it; see [`derive_candidates`] for the
/// filling order. An open position is supplied by nothing.
fn derive<'r>(
    entry: &IndexedDeclaration,
    segments: &'r [String],
    reference: &'r Reference,
    policy: Qualification,
) -> Option<Vec<Option<&'r str>>> {
    let declared = &entry.identity.name.0;
    let mut slots: Vec<Option<&str>> = vec![None; declared.len()];
    for (name, value) in &reference.qualifiers {
        if let Some(at) = entry.labels.iter().position(|label| label == name) {
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
    if missing > 0 {
        if policy == Qualification::Explicit {
            return None;
        }
        if let Some(context) = reference.context.as_deref() {
            slots[open[missing - 1]] = Some(context);
        }
    }
    slots
        .iter()
        .zip(declared)
        .all(|(slot, segment)| slot.is_none_or(|supplied| supplied == segment))
        .then_some(slots)
}

/// The required qualifiers its resolved target fails: the reference
/// supplies each, spelled or derived into the identity field of that
/// name, and the target declares the same value.
fn unmet_qualifiers(
    definition: &Definition,
    bound: &BoundReference,
    target: &IndexedDeclaration,
) -> Vec<UnmetQualifier> {
    let reference = &bound.reference;
    let policy = bound.declared.qualification;
    let segments = match &target.alias {
        Some(_) => reference.name.0.get(1..).unwrap_or_default(),
        None => reference.name.0.as_slice(),
    };
    let derived = derive(target, segments, reference, policy).unwrap_or_default();
    bound
        .declared
        .qualifiers
        .iter()
        .filter(|rule| rule.required)
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

/// Every reference value an item holds, in field order.
fn references_of(item: &Item) -> Vec<&Reference> {
    held_references(item)
        .into_iter()
        .map(|held| held.reference)
        .collect()
}

/// One reference an item holds.
struct Held<'i> {
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
    let fields = match item {
        Item::Record(record) => record.fields.as_slice(),
        Item::Declaration(declaration) => declaration.fields.as_slice(),
        Item::Contribution(contribution) => {
            collect(None, &contribution.value, false, &mut held);
            return held;
        }
        _ => return held,
    };
    for field in fields {
        collect(Some(field.name.as_str()), &field.value, false, &mut held);
    }
    held
}

fn collect<'v>(field: Option<&'v str>, value: &'v Value, key: bool, held: &mut Vec<Held<'v>>) {
    match value {
        Value::Reference(reference) => held.push(Held {
            field,
            key,
            reference,
        }),
        Value::List(values) => values
            .iter()
            .for_each(|value| collect(field, value, key, held)),
        Value::Map(entries) => entries.iter().for_each(|entry| {
            if let Some(reference) = &entry.reference {
                held.push(Held {
                    field,
                    key: true,
                    reference,
                });
            }
            collect(field, &entry.value, key, held);
        }),
        _ => {}
    }
}

/// A name as its segments read, outermost first, under an alias
/// namespace when it has one.
fn spelled(alias: Option<&str>, name: &QualifiedName) -> String {
    alias
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
        spelled(declaration.alias.as_deref(), &declaration.identity.name)
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
    let name = format!("{} in {}", reference.written, reference.domain);
    let name = match (&link.field, link.key) {
        (Some(field), true) => format!("{name} as a key of {field}"),
        _ => name,
    };
    let at = Some(reference.site);
    match &link.outcome {
        LinkOutcome::Resolved => Vec::new(),
        LinkOutcome::Unresolved => {
            let name = match link.gated.as_slice() {
                [] => name,
                gated => format!(
                    "{name} (declared only under the alias namespace {})",
                    gated.join(", ")
                ),
            };
            vec![located(record(UNRESOLVED, &[&name], ORIGIN), at, documents)]
        }
        LinkOutcome::Ambiguous => {
            let candidates = link
                .candidates
                .iter()
                .map(|candidate| spelled(candidate.alias.as_deref(), &candidate.identity.name))
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
