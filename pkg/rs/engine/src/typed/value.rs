//! Decoding bound values into the Rust types a processor declares. The
//! binder has checked every value already, so a failure here means the
//! processor and the definition disagree.

use std::collections::BTreeMap;
use std::fmt;

use kdlith_model::{Entry, Field, Value};

use crate::decode::{Class, class, found};
use crate::syntax::{parse_i64, parse_u64};

/// `ValueExpr`: any single KDL scalar, kept as bound.
pub type ValueExpr = Value;

/// Where and why a bound value does not fit the Rust type declared for it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Mismatch {
    /// Field names, keys, and list positions, outermost first.
    path: Vec<String>,
    reason: String,
}

impl Mismatch {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            path: Vec::new(),
            reason: reason.into(),
        }
    }

    /// A value of the wrong shape.
    pub fn expected(expected: &str, value: &Value) -> Self {
        Self::new(format!("expected {expected}, found {}", found(value)))
    }

    /// The mismatch, placed inside the field, key, or position `segment`.
    #[must_use]
    pub fn within(mut self, segment: impl Into<String>) -> Self {
        self.path.insert(0, segment.into());
        self
    }

    /// The dotted path from the item to the mismatching value.
    pub fn path(&self) -> String {
        self.path.join(".")
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.reason)
        } else {
            write!(f, "{}: {}", self.path(), self.reason)
        }
    }
}

impl std::error::Error for Mismatch {}

/// A Rust type a bound value decodes into.
pub trait FromValue: Sized {
    fn from_value(value: &Value) -> Result<Self, Mismatch>;

    /// The value of an absent field; `None` makes absence a mismatch.
    fn absent() -> Option<Self> {
        None
    }
}

/// A Rust type standing for a logical type, with the type expression a
/// definition writes for a field holding it.
pub trait LogicalType: FromValue {
    fn expression() -> String;

    /// Whether the value stands for an absent field, so output omits it.
    fn is_absent(&self) -> bool {
        false
    }
}

/// A Rust type read from the named fields of one bound node.
pub trait FromFields: Sized {
    fn from_fields(fields: &mut Fields<'_>) -> Result<Self, Mismatch>;
}

/// The named fields of one bound node, each taken at most once.
#[derive(Debug)]
pub struct Fields<'a> {
    fields: Vec<(&'a str, &'a Value, bool)>,
}

impl<'a> Fields<'a> {
    /// The fields of a bound item.
    pub fn of_item(fields: &'a [Field]) -> Self {
        Self {
            fields: fields
                .iter()
                .map(|f| (f.name.as_str(), &f.value, false))
                .collect(),
        }
    }

    /// The fields of a node bound as part of its parent.
    pub fn of_map(entries: &'a [Entry]) -> Self {
        Self {
            fields: entries
                .iter()
                .map(|e| (e.key.as_str(), &e.value, false))
                .collect(),
        }
    }

    /// Decode field `name`; an absent field takes [`FromValue::absent`].
    pub fn take<T: FromValue>(&mut self, name: &str) -> Result<T, Mismatch> {
        let slot = self
            .fields
            .iter_mut()
            .find(|(field, _, taken)| *field == name && !*taken);
        match slot {
            Some((_, value, taken)) => {
                *taken = true;
                T::from_value(value).map_err(|m| m.within(name))
            }
            None => {
                T::absent().ok_or_else(|| Mismatch::new("required field is absent").within(name))
            }
        }
    }

    /// Fail on a bound field the processor did not take.
    pub fn finish(self) -> Result<(), Mismatch> {
        match self.fields.iter().find(|(_, _, taken)| !*taken) {
            Some((name, _, _)) => {
                Err(Mismatch::new("field is not declared by the processor").within(*name))
            }
            None => Ok(()),
        }
    }
}

/// Read every field of one node as `T`; a field `T` leaves untaken fails.
pub fn read_fields<T: FromFields>(mut fields: Fields<'_>) -> Result<T, Mismatch> {
    let read = T::from_fields(&mut fields)?;
    fields.finish()?;
    Ok(read)
}

