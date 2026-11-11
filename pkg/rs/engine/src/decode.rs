//! Decoding scalar values against compiled type expressions: the check a
//! binder runs on every bound scalar and the compiler runs on every
//! literal a definition writes.

use std::collections::BTreeMap;

use kdlith_model::{Asset, Entry, Location, QualifiedName, Reference, Value};
use regex::Regex;

use crate::definition::{Qualification, ReferenceDecl};
use crate::extensions::TYPE_EXPRESSION_VALIDATOR;
use crate::names::{Names, Sym};
use crate::pattern::anchored;
use crate::syntax::parse_u64;
use crate::type_expr::{self, ExprError};
use crate::types::{Builtin, CodecDecl, StringType, TypeCatalog, TypeExpr, TypeId, TypeKind};

/// The compiled patterns of one definition's string types and codecs,
/// built once.
#[derive(Clone, Debug, Default)]
pub(crate) struct Decoder {
    patterns: BTreeMap<TypeId, Regex>,
    codecs: BTreeMap<TypeId, Regex>,
    references: BTreeMap<Sym, ReferenceSyntax>,
}

/// How a field typed by a declared reference type decodes its spelling.
#[derive(Clone, Debug)]
struct ReferenceSyntax {
    /// The lookup domain, else the target's.
    domain: String,
    codec: TypeId,
    /// How each of the codec's captures is read, in capture order.
    captures: Vec<CaptureSyntax>,
    /// The qualification may take a missing qualifier from context.
    contextual: bool,
}

/// How one capture of a reference's codec is read.
#[derive(Clone, Copy, Debug)]
struct CaptureSyntax {
    /// The capture's group in the codec's pattern, when it has one.
    group: Option<usize>,
    /// The capture is a qualifier, not a name segment.
    qualifier: bool,
    /// The capture's type is numeric, so its text is a number.
    numeric: bool,
}

/// Where a value is checked: its site, resolved only when the decoded
/// value records it, and the qualifier-context value in effect there —
/// the value of the slot the reference's binding chose — which a
/// reference whose qualification may come from context carries.
#[derive(Clone, Copy)]
pub(crate) struct At<'a> {
    pub(crate) site: &'a dyn Fn() -> Location,
    pub(crate) context: Option<&'a str>,
}

/// What checking one value against a type came to.
#[derive(Debug)]
pub(crate) enum Decoded {
    /// The value fits as it is.
    Same,
    /// The value decodes to another one: a spelling to the reference,
    /// asset, or codec output it spells.
    Changed(Value),
}

/// A string type or codec whose pattern does not compile.
pub(crate) struct BadPattern {
    pub(crate) ty: TypeId,
    pub(crate) reason: String,
}

impl Decoder {
    /// Compile every string type's and codec's pattern for whole-string
    /// matching.
    pub(crate) fn new(types: &TypeCatalog) -> (Self, Vec<BadPattern>) {
        let mut decoder = Self::default();
        let mut bad = Vec::new();
        for (id, decl) in types.iter() {
            let pattern = match &decl.kind {
                TypeKind::String(StringType {
                    pattern: Some(pattern),
                    ..
                }) => Some(pattern),
                TypeKind::Codec(codec) => Some(&codec.pattern),
                _ => None,
            };
            let Some(pattern) = pattern else {
                continue;
            };
            match anchored(pattern) {
                Ok(regex) => {
                    if matches!(&decl.kind, TypeKind::Codec(_)) {
                        decoder.codecs.insert(id, regex);
                    } else {
                        decoder.patterns.insert(id, regex);
                    }
                }
                Err(error) => bad.push(BadPattern {
                    ty: id,
                    reason: error.to_string(),
                }),
            }
        }
        (decoder, bad)
    }

