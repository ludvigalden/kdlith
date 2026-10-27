# Code areas

Diagnostic codes carry a four-letter area. The registry holds the
full census; these are the areas a consumer most often meets.

## BND — binding

`KDLITH-BND001` an unbound node in its position; `BND002` an
occurrence count outside its rule; `BND003` a value that does not
decode as its type; `BND004` a missing required field or argument;
`BND005` an argument, property, or annotation no binding consumes;
`BND006` a repeated key; `BND008` an unknown enum value; `BND009` a
declined value resolution; `BND010` a map key of unusable type;
`BND011`–`BND014` broken object constraints, numeric bounds, field
exclusivity, and case-selected shapes.

## SCH and DLC — schema and dialect compile

`SCH001` an unknown type; `SCH003` overlapping bindings; `SCH004` a
duplicate root declaration; `SCH006` an undeclared reference;
`SCH009` an expansion or generic cycle; `SCH010` conflicting union
prefixes; `SCH011` profile selection failure; `SCH012`–`SCH013`
profile inheritance faults; `SCH015`–`SCH019` group and `when`-branch
faults; `SCH020` an invalid field child binding; `SCH021` an invalid
open-world arm; `SCH022` a `qualify-from` naming no declared string
slot; `SCH023` a case-level binding naming what its node binds in
common. `DLC005` names a malformed construct, `DLC006` an uninstalled
extension, `DLC007` a broken type expression, `DLC008` a dialect
linking fault, `DLC010` a resolver refusal.

## LOC and IMP — locations and imports

`LOC001` a location naming no document; `LOC002` a location leaving
its source; `LOC003` an unregistered `$name`; `LOC004` inadmissible
location syntax; `LOC005` an invalid root, source, manifest, or
override; `LOC006` an origin the store does not hold; `LOC007` a tree
that does not match its pin; `LOC008` an asset of the wrong kind or
extension; `LOC009` an overridden source (warning); `LOC010` an
unpinned source (note). `IMP002` an
import cycle; `IMP003` a repeat; `IMP006`–`IMP008` data-import
confinement and alias faults.

## REF — linking

`REF001` unresolved (naming the alias namespace when gating hid the
match); `REF002` duplicate identity; `REF003` ambiguous; `REF004`
unmet required qualifier; `REF005` a pending payload that does not
fit its declaration.

## CTX, SRC, CAC, GEN, CLI, SYN, EXP — the rest

Context faults (`CTX004` snapshot mismatch, `CTX005` an effect on an
undeclared slot or namespace, `CTX006` a slot duplicate under
`reject`); source faults (`SRC001`–`SRC003`, including the
invocation bound); cache faults (`CAC002` an unusable owned entry);
generation faults (`GEN001` unowned overwrite, `GEN002` drift,
`GEN006` a workspace dependency missing, different, or local, `GEN007`
a source that cannot pin the kdlith crates);
argument faults (`CLI001`–`CLI004`, exit 2, before any document is
read); syntax faults (`SYN…` from the KDL parser); export faults
(`EXP002` provisional delivery).