/// Decode a node bound as part of its parent: a map of its fields.
pub fn object_value<T: FromFields>(value: &Value) -> Result<T, Mismatch> {
    match value {
        Value::Map(entries) => read_fields(Fields::of_map(entries)),
        other => Err(Mismatch::expected("a node with fields", other)),
    }
}

/// A union value: the node name that spelled the variant, and its value.
/// A scalar, bound from a value rather than a node, selects its variant
/// as binding did: the variant whose prefix its text starts with, with
/// no fallback when that variant's type rejects it, else the first
/// un-prefixed variant, in declaration order, whose type `accepts` it.
/// `variants` lists each discriminator with its prefix, in declaration
/// order.
pub fn variant_value<'v>(
    value: &'v Value,
    variants: &[(&'static str, Option<&'static str>)],
    accepts: impl Fn(&str, &Value) -> bool,
) -> Result<(&'v str, &'v Value), Mismatch> {
    match value {
        Value::Map(entries) if entries.len() == 1 => {
            Ok((entries[0].key.as_str(), &entries[0].value))
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) => {
            if let Value::String(text) = value
                && let Some((node, _)) = variants
                    .iter()
                    .find(|(_, prefix)| prefix.is_some_and(|prefix| text.starts_with(prefix)))
            {
                return Ok((*node, value));
            }
            variants
                .iter()
                .find(|(node, prefix)| prefix.is_none() && accepts(node, value))
                .map(|(node, _)| (*node, value))
                .ok_or_else(|| Mismatch::expected("one union variant", value))
        }
        other => Err(Mismatch::expected("one union variant", other)),
    }
}

/// An enum value: the member its text names.
pub fn member_value<T: Copy>(value: &Value, members: &[(&str, T)]) -> Result<T, Mismatch> {
    let text = text_value(value)?;
    members
        .iter()
        .find(|(name, _)| *name == text)
        .map(|(_, member)| *member)
        .ok_or_else(|| Mismatch::new(format!("{text:?} is not a member")))
}

/// A string value's text.
pub fn text_value(value: &Value) -> Result<&str, Mismatch> {
    match value {
        Value::String(text) => Ok(text),
        other => Err(Mismatch::expected("a string", other)),
    }
}

impl FromValue for String {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        text_value(value).map(str::to_owned)
    }
}

impl LogicalType for String {
    fn expression() -> String {
        "String".to_owned()
    }
}

impl FromValue for bool {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        match value {
            Value::Bool(flag) => Ok(*flag),
            other => Err(Mismatch::expected("a boolean", other)),
        }
    }
}

impl LogicalType for bool {
    fn expression() -> String {
        "Bool".to_owned()
    }
}

impl FromValue for u64 {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        match value {
            Value::Number(spelled) => {
                parse_u64(spelled).ok_or_else(|| Mismatch::expected("a U64", value))
            }
            other => Err(Mismatch::expected("a U64", other)),
        }
    }
}

impl LogicalType for u64 {
    fn expression() -> String {
        "U64".to_owned()
    }
}

/// `Integer`: an integer spelling in any radix. One the definition admits
/// but `i64` cannot hold is a mismatch, never a wrapped value.
impl FromValue for i64 {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        match value {
            Value::Number(spelled) => {
                parse_i64(spelled).ok_or_else(|| Mismatch::expected("an Integer", value))
            }
            other => Err(Mismatch::expected("an Integer", other)),
        }
    }
}

impl LogicalType for i64 {
    fn expression() -> String {
        "Integer".to_owned()
    }
}

/// `Decimal`: a finite number, kept exactly as the document spelled it,
/// so no binary rounding touches it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decimal(String);

impl Decimal {
    /// The number as the document spelled it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromValue for Decimal {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        match value {
            Value::Number(spelled) if class(spelled) != Class::Special => Ok(Self(spelled.clone())),
            other => Err(Mismatch::expected("a Decimal", other)),
        }
    }
}

impl LogicalType for Decimal {
    fn expression() -> String {
        "Decimal".to_owned()
    }
}

/// A decimal serializes as its spelling.
impl serde::Serialize for Decimal {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// `nullable<T>`: a value or an explicit null. Absence stays distinct:
/// `optional<nullable<T>>` reads absence as `None`, null as
/// `Some(Nullable::Null)`.
// Exhaustive: a nullable field holds only these two states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Nullable<T> {
    Null,
    Value(T),
}

