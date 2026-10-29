//! File emission under the pinned writer profile: rustfmt-formatted
//! text, LF newlines, the generated marker first.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_engine::{DefinitionKind, Names, ReferenceDecl, Role, TypeDecl, TypeExpr, TypeKind};

use crate::catalog;
use crate::layout::{Expr, assignment, use_list, version_order};
use crate::manifest::{MANIFEST_NAME, manifest_text};
use crate::model::{Document, Emission, InlineUnion, ItemNode, kind_name};
use crate::render::{
    Spelling, engine_names, field_name, inline_union_name, named_references, pascal, rust_type,
    spells_btree_map,
};
use crate::{Error, Packaging};

const BUILTINS: [&str; 6] = ["String", "Bool", "Integer", "Decimal", "U64", "ValueExpr"];

/// A generated crate: every file's path and complete content, sorted by
/// path. Deterministic by construction — the same definition, crate
/// name, and generator version always yield equal trees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedTree {
    pub(crate) files: Vec<(String, String)>,
    /// The advisory diagnostics the definition's compilation raised;
    /// they shape no file.
    pub(crate) notes: Vec<DiagnosticRecord>,
}

impl GeneratedTree {
    /// The definition's advisory diagnostics, in detection order.
    pub fn notes(&self) -> &[DiagnosticRecord] {
        &self.notes
    }

    /// Every generator-owned path, sorted.
    pub fn paths(&self) -> impl Iterator<Item = &str> + '_ {
        self.files.iter().map(|(path, _)| path.as_str())
    }

    /// One file's content, by path.
    pub fn file(&self, path: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|(owned, _)| owned == path)
            .map(|(_, content)| content.as_str())
    }

    /// How many files the tree holds.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// The line writer every emitter shares.
struct Out {
    text: String,
}

impl Out {
    fn new() -> Self {
        Self {
            text: String::new(),
        }
    }

    fn line(&mut self, text: &str) {
        let _ = writeln!(self.text, "{text}");
    }

    fn blank(&mut self) {
        self.text.push('\n');
    }

    /// A `//!` module-doc block.
    fn doc(&mut self, lines: &[&str]) {
        for line in lines {
            self.line(&format!("//! {line}"));
        }
    }

    /// A `use path::{names};` statement, as rustfmt lays it out.
    fn use_list(&mut self, path: &str, names: &[String]) {
        if !names.is_empty() {
            self.text.push_str(&use_list("", path, names));
        }
    }

    /// `lhs expr;`, as rustfmt lays the item out at `indent`.
    fn assign(&mut self, lhs: &str, expr: &Expr, indent: usize) {
        self.line(&assignment(lhs, expr, indent));
    }

    fn finish(mut self) -> String {
        if !self.text.ends_with('\n') {
            self.text.push('\n');
        }
        self.text
    }
}

/// One emitted Rust file: the marker line, a blank line, then the body.
fn rust_file(body: &str) -> String {
    format!("{}\n\n{}", crate::RUST_MARKER, body)
}

