# Value resolution

Computed defaults and secret providers are descriptors the processor
never computes or executes. Their values reach a bound record
through one hook the consuming program implements.

## The trait

The engine's `ValueResolver` trait serves secrets and non-secret
context variables alike; they differ only in how the result is
shown, which is the output policy's concern. A program registers its
implementation with `with_resolver` (or a generated crate's
`reader_with_resolver`). A reader without one keeps descriptor-only
behavior: an optional field stays absent and a required one is
missing (`KDLITH-BND004`).

## When it runs

When a bound node completes, each field nothing supplied — no
explicit value, no context value, no literal fallback — and whose
declared fallback is computed is put to the resolver once. The call
(`ResolutionCall`) carries the source document, the node's site, the
declared node name, the field with its type and sensitivity, the
declared descriptors, and the node's incoming context slots. Nothing
in the engine ever executes a provider command or computes a
dependency.

## Failure and caching

A resolver that declines returns a reason and the node fails with
`KDLITH-BND009` naming the field, the node, and that reason. A
resolved value that fails the field's type is a decode failure whose
message never names a concealed value. A resolved value records the
origin kind `resolved`.

Resolved values join the bound result's invocation identity, so a
changed value misses the cache and re-binds. A result for which any
`secret` field was resolved is never cached, in the session or on
disk: no key, fingerprint, or clear text of a resolved secret is
ever written.

## --resolve-from

The CLI is itself a consuming program of the hook: every data verb
takes `--resolve-from FILE`, a KDL document of `value KEY VALUE`
nodes. A computed default names exactly one dependency and that
dependency is the key. Without the option no resolver is registered;
nothing else — environment variables, dotfiles, defaults — is ever
read.