impl<T: FromValue> FromValue for Nullable<T> {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        match value {
            Value::Null => Ok(Self::Null),
            other => T::from_value(other).map(Self::Value),
        }
    }
}

impl<T: LogicalType> LogicalType for Nullable<T> {
    fn expression() -> String {
        format!("nullable<{}>", T::expression())
    }
}

/// A null serializes as null; a value as itself.
impl<T: serde::Serialize> serde::Serialize for Nullable<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => serializer.serialize_none(),
            Self::Value(value) => serializer.serialize_some(value),
        }
    }
}

/// `ValueExpr`: any single KDL scalar, kept as bound.
impl FromValue for Value {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        match value {
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(value.clone()),
            other => Err(Mismatch::expected("a single scalar", other)),
        }
    }
}

impl LogicalType for Value {
    fn expression() -> String {
        "ValueExpr".to_owned()
    }
}

/// `optional<T>`: an absent field is `None`.
impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        T::from_value(value).map(Some)
    }

    fn absent() -> Option<Self> {
        Some(None)
    }
}

impl<T: LogicalType> LogicalType for Option<T> {
    fn expression() -> String {
        format!("optional<{}>", T::expression())
    }

    fn is_absent(&self) -> bool {
        self.is_none()
    }
}

/// `list<T>`, in document order.
impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        match value {
            Value::List(items) => items
                .iter()
                .enumerate()
                .map(|(at, item)| T::from_value(item).map_err(|m| m.within(at.to_string())))
                .collect(),
            other => Err(Mismatch::expected("a list", other)),
        }
    }
}

impl<T: LogicalType> LogicalType for Vec<T> {
    fn expression() -> String {
        format!("list<{}>", T::expression())
    }
}

/// `ordered-map<K, V>`: unique keys, in declaration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderedMap<K, V> {
    entries: Vec<(K, V)>,
}

impl<K, V> OrderedMap<K, V> {
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().map(|(key, value)| (key, value))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &K) -> Option<&V>
    where
        K: PartialEq,
    {
        self.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }
}

impl<K: FromValue, V: FromValue> FromValue for OrderedMap<K, V> {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        let Value::Map(entries) = value else {
            return Err(Mismatch::expected("a keyed collection", value));
        };
        let entries = entries
            .iter()
            .map(|entry| keyed(entry).map_err(|m| m.within(entry.key.as_str())))
            .collect::<Result<_, _>>()?;
        Ok(Self { entries })
    }
}

/// `map<K, V>`: unique keys, ordered by key. A reference key keeps the
/// reference it spells. Two entries whose keys decode equal fail rather
/// than one silently replacing the other.
impl<K: FromValue + Ord, V: FromValue> FromValue for BTreeMap<K, V> {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        let Value::Map(entries) = value else {
            return Err(Mismatch::expected("a keyed collection", value));
        };
        let mut map = BTreeMap::new();
        for entry in entries {
            let (key, value) = keyed(entry).map_err(|m| m.within(entry.key.as_str()))?;
            if map.insert(key, value).is_some() {
                let repeated = Mismatch::new("the key repeats an earlier key");
                return Err(repeated.within(entry.key.as_str()));
            }
        }
        Ok(map)
    }
}

impl<K: LogicalType + Ord, V: LogicalType> LogicalType for BTreeMap<K, V> {
    fn expression() -> String {
        format!("map<{}, {}>", K::expression(), V::expression())
    }
}

fn keyed<K: FromValue, V: FromValue>(entry: &Entry) -> Result<(K, V), Mismatch> {
    let key = K::from_value(&entry.key_value())?;
    Ok((key, V::from_value(&entry.value)?))
}

impl<K: LogicalType, V: LogicalType> LogicalType for OrderedMap<K, V> {
    fn expression() -> String {
        format!("ordered-map<{}, {}>", K::expression(), V::expression())
    }
}

/// An ordered map serializes as an object, in declaration order.
impl<K: serde::Serialize, V: serde::Serialize> serde::Serialize for OrderedMap<K, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (key, value) in &self.entries {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}