impl Emission<'_> {
    /// Emit the whole tree: Cargo.toml, lib.rs, compile.rs, the four
    /// module trees, and the output manifest, every path sorted.
    pub(crate) fn emit(self) -> Result<GeneratedTree, Error> {
        let mut files: Vec<(String, String)> = vec![
            ("Cargo.toml".to_owned(), self.cargo_toml()),
            ("src/lib.rs".to_owned(), self.lib_rs()),
            ("src/compile.rs".to_owned(), self.compile_rs()?),
            ("src/types/mod.rs".to_owned(), self.types_mod_rs()),
        ];
        for doc in self.type_documents() {
            files.push((
                format!("src/types/{}.rs", doc.module),
                self.types_doc_rs(doc)?,
            ));
        }
        files.push(("src/bindings/mod.rs".to_owned(), self.bindings_mod_rs()));
        for doc in self.node_documents() {
            files.push((
                format!("src/bindings/{}.rs", doc.module),
                self.bindings_doc_rs(doc),
            ));
        }
        files.push(("src/contexts/mod.rs".to_owned(), self.contexts_mod_rs()));
        for doc in self.slot_documents() {
            files.push((
                format!("src/contexts/{}.rs", doc.module),
                self.contexts_doc_rs(doc)?,
            ));
        }
        files.push((
            "src/processors/mod.rs".to_owned(),
            self.processors_mod_rs()?,
        ));
        for doc in self.item_documents() {
            files.push((
                format!("src/processors/{}.rs", doc.module),
                self.processors_doc_rs(doc),
            ));
        }
        // The dialect form embeds its seed as a snapshot beside the
        // sources: the published crate carries the documents it was
        // generated from and reads nothing outside itself.
        if self.kind == DefinitionKind::Dialect {
            for (file_name, text) in self.seed_documents() {
                files.push((format!("src/dialect/{file_name}"), text.clone()));
            }
        }
        files.push((
            MANIFEST_NAME.to_owned(),
            manifest_text(files.iter().map(|(path, _)| path.as_str())),
        ));
        files.sort_by(|(one, _), (other, _)| one.cmp(other));
        Ok(GeneratedTree {
            files,
            notes: Vec::new(),
        })
    }

    /// The crate manifest, its kdlith dependencies named by the
    /// emission's packaging: standalone, or under the workspace
    /// packaging inheriting the dependencies and the lints from the
    /// consuming workspace.
    fn cargo_toml(&self) -> String {
        let mut out = Out::new();
        out.line(crate::COMMENT_MARKER);
        out.blank();
        out.line("[package]");
        out.line(&format!("name = \"{}\"", self.crate_name));
        out.line(&format!("version = \"{}\"", env!("CARGO_PKG_VERSION")));
        // Registry metadata: name, version, description, and license are
        // what a crates.io-style publication requires of the manifest.
        let form = match self.kind {
            DefinitionKind::Dialect => "dialect",
            _ => "schema",
        };
        out.line(&format!(
            "description = \"Generated kdlith processor for the {} {}.\"",
            self.name, form
        ));
        out.line("license = \"MIT\"");
        out.line("edition = \"2024\"");
        out.line("rust-version = \"1.85\"");
        out.line("autotests = false");
        out.blank();
        if self.packaging == Packaging::Workspace {
            out.line("[lints]");
            out.line("workspace = true");
        } else {
            out.line("[lints.clippy]");
            out.line("all = { level = \"warn\", priority = -1 }");
        }
        out.blank();
        out.line("[dependencies]");
        for dir in crate::dependency_dirs(self.kind) {
            out.line(&match &self.packaging {
                Packaging::Sibling => format!("kdlith-{dir} = {{ path = \"../{dir}\" }}"),
                Packaging::Path(root) => format!("kdlith-{dir} = {{ path = \"{root}/{dir}\" }}"),
                Packaging::Registry(version) => format!("kdlith-{dir} = \"{version}\""),
                Packaging::Workspace => format!("kdlith-{dir} = {{ workspace = true }}"),
            });
        }
        out.finish()
    }

    /// The crate root: module declarations and the compile entry points
    /// the processor re-exports.
    fn lib_rs(&self) -> String {
        let mut out = Out::new();
        match self.kind {
            DefinitionKind::Dialect => out.doc(&[
                "The reference typed reader for schema definitions: the hand-maintained",
                "processor that generated processors mirror. It declares the dialect's",
                "types, node bindings, and context; reading is the engine's.",
            ]),
            _ => out.doc(&[
                "The typed reader for documents written in this schema: it declares the",
                "schema's types, node bindings, and context; reading is the engine's.",
            ]),
        }
        for module in ["bindings", "compile", "contexts", "processors", "types"] {
            out.line(&format!("pub mod {module};"));
        }
        out.blank();
        let names = |list: &[&str]| list.iter().map(|name| (*name).to_owned()).collect();
        let compile: Vec<String> = match self.kind {
            DefinitionKind::Dialect => names(&[
                "load_dialect",
                "load_dialect_sources",
                "load_schema",
                "load_schema_sources",
                "load_schema_sources_with",
                "load_schema_with",
                "seed",
            ]),
            _ => names(&["load", "load_sources", "load_sources_with", "load_with"]),
        };
        let resolution: Vec<String> = names(&[
            "ContributionResolver",
            "LAYERED",
            "Layered",
            "ResolutionCall",
            "SharedContributionResolver",
            "SharedResolver",
            "Unresolvable",
            "ValueResolver",
        ]);
        let mut exports: Vec<(&str, Vec<String>)> = vec![
            ("compile", compile),
            ("kdlith_engine", resolution),
            (
                "processors",
                vec![
                    "DECLARATIONS".to_owned(),
                    self.item_enum.to_owned(),
                    "reader".to_owned(),
                    "reader_with_contribution_resolver".to_owned(),
                    "reader_with_resolver".to_owned(),
                ],
            ),
        ];
        // The dialect form stays byte-equal to the reference processor;
        // schema crates also carry the tool-author surface so a tool
        // needs no dependency beyond the generated crate.
        if self.kind == DefinitionKind::Schema {
            exports.push(("kdlith_diagnostic", names(&["DiagnosticRecord"])));
            exports.push((
                "kdlith_engine",
                names(&[
                    "Asset",
                    "CheckScope",
                    "Extensions",
                    "FromValue",
                    "ItemVariant",
                    "LinkOutcome",
                    "LinkReport",
                    "Mismatch",
                    "OpenError",
                    "Origin",
                    "ReadError",
                    "ReadOptions",
                    "Reading",
                    "Ref",
                    "Regex",
                    "ResolveError",
                    "Resolved",
                    "Source",
                    "SourceTree",
                    "TypedItem",
                    "TypedReader",
                    "UnmetQualifier",
                    "Value",
                    "matches_line",
                ]),
            ));
            exports.push(("kdlith_model", names(&["Location"])));
        }
        if let Some(contribution) = self.contribution_enum {
            exports.push(("processors", vec![contribution.to_owned()]));
        }
        // rustfmt orders the group by path, then by the first name: a
        // lone name before a braced list.
        let key = |(path, names): &(&str, Vec<String>)| {
            let first = names.iter().min_by(|a, b| version_order(a, b));
            (path.to_string(), names.len() > 1, first.cloned())
        };
        exports.sort_by(|one, other| {
            let (one, other) = (key(one), key(other));
            version_order(&one.0, &other.0)
                .then(one.1.cmp(&other.1))
                .then_with(|| match (&one.2, &other.2) {
                    (Some(a), Some(b)) => version_order(a, b),
                    _ => std::cmp::Ordering::Equal,
                })
        });
        for (path, names) in &exports {
            out.text.push_str(&use_list("pub ", path, names));
        }
        rust_file(&out.finish())
    }

    /// The per-document type declarations: one declaration macro per
    /// declared type, in contribution order, then the shape table.
    fn types_doc_rs(&self, doc: &Document<'_>) -> Result<String, Error> {
        let locals: Vec<String> = doc
            .locals
            .iter()
            .map(|node| pascal(self.names.resolve(node.name)))
            .collect();
        let mut declared: Vec<String> = doc
            .types
            .iter()
            .map(|decl| self.rust_name(decl.name))
            .collect();
        declared.extend(locals.iter().cloned());
        declared.extend(
            doc.references
                .iter()
                .map(|decl| self.names.resolve(decl.name).to_owned()),
        );
        let shaped = !doc.types.is_empty() || !doc.references.is_empty();
        // A document contributing only node-local item types has no
        // shape table, so it names neither shape item. A qualified file
        // imports only the declaration macros, which no type can shadow.
        let imported = self.spelling == Spelling::Imported;
        let mut engine: Vec<String> = if !shaped || !imported {
            Vec::new()
        } else {
            vec!["Shaped".into(), "TypeShape".into()]
        };
        let mut references: Vec<String> = Vec::new();
        let mut macros: Vec<String> = Vec::new();
        let mut btree_map = false;
        for node in &doc.locals {
            for field in &node.fields {
                if let Some(ty) = &field.ty {
                    for name in engine_names(ty, self.names) {
                        push_unique(&mut engine, name);
                    }
                    references.extend(self.references(ty));
                    btree_map |= spells_btree_map(ty);
                }
            }
            push_unique(&mut macros, "object_type");
        }
        for decl in &doc.types {
            let name = self.names.resolve(decl.name);
            for expr in declaration_expressions(decl) {
                for name in engine_names(expr, self.names) {
                    push_unique(&mut engine, name);
                }
                references.extend(self.references(expr));
                btree_map |= spells_btree_map(expr);
            }
            match &decl.kind {
                TypeKind::String(_) => push_unique(&mut macros, "string_type"),
                TypeKind::Enum(_) => push_unique(&mut macros, "enum_type"),
                TypeKind::Object(_) => push_unique(&mut macros, "object_type"),
                TypeKind::Union(_) => push_unique(&mut macros, "union_type"),
                TypeKind::Map(_) => push_unique(&mut macros, "map_type"),
                TypeKind::Codec(_) => push_unique(&mut macros, "codec_type"),
                TypeKind::Asset(_) => push_unique(&mut macros, "asset_type"),
                TypeKind::Builtin(_) => unreachable!("built-ins carry no site"),
                kind => {
                    return Err(Error::UnsupportedTypeKind {
                        name: name.to_owned(),
                        kind: kind_name(kind),
                    });
                }
            }
        }
        if !doc.unions.is_empty() {
            push_unique(&mut macros, "inline_union_type");
        }
        for decl in &doc.references {
            references.push(self.names.resolve(decl.target).to_owned());
            push_unique(&mut macros, "reference_type");
        }
        if !imported {
            engine.clear();
            btree_map = false;
        } else if references.iter().any(|name| name == "ValueExpr")
            && !engine.iter().any(|name| name == "ValueExpr")
        {
            engine.push("ValueExpr".to_owned());
        }
        engine.extend(macros);
        engine.sort();
        let mut super_names: Vec<String> = references
            .iter()
            .filter(|name| !BUILTINS.contains(&name.as_str()) && !declared.contains(name))
            .cloned()
            .collect();
        super_names.sort();
        super_names.dedup();

        let mut out = Out::new();
        match catalog::types_header(&doc.file_name) {
            Some(lines) => out.doc(lines),
            None if !shaped => out.doc(&[&format!(
                "Node-local item types contributed by `{}`.",
                doc.file_name
            )]),
            None => out.doc(&[&format!(
                "Logical types contributed by `{}`.",
                doc.file_name
            )]),
        }
        out.blank();
        if btree_map {
            out.line("use std::collections::BTreeMap;");
            out.blank();
        }
        out.use_list("kdlith_engine", &engine);
        if !super_names.is_empty() {
            out.blank();
            out.use_list("super", &super_names);
        }
        for decl in &doc.types {
            out.blank();
            self.type_declaration(&mut out, decl)?;
            self.domain_declaration(&mut out, &self.rust_name(decl.name));
            self.owned_unions(&mut out, doc, &self.rust_name(decl.name));
        }
        for decl in &doc.references {
            out.blank();
            self.reference_declaration(&mut out, decl);
        }
        for (node, name) in doc.locals.iter().zip(&locals) {
            out.blank();
            self.local_declaration(&mut out, node, name)?;
            self.domain_declaration(&mut out, name);
            self.owned_unions(&mut out, doc, name);
        }
        if shaped {
            out.blank();
            out.line("/// The shapes of this document's types, in declaration order.");
            let shapes = doc
                .types
                .iter()
                .map(|decl| decl.name)
                .chain(doc.references.iter().map(|decl| decl.name))
                .map(|name| {
                    let name = self.rust_name(name);
                    match self.spelling {
                        Spelling::Imported => Expr::path(format!("{name}::SHAPE")),
                        Spelling::Qualified => {
                            Expr::Qualified(format!("<{name} as ::kdlith_engine::Shaped>::SHAPE"))
                        }
                    }
                })
                .collect();
            let lhs = format!(
                "pub(crate) const TYPES: &[{}] =",
                self.spelling.engine("TypeShape")
            );
            out.assign(&lhs, &Expr::Array(shapes), 0);
        }
        Ok(rust_file(&out.finish()))
    }

    /// The node-local type an item node bound without `type=` reads as:
    /// an object of its inline-typed fields. It is no logical type of
    /// the definition, so it joins no shape table; its handler reads the
    /// items named after the node.
    fn local_declaration(
        &self,
        out: &mut Out,
        node: &kdlith_engine::NodeDecl,
        name: &str,
    ) -> Result<(), Error> {
        let node_name = self.names.resolve(node.name);
        out.line("object_type! {");
        // Schema-form node-local types match and construct exhaustively;
        // the dialect form keeps the reference processor's bytes.
        if self.kind == DefinitionKind::Schema {
            out.line("    exhaustive;");
        }
        out.line(&format!(
            "    /// The fields of a `{node_name}` node, which declares no type=."
        ));
        if node.fields.is_empty() {
            out.line(&format!("    pub struct {name} {{}}"));
            out.line("}");
            return Ok(());
        }
        out.line(&format!("    pub struct {name} {{"));
        for field in &node.fields {
            let key = self.names.resolve(field.name);
            // The compiler requires an inline type on every field of a
            // typeless node; one without fails closed.
            let Some(ty) = &field.ty else {
                return Err(Error::UnsupportedItem {
                    name: node_name.to_owned(),
                });
            };
            out.line(&format!(
                "        \"{}\" => {}: {},",
                key,
                field_name(key),
                rust_type(
                    ty,
                    self.names,
                    &self.instances,
                    Some(&inline_union_name(name, key)),
                    self.spelling
                )?
            ));
        }
        out.line("    }");
        out.line("}");
        Ok(())
    }

    /// The enums the inline unions of `owner`'s fields render as, each
    /// right after its owner, in field order.
    fn owned_unions(&self, out: &mut Out, doc: &Document<'_>, owner: &str) {
        for union in doc.unions.iter().filter(|union| union.owner == owner) {
            out.blank();
            inline_union_declaration(out, union, self.kind == DefinitionKind::Schema);
        }
    }

    /// The identity domain a type's declarations register in, beside the
    /// type, when declaration nodes read as it. Declarations of one type
    /// registering in different domains leave the type without one
    /// constant to name.
    fn domain_declaration(&self, out: &mut Out, name: &str) {
        let mut domains = self
            .items
            .iter()
            .filter(|item| item.ty == name)
            .filter_map(|item| item.domain);
        let Some(domain) = domains.next() else {
            return;
        };
        if domains.any(|other| other != domain) {
            return;
        }
        out.blank();
        out.line(&format!("impl {name} {{"));
        out.line("    /// The identity domain its declarations register in.");
        out.assign(
            "pub const DOMAIN: &str =",
            &Expr::literal(format!("{domain:?}")),
            4,
        );
        out.line("}");
    }

    /// One declared type, as its declaration macro spells it. A generic
    /// instance spells its rendered name through the macro's `as` arm,
    /// declared under its mangled Rust name.
    fn type_declaration(&self, out: &mut Out, decl: &TypeDecl) -> Result<(), Error> {
        let logical = self.names.resolve(decl.name);
        let name = self.rust_name(decl.name);
        let doc = catalog::type_doc(logical);
        let spelled = (name != logical).then(|| format!("    as {logical:?};"));
        // Only the attribute-carrying macros accept the marker; the
        // newtype macros never carried `#[non_exhaustive]`. Gated here
        // per form, never inside a shared macro arm, so the dialect
        // form's bytes stay pinned to the reference processor.
        let exhaustive = (self.kind == DefinitionKind::Schema
            && matches!(
                decl.kind,
                TypeKind::Enum(_) | TypeKind::Object(_) | TypeKind::Union(_)
            ))
        .then(|| "    exhaustive;".to_owned());
        let open = |out: &mut Out, invocation: &str| {
            out.line(invocation);
            if let Some(spelled) = &spelled {
                out.line(spelled);
            }
            if let Some(exhaustive) = &exhaustive {
                out.line(exhaustive);
            }
        };
        match &decl.kind {
            TypeKind::String(_) => {
                open(out, "string_type! {");
                for line in doc.unwrap_or(&[]) {
                    out.line(&format!("    /// {line}"));
                }
                out.line(&format!("    pub struct {name};"));
                out.line("}");
            }
            TypeKind::Enum(enumeration) => {
                open(out, "enum_type! {");
                for line in doc.unwrap_or(&[]) {
                    out.line(&format!("    /// {line}"));
                }
                out.line(&format!("    pub enum {name} {{"));
                for member in &enumeration.members {
                    out.line(&format!(
                        "        \"{}\" => {},",
                        member.name,
                        pascal(&member.name)
                    ));
                }
                out.line("    }");
                out.line("}");
            }
            TypeKind::Object(object) => {
                open(out, "object_type! {");
                for line in doc.unwrap_or(&[]) {
                    out.line(&format!("    /// {line}"));
                }
                if object.fields.is_empty() && doc.is_none() {
                    out.line(&format!("    pub struct {name} {{}}"));
                } else {
                    out.line(&format!("    pub struct {name} {{"));
                    for field in &object.fields {
                        let key = self.names.resolve(field.name);
                        if let Some(lines) = catalog::field_comment(logical, key) {
                            for line in lines {
                                out.line(&format!("        // {line}"));
                            }
                        }
                        out.line(&format!(
                            "        \"{}\" => {}: {},",
                            key,
                            field_name(key),
                            rust_type(
                                &field.ty,
                                self.names,
                                &self.instances,
                                Some(&inline_union_name(&name, key)),
                                self.spelling
                            )?
                        ));
                    }
                    out.line("    }");
                }
                out.line("}");
            }
            TypeKind::Union(union) => {
                open(out, "union_type! {");
                for line in doc.unwrap_or(&[]) {
                    out.line(&format!("    /// {line}"));
                }
                out.line(&format!("    pub enum {name} {{"));
                for variant in &union.variants {
                    let discriminator = self.names.resolve(variant.discriminator);
                    let prefix = variant
                        .prefix
                        .as_ref()
                        .map_or(String::new(), |prefix| format!(" prefix {prefix:?}"));
                    out.line(&format!(
                        "        \"{}\"{} => {}({}),",
                        discriminator,
                        prefix,
                        pascal(discriminator),
                        rust_type(
                            &variant.ty,
                            self.names,
                            &self.instances,
                            None,
                            self.spelling
                        )?
                    ));
                }
                out.line("    }");
                out.line("}");
            }
            TypeKind::Map(map) => {
                open(out, "map_type! {");
                for line in doc.unwrap_or(&[]) {
                    out.line(&format!("    /// {line}"));
                }
                out.line(&format!(
                    "    pub struct {name}({} => {});",
                    rust_type(&map.key, self.names, &self.instances, None, self.spelling)?,
                    rust_type(&map.value, self.names, &self.instances, None, self.spelling)?
                ));
                out.line("}");
            }
            TypeKind::Asset(_) => {
                open(out, "asset_type! {");
                for line in doc.unwrap_or(&[]) {
                    out.line(&format!("    /// {line}"));
                }
                out.line(&format!("    pub struct {name};"));
                out.line("}");
            }
            TypeKind::Codec(codec) => {
                open(out, "codec_type! {");
                for line in doc.unwrap_or(&[]) {
                    out.line(&format!("    /// {line}"));
                }
                out.line(&format!(
                    "    pub struct {name}({});",
                    rust_type(
                        &codec.output,
                        self.names,
                        &self.instances,
                        None,
                        self.spelling
                    )?
                ));
                out.line("}");
            }
            TypeKind::Builtin(_) => unreachable!("built-ins carry no site"),
            kind => {
                return Err(Error::UnsupportedTypeKind {
                    name,
                    kind: kind_name(kind),
                });
            }
        }
        Ok(())
    }

    /// One reference type: a newtype over the engine's `Ref` to its
    /// target, carrying the lookup domain its references decode into
    /// and its qualifier names, as conformance compares them.
    fn reference_declaration(&self, out: &mut Out, decl: &ReferenceDecl) {
        let name = self.names.resolve(decl.name);
        let target = self.names.resolve(decl.target);
        let lookup = self.names.resolve(decl.lookup.unwrap_or(decl.target));
        let qualifiers: Vec<String> = decl
            .qualifiers
            .iter()
            .map(|qualifier| format!("{:?}", self.names.resolve(qualifier.name)))
            .collect();
        out.line("reference_type! {");
        out.line(&format!("    pub struct {name} => {target} {{"));
        out.line(&format!("        lookup: {lookup:?},"));
        out.line(&format!("        qualifiers: [{}],", qualifiers.join(", ")));
        out.line("    }");
        out.line("}");
    }

    /// The types module root: alphabetical module list and re-exports,
    /// the shape table in contribution order.
    fn types_mod_rs(&self) -> String {
        let mut out = Out::new();
        match self.kind {
            DefinitionKind::Dialect => out.doc(&[
                "The dialect's logical types: one module per contributing document,",
                "each type declared once and read from bound values.",
            ]),
            _ => out.doc(&[
                "The schema's logical types: one module per contributing document,",
                "each type declared once and read from bound values.",
            ]),
        }
        out.blank();
        let mut sorted: Vec<String> = self
            .type_documents()
            .map(|doc| doc.module.clone())
            .collect();
        // Only documents declaring logical types hold a shape table; a
        // document of node-local item types holds none.
        let modules: Vec<String> = self
            .type_documents()
            .filter(|doc| !doc.types.is_empty() || !doc.references.is_empty())
            .map(|doc| doc.module.clone())
            .collect();
        // Module declarations and re-exports are alphabetical; the
        // shape table below stays in contribution order.
        sorted.sort_by(|a, b| version_order(a, b));
        for module in &sorted {
            out.line(&format!("mod {module};"));
        }
        out.blank();
        // The glob re-exports bring declared types into scope, which a
        // qualified file never lets shadow an engine item.
        if self.spelling == Spelling::Imported {
            out.line("use kdlith_engine::TypeShape;");
            out.blank();
        }
        for module in &sorted {
            out.line(&format!("pub use {module}::*;"));
        }
        out.blank();
        out.line("/// Every type's shape, by contributing document.");
        let lhs = format!(
            "pub(crate) const TYPES: &[&[{}]] =",
            self.spelling.engine("TypeShape")
        );
        out.assign(&lhs, &module_tables(&modules, "TYPES"), 0);
        rust_file(&out.finish())
    }

    /// One document's node declarations, with shared parent lists
    /// factored into module constants.
    fn bindings_doc_rs(&self, doc: &Document<'_>) -> String {
        let mut out = Out::new();
        match catalog::bindings_header(&doc.file_name) {
            Some(lines) => out.doc(lines),
            None => out.doc(&[&format!(
                "Node declarations contributed by `{}`.",
                doc.file_name
            )]),
        }
        out.blank();
        out.line("use kdlith_engine::NodeShape;");

        let entries: Vec<NodeEntry> = doc
            .nodes
            .iter()
            .map(|node| NodeEntry {
                name: self.names.resolve(node.name).to_owned(),
                parents: placement_parents(&node.placement, self.names),
                root: matches!(node.placement, kdlith_engine::Placement::RootAndUnder(_)),
                role: node.role,
                ty: node.ty.map(|ty| self.names.resolve(ty).to_owned()),
            })
            .collect();
        for (name, parents) in parent_constants(&entries) {
            out.blank();
            let parents = parents.iter().map(|parent| Expr::string(parent)).collect();
            out.assign(
                &format!("const {name}: &[&str] ="),
                &Expr::Array(parents),
                0,
            );
        }
        out.blank();
        let shapes = entries
            .iter()
            .map(|entry| {
                let constant = parent_constants(&entries)
                    .into_iter()
                    .find(|(_, parents)| parents == &entry.parents)
                    .map(|(name, _)| name);
                node_shape(entry, constant.as_deref())
            })
            .collect();
        out.assign(
            "pub(crate) const NODES: &[NodeShape] =",
            &Expr::Array(shapes),
            0,
        );
        rust_file(&out.finish())
    }

    /// The bindings module root.
    fn bindings_mod_rs(&self) -> String {
        let mut out = Out::new();
        match self.kind {
            DefinitionKind::Dialect => out.doc(&[
                "Node, property, argument, and child bindings for schema definitions:",
                "the dialect's node declarations, one table per contributing document.",
            ]),
            _ => out.doc(&[
                "Node, property, argument, and child bindings for data documents: the",
                "schema's node declarations, one table per contributing document.",
            ]),
        }
        out.blank();
        let modules: Vec<String> = self
            .node_documents()
            .map(|doc| doc.module.clone())
            .collect();
        let mut sorted = modules.clone();
        sorted.sort_by(|a, b| version_order(a, b));
        for module in &sorted {
            out.line(&format!("mod {module};"));
        }
        out.blank();
        out.line("use kdlith_engine::NodeShape;");
        out.blank();
        out.line("/// Every node declaration, by contributing document.");
        out.assign(
            "pub(crate) const NODES: &[&[NodeShape]] =",
            &module_tables(&modules, "NODES"),
            0,
        );
        for constant in &self.annotations {
            out.blank();
            let site = match constant.placement.as_deref() {
                None => String::new(),
                Some([]) => " at the document root".to_owned(),
                Some(parents) => format!(
                    " under {}",
                    parents
                        .iter()
                        .map(|parent| format!("`{parent}`"))
                        .collect::<Vec<_>>()
                        .join(" or ")
                ),
            };
            out.line(&format!(
                "/// The `{}` annotation of the `{}` node declaration{site}.",
                constant.annotation, constant.node
            ));
            out.assign(
                &format!("pub const {}: {} =", constant.name, constant.ty),
                &Expr::literal(constant.value.clone()),
                0,
            );
        }
        rust_file(&out.finish())
    }

    /// The contexts module root; a definition declaring no slots holds
    /// the empty table alone.
    fn contexts_mod_rs(&self) -> String {
        let mut out = Out::new();
        let slots = self.slot_documents().count();
        match (self.kind, slots == 0) {
            (DefinitionKind::Dialect, true) => out.doc(&[
                "Context declarations for schema definitions: the dialect's own typed",
                "context slots. The dialect declares none, so its documents bind with",
                "an empty context.",
            ]),
            (DefinitionKind::Dialect, false) => out.doc(&[
                "Context declarations for schema definitions: the dialect's own typed",
                "context slots, one table per contributing document.",
            ]),
            (_, true) => out.doc(&[
                "Context declarations for schema processing: the schema's own typed",
                "context slots. The schema declares none, so its documents bind with",
                "an empty context.",
            ]),
            (_, false) => out.doc(&[
                "Context declarations for schema processing: the schema's own typed",
                "context slots, one table per contributing document.",
            ]),
        }
        if slots > 0 {
            out.blank();
            let mut sorted: Vec<String> = self
                .slot_documents()
                .map(|doc| doc.module.clone())
                .collect();
            sorted.sort_by(|a, b| version_order(a, b));
            for module in &sorted {
                out.line(&format!("mod {module};"));
            }
        }
        out.blank();
        out.line("use kdlith_engine::SlotShape;");
        out.blank();
        out.line("/// Every context slot, by contributing document.");
        let modules: Vec<String> = self
            .slot_documents()
            .map(|doc| doc.module.clone())
            .collect();
        out.assign(
            "pub(crate) const SLOTS: &[&[SlotShape]] =",
            &module_tables(&modules, "SLOTS"),
            0,
        );
        rust_file(&out.finish())
    }

    /// One document's context slots.
    fn contexts_doc_rs(&self, doc: &Document<'_>) -> Result<String, Error> {
        let mut references: Vec<String> = Vec::new();
        for slot in &doc.slots {
            references.extend(self.references(&slot.ty));
        }
        let mut types: Vec<String> = references
            .into_iter()
            .filter(|name| !BUILTINS.contains(&name.as_str()))
            .collect();
        types.sort();
        types.dedup();

        // Every slot spells its expression through the `LogicalType`
        // trait, plainly or qualified, so the file imports it, and the
        // engine carriers its types render through.
        let imported = self.spelling == Spelling::Imported;
        let mut engine = Vec::new();
        if imported {
            engine = vec!["SlotShape".to_owned(), "LogicalType".to_owned()];
            for slot in &doc.slots {
                for name in engine_names(&slot.ty, self.names) {
                    push_unique(&mut engine, name);
                }
            }
        }
        engine.sort();

        let mut out = Out::new();
        out.doc(&[&format!(
            "Context slots contributed by `{}`.",
            doc.file_name
        )]);
        out.blank();
        if imported && doc.slots.iter().any(|slot| spells_btree_map(&slot.ty)) {
            out.line("use std::collections::BTreeMap;");
            out.blank();
        }
        out.use_list("kdlith_engine", &engine);
        if !types.is_empty() {
            if imported {
                out.blank();
            }
            out.use_list("crate::types", &types);
        }
        if imported || !types.is_empty() {
            out.blank();
        }
        let slot_shape = self.spelling.engine("SlotShape");
        let mut shapes = Vec::new();
        for slot in &doc.slots {
            let name = self.names.resolve(slot.name);
            // A slot owns no inline union's enum, so one in its type
            // fails closed.
            let ty = rust_type(&slot.ty, self.names, &self.instances, None, self.spelling)?;
            let expression = if !imported {
                Expr::Qualified(format!(
                    "<{ty} as ::kdlith_engine::LogicalType>::expression"
                ))
            } else if ty.contains('<') {
                Expr::Qualified(format!("<{ty} as LogicalType>::expression"))
            } else {
                Expr::path(format!("{ty}::expression"))
            };
            let mut shape = Expr::call(
                format!("{slot_shape}::new"),
                vec![Expr::string(name), expression],
            );
            if let Some(namespace) = slot.namespace {
                shape = shape.method(
                    "in_namespace",
                    vec![Expr::string(self.names.resolve(namespace))],
                );
            }
            shapes.push(shape);
        }
        out.assign(
            &format!("pub(crate) const SLOTS: &[{slot_shape}] ="),
            &Expr::Array(shapes),
            0,
        );
        Ok(rust_file(&out.finish()))
    }

    /// The processors module root: the item enum, the handler table, the
    /// declarations, and the one reader.
    fn processors_mod_rs(&self) -> Result<String, Error> {
        let mut sorted: Vec<String> = self.items.iter().map(|item| item.ty.clone()).collect();
        sorted.sort_by(|a, b| version_order(a, b));
        sorted.dedup();

        let mut out = Out::new();
        match self.kind {
            DefinitionKind::Dialect => out.doc(&[
                "Typed readers: the per-role schema-definition processing logic. The",
                "dialect's items, the handlers that read them, and the declarations",
                "the engine conforms to the compiled dialect before reading.",
            ]),
            _ => out.doc(&[
                "Typed readers: the per-role document processing logic. The schema's",
                "items, the handlers that read them, and the declarations the engine",
                "conforms to the compiled schema before reading.",
            ]),
        }
        out.blank();
        let modules: Vec<String> = self
            .item_documents()
            .map(|doc| doc.module.clone())
            .collect();
        let mut mods_sorted = modules.clone();
        mods_sorted.sort_by(|a, b| version_order(a, b));
        for module in &mods_sorted {
            out.line(&format!("mod {module};"));
        }
        if !mods_sorted.is_empty() {
            out.blank();
        }
        let spelling = self.spelling;
        let item = self.item_enum;
        if spelling == Spelling::Imported {
            out.line("use kdlith_diagnostic::DiagnosticRecord;");
            out.line("use kdlith_engine::{");
            out.line(
                "    Declarations, Definition, DefinitionKind, Handler, SharedContributionResolver, SharedResolver,",
            );
            out.line("    TypedReader, item_type,");
            out.line("};");
        } else {
            out.line("use kdlith_engine::item_type;");
        }
        out.blank();
        out.use_list("crate::types", &sorted);
        out.line("use crate::{bindings, contexts, types};");
        out.blank();
        out.line("item_type! {");
        if self.kind == DefinitionKind::Schema {
            out.line("    exhaustive;");
        }
        match self.kind {
            DefinitionKind::Dialect => {
                out.line("    /// A record or declaration of a definition written in the dialect.");
            }
            _ => out.line("    /// A record or declaration of a document written in the schema."),
        }
        out.line(&format!("    pub enum {item} {{"));
        for line in self.item_variants() {
            out.line(&format!("        {line},"));
        }
        out.line("    }");
        out.line("}");
        out.blank();
        let identity_arms = self.identity_arms();
        if self.kind == DefinitionKind::Schema && !identity_arms.is_empty() {
            // The schema form alone names its items; the dialect form's
            // bytes stay pinned to the reference processor.
            let option = self.spelling.path("core::option", "Option");
            out.line(&format!("impl {item} {{"));
            out.line("    /// The declaration's name: the text of its one string identity");
            out.line("    /// field. Records, and declarations whose identity composes");
            out.line("    /// several fields or is no string, name nothing.");
            out.line(&format!("    pub fn name(&self) -> {option}<&str> {{"));
            out.line("        match self {");
            let covered = identity_arms.len();
            for (variant, accessor) in identity_arms {
                let some = format!("{option}::Some(value.{accessor})");
                let arm = format!("            Self::{variant}(value) => {some},");
                if arm.chars().count() <= 100 {
                    out.line(&arm);
                } else {
                    out.line(&format!("            Self::{variant}(value) => {{"));
                    out.line(&format!("                {some}"));
                    out.line("            }");
                }
            }
            // A wildcard over variants all named would be unreachable.
            let variants = self
                .items
                .iter()
                .filter(|item| item.declares_variant)
                .count();
            if covered < variants {
                out.line(&format!("            _ => {option}::None,"));
            }
            out.line("        }");
            out.line("    }");
            out.line("}");
            out.blank();
        }
        if let Some(contribution) = self.contribution_enum {
            out.line("::kdlith_engine::contribution_type! {");
            if self.kind == DefinitionKind::Schema {
                out.line("    exhaustive;");
            }
            out.line("    /// A contribution's value, or a resolution's settled value, as the");
            out.line("    /// type its contribution node declares.");
            out.line(&format!("    pub enum {contribution} {{"));
            for ty in &self.contributions {
                out.line(&format!("        {ty}(types::{ty}),"));
            }
            out.line("    }");
            out.line("}");
            out.blank();
        }
        out.line("/// Every handler, by contributing document.");
        let handler = spelling.engine("Handler");
        out.assign(
            &format!("const HANDLERS: &[&[{handler}<{item}>]] ="),
            &module_tables(&modules, "HANDLERS"),
            0,
        );
        out.blank();
        let (kind_word, kind_path) = match self.kind {
            DefinitionKind::Dialect => ("dialect", "Dialect"),
            _ => ("schema", "Schema"),
        };
        out.line(&format!(
            "/// Everything this processor declares about the {kind_word} it reads."
        ));
        let declarations = spelling.engine("Declarations");
        let value = Expr::call(
            format!("{declarations}::new"),
            vec![
                Expr::path(format!(
                    "{}::{kind_path}",
                    spelling.engine("DefinitionKind")
                )),
                Expr::literal(format!("{:?}", self.name)),
                Expr::literal(format!("{:?}", self.version)),
            ],
        )
        .method("types", vec![Expr::path("types::TYPES")])
        .method("nodes", vec![Expr::path("bindings::NODES")])
        .method("slots", vec![Expr::path("contexts::SLOTS")])
        .method("handlers", vec![Expr::path("HANDLERS")]);
        out.assign(
            &format!("pub static DECLARATIONS: {declarations}<{item}> ="),
            &value,
            0,
        );
        out.blank();
        let (param, reads) = match self.kind {
            DefinitionKind::Dialect => ("dialect", "definitions written in the compiled dialect"),
            _ => ("schema", "documents written under the compiled schema"),
        };
        let definition = spelling.engine("Definition");
        let typed_reader = spelling.engine("TypedReader");
        let ok = format!("{typed_reader}<'_, {item}>");
        let err = format!(
            "{}<{}>",
            spelling.path("std::vec", "Vec"),
            spelling.path("kdlith_diagnostic", "DiagnosticRecord")
        );
        let result = spelling.path("core::result", "Result");
        // `) -> {returns} {` on one line, else the generic arguments
        // one per line, as rustfmt breaks a return type.
        let returns = if format!(") -> {result}<{ok}, {err}> {{").chars().count() <= 100 {
            format!("{result}<{ok}, {err}>")
        } else {
            format!("{result}<\n    {ok},\n    {err},\n>")
        };
        out.line(&format!("/// A reader of {reads} `{param}`."));
        match spelling {
            Spelling::Imported => out.line(&format!(
                "pub fn reader({param}: &{definition}) -> {returns} {{"
            )),
            Spelling::Qualified => {
                out.line("pub fn reader(");
                out.line(&format!("    {param}: &{definition},"));
                out.line(&format!(") -> {returns} {{"));
            }
        }
        out.line(&format!("    {typed_reader}::new({param}, &DECLARATIONS)"));
        out.line("}");
        out.blank();
        out.line(&format!("/// A reader of {reads} `{param}`"));
        out.line("/// whose computed defaults resolve through the consuming program's");
        out.line("/// value resolver `resolver`, which receives the declared descriptors");
        out.line("/// as data.");
        out.line("pub fn reader_with_resolver(");
        out.line(&format!("    {param}: &{definition},"));
        out.line(&format!(
            "    resolver: {},",
            spelling.engine("SharedResolver")
        ));
        out.line(&format!(") -> {returns} {{"));
        out.line(&format!(
            "    reader({param}).map(|typed| typed.with_resolver(resolver))"
        ));
        out.line("}");
        out.blank();
        out.line(&format!("/// A reader of {reads} `{param}`"));
        out.line("/// whose contributions resolve through the processor's contribution");
        out.line("/// resolver `resolver` under the declared resolution policy.");
        out.line("pub fn reader_with_contribution_resolver(");
        out.line(&format!("    {param}: &{definition},"));
        out.line(&format!(
            "    resolver: {},",
            spelling.engine("SharedContributionResolver")
        ));
        out.line(&format!(") -> {returns} {{"));
        out.line(&format!(
            "    reader({param}).map(|typed| typed.with_contribution_resolver(resolver))"
        ));
        out.line("}");
        Ok(rust_file(&out.finish()))
    }

    /// The `Item` enum's variant lines: one per type, named after it,
    /// unless a type backs node-keyed variants, which groups every
    /// variant under the type it reads.
    fn item_variants(&self) -> Vec<String> {
        let declared = self.items.iter().filter(|item| item.declares_variant);
        if !self.items.iter().any(|item| item.keyed) {
            return declared.map(|item| item.ty.clone()).collect();
        }
        let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
        for item in declared {
            match groups.iter_mut().find(|(ty, _)| *ty == item.ty) {
                Some((_, variants)) => variants.push(&item.variant),
                None => groups.push((&item.ty, vec![&item.variant])),
            }
        }
        groups
            .into_iter()
            .map(|(ty, variants)| format!("{ty} => {}", variants.join(" | ")))
            .collect()
    }

    /// Each `Item` variant whose items name themselves by one string
    /// identity field, with the accessor its value reads the field's
    /// text through. Variants whose items disagree name nothing.
    fn identity_arms(&self) -> Vec<(String, String)> {
        let mut arms: BTreeMap<String, Option<String>> = BTreeMap::new();
        for item in &self.items {
            let accessor = self.identity_accessor(item);
            match arms.get_mut(&item.variant) {
                Some(spelled) if spelled.as_deref() != accessor.as_deref() => *spelled = None,
                Some(_) => {}
                None => {
                    arms.insert(item.variant.clone(), accessor);
                }
            }
        }
        arms.into_iter()
            .filter_map(|(variant, accessor)| accessor.map(|accessor| (variant, accessor)))
            .collect()
    }

    /// The accessor a declaring node's one string identity field reads
    /// its text through: `as_str` serves `String` and string types
    /// alike. Records and other identities name nothing.
    fn identity_accessor(&self, item: &ItemNode<'_>) -> Option<String> {
        if item.record {
            return None;
        }
        let [field] = item.identity_fields else {
            return None;
        };
        let field = *field;
        let declared = item.node.ty.and_then(|ty| {
            self.documents.iter().find_map(|doc| {
                doc.types
                    .iter()
                    .find(|decl| decl.name == ty)
                    .and_then(|decl| {
                        match &decl.kind {
                            TypeKind::Object(object) => object
                                .fields
                                .iter()
                                .find(|candidate| candidate.name == field),
                            _ => None,
                        }
                        .map(|found| &found.ty)
                    })
            })
        });
        let ty = declared.or_else(|| {
            item.node
                .fields
                .iter()
                .find(|candidate| candidate.name == field)
                .and_then(|candidate| candidate.ty.as_ref())
        })?;
        let TypeExpr::Named(name) = ty else {
            return None;
        };
        let text = self.names.resolve(*name) == "String"
            || self.documents.iter().any(|doc| {
                doc.types
                    .iter()
                    .any(|decl| decl.name == *name && matches!(decl.kind, TypeKind::String(_)))
            });
        text.then(|| format!("{}.as_str()", field_name(self.names.resolve(field))))
    }

    /// One document's handlers, in node declaration order.
    fn processors_doc_rs(&self, doc: &Document<'_>) -> String {
        let items: Vec<&ItemNode<'_>> = self
            .items
            .iter()
            .filter(|item| item.declares_handler)
            .filter(|item| doc.nodes.iter().any(|node| std::ptr::eq(*node, item.node)))
            .collect();
        let mut types: Vec<String> = items.iter().map(|item| item.ty.clone()).collect();
        types.sort();
        types.dedup();
        let keyed = items.iter().any(|item| item.keyed);

        let mut out = Out::new();
        match catalog::processors_header(&doc.file_name) {
            Some(lines) => out.doc(lines),
            None => out.doc(&[&format!(
                "Handlers for the items `{}` declares.",
                doc.file_name
            )]),
        }
        out.blank();
        let spelling = self.spelling;
        let item_enum = self.item_enum;
        let handler_path = spelling.engine("Handler");
        let read_fields = spelling.engine("read_fields");
        if spelling == Spelling::Imported {
            if keyed {
                out.line("use kdlith_engine::{Handler, read_fields};");
            } else {
                out.line("use kdlith_engine::Handler;");
            }
            out.blank();
        }
        out.line(&format!("use super::{item_enum};"));
        out.use_list("crate::types", &types);
        out.blank();
        let handlers = items
            .iter()
            .map(|item| {
                let (ty, role) = (&item.ty, if item.record { "record" } else { "declaration" });
                if item.keyed {
                    let node = self.names.resolve(item.node.name);
                    let variant = &item.variant;
                    let read = Expr::call(read_fields.clone(), vec![Expr::path("fields")])
                        .method("map", vec![Expr::path(format!("{item_enum}::{variant}"))]);
                    let closure = Expr::Closure {
                        param: "fields".to_owned(),
                        body: Box::new(read),
                    };
                    return Expr::turbofish(
                        format!("{handler_path}::{role}_with"),
                        ty,
                        vec![closure],
                    )
                    .method("node", vec![Expr::string(node)]);
                }
                let handler = Expr::turbofish(format!("{handler_path}::{role}"), ty, Vec::new());
                // A typeless node's items carry the node's name.
                if item.local {
                    handler.method("named", vec![Expr::string(&item.item_name)])
                } else {
                    handler
                }
            })
            .collect();
        out.assign(
            &format!("pub(crate) const HANDLERS: &[{handler_path}<{item_enum}>] ="),
            &Expr::Array(handlers),
            0,
        );
        rust_file(&out.finish())
    }

    /// Every named type an expression references, spelled as the
    /// generated crate declares it.
    fn references(&self, expr: &TypeExpr) -> Vec<String> {
        named_references(expr, self.names)
            .into_iter()
            .map(|name| self.instances.get(&name).cloned().unwrap_or(name))
            .collect()
    }
}

