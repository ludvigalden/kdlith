//! Typed references: a `ref<T>` value as bound, and the item variant a
//! resolved reference is read as. Binding never resolves a reference; a
//! linked reading does.

use std::fmt;
use std::marker::PhantomData;

use kdlith_model::{Location, Position, SourceId, Value};

use crate::typed::value::{FromValue, LogicalType, Mismatch};

/// A `ref<T>` value: a symbolic reference, as bound, to a declaration
/// whose identity domain is the logical type `T`. Reading it never
/// resolves it; a linked [`Reading`](crate::Reading) does.
pub struct Ref<T> {
    domain: String,
    name: Vec<String>,
    written: String,
    qualifiers: Vec<(String, String)>,
    site: Location,
    target: PhantomData<fn() -> T>,
}

impl<T> Ref<T> {
    /// The identity domain the reference looks up in.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// The referenced name's segments, outermost qualifier first.
    pub fn name(&self) -> &[String] {
        &self.name
    }

    /// The reference as the document spelled it.
    pub fn written(&self) -> &str {
        &self.written
    }

    /// The qualifiers the spelling carried, by name, in capture order.
    pub fn qualifiers(&self) -> &[(String, String)] {
        &self.qualifiers
    }

    /// Where the reference was written.
    pub fn site(&self) -> Location {
        self.site
    }
}

impl<T> Clone for Ref<T> {
    fn clone(&self) -> Self {
        Self {
            domain: self.domain.clone(),
            name: self.name.clone(),
            written: self.written.clone(),
            qualifiers: self.qualifiers.clone(),
            site: self.site,
            target: PhantomData,
        }
    }
}

impl<T> fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ref")
            .field("domain", &self.domain)
            .field("name", &self.name)
            .field("written", &self.written)
            .field("qualifiers", &self.qualifiers)
            .field("site", &self.site)
            .finish()
    }
}

impl<T> PartialEq for Ref<T> {
    fn eq(&self, other: &Self) -> bool {
        self.domain == other.domain
            && self.name == other.name
            && self.written == other.written
            && self.qualifiers == other.qualifiers
            && self.site == other.site
    }
}

impl<T> Eq for Ref<T> {}

/// References order component-wise — domain, name segments, spelling,
/// qualifiers, site — so a map keyed by references iterates the same
/// way on every reading.
impl<T> Ord for Ref<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.domain
            .cmp(&other.domain)
            .then_with(|| self.name.cmp(&other.name))
            .then_with(|| self.written.cmp(&other.written))
            .then_with(|| self.qualifiers.cmp(&other.qualifiers))
            .then_with(|| site_key(self.site).cmp(&site_key(other.site)))
    }
}

impl<T> PartialOrd for Ref<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// A site's ordering key, agreeing with its equality.
fn site_key(site: Location) -> (SourceId, Position, Position) {
    (site.source, site.span.start, site.span.end)
}

/// A bound reference whose domain is `T`'s logical type.
impl<T: LogicalType> FromValue for Ref<T> {
    fn from_value(value: &Value) -> Result<Self, Mismatch> {
        reference_value(value, &T::expression())
    }
}

/// A bound reference looking up in `domain`: a declared reference type
/// decodes into its lookup domain, which need not be its target's name.
/// Anything else, including a spelling no codec decoded, is a mismatch.
pub fn reference_value<T>(value: &Value, domain: &str) -> Result<Ref<T>, Mismatch> {
    match value {
        Value::Reference(reference) if reference.domain == domain => Ok(Ref {
            domain: reference.domain.clone(),
            name: reference.name.0.clone(),
            written: reference.written.clone(),
            qualifiers: reference.qualifiers.clone(),
            site: reference.site,
            target: PhantomData,
        }),
        other => Err(Mismatch::expected(
            &format!("a reference to {domain}"),
            other,
        )),
    }
}

impl<T: LogicalType> LogicalType for Ref<T> {
    fn expression() -> String {
        format!("ref<{}>", T::expression())
    }
}

/// A reference serializes as the document spelled it.
impl<T> serde::Serialize for Ref<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.written)
    }
}

/// A processor's item type, projected onto one of its variants: what a
/// resolved reference is read as.
pub trait ItemVariant<D> {
    /// The variant's value, when the item is of type `D`.
    fn variant(&self) -> Option<&D>;
}
