//! Decoding scalar values against compiled type expressions: the check a
//! binder runs on every bound scalar and the compiler runs on every
//! literal a definition writes.

use std::collections::BTreeMap;

use kdlith_model::{Entry, Location, QualifiedName, Reference, Value};
use regex::Regex;

use crate::deferred::{Spelling, deferred_by, defers, generic_spelling};
use crate::definition::Deferral;
use crate::extensions::TYPE_EXPRESSION_VALIDATOR;
use crate::names::Names;
use crate::syntax::parse_u64;
use crate::type_expr::{self, ExprError};
use crate::types::{Builtin, StringType, TypeCatalog, TypeExpr, TypeId, TypeKind};

/// The compiled patterns of one definition's string types, built once.
#[derive(Clone, Debug, Default)]
pub(crate) struct Decoder {
    patterns: BTreeMap<TypeId, Regex>,
}

/// A string type whose pattern does not compile.
pub(crate) struct BadPattern {
    pub(crate) ty: TypeId,
    pub(crate) reason: String,
}

impl Decoder {
    /// Compile every string type's pattern for whole-string matching.
    pub(crate) fn new(types: &TypeCatalog) -> (Self, Vec<BadPattern>) {
        let mut decoder = Self::default();
        let mut bad = Vec::new();
        for (id, decl) in types.iter() {
            let TypeKind::String(StringType {
                pattern: Some(pattern),
                ..
            }) = &decl.kind
            else {
                continue;
            };
            match Regex::new(&format!("^(?:{pattern})$")) {
                Ok(regex) => {
                    decoder.patterns.insert(id, regex);
                }
                Err(error) => bad.push(BadPattern {
                    ty: id,
                    reason: error.to_string(),
                }),
            }
        }
        (decoder, bad)
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
        match ty {
            TypeExpr::Optional(inner) => self.decode(types, names, inner, value, site),
            TypeExpr::Nullable(inner) => {
                if *value == Value::Null {
                    Some(Value::Null)
                } else {
                    self.decode(types, names, inner, value, site)
                }
            }
            TypeExpr::Ref(inner) => match (inner.as_ref(), value) {
                (TypeExpr::Named(target), Value::String(text)) => {
                    let name = QualifiedName(vec![text.clone()]);
                    let domain = names.resolve(*target);
                    let reference = Reference::new(domain, name, text.clone(), site);
                    Some(Value::Reference(reference))
                }
                (TypeExpr::Named(target), Value::Reference(reference))
                    if reference.domain == names.resolve(*target) =>
                {
                    Some(value.clone())
                }
                _ => None,
            },
            // Composite values arrive already bound, element by element;
            // checking them again walks the same types.
            TypeExpr::List(inner) => match value {
                Value::List(items) => items
                    .iter()
                    .map(|item| self.decode(types, names, inner, item, site))
                    .collect::<Option<Vec<_>>>()
                    .map(Value::List),
                _ => None,
            },
            TypeExpr::Map(key, inner) | TypeExpr::OrderedMap(key, inner) => match value {
                Value::Map(entries) => entries
                    .iter()
                    .map(|entry| {
                        let key_value = Value::String(entry.key.clone());
                        self.decode(types, names, key, &key_value, site)?;
                        let value = self.decode(types, names, inner, &entry.value, site)?;
                        Some(Entry::new(entry.key.clone(), value))
                    })
                    .collect::<Option<Vec<_>>>()
                    .map(Value::Map),
                _ => None,
            },
            TypeExpr::Union(members) => members
                .iter()
                .find_map(|member| self.decode(types, names, member, value, site)),
            TypeExpr::Named(name) => types
                .lookup(*name)
                .is_some_and(|id| self.named(types, id, value))
                .then(|| value.clone()),
        }
    }

    fn named(&self, types: &TypeCatalog, id: TypeId, value: &Value) -> bool {
        let Some(decl) = types.get(id) else {
            return false;
        };
        match (&decl.kind, value) {
            (TypeKind::Builtin(builtin), value) => builtin_accepts(*builtin, value),
            (TypeKind::String(string), Value::String(text)) => self.string(id, string, text),
            (TypeKind::Enum(enumeration), Value::String(text)) => enumeration
                .members
                .iter()
                .any(|member| member.name == *text),
            // Object, union, and map values are bound from node structure,
            // which the binding already checked field by field.
            (TypeKind::Object(_) | TypeKind::Union(_) | TypeKind::Map(_), Value::Map(_)) => true,
            _ => false,
        }
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

/// The dialect's string type for type names.
const TYPE_NAME: &str = "TypeName";

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
    /// A spelling of a mechanism the governing dialect defers.
    Deferred { mechanism: String },
}

/// Classify a value that failed to decode as `ty`. `deferrals` are the
/// governing dialect's when the value comes from a definition document,
/// and empty otherwise.
pub(crate) fn mismatch(
    types: &TypeCatalog,
    names: &Names,
    ty: &TypeExpr,
    value: &Value,
    deferrals: &[Deferral],
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
    let deferred = |mechanism: &str| Mismatch::Deferred {
        mechanism: mechanism.to_owned(),
    };
    match &decl.kind {
        TypeKind::Enum(_) => {
            let ty = names.resolve(*name);
            match deferred_by(deferrals, Spelling::Member(ty, text)) {
                Some(mechanism) => deferred(mechanism),
                None => Mismatch::Member { ty: ty.to_owned() },
            }
        }
        TypeKind::String(string)
            if string
                .validator
                .as_ref()
                .is_some_and(|v| v.name == TYPE_EXPRESSION_VALIDATOR) =>
        {
            match type_expr::parse(text, &mut Names::default()) {
                Err(ExprError::Generic(_)) if defers(deferrals, "generic-types") => {
                    deferred("generic-types")
                }
                Err(ExprError::Generic(head)) => Mismatch::TypeExpression {
                    reason: format!("{head} is instantiated with type arguments"),
                },
                Err(ExprError::Syntax(reason)) => Mismatch::TypeExpression { reason },
                Ok(_) => Mismatch::Value,
            }
        }
        // A type name spelled as a generic declaration or instantiation.
        TypeKind::String(_)
            if names.resolve(*name) == TYPE_NAME
                && generic_spelling(text)
                && defers(deferrals, "generic-types") =>
        {
            deferred("generic-types")
        }
        _ => Mismatch::Value,
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
enum Class {
    Integer,
    Fraction,
    /// `#inf`, `#-inf`, or `#nan`.
    Special,
}

/// The lexical class of a number spelling as the parser keeps it.
fn class(text: &str) -> Class {
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
        _ => "an unsupported value".to_owned(),
    }
}
