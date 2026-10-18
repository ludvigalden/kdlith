//! The emission model: what a compiled definition contributes to each
//! generated file, decided once before any file is written. Emission
//! reads this model, never the definition again.

use std::collections::BTreeMap;

use kdlith_engine::{
    CheckScope, Definition, DefinitionKind, Names, NodeDecl, ReferenceDecl, Role, SlotDecl,
    TypeDecl, TypeKind,
};

use crate::Error;
use crate::render::{field_name, include_path, is_identifier, normalize_path, pascal};

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
    /// The document's reference types, in contribution order.
    pub references: Vec<&'d ReferenceDecl>,
    /// The document's node declarations, in contribution order.
    pub nodes: Vec<&'d NodeDecl>,
    /// The document's context slots, in contribution order.
    pub slots: Vec<&'d SlotDecl>,
    /// The document's item nodes bound without `type=`, each read as a
    /// node-local Rust type built from its inline field types.
    pub locals: Vec<&'d NodeDecl>,
}

/// One item of the generated processor's `Item` enum: a node declaration
/// with a reading role and the type it reads into.
pub(crate) struct ItemNode<'d> {
    pub node: &'d NodeDecl,
    /// The Rust type the node's items read as: the logical type's name,
    /// or a typeless node's local type.
    pub ty: String,
    /// The type name the node's items carry: the logical type's name,
    /// or a typeless node's own name.
    pub item_name: String,
    /// Whether the node's role is record rather than declaration.
    pub record: bool,
    /// Whether the node is bound without `type=`, so its Rust type is
    /// node-local rather than a declared logical type.
    pub local: bool,
    /// The `Item` variant the node's items read as: the type's name, or
    /// the node's own name when the type backs several nodes.
    pub variant: String,
    /// Whether the variant is keyed by the node rather than the type, so
    /// the node's handler is registered for the node alone.
    pub keyed: bool,
    /// Whether this node is the first to spell its variant, and so the
    /// one that declares it in the `Item` enum.
    pub declares_variant: bool,
    /// Whether this node is the first to need its handler: same-named
    /// nodes under different parents share one.
    pub declares_handler: bool,
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
        // order, so per-document grouping preserves it. A generic
        // declaration is a template, never spelled itself; its
        // monomorphized instances carry the declarations.
        let mut type_groups: BTreeMap<usize, Vec<&TypeDecl>> = BTreeMap::new();
        for (_, decl) in definition.types.iter() {
            if !decl.parameters.is_empty() {
                continue;
            }
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

        let mut reference_groups: BTreeMap<usize, Vec<&ReferenceDecl>> = BTreeMap::new();
        for reference in &definition.references {
            reference_groups
                .entry(reference.site.document)
                .or_default()
                .push(reference);
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
                references: reference_groups.remove(&at).unwrap_or_default(),
                nodes: node_groups.remove(&at).unwrap_or_default(),
                slots: slot_groups.remove(&at).unwrap_or_default(),
                locals: Vec::new(),
            });
        }

        // The item vocabulary: every node a reading role and a type
        // name, across contributing documents in contribution order. A
        // node bound without `type=` reads as a node-local type named
        // after it, which must not collide with a declared type.
        let names = &definition.names;
        // Built-ins are in the type catalog too, so a local type can
        // shadow neither a declared type nor a built-in's Rust spelling.
        let declared: Vec<&str> = definition
            .types
            .iter()
            .filter(|(_, decl)| decl.parameters.is_empty())
            .map(|(_, decl)| names.resolve(decl.name))
            .chain(
                definition
                    .references
                    .iter()
                    .map(|decl| names.resolve(decl.name)),
            )
            .chain(["Item", "Ref", "OrderedMap"])
            .collect();
        let mut items = Vec::new();
        for document in &mut documents {
            for node in &document.nodes {
                let role = match node.role {
                    Some(role @ (Role::Declaration | Role::Record)) => role,
                    _ => continue,
                };
                let node_name = names.resolve(node.name).to_owned();
                let (ty, item_name, local) = match node.ty {
                    Some(ty) => {
                        let ty = names.resolve(ty).to_owned();
                        (ty.clone(), ty, false)
                    }
                    None => {
                        let ty = pascal(&node_name);
                        if declared.contains(&ty.as_str()) {
                            return Err(Error::LocalTypeCollision {
                                node: node_name,
                                ty,
                            });
                        }
                        if UNQUALIFIED.contains(&ty.as_str()) {
                            return Err(Error::UnspellableName {
                                what: "node-local type",
                                owner: node_name,
                                name: ty,
                                reason: SHADOWS,
                            });
                        }
                        (ty, node_name.clone(), true)
                    }
                };
                if local {
                    document.locals.push(node);
                }
                items.push(ItemNode {
                    node,
                    variant: ty.clone(),
                    ty,
                    item_name,
                    record: role == Role::Record,
                    local,
                    keyed: false,
                    declares_variant: true,
                    declares_handler: true,
                });
            }
        }

        if items.is_empty() {
            return Err(Error::NoItems);
        }
        key_variants(&mut items, names, definition.kind)?;

        let emission = Self {
            names,
            crate_name: crate_name.to_owned(),
            kind: definition.kind,
            name: definition.name.clone(),
            version,
            documents,
            items,
        };
        emission.check_spellings()?;
        Ok(emission)
    }

    /// Every name the crate spells as a Rust identifier must have one:
    /// the compiler admits names Rust cannot spell, and a crate that
    /// cannot compile must fail generation, not the consumer's build.
    fn check_spellings(&self) -> Result<(), Error> {
        let unspellable = |what, owner: &str, name: &str, reason| Error::UnspellableName {
            what,
            owner: owner.to_owned(),
            name: name.to_owned(),
            reason,
        };
        for doc in &self.documents {
            if !is_identifier(&doc.module) {
                return Err(unspellable(
                    "module",
                    &doc.path,
                    &doc.module,
                    "it is no Rust identifier",
                ));
            }
            for decl in &doc.types {
                let name = self.names.resolve(decl.name);
                if UNQUALIFIED.contains(&name) {
                    return Err(unspellable("type", &doc.file_name, name, SHADOWS));
                }
                // Only a monomorphized instance renders as `Head<Args>`.
                if let Some((generic, _)) = name.split_once('<') {
                    return Err(Error::UnsupportedGeneric {
                        name: name.to_owned(),
                        document: doc.file_name.clone(),
                        generic: generic.trim().to_owned(),
                    });
                }
                match &decl.kind {
                    TypeKind::Enum(enumeration) => {
                        let members = enumeration.members.iter().map(|m| m.name.as_str());
                        variants("member", name, members, pascal)?;
                    }
                    TypeKind::Union(union) => {
                        let discriminators = union
                            .variants
                            .iter()
                            .map(|variant| self.names.resolve(variant.discriminator));
                        variants("variant", name, discriminators, pascal)?;
                    }
                    TypeKind::Object(object) => {
                        let keys = object.fields.iter().map(|f| self.names.resolve(f.name));
                        variants("field", name, keys, field_name)?;
                    }
                    _ => {}
                }
            }
            for decl in &doc.references {
                let name = self.names.resolve(decl.name);
                if !is_identifier(name) {
                    return Err(unspellable(
                        "reference type",
                        &doc.file_name,
                        name,
                        "it is no Rust identifier",
                    ));
                }
                if UNQUALIFIED.contains(&name) {
                    return Err(unspellable("reference type", &doc.file_name, name, SHADOWS));
                }
            }
            for node in &doc.locals {
                let name = self.names.resolve(node.name);
                let keys = node.fields.iter().map(|f| self.names.resolve(f.name));
                variants("field", name, keys, field_name)?;
            }
        }
        Ok(())
    }

    /// The documents that declared types, reference types, or typeless
    /// item nodes, in contribution order.
    pub(crate) fn type_documents(&self) -> impl Iterator<Item = &Document<'d>> + '_ {
        self.documents.iter().filter(|doc| {
            !doc.types.is_empty() || !doc.references.is_empty() || !doc.locals.is_empty()
        })
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

/// Name each item's `Item` variant: after its type, unless differently
/// named nodes of a schema bind the type, when each node keys its own
/// variant. Two nodes spelling one variant for different items fail.
fn key_variants(
    items: &mut [ItemNode<'_>],
    names: &Names,
    kind: DefinitionKind,
) -> Result<(), Error> {
    let node_name = |item: &ItemNode<'_>| names.resolve(item.node.name);
    let mut shared: Vec<&str> = Vec::new();
    if kind == DefinitionKind::Schema {
        for item in items.iter() {
            let several = items
                .iter()
                .any(|other| other.ty == item.ty && node_name(other) != node_name(item));
            if several && !shared.contains(&item.ty.as_str()) {
                shared.push(&item.ty);
            }
        }
    }
    let shared: Vec<String> = shared.into_iter().map(str::to_owned).collect();

    // Each variant, with the node name and type of the items it reads.
    let mut spelled: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut handlers: Vec<(bool, String, Option<String>)> = Vec::new();
    for item in items.iter_mut() {
        let node = node_name(item).to_owned();
        if shared.contains(&item.ty) {
            item.keyed = true;
            item.variant = pascal(&node);
            if !is_identifier(&item.variant) || item.variant == "Self" {
                return Err(Error::UnspellableName {
                    what: "item variant",
                    owner: node,
                    name: item.variant.clone(),
                    reason: "it is no Rust identifier",
                });
            }
        }
        match spelled.get(&item.variant) {
            // A same-named node under another parent reads the same
            // items, unless its items are its own node-local type.
            Some((first, ty)) if *first == node && *ty == item.ty && !item.local => {
                item.declares_variant = false;
            }
            Some((first, _)) => {
                return Err(Error::ItemVariantCollision {
                    variant: item.variant.clone(),
                    node,
                    other: first.clone(),
                });
            }
            None => {
                spelled.insert(item.variant.clone(), (node.clone(), item.ty.clone()));
            }
        }
        let handler = (item.record, item.ty.clone(), item.keyed.then_some(node));
        if handlers.contains(&handler) {
            item.declares_handler = false;
        } else {
            handlers.push(handler);
        }
    }
    Ok(())
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

/// The names a generated crate spells unqualified beside the declared
/// types: the Rust prelude items its renderings use, `Self`, and the
/// engine items its modules import. A declared or node-local type
/// taking one would shadow it, so the crate would not compile.
const UNQUALIFIED: [&str; 24] = [
    "Declarations",
    "Definition",
    "DefinitionKind",
    "DiagnosticRecord",
    "Err",
    "Handler",
    "Item",
    "LogicalType",
    "NodeShape",
    "None",
    "Ok",
    "Option",
    "OrderedMap",
    "Ref",
    "Result",
    "Self",
    "Shaped",
    "SharedContributionResolver",
    "SharedResolver",
    "SlotShape",
    "Some",
    "TypeShape",
    "TypedReader",
    "Vec",
];

const SHADOWS: &str = "it shadows a name the generated crate spells unqualified";

/// Check that each of `owner`'s `what`s spells as a distinct Rust
/// identifier under `spell`.
fn variants<'n>(
    what: &'static str,
    owner: &str,
    names: impl Iterator<Item = &'n str>,
    spell: fn(&str) -> String,
) -> Result<(), Error> {
    let mut spelled: BTreeMap<String, &str> = BTreeMap::new();
    for name in names {
        let rust = spell(name);
        let bare = rust.strip_prefix("r#").unwrap_or(&rust);
        let raw_forbidden = matches!(bare, "Self" | "crate" | "self" | "super");
        let reason = if !is_identifier(bare) || raw_forbidden {
            Some("it is no Rust identifier")
        } else if spelled.insert(rust.clone(), name).is_some() {
            Some("another one in the same declaration spells the same identifier")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(Error::UnspellableName {
                what,
                owner: owner.to_owned(),
                name: name.to_owned(),
                reason,
            });
        }
    }
    Ok(())
}
