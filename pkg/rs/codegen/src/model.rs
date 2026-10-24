//! The emission model: what a compiled definition contributes to each
//! generated file, decided once before any file is written. Emission
//! reads this model, never the definition again.

use std::collections::BTreeMap;

use kdlith_engine::{
    Builtin, CheckScope, Definition, DefinitionKind, Names, NodeDecl, ReferenceDecl, Role,
    SlotDecl, Sym, TypeDecl, TypeExpr, TypeKind,
};

use crate::emit::{declaration_expressions, placement_parents};
use crate::render::{
    Spelling, field_name, include_path, inline_union_name, is_identifier, mangle, normalize_path,
    pascal, rust_type, subexpressions, variant_name,
};
use crate::{Error, Packaging};

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
    /// The identity domain this declaration registers in, mirroring the
    /// symbol catalog's descriptor; records register none.
    pub domain: Option<&'d str>,
    /// The identity fields this declaration composes its symbol from,
    /// mirroring the symbol catalog's descriptor; records compose none.
    pub identity_fields: &'d [Sym],
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
    /// Every type a typed contribution node contributes as, in
    /// contribution order: the variants of the contribution enum.
    pub contributions: Vec<String>,
    /// The processor's contribution enum, when the definition declares
    /// typed contribution nodes.
    pub contribution_enum: Option<&'static str>,
    /// Each monomorphized generic instance's rendered name, with the
    /// Rust name it reads under.
    pub instances: BTreeMap<String, String>,
    /// How the generated files spell the names they do not declare.
    pub spelling: Spelling,
    /// The processor's item enum: `Item`, or `ItemKind` when a type the
    /// crate declares takes `Item`.
    pub item_enum: &'static str,
    /// How the manifest names the kdlith crates it depends on.
    pub packaging: Packaging,
    /// One public constant per annotation fact a node declaration
    /// carries, in contribution order.
    pub annotations: Vec<AnnotationConst>,
}

/// One annotation fact as the generated crate exposes it: a public
/// constant named after the node and the annotation, typed by the
/// annotation's declared scalar type.
pub(crate) struct AnnotationConst {
    /// The constant's name: `<NODE>_<ANNOTATION>`, upper snake case,
    /// with an ordinal suffix for a same-named declaration's later facts.
    pub name: String,
    pub node: String,
    pub annotation: String,
    /// The declaration's parent names, recorded only when same-named
    /// declarations share the stem and the doc comment must tell them
    /// apart; empty for a root declaration.
    pub placement: Option<Vec<String>>,
    /// The Rust type the constant holds.
    pub ty: String,
    /// The constant's value, as a Rust expression.
    pub value: String,
}

