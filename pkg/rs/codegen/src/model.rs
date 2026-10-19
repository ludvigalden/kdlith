//! The emission model: what a compiled definition contributes to each
//! generated file, decided once before any file is written. Emission
//! reads this model, never the definition again.

use std::collections::BTreeMap;

use kdlith_engine::{
    CheckScope, Definition, DefinitionKind, Names, NodeDecl, ReferenceDecl, Role, SlotDecl, Sym,
    TypeDecl, TypeExpr, TypeKind,
};

use crate::Error;
use crate::emit::declaration_expressions;
use crate::render::{
    expression_text, field_name, include_path, inline_union_name, is_identifier, mangle,
    normalize_path, pascal, rust_type, subexpressions, variant_name,
};

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
    /// The inline unions its objects' and node-local types' fields
    /// write, in contribution order.
    pub unions: Vec<InlineUnion>,
}

/// An inline `union<…>` one field's type writes: the enum it renders as,
/// declared beside the field's owner.
pub(crate) struct InlineUnion {
    /// The Rust type owning the field.
    pub owner: String,
    /// The field's key.
    pub key: String,
    /// The enum's name, `<Owner><Field>`.
    pub name: String,
    /// Each member's variant and Rust type, in member order.
    pub members: Vec<(String, String)>,
}

