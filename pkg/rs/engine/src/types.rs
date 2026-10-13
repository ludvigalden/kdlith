//! The TypeCatalog: built-in scalars, declared logical types, and the
//! compiled type expressions that name them.

use std::collections::BTreeMap;

use kdlith_model::Value;

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
/// each one is declared.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TypeExpr {
    Named(Sym),
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
/// absent value; `fallback` does.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FieldDef {
    pub name: Sym,
    pub ty: TypeExpr,
    pub description: Option<String>,
    pub documented_default: Option<Value>,
    pub fallback: Option<Value>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Variant {
    pub discriminator: Sym,
    pub ty: TypeExpr,
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
