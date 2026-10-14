//! File emission under the pinned writer profile: pre-formatted text,
//! LF newlines, fixed indentation, the generated marker first, and
//! width-pinned wrapping.

use std::fmt::Write as _;

use kdlith_engine::{DefinitionKind, Names, Role, TypeDecl, TypeExpr, TypeKind};

use crate::Error;
use crate::catalog;
use crate::manifest::{MANIFEST_NAME, manifest_text};
use crate::model::{Document, Emission, kind_name};
use crate::render::{field_name, named_references, pascal, rust_type};

/// Width a `use` list or other greedy-wrapped list fills to.
const LIST_WIDTH: usize = 100;
/// Width a builder chain still takes on one line.
const CHAIN_WIDTH: usize = 65;

const BUILTINS: [&str; 6] = ["String", "Bool", "Integer", "Decimal", "U64", "ValueExpr"];

/// A generated crate: every file's path and complete content, sorted by
/// path. Deterministic by construction — the same definition, crate
/// name, and generator version always yield equal trees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedTree {
    pub(crate) files: Vec<(String, String)>,
}

impl GeneratedTree {
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

    /// A `use path::{names};` statement: single name unwrapped, one line
    /// when it fits, greedily wrapped lines otherwise.
    fn use_list(&mut self, path: &str, names: &[String]) {
        if names.is_empty() {
            return;
        }
        if let [only] = names {
            self.line(&format!("use {path}::{only};"));
            return;
        }
        let single = format!("use {}::{{{}}};", path, names.join(", "));
        if single.len() <= LIST_WIDTH {
            self.line(&single);
            return;
        }
        self.line(&format!("use {path}::{{"));
        let mut buffer = String::new();
        for name in names {
            // The line is the four-space indent plus the buffer; adding
            // a name costs its own width plus ", ".
            if !buffer.is_empty() && 4 + buffer.len() + 2 + name.len() + 1 > LIST_WIDTH {
                self.line(&format!("    {buffer},"));
                buffer.clear();
            }
            if !buffer.is_empty() {
                buffer.push_str(", ");
            }
            buffer.push_str(name);
        }
        if !buffer.is_empty() {
            self.line(&format!("    {buffer},"));
        }
        self.line("};");
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
        files.push((
            MANIFEST_NAME.to_owned(),
            manifest_text(files.iter().map(|(path, _)| path.as_str())),
        ));
        files.sort_by(|(one, _), (other, _)| one.cmp(other));
        Ok(GeneratedTree { files })
    }

