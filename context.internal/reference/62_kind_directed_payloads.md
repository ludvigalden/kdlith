# Kind-directed payloads

A dependent value binding under `codec=argument-tail` or
`codec=fixture-value` checks its payload against the declared type of
the referenced declaration's member. Under `codec=from-kind` the
member instead holds a kind, and the value it holds selects the
payload's shape, so one contribution node can carry a flag for one
setting and a number for another.

```kdl
types {
    object Setting {
        field name String
        field kind "optional<String>"
        field choices "optional<list<String>>"
    }
}
node put role=contribution {
    field target "ref<Setting>" arg=0
    value codec=from-kind {
        against target=target member=kind
    }
}
```

The shape is only known once the reference resolves, so the binder
always defers a `from-kind` payload to a pending typed-binding item,
even when the member's declared type is known. Under complete scope,
linking reads the member's value from the resolved declaration and
maps it:

| Kind value          | The payload must be                                                     |
| ------------------- | ----------------------------------------------------------------------- |
| `flag`              | one `Bool`                                                              |
| `number`            | one `U64`                                                               |
| `text`              | one `String`                                                            |
| `choice`            | one scalar equal to one value of the same declaration's bound `choices` |
| no value or `#null` | nothing fits: `KDLITH-REF005`, naming the member                        |
| any other value     | nothing fits: `KDLITH-REF005`, naming the kind the codec does not bind  |

A scalar arm takes exactly one value and never promotes it to a list.
Choice equality compares spellings, as case dispatch does; a `choice`
declaration with no bound `choices` admits nothing. A payload outside
the selected shape is `KDLITH-REF005`, its expected value the
selected type or, for a choice, the rendered choices. The map is
exhaustive, so a kind it does not name fails closed rather than
admitting the payload unchecked. An unresolved reference still
reports its link failure alone and leaves the payload unvalidated.

The binding's shape is checked when the definition compiles, for
every codec: an against target whose field type is not `ref<T>`, and
a member the referenced object type does not declare, are each
`KDLITH-DLC005`.