impl<'d> Emission<'d> {
    pub(crate) fn new(
        definition: &'d Definition,
        crate_name: &str,
        packaging: Packaging,
    ) -> Result<Self, Error> {
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
        if let Some(locations) = &definition.locations {
            mark(&mut contributing, locations.site);
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

        // Catalog iteration order is declaration order, so grouping by
        // document keeps contribution order. A generic declaration is a
        // template, never spelled; each instance is its own declaration.
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
            .collect();
        // Monomorphization names an instance by its rendering, which no
        // Rust identifier spells; it reads under the mangled name.
        let instances: BTreeMap<String, String> = declared
            .iter()
            .filter(|name| name.contains('<'))
            .map(|name| ((*name).to_owned(), mangle(name)))
            .collect();
        let mut items = Vec::new();
        let mut contributions: Vec<String> = Vec::new();
        let mut contributes = false;
        // The symbol catalog's descriptor of a declaring node.
        let descriptor = |node: &NodeDecl| {
            definition
                .bindings
                .iter()
                .find(|(_, bound)| std::ptr::eq(*bound, node))
                .and_then(|(id, _)| definition.symbols.declaration(id))
        };
        // The identity domain a declaring node registers in, and the
        // fields its declarations compose their symbol from, as the
        // descriptor spells them.
        let declaration_identity = |node: &NodeDecl| {
            descriptor(node).map_or((None, &[][..]), |found| {
                (Some(names.resolve(found.domain)), &found.identity_fields)
            })
        };
        for document in &mut documents {
            for node in &document.nodes {
                let role = match node.role {
                    Some(role @ (Role::Declaration | Role::Record)) => role,
                    Some(Role::Contribution) => {
                        contributes = true;
                        // A typed contribution names the type its value
                        // reads as; an untyped one keeps its raw record.
                        if let Some(ty) = node.ty {
                            let rendered = names.resolve(ty).to_owned();
                            let ty = instances
                                .get(&rendered)
                                .cloned()
                                .unwrap_or_else(|| rendered.clone());
                            if !contributions.contains(&ty) {
                                contributions.push(ty);
                            }
                        }
                        continue;
                    }
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
                        if let Some(reason) = unspellable_name(definition.kind, &ty) {
                            return Err(Error::UnspellableName {
                                what: "node-local type",
                                owner: node_name,
                                name: ty,
                                reason,
                            });
                        }
                        (ty, node_name.clone(), true)
                    }
                };
                if local {
                    document.locals.push(node);
                }
                let (domain, identity_fields) = if role == Role::Declaration {
                    declaration_identity(node)
                } else {
                    (None, &[][..])
                };
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
                    domain,
                    identity_fields,
                });
            }
        }

        // Contributions stream as contribution items, never as `Item`
        // variants, so a contribution-only schema reads an empty `Item`.
        if items.is_empty() && !contributes {
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
            contributions,
            contribution_enum: None,
            instances,
            spelling: Spelling::of(definition.kind),
            item_enum: "Item",
            packaging,
            annotations: annotation_constants(definition)?,
        };
        emission.check_spellings()?;
        emission.check_map_keys(definition)?;
        emission.check_recursion(definition)?;
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
        check_instances(definition.kind, &emission.instances, &mut taken)?;
        for at in 0..emission.documents.len() {
            let doc = &emission.documents[at];
            let unions = inline_unions(doc, &emission, definition, &mut taken)?;
            emission.documents[at].unions = unions;
        }
        emission.item_enum = item_enum(&definition.name, &taken)?;
        if !emission.contributions.is_empty() {
            emission.contribution_enum = Some(contribution_enum(&definition.name, &taken)?);
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
            spelling: format!("map<{}, {}>", key.render(names), value.render(names)),
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

    /// Refuse a definition whose rendered types form a cycle with no
    /// Rust indirection: the emitted crate cannot size such a type. A
    /// reference type renders as a `Ref` newtype, never a by-value
    /// payload, so it joins neither side of an edge.
    fn check_recursion(&self, definition: &Definition) -> Result<(), Error> {
        let names = self.names;
        let mut nodes: Vec<String> = Vec::new();
        let mut edges: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        let mut admit = |owner: String, found: Vec<(String, String)>| {
            nodes.push(owner.clone());
            edges.insert(owner, found);
        };
        for doc in &self.documents {
            for decl in &doc.types {
                let owner = self.rust_name(decl.name);
                let mut found = Vec::new();
                for (key, ty) in declaration_sites(decl, names) {
                    direct_edges(
                        ty,
                        &owner,
                        &key,
                        names,
                        definition,
                        &self.instances,
                        &mut found,
                    );
                }
                admit(owner, found);
            }
            for node in &doc.locals {
                let owner = pascal(names.resolve(node.name));
                let mut found = Vec::new();
                for field in &node.fields {
                    if let Some(ty) = &field.ty {
                        direct_edges(
                            ty,
                            &owner,
                            names.resolve(field.name),
                            names,
                            definition,
                            &self.instances,
                            &mut found,
                        );
                    }
                }
                admit(owner, found);
            }
        }
        match find_cycle(&nodes, &edges) {
            Some((types, edge)) => Err(Error::RecursiveType { types, edge }),
            None => Ok(()),
        }
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
                // A generic instance's mangled name is checked with the
                // other instances'.
                if !self.instances.contains_key(name)
                    && let Some(reason) = unspellable_name(self.kind, name)
                {
                    return Err(unspellable("type", &doc.file_name, name, reason));
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
                if let Some(reason) = unspellable_name(self.kind, name) {
                    return Err(unspellable("reference type", &doc.file_name, name, reason));
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

/// The type expressions a declaration holds by value, each with the
/// site it sits at: an object's fields, a union's variants, a map's key
/// and value, a codec's output.
fn declaration_sites<'a>(decl: &'a TypeDecl, names: &Names) -> Vec<(String, &'a TypeExpr)> {
    match &decl.kind {
        TypeKind::Object(object) => object
            .fields
            .iter()
            .map(|field| (names.resolve(field.name).to_owned(), &field.ty))
            .collect(),
        TypeKind::Union(union) => union
            .variants
            .iter()
            .map(|variant| (names.resolve(variant.discriminator).to_owned(), &variant.ty))
            .collect(),
        TypeKind::Map(map) => vec![
            ("key".to_owned(), &map.key),
            ("value".to_owned(), &map.value),
        ],
        TypeKind::Codec(codec) => vec![("output".to_owned(), &codec.output)],
        _ => Vec::new(),
    }
}

/// The by-value edges the expression at `owner`'s site `key` draws: a
/// named declared type reached through no indirection. `Vec`, the
/// maps, and `Ref` sever the chain below them; an inline union holds
/// its members by value, so they draw the owner's edges.
fn direct_edges(
    expr: &TypeExpr,
    owner: &str,
    key: &str,
    names: &Names,
    definition: &Definition,
    instances: &BTreeMap<String, String>,
    edges: &mut Vec<(String, String)>,
) {
    match expr {
        TypeExpr::Named(sym) => {
            let declared = definition
                .types
                .lookup(*sym)
                .and_then(|id| definition.types.get(id));
            let carries_payload = |decl: &TypeDecl| {
                decl.parameters.is_empty()
                    && matches!(
                        decl.kind,
                        TypeKind::Object(_)
                            | TypeKind::Union(_)
                            | TypeKind::Map(_)
                            | TypeKind::Codec(_)
                    )
            };
            if declared.is_some_and(carries_payload) {
                let name = names.resolve(*sym);
                let rendered = instances
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| name.to_owned());
                edges.push((rendered, format!("{owner}.{key}")));
            }
        }
        TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) => {
            direct_edges(inner, owner, key, names, definition, instances, edges);
        }
        TypeExpr::Union(members) => {
            for member in members {
                direct_edges(member, owner, key, names, definition, instances, edges);
            }
        }
        _ => {}
    }
}

const WHITE: u8 = 0;
const GRAY: u8 = 1;
const BLACK: u8 = 2;

/// The first cycle a depth-first walk of `nodes` under `edges` finds,
/// as the types it visits in order — closing back on its first — and
/// the edge that closes it. Node and edge order is contribution order,
/// so the refusal is byte-stable.
fn find_cycle(
    nodes: &[String],
    edges: &BTreeMap<String, Vec<(String, String)>>,
) -> Option<(Vec<String>, String)> {
    let mut color = vec![WHITE; nodes.len()];
    let mut path: Vec<usize> = Vec::new();
    for start in 0..nodes.len() {
        if color[start] == WHITE
            && let Some(found) = visit(start, nodes, edges, &mut color, &mut path)
        {
            return Some(found);
        }
    }
    None
}

fn visit(
    at: usize,
    nodes: &[String],
    edges: &BTreeMap<String, Vec<(String, String)>>,
    color: &mut [u8],
    path: &mut Vec<usize>,
) -> Option<(Vec<String>, String)> {
    color[at] = GRAY;
    path.push(at);
    for (target, label) in edges.get(&nodes[at]).into_iter().flatten() {
        let Some(to) = nodes.iter().position(|node| node == target) else {
            continue;
        };
        if color[to] == GRAY {
            let start = path
                .iter()
                .position(|node| *node == to)
                .expect("a gray node is on the walk's stack");
            let mut cycle: Vec<String> = path[start..]
                .iter()
                .map(|&node| nodes[node].clone())
                .collect();
            cycle.push(nodes[to].clone());
            return Some((cycle, format!("{label} -> {}", nodes[to])));
        }
        if color[to] == WHITE
            && let Some(found) = visit(to, nodes, edges, color, path)
        {
            return Some(found);
        }
    }
    path.pop();
    color[at] = BLACK;
    None
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
            types: selected.iter().map(|ty| ty.render(names)).collect(),
        });
    };
    let ty = instances
        .get(rendered)
        .cloned()
        .unwrap_or_else(|| rendered.to_owned());
    Ok(Some((ty, rendered.to_owned())))
}