/// Every type expression a declaration references: an object's fields,
/// a union's variants, a codec's output, a map's key and value.
pub(crate) fn declaration_expressions(decl: &TypeDecl) -> Vec<&TypeExpr> {
    match &decl.kind {
        TypeKind::Object(object) => object.fields.iter().map(|field| &field.ty).collect(),
        TypeKind::Union(union) => union.variants.iter().map(|variant| &variant.ty).collect(),
        TypeKind::Codec(codec) => vec![&codec.output],
        TypeKind::Map(map) => vec![&map.key, &map.value],
        _ => Vec::new(),
    }
}

/// One inline union's enum, its variants in member order.
fn inline_union_declaration(out: &mut Out, union: &InlineUnion, exhaustive: bool) {
    out.line("inline_union_type! {");
    if exhaustive {
        out.line("    exhaustive;");
    }
    out.line(&format!(
        "    /// The inline union `{}.{}` holds.",
        union.owner, union.key
    ));
    out.line(&format!("    pub enum {} {{", union.name));
    for (variant, ty) in &union.members {
        out.line(&format!("        {variant}({ty}),"));
    }
    out.line("    }");
    out.line("}");
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|owned| owned == value) {
        values.push(value.to_owned());
    }
}

/// One node declaration, as the bindings table renders it.
struct NodeEntry {
    name: String,
    parents: Vec<String>,
    root: bool,
    role: Option<Role>,
    ty: Option<String>,
}