/// One item of the generated processor's `Item` enum: a node declaration
/// with a reading role and the type it reads into.
pub(crate) struct ItemNode<'d> {
    pub node: &'d NodeDecl,
    /// The Rust type the node's items read as: the logical type's name
    /// (a generic instance's mangled), the type its cases select, or a
    /// typeless node's local type.
    pub ty: String,
    /// The type name the node's items carry: the logical type's name,
    /// the type its cases select, or a typeless node's own name.
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
    /// Each monomorphized generic instance's rendered name, with the
    /// Rust name it reads under.
    pub instances: BTreeMap<String, String>,
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
        // declaration is a template, never spelled itself; each of its
        // monomorphized instances is a declaration of its own.
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
                unions: Vec::new(),
            });
        }

        // The item vocabulary: every node a reading role and a type
        // name, across contributing documents in contribution order. A
        // node bound without `type=` reads as a node-local type named
        // after it, which must not collide with a declared type.
        let names = &definition.names;
        // Built-ins are in the type catalog too, so a local type can
        // shadow neither a declared type nor a built-in's Rust spelling.
        // Generic declarations are left out: they are never spelled.
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
        // Monomorphization names an instance by its rendering, which no
        // Rust identifier spells; it reads under the mangled name.
        let instances: BTreeMap<String, String> = declared
            .iter()
            .filter(|name| name.contains('<'))
            .map(|name| ((*name).to_owned(), mangle(name)))
            .collect();
        let mut items = Vec::new();
        for document in &mut documents {
            for node in &document.nodes {
                let role = match node.role {
                    Some(role @ (Role::Declaration | Role::Record)) => role,
                    _ => continue,
                };
                let node_name = names.resolve(node.name).to_owned();
                let selected = case_type(node, definition, &instances)?;
                let (ty, item_name, local) = match (selected, node.ty) {
                    // Binding retypes a case node's items as the type
                    // its cases select, never its own.
                    (Some((ty, rendered)), _) => (ty, rendered, false),
                    (None, Some(ty)) => {
                        let rendered = names.resolve(ty).to_owned();
                        let ty = instances
                            .get(&rendered)
                            .cloned()
                            .unwrap_or_else(|| rendered.clone());
                        (ty, rendered, false)
                    }
                    (None, None) => {
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

        let mut emission = Self {
            names,
            crate_name: crate_name.to_owned(),
            kind: definition.kind,
            name: definition.name.clone(),
            version,
            documents,
            items,
            instances,
        };
        emission.check_spellings()?;
        emission.check_map_keys(definition)?;
        // Every type the crate declares is taken: a generic instance's
        // mangled name or an inline union's enum must not take a
        // declared, node-local, reference, or another instance's name.
        let mut taken: Vec<String> = declared
            .iter()
            .filter(|name| !emission.instances.contains_key(**name))
            .map(|name| (*name).to_owned())
            .collect();
        for document in &emission.documents {
            taken.extend(
                document
                    .locals
                    .iter()
                    .map(|node| pascal(names.resolve(node.name))),
            );
        }
        check_instances(&emission.instances, &mut taken)?;
        for at in 0..emission.documents.len() {
            let doc = &emission.documents[at];
            let unions = inline_unions(doc, names, definition, &emission.instances, &mut taken)?;
            emission.documents[at].unions = unions;
        }
        Ok(emission)
    }

    /// The Rust name the declared type `name` takes: a generic
    /// instance's mangled name, else its own.
    pub(crate) fn rust_name(&self, name: Sym) -> String {
        let name = self.names.resolve(name);
        self.instances
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_owned())
    }

    /// A `map<K, V>` renders as a `BTreeMap`, so its key must order:
    /// binding admits only string and reference keys, so `String`, a
    /// string type, or a reference. Any other key fails closed.
    fn check_map_keys(&self, definition: &Definition) -> Result<(), Error> {
        let names = self.names;
        let orderable = |key: &TypeExpr| match key {
            TypeExpr::Named(name) => {
                names.resolve(*name) == "String"
                    || definition.symbols.reference(*name).is_some()
                    || definition
                        .types
                        .lookup(*name)
                        .and_then(|id| definition.types.get(id))
                        .is_some_and(|decl| matches!(decl.kind, TypeKind::String(_)))
            }
            TypeExpr::Ref(inner) => matches!(inner.as_ref(), TypeExpr::Named(_)),
            _ => false,
        };
        let refuse = |key: &TypeExpr, value: &TypeExpr| Error::UnsupportedExpression {
            spelling: format!(
                "map<{}, {}>",
                expression_text(key, names),
                expression_text(value, names)
            ),
        };
        for doc in &self.documents {
            let mut expressions: Vec<&TypeExpr> = Vec::new();
            for decl in &doc.types {
                if let TypeKind::Map(map) = &decl.kind
                    && !orderable(&map.key)
                {
                    return Err(refuse(&map.key, &map.value));
                }
                expressions.extend(declaration_expressions(decl));
            }
            for node in &doc.locals {
                expressions.extend(node.fields.iter().filter_map(|field| field.ty.as_ref()));
            }
            expressions.extend(doc.slots.iter().map(|slot| &slot.ty));
            for expr in expressions {
                for sub in subexpressions(expr) {
                    if let TypeExpr::Map(key, value) = sub
                        && !orderable(key)
                    {
                        return Err(refuse(key, value));
                    }
                }
            }
        }
        Ok(())
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

/// The Rust and rendered names of the one object type every case and
/// `otherwise` of a dispatching node select; any other dispatch fails
/// closed, since one item reads as one type.
fn case_type(
    node: &NodeDecl,
    definition: &Definition,
    instances: &BTreeMap<String, String>,
) -> Result<Option<(String, String)>, Error> {
    if node.cases.is_empty() {
        return Ok(None);
    }
    let names = &definition.names;
    let mut selected: Vec<&TypeExpr> = Vec::new();
    for ty in node
        .cases
        .iter()
        .map(|case| &case.ty)
        .chain(&node.otherwise)
    {
        if !selected.contains(&ty) {
            selected.push(ty);
        }
    }
    let object = match selected.as_slice() {
        [TypeExpr::Named(name)] => definition
            .types
            .lookup(*name)
            .and_then(|id| definition.types.get(id))
            .filter(|decl| matches!(decl.kind, TypeKind::Object(_)))
            .map(|_| names.resolve(*name)),
        _ => None,
    };
    let Some(rendered) = object else {
        return Err(Error::UnsupportedCaseNode {
            node: names.resolve(node.name).to_owned(),
            types: selected
                .iter()
                .map(|ty| expression_text(ty, names))
                .collect(),
        });
    };
    let ty = instances
        .get(rendered)
        .cloned()
        .unwrap_or_else(|| rendered.to_owned());
    Ok(Some((ty, rendered.to_owned())))
}

/// Check each instance's mangled name is an identifier, shadows nothing,
/// and is not `taken`. `Pair<x-y, z>` and `Pair<x, y-z>` both mangle to
/// `PairXYZ`, so a collision fails closed; a suffix would not be pure.
fn check_instances(
    instances: &BTreeMap<String, String>,
    taken: &mut Vec<String>,
) -> Result<(), Error> {
    for (rendered, name) in instances {
        let reason = if !is_identifier(name) {
            Some("it is no Rust identifier")
        } else if UNQUALIFIED.contains(&name.as_str()) {
            Some(SHADOWS)
        } else if taken.contains(name) {
            Some("it takes the name of another type the generated crate declares")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(Error::UnspellableName {
                what: "generic instance",
                owner: rendered.clone(),
                name: name.clone(),
                reason,
            });
        }
        taken.push(name.clone());
    }
    Ok(())
}

/// Whether a role makes a node an item of the processor's stream.
pub(crate) fn reading_role(role: Option<Role>) -> bool {
    matches!(role, Some(Role::Declaration | Role::Record))
}

/// The inline unions `doc`'s objects and node-local types write, one
/// enum per field, named `<Owner><Field>`. A name already `taken` fails
/// closed, as does a field writing more than one union.
fn inline_unions(
    doc: &Document<'_>,
    names: &Names,
    definition: &Definition,
    instances: &BTreeMap<String, String>,
    taken: &mut Vec<String>,
) -> Result<Vec<InlineUnion>, Error> {
    let mut owned: Vec<(String, Sym, &TypeExpr)> = Vec::new();
    for decl in &doc.types {
        if let TypeKind::Object(object) = &decl.kind {
            let owner = names.resolve(decl.name);
            let owner = instances.get(owner).map_or(owner, String::as_str);
            owned.extend(
                object
                    .fields
                    .iter()
                    .map(|f| (owner.to_owned(), f.name, &f.ty)),
            );
        }
    }
    for node in &doc.locals {
        let owner = pascal(names.resolve(node.name));
        for field in &node.fields {
            if let Some(ty) = &field.ty {
                owned.push((owner.clone(), field.name, ty));
            }
        }
    }
    let mut unions = Vec::new();
    for (owner, key, ty) in owned {
        let found: Vec<&Vec<TypeExpr>> = subexpressions(ty)
            .into_iter()
            .filter_map(|sub| match sub {
                TypeExpr::Union(members) => Some(members),
                _ => None,
            })
            .collect();
        let members = match found.as_slice() {
            [] => continue,
            [members] => *members,
            _ => {
                return Err(Error::UnsupportedExpression {
                    spelling: expression_text(ty, names),
                });
            }
        };
        let key = names.resolve(key);
        let name = inline_union_name(&owner, key);
        let reason = if !is_identifier(&name) {
            Some("it is no Rust identifier")
        } else if UNQUALIFIED.contains(&name.as_str()) {
            Some(SHADOWS)
        } else if taken.contains(&name) {
            Some("it takes the name of another type the generated crate declares")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(Error::UnspellableName {
                what: "inline union",
                owner: format!("{owner}.{key}"),
                name,
                reason,
            });
        }
        let renderings = members
            .iter()
            .map(|member| rust_type(member, names, instances, None))
            .collect::<Result<Vec<_>, _>>()?;
        variants(
            "union member",
            &name,
            renderings.iter().map(String::as_str),
            variant_name,
        )?;
        // The binder keeps the bound value, not the member it fit, so
        // reading re-selects the first member that fits; that agrees
        // with binding only when no two members accept one value shape.
        let mut seen = 0;
        for member in members {
            let accepted = shapes(definition, member);
            if accepted & seen != 0 {
                let union = TypeExpr::Union(members.clone());
                return Err(Error::UnsupportedExpression {
                    spelling: expression_text(&union, names),
                });
            }
            seen |= accepted;
        }
        taken.push(name.clone());
        unions.push(InlineUnion {
            owner,
            key: key.to_owned(),
            name,
            members: renderings
                .into_iter()
                .map(|rendering| (variant_name(&rendering), rendering))
                .collect(),
        });
    }
    Ok(unions)
}

const STRING: u8 = 1;
const BOOL: u8 = 1 << 1;
const NUMBER: u8 = 1 << 2;
const NULL: u8 = 1 << 3;
const REFERENCE: u8 = 1 << 4;
const LIST: u8 = 1 << 5;
const MAP: u8 = 1 << 6;
const ANY: u8 = STRING | BOOL | NUMBER | NULL | REFERENCE | LIST | MAP;

/// The bound value shapes a type's reading accepts: binding has already
/// decoded a reference type's or `ref<T>`'s spelling into a reference,
/// and a codec's into its output.
fn shapes(definition: &Definition, ty: &TypeExpr) -> u8 {
    match ty {
        TypeExpr::Named(name) => match definition.names.resolve(*name) {
            "String" => STRING,
            "Bool" => BOOL,
            "U64" | "Integer" | "Decimal" => NUMBER,
            "ValueExpr" => STRING | BOOL | NUMBER | NULL,
            _ if definition.symbols.reference(*name).is_some() => REFERENCE,
            _ => {
                let decl = definition
                    .types
                    .lookup(*name)
                    .and_then(|id| definition.types.get(id));
                match decl.map(|decl| &decl.kind) {
                    Some(TypeKind::String(_) | TypeKind::Enum(_)) => STRING,
                    Some(TypeKind::Object(_) | TypeKind::Map(_)) => MAP,
                    Some(TypeKind::Union(_)) => STRING | MAP,
                    Some(TypeKind::Codec(codec)) => shapes(definition, &codec.output),
                    _ => ANY,
                }
            }
        },
        TypeExpr::Optional(inner) => shapes(definition, inner),
        TypeExpr::Nullable(inner) => NULL | shapes(definition, inner),
        TypeExpr::List(_) => LIST,
        TypeExpr::Map(_, _) | TypeExpr::OrderedMap(_, _) => MAP,
        TypeExpr::Ref(_) => REFERENCE,
        TypeExpr::Union(members) => members
            .iter()
            .fold(0, |all, member| all | shapes(definition, member)),
        _ => ANY,
    }
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
/// types: the Rust prelude items its renderings use, `Self`, the engine
/// items its modules import, and the standard library's `BTreeMap`. A
/// declared or node-local type taking one would shadow it, so the crate
/// would not compile.
const UNQUALIFIED: [&str; 27] = [
    "BTreeMap",
    "Decimal",
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
    "Nullable",
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
