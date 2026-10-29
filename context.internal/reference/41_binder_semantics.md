# Binder semantics

The engine's binder (`pkg/rs/engine/src/bind/`) is the runtime
processor that turns a data document into processing items. It is
built from a compiled definition once and never reads the
definition's KDL.

## Dispatch and fields

Each node is matched by its parent's compiled scope, the active
profile, and its name. Arguments, properties, and children land in
the fields their bindings name and are decoded against the field's
type as they arrive. An entries envelope consumes its key argument
once; a repeated key under `duplicate-keys reject` and a second
source under `collision reject` are rejected. A union value is kept
under the name of the node that spelled its variant; a value whose
type is a codec decodes through it as it arrives.

An entries envelope's key decodes against its map's key type: a
string-keyed entry keys by the decoded string, a reference-typed key
binds the entry with its reference beside its written form, and a key
decoding to anything else is rejected (`KDLITH-BND010`, the node not
emitted, no key dropped). A value whose type names a declared
reference type binds a structured reference: the reference codec's
captures become qualifiers and name segments per the catalog row, the
`namespace` slot in effect at the bind position is captured only when
the policy admits context, and the domain is the type's `lookup`
(else its `target`).

A node with a `cases` block binds under the type its first matching
case selects; that type's required fields are checked like the node's
own (`KDLITH-BND004`, named `Type.field`), and the selected object
type is closed (`KDLITH-BND014`, whether the value was written,
filled by context or fallback, or spliced by an `include`). The case
selector is dispatch input and is always admitted; a part's payload
carries it only when the case-selected type declares a field of that
name. An `identity` block
pins its declaration's domain regardless of the selected type: the
composed identity registers under the block's domain, and only a
declaration without one falls back to the selected type's name (its
own when no case fired).

## Constraints at scope completion

After case selection, the binder enforces the node's declared type's
constraints and the case-selected type's (`KDLITH-BND011`): `requires
F G` fails when F holds a value and G none; `all-or-none` fails when
some but not all hold one, once per absent field. A field holds a
value once anything binds it — written, from context, a literal
fallback, the value resolver, or a collection default — so `#null`
and an empty list are supplied. Numeric `bound`s compare the number's
exact value, never a float (`0x1388`, `5000`, and `5e3` all equal
5000), each offender `KDLITH-BND012`, with absent or `#null` fields
unchecked and ends unsigned at this version. Exclusivity lists
(`at-most-one`, `exactly-one-field`) fail per broken list
(`KDLITH-BND013`), fields named never valued; a literal fallback or a
resolved computed default counts as present, an `include`-spliced
field counts like a declared one, and layered contributions apply
after binding and are outside the lists' reach. A node that breaks a
constraint, misses a selected type's required field, or holds a field
the selected type does not declare is not emitted.

## Context, items, stream order

A context-bound field takes the slot's value only under `fallback`;
under `fixed` an explicit value must agree. Effects follow the
declared timing, and a node that fails applies no following effects.

A bound node yields the item its role names; every diagnostic is an
item in the same stream; fields holding references stay provisional
until linking. A node's `NodeId.index` numbers it within its source
in document order as nodes open (pre-order); a node that does not
bind is numbered, but nodes written inside it are not. A node's item
is emitted when the node completes (post-order). Each record and
declaration carries, as `parent`, the `NodeId` of the nearest
enclosing node whose role yields an item — a context control is
passed over — and `None` at a document's root level, including the
root of an imported document. The parent is always in the same
source, has a lower index, and is emitted later in the stream;
readers may rely on this order and linkage. `parent` is not part of
an item's serialized form; the typed reader carries it unchanged.

Literals a definition writes (fallbacks, documented defaults, and
`set` values) are decoded against their types when the definition
compiles, through the same decoder.