/// The resolved parent names of a node's placement, in declared order.
pub(crate) fn placement_parents(
    placement: &kdlith_engine::Placement,
    names: &Names,
) -> Vec<String> {
    placement
        .parents()
        .iter()
        .map(|parent| names.resolve(*parent).to_owned())
        .collect()
}

/// One node declaration's `NodeShape` entry.
fn node_shape(entry: &NodeEntry, constant: Option<&str>) -> Expr {
    let mut shape = Expr::call("NodeShape::new", vec![Expr::string(&entry.name)]);
    if !entry.parents.is_empty() {
        let parents = match constant {
            Some(constant) => Expr::path(constant),
            None => Expr::Array(entry.parents.iter().map(|p| Expr::string(p)).collect()),
        };
        shape = shape.method("under", vec![parents]);
        if entry.root {
            shape = shape.method("also_root", Vec::new());
        }
    }
    if let Some(role) = entry.role {
        let role = match role {
            Role::Record => "record",
            Role::Declaration => "declaration",
            Role::Contribution => "contribution",
            Role::ContextControl => "context_control",
            Role::SourceImport => "source_import",
            Role::Container => "container",
            _ => unreachable!("every role is spelled"),
        };
        shape = shape.method(role, Vec::new());
    }
    if let Some(ty) = &entry.ty {
        shape = shape.method("ty", vec![Expr::string(ty)]);
    }
    shape
}

