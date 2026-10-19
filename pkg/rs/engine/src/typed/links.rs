//! Linking typed references: a reading's declarations by identity, and
//! a `ref<T>` value resolved against them. Binding never resolves a
//! reference; linking is a separate step over a typed reading's items.

use std::fmt;
use std::marker::PhantomData;

use kdlith_model::{Identity, Location, Position, SourceId, Value};

use crate::typed::read::TypedItem;
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

/// Why a reference resolves to no single declaration of its type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unresolved {
    /// No declaration in the reference's domain carries its name.
    Missing,
    /// More than one declaration in the domain carries its name.
    Ambiguous { candidates: usize },
    /// The one declaration found is not read as the referenced type.
    NotOfType,
}

impl fmt::Display for Unresolved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("no declaration carries the referenced name"),
            Self::Ambiguous { candidates } => {
                write!(f, "{candidates} declarations carry the referenced name")
            }
            Self::NotOfType => f.write_str("the declaration is not of the referenced type"),
        }
    }
}

impl std::error::Error for Unresolved {}

/// The declarations of a typed reading, by identity, in reading order:
/// the linked graph a `ref<T>` value resolves against.
#[deprecated(
    note = "matches names only, without alias gating, qualifiers, or duplicate checks; \
            collect a linked reading and use `Reading::resolve` or `Reading::settled`"
)]
#[derive(Clone, Debug)]
pub struct Links<T> {
    declarations: Vec<(Identity, T)>,
}

// The deprecated type's own impls are its definition, not a use of it.
#[allow(deprecated)]
impl<T> Default for Links<T> {
    fn default() -> Self {
        Self {
            declarations: Vec::new(),
        }
    }
}

#[allow(deprecated)]
impl<T> Links<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one declaration under its identity.
    pub fn declare(&mut self, identity: Identity, value: T) {
        self.declarations.push((identity, value));
    }

    /// How many declarations are registered.
    pub fn len(&self) -> usize {
        self.declarations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }

    /// The one declaration `reference` names, read as its type: exactly
    /// one candidate in the reference's domain must carry its name.
    pub fn resolve<D>(&self, reference: &Ref<D>) -> Result<&D, Unresolved>
    where
        T: ItemVariant<D>,
    {
        let mut candidates = self.declarations.iter().filter(|(identity, _)| {
            identity.domain == reference.domain && identity.name.0 == reference.name
        });
        match (candidates.next(), candidates.next()) {
            (None, _) => Err(Unresolved::Missing),
            (Some((_, value)), None) => value.variant().ok_or(Unresolved::NotOfType),
            (Some(_), Some(_)) => Err(Unresolved::Ambiguous {
                candidates: 2 + candidates.count(),
            }),
        }
    }
}

#[allow(deprecated)]
impl<T: Clone> Links<T> {
    /// Register the item when it is a declaration; every other item
    /// passes untouched, so a reading's sink can observe its whole
    /// stream.
    pub fn observe(&mut self, item: &TypedItem<T>) {
        if let TypedItem::Declaration {
            identity, value, ..
        } = item
        {
            self.declare(identity.clone(), value.clone());
        }
    }
}
