//! Declaration macros: each logical type expands into its Rust type,
//! decoding, conformance shape, and serialization, spelled the same way
//! in hand-written and generated processors.

/// A patterned string type: a newtype over the bound text.
#[macro_export]
macro_rules! string_type {
    ($(#[$meta:meta])* $vis:vis struct $name:ident;) => {
        $crate::string_type! { as stringify!($name); $(#[$meta])* $vis struct $name; }
    };
    // Each declaration macro's `as "Pair<String, Mode>";` arm spells a
    // generic instance its Rust name (`PairStringMode`) cannot.
    (as $spelled:expr; $(#[$meta:meta])* $vis:vis struct $name:ident;) => {
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
                ::std::string::String::from($spelled)
            }
        }

        impl $crate::Shaped for $name {
            const SHAPE: $crate::TypeShape = $crate::TypeShape::string($spelled);
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
        $crate::enum_type! {
            as stringify!($name);
            $(#[$meta])* $vis enum $name { $($(#[$vmeta])* $member => $variant),+ }
        }
    };
    (
        as $spelled:expr;
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
                ::std::string::String::from($spelled)
            }
        }

        impl $crate::Shaped for $name {
            const SHAPE: $crate::TypeShape =
                $crate::TypeShape::enumeration($spelled, &[$($member),+]);
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
    (@logical $spelled:expr, $name:ident { $($key:literal: $ty:ty),* }) => {
        impl $crate::FromValue for $name {
            fn from_value(
                value: &$crate::__private::Value,
            ) -> ::core::result::Result<Self, $crate::Mismatch> {
                $crate::object_value(value)
            }
        }

        impl $crate::LogicalType for $name {
            fn expression() -> ::std::string::String {
                ::std::string::String::from($spelled)
            }
        }

        impl $crate::Shaped for $name {
            const SHAPE: $crate::TypeShape = $crate::TypeShape::object(
                $spelled,
                &[$($crate::FieldShape::new($key, <$ty as $crate::LogicalType>::expression)),*],
            );
        }
    };
    ($(#[$meta:meta])* $vis:vis struct $name:ident {}) => {
        $crate::object_type! { as stringify!($name); $(#[$meta])* $vis struct $name {} }
    };
    (
        $(#[$meta:meta])* $vis:vis struct $name:ident {
            $($(#[$fmeta:meta])* $key:literal => $field:ident: $ty:ty),+ $(,)?
        }
    ) => {
        $crate::object_type! {
            as stringify!($name);
            $(#[$meta])* $vis struct $name { $($(#[$fmeta])* $key => $field: $ty),+ }
        }
    };
    (as $spelled:expr; $(#[$meta:meta])* $vis:vis struct $name:ident {}) => {
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

        $crate::object_type!(@logical $spelled, $name {});

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
        as $spelled:expr;
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

        $crate::object_type!(@logical $spelled, $name { $($key: $ty),+ });

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
        $crate::union_type! {
            as stringify!($name);
            $(#[$meta])* $vis enum $name { $($(#[$vmeta])* $node => $variant($ty)),+ }
        }
    };
    (
        as $spelled:expr;
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
                        $spelled
                    ))),
                }
            }
        }

        impl $crate::LogicalType for $name {
            fn expression() -> ::std::string::String {
                ::std::string::String::from($spelled)
            }
        }

        impl $crate::Shaped for $name {
            const SHAPE: $crate::TypeShape = $crate::TypeShape::union(
                $spelled,
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

/// A named reference type: a newtype over the `ref<Target>` carrier,
/// spelled as the declaration names it. Its references decode into the
/// lookup domain, so reading compares against that domain rather than
/// the target's name; a spelling no codec decoded never reads as one.
#[macro_export]
macro_rules! reference_type {
    (
        $(#[$meta:meta])* $vis:vis struct $name:ident => $target:ident {
            lookup: $lookup:literal,
            qualifiers: [$($qualifier:literal),* $(,)?] $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        $vis struct $name(pub $crate::Ref<$target>);

        impl $name {
            /// The identity domain its references look up in.
            pub const LOOKUP: &'static str = $lookup;

            /// Its qualifier names, in declaration order.
            pub const QUALIFIERS: &'static [&'static str] = &[$($qualifier),*];
        }

        impl $crate::FromValue for $name {
            fn from_value(
                value: &$crate::__private::Value,
            ) -> ::core::result::Result<Self, $crate::Mismatch> {
                $crate::reference_value(value, Self::LOOKUP).map(Self)
            }
        }

        impl $crate::LogicalType for $name {
            fn expression() -> ::std::string::String {
                ::std::string::String::from(stringify!($name))
            }
        }

        impl $crate::Shaped for $name {
            const SHAPE: $crate::TypeShape = $crate::TypeShape::reference(
                stringify!($name),
                stringify!($target),
                $lookup,
                &[$($qualifier),*],
            );
        }

        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                $crate::__private::serde::Serialize::serialize(&self.0, serializer)
            }
        }
    };
}

/// A codec: a newtype over its output type, spelled as the codec. A
/// field typed by the codec binds the decoded output value.
#[macro_export]
macro_rules! codec_type {
    ($(#[$meta:meta])* $vis:vis struct $name:ident($output:ty);) => {
        $crate::codec_type! { as stringify!($name); $(#[$meta])* $vis struct $name($output); }
    };
    (as $spelled:expr; $(#[$meta:meta])* $vis:vis struct $name:ident($output:ty);) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        $vis struct $name(pub $output);

        impl $crate::FromValue for $name {
            fn from_value(
                value: &$crate::__private::Value,
            ) -> ::core::result::Result<Self, $crate::Mismatch> {
                <$output as $crate::FromValue>::from_value(value).map(Self)
            }
        }

        impl $crate::LogicalType for $name {
            fn expression() -> ::std::string::String {
                ::std::string::String::from($spelled)
            }
        }

        impl $crate::Shaped for $name {
            const SHAPE: $crate::TypeShape = $crate::TypeShape::codec(
                $spelled,
                <$output as $crate::LogicalType>::expression,
            );
        }

        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                $crate::__private::serde::Serialize::serialize(&self.0, serializer)
            }
        }
    };
}

/// A declared map type: a newtype over the `map<Key, Value>` carrier,
/// spelled as the declaration names it.
#[macro_export]
macro_rules! map_type {
    ($(#[$meta:meta])* $vis:vis struct $name:ident($key:ty => $value:ty);) => {
        $crate::map_type! { as stringify!($name); $(#[$meta])* $vis struct $name($key => $value); }
    };
    (as $spelled:expr; $(#[$meta:meta])* $vis:vis struct $name:ident($key:ty => $value:ty);) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        $vis struct $name(pub ::std::collections::BTreeMap<$key, $value>);

        impl $crate::FromValue for $name {
            fn from_value(
                value: &$crate::__private::Value,
            ) -> ::core::result::Result<Self, $crate::Mismatch> {
                <::std::collections::BTreeMap<$key, $value> as $crate::FromValue>::from_value(
                    value,
                )
                .map(Self)
            }
        }

        impl $crate::LogicalType for $name {
            fn expression() -> ::std::string::String {
                ::std::string::String::from($spelled)
            }
        }

        impl $crate::Shaped for $name {
            const SHAPE: $crate::TypeShape = $crate::TypeShape::map(
                $spelled,
                <$key as $crate::LogicalType>::expression,
                <$value as $crate::LogicalType>::expression,
            );
        }

        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                $crate::__private::serde::Serialize::serialize(&self.0, serializer)
            }
        }
    };
}

/// An inline `union<…>` a field's type expression writes: one Rust
/// variant per member, in member order. A value reads as the first
/// member it fits, as binding checks it, and serializes as that member.
#[macro_export]
macro_rules! inline_union_type {
    (
        $(#[$meta:meta])* $vis:vis enum $name:ident {
            $($(#[$vmeta:meta])* $variant:ident($ty:ty)),+ $(,)?
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
                $(if let ::core::result::Result::Ok(read) =
                    <$ty as $crate::FromValue>::from_value(value)
                {
                    return ::core::result::Result::Ok(Self::$variant(read));
                })+
                ::core::result::Result::Err($crate::Mismatch::expected(
                    &<Self as $crate::LogicalType>::expression(),
                    value,
                ))
            }
        }

        impl $crate::LogicalType for $name {
            fn expression() -> ::std::string::String {
                let members: &[::std::string::String] =
                    &[$(<$ty as $crate::LogicalType>::expression()),+];
                ::std::format!("union<{}>", members.join(", "))
            }
        }

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

/// What a processor yields for its items: one variant per item type.
/// The grouped form `Type => A | B` lets one type back several
/// variants; only a one-variant type converts into the item.
#[macro_export]
macro_rules! item_type {
    (@from $name:ident $ty:ident $only:ident) => {
        impl ::core::convert::From<$ty> for $name {
            fn from(value: $ty) -> Self {
                Self::$only(value)
            }
        }
    };
    (@from $name:ident $ty:ident $($variant:ident)+) => {};
    (
        $(#[$meta:meta])* $vis:vis enum $name:ident {
            $($ty:ident => $($variant:ident)|+),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        #[non_exhaustive]
        $vis enum $name {
            $($($variant($ty)),+),+
        }

        $($crate::item_type!(@from $name $ty $($variant)+);)+

        $(impl $crate::ItemVariant<$ty> for $name {
            // A type backing every variant leaves the fallback arm unreachable.
            #[allow(unreachable_patterns)]
            fn variant(&self) -> ::core::option::Option<&$ty> {
                match self {
                    $(Self::$variant(inner))|+ => ::core::option::Option::Some(inner),
                    _ => ::core::option::Option::None,
                }
            }
        })+

        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                match self {
                    $($(Self::$variant(inner))|+ => {
                        $crate::__private::serde::Serialize::serialize(inner, serializer)
                    })+
                }
            }
        }
    };
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

        $(impl $crate::ItemVariant<$variant> for $name {
            // A one-variant item type leaves the fallback arm unreachable.
            #[allow(unreachable_patterns)]
            fn variant(&self) -> ::core::option::Option<&$variant> {
                match self {
                    Self::$variant(inner) => ::core::option::Option::Some(inner),
                    _ => ::core::option::Option::None,
                }
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
