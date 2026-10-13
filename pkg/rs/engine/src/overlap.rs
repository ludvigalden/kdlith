//! Overlapping bindings: two fields of one node bound to the same
//! argument, property, or remaining arguments. The compiler and the
//! binder both report an overlap through this check, with one code.

use kdlith_model::Value;

use crate::syntax::parse_u64;

/// The dialect construct that declares a node; its bound `fields` map
/// holds the field bindings this check reads.
pub(crate) const NODE_CONSTRUCT: &str = "node";

/// Nested `child` and `children` sources are followed at most this deep.
const MAX_DEPTH: usize = 16;

/// The piece of a node's syntax one binding source consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Consumed<'a> {
    Argument(u64),
    /// The remaining positional arguments.
    Rest,
    Property(&'a str),
}

impl Consumed<'_> {
    fn spelled(self) -> String {
        match self {
            Consumed::Argument(index) => format!("argument {index}"),
            Consumed::Rest => "the remaining arguments".to_owned(),
            Consumed::Property(name) => format!("property {name}"),
        }
    }
}

/// A second field bound to syntax an earlier field already consumes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Overlap {
    /// The position of the overlapping use.
    pub(crate) at: usize,
    pub(crate) syntax: String,
    /// The field that consumed the syntax first.
    pub(crate) first: String,
    pub(crate) field: String,
}

impl Overlap {
    /// The values of the conflicting-binding message.
    pub(crate) fn values(&self) -> [&str; 3] {
        [&self.syntax, &self.first, &self.field]
    }
}

/// The overlaps among the sources of one scope, in use order. `claimed`
/// is syntax consumed before any field (an entries envelope's key);
/// `uses` pairs each source with the field it populates.
pub(crate) fn overlaps(
    claimed: &[(Consumed<'_>, &str)],
    uses: &[(&str, Consumed<'_>)],
) -> Vec<Overlap> {
    let mut owners: Vec<(Consumed<'_>, &str)> = claimed.to_vec();
    let mut found = Vec::new();
    for (at, &(field, consumed)) in uses.iter().enumerate() {
        match owners.iter().find(|(owned, _)| *owned == consumed) {
            Some((_, first)) => found.push(Overlap {
                at,
                syntax: consumed.spelled(),
                first: (*first).to_owned(),
                field: field.to_owned(),
            }),
            None => owners.push((consumed, field)),
        }
    }
    found
}

/// The overlaps in a node declaration bound as data of the dialect, given
/// its bound fields: the field bindings of its `fields` map, and the
/// sources nested under each child they select from.
pub(crate) fn in_declaration(fields: &[(&str, &Value)]) -> Vec<Overlap> {
    let Some(Value::Map(bindings)) = fields
        .iter()
        .find(|(name, _)| *name == "fields")
        .map(|(_, value)| *value)
    else {
        return Vec::new();
    };
    let mut scope = BoundScope::default();
    for binding in bindings {
        let Value::Map(parts) = &binding.value else {
            continue;
        };
        let field = binding.key.as_str();
        for part in parts {
            match (part.key.as_str(), &part.value) {
                ("arg", Value::Number(index)) => {
                    if let Some(index) = parse_u64(index) {
                        scope.uses.push((field, Consumed::Argument(index)));
                    }
                }
                ("property", Value::String(name)) => {
                    scope.uses.push((field, Consumed::Property(name)));
                }
                ("sources", Value::List(sources)) => scope.sources(field, sources),
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    scope.check(&mut found, 0);
    found
}

/// The nested sources fields select from one child with, by field.
type NestedGroup<'v> = Vec<(&'v str, &'v [Value])>;

/// The sources selecting from one scope of a bound node declaration: its
/// own syntax, and the children nested sources select from.
#[derive(Default)]
struct BoundScope<'v> {
    uses: Vec<(&'v str, Consumed<'v>)>,
    /// Child names in first-use order, each with its nested sources.
    nested: Vec<(&'v str, NestedGroup<'v>)>,
}

impl<'v> BoundScope<'v> {
    /// Add a field's list of source variants, each a single-key map.
    fn sources(&mut self, field: &'v str, sources: &'v [Value]) {
        for source in sources {
            let Value::Map(variant) = source else {
                continue;
            };
            let [variant] = variant.as_slice() else {
                continue;
            };
            let Value::Map(parts) = &variant.value else {
                continue;
            };
            let part = |key: &str| parts.iter().find(|p| p.key == key).map(|p| &p.value);
            match variant.key.as_str() {
                "argument" => {
                    if let Some(Value::Number(index)) = part("index")
                        && let Some(index) = parse_u64(index)
                    {
                        self.uses.push((field, Consumed::Argument(index)));
                    }
                }
                "args" => self.uses.push((field, Consumed::Rest)),
                "property" => {
                    if let Some(Value::String(name)) = part("name") {
                        self.uses.push((field, Consumed::Property(name)));
                    }
                }
                "child" | "children" => {
                    if let (Some(Value::String(name)), Some(Value::List(inner))) =
                        (part("name"), part("sources"))
                        && !inner.is_empty()
                    {
                        match self.nested.iter_mut().find(|(n, _)| n == name) {
                            Some((_, group)) => group.push((field, inner)),
                            None => self.nested.push((name, vec![(field, inner)])),
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn check(self, found: &mut Vec<Overlap>, depth: usize) {
        found.extend(overlaps(&[], &self.uses));
        if depth >= MAX_DEPTH {
            return;
        }
        for (_, group) in self.nested {
            let mut inner = BoundScope::default();
            for (field, sources) in group {
                inner.sources(field, sources);
            }
            inner.check(found, depth + 1);
        }
    }
}