/// Check each instance's mangled name is spellable and is not `taken`.
/// `Pair<x-y, z>` and `Pair<x, y-z>` both mangle to `PairXYZ`, so a
/// collision fails closed; a suffix would not be pure.
fn check_instances(
    kind: DefinitionKind,
    instances: &BTreeMap<String, String>,
    taken: &mut Vec<String>,
) -> Result<(), Error> {
    for (rendered, name) in instances {
        let reason = unspellable_name(kind, name).or_else(|| taken.contains(name).then_some(TAKEN));
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
    emission: &Emission<'_>,
    definition: &Definition,
    taken: &mut Vec<String>,
) -> Result<Vec<InlineUnion>, Error> {
    let (names, instances) = (emission.names, &emission.instances);
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
                    spelling: ty.render(names),
                });
            }
        };
        let key = names.resolve(key);
        let name = inline_union_name(&owner, key);
        let reason = unspellable_name(emission.kind, &name)
            .or_else(|| taken.contains(&name).then_some(TAKEN));
        if let Some(reason) = reason {
            return Err(Error::UnspellableName {
                what: "inline union",
                owner: format!("{owner}.{key}"),
                name,
                reason,
            });
        }
        // A member's variant is named after its plain rendering, whatever
        // path the file spells its type under.
        let render = |spelling| {
            members
                .iter()
                .map(|member| rust_type(member, names, instances, None, spelling))
                .collect::<Result<Vec<_>, _>>()
        };
        let plain = render(Spelling::Imported)?;
        let renderings = render(emission.spelling)?;
        variants(
            "union member",
            &name,
            plain.iter().map(String::as_str),
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
                    spelling: union.render(names),
                });
            }
            seen |= accepted;
        }
        taken.push(name.clone());
        unions.push(InlineUnion {
            owner,
            key: key.to_owned(),
            name,
            members: plain
                .iter()
                .map(|rendering| variant_name(rendering))
                .zip(renderings)
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
const ASSET: u8 = 1 << 7;
const ANY: u8 = STRING | BOOL | NUMBER | NULL | REFERENCE | LIST | MAP | ASSET;

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
                    Some(TypeKind::Asset(_)) => ASSET,
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
        TypeKind::Asset(_) => "asset",
        _ => "declared",
    }
}

