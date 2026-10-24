//! Asset values: each location an asset-typed field holds, resolved in
//! the source of the document that wrote it and checked against its
//! asset type, as the reading binds the item holding it.

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_model::{Asset, Field, Item, ValidationState, Value};

use crate::definition::Definition;
use crate::locations::{Place, Sources};
use crate::types::TypeKind;

const ORIGIN: &str = "data-document";

/// Resolve every asset `item` holds, in the source `place` names (`None`
/// for a document held in memory). Each failure is returned located at
/// the asset's spelling in `shown`, and invalidates the item; a failed
/// asset keeps its written spelling.
pub(crate) fn resolve_item(
    def: &Definition,
    sources: &Sources,
    place: Option<&Place>,
    shown: &str,
    item: &mut Item,
) -> Vec<DiagnosticRecord> {
    let mut resolver = Resolver {
        def,
        sources,
        place,
        shown,
        failures: Vec::new(),
    };
    let validation = match item {
        Item::Record(record) => {
            resolver.fields(&mut record.fields);
            Some(&mut record.validation)
        }
        Item::Declaration(declaration) => {
            resolver.fields(&mut declaration.fields);
            Some(&mut declaration.validation)
        }
        Item::Contribution(contribution) => {
            resolver.value(&mut contribution.value);
            resolver.fields(&mut contribution.members);
            Some(&mut contribution.validation)
        }
        _ => None,
    };
    if !resolver.failures.is_empty()
        && let Some(validation) = validation
    {
        *validation = ValidationState::Invalid;
    }
    resolver.failures
}

struct Resolver<'r> {
    def: &'r Definition,
    sources: &'r Sources,
    place: Option<&'r Place>,
    shown: &'r str,
    failures: Vec<DiagnosticRecord>,
}

impl Resolver<'_> {
    fn fields(&mut self, fields: &mut [Field]) {
        for field in fields {
            self.value(&mut field.value);
        }
    }

    fn value(&mut self, value: &mut Value) {
        match value {
            Value::Asset(asset) => {
                if let Some([one]) = self.resolve(asset, false).as_deref() {
                    *asset = one.clone();
                }
            }
            Value::List(items) => {
                // A pattern item expands in place, its matches in
                // portable-path-byte order.
                let mut expanded = Vec::with_capacity(items.len());
                for mut item in std::mem::take(items) {
                    match &item {
                        Value::Asset(asset) => match self.resolve(asset, true) {
                            Some(resolved) => {
                                expanded.extend(resolved.into_iter().map(Value::Asset));
                            }
                            None => expanded.push(item),
                        },
                        _ => {
                            self.value(&mut item);
                            expanded.push(item);
                        }
                    }
                }
                *items = expanded;
            }
            Value::Map(entries) => {
                for entry in entries {
                    self.value(&mut entry.value);
                }
            }
            _ => {}
        }
    }

    /// The assets one written asset resolves to; `None` when it was
    /// already resolved, or when it fails, the failure recorded.
    fn resolve(&mut self, asset: &Asset, listed: bool) -> Option<Vec<Asset>> {
        let spelling = asset.spelling()?;
        let decl = self
            .def
            .names
            .get(&spelling.ty)
            .and_then(|name| self.def.types.lookup(name))
            .and_then(|id| self.def.types.get(id))
            .and_then(|decl| match &decl.kind {
                TypeKind::Asset(asset) => Some(asset),
                _ => None,
            })?;
        let resolved = self.sources.resolve_asset(
            &spelling.text,
            self.place,
            (&spelling.ty, decl),
            listed,
            ORIGIN,
        );
        match resolved {
            Ok(targets) => {
                let set = self.sources.fingerprint();
                Some(
                    targets
                        .into_iter()
                        .map(|target| {
                            Asset::resolved(target.shown, target.source, target.digest, set)
                        })
                        .collect(),
                )
            }
            Err(record) => {
                let start = spelling.site.span.start;
                let record = record.with_source(self.shown).with_span(
                    spelling.site.span.byte_span(),
                    Some(start.line),
                    Some(start.column),
                );
                self.failures.push(record);
                None
            }
        }
    }
}
