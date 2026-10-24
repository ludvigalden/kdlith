//! Enumeration of a definition's types, nodes, and profiles and of the
//! identities its data binds, each listing sorted by its own key.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::{Item, Value};

use crate::data::{DataReader, ReadOptions};
use crate::definition::Definition;
use crate::typed::ReadError;
use crate::types::TypeKind;

/// One declared type: its name, its type parameters, and the kind of
/// declaration it is (`string`, `enum`, `object`, `union`, `map`, or
/// `codec`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct TypeEntry {
    pub name: String,
    pub parameters: Vec<String>,
    pub kind: &'static str,
}

/// One declared node binding: its name, the parents it is admitted
/// under (empty for a document root), its processing role, its bound
/// type, and the profile it belongs to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct NodeEntry {
    pub name: String,
    pub parents: Vec<String>,
    /// Admitted at the root as well as under `parents` (`root=#true`).
    pub root: bool,
    pub role: Option<&'static str>,
    pub ty: Option<String>,
    pub profile: Option<String>,
}

/// One declared profile: its name, the root types it admits (`None`
/// admits every declared root), and the locations rule it declares.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct ProfileEntry {
    pub name: String,
    pub roots: Option<Vec<String>>,
    pub locations: Option<String>,
}

/// The kind of item an index entry names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum IndexKind {
    Declaration,
    Record,
}

impl IndexKind {
    pub fn name(self) -> &'static str {
        match self {
            IndexKind::Declaration => "declaration",
            IndexKind::Record => "record",
        }
    }
}

/// One indexed data item: what it is, the domain its identity lives in
/// (the identity domain of a declaration, the type of a record), its
/// lookup identity, and where it was written. The bound fields are not
/// kept: an index entry names an item, it does not hold it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct IndexEntry {
    pub kind: IndexKind,
    pub domain: String,
    /// The identity a lookup finds the item by; `None` for a record
    /// that binds no `id` string, which no lookup can address.
    pub identity: Option<String>,
    pub path: String,
    /// The 1-based line and column of the item's node, when known.
    pub at: Option<(u32, u32)>,
}

/// The index of one data reading: its sorted entries and every
/// diagnostic the reading produced, in detection order.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ItemIndex {
    pub entries: Vec<IndexEntry>,
    pub records: Vec<DiagnosticRecord>,
}

/// The types `def` declares — built-ins excluded, the same filter the
/// definition summary counts — sorted by name.
pub fn list_types(def: &Definition) -> Vec<TypeEntry> {
    let n = &def.names;
    let mut entries: Vec<TypeEntry> = def
        .types
        .iter()
        .filter(|(_, decl)| decl.site.is_some())
        .map(|(_, decl)| TypeEntry {
            name: n.resolve(decl.name).to_owned(),
            parameters: decl
                .parameters
                .iter()
                .map(|p| n.resolve(*p).to_owned())
                .collect(),
            kind: kind_name(&decl.kind),
        })
        .collect();
    entries.sort();
    entries
}

fn kind_name(kind: &TypeKind) -> &'static str {
    match kind {
        TypeKind::Builtin(_) => "builtin",
        TypeKind::String(_) => "string",
        TypeKind::Enum(_) => "enum",
        TypeKind::Object(_) => "object",
        TypeKind::Union(_) => "union",
        TypeKind::Map(_) => "map",
        TypeKind::Codec(_) => "codec",
    }
}

/// The node bindings `def` declares, sorted by name, then by the
/// sorted parents they are admitted under.
pub fn list_nodes(def: &Definition) -> Vec<NodeEntry> {
    let n = &def.names;
    let mut entries: Vec<NodeEntry> = def
        .bindings
        .iter()
        .map(|(_, node)| {
            let mut parents: Vec<String> = node
                .placement
                .parents()
                .iter()
                .map(|p| n.resolve(*p).to_owned())
                .collect();
            parents.sort();
            parents.dedup();
            NodeEntry {
                name: n.resolve(node.name).to_owned(),
                root: !parents.is_empty() && node.placement.at_root(),
                parents,
                role: node.role.map(|role| role.name()),
                ty: node.ty.map(|ty| n.resolve(ty).to_owned()),
                profile: node.profile.map(|p| n.resolve(p).to_owned()),
            }
        })
        .collect();
    entries.sort();
    entries
}