/// A module-level table of each module's table `name`, in order.
fn module_tables(modules: &[String], name: &str) -> Expr {
    Expr::Array(
        modules
            .iter()
            .map(|module| Expr::path(format!("{module}::{name}")))
            .collect(),
    )
}

/// Parent lists shared by two or more nodes with three or more parents
/// become one constant each, named after the Pascal word the sharing
/// nodes' bound types end in.
fn parent_constants(entries: &[NodeEntry]) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(&Vec<String>, usize, Vec<&str>)> = Vec::new();
    for entry in entries {
        if entry.parents.len() < 3 {
            continue;
        }
        if let Some(group) = groups
            .iter_mut()
            .find(|(parents, _, _)| *parents == &entry.parents)
        {
            group.1 += 1;
            if let Some(ty) = &entry.ty {
                group.2.push(ty);
            }
        } else {
            groups.push((
                &entry.parents,
                1,
                entry.ty.iter().map(String::as_str).collect(),
            ));
        }
    }
    groups
        .into_iter()
        .filter(|(_, uses, types)| *uses >= 2 && types.len() == *uses)
        .map(|(parents, _, types)| {
            let name = match common_trailing_word(&types) {
                Some(word) => format!("{}_PARENTS", word.to_uppercase()),
                None => format!(
                    "{}_PARENTS",
                    parents.join("_").to_uppercase().replace('-', "_")
                ),
            };
            (name, parents.clone())
        })
        .collect()
}

/// The Pascal word every name ends in, when they share one.
fn common_trailing_word(names: &[&str]) -> Option<String> {
    let split: Vec<Vec<&str>> = names.iter().map(|name| pascal_words(name)).collect();
    let (first, rest) = split.split_first()?;
    let mut shared = 0;
    while first.len() > shared
        && rest.iter().all(|words| {
            words.len() > shared
                && words[words.len() - shared - 1] == first[first.len() - shared - 1]
        })
    {
        shared += 1;
    }
    if shared == 0 {
        return None;
    }
    Some(first[first.len() - shared..].concat())
}

/// A Pascal-case name split into its words.
fn pascal_words(name: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut start = 0;
    for (at, character) in name.char_indices().skip(1) {
        if character.is_uppercase() {
            words.push(&name[start..at]);
            start = at;
        }
    }
    words.push(&name[start..]);
    words
}
