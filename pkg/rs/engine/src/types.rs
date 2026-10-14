//! The TypeCatalog: built-in scalars, declared logical types, and the
//! compiled type expressions that name them.

use std::collections::BTreeMap;

use kdlith_model::{Entry, Value};

use crate::diag::Site;
use crate::names::{Names, Sym};

/// A type in its catalog; built-ins take the first ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(u32);

impl TypeId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The built-in scalars a type expression names without a declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Builtin {
    String,
    Bool,
    Integer,
    Decimal,
    U64,
    /// Any single KDL scalar, checked against the type of its field.
    ValueExpr,
}

impl Builtin {
    pub const ALL: [Builtin; 6] = [
        Builtin::String,
        Builtin::Bool,
        Builtin::Integer,
        Builtin::Decimal,
        Builtin::U64,
        Builtin::ValueExpr,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Builtin::String => "String",
            Builtin::Bool => "Bool",
            Builtin::Integer => "Integer",
            Builtin::Decimal => "Decimal",
            Builtin::U64 => "U64",
            Builtin::ValueExpr => "ValueExpr",
        }
    }
}

/// A compiled type expression. Names stay symbolic; linking checks that
/// each one is declared. An instantiation is the pre-monomorphization
/// form: the monomorphization pass replaces it with the substituted
/// declaration before linking, so none survives a complete compile.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TypeExpr {
    Named(Sym),
    /// A generic instantiation `T<A, B>`; arguments are themselves type
    /// expressions.
    Instantiation(Sym, Vec<TypeExpr>),
    /// The field may be absent.
    Optional(Box<TypeExpr>),
    /// The field may explicitly hold null.
    Nullable(Box<TypeExpr>),
    List(Box<TypeExpr>),
    Map(Box<TypeExpr>, Box<TypeExpr>),
    /// Unique keys, declaration order retained.
    OrderedMap(Box<TypeExpr>, Box<TypeExpr>),
    Union(Vec<TypeExpr>),
    /// A symbolic reference to a declared entity of the target type.
    Ref(Box<TypeExpr>),
}

/// One declared or built-in type.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TypeDecl {
    pub name: Sym,
    /// The type parameters the declaration names, in order; empty for a
    /// non-generic declaration. A codec may declare them too.
    pub parameters: Vec<Sym>,
    pub kind: TypeKind,
    /// Where it was declared; `None` for a built-in.
    pub site: Option<Site>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TypeKind {
    Builtin(Builtin),
    String(StringType),
    Enum(EnumType),
    Object(ObjectType),
    Union(UnionType),
    Map(MapType),
    /// A pattern codec: a string decoder whose output is a declared type.
    Codec(CodecDecl),
}

/// A pattern codec: one whole-string regular expression, the captures it
/// types, and the output type a decoded value carries.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CodecDecl {
    /// The logical type a decoded value carries.
    pub output: TypeExpr,
    /// The whole-string match; no automatic trimming.
    pub pattern: String,
    /// One typed named capture group, in declaration order.
    pub captures: Vec<CaptureDef>,
    /// The reference namespace a decoded identity belongs to; metadata
    /// only, consumed when symbol-catalogs lands.
    pub namespace: Option<Sym>,
}

/// One typed capture group of a codec.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CaptureDef {
    pub name: String,
    pub ty: TypeExpr,
}

/// A patterned string: a whole-string pattern, a built-in format, a
/// registered validator, and a minimum length, each optional.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct StringType {
    pub format: Option<FormatRule>,
    pub pattern: Option<String>,
    pub validator: Option<ValidatorUse>,
    pub min_length: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FormatRule {
    pub format: StringFormat,
    /// The digit count of `zero-padded`; absent for every other format.
    pub width: Option<u64>,
}

/// The built-in string formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StringFormat {
    /// Lowercase ASCII words of letters and digits joined by single `-`.
    KebabCase,
    /// A lowercase ASCII letter, then ASCII letters and digits.
    CamelCase,
    /// Lowercase ASCII words of letters and digits joined by single `_`.
    LowerSnake,
    /// Uppercase ASCII words of letters and digits joined by single `_`.
    UpperSnake,
    /// Hex digits grouped 8-4-4-4-12 by `-`, either case.
    Uuid,
    /// Exactly `width` ASCII digits, leading zeros kept.
    ZeroPadded,
}

