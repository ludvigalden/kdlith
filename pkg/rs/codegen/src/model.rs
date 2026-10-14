//! The emission model: what a compiled definition contributes to each
//! generated file, decided once before any file is written. Emission
//! reads this model, never the definition again.

use std::collections::BTreeMap;

use kdlith_engine::{
    CheckScope, Definition, DefinitionKind, Names, NodeDecl, Role, SlotDecl, TypeDecl, TypeKind,
};

use crate::Error;
use crate::render::{include_path, normalize_path};

/// One contributing document and what it declares.
pub(crate) struct Document<'d> {
    /// The document's path as the reader showed it, lexically
    /// normalized.
    pub path: String,
    /// The document's file name, e.g. `types.kdl`.
    pub file_name: String,
    /// The module the document's per-document files take.
    pub module: String,
    /// The document's declared types, in contribution order.
    pub types: Vec<&'d TypeDecl>,
    /// The document's node declarations, in contribution order.
    pub nodes: Vec<&'d NodeDecl>,
    /// The document's context slots, in contribution order.
    pub slots: Vec<&'d SlotDecl>,
}

/// One item of the generated processor's `Item` enum: a node declaration
/// with a reading role and the type it reads into.
pub(crate) struct ItemNode<'d> {
    pub node: &'d NodeDecl,
    /// The logical type's name.
    pub ty: String,
    /// Whether the node's role is record rather than declaration.
    pub record: bool,
}

/// The model every emitted file is rendered from.
pub(crate) struct Emission<'d> {
    /// The names table every symbol resolves through.
    pub names: &'d Names,
    /// The package name of the generated crate.
    pub crate_name: String,
    /// Which authoring level the definition belongs to.
    pub kind: DefinitionKind,
    /// The definition's declared name.
    pub name: String,
    /// The definition's declared version.
    pub version: String,
    /// Every contributing document, in contribution order.
    pub documents: Vec<Document<'d>>,
    /// Every item node, in contribution order across documents.
    pub items: Vec<ItemNode<'d>>,
}

impl<'d> Emission<'d> {
    pub(crate) fn new(definition: &'d Definition, crate_name: &str) -> Result<Self, Error> {
        if crate_name.is_empty()
            || !crate_name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
            || crate_name.starts_with('-')
            || crate_name.ends_with('-')
        {
            return Err(Error::InvalidCrateName {
                name: crate_name.to_owned(),
            });
        }
        if definition.scope != CheckScope::Complete {
            return Err(Error::IncompleteScope {
                scope: definition.scope,
            });
        }
        let Some(version) = definition.version.clone() else {
            return Err(Error::MissingVersion);
        };

        // A document contributes when it declared anything with a site;
        // the entrypoint that only imports contributes nothing and is
        // not embedded in the seed.
        let mut contributing = vec![false; definition.documents.len()];
        let mark = |contributing: &mut Vec<bool>, site: kdlith_engine::Site| {
            if let Some(flag) = contributing.get_mut(site.document) {
                *flag = true;
            }
        };
        for (_, decl) in definition.types.iter() {
            if let Some(site) = decl.site {
                mark(&mut contributing, site);
            }
        }
        for (_, node) in definition.bindings.iter() {
            mark(&mut contributing, node.site);
        }
        for slot in &definition.slots {
            mark(&mut contributing, slot.site);
        }
        for profile in &definition.profiles {
            mark(&mut contributing, profile.site);
        }
        for discovery in &definition.discoveries {
            mark(&mut contributing, discovery.site);
            for anchor in &discovery.anchors {
                mark(&mut contributing, anchor.site);
            }
        }
        for reference in &definition.references {
            mark(&mut contributing, reference.site);
        }
        for provider in &definition.providers {
            mark(&mut contributing, provider.site);
        }
        if let Some(resolution) = &definition.resolution {
            mark(&mut contributing, resolution.site);
        }
        if let Some(rule) = &definition.document_rule {
            mark(&mut contributing, rule.site);
        }
        if let Some(context) = &definition.source_context {
            mark(&mut contributing, context.site);
        }

        // Group each catalog's declarations under their document, in
        // contribution order: catalog iteration order is declaration
        // order, so per-document grouping preserves it.
        let mut type_groups: BTreeMap<usize, Vec<&TypeDecl>> = BTreeMap::new();
        for (_, decl) in definition.types.iter() {
            if let Some(site) = decl.site {
                type_groups.entry(site.document).or_default().push(decl);
            }
        }
        let mut node_groups: BTreeMap<usize, Vec<&NodeDecl>> = BTreeMap::new();
        for (_, node) in definition.bindings.iter() {
            node_groups
                .entry(node.site.document)
                .or_default()
                .push(node);
        }
        let mut slot_groups: BTreeMap<usize, Vec<&SlotDecl>> = BTreeMap::new();
        for slot in &definition.slots {
            slot_groups
                .entry(slot.site.document)
                .or_default()
                .push(slot);
        }

        let mut documents = Vec::new();
        let mut modules: BTreeMap<String, String> = BTreeMap::new();
        for (at, path) in definition.documents.iter().enumerate() {
            if !contributing.get(at).copied().unwrap_or(false) {
                continue;
            }
            let path = normalize_path(path);
            let file_name = path.rsplit('/').next().unwrap_or(&path).to_owned();
            let stem = file_name.strip_suffix(".kdl").unwrap_or(&file_name);
            let module = format!("{}_kdl", stem);
            if let Some(first) = modules.get(&module) {
                return Err(Error::ModuleCollision {
                    first: first.clone(),
                    second: path.clone(),
                    module,
                });
            }
            modules.insert(module.clone(), path.clone());
            documents.push(Document {
                path,
                file_name,
                module,
                types: type_groups.remove(&at).unwrap_or_default(),
                nodes: node_groups.remove(&at).unwrap_or_default(),
                slots: slot_groups.remove(&at).unwrap_or_default(),
            });
        }

        // The item vocabulary: every node a reading role and a type
        // name, across contributing documents in contribution order.
        let names = &definition.names;
        let mut items = Vec::new();
        let mut bound: BTreeMap<String, String> = BTreeMap::new();
        for document in &documents {
            for node in &document.nodes {
                let role = match node.role {
                    Some(role @ (Role::Declaration | Role::Record)) => role,
                    _ => continue,
                };
                let Some(ty) = node.ty else {
                    return Err(Error::UnsupportedItem {
                        name: names.resolve(node.name).to_owned(),
                    });
                };
                let ty = names.resolve(ty).to_owned();
                if let Some(first) = bound.get(&ty) {
                    return Err(Error::DuplicateItemBinding {
                        ty: ty.clone(),
                        first: first.clone(),
                        second: names.resolve(node.name).to_owned(),
                    });
                }
                bound.insert(ty.clone(), names.resolve(node.name).to_owned());
                items.push(ItemNode {
                    node,
                    ty,
                    record: role == Role::Record,
                });
            }
        }

        if items.is_empty() {
            return Err(Error::NoItems);
        }

        Ok(Self {
            names,
            crate_name: crate_name.to_owned(),
            kind: definition.kind,
            name: definition.name.clone(),
            version,
            documents,
            items,
        })
    }