    /// The crate manifest: standalone, no workspace inheritance, path
    /// dependencies on the sibling engine crates under the pinned
    /// layout.
    fn cargo_toml(&self) -> String {
        let mut out = Out::new();
        out.line(crate::COMMENT_MARKER);
        out.blank();
        out.line("[package]");
        out.line(&format!("name = \"{}\"", self.crate_name));
        out.line(&format!("version = \"{}\"", env!("CARGO_PKG_VERSION")));
        out.line("edition = \"2024\"");
        out.line("rust-version = \"1.85\"");
        out.line("autotests = false");
        out.blank();
        out.line("[lints.clippy]");
        out.line("all = { level = \"warn\", priority = -1 }");
        out.blank();
        out.line("[dependencies]");
        out.line("kdlith-diagnostic = { path = \"../diagnostic\" }");
        out.line("kdlith-engine = { path = \"../engine\" }");
        match self.kind {
            DefinitionKind::Dialect => {
                out.line("kdlith-runtime = { path = \"../runtime\" }");
            }
            DefinitionKind::Schema => {
                out.line("kdlith-superproc = { path = \"../superproc\" }");
            }
            _ => {}
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
        match self.kind {
            DefinitionKind::Dialect => {
                out.line("pub use compile::{load_dialect, load_schema, seed};");
            }
            _ => out.line("pub use compile::load;"),
        }
        out.line("pub use processors::{DECLARATIONS, Item, reader};");
        rust_file(&out.finish())
    }

    /// The per-document type declarations: one declaration macro per
    /// declared type, in contribution order, then the shape table.
    fn types_doc_rs(&self, doc: &Document<'_>) -> Result<String, Error> {
        let declared: Vec<&str> = doc
            .types
            .iter()
            .map(|decl| self.names.resolve(decl.name))
            .collect();
        let mut engine: Vec<String> = vec!["Shaped".into(), "TypeShape".into()];
        let mut references: Vec<String> = Vec::new();
        let mut macros: Vec<String> = Vec::new();
        for decl in &doc.types {
            let name = self.names.resolve(decl.name);
            for expr in declaration_expressions(decl) {
                if matches!(expr, TypeExpr::OrderedMap(_, _))
                    && !engine.iter().any(|n| n == "OrderedMap")
                {
                    engine.push("OrderedMap".to_owned());
                }
                references.extend(named_references(expr, self.names));
            }
            match &decl.kind {
                TypeKind::String(_) => push_unique(&mut macros, "string_type"),
                TypeKind::Enum(_) => push_unique(&mut macros, "enum_type"),
                TypeKind::Object(_) => push_unique(&mut macros, "object_type"),
                TypeKind::Union(_) => push_unique(&mut macros, "union_type"),
                TypeKind::Builtin(_) => unreachable!("built-ins carry no site"),
                kind => {
                    return Err(Error::UnsupportedTypeKind {
                        name: name.to_owned(),
                        kind: kind_name(kind),
                    });
                }
            }
        }
        if references.iter().any(|name| name == "ValueExpr")
            && !engine.iter().any(|name| name == "ValueExpr")
        {
            engine.push("ValueExpr".to_owned());
        }
        engine.extend(macros);
        engine.sort();
        let mut super_names: Vec<String> = references
            .iter()
            .filter(|name| !BUILTINS.contains(&name.as_str()) && !declared.contains(&name.as_str()))
            .cloned()
            .collect();
        super_names.sort();
        super_names.dedup();

        let mut out = Out::new();
        match catalog::types_header(&doc.file_name) {
            Some(lines) => out.doc(lines),
            None => out.doc(&[&format!(
                "Logical types contributed by `{}`.",
                doc.file_name
            )]),
        }
        out.blank();
        out.use_list("kdlith_engine", &engine);
        if !super_names.is_empty() {
            out.blank();
            out.use_list("super", &super_names);
        }
        for decl in &doc.types {
            out.blank();
            self.type_declaration(&mut out, decl)?;
        }
        out.blank();
        out.line("/// The shapes of this document's types, in declaration order.");
        out.line("pub(crate) const TYPES: &[TypeShape] = &[");
        for decl in &doc.types {
            out.line(&format!("    {}::SHAPE,", self.names.resolve(decl.name)));
        }
        out.line("];");
        Ok(rust_file(&out.finish()))
    }

    /// One declared type, as its declaration macro spells it.
    fn type_declaration(&self, out: &mut Out, decl: &TypeDecl) -> Result<(), Error> {
        let name = self.names.resolve(decl.name).to_owned();
        let doc = catalog::type_doc(&name);
        match &decl.kind {
            TypeKind::String(_) => {
                out.line("string_type! {");
                for line in doc.unwrap_or(&[]) {
                    out.line(&format!("    /// {line}"));
                }
                out.line(&format!("    pub struct {name};"));
                out.line("}");
            }
            TypeKind::Enum(enumeration) => {
                out.line("enum_type! {");
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
                out.line("object_type! {");
                for line in doc.unwrap_or(&[]) {
                    out.line(&format!("    /// {line}"));
                }
                if object.fields.is_empty() && doc.is_none() {
                    out.line(&format!("    pub struct {name} {{}}"));
                } else {
                    out.line(&format!("    pub struct {name} {{"));
                    for field in &object.fields {
                        let key = self.names.resolve(field.name);
                        if let Some(lines) = catalog::field_comment(&name, key) {
                            for line in lines {
                                out.line(&format!("        // {line}"));
                            }
                        }
                        out.line(&format!(
                            "        \"{}\" => {}: {},",
                            key,
                            field_name(key),
                            rust_type(&field.ty, self.names)?
                        ));
                    }
                    out.line("    }");
                }
                out.line("}");
            }
            TypeKind::Union(union) => {
                out.line("union_type! {");
                for line in doc.unwrap_or(&[]) {
                    out.line(&format!("    /// {line}"));
                }
                out.line(&format!("    pub enum {name} {{"));
                for variant in &union.variants {
                    let discriminator = self.names.resolve(variant.discriminator);
                    out.line(&format!(
                        "        \"{}\" => {}({}),",
                        discriminator,
                        pascal(discriminator),
                        rust_type(&variant.ty, self.names)?
                    ));
                }
                out.line("    }");
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
        let modules: Vec<String> = self
            .type_documents()
            .map(|doc| doc.module.clone())
            .collect();
        // Module declarations and re-exports are alphabetical; the
        // shape table below stays in contribution order.
        let mut sorted = modules.clone();
        sorted.sort();
        for module in &sorted {
            out.line(&format!("mod {module};"));
        }
        out.blank();
        out.line("use kdlith_engine::TypeShape;");
        out.blank();
        for module in &sorted {
            out.line(&format!("pub use {module}::*;"));
        }
        out.blank();
        out.line("/// Every type's shape, by contributing document.");
        out.line("pub(crate) const TYPES: &[&[TypeShape]] = &[");
        for module in &modules {
            out.line(&format!("    {module}::TYPES,"));
        }
        out.line("];");
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
                role: node.role,
                ty: node.ty.map(|ty| self.names.resolve(ty).to_owned()),
            })
            .collect();
        for (name, parents) in parent_constants(&entries) {
            out.blank();
            out.line(&format!(
                "const {}: &[&str] = &[{}];",
                name,
                parents
                    .iter()
                    .map(|parent| format!("\"{parent}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        out.blank();
        out.line("pub(crate) const NODES: &[NodeShape] = &[");
        for entry in &entries {
            let constant = parent_constants(&entries)
                .into_iter()
                .find(|(_, parents)| parents == &entry.parents)
                .map(|(name, _)| name);
            out.line(&node_shape(entry, constant.as_deref()));
        }
        out.line("];");
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
        sorted.sort();
        for module in &sorted {
            out.line(&format!("mod {module};"));
        }
        out.blank();
        out.line("use kdlith_engine::NodeShape;");
        out.blank();
        out.line("/// Every node declaration, by contributing document.");
        out.line("pub(crate) const NODES: &[&[NodeShape]] = &[");
        for module in &modules {
            out.line(&format!("    {module}::NODES,"));
        }
        out.line("];");
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
            sorted.sort();
            for module in &sorted {
                out.line(&format!("mod {module};"));
            }
        }
        out.blank();
        out.line("use kdlith_engine::SlotShape;");
        out.blank();
        out.line("/// Every context slot, by contributing document.");
        if slots == 0 {
            out.line("pub(crate) const SLOTS: &[&[SlotShape]] = &[];");
        } else {
            out.line("pub(crate) const SLOTS: &[&[SlotShape]] = &[");
            for doc in self.slot_documents() {
                out.line(&format!("    {}::SLOTS,", doc.module));
            }
            out.line("];");
        }
        rust_file(&out.finish())
    }

    /// One document's context slots.
    fn contexts_doc_rs(&self, doc: &Document<'_>) -> Result<String, Error> {
        let mut references: Vec<String> = Vec::new();
        for slot in &doc.slots {
            references.extend(named_references(&slot.ty, self.names));
        }
        let mut types: Vec<String> = references
            .into_iter()
            .filter(|name| !BUILTINS.contains(&name.as_str()))
            .collect();
        types.sort();
        types.dedup();

        // A slot whose type is a plain named type spells the expression
        // through the `LogicalType` trait, which the file must import;
        // constructor types spell it fully qualified and need nothing.
        let mut engine = vec!["SlotShape".to_owned()];
        let mut short_form = false;
        for slot in &doc.slots {
            if !rust_type(&slot.ty, self.names)?.contains('<') {
                short_form = true;
            }
        }
        if short_form {
            engine.push("LogicalType".to_owned());
        }

        let mut out = Out::new();
        out.doc(&[&format!(
            "Context slots contributed by `{}`.",
            doc.file_name
        )]);
        out.blank();
        out.use_list("kdlith_engine", &engine);
        if !types.is_empty() {
            out.blank();
            out.use_list("crate::types", &types);
        }
        out.blank();
        out.line("pub(crate) const SLOTS: &[SlotShape] = &[");
        for slot in &doc.slots {
            let name = self.names.resolve(slot.name);
            let ty = rust_type(&slot.ty, self.names)?;
            let expression = if ty.contains('<') {
                format!("<{ty} as LogicalType>::expression")
            } else {
                format!("{ty}::expression")
            };
            let mut segments = vec![format!("SlotShape::new(\"{name}\", {expression})")];
            if let Some(namespace) = slot.namespace {
                segments.push(format!(
                    ".in_namespace(\"{}\")",
                    self.names.resolve(namespace)
                ));
            }
            let single = format!("    {},", segments.join(""));
            if single.len() <= CHAIN_WIDTH {
                out.line(&single);
            } else {
                out.line(&format!("    {}", segments[0]));
                for segment in &segments[1..] {
                    out.line(&format!("        {segment}"));
                }
            }
        }
        out.line("];");
        Ok(rust_file(&out.finish()))
    }

    /// The processors module root: the item enum, the handler table, the
    /// declarations, and the one reader.
    fn processors_mod_rs(&self) -> Result<String, Error> {
        let item_types: Vec<String> = self.items.iter().map(|item| item.ty.clone()).collect();
        let mut sorted = item_types.clone();
        sorted.sort();
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
        mods_sorted.sort();
        for module in &mods_sorted {
            out.line(&format!("mod {module};"));
        }
        out.blank();
        out.line("use kdlith_diagnostic::DiagnosticRecord;");
        out.line(
            "use kdlith_engine::{Declarations, Definition, DefinitionKind, Handler, TypedReader, item_type};",
        );
        out.blank();
        out.use_list("crate::types", &sorted);
        out.line("use crate::{bindings, contexts, types};");
        out.blank();
        out.line("item_type! {");
        match self.kind {
            DefinitionKind::Dialect => {
                out.line("    /// A record or declaration of a definition written in the dialect.");
            }
            _ => out.line("    /// A record or declaration of a document written in the schema."),
        }
        out.line("    pub enum Item {");
        for ty in &item_types {
            out.line(&format!("        {ty},"));
        }
        out.line("    }");
        out.line("}");
        out.blank();
        out.line("/// Every handler, by contributing document.");
        out.line("const HANDLERS: &[&[Handler<Item>]] = &[");
        for module in &modules {
            out.line(&format!("    {module}::HANDLERS,"));
        }
        out.line("];");
        out.blank();
        let (kind_word, kind_path) = match self.kind {
            DefinitionKind::Dialect => ("dialect", "Dialect"),
            _ => ("schema", "Schema"),
        };
        out.line(&format!(
            "/// Everything this processor declares about the {kind_word} it reads."
        ));
        out.line("pub static DECLARATIONS: Declarations<Item> =");
        out.line(&format!(
            "    Declarations::new(DefinitionKind::{kind_path}, \"{}\", \"{}\")",
            self.name, self.version
        ));
        out.line("        .types(types::TYPES)");
        out.line("        .nodes(bindings::NODES)");
        out.line("        .slots(contexts::SLOTS)");
        out.line("        .handlers(HANDLERS);");
        out.blank();
        match self.kind {
            DefinitionKind::Dialect => {
                out.line("/// A reader of definitions written in the compiled dialect `dialect`.");
                out.line("pub fn reader(dialect: &Definition) -> Result<TypedReader<'_, Item>, Vec<DiagnosticRecord>> {");
                out.line("    TypedReader::new(dialect, &DECLARATIONS)");
            }
            _ => {
                out.line("/// A reader of documents written under the compiled schema `schema`.");
                out.line("pub fn reader(schema: &Definition) -> Result<TypedReader<'_, Item>, Vec<DiagnosticRecord>> {");
                out.line("    TypedReader::new(schema, &DECLARATIONS)");
            }
        }
        out.line("}");
        Ok(rust_file(&out.finish()))
    }

    /// One document's handlers, in node declaration order.
    fn processors_doc_rs(&self, doc: &Document<'_>) -> String {
        let items: Vec<(&str, bool)> = self
            .items
            .iter()
            .filter(|item| doc.nodes.iter().any(|node| std::ptr::eq(*node, item.node)))
            .map(|item| (item.ty.as_str(), item.record))
            .collect();
        let mut types: Vec<String> = items.iter().map(|(ty, _)| (*ty).to_owned()).collect();
        types.sort();
        types.dedup();

        let mut out = Out::new();
        match catalog::processors_header(&doc.file_name) {
            Some(lines) => out.doc(lines),
            None => out.doc(&[&format!(
                "Handlers for the items `{}` declares.",
                doc.file_name
            )]),
        }
        out.blank();
        out.line("use kdlith_engine::Handler;");
        out.blank();
        out.line("use super::Item;");
        out.use_list("crate::types", &types);
        out.blank();
        let handlers: Vec<String> = items
            .iter()
            .map(|(ty, record)| {
                if *record {
                    format!("Handler::record::<{ty}>()")
                } else {
                    format!("Handler::declaration::<{ty}>()")
                }
            })
            .collect();
        let single = format!(
            "pub(crate) const HANDLERS: &[Handler<Item>] = &[{}];",
            handlers.join(", ")
        );
        if single.len() <= LIST_WIDTH {
            out.line(&single);
        } else {
            out.line("pub(crate) const HANDLERS: &[Handler<Item>] = &[");
            for handler in &handlers {
                out.line(&format!("    {handler},"));
            }
            out.line("];");
        }
        rust_file(&out.finish())
    }
}

/// Every type expression a declaration references: an object's fields,
/// a union's variants; the other kinds reference none.
fn declaration_expressions(decl: &TypeDecl) -> Vec<&TypeExpr> {
    match &decl.kind {
        TypeKind::Object(object) => object.fields.iter().map(|field| &field.ty).collect(),
        TypeKind::Union(union) => union.variants.iter().map(|variant| &variant.ty).collect(),
        _ => Vec::new(),
    }
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
    role: Option<Role>,
    ty: Option<String>,
}

/// The resolved parent names of a node's placement, in declared order.
fn placement_parents(placement: &kdlith_engine::Placement, names: &Names) -> Vec<String> {
    match placement {
        kdlith_engine::Placement::Root => Vec::new(),
        kdlith_engine::Placement::Under(parents) => parents
            .iter()
            .map(|parent| names.resolve(*parent).to_owned())
            .collect(),
        // A placement kind this version does not know binds nowhere the
        // tables spell; no parent list is emitted for it.
        _ => Vec::new(),
    }
}

/// One node declaration's `NodeShape` entry, wrapped per the pinned
/// chain width.
fn node_shape(entry: &NodeEntry, constant: Option<&str>) -> String {
    let mut segments = vec![format!("NodeShape::new(\"{}\")", entry.name)];
    if !entry.parents.is_empty() {
        match constant {
            Some(constant) => segments.push(format!(".under({constant})")),
            None => segments.push(format!(
                ".under(&[{}])",
                entry
                    .parents
                    .iter()
                    .map(|parent| format!("\"{parent}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
    if let Some(role) = entry.role {
        segments.push(match role {
            Role::Record => ".record()".to_owned(),
            Role::Declaration => ".declaration()".to_owned(),
            Role::Contribution => ".contribution()".to_owned(),
            Role::ContextControl => ".context_control()".to_owned(),
            Role::SourceImport => ".source_import()".to_owned(),
            Role::Container => ".container()".to_owned(),
            _ => unreachable!("every role is spelled"),
        });
    }
    if let Some(ty) = &entry.ty {
        segments.push(format!(".ty(\"{ty}\")"));
    }
    let single = format!("    {},", segments.join(""));
    if single.len() <= CHAIN_WIDTH {
        return single;
    }
    let mut lines = format!("    {}", segments[0]);
    for (at, segment) in segments.iter().enumerate().skip(1) {
        if at + 1 == segments.len() {
            let _ = write!(lines, "\n        {segment},");
        } else {
            let _ = write!(lines, "\n        {segment}");
        }
    }
    lines
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
