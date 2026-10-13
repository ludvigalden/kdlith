//! Declaration macros: each logical type expands into its Rust type,
//! decoding, conformance shape, and serialization, spelled the same way
//! in hand-written and generated processors.

/// A patterned string type: a newtype over the bound text.
#[macro_export]
macro_rules! string_type {
    ($(#[$meta:meta])* $vis:vis struct $name:ident;) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        $vis struct $name(::std::string::String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl $crate::FromValue for $name {
            fn from_value(
                value: &$crate::__private::Value,
            ) -> ::core::result::Result<Self, $crate::Mismatch> {
                $crate::text_value(value).map(|text| Self(text.to_owned()))
            }
        }

        impl $crate::LogicalType for $name {
            fn expression() -> ::std::string::String {
                ::std::string::String::from(stringify!($name))
            }
        }

        impl $crate::Shaped for $name {
            const SHAPE: $crate::TypeShape = $crate::TypeShape::string(stringify!($name));
        }

        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }
    };
}

/// An enum type: one Rust variant per member, spelled as the member.
#[macro_export]
macro_rules! enum_type {
    (
        $(#[$meta:meta])* $vis:vis enum $name:ident {
            $($(#[$vmeta:meta])* $member:literal => $variant:ident),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        $vis enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// Every member with its spelling, in declaration order.
            pub const MEMBERS: &'static [(&'static str, Self)] = &[$(($member, Self::$variant)),+];

            /// The member as a definition spells it.
            pub fn member(self) -> &'static str {
                match self {
                    $(Self::$variant => $member),+
                }
            }
        }

        impl $crate::FromValue for $name {
            fn from_value(
                value: &$crate::__private::Value,
            ) -> ::core::result::Result<Self, $crate::Mismatch> {
                $crate::member_value(value, Self::MEMBERS)
            }
        }

        impl $crate::LogicalType for $name {
            fn expression() -> ::std::string::String {
                ::std::string::String::from(stringify!($name))
            }
        }

        impl $crate::Shaped for $name {
            const SHAPE: $crate::TypeShape =
                $crate::TypeShape::enumeration(stringify!($name), &[$($member),+]);
        }

        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                serializer.serialize_str(self.member())
            }
        }
    };
}

/// An object type: one Rust field per logical field, read by its name.
#[macro_export]
macro_rules! object_type {
    (@logical $name:ident { $($key:literal: $ty:ty),* }) => {
        impl $crate::FromValue for $name {
            fn from_value(
                value: &$crate::__private::Value,
            ) -> ::core::result::Result<Self, $crate::Mismatch> {
                $crate::object_value(value)
            }
        }

        impl $crate::LogicalType for $name {
            fn expression() -> ::std::string::String {
                ::std::string::String::from(stringify!($name))
            }
        }

        impl $crate::Shaped for $name {
            const SHAPE: $crate::TypeShape = $crate::TypeShape::object(
                stringify!($name),
                &[$($crate::FieldShape::new($key, <$ty as $crate::LogicalType>::expression)),*],
            );
        }
    };
    ($(#[$meta:meta])* $vis:vis struct $name:ident {}) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        #[non_exhaustive]
        $vis struct $name {}

        impl $crate::FromFields for $name {
            fn from_fields(
                _: &mut $crate::Fields<'_>,
            ) -> ::core::result::Result<Self, $crate::Mismatch> {
                Ok(Self {})
            }
        }

        $crate::object_type!(@logical $name {});

        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                use $crate::__private::serde::ser::SerializeMap as _;
                serializer.serialize_map(Some(0))?.end()
            }
        }
    };
    (
        $(#[$meta:meta])* $vis:vis struct $name:ident {
            $($(#[$fmeta:meta])* $key:literal => $field:ident: $ty:ty),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        #[non_exhaustive]
        $vis struct $name {
            $($(#[$fmeta])* pub $field: $ty),+
        }

        impl $crate::FromFields for $name {
            fn from_fields(
                fields: &mut $crate::Fields<'_>,
            ) -> ::core::result::Result<Self, $crate::Mismatch> {
                Ok(Self {
                    $($field: fields.take::<$ty>($key)?),+
                })
            }
        }

        $crate::object_type!(@logical $name { $($key: $ty),+ });

        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                use $crate::__private::serde::ser::SerializeMap as _;
                let mut map = serializer.serialize_map(None)?;
                $(
                    if !$crate::LogicalType::is_absent(&self.$field) {
                        map.serialize_entry($key, &self.$field)?;
                    }
                )+
                map.end()
            }
        }
    };
}

/// A union type: one Rust variant per variant, selected by the node name
/// that spelled it.
#[macro_export]
macro_rules! union_type {
    (
        $(#[$meta:meta])* $vis:vis enum $name:ident {
            $($(#[$vmeta:meta])* $node:literal => $variant:ident($ty:ty)),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        #[non_exhaustive]
        $vis enum $name {
            $($(#[$vmeta])* $variant($ty)),+
        }

        impl $crate::FromValue for $name {
            fn from_value(
                value: &$crate::__private::Value,
            ) -> ::core::result::Result<Self, $crate::Mismatch> {
                let (node, inner) = $crate::variant_value(value)?;
                match node {
                    $($node => <$ty as $crate::FromValue>::from_value(inner)
                        .map(Self::$variant)
                        .map_err(|mismatch| mismatch.within($node)),)+
                    other => Err($crate::Mismatch::new(::std::format!(
                        "{other} is not a variant of {}",
                        stringify!($name)
                    ))),
                }
            }
        }

        impl $crate::LogicalType for $name {
            fn expression() -> ::std::string::String {
                ::std::string::String::from(stringify!($name))
            }
        }

        impl $crate::Shaped for $name {
            const SHAPE: $crate::TypeShape = $crate::TypeShape::union(
                stringify!($name),
                &[$($crate::VariantShape::new($node, <$ty as $crate::LogicalType>::expression)),+],
            );
        }

        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                use $crate::__private::serde::ser::SerializeMap as _;
                let mut map = serializer.serialize_map(Some(1))?;
                match self {
                    $(Self::$variant(inner) => map.serialize_entry($node, inner)?,)+
                }
                map.end()
            }
        }
    };
}

/// What a processor yields for its items: one variant per item type,
/// named after it, each converting from that type.
#[macro_export]
macro_rules! item_type {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($(#[$vmeta:meta])* $variant:ident),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        #[non_exhaustive]
        $vis enum $name {
            $($(#[$vmeta])* $variant($variant)),+
        }

        $(impl ::core::convert::From<$variant> for $name {
            fn from(value: $variant) -> Self {
                Self::$variant(value)
            }
        })+

        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                match self {
                    $(Self::$variant(inner) => {
                        $crate::__private::serde::Serialize::serialize(inner, serializer)
                    })+
                }
            }
        }
    };
}