    /// The documents that declared types, in contribution order.
    pub(crate) fn type_documents(&self) -> impl Iterator<Item = &Document<'d>> + '_ {
        self.documents.iter().filter(|doc| !doc.types.is_empty())
    }

    /// The documents that declared nodes, in contribution order.
    pub(crate) fn node_documents(&self) -> impl Iterator<Item = &Document<'d>> + '_ {
        self.documents.iter().filter(|doc| !doc.nodes.is_empty())
    }

    /// The documents that declared context slots, in contribution order.
    pub(crate) fn slot_documents(&self) -> impl Iterator<Item = &Document<'d>> + '_ {
        self.documents.iter().filter(|doc| !doc.slots.is_empty())
    }

    /// The documents that declared item nodes, in contribution order.
    pub(crate) fn item_documents(&self) -> impl Iterator<Item = &Document<'d>> + '_ {
        self.documents
            .iter()
            .filter(|doc| doc.nodes.iter().any(|node| reading_role(node.role)))
    }

    /// The documents the seed embeds: every contributing document, whose
    /// include paths the pinned layout spells.
    pub(crate) fn seed_documents(
        &self,
    ) -> impl Iterator<Item = Result<(String, String), Error>> + '_ {
        self.documents
            .iter()
            .map(|doc| Ok((doc.file_name.clone(), include_path(&doc.path)?)))
    }
}

/// Whether a role makes a node an item of the processor's stream.
pub(crate) fn reading_role(role: Option<Role>) -> bool {
    matches!(role, Some(Role::Declaration | Role::Record))
}

/// The kind name a type declaration renders as.
pub(crate) fn kind_name(kind: &TypeKind) -> &'static str {
    match kind {
        TypeKind::Builtin(_) => "builtin",
        TypeKind::String(_) => "string",
        TypeKind::Enum(_) => "enum",
        TypeKind::Object(_) => "object",
        TypeKind::Union(_) => "union",
        TypeKind::Map(_) => "map",
        TypeKind::Codec(_) => "codec",
        _ => "declared",
    }
}