/// The profiles `def` declares, sorted by name; each profile's root
/// types are sorted too.
pub fn list_profiles(def: &Definition) -> Vec<ProfileEntry> {
    let n = &def.names;
    let mut entries: Vec<ProfileEntry> = def
        .profiles
        .iter()
        .map(|profile| ProfileEntry {
            name: n.resolve(profile.name).to_owned(),
            roots: profile.roots.as_ref().map(|roots| {
                let mut roots: Vec<String> =
                    roots.iter().map(|r| n.resolve(*r).to_owned()).collect();
                roots.sort();
                roots
            }),
            locations: profile.locations.as_ref().map(crate::locations::describe),
        })
        .collect();
    entries.sort();
    entries
}

/// The lookup identity of a record item: the string value of its bound
/// `id` field, the identity a data lookup finds the record by. Records
/// carry no registered identity of their own. A resolution item is
/// found by its target as the contributions wrote it.
pub fn record_identity(item: &Item) -> Option<&str> {
    let record = match item {
        Item::Record(record) => record,
        Item::Resolution(resolved) => return Some(resolved.target.written.as_str()),
        _ => return None,
    };
    record
        .fields
        .iter()
        .find(|field| field.name == "id")
        .and_then(|field| match &field.value {
            Value::String(id) => Some(id.as_str()),
            _ => None,
        })
}

/// Index the data `entries` — and the documents they import — read
/// through `reader`: every bound declaration by its identity and every
/// bound record by its lookup identity, each with the document and
/// position it was written at, sorted. Items stream through the reading
/// one at a time and only the index entry of each is kept, so the bound
/// objects are never collected. The reading always runs to its end;
/// its diagnostics ride in the index for the caller to report.
pub fn index_items(
    reader: &DataReader<'_>,
    entries: &[&Path],
    options: &ReadOptions,
) -> Result<ItemIndex, ReadError> {
    let mut paths: BTreeMap<u32, String> = BTreeMap::new();
    let mut index = ItemIndex::default();
    let mut sink = |item: Item| match &item {
        Item::Document(document) => {
            paths.insert(document.source.0, document.path.clone());
        }
        Item::Diagnostic(record) => index.records.push((**record).clone()),
        Item::Record(record) => {
            let identity = record_identity(&item).map(str::to_owned);
            index.entries.push(IndexEntry {
                kind: IndexKind::Record,
                domain: record.type_name.clone(),
                identity,
                path: path_of(&paths, record.node.source.0),
                at: record
                    .site
                    .map(|site| (site.span.start.line, site.span.start.column)),
            });
        }
        Item::Declaration(declaration) => {
            index.entries.push(IndexEntry {
                kind: IndexKind::Declaration,
                domain: declaration.identity.domain.clone(),
                identity: Some(declaration.identity.name.0.join(".")),
                path: path_of(&paths, declaration.node.source.0),
                at: declaration
                    .site
                    .map(|site| (site.span.start.line, site.span.start.column)),
            });
        }
        _ => {}
    };
    reader.read(entries, options, &mut sink)?;
    index.entries.sort();
    Ok(index)
}

fn path_of(paths: &BTreeMap<u32, String>, source: u32) -> String {
    paths.get(&source).cloned().unwrap_or_default()
}

/// `NAME[<P, ...>] KIND`.
impl fmt::Display for TypeEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)?;
        if !self.parameters.is_empty() {
            write!(f, "<{}>", self.parameters.join(", "))?;
        }
        write!(f, " {}", self.kind)
    }
}

/// `NAME parent=root|[root,]P,... role=ROLE|- type=TYPE|- profile=NAME|-`.
impl fmt::Display for NodeEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parent = match (self.parents.is_empty(), self.root) {
            (true, _) => "root".to_owned(),
            (false, false) => self.parents.join(","),
            (false, true) => format!("root,{}", self.parents.join(",")),
        };
        write!(
            f,
            "{} parent={parent} role={} type={} profile={}",
            self.name,
            self.role.unwrap_or("-"),
            self.ty.as_deref().unwrap_or("-"),
            self.profile.as_deref().unwrap_or("-"),
        )
    }
}

/// `NAME roots=T,...|- locations=[PATTERN,...][ allow-empty]|-`.
impl fmt::Display for ProfileEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let roots = self
            .roots
            .as_ref()
            .map_or_else(|| "-".to_owned(), |roots| roots.join(","));
        write!(
            f,
            "{} roots={roots} locations={}",
            self.name,
            self.locations.as_deref().unwrap_or("-"),
        )
    }
}

/// `KIND DOMAIN IDENTITY|- PATH[:LINE:COLUMN]`.
impl fmt::Display for IndexEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            self.kind.name(),
            self.domain,
            self.identity.as_deref().unwrap_or("-"),
            self.path,
        )?;
        if let Some((line, column)) = self.at {
            write!(f, ":{line}:{column}")?;
        }
        Ok(())
    }
}