impl StringFormat {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "kebab-case" => StringFormat::KebabCase,
            "camel-case" => StringFormat::CamelCase,
            "lower-snake" => StringFormat::LowerSnake,
            "upper-snake" => StringFormat::UpperSnake,
            "uuid" => StringFormat::Uuid,
            "zero-padded" => StringFormat::ZeroPadded,
            _ => return None,
        })
    }

    /// Whether `text` follows the format; `width` applies to `zero-padded`.
    pub fn matches(self, text: &str, width: Option<u64>) -> bool {
        match self {
            StringFormat::KebabCase => words(text, '-', |b| b.is_ascii_lowercase()),
            StringFormat::LowerSnake => words(text, '_', |b| b.is_ascii_lowercase()),
            StringFormat::UpperSnake => words(text, '_', |b| b.is_ascii_uppercase()),
            StringFormat::CamelCase => {
                let mut bytes = text.bytes();
                bytes.next().is_some_and(|b| b.is_ascii_lowercase())
                    && bytes.all(|b| b.is_ascii_alphanumeric())
            }
            StringFormat::Uuid => {
                let groups: Vec<&str> = text.split('-').collect();
                groups.len() == 5
                    && groups.iter().zip([8, 4, 4, 4, 12]).all(|(group, len)| {
                        group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit())
                    })
            }
            StringFormat::ZeroPadded => {
                width.is_some_and(|width| text.len() as u64 == width)
                    && text.bytes().all(|b| b.is_ascii_digit())
            }
        }
    }
}

/// Words of letters (in the given case) and digits, starting with a
/// letter, joined by single separators.
fn words(text: &str, separator: char, letter: impl Fn(u8) -> bool) -> bool {
    text.bytes().next().is_some_and(&letter)
        && text
            .split(separator)
            .all(|word| !word.is_empty() && word.bytes().all(|b| letter(b) || b.is_ascii_digit()))
}

/// A registered validator a string type names, with its version.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ValidatorUse {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EnumType {
    pub members: Vec<EnumMember>,
}

/// An enum member; its description stays as metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EnumMember {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ObjectType {
    pub fields: Vec<FieldDef>,
    pub constraints: Vec<Constraint>,
}

/// A logical field. `documented_default` is metadata and never fills an
/// absent value; `fallback` does, when it is a literal.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FieldDef {
    pub name: Sym,
    pub ty: TypeExpr,
    pub description: Option<String>,
    pub documented_default: Option<Value>,
    /// The field's fallback: a literal value the engine applies to an
    /// absent field, or a computed default it never computes — a
    /// descriptor carrying its dependencies for the owning domain
    /// process.
    pub fallback: Option<DefaultSpec>,
    /// Whether the field holds a resolved sensitive value or a locator
    /// for one; metadata that an output policy enforces.
    pub sensitivity: Option<Sensitivity>,
}

/// A declared default: a literal value, or a computed descriptor. A
/// computed default is never computed by the processor — computation
/// belongs to the owning domain process — so only the literal variant
/// ever supplies an absent field.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DefaultSpec {
    /// A literal value, decoded against the field's type at compile time.
    Literal(Value),
    /// A computed default: its dependencies as typed references, preserved
    /// as descriptor data for the owning domain process.
    Computed { dependencies: Vec<String> },
}

impl DefaultSpec {
    /// The literal a default supplies, when it is one. A computed default
    /// supplies nothing; the field stays absent for the domain process to
    /// fill.
    pub fn literal(&self) -> Option<&Value> {
        match self {
            DefaultSpec::Literal(value) => Some(value),
            DefaultSpec::Computed { .. } => None,
        }
    }

    /// The default as the bound value a reader holds: the part's fields
    /// wrapped under the union member its case dispatch selected.
    pub fn to_bound(&self) -> Value {
        match self {
            DefaultSpec::Literal(value) => Value::Map(vec![Entry::new(
                "literal".to_owned(),
                Value::Map(vec![Entry::new("value".to_owned(), value.clone())]),
            )]),
            DefaultSpec::Computed { dependencies } => Value::Map(vec![Entry::new(
                "computed".to_owned(),
                Value::Map(vec![Entry::new(
                    "dependencies".to_owned(),
                    Value::List(
                        dependencies
                            .iter()
                            .map(|dep| Value::String(dep.clone()))
                            .collect(),
                    ),
                )]),
            )]),
        }
    }

    /// The default a bound value spells: a scalar is a literal (the
    /// spelling every pre-computed default used), a single-key map names
    /// the union member a case dispatch selected. `None` spells no
    /// recognizable default and the caller rejects it.
    pub fn from_bound(value: &Value) -> Option<Self> {
        match value {
            Value::Map(entries) => match entries.as_slice() {
                [entry] if entry.key == "literal" => {
                    let Value::Map(inner) = &entry.value else {
                        return None;
                    };
                    let value = inner.iter().find(|field| field.key == "value")?;
                    Some(DefaultSpec::Literal(value.value.clone()))
                }
                [entry] if entry.key == "computed" => {
                    let Value::Map(inner) = &entry.value else {
                        return None;
                    };
                    let dependencies = inner.iter().find(|field| field.key == "dependencies")?;
                    let Value::List(items) = &dependencies.value else {
                        return None;
                    };
                    let mut spelled = Vec::new();
                    for item in items {
                        let Value::String(dep) = item else {
                            return None;
                        };
                        spelled.push(dep.clone());
                    }
                    Some(DefaultSpec::Computed {
                        dependencies: spelled,
                    })
                }
                _ => None,
            },
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
                Some(DefaultSpec::Literal(value.clone()))
            }
            _ => None,
        }
    }
}