    /// Install the declared reference types, so a field typed by one
    /// decodes through its codec. A declared reference type wins over a
    /// same-named codec: the codec is reached through it.
    pub(crate) fn with_references(
        mut self,
        types: &TypeCatalog,
        names: &Names,
        references: &[ReferenceDecl],
    ) -> Self {
        for reference in references {
            let codec = reference.codec.and_then(|codec| types.lookup(codec));
            let Some(codec) = codec.filter(|id| self.codecs.contains_key(id)) else {
                continue;
            };
            let qualifiers: Vec<&str> = reference
                .qualifiers
                .iter()
                .map(|q| names.resolve(q.name))
                .collect();
            let groups: Vec<Option<&str>> = self
                .codecs
                .get(&codec)
                .map(|regex| regex.capture_names().collect())
                .unwrap_or_default();
            let captures = match types.get(codec).map(|decl| &decl.kind) {
                Some(TypeKind::Codec(decl)) => decl
                    .captures
                    .iter()
                    .map(|capture| CaptureSyntax {
                        group: groups
                            .iter()
                            .position(|group| *group == Some(capture.name.as_str())),
                        qualifier: qualifiers.contains(&capture.name.as_str()),
                        numeric: numeric_builtin(types, &capture.ty).is_some(),
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let syntax = ReferenceSyntax {
                domain: names
                    .resolve(reference.lookup.unwrap_or(reference.target))
                    .to_owned(),
                codec,
                captures,
                contextual: reference.qualification != Qualification::Explicit,
            };
            self.references.insert(reference.name, syntax);
        }
        self
    }

    /// Decode `value` as `ty`: `Some` with the decoded value (a `ref<T>`
    /// string becomes a reference; everything else is unchanged), or
    /// `None` when the value does not fit, leaving it to the caller's
    /// diagnostic.
    pub(crate) fn decode(
        &self,
        types: &TypeCatalog,
        names: &Names,
        ty: &TypeExpr,
        value: &Value,
        site: Location,
    ) -> Option<Value> {
        let at = At {
            site: &|| site,
            context: None,
        };
        Some(match self.check(types, names, ty, value, at)? {
            Decoded::Same => value.clone(),
            Decoded::Changed(decoded) => decoded,
        })
    }

    /// Check `value` against `ty` at a bind position, without copying a
    /// value that fits as it is: `Some` says whether the value stands or
    /// what it decodes to, `None` that it does not fit, leaving it to
    /// the caller's diagnostic.
    pub(crate) fn check(
        &self,
        types: &TypeCatalog,
        names: &Names,
        ty: &TypeExpr,
        value: &Value,
        at: At<'_>,
    ) -> Option<Decoded> {
        match ty {
            TypeExpr::Optional(inner) => self.check(types, names, inner, value, at),
            TypeExpr::Nullable(inner) => {
                if *value == Value::Null {
                    Some(Decoded::Same)
                } else {
                    self.check(types, names, inner, value, at)
                }
            }
            TypeExpr::Ref(inner) => match (inner.as_ref(), value) {
                (TypeExpr::Named(target), Value::String(text)) => {
                    let name = QualifiedName(vec![text.clone()]);
                    let domain = names.resolve(*target);
                    let reference = Reference::new(domain, name, text.clone(), (at.site)());
                    Some(Decoded::Changed(Value::Reference(reference)))
                }
                (TypeExpr::Named(target), Value::Reference(reference))
                    if reference.domain == names.resolve(*target) =>
                {
                    Some(Decoded::Same)
                }
                _ => None,
            },
            // Composite values arrive already bound, element by element;
            // checking them again walks the same types.
            TypeExpr::List(inner) => {
                let Value::List(items) = value else {
                    return None;
                };
                let mut changed: Option<Vec<Value>> = None;
                for (position, item) in items.iter().enumerate() {
                    match self.check(types, names, inner, item, at)? {
                        Decoded::Same => {
                            if let Some(decoded) = &mut changed {
                                decoded.push(item.clone());
                            }
                        }
                        Decoded::Changed(element) => changed
                            .get_or_insert_with(|| items[..position].to_vec())
                            .push(element),
                    }
                }
                Some(changed.map_or(Decoded::Same, |items| Decoded::Changed(Value::List(items))))
            }
            TypeExpr::Map(key, inner) | TypeExpr::OrderedMap(key, inner) => {
                let Value::Map(entries) = value else {
                    return None;
                };
                let mut changed: Option<Vec<Entry>> = None;
                for (position, entry) in entries.iter().enumerate() {
                    self.check(types, names, key, &entry.key_value(), at)?;
                    match self.check(types, names, inner, &entry.value, at)? {
                        Decoded::Same => {
                            if let Some(decoded) = &mut changed {
                                decoded.push(entry.clone());
                            }
                        }
                        Decoded::Changed(held) => {
                            let mut decoded = entry.clone();
                            decoded.value = held;
                            changed
                                .get_or_insert_with(|| entries[..position].to_vec())
                                .push(decoded);
                        }
                    }
                }
                Some(changed.map_or(Decoded::Same, |entries| {
                    Decoded::Changed(Value::Map(entries))
                }))
            }
            TypeExpr::Union(members) => members
                .iter()
                .find_map(|member| self.check(types, names, member, value, at)),
            // A surviving instantiation means incomplete checking: fail
            // closed rather than guess the unsubstituted body.
            TypeExpr::Instantiation(_, _) => None,
            TypeExpr::Named(name) => {
                if let Some(syntax) = self.references.get(name) {
                    return self.reference(types, names, syntax, value, at);
                }
                let id = types.lookup(*name)?;
                self.named(types, names, id, value, at)
            }
        }
    }

    fn named(
        &self,
        types: &TypeCatalog,
        names: &Names,
        id: TypeId,
        value: &Value,
        at: At<'_>,
    ) -> Option<Decoded> {
        let decl = types.get(id)?;
        match (&decl.kind, value) {
            (TypeKind::Builtin(builtin), value) => {
                builtin_accepts(*builtin, value).then_some(Decoded::Same)
            }
            (TypeKind::String(string), Value::String(text)) => {
                self.string(id, string, text).then_some(Decoded::Same)
            }
            (TypeKind::Enum(enumeration), Value::String(text)) => enumeration
                .members
                .iter()
                .any(|member| member.name == *text)
                .then_some(Decoded::Same),
            // A prefix selects its variant with no fallback on failure; a
            // scalar no prefix selects binds as the first un-prefixed
            // variant, in declaration order, whose type accepts it.
            (TypeKind::Union(union), Value::String(_) | Value::Number(_) | Value::Bool(_)) => {
                if let Value::String(text) = value
                    && let Some(variant) = union.variants.iter().find(|variant| {
                        variant
                            .prefix
                            .as_deref()
                            .is_some_and(|prefix| text.starts_with(prefix))
                    })
                {
                    return self.check(types, names, &variant.ty, value, at);
                }
                union
                    .variants
                    .iter()
                    .filter(|variant| variant.prefix.is_none())
                    .find_map(|variant| self.check(types, names, &variant.ty, value, at))
            }
            // A codec decodes a matching string in the stream; the
            // decoded value carries the output type.
            (TypeKind::Codec(codec), Value::String(text)) => self
                .codec(types, names, id, codec, text, (at.site)())
                .map(Decoded::Changed),
            // An asset's spelling binds as written; the reading resolves
            // it in the source of the document that wrote it.
            (TypeKind::Asset(_), Value::String(text)) => Some(Decoded::Changed(Value::Asset(
                Asset::written(names.resolve(decl.name), text.clone(), (at.site)()),
            ))),
            (TypeKind::Asset(_), Value::Asset(_)) => Some(Decoded::Same),
            // Object, union, and map values are bound from node structure,
            // which the binding already checked field by field.
            (TypeKind::Object(_) | TypeKind::Union(_) | TypeKind::Map(_), Value::Map(_)) => {
                Some(Decoded::Same)
            }
            _ => None,
        }
    }

    /// Decode a reference's spelling through its codec: qualifier-named
    /// captures become qualifiers, the rest the name's segments in
    /// capture order; a spelling yielding no segment is no reference.
    fn reference(
        &self,
        types: &TypeCatalog,
        names: &Names,
        syntax: &ReferenceSyntax,
        value: &Value,
        at: At<'_>,
    ) -> Option<Decoded> {
        let text = match value {
            Value::String(text) => text,
            Value::Reference(reference) if reference.domain == syntax.domain => {
                return Some(Decoded::Same);
            }
            _ => return None,
        };
        let TypeKind::Codec(codec) = &types.get(syntax.codec)?.kind else {
            return None;
        };
        let captures = self.codecs.get(&syntax.codec)?.captures(text)?;
        let mut segments = Vec::new();
        let mut qualifiers = Vec::new();
        for (capture, how) in codec.captures.iter().zip(&syntax.captures) {
            let Some(found) = how.group.and_then(|group| captures.get(group)) else {
                continue;
            };
            // A capture's text decodes as its declared type: a numeric
            // capture carries its text as the number it spells.
            let raw = found.as_str().to_owned();
            let typed = if how.numeric {
                Value::Number(raw)
            } else {
                Value::String(raw)
            };
            self.check(types, names, &capture.ty, &typed, at)?;
            let (Value::Number(raw) | Value::String(raw)) = typed else {
                return None;
            };
            if how.qualifier {
                qualifiers.push((capture.name.clone(), raw));
            } else {
                segments.push(raw);
            }
        }
        if segments.is_empty() {
            return None;
        }
        let mut reference = Reference::new(
            syntax.domain.clone(),
            QualifiedName(segments),
            text.clone(),
            (at.site)(),
        );
        reference.qualifiers = qualifiers;
        if syntax.contextual {
            reference.context = at.context.map(Box::from);
        }
        Some(Decoded::Changed(Value::Reference(reference)))
    }

    /// Decode a string through a codec: the whole-string match, each
    /// named capture decoded as its declared type, and the result
    /// carrying the codec's output type.
    fn codec(
        &self,
        types: &TypeCatalog,
        names: &Names,
        id: TypeId,
        codec: &CodecDecl,
        text: &str,
        site: Location,
    ) -> Option<Value> {
        let regex = self.codecs.get(&id)?;
        let captures = regex.captures(text)?;
        let mut entries = Vec::with_capacity(codec.captures.len());
        for capture in &codec.captures {
            let raw = captures
                .name(&capture.name)
                .map(|found| found.as_str().to_owned())
                .unwrap_or_default();
            // A capture's text decodes as its declared type: a numeric
            // capture carries its text as the number it spells.
            let value = match numeric_builtin(types, &capture.ty) {
                Some(_) => Value::Number(raw),
                None => Value::String(raw),
            };
            let decoded = self.decode(types, names, &capture.ty, &value, site)?;
            entries.push(Entry::new(capture.name.clone(), decoded));
        }
        let decoded = Value::Map(entries);
        if let Some(value) = self.decode(types, names, &codec.output, &decoded, site) {
            return Some(value);
        }
        // A single capture may decode straight into a scalar output.
        if codec.captures.len() == 1
            && let Value::Map(mut pair) = decoded
            && let Some(entry) = pair.pop()
        {
            return self.decode(types, names, &codec.output, &entry.value, site);
        }
        None
    }

    fn string(&self, id: TypeId, string: &StringType, text: &str) -> bool {
        if let Some(rule) = string.format
            && !rule.format.matches(text, rule.width)
        {
            return false;
        }
        if string.pattern.is_some() && !self.patterns.get(&id).is_some_and(|re| re.is_match(text)) {
            return false;
        }
        if let Some(min) = string.min_length
            && (text.chars().count() as u64) < min
        {
            return false;
        }
        match &string.validator {
            None => true,
            // The only validator the engine implements; any other one
            // fails closed rather than passing unchecked.
            Some(validator) if validator.name == TYPE_EXPRESSION_VALIDATOR => {
                type_expr::parse(text, &mut Names::default()).is_ok()
            }
            Some(_) => false,
        }
    }
}

/// Why a value did not decode, as precisely as one diagnostic code can
/// say it. Every reading path classifies a failed value the same way,
/// so one rejected construct gets one code.
pub(crate) enum Mismatch {
    /// The value is not of the type.
    Value,
    /// A string naming no member of the enum type `ty`.
    Member { ty: String },
    /// A string the type-expression validator rejects.
    TypeExpression { reason: String },
}

/// Classify a value that failed to decode as `ty`.
pub(crate) fn mismatch(
    types: &TypeCatalog,
    names: &Names,
    ty: &TypeExpr,
    value: &Value,
) -> Mismatch {
    let mut ty = ty;
    while let TypeExpr::Optional(inner) | TypeExpr::Nullable(inner) = ty {
        ty = inner;
    }
    let (TypeExpr::Named(name), Value::String(text)) = (ty, value) else {
        return Mismatch::Value;
    };
    let Some(decl) = types.lookup(*name).and_then(|id| types.get(id)) else {
        return Mismatch::Value;
    };
    match &decl.kind {
        TypeKind::Enum(_) => {
            let ty = names.resolve(*name);
            Mismatch::Member { ty: ty.to_owned() }
        }
        TypeKind::String(string)
            if string
                .validator
                .as_ref()
                .is_some_and(|v| v.name == TYPE_EXPRESSION_VALIDATOR) =>
        {
            match type_expr::parse(text, &mut Names::default()) {
                Ok(_) => Mismatch::Value,
                Err(ExprError::Syntax(reason)) => Mismatch::TypeExpression { reason },
            }
        }
        _ => Mismatch::Value,
    }
}

/// The numeric builtin a type expression names, if any: a codec capture
/// declared as one carries its matched text as a number.
fn numeric_builtin(types: &TypeCatalog, ty: &TypeExpr) -> Option<Builtin> {
    let TypeExpr::Named(name) = ty else {
        return None;
    };
    let builtin = types.lookup(*name).and_then(|id| types.get(id))?;
    match builtin.kind {
        TypeKind::Builtin(builtin @ (Builtin::Integer | Builtin::Decimal | Builtin::U64)) => {
            Some(builtin)
        }
        _ => None,
    }
}

fn builtin_accepts(builtin: Builtin, value: &Value) -> bool {
    match (builtin, value) {
        (Builtin::String, Value::String(_)) | (Builtin::Bool, Value::Bool(_)) => true,
        (Builtin::Integer, Value::Number(text)) => class(text) == Class::Integer,
        (Builtin::U64, Value::Number(text)) => parse_u64(text).is_some(),
        (Builtin::Decimal, Value::Number(text)) => class(text) != Class::Special,
        (Builtin::ValueExpr, value) => matches!(
            value,
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
        ),
        _ => false,
    }
}

#[derive(PartialEq, Eq)]
pub(crate) enum Class {
    Integer,
    Fraction,
    /// `#inf`, `#-inf`, or `#nan`.
    Special,
}

/// The lexical class of a number spelling as the parser keeps it.
pub(crate) fn class(text: &str) -> Class {
    if text.starts_with('#') {
        return Class::Special;
    }
    let digits = text.trim_start_matches(['+', '-']);
    let radix = ["0x", "0o", "0b"].iter().any(|p| digits.starts_with(p));
    if !radix && digits.contains(['.', 'e', 'E']) {
        Class::Fraction
    } else {
        Class::Integer
    }
}

/// A short rendering of a value for diagnostics.
pub(crate) fn found(value: &Value) -> String {
    match value {
        Value::Null => "#null".to_owned(),
        Value::Bool(flag) => format!("#{flag}"),
        Value::Number(text) => format!("number {text}"),
        Value::String(text) => format!("string {text:?}"),
        Value::List(_) => "a list".to_owned(),
        Value::Map(_) => "a node with fields".to_owned(),
        Value::Reference(reference) => format!("reference {:?}", reference.written),
        Value::Asset(asset) => format!("asset {:?}", asset.location),
        _ => "an unsupported value".to_owned(),
    }
}
