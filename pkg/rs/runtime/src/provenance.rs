//! The provenance envelope an exported record carries: the source
//! revisions its locations refer to, one shared origin table, and the
//! origin of each emitted field. Shared by every consumer that writes
//! bound items for others to trust.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use kdlith_model::{Field, Item, Location, Origin, OriginId, SourceId};
use serde::Serialize;

/// The revision of one source a reading bound: spans are reliable
/// provenance only beside the digest of the text they were measured in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SourceRevision {
    pub source: SourceId,
    pub path: String,
    /// The content digest of the revision, stamped with its algorithm.
    pub revision: Option<String>,
}

/// The source revisions a stream has announced so far, by identity.
#[derive(Clone, Debug, Default)]
pub struct SourceRevisions {
    by_id: BTreeMap<SourceId, SourceRevision>,
}

impl SourceRevisions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Note the revision a document item announces; every other item
    /// passes untouched.
    pub fn observe(&mut self, item: &Item) {
        if let Item::Document(document) = item {
            self.by_id.insert(
                document.source,
                SourceRevision {
                    source: document.source,
                    path: document.path.clone(),
                    revision: document.digest.clone(),
                },
            );
        }
    }

    /// The provenance of a bound record, declaration, or contribution;
    /// `None` for an item that is stream structure rather than data.
    pub fn provenance(&self, item: &Item) -> Option<Provenance> {
        let (site, fields) = match item {
            Item::Record(record) => (record.site, record.fields.as_slice()),
            Item::Declaration(declaration) => (declaration.site, declaration.fields.as_slice()),
            Item::Contribution(contribution) => {
                (contribution.site, contribution.members.as_slice())
            }
            _ => return None,
        };
        let mut table = Table::default();
        let fields = fields
            .iter()
            .map(|field| table.field(field))
            .collect::<Vec<_>>();
        let mut referenced: BTreeSet<SourceId> = table.sources;
        if let Some(site) = site {
            referenced.insert(site.source);
        }
        let sources = referenced
            .into_iter()
            .map(|source| {
                self.by_id.get(&source).cloned().unwrap_or(SourceRevision {
                    source,
                    path: String::new(),
                    revision: None,
                })
            })
            .collect();
        Some(Provenance {
            node: site,
            sources,
            origins: table.origins,
            fields,
        })
    }
}

/// The provenance envelope of one exported item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Provenance {
    /// Where the item's node was written.
    pub node: Option<Location>,
    /// Every source a location of this envelope names, with the
    /// revision its spans were measured in, in source order.
    pub sources: Vec<SourceRevision>,
    /// The origin records the fields share: a derived record's inputs
    /// index this table, and an identical record is stored once.
    pub origins: Vec<Origin>,
    /// The origin of each emitted field, in the item's field order. A
    /// field the output policy omitted is absent here as in the data.
    pub fields: Vec<FieldProvenance>,
}

/// One emitted field's provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct FieldProvenance {
    pub name: String,
    /// The field's own origin in the shared table; absent when untraced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<OriginId>,
    /// The value is a redaction marker standing in for a sensitive
    /// value the output policy withheld.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub redacted: bool,
}

/// One item as an exported line: its serialized form with the
/// provenance envelope beside its data.
pub fn export_line(item: &Item, provenance: Option<&Provenance>) -> serde_json::Value {
    let mut line = serde_json::to_value(item).unwrap_or(serde_json::Value::Null);
    if let (Some(provenance), serde_json::Value::Object(members)) = (provenance, &mut line)
        && let Ok(envelope) = serde_json::to_value(provenance)
    {
        members.insert("provenance".to_owned(), envelope);
    }
    line
}

/// The shared origin table one envelope builds.
#[derive(Default)]
struct Table {
    origins: Vec<Origin>,
    index: HashMap<Origin, OriginId>,
    sources: BTreeSet<SourceId>,
}

impl Table {
    /// Intern a field's trace, returning its provenance entry. A trace
    /// lists its own origin first and derived inputs after it, so the
    /// records intern last to first: every input is in the table before
    /// the record naming it.
    fn field(&mut self, field: &Field) -> FieldProvenance {
        let local = field.trace.origins();
        let mut global: Vec<OriginId> = vec![OriginId(0); local.len()];
        for at in (0..local.len()).rev() {
            let origin = match &local[at] {
                Origin::Derived {
                    operation,
                    location,
                    inputs,
                } => Origin::Derived {
                    operation: *operation,
                    location: *location,
                    inputs: inputs
                        .iter()
                        .filter_map(|OriginId(input)| global.get(*input as usize).copied())
                        .collect(),
                },
                other => other.clone(),
            };
            global[at] = self.intern(origin);
        }
        FieldProvenance {
            name: field.name.clone(),
            origin: global.first().copied(),
            redacted: field.redacted,
        }
    }

    fn intern(&mut self, origin: Origin) -> OriginId {
        if let Some(id) = self.index.get(&origin) {
            return *id;
        }
        if let Some(location) = origin.location() {
            self.sources.insert(location.source);
        }
        let id = OriginId(u32::try_from(self.origins.len()).unwrap_or(u32::MAX));
        self.origins.push(origin.clone());
        self.index.insert(origin, id);
        id
    }
}