/// What a sensitive field holds. A type modeling a secret and its
/// locator keeps two separate fields, each tagged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Sensitivity {
    /// A resolved sensitive value; emitted only when the output policy
    /// explicitly says emit.
    Secret,
    /// An identifier that locates a sensitive value.
    Reference,
}

impl Sensitivity {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "secret" => Sensitivity::Secret,
            "reference" => Sensitivity::Reference,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Sensitivity::Secret => "secret",
            Sensitivity::Reference => "reference",
        }
    }
}

/// The explicit output policy every serialization and diagnostic surface
/// applies to sensitive fields. The fail-closed default redacts a secret;
/// a reference defaults to emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OutputPolicy {
    Emit,
    Omit,
    Redact,
}

impl OutputPolicy {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "emit" => OutputPolicy::Emit,
            "omit" => OutputPolicy::Omit,
            "redact" => OutputPolicy::Redact,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            OutputPolicy::Emit => "emit",
            OutputPolicy::Omit => "omit",
            OutputPolicy::Redact => "redact",
        }
    }

    /// The policy a sensitivity carries when no explicit policy is set:
    /// redact for a secret (fail closed), emit for a reference.
    pub fn default_for(sensitivity: Sensitivity) -> Self {
        match sensitivity {
            Sensitivity::Secret => OutputPolicy::Redact,
            Sensitivity::Reference => OutputPolicy::Emit,
        }
    }

    /// The value a field's content takes under this policy: the value
    /// itself under emit, a redaction marker under redact, and nothing
    /// under omit.
    pub fn apply(self, value: Value) -> Option<Value> {
        match self {
            OutputPolicy::Emit => Some(value),
            OutputPolicy::Redact => Some(Value::String("[redacted]".to_owned())),
            OutputPolicy::Omit => None,
        }
    }
}

/// A declarative object constraint an author states explicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Constraint {
    AllOrNone(Vec<Sym>),
    Requires { field: Sym, required: Sym },
}

/// A union discriminated by the node name that spells each variant.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnionType {
    pub variants: Vec<Variant>,
}

/// A union variant. The node name that spells a child-bound variant is
/// its discriminator; a value-bound variant is selected by the prefix
/// its text starts with.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Variant {
    pub discriminator: Sym,
    pub ty: TypeExpr,
    /// Selects the variant for a value-bound union when the value's text
    /// starts with it.
    pub prefix: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct MapType {
    pub key: TypeExpr,
    pub value: TypeExpr,
}

/// Every type of one definition: the built-ins, then declarations in
/// contribution order, looked up by interned name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeCatalog {
    types: Vec<TypeDecl>,
    by_name: BTreeMap<Sym, TypeId>,
}

impl TypeCatalog {
    pub(crate) fn with_builtins(names: &mut Names) -> Self {
        let mut catalog = Self {
            types: Vec::new(),
            by_name: BTreeMap::new(),
        };
        for builtin in Builtin::ALL {
            // Built-in names are distinct, so registering one never collides.
            let _ = catalog.push(TypeDecl {
                name: names.intern(builtin.name()),
                parameters: Vec::new(),
                kind: TypeKind::Builtin(builtin),
                site: None,
            });
        }
        catalog
    }

    /// Add a declaration; on a name already taken, returns the earlier
    /// declaration's id and adds nothing.
    pub(crate) fn push(&mut self, decl: TypeDecl) -> Result<TypeId, TypeId> {
        if let Some(earlier) = self.by_name.get(&decl.name) {
            return Err(*earlier);
        }
        let id = TypeId(u32::try_from(self.types.len()).unwrap_or(u32::MAX));
        self.by_name.insert(decl.name, id);
        self.types.push(decl);
        Ok(id)
    }

    pub fn lookup(&self, name: Sym) -> Option<TypeId> {
        self.by_name.get(&name).copied()
    }

    pub fn get(&self, id: TypeId) -> Option<&TypeDecl> {
        self.types.get(id.index())
    }

    /// Take one declaration out, leaving a placeholder that keeps the
    /// name and site; compile passes rewrite the taken declaration while
    /// freely reading and extending the rest of the catalog. Put it back
    /// with [`put_decl`][Self::put_decl].
    pub(crate) fn take_decl(&mut self, id: TypeId) -> Option<TypeDecl> {
        let decl = self.types.get_mut(id.index())?;
        let placeholder = TypeDecl {
            name: decl.name,
            parameters: Vec::new(),
            kind: TypeKind::Builtin(Builtin::ValueExpr),
            site: decl.site,
        };
        Some(std::mem::replace(decl, placeholder))
    }

    /// Return a declaration taken by [`take_decl`][Self::take_decl].
    pub(crate) fn put_decl(&mut self, id: TypeId, decl: TypeDecl) {
        if let Some(slot) = self.types.get_mut(id.index()) {
            *slot = decl;
        }
    }

    /// Every type in id order.
    pub fn iter(&self) -> impl Iterator<Item = (TypeId, &TypeDecl)> {
        self.types
            .iter()
            .enumerate()
            .map(|(at, decl)| (TypeId(u32::try_from(at).unwrap_or(u32::MAX)), decl))
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}
