# Changing the dialect

Change the canonical files first, then the record. A mechanism moves
between files, or between defined and deferred, only as a deliberate
change to both: the declarations move, fixture coverage follows, and
the mechanism tables of this tree move with them. Retiring the
`deferred` marker followed the same rule in reverse — the marker's
removal from `documents.kdl` and the deferral disposition landed
together, and a future deferral re-introduces the marker the same
way.

A change to the canonical files changes the dialect's canonical
digest, so every digest-pinned fixture (the entrypoint equivalence,
the oracle byte-comparison, the exercise digests) moves with it. The
fixture coverage rule is part of the contract: at least one positive
bind scenario per mechanism and one negative control per new code,
landing with the canonical declarations that define them, recorded in
the committed census `tests/fixtures/COVERAGE.md`.