/// The constants a definition's annotation facts generate, one per fact.
/// Same-named declarations share a stem: the first in contribution order
/// keeps it, later ones take `_2`, `_3`, … per node and annotation pair.
fn annotation_constants(definition: &Definition) -> Result<Vec<AnnotationConst>, Error> {
    let names = &definition.names;
    let mut spelled: BTreeMap<(Sym, Sym), usize> = BTreeMap::new();
    for (_, node) in definition.bindings.iter() {
        for fact in &node.annotations {
            *spelled.entry((node.name, fact.name)).or_default() += 1;
        }
    }
    let mut ordinals: BTreeMap<(Sym, Sym), usize> = BTreeMap::new();
    let mut constants: Vec<AnnotationConst> = Vec::new();
    for (_, node) in definition.bindings.iter() {
        let node_name = names.resolve(node.name);
        for fact in &node.annotations {
            let annotation = names.resolve(fact.name);
            let Some(decl) = definition
                .annotations
                .iter()
                .find(|decl| decl.name == fact.name)
            else {
                continue;
            };
            let (ty, value) = annotation_value(definition, &decl.ty, &fact.value);
            let stem = format!(
                "{}_{}",
                node_name.replace('-', "_").to_uppercase(),
                annotation.replace('-', "_").to_uppercase()
            );
            let ordinal = ordinals.entry((node.name, fact.name)).or_default();
            *ordinal += 1;
            let name = match *ordinal {
                1 => stem,
                n => format!("{stem}_{n}"),
            };
            if constants.iter().any(|other| other.name == name) {
                return Err(Error::UnspellableName {
                    what: "annotation constant",
                    owner: node_name.to_owned(),
                    name,
                    reason: "another annotation fact spells the same constant",
                });
            }
            let shared = spelled.get(&(node.name, fact.name)).copied().unwrap_or(0) > 1;
            constants.push(AnnotationConst {
                name,
                node: node_name.to_owned(),
                annotation: annotation.to_owned(),
                placement: shared.then(|| placement_parents(&node.placement, names)),
                ty,
                value,
            });
        }
    }
    Ok(constants)
}

