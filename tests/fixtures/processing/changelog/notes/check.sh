#!/bin/sh
# Judge `kdlith changelog` on the build path the run declares:
# KDLITH_FX_CHANGELOG=absent for a binary built without relmap,
# anything else for one built with it.
"$KDLITH_FX_KDLITH" changelog > out 2> err
status=$?
if [ "${KDLITH_FX_CHANGELOG:-embedded}" = absent ]; then
    test "$status" -eq 1 || { echo "exit $status, expected 1"; exit 1; }
    test ! -s out || { echo "stdout is not empty"; exit 1; }
    printf 'error[KDLITH-CHG001]: changelog data is unavailable: this kdlith was built without relmap\n  help: Rebuild kdlith with relmap on PATH, or with RELMAP_BIN naming it\n' > want
    cmp -s want err || { echo "stderr differs:"; cat err; exit 1; }
    echo "changelog unavailable as declared"
else
    test "$status" -eq 0 || { echo "exit $status, expected 0:"; cat err; exit 1; }
    test ! -s err || { echo "stderr is not empty:"; cat err; exit 1; }
    printf '## ' > want
    head -c 3 out | cmp -s want - || { echo "notes do not open with a release heading"; exit 1; }
    printf '## 0.1.0 (2026-11-07)\n\nBaseline of the first kdlith development line\n\nkdlith provides typed, streaming KDL processing in Rust: dialect and schema definitions describe families of KDL documents, which kdlith processes directly or turns into generated Rust processors. This pre-1.0 baseline is documented as unstable.\n\n### Added\n' > want
    sed -n '/^## 0\.1\.0 (2026-11-07)$/,/^### Added$/p' out | cmp -s want - || { echo "the 0.1.0 notes do not open with its summary and paragraph"; exit 1; }
    echo "changelog embedded as declared"
fi
