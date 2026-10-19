//! Typed processing: the declarations a typed processor makes about the
//! definition it reads, their conformance to the compiled definition,
//! decoding bound values into its types, and the reader that drives it.

mod conform;
mod links;
mod macros;
mod origin;
mod read;
mod shape;
mod value;

#[allow(deprecated)]
pub use links::Links;
pub use links::{ItemVariant, Ref, Unresolved, reference_value};
pub use origin::Origin;
pub use read::{ReadError, Reading, ResolveError, TypedItem, TypedReader};
pub use shape::{
    Declarations, Expression, FieldShape, Handler, NodeShape, ReadFn, Shaped, SlotShape, TypeShape,
    VariantShape,
};
pub use value::{
    Decimal, Fields, FromFields, FromValue, LogicalType, Mismatch, Nullable, OrderedMap, ValueExpr,
    member_value, object_value, read_fields, text_value, variant_value,
};