/// The Rust type and literal of one annotation value, which the compiler
/// already decoded as its declared scalar type.
fn annotation_value(
    definition: &Definition,
    ty: &TypeExpr,
    value: &kdlith_engine::Value,
) -> (String, String) {
    use kdlith_engine::Value;
    let declared = match ty {
        TypeExpr::Named(name) => definition
            .types
            .lookup(*name)
            .and_then(|id| definition.types.get(id))
            .map(|decl| (definition.names.resolve(*name), &decl.kind)),
        _ => None,
    };
    let text = |spelled: &str| ("&str".to_owned(), format!("{spelled:?}"));
    let digits = |spelled: &str| spelled.strip_prefix('+').unwrap_or(spelled).to_owned();
    match (declared, value) {
        (Some((_, TypeKind::Builtin(Builtin::Bool))), Value::Bool(flag)) => {
            ("bool".to_owned(), flag.to_string())
        }
        (Some((_, TypeKind::Builtin(Builtin::U64))), Value::Number(spelled)) => {
            ("u64".to_owned(), digits(spelled))
        }
        (Some((_, TypeKind::Builtin(Builtin::Integer))), Value::Number(spelled)) => {
            ("i64".to_owned(), digits(spelled))
        }
        (Some((name, TypeKind::Enum(_))), Value::String(member)) => {
            let ty = format!("crate::types::{name}");
            let member = format!("{ty}::{}", pascal(member));
            (ty, member)
        }
        (_, Value::String(spelled) | Value::Number(spelled)) => text(spelled),
        (_, other) => text(&format!("{other:?}")),
    }
}

/// Why a type the crate declares under `name` has no spelling, if it
/// has none: `Self` is never a type name, schema crates spell
/// undeclared names by absolute path, and dialect crates stay
/// byte-equal to the reference processor.
fn unspellable_name(kind: DefinitionKind, name: &str) -> Option<&'static str> {
    if !is_identifier(name) || name == "Self" {
        Some(NOT_IDENTIFIER)
    } else if kind == DefinitionKind::Dialect && SHADOWED_IN_DIALECT.contains(&name) {
        Some(SHADOWS)
    } else {
        None
    }
}

/// The processor's item enum: `Item`, else `ItemKind` when a declared
/// type takes `Item`; both taken fails closed rather than inventing a
/// third spelling.
fn item_enum(owner: &str, taken: &[String]) -> Result<&'static str, Error> {
    processor_enum(owner, taken, "Item", "ItemKind")
}

/// The processor's contribution enum, when the definition declares typed
/// contribution nodes: `Contribution`, else `ContributionKind` when a
/// declared type takes it; both taken fails closed rather than inventing
/// a third spelling.
fn contribution_enum(owner: &str, taken: &[String]) -> Result<&'static str, Error> {
    processor_enum(owner, taken, "Contribution", "ContributionKind")
}

fn processor_enum(
    owner: &str,
    taken: &[String],
    primary: &'static str,
    fallback: &'static str,
) -> Result<&'static str, Error> {
    let free = |name: &str| !taken.iter().any(|other| other == name);
    if free(primary) {
        return Ok(primary);
    }
    if free(fallback) {
        return Ok(fallback);
    }
    Err(Error::UnspellableName {
        what: "processor enum",
        owner: owner.to_owned(),
        name: fallback.to_owned(),
        reason: TAKEN,
    })
}

/// The names a dialect crate spells bare beside its declared types —
/// prelude items, `Self`, its imported engine items, `BTreeMap` — which
/// a declared or node-local type taking one would shadow.
const SHADOWED_IN_DIALECT: [&str; 27] = [
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
const NOT_IDENTIFIER: &str = "it is no Rust identifier";
const TAKEN: &str = "it takes the name of another type the generated crate declares";

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
